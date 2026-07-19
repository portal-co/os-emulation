//! RISC-V 64 backend for the shared `OsOp` stack-machine IR.

extern crate alloc;

use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;
use core::fmt::Display;
use core::marker::PhantomData;

use os_target_core::{Backend, MemWidth, OsOp};
use portal_pc_asm_common::types::mem::MemorySize;
use portal_pc_asm_common::types::reg::Reg;
use portal_solutions_asm_riscv64::out::arg::{ArgKind, MemArgKind as RvMemArgKind};

use crate::NativeHelpers;
use portal_solutions_asm_riscv64::out::rv_asm_backend::RvAsmWriter;
use portal_solutions_asm_riscv64::out::{Writer as RvWriter, WriterCore as RvWriterCore};
use portal_solutions_asm_riscv64::{RegisterClass, RiscV64Arch};

pub type Label = &'static str;

/// RISC-V 64 backend for OsOp.
///
/// Generic over any `asm-riscv64` writer, intended to be used with the binary
/// `RvAsmWriter` for machine-code generation.
///
/// Register usage:
/// - A0 (X10) scratch/return value/first arg.
/// - A1 (X11) second arg.
/// - SP (X2) operand stack.
/// - RA (X1) and FP (X8) saved by the generated prologue.
pub struct Riscv64Backend<W, L = Label>
where
    W: RvWriterCore<()> + RvWriter<L, ()>,
{
    cfg: Riscv64Config,
    pub writer: W,
    _label: PhantomData<L>,
}

pub type BinaryRiscv64Backend = Riscv64Backend<RvAsmWriter<Label>, Label>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Riscv64Config {
    pub emit_frame: bool,
    pub helpers: NativeHelpers,
}

impl Default for Riscv64Config {
    fn default() -> Self {
        Self {
            emit_frame: true,
            helpers: NativeHelpers::DEFAULTS,
        }
    }
}

impl BinaryRiscv64Backend {
    /// Create a binary backend.
    pub fn new_binary() -> Self {
        Self::with_config_and_writer(Riscv64Config::default(), RvAsmWriter::<Label>::new())
    }

    /// Take the emitted machine code bytes.
    pub fn into_bytes(self) -> Vec<u8> {
        self.writer.into_bytes()
    }
}

