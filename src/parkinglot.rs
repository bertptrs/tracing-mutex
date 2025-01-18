//! Wrapper types and type aliases for tracing [`parking_lot`] mutexes.
//!
//! This module provides type aliases that use the [`lockapi`][crate::lockapi] module to provide
//! tracing variants of the `parking_lot` primitives. The [`tracing`] module contains type aliases
//! that use dependency tracking, while the main `parking_lot` primitives are reexported as [`raw`].
//!
//! This main module imports from [`tracing`] when `debug_assertions` are enabled, and from [`raw`]
//! when they're not. Note that primitives for which no tracing wrapper exists are not imported into
//! the main module.
//!
//! # Usage
//!
//! ```
//! # use std::sync::Arc;
//! # use std::thread;
//! use tracing_mutex::parkinglot::Mutex;
//! let mutex = Arc::new(Mutex::new(0));
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

pub use parking_lot as raw;

#[cfg(debug_assertions)]
pub use tracing::{
    FairMutex, FairMutexGuard, MappedFairMutexGuard, MappedMutexGuard, MappedReentrantMutexGuard,
    MappedRwLockReadGuard, MappedRwLockWriteGuard, Mutex, MutexGuard, Once, OnceState,
    ReentrantMutex, ReentrantMutexGuard, RwLock, RwLockReadGuard, RwLockUpgradableReadGuard,
    RwLockWriteGuard,
};

#[cfg(not(debug_assertions))]
pub use parking_lot::{
    FairMutex, FairMutexGuard, MappedFairMutexGuard, MappedMutexGuard, MappedReentrantMutexGuard,
    MappedRwLockReadGuard, MappedRwLockWriteGuard, Mutex, MutexGuard, Once, OnceState,
    ReentrantMutex, ReentrantMutexGuard, RwLock, RwLockReadGuard, RwLockUpgradableReadGuard,
    RwLockWriteGuard,
};

/// Dependency tracing wrappers for [`parking_lot`].
pub mod tracing {
    pub use parking_lot::OnceState;

    use crate::lockapi::TracingWrapper;
    use crate::LazyMutexId;

    type RawFairMutex = TracingWrapper<parking_lot::RawFairMutex>;
    type RawMutex = TracingWrapper<parking_lot::RawMutex>;
    type RawRwLock = TracingWrapper<parking_lot::RawRwLock>;

    /// Dependency tracking fair mutex. See: [`parking_lot::FairMutex`].
    pub type FairMutex<T> = lock_api::Mutex<RawFairMutex, T>;
    /// Mutex guard for [`FairMutex`].
    pub type FairMutexGuard<'a, T> = lock_api::MutexGuard<'a, RawFairMutex, T>;
    /// RAII guard for [`FairMutexGuard::map`].
    pub type MappedFairMutexGuard<'a, T> = lock_api::MappedMutexGuard<'a, RawFairMutex, T>;

    /// Dependency tracking mutex. See: [`parking_lot::Mutex`].
    pub type Mutex<T> = lock_api::Mutex<RawMutex, T>;
    /// Mutex guard for [`Mutex`].
    pub type MutexGuard<'a, T> = lock_api::MutexGuard<'a, RawMutex, T>;
    /// RAII guard for [`MutexGuard::map`].
    pub type MappedMutexGuard<'a, T> = lock_api::MappedMutexGuard<'a, RawMutex, T>;

    /// Dependency tracking reentrant mutex. See: [`parking_lot::ReentrantMutex`].
    ///
    /// **Note:** due to the way dependencies are tracked, this mutex can only be acquired directly
    /// after itself. Acquiring any other mutex in between introduces a dependency cycle, and will
    /// therefore be rejected.
    pub type ReentrantMutex<T> = lock_api::ReentrantMutex<RawMutex, parking_lot::RawThreadId, T>;
    /// Mutex guard for [`ReentrantMutex`].
    pub type ReentrantMutexGuard<'a, T> =
        lock_api::ReentrantMutexGuard<'a, RawMutex, parking_lot::RawThreadId, T>;
    /// RAII guard for `ReentrantMutexGuard::map`.
    pub type MappedReentrantMutexGuard<'a, T> =
        lock_api::MappedReentrantMutexGuard<'a, RawMutex, parking_lot::RawThreadId, T>;

