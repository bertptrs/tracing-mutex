//! Wrapper implementations for [`lock_api`].
use lock_api::GuardNoSend;
use lock_api::RawMutex;
use lock_api::RawMutexFair;
use lock_api::RawMutexTimed;

use crate::LazyMutexId;

/// Tracing wrapper for all [`lock_api`] traits.
///
/// This wrapper implements any of the locking traits available, given that the wrapped type
/// implements them. As such, this wrapper can be used both for normal mutexes and rwlocks.
#[derive(Debug, Default)]
pub struct TracingWrapper<T> {
    inner: T,
    id: LazyMutexId,
}

impl<T> TracingWrapper<T> {
    /// Mark this lock as held in the dependency graph.
    fn mark_held(&self) {
        self.id.mark_held();
    }

    /// Mark this lock as released in the dependency graph.
    ///
    /// # Safety
    ///
    /// This function should only be called when the lock has been previously acquired by this
    /// thread.
    unsafe fn mark_released(&self) {
        self.id.mark_released();
    }

    /// Conditionally lock the mutex.
    ///
    /// First acquires the lock, then runs the provided function. If that function returns true,
    /// then the lock is kept, otherwise the mutex is immediately marked as relased.
    fn conditionally_lock(&self, f: impl FnOnce() -> bool) -> bool {
        // Mark as locked while we try to do the thing
        self.mark_held();

        if f() {
            true
        } else {
            // Safety: we just locked it above.
            unsafe { self.mark_released() }
            false
        }
    }
}

unsafe impl<T> RawMutex for TracingWrapper<T>
where
    T: RawMutex,
{
    const INIT: Self = Self {
        inner: T::INIT,
        id: LazyMutexId::new(),
    };

    /// Always equal to [`GuardNoSend`], as an implementation detail in the tracking system requires
    /// this behaviour. May change in the future to reflect the actual guard type from the wrapped
    /// primitive.
    type GuardMarker = GuardNoSend;

    fn lock(&self) {
        self.mark_held();
        self.inner.lock();
    }

    fn try_lock(&self) -> bool {
        self.conditionally_lock(|| self.inner.try_lock())
    }

    unsafe fn unlock(&self) {
        self.inner.unlock();
        self.mark_released();
    }

    fn is_locked(&self) -> bool {
        // Can't use the default implementation as the inner type might've overwritten it.
        self.inner.is_locked()
    }
}

unsafe impl<T> RawMutexFair for TracingWrapper<T>
where
    T: RawMutexFair,
{
    unsafe fn unlock_fair(&self) {
        self.inner.unlock_fair();
        self.mark_released();
    }

    unsafe fn bump(&self) {
        // Bumping effectively doesn't change which locks are held, so we don't need to manage the
        // lock state.
        self.inner.bump();
    }
}

unsafe impl<T> RawMutexTimed for TracingWrapper<T>
where
    T: RawMutexTimed,
{
    type Duration = T::Duration;

    type Instant = T::Instant;

    fn try_lock_for(&self, timeout: Self::Duration) -> bool {
        self.conditionally_lock(|| self.inner.try_lock_for(timeout))
    }

    fn try_lock_until(&self, timeout: Self::Instant) -> bool {
        self.conditionally_lock(|| self.inner.try_lock_until(timeout))
    }
}
