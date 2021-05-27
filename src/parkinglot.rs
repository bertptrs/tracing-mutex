use crate::lockapi::TracingWrapper;

macro_rules! create_mutex_wrapper {
    ($wrapped:ty, $tracing_name:ident, $debug_name:ident) => {
        pub type $tracing_name<T> = lock_api::Mutex<TracingWrapper<$wrapped>, T>;

        #[cfg(debug_assertions)]
        pub type $debug_name<T> = $tracing_name<T>;
        #[cfg(not(debug_assertions))]
        pub type $debug_name<T> = lock_api::Mutex<$wrapped, T>;
    };
}

create_mutex_wrapper!(parking_lot::RawFairMutex, TracingFairMutex, DebugFairMutex);
create_mutex_wrapper!(parking_lot::RawMutex, TracingMutex, DebugMutex);

pub type TracingReentrantMutex<T> =
    lock_api::ReentrantMutex<TracingWrapper<parking_lot::RawMutex>, parking_lot::RawThreadId, T>;
#[cfg(debug_assertions)]
pub type DebugReentrantMutex<T> = TracingReentrantMutex<T>;
#[cfg(not(debug_assertions))]
pub type DebugReentrantMutex<T> = parking_lot::ReentrantMutex<T>;

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