    /// Dependency tracking RwLock. See: [`parking_lot::RwLock`].
    pub type RwLock<T> = lock_api::RwLock<RawRwLock, T>;
    /// Read guard for [`RwLock`].
    pub type RwLockReadGuard<'a, T> = lock_api::RwLockReadGuard<'a, RawRwLock, T>;
    /// Upgradable Read guard for [`RwLock`].
    pub type RwLockUpgradableReadGuard<'a, T> =
        lock_api::RwLockUpgradableReadGuard<'a, RawRwLock, T>;
    /// Write guard for [`RwLock`].
    pub type RwLockWriteGuard<'a, T> = lock_api::RwLockWriteGuard<'a, RawRwLock, T>;
    /// RAII guard for `RwLockReadGuard::map`.
    pub type MappedRwLockReadGuard<'a, T> = lock_api::MappedRwLockReadGuard<'a, RawRwLock, T>;
    /// RAII guard for `RwLockWriteGuard::map`.
    pub type MappedRwLockWriteGuard<'a, T> = lock_api::MappedRwLockWriteGuard<'a, RawRwLock, T>;

    /// A dependency-tracking wrapper for [`parking_lot::Once`].
    #[derive(Debug, Default)]
    pub struct Once {
        inner: parking_lot::Once,
        id: LazyMutexId,
    }

    impl Once {
        /// Create a new `Once` value.
        pub const fn new() -> Self {
            Self {
                inner: parking_lot::Once::new(),
                id: LazyMutexId::new(),
            }
        }

        /// Returns the current state of this `Once`.
        pub fn state(&self) -> OnceState {
            self.inner.state()
        }

        /// This call is considered as "locking this `Once`" and it participates in dependency
        /// tracking as such.
        ///
        /// # Panics
        ///
        /// This method will panic if `f` panics, poisoning this `Once`. In addition, this function
        /// panics when the lock acquisition order is determined to be inconsistent.
        pub fn call_once(&self, f: impl FnOnce()) {
            self.id.with_held(|| self.inner.call_once(f));
        }

        /// Performs the given initialization routine once and only once.
        ///
        /// This method is identical to [`Once::call_once`] except it ignores poisoning.
        pub fn call_once_force(&self, f: impl FnOnce(OnceState)) {
            self.id.with_held(|| self.inner.call_once_force(f));
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::thread;

    use super::tracing;

    #[test]
    fn test_mutex_usage() {
        let mutex = Arc::new(tracing::Mutex::new(()));
        let local_lock = mutex.lock();
        drop(local_lock);

        thread::spawn(move || {
            let _remote_lock = mutex.lock();
        })
        .join()
        .unwrap();
    }

    #[test]
    #[should_panic]
    fn test_mutex_conflict() {
        let mutexes = [
            tracing::Mutex::new(()),
            tracing::Mutex::new(()),
            tracing::Mutex::new(()),
        ];

        for i in 0..3 {
            let _first_lock = mutexes[i].lock();
            let _second_lock = mutexes[(i + 1) % 3].lock();
        }
    }

    #[test]
    fn test_rwlock_usage() {
        let lock = Arc::new(tracing::RwLock::new(()));
        let lock2 = Arc::clone(&lock);

        let _read_lock = lock.read();

        // Should be able to acquire lock in the background
        thread::spawn(move || {
            let _read_lock = lock2.read();
        })
        .join()
        .unwrap();
    }

    #[test]
    fn test_rwlock_upgradable_read_usage() {
        let lock = tracing::RwLock::new(());

        // Should be able to acquire an upgradable read lock.
        let upgradable_guard: tracing::RwLockUpgradableReadGuard<'_, _> = lock.upgradable_read();

        // Should be able to upgrade the guard.
        let _write_guard: tracing::RwLockWriteGuard<'_, _> =
            tracing::RwLockUpgradableReadGuard::upgrade(upgradable_guard);
    }

    #[test]
    fn test_once_usage() {
        let once = Arc::new(tracing::Once::new());
        let once_clone = once.clone();

        assert!(!once_clone.state().done());

        let handle = thread::spawn(move || {
            assert!(!once_clone.state().done());

            once_clone.call_once(|| {});

            assert!(once_clone.state().done());
        });

        handle.join().unwrap();

        assert!(once.state().done());
    }
}
