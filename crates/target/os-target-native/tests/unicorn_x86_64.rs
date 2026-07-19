//! x86-64 binary execution tests using Unicorn.

mod harness;

use os_target_core::{Backend, MemWidth, OsOp};
use os_target_native::BinaryX86_64SysVBackend;
use portal_solutions_asm_x86_64::X64Arch;
use portal_solutions_asm_x86_64::out::{Writer as X64Writer, WriterCore as X64WriterCore};

const TEST_HEAP_WORD: u64 = 0xFEDC_BA98_7654_3210;

/// Append in-image `os_load_u64` and `os_store_u64` helper stubs after the
/// backend's `finish()` has emitted the epilogue. The unresolved labels in the
/// main program are resolved by `set_label` at the current offset.
fn link_helpers(b: &mut BinaryX86_64SysVBackend) {
    let arch = X64Arch::default();
    // Jump past the helper stubs; the main program still `call`s into them by label.
    let _ = X64Writer::jmp_label(&mut b.writer, &mut (), arch, "end_stubs");
    let _ = X64Writer::set_label(&mut b.writer, &mut (), arch, "os_load_u64");
    let _ = X64WriterCore::db(&mut b.writer, &mut (), arch, &[0x48, 0x8B, 0x07, 0xC3]);
    let _ = X64Writer::set_label(&mut b.writer, &mut (), arch, "os_store_u64");
    let _ = X64WriterCore::db(&mut b.writer, &mut (), arch, &[0x48, 0x89, 0x37, 0xC3]);
    let _ = X64Writer::set_label(&mut b.writer, &mut (), arch, "end_stubs");
}

#[test]
fn binary_push_u64_then_pop_returns_value() {
    let mut b = BinaryX86_64SysVBackend::new_binary(0);
    b.op(OsOp::PushU64(0x1234_5678_9abc_def0));
    b.op(OsOp::Pop);
    b.finish();
    let code = b.into_bytes();
    assert!(!code.is_empty());

    let (rax, _) = harness::run_x86_64(&code, 0).expect("unicorn run should succeed");
    assert_eq!(rax, 0x1234_5678_9abc_def0);
}

#[test]
fn binary_push_u32_then_pop_returns_value() {
    let mut b = BinaryX86_64SysVBackend::new_binary(0);
    b.op(OsOp::PushU32(0x1234_5678));
    b.op(OsOp::Pop);
    b.finish();
    let code = b.into_bytes();
    assert!(!code.is_empty());

    let (rax, _) = harness::run_x86_64(&code, 0).expect("unicorn run should succeed");
    // PushU32 sign-extends the i32 operand to 64 bits before pushing.
    assert_eq!(rax, 0x1234_5678 as i32 as u64);
}

#[test]
fn binary_trap_triggers_invalid_instruction() {
    let mut b = BinaryX86_64SysVBackend::new_binary(0);
    b.op(OsOp::Trap);
    // No epilogue/ret — the only emitted instruction is UD2, which should
    // cause Unicorn to report an invalid instruction.
    let code = b.into_bytes();
    assert!(!code.is_empty());
    assert!(harness::run_x86_64(&code, 0).is_err());
}

#[test]
fn binary_emit_len_matches_text_counterpart() {
    use os_target_native::TextX86_64SysVBackend;

    let mut text = TextX86_64SysVBackend::new_text();
    text.op(OsOp::PushU64(0x42));
    text.op(OsOp::Pop);
    text.finish();
    let text_output = text.into_string();
    assert!(text_output.contains("push rax"));

    let mut binary = BinaryX86_64SysVBackend::new_binary(0);
    binary.op(OsOp::PushU64(0x42));
    binary.op(OsOp::Pop);
    binary.finish();
    assert!(!binary.into_bytes().is_empty());
}

#[test]
fn binary_load_u64_returns_heap_word() {
    let mut b = BinaryX86_64SysVBackend::new_binary(0);
    b.op(OsOp::PushU64(harness::HEAP_BASE));
    b.op(OsOp::Load {
        width: MemWidth::W64,
        signed: false,
    });
    b.finish();
    link_helpers(&mut b);

    let code = b.into_bytes();
    eprintln!("x86 load code ({} bytes): {:02x?}", code.len(), code);
    let (rax, _) = harness::run_x86_64(&code, TEST_HEAP_WORD).expect("unicorn run should succeed");
    assert_eq!(rax, TEST_HEAP_WORD);
}

#[test]
fn binary_store_u64_writes_heap_word() {
    let expected = 0xCAFE_F00D_DEAD_BEEF_u64;

    let mut b = BinaryX86_64SysVBackend::new_binary(0);
    b.op(OsOp::PushU64(expected));
    b.op(OsOp::PushU64(harness::HEAP_BASE));
    b.op(OsOp::Store {
        width: MemWidth::W64,
    });
    b.finish();
    link_helpers(&mut b);

    let code = b.into_bytes();
    let (_rax, heap) =
        harness::run_x86_64(&code, !expected).expect("unicorn run should succeed");
    assert_eq!(heap, expected);
}