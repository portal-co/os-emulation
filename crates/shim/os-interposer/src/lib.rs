//! Interposer dylib artifact paths and configuration.

use std::path::{Path, PathBuf};

/// Basename of the built interposer shared library (without extension).
pub const LIB_BASENAME: &str = "libos_interposer";

/// Directory where `build.rs` places `libos_interposer.{so,dylib}`.
pub fn build_out_dir() -> PathBuf {
    PathBuf::from(env!("OS_INTERPOSER_OUT_DIR"))
}

/// Path to the interposer shared library produced by this crate's build script.
pub fn lib_path() -> PathBuf {
    let dir = build_out_dir();
    let ext = if cfg!(target_os = "macos") { "dylib" } else { "so" };
    dir.join(format!("{LIB_BASENAME}.{ext}"))
}

/// Copy the built interposer to `dest` if it exists.
pub fn copy_artifact_to(dest: &Path) -> Result<(), String> {
    let src = lib_path();
    if !src.exists() {
        return Err(format!("interposer artifact missing: {}", src.display()));
    }
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    std::fs::copy(&src, dest)
        .map(|_| ())
        .map_err(|e| format!("copy {} -> {}: {e}", src.display(), dest.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lib_basename_is_stable() {
        assert_eq!(LIB_BASENAME, "libos_interposer");
    }
}
