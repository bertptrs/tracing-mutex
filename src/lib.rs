use std::cell::RefCell;
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
    static HELD_LOCKS: RefCell<Vec<MutexID>> = RefCell::new(Vec::new());
}

lazy_static! {
    static ref DEPENDENCY_GRAPH: Mutex<DiGraph> = Default::default();
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
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
struct MutexID(usize);

impl MutexID {
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
}

/// Get a reference to the current dependency graph
fn get_depedency_graph() -> impl DerefMut<Target = DiGraph> {
    DEPENDENCY_GRAPH
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
}

/// Register that a lock is currently held
fn register_lock(lock: MutexID) {
    HELD_LOCKS.with(|locks| locks.borrow_mut().push(lock))
}

/// Drop a lock held by the current thread.
///
/// # Panics
///
/// This function panics if the lock did not appear to be handled by this thread. If that happens,
/// that is an indication of a serious design flaw in this library.
fn drop_lock(id: MutexID) {
    HELD_LOCKS.with(|locks| {
        let mut locks = locks.borrow_mut();

        for (i, &lock) in locks.iter().enumerate().rev() {
            if lock == id {
                locks.remove(i);
                return;
            }
        }

        panic!("Tried to drop lock for mutex {:?} but it wasn't held", id)
    });
}

/// Register a dependency in the dependency graph
///
/// If the dependency is new, check for cycles in the dependency graph. If not, there shouldn't be
/// any cycles so we don't need to check.
///
/// # Panics
///
/// This function panics if the new dependency would introduce a cycle.
fn register_dependency(lock: MutexID) {
    let creates_cycle = HELD_LOCKS.with(|locks| {
        if let Some(&previous) = locks.borrow().last() {
            let mut graph = get_depedency_graph();

            graph.add_edge(previous, lock) && graph.has_cycles()
        } else {
            false
        }
    });

    if creates_cycle {
        // Panic without holding the lock to avoid needlessly poisoning it
        panic!("Mutex order graph should not have cycles");
    }
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
        let initial = MutexID::new();
        let next = MutexID::new();

        // Can't assert N + 1 because multiple threads running tests
        assert!(initial.0 < next.0);
    }
}
