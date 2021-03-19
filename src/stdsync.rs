//! Tracing mutex wrappers for types found in `std::sync`.
//!
//! This module provides wrappers for `std::sync` primitives with exactly the same API and
//! functionality as their counterparts, with the exception that their acquisition order is
//! tracked.
//!
//! ```rust
//! # use tracing_mutex::stdsync::TracingMutex;
//! # use tracing_mutex::stdsync::TracingRwLock;
//! let mutex = TracingMutex::new(());
//! mutex.lock().unwrap();
//!
//! let rwlock = TracingRwLock::new(());
//! rwlock.read().unwrap();
//! ```
use std::fmt;
use std::mem;
use std::ops::Deref;
use std::ops::DerefMut;
use std::ptr;
use std::sync::LockResult;
use std::sync::Mutex;
use std::sync::MutexGuard;
use std::sync::PoisonError;
use std::sync::RwLock;
use std::sync::RwLockReadGuard;
use std::sync::RwLockWriteGuard;
use std::sync::TryLockError;
use std::sync::TryLockResult;

use crate::drop_lock;
use crate::get_depedency_graph;
use crate::register_dependency;
use crate::register_lock;
use crate::MutexID;

/// Wrapper for `std::sync::Mutex`
#[derive(Debug)]
pub struct TracingMutex<T> {
    inner: Mutex<T>,
    id: MutexID,
}

/// Wrapper for `std::sync::MutexGuard`
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
            id: MutexID::new(),
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

    pub fn is_poisoned(&self) -> bool {
        self.inner.is_poisoned()
    }

    pub fn get_mut(&mut self) -> LockResult<&mut T> {
        self.inner.get_mut()
    }

    pub fn into_inner(self) -> LockResult<T> {
        self.deregister();

        // Safety: we forget the original immediately after
        let inner = unsafe { ptr::read(&self.inner) };
        mem::forget(self);

        inner.into_inner()
    }

    fn deregister(&self) {
        get_depedency_graph().remove_node(self.id);
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
        self.deregister();
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

/// Wrapper for `std::sync::RwLock`
#[derive(Debug)]
pub struct TracingRwLock<T> {
    inner: RwLock<T>,
    id: MutexID,
}

/// Hybrid wrapper for both `std::sync::RwLockReadGuard` and `std::sync::RwLockWriteGuard`.
///
/// Please refer to [`TracingReadGuard`] and [`TracingWriteGuard`] for usable types.
#[derive(Debug)]
pub struct TracingRwLockGuard<'a, T, L> {
    inner: L,
    mutex: &'a TracingRwLock<T>,
}

/// Wrapper around `std::sync::RwLockReadGuard`.
pub type TracingReadGuard<'a, T> = TracingRwLockGuard<'a, T, RwLockReadGuard<'a, T>>;
/// Wrapper around `std::sync::RwLockWriteGuard`.
pub type TracingWriteGuard<'a, T> = TracingRwLockGuard<'a, T, RwLockWriteGuard<'a, T>>;

impl<T> TracingRwLock<T> {
    pub fn new(t: T) -> Self {
        Self {
            inner: RwLock::new(t),
            id: MutexID::new(),
        }
    }

    #[track_caller]
    pub fn read(&self) -> LockResult<TracingReadGuard<T>> {
        register_dependency(self.id);
        let result = self.inner.read();
        register_lock(self.id);

        map_lockresult(result, |lock| TracingRwLockGuard {
            inner: lock,
            mutex: self,
        })
    }

    #[track_caller]
    pub fn write(&self) -> LockResult<TracingWriteGuard<T>> {
        register_dependency(self.id);
        let result = self.inner.write();
        register_lock(self.id);
        let id = self.id;

        get_depedency_graph().remove_node(id);
        map_lockresult(result, |lock| TracingRwLockGuard {
            inner: lock,
            mutex: self,
        })
    }

    #[track_caller]
    pub fn try_read(&self) -> TryLockResult<TracingReadGuard<T>> {
        register_dependency(self.id);
        let result = self.inner.try_read();
        register_lock(self.id);

        map_trylockresult(result, |lock| TracingRwLockGuard {
            inner: lock,
            mutex: self,
        })
    }

    #[track_caller]
    pub fn try_write(&self) -> TryLockResult<TracingWriteGuard<T>> {
        register_dependency(self.id);
        let result = self.inner.try_write();
        register_lock(self.id);

        map_trylockresult(result, |lock| TracingRwLockGuard {
            inner: lock,
            mutex: self,
        })
    }

    pub fn get_mut(&mut self) -> LockResult<&mut T> {
        self.inner.get_mut()
    }

    pub fn into_inner(self) -> LockResult<T> {
        self.deregister();

        // Grab our contents and then forget ourselves
        // Safety: we immediately forget the mutex after copying
        let inner = unsafe { ptr::read(&self.inner) };
        mem::forget(self);

        inner.into_inner()
    }

    fn deregister(&self) {
        get_depedency_graph().remove_node(self.id);
    }
}

impl<T> Drop for TracingRwLock<T> {
    fn drop(&mut self) {
        self.deregister();
    }
}

impl<T> Default for TracingRwLock<T>
where
    T: Default,
{
    fn default() -> Self {
        Self::new(T::default())
    }
}

impl<'a, T, L> Drop for TracingRwLockGuard<'a, T, L> {
    fn drop(&mut self) {
        drop_lock(self.mutex.id)
    }
}

impl<'a, T, L> Deref for TracingRwLockGuard<'a, T, L>
where
    L: Deref<Target = T>,
{
    type Target = T;

    fn deref(&self) -> &Self::Target {
        self.inner.deref()
    }
}

impl<'a, T, L> DerefMut for TracingRwLockGuard<'a, T, L>
where
    L: Deref<Target = T> + DerefMut,
{
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.inner.deref_mut()
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
    fn test_rwlock_usage() {
        let _graph_lock = GRAPH_MUTEX.lock();

        let rwlock = Arc::new(TracingRwLock::new(()));
        let rwlock_clone = rwlock.clone();

        let _read_lock = rwlock.read().unwrap();

        // Now try to cause a blocking exception in another thread
        let handle = thread::spawn(move || {
            let write_result = rwlock_clone.try_write().unwrap_err();

            assert!(matches!(write_result, TryLockError::WouldBlock));

            // Should be able to get a read lock just fine.
            let _ = rwlock_clone.read().unwrap();
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
