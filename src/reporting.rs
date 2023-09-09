//! Cycle reporting primitives
//!
//! This module exposes [`Dep`], which resolves to either something that tracks dependencies or to
//! something that doesn't.  It should only be assumed to implement the [`Reportable`] trait.
use std::backtrace::Backtrace;
use std::borrow::Cow;
use std::fmt::Write;
use std::sync::Arc;

#[cfg(feature = "backtraces")]
pub type Dep = MutexDep<Arc<Backtrace>>;
#[cfg(not(feature = "backtraces"))]
pub type Dep = MutexDep<()>;

// Base message to be reported when cycle is detected
const BASE_MESSAGE: &str = "Found cycle in mutex dependency graph:";

pub trait Reportable: Clone {
    /// Capture the current state
    fn capture() -> Self;

    /// Format a trace of state for human readable consumption.
    fn panic_message(trace: &[Self]) -> Cow<'static, str>;
}

#[derive(Clone)]
pub struct MutexDep<T>(T);

/// Use a unit as tracing data: no tracing.
///
/// This should have no runtime overhead for capturing traces and should therefore be cheap enough
/// for most purposes.
impl Reportable for MutexDep<()> {
    fn capture() -> Self {
        Self(())
    }

    fn panic_message(_trace: &[Self]) -> Cow<'static, str> {
        Cow::Borrowed(BASE_MESSAGE)
    }
}

/// Use a full backtrace as tracing data
///
/// Capture the entire backtrace which may be expensive. This implementation does not force capture
/// in the event that backtraces are disabled at runtime, so the exact overhead can still be
/// controlled a little.
///
/// N.B. the [`Backtrace`] needs to be wrapped in an Arc as backtraces are not [`Clone`].
impl Reportable for MutexDep<Arc<Backtrace>> {
    fn capture() -> Self {
        Self(Arc::new(Backtrace::capture()))
    }

    fn panic_message(trace: &[Self]) -> Cow<'static, str> {
        let mut message = format!("{BASE_MESSAGE}\n");

        for entry in trace {
            let _ = writeln!(message, "{}", entry.0);
        }

        message.into()
    }
}
