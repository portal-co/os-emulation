//! Pluggable handler registry for shared `os_shim_*` C implementations.
//!
//! BridgeSupport provides weak default forwards; host plugins (e.g. the daemon
//! for `execve`) register strong overrides without coupling the default core.

extern crate alloc;

mod c_emit;
mod forward;
mod registry;

pub use c_emit::{c_host_symbol, c_param_type, c_return_type, os_shim_name};
pub use forward::{BridgeSupportForward, BridgeSupportForwardHandler, emit_weak_forward};
pub use registry::{ShimHandler, ShimRegistry};
