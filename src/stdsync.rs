//! Tracing mutex wrappers for locks found in `std::sync`.
//!
//! This module provides wrappers for `std::sync` primitives with exactly the same API and
//! functionality as their counterparts, with the exception that their acquisition order is tracked.
//!
//! Dedicated wrappers that provide the dependency tracing can be found in the [`tracing`] module.
//! The original primitives are available from [`std::sync`], imported as [`raw`] for convenience.
//!
//! If debug assertions are enabled, this module imports the primitives from [`tracing`], otherwise
//! it will import from [`raw`].
//!
//! ```rust
//! # use tracing_mutex::stdsync::tracing::Mutex;
//! # use tracing_mutex::stdsync::tracing::RwLock;
//! let mutex = Mutex::new(());
//! mutex.lock().unwrap();
//!
//! let rwlock = RwLock::new(());
//! rwlock.read().unwrap();
//! ```
pub use std::sync as raw;

#[cfg(not(debug_assertions))]
pub use std::sync::{Condvar, Mutex, MutexGuard, Once, RwLock, RwLockReadGuard, RwLockWriteGuard};

#[cfg(debug_assertions)]
pub use tracing::{Condvar, Mutex, MutexGuard, Once, RwLock, RwLockReadGuard, RwLockWriteGuard};

/// Dependency tracing versions of [`std::sync`].
pub mod tracing {
    use std::fmt;
    use std::ops::Deref;
    use std::ops::DerefMut;
    use std::sync;
    use std::sync::LockResult;
    use std::sync::OnceState;
    use std::sync::PoisonError;
    use std::sync::TryLockError;
    use std::sync::TryLockResult;
    use std::sync::WaitTimeoutResult;
    use std::time::Duration;

    use crate::BorrowedMutex;
    use crate::LazyMutexId;
    use crate::MutexId;

    /// Wrapper for [`std::sync::Mutex`].
    ///
    /// Refer to the [crate-level][`crate`] documentation for the differences between this struct and
    /// the one it wraps.
    #[derive(Debug, Default)]
    pub struct Mutex<T> {
        inner: sync::Mutex<T>,
        id: MutexId,
    }

    /// Wrapper for [`std::sync::MutexGuard`].
    ///
    /// Refer to the [crate-level][`crate`] documentation for the differences between this struct and
    /// the one it wraps.
    #[derive(Debug)]
    pub struct MutexGuard<'a, T> {
        inner: sync::MutexGuard<'a, T>,
        _mutex: BorrowedMutex<'a>,
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

    impl<T> Mutex<T> {
        /// Create a new tracing mutex with the provided value.
        pub fn new(t: T) -> Self {
            Self {
                inner: sync::Mutex::new(t),
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
        pub fn lock(&self) -> LockResult<MutexGuard<T>> {
            let mutex = self.id.get_borrowed();
            let result = self.inner.lock();

            let mapper = |guard| MutexGuard {
                _mutex: mutex,
                inner: guard,
            };

            map_lockresult(result, mapper)
        }

        /// Wrapper for [`std::sync::Mutex::try_lock`].
        ///
        /// # Panics
        ///
        /// This method participates in lock dependency tracking. If acquiring this lock introduces a
        /// dependency cycle, this method will panic.
        #[track_caller]
        pub fn try_lock(&self) -> TryLockResult<MutexGuard<T>> {
            let mutex = self.id.get_borrowed();
            let result = self.inner.try_lock();

            let mapper = |guard| MutexGuard {
                _mutex: mutex,
                inner: guard,
            };

            map_trylockresult(result, mapper)
        }

        /// Wrapper for [`std::sync::Mutex::is_poisoned`].
        pub fn is_poisoned(&self) -> bool {
            self.inner.is_poisoned()
        }

        /// Return a mutable reference to the underlying data.
        ///
        /// This method does not block as the locking is handled compile-time by the type system.
        pub fn get_mut(&mut self) -> LockResult<&mut T> {
            self.inner.get_mut()
        }

        /// Unwrap the mutex and return its inner value.
        pub fn into_inner(self) -> LockResult<T> {
            self.inner.into_inner()
        }
    }

    impl<T> From<T> for Mutex<T> {
        fn from(t: T) -> Self {
            Self::new(t)
        }
    }

    impl<'a, T> Deref for MutexGuard<'a, T> {
        type Target = T;

        fn deref(&self) -> &Self::Target {
            &self.inner
        }
    }

    impl<'a, T> DerefMut for MutexGuard<'a, T> {
        fn deref_mut(&mut self) -> &mut Self::Target {
            &mut self.inner
        }
    }

    impl<'a, T: fmt::Display> fmt::Display for MutexGuard<'a, T> {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            self.inner.fmt(f)
        }
    }

