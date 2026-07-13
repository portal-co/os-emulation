//! RISC-V 64 binary execution tests using Unicorn.

mod harness;

use os_target_core::{Backend, OsOp};
use os_target_native::BinaryRiscv64Backend;

#[test]
fn binary_push_u64_then_pop_returns_value() {
    let mut b = BinaryRiscv64Backend::new_binary();
    b.op(OsOp::PushU64(0x1234_5678_9abc_def0));
    b.op(OsOp::Pop);
    b.finish();
    let code = b.into_bytes();
    assert!(!code.is_empty());

    let a0 = harness::run_riscv64(&code).expect("unicorn run should succeed");
    assert_eq!(a0, 0x1234_5678_9abc_def0);
}

#[test]
fn binary_push_u32_then_pop_sign_extends() {
    let mut b = BinaryRiscv64Backend::new_binary();
    b.op(OsOp::PushU32(0xcafe_babe));
    b.op(OsOp::Pop);
    b.finish();
    let code = b.into_bytes();

    let a0 = harness::run_riscv64(&code).expect("unicorn run should succeed");
    assert_eq!(a0, 0xcafe_babe_u32 as i32 as u64);
}

#[test]
fn binary_trap_triggers_breakpoint() {
    let mut b = BinaryRiscv64Backend::new_binary();
    b.op(OsOp::Trap);
    let code = b.into_bytes();
    assert!(!code.is_empty());
    assert!(harness::run_riscv64(&code).is_err());
}