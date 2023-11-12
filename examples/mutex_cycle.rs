//! Show what a crash looks like
//!
//! This shows what a traceback of a cycle detection looks like. It is expected to crash when run in
//! debug mode, because it might deadlock. In release mode, no tracing is used and the program may
//! do any of the following:
//!
//! - Return a random valuation of `a`, `b`, and `c`. The implementation has a race-condition by
//!   design. I have observed (4, 3, 6), but also (6, 3, 5).
//! - Deadlock forever.
//!
//! One can increase the SLEEP_TIME constant to increase the likelihood of a deadlock to occur. On
//! my machine, 1ns of sleep time gives about a 50/50 chance of the program deadlocking.
use std::thread;
use std::time::Duration;

use tracing_mutex::stdsync::Mutex;

fn main() {
    let a = Mutex::new(1);
    let b = Mutex::new(2);
    let c = Mutex::new(3);

    // Increase this time to increase the likelihood of a deadlock.
    const SLEEP_TIME: Duration = Duration::from_nanos(1);

    // Depending on random CPU performance, this section may deadlock, or may return a result. With
    // tracing enabled, the potential deadlock is always detected and a backtrace should be
    // produced.
    thread::scope(|s| {
        // Create an edge from a to b
        s.spawn(|| {
            let a = a.lock().unwrap();
            thread::sleep(SLEEP_TIME);
            *b.lock().unwrap() += *a;
        });

        // Create an edge from b to c
        s.spawn(|| {
            let b = b.lock().unwrap();
            thread::sleep(SLEEP_TIME);
            *c.lock().unwrap() += *b;
        });

        // Create an edge from c to a
        //
        // N.B. the program can crash on any of the three edges, as there is no guarantee which
        // thread will execute first. Nevertheless, any one of them is guaranteed to panic with
        // tracing enabled.
        s.spawn(|| {
            let c = c.lock().unwrap();
            thread::sleep(SLEEP_TIME);
            *a.lock().unwrap() += *c;
        });
    });

    println!(
        "{}, {}, {}",
        a.into_inner().unwrap(),
        b.into_inner().unwrap(),
        c.into_inner().unwrap()
    );
}
