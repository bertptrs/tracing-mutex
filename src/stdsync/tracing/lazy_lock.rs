//! Wrapper implementation for LazyLock
//!
//! This lives in a separate module as LazyLock would otherwise raise our MSRV to 1.80. Reevaluate
//! this in the future.
use std::fmt;
use std::fmt::Debug;
use std::ops::Deref;

use crate::LazyMutexId;

/// Wrapper for [`std::sync::LazyLock`]
///
/// This wrapper participates in cycle detection like all other primitives in this crate. It should
/// only be possible to encounter cycles when acquiring mutexes in the initialisation function.
///
/// # Examples
///
/// ```
/// use tracing_mutex::stdsync::tracing::LazyLock;
///
/// static LOCK: LazyLock<i32> = LazyLock::new(|| {
///     println!("Hello, world!");
///     42
/// });
///
/// // This should print "Hello, world!"
/// println!("{}", *LOCK);
/// // This should not.
/// println!("{}", *LOCK);
/// ```
pub struct LazyLock<T, F = fn() -> T> {
    inner: std::sync::LazyLock<T, F>,
    id: LazyMutexId,
}

impl<T, F: FnOnce() -> T> LazyLock<T, F> {
    /// Creates a new lazy value with the given initializing function.
    pub const fn new(f: F) -> LazyLock<T, F> {
        Self {
            id: LazyMutexId::new(),
            inner: std::sync::LazyLock::new(f),
        }
    }

    /// Force this lazy lock to be evaluated.
    ///
    /// This is equivalent to dereferencing, but is more explicit.
    pub fn force(this: &LazyLock<T, F>) -> &T {
        &*this
    }
}

impl<T, F: FnOnce() -> T> Deref for LazyLock<T, F> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        self.id.with_held(|| &*self.inner)
    }
}

impl<T: Default> Default for LazyLock<T> {
    /// Return a `LazyLock` that is initialized through [`Default`].
    fn default() -> Self {
        Self::new(Default::default)
    }
}

impl<T: Debug, F> Debug for LazyLock<T, F> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Cannot implement this ourselves because the get() used is nightly, so delegate.
        self.inner.fmt(f)
    }
}

#[cfg(test)]
mod tests {
    use crate::stdsync::Mutex;

    use super::*;

    #[test]
    fn test_only_init_once() {
        let mut init_counter = 0;

        let lock = LazyLock::new(|| {
            init_counter += 1;
            42
        });

        assert_eq!(*lock, 42);
        LazyLock::force(&lock);

        // Ensure we can access the init counter
        drop(lock);

        assert_eq!(init_counter, 1);
    }

    #[test]
    #[should_panic(expected = "Found cycle")]
    fn test_panic_with_cycle() {
        let mutex = Mutex::new(());

        let lock = LazyLock::new(|| *mutex.lock().unwrap());

        // Establish the relation from lock to mutex
        LazyLock::force(&lock);

        // Now do it the other way around, which should crash
        let _guard = mutex.lock().unwrap();
        LazyLock::force(&lock);
    }
}
