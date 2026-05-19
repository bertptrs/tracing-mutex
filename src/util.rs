//! Utilities related to the internals of dependency tracking.
use crate::MutexId;
#[cfg(feature = "experimental")]
#[cfg(feature = "experimental")]
use crate::reporting::cycle_handler_storage;

/// Reset the dependencies for the given entity.
///
/// # Performance
///
/// This function locks the dependency graph to remove the item from it. This is an `O(E)` operation
/// with `E` being the number of dependencies directly associated with this particular instance. As
/// such, it is not advisable to call this method from a hot loop.
///
/// # Safety
///
/// Use of this method invalidates the deadlock prevention guarantees that this library makes. As
/// such, it should only be used when it is absolutely certain this will not introduce deadlocks
/// later.
///
/// Other than deadlocks, no undefined behaviour can result from the use of this function.
///
/// # Example
///
/// ```
/// use tracing_mutex::stdsync::Mutex;
/// use tracing_mutex::util;
///
/// let first = Mutex::new(());
/// let second = Mutex::new(());
///
/// {
///     let _first_lock = first.lock().unwrap();
///     second.lock().unwrap();
/// }
///
/// // Reset the dependencies for the first mutex
/// unsafe { util::reset_dependencies(&first) };
///
/// // Now we can unlock the mutexes in the opposite order without a panic.
/// let _second_lock = second.lock().unwrap();
/// first.lock().unwrap();
/// ```
#[cfg(feature = "experimental")]
#[cfg_attr(docsrs, doc(cfg(feature = "experimental")))]
pub unsafe fn reset_dependencies<T: Traced>(traced: &T) {
    crate::get_dependency_graph().remove_node(traced.get_id().value());
}

/// Install a new cycle handler
///
/// This function will be called whenever a cycle in the dependency graph is detected. The default
/// handler will panic with a message describing the panic, but this can be downgraded to logging,
/// or even some complicated reporting mechanism.
///
/// ```
/// # use tracing_mutex::util::set_cycle_handler;
/// let mut logged = false;
///
/// set_cycle_handler(move |message| {
///     if !logged {
///         logged = true;
///         eprintln!("{message}");
///     }
/// });
/// ```
///
/// This handler can be reset to its original state via [`reset_cycle_handler`].
#[cfg(feature = "experimental")]
#[cfg_attr(docsrs, doc(cfg(feature = "experimental")))]
pub fn set_cycle_handler<T>(handler: T)
where
    T: FnMut(&str) + Send + Sync + 'static,
{
    *cycle_handler_storage() = Some(Box::new(handler))
}

/// Reset cycle handler to its original state
///
/// This function returns `true` if a custom handler was previously installed, and `false`
/// otherwise.
#[cfg(feature = "experimental")]
#[cfg_attr(docsrs, doc(cfg(feature = "experimental")))]
pub fn reset_cycle_handler() -> bool {
    cycle_handler_storage().take().is_some()
}

/// Types that participate in dependency tracking
///
/// This trait is a public marker trait and is automatically implemented fore all types that
/// implement the internal dependency tracking features.
#[cfg(feature = "experimental")]
#[cfg_attr(docsrs, doc(cfg(feature = "experimental")))]
#[allow(private_bounds)]
pub trait Traced: PrivateTraced {}

#[cfg(feature = "experimental")]
#[cfg_attr(docsrs, doc(cfg(feature = "experimental")))]
impl<T: PrivateTraced> Traced for T {}

/// Private implementation of the traced marker.
///
/// This trait is private (and seals the outer trait) to avoid exposing the MutexId type.
#[cfg_attr(not(feature = "experimental"), allow(unused))]
pub(crate) trait PrivateTraced {
    /// Get the mutex id associated with this traced item.
    fn get_id(&self) -> &MutexId;
}
