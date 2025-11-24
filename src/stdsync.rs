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

// Skip reformatting the combined imports as it duplicates the guards
#[rustfmt::skip]
#[cfg(not(debug_assertions))]
pub use std::sync::{
    Condvar, Mutex, MutexGuard, Once, OnceLock, RwLock, RwLockReadGuard, RwLockWriteGuard,
};

#[rustfmt::skip]
#[cfg(debug_assertions)]
pub use tracing::{
    Condvar, Mutex, MutexGuard, Once, OnceLock, RwLock, RwLockReadGuard, RwLockWriteGuard,
};

#[cfg(all(has_std__sync__LazyLock, debug_assertions))]
pub use tracing::LazyLock;

#[cfg(all(has_std__sync__LazyLock, not(debug_assertions)))]
pub use std::sync::LazyLock;

/// Dependency tracing versions of [`std::sync`].
pub mod tracing;
