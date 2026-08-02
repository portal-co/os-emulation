//! Paths and helpers for the shared C shim core.

use std::path::{Path, PathBuf};

/// Symbols with checked-in weak `os_shim_*` core implementations.
pub const CORE_SYMBOLS: &[&str] = &[
    "write", "exit", "printf", "putchar", "strlen", "getenv", "execve",
];

/// Whether a shared core implementation exists for `symbol`.
pub fn has_core_impl(symbol: &str) -> bool {
    let bare = symbol.strip_prefix('_').unwrap_or(symbol);
    CORE_SYMBOLS.contains(&bare)
}
/// Directory containing checked-in generated `os_shim_*` C sources.
pub fn generated_core_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/generated")
}

/// Header directory (`os_shim.h`).
pub fn include_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("include")
}

/// Checked-in weak-forward core sources (always linked).
pub fn core_source_paths() -> Vec<PathBuf> {
    let dir = generated_core_dir();
    [
        "write.c",
        "exit.c",
        "printf.c",
        "putchar.c",
        "strlen.c",
        "getenv.c",
        "execve.c",
    ]
    .into_iter()
    .map(|f| dir.join(f))
    .collect()
}

/// Emit daemon strong override source for `os_shim_execve` (compile separately).
#[cfg(feature = "daemon")]
pub fn emit_daemon_execve_c() -> String {
    use os_shim_handler::ShimHandler;
    os_daemon_shim::DaemonExecveHandler::default().emit_core(&execve_abi_function())
}

#[cfg(not(feature = "daemon"))]
pub fn emit_daemon_execve_c() -> String {
    String::new()
}

#[cfg(feature = "daemon")]
fn execve_abi_function() -> os_abi_spec::AbiFunction {
    use os_abi_spec::{AbiArg, AbiFunction, AbiValueKind};
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

/// Write `execve_daemon.c` under `work_dir` when daemon feature is enabled.
pub fn write_daemon_execve_override(work_dir: &Path) -> Result<Option<PathBuf>, String> {
    #[cfg(feature = "daemon")]
    {
        let src = emit_daemon_execve_c();
        if src.is_empty() {
            return Ok(None);
        }
        let path = work_dir.join("execve_daemon.c");
        std::fs::write(&path, src).map_err(|e| e.to_string())?;
        return Ok(Some(path));
    }
    #[cfg(not(feature = "daemon"))]
    {
        let _ = work_dir;
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn core_sources_exist() {
        for p in core_source_paths() {
            assert!(p.exists(), "missing {}", p.display());
        }
    }
}
