//! Cycle reporting primitives
//!
//! This module exposes [`Dep`], which resolves to either something that tracks dependencies or to
//! something that doesn't.  It should only be assumed to implement the [`Reportable`] trait.
use std::backtrace::Backtrace;
use std::borrow::Cow;
use std::fmt::Write;
use std::sync::Arc;
use std::sync::atomic::AtomicU8;
use std::sync::atomic::Ordering;

#[cfg(feature = "backtraces")]
pub type Dep = MutexDep<Arc<Backtrace>>;
#[cfg(not(feature = "backtraces"))]
pub type Dep = MutexDep<()>;

// Base message to be reported when cycle is detected
const BASE_MESSAGE: &str = "Found cycle in mutex dependency graph:";

/// Action to take when a cycle is detected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum PanicAction {
    /// Panic when a cycle is detected.
    Panic = 0,
    /// Log the cycle to stderr but do not panic.
    #[cfg(feature = "experimental")]
    Log = 1,
}

static PANIC_ACTION: AtomicU8 = AtomicU8::new(0);

/// Set the action to take when a cycle is detected.
///
/// This is useful for incrementally adopting tracing-mutex to a large codebase compiled with
/// `panic=abort`, as it allows you to continue running your program even when a cycle is detected.
#[cfg(feature = "experimental")]
pub fn set_panic_action(action: PanicAction) {
    PANIC_ACTION.store(action as u8, Ordering::Relaxed);
}

pub(crate) fn report_cycle(cycle: &[Dep]) {
    let message = Dep::message(cycle);
    let action = PANIC_ACTION.load(Ordering::Relaxed);
    if action == PanicAction::Panic as u8 {
        panic!("{message}");
    } else {
        eprintln!("{message}");
    }
}

pub trait Reportable: Clone {
    /// Capture the current state
    fn capture() -> Self;

    /// Format a trace of state for human readable consumption.
    fn message(trace: &[Self]) -> Cow<'static, str>;
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

    fn message(_trace: &[Self]) -> Cow<'static, str> {
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

    fn message(trace: &[Self]) -> Cow<'static, str> {
        let mut message = format!("{BASE_MESSAGE}\n");

        for entry in trace {
            let _ = writeln!(message, "{}", entry.0);
        }

        message.into()
    }
}
