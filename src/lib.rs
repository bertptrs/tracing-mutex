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
static ID_SEQUENCE: AtomicUsize = AtomicUsize::new(0);

thread_local! {
    /// Stack to track which locks are held
    ///
    /// Assuming that locks are roughly released in the reverse order in which they were acquired,
    /// a stack should be more efficient to keep track of the current state than a set would be.
    static HELD_LOCKS: RefCell<Vec<usize>> = RefCell::new(Vec::new());
}

lazy_static! {
    static ref DEPENDENCY_GRAPH: Mutex<DiGraph> = Default::default();
}

fn next_mutex_id() -> usize {
    ID_SEQUENCE
        .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |id| id.checked_add(1))
        .expect("Mutex ID wraparound happened, results unreliable")
}

/// Get a reference to the current dependency graph
fn get_depedency_graph() -> impl DerefMut<Target = DiGraph> {
    DEPENDENCY_GRAPH
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
}

/// Register that a lock is currently held
fn register_lock(lock: usize) {
    HELD_LOCKS.with(|locks| locks.borrow_mut().push(lock))
}

fn drop_lock(id: usize) {
    HELD_LOCKS.with(|locks| {
        let mut locks = locks.borrow_mut();

        for (i, &lock) in locks.iter().enumerate().rev() {
            if lock == id {
                locks.remove(i);
                return;
            }
        }

        panic!("Tried to drop lock for mutex {} but it wasn't held", id)
    });
}

/// Register a dependency in the dependency graph
///
/// If the dependency is new, check for cycles in the dependency graph. If not, there shouldn't be
/// any cycles so we don't need to check.
fn register_dependency(lock: usize) {
    HELD_LOCKS.with(|locks| {
        if let Some(&previous) = locks.borrow().last() {
            let mut graph = get_depedency_graph();

            if graph.add_edge(previous, lock) && graph.has_cycles() {
                panic!("Mutex order graph should not have cycles");
            }
        }
    })
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
        let initial = next_mutex_id();
        let next = next_mutex_id();

        // Can't assert N + 1 because multiple threads running tests
        assert!(initial < next);
    }
}
