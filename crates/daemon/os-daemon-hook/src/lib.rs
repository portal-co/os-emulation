//! Generator for the minimal C execve-interposition stub linked into
//! guest/target binaries.
//!
//! Delegates the daemon consult body to [`os_daemon_shim`] (`os_shim_execve`).

use os_abi_spec::{AbiArg, AbiFunction, AbiValueKind};
use os_shim_handler::ShimHandler;

/// Emit C source implementing `int <hook_symbol_name>(path, argv, envp)`
/// that consults the daemon over the v2 wire protocol, then calls `execve()`.
///
/// Prefer linking [`os_shim_execve`] from `os-shim-core` + optional daemon
/// plugin directly; this wrapper remains for legacy `__speet_execve_hook` call sites.
pub fn generate_execve_hook_c(hook_symbol_name: &str, backend_id: &str, socket_env_var: &str) -> String {
    let handler = os_daemon_shim::DaemonExecveHandler {
        backend_id: backend_id.to_string(),
        socket_env_var: socket_env_var.to_string(),
    };
    let body = handler.emit_core(&execve_abi_function());
    body.replace("os_shim_execve", hook_symbol_name)
}

fn execve_abi_function() -> AbiFunction {
    AbiFunction {
        name: "execve".into(),
        args: vec![
            AbiArg {
                kind: AbiValueKind::Pointer,
                bridgesupport_type: Some("*".into()),
                function_pointer: false,
                pointer: true,
            },
            AbiArg {
                kind: AbiValueKind::Pointer,
                bridgesupport_type: Some("^*".into()),
                function_pointer: false,
                pointer: true,
            },
            AbiArg {
                kind: AbiValueKind::Pointer,
                bridgesupport_type: Some("^*".into()),
                function_pointer: false,
                pointer: true,
            },
        ],
        retval: AbiArg {
            kind: AbiValueKind::Scalar,
            bridgesupport_type: Some("i".into()),
            function_pointer: false,
            pointer: false,
        },
        variadic: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embeds_the_requested_symbol_name_and_backend_id() {
        let src = generate_execve_hook_c("__speet_execve_hook", "integrated", "SPEET_RTD_SOCK");
        assert!(src.contains("__speet_execve_hook"));
        assert!(src.contains("\"integrated\""));
        assert!(src.contains("SPEET_RTD_SOCK"));
    }

    #[test]
    fn different_backend_ids_produce_different_source() {
        let a = generate_execve_hook_c("__os_execve_hook", "simple-rewrite", "SOEL_DAEMON_SOCK");
        assert!(a.contains("\"simple-rewrite\""));
        assert!(!a.contains("__speet_execve_hook"));
    }
}