    /// Wrapper around [`std::sync::Condvar`].
    ///
    /// Allows `TracingMutexGuard` to be used with a `Condvar`. Unlike other structs in this module,
    /// this wrapper does not add any additional dependency tracking or other overhead on top of the
    /// primitive it wraps. All dependency tracking happens through the mutexes itself.
    ///
    /// # Panics
    ///
    /// This struct does not add any panics over the base implementation of `Condvar`, but panics due to
    /// dependency tracking may poison associated mutexes.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::sync::Arc;
    /// use std::thread;
    ///
    /// use tracing_mutex::stdsync::tracing::{Condvar, Mutex};
    ///
    /// let pair = Arc::new((Mutex::new(false), Condvar::new()));
    /// let pair2 = Arc::clone(&pair);
    ///
    /// // Spawn a thread that will unlock the condvar
    /// thread::spawn(move || {
    ///     let (lock, condvar) = &*pair2;
    ///     *lock.lock().unwrap() = true;
    ///     condvar.notify_one();
    /// });
    ///
    /// // Wait until the thread unlocks the condvar
    /// let (lock, condvar) = &*pair;
    /// let guard = lock.lock().unwrap();
    /// let guard = condvar.wait_while(guard, |started| !*started).unwrap();
    ///
    /// // Guard should read true now
    /// assert!(*guard);
    /// ```
    #[derive(Debug, Default)]
    pub struct Condvar(sync::Condvar);

    impl Condvar {
        /// Creates a new condition variable which is ready to be waited on and notified.
        pub fn new() -> Self {
            Default::default()
        }

        /// Wrapper for [`std::sync::Condvar::wait`].
        pub fn wait<'a, T>(&self, guard: MutexGuard<'a, T>) -> LockResult<MutexGuard<'a, T>> {
            let MutexGuard { _mutex, inner } = guard;

            map_lockresult(self.0.wait(inner), |inner| MutexGuard { _mutex, inner })
        }

