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

pub use parking_lot::OnceState;
pub use parking_lot::RawThreadId;
pub use parking_lot::WaitTimeoutResult;

pub mod tracing;

#[cfg(debug_assertions)]
pub use tracing::{
    FairMutex, FairMutexGuard, MappedFairMutexGuard, MappedMutexGuard, MappedReentrantMutexGuard,
    MappedRwLockReadGuard, MappedRwLockWriteGuard, Mutex, MutexGuard, Once, RawFairMutex, RawMutex,
    RawRwLock, ReentrantMutex, ReentrantMutexGuard, RwLock, RwLockReadGuard,
    RwLockUpgradableReadGuard, RwLockWriteGuard, const_fair_mutex, const_mutex,
    const_reentrant_mutex, const_rwlock,
};

#[cfg(not(debug_assertions))]
pub use parking_lot::{
    FairMutex, FairMutexGuard, MappedFairMutexGuard, MappedMutexGuard, MappedReentrantMutexGuard,
    MappedRwLockReadGuard, MappedRwLockWriteGuard, Mutex, MutexGuard, Once, RawFairMutex, RawMutex,
    RawRwLock, ReentrantMutex, ReentrantMutexGuard, RwLock, RwLockReadGuard,
    RwLockUpgradableReadGuard, RwLockWriteGuard, const_fair_mutex, const_mutex,
    const_reentrant_mutex, const_rwlock,
};
