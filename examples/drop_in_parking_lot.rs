//! This example shows how you can use the [`tracing-mutex`] crate as a drop-in replacement for the
//! parking_lot crate. By default, `tracing-mutex` offers a set of type aliases that allows you to
//! use cycle-checking in development, and raw primitives in release mode, but this way, you can
//! even remove the dependency altogether, or hide it behind a feature.
//!
//! You can use whatever conditional compilation makes sense in context.
use std::sync::Arc;

#[cfg(not(debug_assertions))]
use parking_lot;

#[cfg(debug_assertions)]
// Note: specifically use the `tracing` module, because at this point we are very sure we want to do
// deadlock tracing, so no need to use the automatic selection.
use tracing_mutex::parkinglot::tracing as parking_lot;

fn main() {
    let mutex = Arc::new(parking_lot::const_mutex(0));

    let handles: Vec<_> = (0..42)
        .map(|_| {
            let mutex = Arc::clone(&mutex);
            std::thread::spawn(move || *mutex.lock() += 1)
        })
        .collect();

    handles
        .into_iter()
        .for_each(|handle| handle.join().unwrap());

    assert_eq!(*mutex.lock(), 42);
}
