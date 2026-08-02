//! Daemon plugin: strong `os_shim_execve` override (optional link).

mod execve;

pub use execve::{DaemonExecveHandler, DEFAULT_BACKEND_ID, DEFAULT_SOCKET_ENV};

/// Register daemon overrides on a [`os_shim_handler::ShimRegistry`].
pub fn register_daemon_overrides(registry: &mut os_shim_handler::ShimRegistry) {
    registry.register_override(Box::new(DaemonExecveHandler::default()));
}