impl<W, L> Riscv64Backend<W, L>
where
    W: RvWriterCore<()> + RvWriter<L, ()>,
    L: Display + Ord + Clone + From<&'static str>,
{
    pub fn with_config_and_writer(cfg: Riscv64Config, writer: W) -> Self {
        let mut s = Self {
            cfg,
            writer,
            _label: PhantomData,
        };
        s.emit_prologue();
        s
    }

    fn arch() -> RiscV64Arch {
        RiscV64Arch::default()
    }

    fn zero(&self) -> Reg {
        Reg(0)
    }
    fn ra(&self) -> Reg {
        Reg(1)
    }
    fn sp(&self) -> Reg {
        Reg(2)
    }
    fn fp(&self) -> Reg {
        Reg(8)
    }
    fn a0(&self) -> Reg {
        Reg(10)
    }
    fn a1(&self) -> Reg {
        Reg(11)
    }

    fn emit_prologue(&mut self) {
        if self.cfg.emit_frame {
            let arch = Self::arch();
            let sp = self.sp();
            let ra = self.ra();
            let fp = self.fp();
            // addi sp, sp, -16
            let _ = RvWriterCore::addi(&mut self.writer, &mut (), arch, &sp, &sp, -16);
            // sd ra, 0(sp)
            let _ = RvWriterCore::sd(&mut self.writer, &mut (), arch, &ra, &sp_mem(0));
            // sd fp, 8(sp)
            let _ = RvWriterCore::sd(&mut self.writer, &mut (), arch, &fp, &sp_mem(8));
            // mv fp, sp
            let _ = RvWriterCore::mv(&mut self.writer, &mut (), arch, &fp, &sp);
        }
    }

    fn emit_epilogue(&mut self) {
        if self.cfg.emit_frame {
            let arch = Self::arch();
            let sp = self.sp();
            let ra = self.ra();
            let fp = self.fp();
            // Realign SP to the saved frame record before restoring it.
            // mv sp, fp
            let _ = RvWriterCore::mv(&mut self.writer, &mut (), arch, &sp, &fp);
            // ld fp, 8(sp)
            let _ = RvWriterCore::ld(&mut self.writer, &mut (), arch, &fp, &sp_mem(8));
            // ld ra, 0(sp)
            let _ = RvWriterCore::ld(&mut self.writer, &mut (), arch, &ra, &sp_mem(0));
            // addi sp, sp, 16
            let _ = RvWriterCore::addi(&mut self.writer, &mut (), arch, &sp, &sp, 16);
            // ret -> jalr zero, ra, 0
            let _ = RvWriterCore::ret(&mut self.writer, &mut (), arch);
        }
    }

    fn load_label(&self, width: MemWidth, signed: bool) -> L {
        let name = match (width, signed) {
            (MemWidth::W8, false) => self.cfg.helpers.load_u8,
            (MemWidth::W8, true) => self.cfg.helpers.load_i8,
            (MemWidth::W16, false) => self.cfg.helpers.load_u16,
            (MemWidth::W16, true) => self.cfg.helpers.load_i16,
            (MemWidth::W32, false) => self.cfg.helpers.load_u32,
            (MemWidth::W32, true) => self.cfg.helpers.load_i32,
            (MemWidth::W64, false) => self.cfg.helpers.load_u64,
            (MemWidth::W64, true) => self.cfg.helpers.load_u64,
            (MemWidth::W128, _) => "os_load_u128",
        };
        L::from(name)
    }

    fn store_label(&self, width: MemWidth) -> L {
        let name = match width {
            MemWidth::W8 => self.cfg.helpers.store_u8,
            MemWidth::W16 => self.cfg.helpers.store_u16,
            MemWidth::W32 => self.cfg.helpers.store_u32,
            MemWidth::W64 => self.cfg.helpers.store_u64,
            MemWidth::W128 => "os_store_u128",
        };
        L::from(name)
    }

    fn ecall_label(&self) -> L {
        L::from(self.cfg.helpers.ecall)
    }

    fn leak_label(&self, s: String) -> L {
        L::from(Box::leak(s.into_boxed_str()))
    }
}

impl<W, L> os_target_core::NativeBackend for Riscv64Backend<W, L>
where
    W: RvWriterCore<()> + RvWriter<L, ()>,
    L: Display + Ord + Clone + From<&'static str>,
{
}

