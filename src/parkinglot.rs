use crate::lockapi::TracingWrapper;

macro_rules! debug_variant {
    ($debug_name:ident, $tracing_name:ident, $normal_name:ty) => {
        type $tracing_name = TracingWrapper<$normal_name>;

        #[cfg(debug_assertions)]
        type $debug_name = TracingWrapper<$normal_name>;
        #[cfg(not(debug_assertions))]
        type $debug_name = $normal_name;
    };
}

debug_variant!(
    DebugRawFairMutex,
    TracingRawFairMutex,
    parking_lot::RawFairMutex
);
debug_variant!(DebugRawMutex, TracingRawMutex, parking_lot::RawMutex);

/// Dependency tracking fair mutex. See: [`parking_lot::FairMutex`].
pub type TracingFairMutex<T> = lock_api::Mutex<TracingRawFairMutex, T>;
/// Mutex guard for [`TracingFairMutex`].
pub type TracingFairMutexGuard<'a, T> = lock_api::MutexGuard<'a, TracingRawFairMutex, T>;
/// Debug-only dependency tracking fair mutex.
///
/// If debug assertions are enabled this resolves to [`TracingFairMutex`] and to
/// [`parking_lot::FairMutex`] if it's not.
pub type DebugFairMutex<T> = lock_api::Mutex<DebugRawFairMutex, T>;
/// Mutex guard for [`DebugFairMutex`].
pub type DebugFairMutexGuard<'a, T> = lock_api::MutexGuard<'a, DebugRawFairMutex, T>;

/// Dependency tracking mutex. See: [`parking_lot::Mutex`].
pub type TracingMutex<T> = lock_api::Mutex<TracingRawMutex, T>;
/// Mutex guard for [`TracingMutex`].
pub type TracingMutexGuard<'a, T> = lock_api::MutexGuard<'a, TracingRawMutex, T>;
/// Debug-only dependency tracking mutex.
///
/// If debug assertions are enabled this resolves to [`TracingMutex`] and to [`parking_lot::Mutex`]
/// if it's not.
pub type DebugMutex<T> = lock_api::Mutex<DebugRawMutex, T>;
/// Mutex guard for [`DebugMutex`].
pub type DebugMutexGuard<'a, T> = lock_api::MutexGuard<'a, DebugRawMutex, T>;

/// Dependency tracking reentrant mutex. See: [`parking_lot::ReentrantMutex`].
pub type TracingReentrantMutex<T> =
    lock_api::ReentrantMutex<TracingWrapper<parking_lot::RawMutex>, parking_lot::RawThreadId, T>;
/// Mutex guard for [`TracingReentrantMutex`].
pub type TracingReentrantMutexGuard<'a, T> = lock_api::ReentrantMutexGuard<
    'a,
    TracingWrapper<parking_lot::RawMutex>,
    parking_lot::RawThreadId,
    T,
>;

/// Debug-only dependency tracking reentrant mutex.
///
/// If debug assertions are enabled this resolves to [`TracingReentrantMutex`] and to
/// [`parking_lot::ReentrantMutex`] if it's not.
pub type DebugReentrantMutex<T> =
    lock_api::ReentrantMutex<DebugRawMutex, parking_lot::RawThreadId, T>;
/// Mutex guard for [`DebugReentrantMutex`].
pub type DebugReentrantMutexGuard<'a, T> =
    lock_api::ReentrantMutexGuard<'a, DebugRawMutex, parking_lot::RawThreadId, T>;

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::thread;

    use super::*;

    #[test]
    fn test_mutex_usage() {
        let mutex = Arc::new(TracingMutex::new(()));
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
            TracingMutex::new(()),
            TracingMutex::new(()),
            TracingMutex::new(()),
        ];

        for i in 0..3 {
            let _first_lock = mutexes[i].lock();
            let _second_lock = mutexes[(i + 1) % 3].lock();
        }
    }
}