        /// Wrapper for [`std::sync::Condvar::wait_while`].
        pub fn wait_while<'a, T, F>(
            &self,
            guard: MutexGuard<'a, T>,
            condition: F,
        ) -> LockResult<MutexGuard<'a, T>>
        where
            F: FnMut(&mut T) -> bool,
        {
            let MutexGuard { _mutex, inner } = guard;

            map_lockresult(self.0.wait_while(inner, condition), |inner| MutexGuard {
                _mutex,
                inner,
            })
        }

        /// Wrapper for [`std::sync::Condvar::wait_timeout`].
        pub fn wait_timeout<'a, T>(
            &self,
            guard: MutexGuard<'a, T>,
            dur: Duration,
        ) -> LockResult<(MutexGuard<'a, T>, WaitTimeoutResult)> {
            let MutexGuard { _mutex, inner } = guard;

            map_lockresult(self.0.wait_timeout(inner, dur), |(inner, result)| {
                (MutexGuard { _mutex, inner }, result)
            })
        }

        /// Wrapper for [`std::sync::Condvar::wait_timeout_while`].
        pub fn wait_timeout_while<'a, T, F>(
            &self,
            guard: MutexGuard<'a, T>,
            dur: Duration,
            condition: F,
        ) -> LockResult<(MutexGuard<'a, T>, WaitTimeoutResult)>
        where
            F: FnMut(&mut T) -> bool,
        {
            let MutexGuard { _mutex, inner } = guard;

            map_lockresult(
                self.0.wait_timeout_while(inner, dur, condition),
                |(inner, result)| (MutexGuard { _mutex, inner }, result),
            )
        }

        /// Wrapper for [`std::sync::Condvar::notify_one`].
        pub fn notify_one(&self) {
            self.0.notify_one();
        }

        /// Wrapper for [`std::sync::Condvar::notify_all`].
        pub fn notify_all(&self) {
            self.0.notify_all();
        }
    }

    /// Wrapper for [`std::sync::RwLock`].
    #[derive(Debug, Default)]
    pub struct RwLock<T> {
        inner: sync::RwLock<T>,
        id: MutexId,
    }

    /// Hybrid wrapper for both [`std::sync::RwLockReadGuard`] and [`std::sync::RwLockWriteGuard`].
    ///
    /// Please refer to [`RwLockReadGuard`] and [`RwLockWriteGuard`] for usable types.
    #[derive(Debug)]
    pub struct TracingRwLockGuard<'a, L> {
        inner: L,
        _mutex: BorrowedMutex<'a>,
    }

    /// Wrapper around [`std::sync::RwLockReadGuard`].
    pub type RwLockReadGuard<'a, T> = TracingRwLockGuard<'a, sync::RwLockReadGuard<'a, T>>;
    /// Wrapper around [`std::sync::RwLockWriteGuard`].
    pub type RwLockWriteGuard<'a, T> = TracingRwLockGuard<'a, sync::RwLockWriteGuard<'a, T>>;

    impl<T> RwLock<T> {
        pub fn new(t: T) -> Self {
            Self {
                inner: sync::RwLock::new(t),
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
        pub fn read(&self) -> LockResult<RwLockReadGuard<T>> {
            let mutex = self.id.get_borrowed();
            let result = self.inner.read();

            map_lockresult(result, |inner| TracingRwLockGuard {
                inner,
                _mutex: mutex,
            })
        }

        /// Wrapper for [`std::sync::RwLock::write`].
        ///
        /// # Panics
        ///
        /// This method participates in lock dependency tracking. If acquiring this lock introduces a
        /// dependency cycle, this method will panic.
        #[track_caller]
        pub fn write(&self) -> LockResult<RwLockWriteGuard<T>> {
            let mutex = self.id.get_borrowed();
            let result = self.inner.write();

            map_lockresult(result, |inner| TracingRwLockGuard {
                inner,
                _mutex: mutex,
            })
        }

        /// Wrapper for [`std::sync::RwLock::try_read`].
        ///
        /// # Panics
        ///
        /// This method participates in lock dependency tracking. If acquiring this lock introduces a
        /// dependency cycle, this method will panic.
        #[track_caller]
        pub fn try_read(&self) -> TryLockResult<RwLockReadGuard<T>> {
            let mutex = self.id.get_borrowed();
            let result = self.inner.try_read();

            map_trylockresult(result, |inner| TracingRwLockGuard {
                inner,
                _mutex: mutex,
            })
        }

        /// Wrapper for [`std::sync::RwLock::try_write`].
        ///
        /// # Panics
        ///
        /// This method participates in lock dependency tracking. If acquiring this lock introduces a
        /// dependency cycle, this method will panic.
        #[track_caller]
        pub fn try_write(&self) -> TryLockResult<RwLockWriteGuard<T>> {
            let mutex = self.id.get_borrowed();
            let result = self.inner.try_write();

            map_trylockresult(result, |inner| TracingRwLockGuard {
                inner,
                _mutex: mutex,
            })
        }

        /// Return a mutable reference to the underlying data.
        ///
        /// This method does not block as the locking is handled compile-time by the type system.
        pub fn get_mut(&mut self) -> LockResult<&mut T> {
            self.inner.get_mut()
        }

        /// Unwrap the mutex and return its inner value.
        pub fn into_inner(self) -> LockResult<T> {
            self.inner.into_inner()
        }
    }

    impl<T> From<T> for RwLock<T> {
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

    /// Wrapper around [`std::sync::Once`].
    ///
    /// Refer to the [crate-level][`crate`] documentaiton for the differences between this struct and
    /// the one it wraps.
    #[derive(Debug)]
    pub struct Once {
        inner: sync::Once,
        mutex_id: LazyMutexId,
    }

    impl Once {
        /// Create a new `Once` value.
        pub const fn new() -> Self {
            Self {
                inner: sync::Once::new(),
                mutex_id: LazyMutexId::new(),
            }
        }

        /// Wrapper for [`std::sync::Once::call_once`].
        ///
        /// # Panics
        ///
        /// In addition to the panics that `Once` can cause, this method will panic if calling it
        /// introduces a cycle in the lock dependency graph.
        pub fn call_once<F>(&self, f: F)
        where
            F: FnOnce(),
        {
            let _guard = self.mutex_id.get_borrowed();
            self.inner.call_once(f);
        }

        /// Performs the same operation as [`call_once`][Once::call_once] except it ignores
        /// poisoning.
        ///
        /// # Panics
        ///
        /// This method participates in lock dependency tracking. If acquiring this lock introduces a
        /// dependency cycle, this method will panic.
        pub fn call_once_force<F>(&self, f: F)
        where
            F: FnOnce(&OnceState),
        {
            let _guard = self.mutex_id.get_borrowed();
            self.inner.call_once_force(f);
        }

        /// Returns true if some `call_once` has completed successfully.
        pub fn is_completed(&self) -> bool {
            self.inner.is_completed()
        }
    }

    #[cfg(test)]
    mod tests {
        use std::sync::Arc;
        use std::thread;

        use super::*;

        #[test]
        fn test_mutex_usage() {
            let mutex = Arc::new(Mutex::new(0));

            assert_eq!(*mutex.lock().unwrap(), 0);
            *mutex.lock().unwrap() = 1;
            assert_eq!(*mutex.lock().unwrap(), 1);

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
            let rwlock = Arc::new(RwLock::new(0));

            assert_eq!(*rwlock.read().unwrap(), 0);
            assert_eq!(*rwlock.write().unwrap(), 0);
            *rwlock.write().unwrap() = 1;
            assert_eq!(*rwlock.read().unwrap(), 1);
            assert_eq!(*rwlock.write().unwrap(), 1);

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
        fn test_once_usage() {
            let once = Arc::new(Once::new());
            let once_clone = once.clone();

            assert!(!once.is_completed());

            let handle = thread::spawn(move || {
                assert!(!once_clone.is_completed());

                once_clone.call_once(|| {});

                assert!(once_clone.is_completed());
            });

            handle.join().unwrap();

            assert!(once.is_completed());
        }

        #[test]
        #[should_panic(expected = "Mutex order graph should not have cycles")]
        fn test_detect_cycle() {
            let a = Mutex::new(());
            let b = Mutex::new(());

            let hold_a = a.lock().unwrap();
            let _ = b.lock();

            drop(hold_a);

            let _hold_b = b.lock().unwrap();
            let _ = a.lock();
        }
    }
}