impl<W, L> Backend for Riscv64Backend<W, L>
where
    W: RvWriterCore<()> + RvWriter<L, ()>,
    L: Display + Ord + Clone + From<&'static str>,
{
    fn op(&mut self, op: OsOp) {
        let arch = Self::arch();
        let a0 = self.a0();
        let a1 = self.a1();
        let sp = self.sp();
        let zero = self.zero();

        match op {
            OsOp::PushU64(v) => {
                let _ = RvWriterCore::li(&mut self.writer, &mut (), arch, &a0, v);
                let _ = RvWriterCore::addi(&mut self.writer, &mut (), arch, &sp, &sp, -8);
                let _ = RvWriterCore::sd(&mut self.writer, &mut (), arch, &a0, &sp_mem(0));
            }
            OsOp::PushU32(v) => {
                let v = (v as i32) as u64; // sign-extend to 64 bits.
                let _ = RvWriterCore::li(&mut self.writer, &mut (), arch, &a0, v);
                let _ = RvWriterCore::addi(&mut self.writer, &mut (), arch, &sp, &sp, -8);
                let _ = RvWriterCore::sd(&mut self.writer, &mut (), arch, &a0, &sp_mem(0));
            }
            OsOp::Pop => {
                let _ = RvWriterCore::ld(&mut self.writer, &mut (), arch, &a0, &sp_mem(0));
                let _ = RvWriterCore::addi(&mut self.writer, &mut (), arch, &sp, &sp, 8);
            }
            OsOp::Load { width, signed } => {
                // Pop address into a0, call helper, push result.
                let ra = self.ra();
                let label = self.load_label(width, signed);
                let _ = RvWriterCore::ld(&mut self.writer, &mut (), arch, &a0, &sp_mem(0));
                let _ = RvWriterCore::addi(&mut self.writer, &mut (), arch, &sp, &sp, 8);
                let _ = RvWriter::jal_label(&mut self.writer, &mut (), arch, &ra, label);
                let _ = RvWriterCore::addi(&mut self.writer, &mut (), arch, &sp, &sp, -8);
                let _ = RvWriterCore::sd(&mut self.writer, &mut (), arch, &a0, &sp_mem(0));
            }
            OsOp::Store { width } => {
                // Pop address into a0, value into a1.
                let ra = self.ra();
                let label = self.store_label(width);
                let _ = RvWriterCore::ld(&mut self.writer, &mut (), arch, &a0, &sp_mem(0));
                let _ = RvWriterCore::addi(&mut self.writer, &mut (), arch, &sp, &sp, 8);
                let _ = RvWriterCore::ld(&mut self.writer, &mut (), arch, &a1, &sp_mem(0));
                let _ = RvWriterCore::addi(&mut self.writer, &mut (), arch, &sp, &sp, 8);
                let _ = RvWriter::jal_label(&mut self.writer, &mut (), arch, &ra, label);
            }
            OsOp::Ecall { .. } => {
                let ra = self.ra();
                let label = self.ecall_label();
                let _ = RvWriter::jal_label(&mut self.writer, &mut (), arch, &ra, label);
            }
            OsOp::Jump { .. } => {
                let _ = RvWriterCore::ld(&mut self.writer, &mut (), arch, &a0, &sp_mem(0));
                let _ = RvWriterCore::addi(&mut self.writer, &mut (), arch, &sp, &sp, 8);
                let _ = RvWriterCore::jalr(&mut self.writer, &mut (), arch, &zero, &a0, 0);
            }
            OsOp::TailCall { helper } => {
                let label = self.leak_label(helper);
                let _ = RvWriter::jal_label(&mut self.writer, &mut (), arch, &zero, label);
            }
            OsOp::Trap => {
                let _ = RvWriterCore::ebreak(&mut self.writer, &mut (), arch);
            }
        }
    }

    fn finish(&mut self) {
        self.emit_epilogue();
    }
}

fn sp_mem(disp: i32) -> RvMemArgKind<ArgKind> {
    mem(Reg(2), disp)
}

fn mem(base: Reg, disp: i32) -> RvMemArgKind<ArgKind> {
    RvMemArgKind::Mem {
        base: ArgKind::Reg {
            reg: base,
            size: MemorySize::_64,
        },
        offset: None,
        disp,
        size: MemorySize::_64,
        reg_class: RegisterClass::Gpr,
    }
}

#[cfg(test)]
mod tests {
    use os_target_core::MemWidth;

    use super::*;

    fn helper_suffix(width: MemWidth) -> &'static str {
        match width {
            MemWidth::W8 => "8",
            MemWidth::W16 => "16",
            MemWidth::W32 => "32",
            MemWidth::W64 => "64",
            MemWidth::W128 => "128",
        }
    }

    #[test]
    fn binary_push_pop_round_trip() {
        let mut b = BinaryRiscv64Backend::new_binary();
        b.op(OsOp::PushU64(0x1234_5678_9abc_def0));
        b.op(OsOp::Pop);
        b.finish();
        assert!(!b.into_bytes().is_empty());
    }

    #[test]
    fn binary_store_helper_code_is_nonempty() {
        let mut b = BinaryRiscv64Backend::new_binary();
        b.op(OsOp::PushU64(0x200));
        b.op(OsOp::PushU64(0x42));
        b.op(OsOp::Store { width: MemWidth::W32 });
        assert!(!b.into_bytes().is_empty());
    }

    #[test]
    fn binary_trap_is_nonempty() {
        let mut b = BinaryRiscv64Backend::new_binary();
        b.op(OsOp::Trap);
        assert!(!b.into_bytes().is_empty());
    }

    #[test]
    fn helper_suffix_table() {
        assert_eq!(helper_suffix(MemWidth::W16), "16");
        assert_eq!(helper_suffix(MemWidth::W64), "64");
    }
}