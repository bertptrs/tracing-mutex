//! Wrapper types and type aliases for tracing [`tokio`] mutexes.
//!
//! This module provides type aliases that use the [`lockapi`][crate::lockapi] module to provide
//! tracing variants of the `parking_lot` primitives. Each of the `TracingX` type aliases wraps an
//! `X` in the `parkint_lot` api with dependency tracking, and a `DebugX` will refer to a `TracingX`
//! when  `debug_assertions` are enabled and to `X` when they're not. This can be used to aid
//! debugging in development while enjoying maximum performance in production.
//!
//! # Usage
//!
//! ```
//! # use std::sync::Arc;
//! # use std::thread;
//! # use lock_api::Mutex;
//! # use tracing_mutex::parkinglot::TracingMutex;
//! let mutex = Arc::new(TracingMutex::new(0));
//!
//! let handles: Vec<_> = (0..10).map(|_| {
//!    let mutex = Arc::clone(&mutex);
//!    thread::spawn(move || *mutex.lock() += 1)
//! }).collect();
//!
//! handles.into_iter().for_each(|handle| handle.join().unwrap());
//!
//! // All threads completed so the value should be 10.
//! assert_eq!(10, *mutex.lock());
//! ```
//!
//! # Limitations
//!
//! The main lock for the global state is still provided by `std::sync` and the tracing primitives
//! are larger than the `parking_lot` primitives they wrap, so there can be a performance
//! degradation between using this and using `parking_lot` directly. If this is of concern to you,
//! try using the `DebugX`-structs, which provide cycle detection only when `debug_assertions` are
//! enabled and have no overhead when they're not.
//!
//! In addition, the mutex guards returned by the tracing wrappers are `!Send`, regardless of
//! whether `parking_lot` is configured to have `Send` mutex guards. This is a limitation of the
//! current bookkeeping system.
use std::fmt;
use std::ops::{Deref, DerefMut};
use tokio::sync::{Mutex, MutexGuard, RwLock, RwLockReadGuard, RwLockWriteGuard, TryLockError};
use crate::{BorrowedMutex, MutexId};

/// Wrapper for [`std::sync::Mutex`].
///
/// Refer to the [crate-level][`crate`] documentaiton for the differences between this struct and
/// the one it wraps.
#[derive(Debug, Default)]
pub struct TracingMutex<T> {
    inner: Mutex<T>,
    id: MutexId,
}

/// Wrapper for [`std::sync::MutexGuard`].
///
/// Refer to the [crate-level][`crate`] documentation for the differences between this struct and
/// the one it wraps.
#[derive(Debug)]
pub struct TracingMutexGuard<'a, T> {
    inner: MutexGuard<'a, T>,
    _mutex: BorrowedMutex<'a>,
}

impl<T> TracingMutex<T> {
    /// Create a new tracing mutex with the provided value.
    pub fn new(t: T) -> Self {
        Self {
            inner: Mutex::new(t),
            id: MutexId::new(),
        }
    }

    /// Wrapper for [`std::sync::Mutex::lock`].
    ///
    /// # Panics
    ///
    /// This method participates in lock dependency tracking. If acquiring this lock introduces a
    /// dependency cycle, this method will panic.
    #[track_caller]
    pub async fn lock(&self) -> TracingMutexGuard<'_, T> {
        let mutex = self.id.get_borrowed();
        let guard = self.inner.lock().await;

        TracingMutexGuard {
            _mutex: mutex,
            inner: guard,
        }
    }

    /// Wrapper for [`tokio::sync::Mutex::try_lock`].
    ///
    /// # Panics
    ///
    /// This method participates in lock dependency tracking. If acquiring this lock introduces a
    /// dependency cycle, this method will panic.
    #[track_caller]
    pub async fn try_lock(&self) -> Result<TracingMutexGuard<'_, T>, TryLockError> {
        let mutex = self.id.get_borrowed();
        let result = self.inner.try_lock();

        match result {
            Ok(guard) => Ok(TracingMutexGuard {
                _mutex: mutex,
                inner: guard,
            }),
            Err(e) => Err(e),
        }
    }

    /// Return a mutable reference to the underlying data.
    ///
    /// This method does not block as the locking is handled compile-time by the type system.
    pub fn get_mut(&mut self) -> &mut T {
        self.inner.get_mut()
    }

    /// Unwrap the mutex and return its inner value.
    pub fn into_inner(self) -> T {
        self.inner.into_inner()
    }
}

impl<T> From<T> for TracingMutex<T> {
    fn from(t: T) -> Self {
        Self::new(t)
    }
}

