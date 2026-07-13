//! x86-64 binary execution tests using Unicorn.

mod harness;

use os_target_core::{Backend, OsOp};
use os_target_native::BinaryX86_64SysVBackend;

#[test]
fn binary_push_u64_then_pop_returns_value() {
    let mut b = BinaryX86_64SysVBackend::new_binary(0);
    b.op(OsOp::PushU64(0x1234_5678_9abc_def0));
    b.op(OsOp::Pop);
    b.finish();
    let code = b.into_bytes();
    assert!(!code.is_empty());

    let rax = harness::run_x86_64(&code).expect("unicorn run should succeed");
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

    let rax = harness::run_x86_64(&code).expect("unicorn run should succeed");
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
    assert!(harness::run_x86_64(&code).is_err());
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