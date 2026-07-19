//! AArch64 binary execution tests using Unicorn.

mod harness;

use os_target_core::{Backend, MemWidth, OsOp};
use os_target_native::BinaryAArch64SysVBackend;
use portal_pc_asm_common::types::mem::MemorySize;
use portal_pc_asm_common::types::reg::Reg;
use portal_solutions_asm_aarch64::AArch64Arch;
use portal_solutions_asm_aarch64::out::{
    Writer as A64Writer, WriterCore as A64WriterCore,
};
use portal_solutions_asm_aarch64::out::arg::{AddressingMode, ArgKind, MemArgKind as A64MemArgKind};
use portal_solutions_asm_aarch64::RegisterClass;

const TEST_HEAP_WORD: u64 = 0xFEDC_BA98_7654_3210;

fn mem_x0_64(size: MemorySize) -> A64MemArgKind<ArgKind> {
    A64MemArgKind::Mem {
        base: ArgKind::Reg {
            reg: Reg(0),
            size: MemorySize::_64,
        },
        offset: None,
        disp: 0,
        size,
        reg_class: RegisterClass::Gpr,
        mode: AddressingMode::Offset,
    }
}

/// Link in-image helper stubs after the backend's `finish()` so unresolved
/// `bl` references in the main program become direct calls to simple load/store
/// sequences.
fn link_helpers(b: &mut BinaryAArch64SysVBackend) {
    let arch = AArch64Arch::default();
    let x0 = Reg(0);
    let x1 = Reg(1);

    // os_load_u64: ldr x0, [x0]; ret
    let _ = A64Writer::set_label(&mut b.writer, &mut (), arch, "os_load_u64");
    let _ = A64WriterCore::ldr(&mut b.writer, &mut (), arch, &x0, &mem_x0_64(MemorySize::_64));
    let _ = A64WriterCore::ret(&mut b.writer, &mut (), arch);

    // os_store_u64: str x1, [x0]; ret
    let _ = A64Writer::set_label(&mut b.writer, &mut (), arch, "os_store_u64");
    let _ = A64WriterCore::str(&mut b.writer, &mut (), arch, &x1, &mem_x0_64(MemorySize::_64));
    let _ = A64WriterCore::ret(&mut b.writer, &mut (), arch);
}

#[test]
fn binary_push_u64_then_pop_returns_value() {
    let mut b = BinaryAArch64SysVBackend::new_binary(0);
    b.op(OsOp::PushU64(0x1234_5678_9abc_def0));
    b.op(OsOp::Pop);
    b.finish();
    let code = b.into_bytes();
    assert!(!code.is_empty());

    let (x0, _) = harness::run_aarch64(&code, 0).expect("unicorn run should succeed");
    assert_eq!(x0, 0x1234_5678_9abc_def0);
}

#[test]
fn binary_push_u32_then_pop_sign_extends() {
    let mut b = BinaryAArch64SysVBackend::new_binary(0);
    b.op(OsOp::PushU32(0xcafe_babe));
    b.op(OsOp::Pop);
    b.finish();
    let code = b.into_bytes();

    let (x0, _) = harness::run_aarch64(&code, 0).expect("unicorn run should succeed");
    assert_eq!(x0, 0xcafe_babe_u32 as i32 as u64);
}

#[test]
fn binary_trap_triggers_breakpoint() {
    let mut b = BinaryAArch64SysVBackend::new_binary(0);
    b.op(OsOp::Trap);
    let code = b.into_bytes();
    assert!(!code.is_empty());
    assert!(harness::run_aarch64(&code, 0).is_err());
}

#[test]
fn binary_load_u64_returns_heap_word() {
    let mut b = BinaryAArch64SysVBackend::new_binary(0);
    b.op(OsOp::PushU64(harness::HEAP_BASE));
    b.op(OsOp::Load {
        width: MemWidth::W64,
        signed: false,
    });
    b.finish();
    link_helpers(&mut b);

    let code = b.into_bytes();
    let (x0, _) = harness::run_aarch64(&code, TEST_HEAP_WORD).expect("unicorn run should succeed");
    assert_eq!(x0, TEST_HEAP_WORD);
}

#[test]
fn binary_store_u64_writes_heap_word() {
    let expected = 0xCAFE_F00D_DEAD_BEEF_u64;

    let mut b = BinaryAArch64SysVBackend::new_binary(0);
    b.op(OsOp::PushU64(expected));
    b.op(OsOp::PushU64(harness::HEAP_BASE));
    b.op(OsOp::Store {
        width: MemWidth::W64,
    });
    b.finish();
    link_helpers(&mut b);

    let code = b.into_bytes();
    let (_x0, heap) =
        harness::run_aarch64(&code, !expected).expect("unicorn run should succeed");
    assert_eq!(heap, expected);
}