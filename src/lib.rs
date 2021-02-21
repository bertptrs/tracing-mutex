use std::cell::RefCell;
use std::fmt;
use std::fmt::Display;
use std::ops::Deref;
use std::ops::DerefMut;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;
use std::sync::LockResult;
use std::sync::Mutex;
use std::sync::MutexGuard;
use std::sync::PoisonError;
use std::sync::TryLockError;
use std::sync::TryLockResult;

use lazy_static::lazy_static;

use crate::graph::DiGraph;

mod graph;

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

/// Wrapper for std::sync::Mutex
#[derive(Debug)]
pub struct TracingMutex<T> {
    inner: Mutex<T>,
    id: usize,
}

/// Wrapper for std::sync::MutexGuard
#[derive(Debug)]
pub struct TracingMutexGuard<'a, T> {
    inner: MutexGuard<'a, T>,
    mutex: &'a TracingMutex<T>,
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

fn map_lockresult<T, I, F>(result: LockResult<I>, mapper: F) -> LockResult<T>
where
    F: FnOnce(I) -> T,
{
    match result {
        Ok(inner) => Ok(mapper(inner)),
        Err(poisoned) => Err(PoisonError::new(mapper(poisoned.into_inner()))),
    }
}

fn map_trylockresult<T, I, F>(result: TryLockResult<I>, mapper: F) -> TryLockResult<T>
where
    F: FnOnce(I) -> T,
{
    match result {
        Ok(inner) => Ok(mapper(inner)),
        Err(TryLockError::WouldBlock) => Err(TryLockError::WouldBlock),
        Err(TryLockError::Poisoned(poisoned)) => {
            Err(PoisonError::new(mapper(poisoned.into_inner())).into())
        }
    }
}

impl<T> TracingMutex<T> {
    pub fn new(t: T) -> Self {
        Self {
            inner: Mutex::new(t),
            id: next_mutex_id(),
        }
    }

    #[track_caller]
    pub fn lock(&self) -> LockResult<TracingMutexGuard<T>> {
        register_dependency(self.id);
        let result = self.inner.lock();
        register_lock(self.id);

        let mapper = |guard| TracingMutexGuard {
            mutex: self,
            inner: guard,
        };

        map_lockresult(result, mapper)
    }

    #[track_caller]
    pub fn try_lock(&self) -> TryLockResult<TracingMutexGuard<T>> {
        register_dependency(self.id);
        let result = self.inner.try_lock();
        register_lock(self.id);

        let mapper = |guard| TracingMutexGuard {
            mutex: self,
            inner: guard,
        };

        map_trylockresult(result, mapper)
    }

    pub fn get_id(&self) -> usize {
        self.id
    }

    pub fn is_poisoned(&self) -> bool {
        self.inner.is_poisoned()
    }

    pub fn get_mut(&mut self) -> LockResult<&mut T> {
        self.inner.get_mut()
    }
}

impl<T: ?Sized + Default> Default for TracingMutex<T> {
    fn default() -> Self {
        Self::new(T::default())
    }
}

impl<T> From<T> for TracingMutex<T> {
    fn from(t: T) -> Self {
        Self::new(t)
    }
}

impl<T> Drop for TracingMutex<T> {
    fn drop(&mut self) {
        let id = self.id;

        get_depedency_graph().remove_node(id);
    }
}

impl<'a, T> Deref for TracingMutexGuard<'a, T> {
    type Target = MutexGuard<'a, T>;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<'a, T> DerefMut for TracingMutexGuard<'a, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

impl<'a, T: Display> fmt::Display for TracingMutexGuard<'a, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.inner.fmt(f)
    }
}

impl<'a, T> Drop for TracingMutexGuard<'a, T> {
    fn drop(&mut self) {
        HELD_LOCKS.with(|locks| {
            let id = self.mutex.id;
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
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::thread;

    use super::*;

    lazy_static! {
        /// Mutex to isolate tests manipulating the global mutex graph
        static ref GRAPH_MUTEX: Mutex<()> = Mutex::new(());
    }

    #[test]
    fn test_next_mutex_id() {
        let initial = next_mutex_id();
        let next = next_mutex_id();

        // Can't assert N + 1 because multiple threads running tests
        assert!(initial < next);
    }

    #[test]
    fn test_mutex_usage() {
        let _graph_lock = GRAPH_MUTEX.lock();

        let mutex = Arc::new(TracingMutex::new(()));
        let mutex_clone = mutex.clone();

        let _guard = mutex.lock().unwrap();

        // Now try to cause a blocking exception in another thread
        let handle = thread::spawn(move || {
            let result = mutex_clone.try_lock().unwrap_err();

            assert!(matches!(result, TryLockError::WouldBlock));
        });

        handle.join().unwrap();
    }

    #[test]
    #[should_panic(expected = "Mutex order graph should not have cycles")]
    fn test_detect_cycle() {
        let _graph_lock = GRAPH_MUTEX.lock();

        let a = TracingMutex::new(());
        let b = TracingMutex::new(());

        let hold_a = a.lock().unwrap();
        let _ = b.lock();

        drop(hold_a);

        let _hold_b = b.lock().unwrap();
        let _ = a.lock();
    }
}
