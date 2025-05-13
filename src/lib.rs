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
//! will instead panic. This panic will not poison the underlying mutex.
//!
//! This conflicting dependency is not added to the graph, so future attempts at locking should
//! succeed as normal.
//!
//! # Structure
//!
//! Each module in this crate exposes wrappers for a specific base-mutex with dependency trakcing
//! added. This includes [`stdsync`] which provides wrappers for the base locks in the standard
//! library, and more depending on enabled compile-time features. More back-ends may be added as
//! features in the future.
//!
//! # Feature flags
//!
//! `tracing-mutex` uses feature flags to reduce the impact of this crate on both your compile time
//! and runtime overhead. Below are the available flags. Modules are annotated with the features
//! they require.
//!
//! - `backtraces`: Enables capturing backtraces of mutex dependencies, to make it easier to
//!   determine what sequence of events would trigger a deadlock. This is enabled by default, but if
//!   the performance overhead is unacceptable, it can be disabled by disabling default features.
//!
//! - `lockapi`: Enables the wrapper lock for [`lock_api`][lock_api] locks
//!
//! - `parkinglot`: Enables wrapper types for [`parking_lot`][parking_lot] mutexes
//!
//! - `experimental`: Enables experimental features. Experimental features are intended to test new
//!   APIs and play with new APIs before committing to them. As such, breaking changes may be
//!   introduced in it between otherwise semver-compatible versions, and the MSRV does not apply to
//!   experimental features.
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
//! - **Deallocating a mutex** temporarily locks the global dependency graph to remove the lock
//!   entry in the dependency graph.
//!
//! These operations have been reasonably optimized, but the performance penalty may yet be too much
//! for production use. In those cases, it may be beneficial to instead use debug-only versions
//! (such as [`stdsync::Mutex`]) which evaluate to a tracing mutex when debug assertions are
//! enabled, and to the underlying mutex when they're not.
//!
//! For ease of debugging, this crate will, by default, capture a backtrace when establishing a new
//! dependency between two mutexes. This has an additional overhead of over 60%. If this additional
//! debugging aid is not required, it can be disabled by disabling default features.
//!
//! [paper]: https://whileydave.com/publications/pk07_jea/
//! [lock_api]: https://docs.rs/lock_api/0.4/lock_api/index.html
//! [parking_lot]: https://docs.rs/parking_lot/0.12.1/parking_lot/
#![cfg_attr(docsrs, feature(doc_cfg))]
use std::cell::RefCell;
use std::fmt;
use std::marker::PhantomData;
use std::ops::Deref;
use std::ops::DerefMut;
use std::sync::Mutex;
use std::sync::MutexGuard;
use std::sync::OnceLock;
use std::sync::PoisonError;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;

#[cfg(feature = "lock_api")]
#[cfg_attr(docsrs, doc(cfg(feature = "lockapi")))]
#[deprecated = "The top-level re-export `lock_api` is deprecated. Use `tracing_mutex::lockapi::raw` instead"]
pub use lock_api;
#[cfg(feature = "parking_lot")]
#[cfg_attr(docsrs, doc(cfg(feature = "parkinglot")))]
#[deprecated = "The top-level re-export `parking_lot` is deprecated. Use `tracing_mutex::parkinglot::raw` instead"]
pub use parking_lot;

use graph::DiGraph;
use reporting::Dep;
use reporting::Reportable;

mod graph;
#[cfg(any(feature = "lock_api", feature = "lockapi"))]
#[cfg_attr(docsrs, doc(cfg(feature = "lock_api")))]
#[cfg_attr(
    all(not(docsrs), feature = "lockapi", not(feature = "lock_api")),
    deprecated = "The `lockapi` feature has been renamed `lock_api`"
)]
pub mod lockapi;
#[cfg(any(feature = "parking_lot", feature = "parkinglot"))]
#[cfg_attr(docsrs, doc(cfg(feature = "parking_lot")))]
#[cfg_attr(
    all(not(docsrs), feature = "parkinglot", not(feature = "parking_lot")),
    deprecated = "The `parkinglot` feature has been renamed `parking_lot`"
)]
pub mod parkinglot;
mod reporting;
pub mod stdsync;
pub mod util;

thread_local! {
    /// Stack to track which locks are held
    ///
    /// Assuming that locks are roughly released in the reverse order in which they were acquired,
    /// a stack should be more efficient to keep track of the current state than a set would be.
    static HELD_LOCKS: RefCell<Vec<usize>> = const { RefCell::new(Vec::new()) };
}

/// Dedicated ID type for Mutexes
///
/// # Unstable
///
/// This type is currently private to prevent usage while the exact implementation is figured out,
/// but it will likely be public in the future.
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
        // Counter for Mutex IDs. Atomic avoids the need for locking.
        static ID_SEQUENCE: AtomicUsize = AtomicUsize::new(0);

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
        self.mark_held();
        BorrowedMutex {
            id: self,
            _not_send: PhantomData,
        }
    }

    /// Mark this lock as held for the purposes of dependency tracking.
    ///
    /// # Panics
    ///
    /// This method panics if the new dependency would introduce a cycle.
    pub fn mark_held(&self) {
        let opt_cycle = HELD_LOCKS.with(|locks| {
            if let Some(&previous) = locks.borrow().last() {
                let mut graph = get_dependency_graph();

                graph.add_edge(previous, self.value(), Dep::capture).err()
            } else {
                None
            }
        });

        if let Some(cycle) = opt_cycle {
            panic!("{}", Dep::panic_message(&cycle))
        }

        HELD_LOCKS.with(|locks| locks.borrow_mut().push(self.value()));
    }

    pub unsafe fn mark_released(&self) {
        HELD_LOCKS.with(|locks| {
            let mut locks = locks.borrow_mut();

            for (i, &lock) in locks.iter().enumerate().rev() {
                if lock == self.value() {
                    locks.remove(i);
                    return;
                }
            }

            // Drop impls shouldn't panic but if this happens something is seriously broken.
            unreachable!("Tried to drop lock for mutex {:?} but it wasn't held", self)
        });
    }

    /// Execute the given closure while the guard is held.
    pub fn with_held<T>(&self, f: impl FnOnce() -> T) -> T {
        // Note: we MUST construct the RAII guard, we cannot simply mark held + mark released, as
        // f() may panic and corrupt our state.
        let _guard = self.get_borrowed();
        f()
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
        get_dependency_graph().remove_node(self.value());
    }
}

