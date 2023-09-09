//! Show what a crash looks like
//!
//! This shows what a traceback of a cycle detection looks like. It is expected to crash.
use tracing_mutex::stdsync::Mutex;

fn main() {
    let a = Mutex::new(());
    let b = Mutex::new(());
    let c = Mutex::new(());

    // Create an edge from a to b
    {
        let _a = a.lock();
        let _b = b.lock();
    }

    // Create an edge from b to c
    {
        let _b = b.lock();
        let _c = c.lock();
    }

    // Now crash by trying to add an edge from c to a
    let _c = c.lock();
    let _a = a.lock(); // This line will crash
}
