//! RISC-V 64 binary execution tests using Unicorn.

mod harness;

use os_target_core::{Backend, MemWidth, OsOp};
use os_target_native::BinaryRiscv64Backend;
use portal_pc_asm_common::types::mem::MemorySize;
use portal_pc_asm_common::types::reg::Reg;
use portal_solutions_asm_riscv64::RiscV64Arch;
use portal_solutions_asm_riscv64::out::{
    Writer as RvWriter, WriterCore as RvWriterCore,
};
use portal_solutions_asm_riscv64::out::arg::{ArgKind, MemArgKind as RvMemArgKind};
use portal_solutions_asm_riscv64::RegisterClass;

const TEST_HEAP_WORD: u64 = 0xFEDC_BA98_7654_3210;

fn mem_a0_64() -> RvMemArgKind<ArgKind> {
    RvMemArgKind::Mem {
        base: ArgKind::Reg {
            reg: Reg(10),
            size: MemorySize::_64,
        },
        offset: None,
        disp: 0,
        size: MemorySize::_64,
        reg_class: RegisterClass::Gpr,
    }
}

/// Link in-image helper stubs after the backend's `finish()` so unresolved
/// `jal` references in the main program become direct calls to simple load/store
/// sequences.
fn link_helpers(b: &mut BinaryRiscv64Backend) {
    let arch = RiscV64Arch::default();
    let a0 = Reg(10);
    let a1 = Reg(11);

    // os_load_u64: ld a0, 0(a0); ret
    let _ = RvWriter::set_label(&mut b.writer, &mut (), arch, "os_load_u64");
    let _ = RvWriterCore::ld(&mut b.writer, &mut (), arch, &a0, &mem_a0_64());
    let _ = RvWriterCore::ret(&mut b.writer, &mut (), arch);

    // os_store_u64: sd a1, 0(a0); ret
    let _ = RvWriter::set_label(&mut b.writer, &mut (), arch, "os_store_u64");
    let _ = RvWriterCore::sd(&mut b.writer, &mut (), arch, &a1, &mem_a0_64());
    let _ = RvWriterCore::ret(&mut b.writer, &mut (), arch);
}

#[test]
fn binary_push_u64_then_pop_returns_value() {
    let mut b = BinaryRiscv64Backend::new_binary();
    b.op(OsOp::PushU64(0x1234_5678_9abc_def0));
    b.op(OsOp::Pop);
    b.finish();
    let code = b.into_bytes();
    assert!(!code.is_empty());

    let (a0, _) = harness::run_riscv64(&code, 0).expect("unicorn run should succeed");
    assert_eq!(a0, 0x1234_5678_9abc_def0);
}

#[test]
fn binary_push_u32_then_pop_sign_extends() {
    let mut b = BinaryRiscv64Backend::new_binary();
    b.op(OsOp::PushU32(0xcafe_babe));
    b.op(OsOp::Pop);
    b.finish();
    let code = b.into_bytes();

    let (a0, _) = harness::run_riscv64(&code, 0).expect("unicorn run should succeed");
    assert_eq!(a0, 0xcafe_babe_u32 as i32 as u64);
}

#[test]
fn binary_trap_triggers_breakpoint() {
    let mut b = BinaryRiscv64Backend::new_binary();
    b.op(OsOp::Trap);
    let code = b.into_bytes();
    assert!(!code.is_empty());
    assert!(harness::run_riscv64(&code, 0).is_err());
}

#[test]
fn binary_load_u64_returns_heap_word() {
    let mut b = BinaryRiscv64Backend::new_binary();
    b.op(OsOp::PushU64(harness::HEAP_BASE));
    b.op(OsOp::Load {
        width: MemWidth::W64,
        signed: false,
    });
    b.finish();
    link_helpers(&mut b);

    let code = b.into_bytes();
    let (a0, _) = harness::run_riscv64(&code, TEST_HEAP_WORD).expect("unicorn run should succeed");
    assert_eq!(a0, TEST_HEAP_WORD);
}

#[test]
fn binary_store_u64_writes_heap_word() {
    let expected = 0xCAFE_F00D_DEAD_BEEF_u64;

    let mut b = BinaryRiscv64Backend::new_binary();
    b.op(OsOp::PushU64(expected));
    b.op(OsOp::PushU64(harness::HEAP_BASE));
    b.op(OsOp::Store {
        width: MemWidth::W64,
    });
    b.finish();
    link_helpers(&mut b);

    let code = b.into_bytes();
    let (_a0, heap) =
        harness::run_riscv64(&code, !expected).expect("unicorn run should succeed");
    assert_eq!(heap, expected);
}