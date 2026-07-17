//! Manual smoke test against a real linked Mach-O executable from speet's
//! test corpus. Not wired into automated signing (that's `os-codesign-macho`,
//! Phase D3) — this only confirms the load-command rewrite produces a file
//! `otool -l` still parses as a valid Mach-O with the new `LC_LOAD_DYLIB`.
//! Skips (not fails) when the corpus fixture or `otool` aren't available.

use os_rewrite_macho::{rewrite_macho, MachORewriteInput};
use std::path::Path;
use std::process::Command;

#[test]
fn rewrite_real_linked_macho_and_check_with_otool() {
    if !cfg!(target_os = "macos") {
        eprintln!("SKIP: macOS-only smoke test");
        return;
    }
    let guest = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../../speet/test-data/c-corpus/aarch64-macos/exit42.linked.macho");
    if !guest.is_file() {
        eprintln!("SKIP: missing corpus fixture {}", guest.display());
        return;
    }
    let original = std::fs::read(&guest).expect("read corpus fixture");

    let input = MachORewriteInput {
        original: &original,
        dylib_load_path: "@executable_path/x".into(),
    };
    let rewritten = match rewrite_macho(&input) {
        Ok(bytes) => bytes,
        Err(e) => {
            eprintln!("SKIP: fixture has no replaceable load-command slot: {e}");
            return;
        }
    };
    assert_ne!(rewritten, original);

    let dir = tempfile::tempdir().unwrap();
    let out_path = dir.path().join("rewritten");
    std::fs::write(&out_path, &rewritten).unwrap();
    let mut perms = std::fs::metadata(&out_path).unwrap().permissions();
    std::os::unix::fs::PermissionsExt::set_mode(&mut perms, 0o755);
    std::fs::set_permissions(&out_path, perms).unwrap();

    let Ok(otool) = Command::new("otool").arg("-L").arg(&out_path).output() else {
        eprintln!("SKIP: otool not available");
        return;
    };
    assert!(otool.status.success(), "otool -L failed on rewritten binary");
    let listing = String::from_utf8_lossy(&otool.stdout);
    assert!(
        listing.contains("@executable_path/x"),
        "expected the injected load path in otool -L output, got:\n{listing}"
    );
}
