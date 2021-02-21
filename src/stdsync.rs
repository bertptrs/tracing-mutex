//! Tracing Mutex implementations for `std::sync`.
use std::fmt;
use std::ops::Deref;
use std::ops::DerefMut;
use std::sync::LockResult;
use std::sync::Mutex;
use std::sync::MutexGuard;
use std::sync::PoisonError;
use std::sync::TryLockError;
use std::sync::TryLockResult;

use crate::drop_lock;
use crate::get_depedency_graph;
use crate::next_mutex_id;
use crate::register_dependency;
use crate::register_lock;

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

impl<'a, T: fmt::Display> fmt::Display for TracingMutexGuard<'a, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.inner.fmt(f)
    }
}

impl<'a, T> Drop for TracingMutexGuard<'a, T> {
    fn drop(&mut self) {
        drop_lock(self.mutex.id);
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::thread;

    use super::*;
    use crate::tests::GRAPH_MUTEX;

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