/// `const`-compatible version of [`crate::MutexId`].
///
/// This struct can be used similarly to the normal mutex ID, but to be const-compatible its ID is
/// generated on first use. This allows it to be used as the mutex ID for mutexes with a `const`
/// constructor.
///
/// This type can be largely replaced once std::lazy gets stabilized.
struct LazyMutexId {
    inner: OnceLock<MutexId>,
}

impl LazyMutexId {
    pub const fn new() -> Self {
        Self {
            inner: OnceLock::new(),
        }
    }
}

impl fmt::Debug for LazyMutexId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self.deref())
    }
}

impl Default for LazyMutexId {
    fn default() -> Self {
        Self::new()
    }
}

impl Deref for LazyMutexId {
    type Target = MutexId;

    fn deref(&self) -> &Self::Target {
        self.inner.get_or_init(MutexId::new)
    }
}

/// Borrowed mutex ID
///
/// This type should be used as part of a mutex guard wrapper. It can be acquired through
/// [`MutexId::get_borrowed`] and will automatically mark the mutex as not borrowed when it is
/// dropped.
///
/// This type intentionally is [`!Send`](std::marker::Send) because the ownership tracking is based
/// on a thread-local stack which doesn't work if a guard gets released in a different thread from
/// where they're acquired.
#[derive(Debug)]
struct BorrowedMutex<'a> {
    /// Reference to the mutex we're borrowing from
    id: &'a MutexId,
    /// This value serves no purpose but to make the type [`!Send`](std::marker::Send)
    _not_send: PhantomData<MutexGuard<'static, ()>>,
}

/// Drop a lock held by the current thread.
///
/// # Panics
///
/// This function panics if the lock did not appear to be handled by this thread. If that happens,
/// that is an indication of a serious design flaw in this library.
impl Drop for BorrowedMutex<'_> {
    fn drop(&mut self) {
        // Safety: the only way to get a BorrowedMutex is by locking the mutex.
        unsafe { self.id.mark_released() };
    }
}

/// Get a reference to the current dependency graph
fn get_dependency_graph() -> impl DerefMut<Target = DiGraph<usize, Dep>> {
    static DEPENDENCY_GRAPH: OnceLock<Mutex<DiGraph<usize, Dep>>> = OnceLock::new();

    DEPENDENCY_GRAPH
        .get_or_init(Default::default)
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use rand::seq::SliceRandom;
    use rand::thread_rng;

    use super::*;

    #[test]
    fn test_next_mutex_id() {
        let initial = MutexId::new();
        let next = MutexId::new();

        // Can't assert N + 1 because multiple threads running tests
        assert!(initial.0 < next.0);
    }

    #[test]
    fn test_lazy_mutex_id() {
        let a = LazyMutexId::new();
        let b = LazyMutexId::new();
        let c = LazyMutexId::new();

        let mut graph = get_dependency_graph();
        assert!(graph.add_edge(a.value(), b.value(), Dep::capture).is_ok());
        assert!(graph.add_edge(b.value(), c.value(), Dep::capture).is_ok());

        // Creating an edge c → a should fail as it introduces a cycle.
        assert!(graph.add_edge(c.value(), a.value(), Dep::capture).is_err());

        // Drop graph handle so we can drop vertices without deadlocking
        drop(graph);

        drop(b);

        // If b's destructor correctly ran correctly we can now add an edge from c to a.
        assert!(
            get_dependency_graph()
                .add_edge(c.value(), a.value(), Dep::capture)
                .is_ok()
        );
    }

    /// Test creating a cycle, then panicking.
    #[test]
    #[should_panic]
    fn test_mutex_id_conflict() {
        let ids = [MutexId::new(), MutexId::new(), MutexId::new()];

        for i in 0..3 {
            let _first_lock = ids[i].get_borrowed();
            let _second_lock = ids[(i + 1) % 3].get_borrowed();
        }
    }

    /// Fuzz the global dependency graph by fake-acquiring lots of mutexes in a valid order.
    ///
    /// This test generates all possible forward edges in a 100-node graph consisting of natural
    /// numbers, shuffles them, then adds them to the graph. This will always be a valid directed,
    /// acyclic graph because there is a trivial order (the natural numbers) but because the edges
    /// are added in a random order the DiGraph will still occassionally need to reorder nodes.
    #[test]
    fn fuzz_mutex_id() {
        const NUM_NODES: usize = 100;

        let ids: Vec<MutexId> = (0..NUM_NODES).map(|_| Default::default()).collect();

        let mut edges = Vec::with_capacity(NUM_NODES * NUM_NODES);
        for i in 0..NUM_NODES {
            for j in i..NUM_NODES {
                if i != j {
                    edges.push((i, j));
                }
            }
        }

        edges.shuffle(&mut thread_rng());

        for (x, y) in edges {
            // Acquire the mutexes, smallest first to ensure a cycle-free graph
            let _ignored = ids[x].get_borrowed();
            let _ = ids[y].get_borrowed();
        }
    }
}