impl<'a, T> Deref for TracingMutexGuard<'a, T> {
    type Target = T;

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
#[derive(Debug, Default)]
pub struct TracingRwLock<T> {
    inner: RwLock<T>,
    id: MutexId,
}

/// Hybrid wrapper for both [`tokio::sync::RwLockReadGuard`] and [`tokio::sync::RwLockWriteGuard`].
///
/// Please refer to [`TracingReadGuard`] and [`TracingWriteGuard`] for usable types.
#[derive(Debug)]
pub struct TracingRwLockGuard<'a, L> {
    inner: L,
    _mutex: BorrowedMutex<'a>,
}

/// Wrapper around [`tokio::sync::RwLockReadGuard`].
pub type TracingReadGuard<'a, T> = TracingRwLockGuard<'a, RwLockReadGuard<'a, T>>;
/// Wrapper around [`tokio::sync::RwLockWriteGuard`].
pub type TracingWriteGuard<'a, T> = TracingRwLockGuard<'a, RwLockWriteGuard<'a, T>>;
/// Wrapper around [`tokio::sync::RwLockReadGuard`].
pub type TracingTryReadGuard<'a, T> = TracingRwLockGuard<'a, Result<RwLockReadGuard<'a, T>, TryLockError>>;
/// Wrapper around [`tokio::sync::RwLockWriteGuard`].
pub type TracingTryWriteGuard<'a, T> = TracingRwLockGuard<'a, Result<RwLockWriteGuard<'a, T>, TryLockError>>;

impl<T> TracingRwLock<T> {
    pub fn new(t: T) -> Self {
        Self {
            inner: RwLock::new(t),
            id: MutexId::new(),
        }
    }

    /// Wrapper for [`std::sync::RwLock::read`].
    ///
    /// # Panics
    ///
    /// This method participates in lock dependency tracking. If acquiring this lock introduces a
    /// dependency cycle, this method will panic.
    #[track_caller]
    pub async fn read(&self) -> TracingReadGuard<'_, T> {
        let mutex = self.id.get_borrowed();
        let result = self.inner.read().await;

        TracingRwLockGuard {
            inner: result,
            _mutex: mutex,
        }
    }

    /// Wrapper for [`std::sync::RwLock::write`].
    ///
    /// # Panics
    ///
    /// This method participates in lock dependency tracking. If acquiring this lock introduces a
    /// dependency cycle, this method will panic.
    #[track_caller]
    pub async fn write(&self) -> TracingWriteGuard<'_, T> {
        let mutex = self.id.get_borrowed();
        let result = self.inner.write().await;

        TracingRwLockGuard {
            inner: result,
            _mutex: mutex,
        }
    }

    /// Wrapper for [`std::sync::RwLock::try_read`].
    ///
    /// # Panics
    ///
    /// This method participates in lock dependency tracking. If acquiring this lock introduces a
    /// dependency cycle, this method will panic.
    #[track_caller]
    pub async fn try_read(&self) -> TracingTryReadGuard<'_, T> {
        let mutex = self.id.get_borrowed();
        let result = self.inner.try_read();

       TracingRwLockGuard {
            inner: result,
            _mutex: mutex,
        }
    }

    /// Wrapper for [`std::sync::RwLock::try_write`].
    ///
    /// # Panics
    ///
    /// This method participates in lock dependency tracking. If acquiring this lock introduces a
    /// dependency cycle, this method will panic.
    #[track_caller]
    pub fn try_write(&self) -> TracingTryWriteGuard<T> {
        let mutex = self.id.get_borrowed();
        let result = self.inner.try_write();

        TracingRwLockGuard {
            inner: result,
            _mutex: mutex,
        }
    }

    /// Return a mutable reference to the underlying data.
    ///
    /// This method does not block as the locking is handled compile-time by the type system.
    pub fn get_mut(&mut self) -> &mut T {
        self.inner.get_mut()
    }

    /// Unwrap the mutex and return its inner value.
    pub fn into_inner(self) -> T {
        self.inner.into_inner()
    }
}

impl<T> From<T> for TracingRwLock<T> {
    fn from(t: T) -> Self {
        Self::new(t)
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

    #[tokio::test]
    async fn test_mutex_usage() {
        let mutex = Arc::new(TracingMutex::new(()));
        let local_lock = mutex.lock().await;
        drop(local_lock);

        tokio::spawn(async move {
            let _remote_lock = mutex.lock();
        }).await.unwrap();
    }

    #[tokio::test]
    #[should_panic]
    async fn test_mutex_conflict() {
        let mutexes = [
            TracingMutex::new(()),
            TracingMutex::new(()),
            TracingMutex::new(()),
        ];

        for i in 0..3 {
            let _first_lock = mutexes[i].lock().await;
            let _second_lock = mutexes[(i + 1) % 3].lock().await;
        }
    }

    #[tokio::test]
    async fn test_rwlock_usage() {
        let lock = Arc::new(TracingRwLock::new(()));
        let lock2 = Arc::clone(&lock);

        let _read_lock = lock.read();

        // Should be able to acquire lock in the background
        thread::spawn(move || {
            let _read_lock = lock2.read();
        })
            .join()
            .unwrap();
    }
}
