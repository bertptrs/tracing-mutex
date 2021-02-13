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

impl<T> TracingMutex<T> {
    pub fn new(t: T) -> Self {
        Self {
            inner: Mutex::new(t),
            id: next_mutex_id(),
        }
    }

    pub fn lock(&self) -> LockResult<TracingMutexGuard<T>> {
        let result = self.inner.lock();
        self.register_lock();

        let mapper = |guard| TracingMutexGuard {
            mutex: self,
            inner: guard,
        };

        result
            .map(mapper)
            .map_err(|poison| PoisonError::new(mapper(poison.into_inner())))
    }

    pub fn try_lock(&self) -> TryLockResult<TracingMutexGuard<T>> {
        let result = self.inner.try_lock();
        self.register_lock();

        let mapper = |guard| TracingMutexGuard {
            mutex: self,
            inner: guard,
        };

        result.map(mapper).map_err(|error| match error {
            TryLockError::Poisoned(poison) => PoisonError::new(mapper(poison.into_inner())).into(),
            TryLockError::WouldBlock => TryLockError::WouldBlock,
        })
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

    fn register_lock(&self) {
        HELD_LOCKS.with(|locks| locks.borrow_mut().push(self.id))
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

    #[test]
    fn test_next_mutex_id() {
        let initial = next_mutex_id();
        let next = next_mutex_id();

        // Can't assert N + 1 because multiple threads running tests
        assert!(initial < next);
    }

    #[test]
    fn test_mutex_usage() {
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
}
