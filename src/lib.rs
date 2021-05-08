//! Mutexes can deadlock each other, but you can avoid this by always acquiring your locks in a
//! consistent order. This crate provides tracing to ensure that you do.
//!
//! This crate tracks a virtual "stack" of locks that the current thread holds, and whenever a new
//! lock is acquired, a dependency is created from the last lock to the new one. These dependencies
//! together form a graph. As long as that graph does not contain any cycles, your program is
//! guaranteed to never deadlock.
//!
//! # Panics
//!
//! The primary method by which this crate signals an invalid lock acquisition order is by
//! panicking. When a cycle is created in the dependency graph when acquiring a lock, the thread
//! will instead panic. This panic will not poison the underlying mutex. Each following acquired
//! that introduces a **new** dependency will also panic, until enough mutexes are deallocated to
//! break the cycle in the graph.
//!
//! # Structure
//!
//! Each module in this crate exposes wrappers for a specific base-mutex with dependency trakcing
//! added. For now, that is limited to [`stdsync`] which provides wrappers for the base locks in the
//! standard library. More back-ends may be added as features in the future.
//!
//! # Performance considerations
//!
//! Tracing a mutex adds overhead to certain mutex operations in order to do the required
//! bookkeeping. The following actions have the following overhead.
//!
//! - **Acquiring a lock** locks the global dependency graph temporarily to check if the new lock
//!   would introduce a cyclic dependency. This crate uses the algorithm proposed in ["A Dynamic
//!   Topological Sort Algorithm for Directed Acyclic Graphs" by David J. Pearce and Paul H.J.
//!   Kelly][paper] to detect cycles as efficently as possible. In addition, a thread local lock set
//!   is updated with the new lock.
//!
//! - **Releasing a lock** updates a thread local lock set to remove the released lock.
//!
//! - **Allocating a lock** performs an atomic update to a shared counter.
//!
//! - **Deallocating a mutex** temporarily locks the global dependency graph to remove the lock from
//!   it. If the graph contained a cycle, a complete scan of the (now pruned) graph is done to
//!   determine if this is still the case.
//!
//! These operations have been reasonably optimized, but the performance penalty may yet be too much
//! for production use. In those cases, it may be beneficial to instead use debug-only versions
//! (such as [`stdsync::DebugMutex`]) which evaluate to a tracing mutex when debug assertions are
//! enabled, and to the underlying mutex when they're not.
//!
//! [paper]: https://whileydave.com/publications/pk07_jea/
use std::cell::RefCell;
use std::fmt;
use std::ops::DerefMut;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;
use std::sync::Mutex;
use std::sync::PoisonError;

use lazy_static::lazy_static;

use crate::graph::DiGraph;

mod graph;
pub mod stdsync;

/// Counter for Mutex IDs. Atomic avoids the need for locking.
///
/// Should be part of the `MutexID` impl but static items are not yet a thing.
static ID_SEQUENCE: AtomicUsize = AtomicUsize::new(0);

thread_local! {
    /// Stack to track which locks are held
    ///
    /// Assuming that locks are roughly released in the reverse order in which they were acquired,
    /// a stack should be more efficient to keep track of the current state than a set would be.
    static HELD_LOCKS: RefCell<Vec<usize>> = RefCell::new(Vec::new());
}

lazy_static! {
    static ref DEPENDENCY_GRAPH: Mutex<DiGraph<usize>> = Default::default();
}

/// Dedicated ID type for Mutexes
///
/// # Unstable
///
/// This type is currently private to prevent usage while the exact implementation is figured out,
/// but it will likely be public in the future.
///
/// One possible alteration is to make this type not `Copy` but `Drop`, and handle deregistering
/// the lock from there.
struct MutexId(usize);

impl MutexId {
    /// Get a new, unique, mutex ID.
    ///
    /// This ID is guaranteed to be unique within the runtime of the program.
    ///
    /// # Panics
    ///
    /// This function may panic when there are no more mutex IDs available. The number of mutex ids
    /// is `usize::MAX - 1` which should be plenty for most practical applications.
    pub fn new() -> Self {
        ID_SEQUENCE
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |id| id.checked_add(1))
            .map(Self)
            .expect("Mutex ID wraparound happened, results unreliable")
    }

    pub fn value(&self) -> usize {
        self.0
    }

    /// Get a borrowed guard for this lock.
    ///
    /// This method adds checks adds this Mutex ID to the dependency graph as needed, and adds the
    /// lock to the list of
    ///
    /// # Panics
    ///
    /// This method panics if the new dependency would introduce a cycle.
    pub fn get_borrowed(&self) -> BorrowedMutex {
        let creates_cycle = HELD_LOCKS.with(|locks| {
            if let Some(&previous) = locks.borrow().last() {
                let mut graph = get_depedency_graph();

                graph.add_edge(previous, self.value()) && graph.has_cycles()
            } else {
                false
            }
        });

        if creates_cycle {
            // Panic without holding the lock to avoid needlessly poisoning it
            panic!("Mutex order graph should not have cycles");
        }

        HELD_LOCKS.with(|locks| locks.borrow_mut().push(self.value()));
        BorrowedMutex(self)
    }
}

impl Default for MutexId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Debug for MutexId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "MutexID({:?})", self.0)
    }
}

impl Drop for MutexId {
    fn drop(&mut self) {
        get_depedency_graph().remove_node(self.value());
    }
}

#[derive(Debug)]
struct BorrowedMutex<'a>(&'a MutexId);

/// Drop a lock held by the current thread.
///
/// # Panics
///
/// This function panics if the lock did not appear to be handled by this thread. If that happens,
/// that is an indication of a serious design flaw in this library.
impl<'a> Drop for BorrowedMutex<'a> {
    fn drop(&mut self) {
        let id = self.0;

        HELD_LOCKS.with(|locks| {
            let mut locks = locks.borrow_mut();

            for (i, &lock) in locks.iter().enumerate().rev() {
                if lock == id.value() {
                    locks.remove(i);
                    return;
                }
            }

            // Drop impls shouldn't panic but if this happens something is seriously broken.
            unreachable!("Tried to drop lock for mutex {:?} but it wasn't held", id)
        });
    }
}

/// Get a reference to the current dependency graph
fn get_depedency_graph() -> impl DerefMut<Target = DiGraph<usize>> {
    DEPENDENCY_GRAPH
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use super::*;

    lazy_static! {
        /// Mutex to isolate tests manipulating the global mutex graph
        pub(crate) static ref GRAPH_MUTEX: Mutex<()> = Mutex::new(());
    }

    #[test]
    fn test_next_mutex_id() {
        let initial = MutexId::new();
        let next = MutexId::new();

        // Can't assert N + 1 because multiple threads running tests
        assert!(initial.0 < next.0);
    }
}
