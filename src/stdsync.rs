//! Tracing mutex wrappers for locks found in `std::sync`.
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
use std::ops::Deref;
use std::ops::DerefMut;
use std::sync::LockResult;
use std::sync::Mutex;
use std::sync::MutexGuard;
use std::sync::PoisonError;
use std::sync::RwLock;
use std::sync::RwLockReadGuard;
use std::sync::RwLockWriteGuard;
use std::sync::TryLockError;
use std::sync::TryLockResult;

use crate::BorrowedMutex;
use crate::MutexId;

/// Debug-only tracing `Mutex`.
///
/// Type alias that resolves to [`TracingMutex`] when debug assertions are enabled and to
/// [`std::sync::Mutex`] when they're not. Use this if you want to have the benefits of cycle
/// detection in development but do not want to pay the performance penalty in release.
#[cfg(debug_assertions)]
pub type DebugMutex<T> = TracingMutex<T>;
#[cfg(not(debug_assertions))]
pub type DebugMutex<T> = Mutex<T>;

/// Debug-only tracing `RwLock`.
///
/// Type alias that resolves to [`RwLock`] when debug assertions are enabled and to
/// [`std::sync::RwLock`] when they're not. Use this if you want to have the benefits of cycle
/// detection in development but do not want to pay the performance penalty in release.
#[cfg(debug_assertions)]
pub type DebugRwLock<T> = TracingRwLock<T>;
#[cfg(not(debug_assertions))]
pub type DebugRwLock<T> = RwLock<T>;

/// Wrapper for [`std::sync::Mutex`].
///
/// Refer to the [crate-level][`crate`] documentaiton for the differences between this struct and
/// the one it wraps.
#[derive(Debug)]
pub struct TracingMutex<T> {
    inner: Mutex<T>,
    id: MutexId,
}

/// Wrapper for [`std::sync::MutexGuard`].
///
/// Refer to the [crate-level][`crate`] documentaiton for the differences between this struct and
/// the one it wraps.
#[derive(Debug)]
pub struct TracingMutexGuard<'a, T> {
    inner: MutexGuard<'a, T>,
    mutex: BorrowedMutex<'a>,
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
            id: MutexId::new(),
        }
    }

    #[track_caller]
    pub fn lock(&self) -> LockResult<TracingMutexGuard<T>> {
        let mutex = self.id.get_borrowed();
        let result = self.inner.lock();

        let mapper = |guard| TracingMutexGuard {
            mutex,
            inner: guard,
        };

        map_lockresult(result, mapper)
    }

    #[track_caller]
    pub fn try_lock(&self) -> TryLockResult<TracingMutexGuard<T>> {
        let mutex = self.id.get_borrowed();
        let result = self.inner.try_lock();

        let mapper = |guard| TracingMutexGuard {
            mutex,
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
        self.inner.into_inner()
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

/// Wrapper for [`std::sync::RwLock`].
#[derive(Debug)]
pub struct TracingRwLock<T> {
    inner: RwLock<T>,
    id: MutexId,
}

/// Hybrid wrapper for both [`std::sync::RwLockReadGuard`] and [`std::sync::RwLockWriteGuard`].
///
/// Please refer to [`TracingReadGuard`] and [`TracingWriteGuard`] for usable types.
#[derive(Debug)]
pub struct TracingRwLockGuard<'a, L> {
    inner: L,
    mutex: BorrowedMutex<'a>,
}

/// Wrapper around [`std::sync::RwLockReadGuard`].
pub type TracingReadGuard<'a, T> = TracingRwLockGuard<'a, RwLockReadGuard<'a, T>>;
/// Wrapper around [`std::sync::RwLockWriteGuard`].
pub type TracingWriteGuard<'a, T> = TracingRwLockGuard<'a, RwLockWriteGuard<'a, T>>;

impl<T> TracingRwLock<T> {
    pub fn new(t: T) -> Self {
        Self {
            inner: RwLock::new(t),
            id: MutexId::new(),
        }
    }

    #[track_caller]
    pub fn read(&self) -> LockResult<TracingReadGuard<T>> {
        let mutex = self.id.get_borrowed();
        let result = self.inner.read();

        map_lockresult(result, |inner| TracingRwLockGuard { inner, mutex })
    }

    #[track_caller]
    pub fn write(&self) -> LockResult<TracingWriteGuard<T>> {
        let mutex = self.id.get_borrowed();
        let result = self.inner.write();

        map_lockresult(result, |inner| TracingRwLockGuard { inner, mutex })
    }

    #[track_caller]
    pub fn try_read(&self) -> TryLockResult<TracingReadGuard<T>> {
        let mutex = self.id.get_borrowed();
        let result = self.inner.try_read();

        map_trylockresult(result, |inner| TracingRwLockGuard { inner, mutex })
    }

    #[track_caller]
    pub fn try_write(&self) -> TryLockResult<TracingWriteGuard<T>> {
        let mutex = self.id.get_borrowed();
        let result = self.inner.try_write();

        map_trylockresult(result, |inner| TracingRwLockGuard { inner, mutex })
    }

    pub fn get_mut(&mut self) -> LockResult<&mut T> {
        self.inner.get_mut()
    }

    pub fn into_inner(self) -> LockResult<T> {
        self.inner.into_inner()
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

impl<'a, L, T> Deref for TracingRwLockGuard<'a, L>
where
    L: Deref<Target = T>,
{
    type Target = T;

    fn deref(&self) -> &Self::Target {
        self.inner.deref()
    }
}

impl<'a, T, L> DerefMut for TracingRwLockGuard<'a, L>
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
            let _read_lock = rwlock_clone.read().unwrap();
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
