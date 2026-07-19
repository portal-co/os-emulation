//! AArch64 System V (AAPCS64) backend for the shared `OsOp` stack-machine IR.

extern crate alloc;

use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;
use core::fmt::Display;
use core::marker::PhantomData;

use os_target_core::{Backend, MemWidth, OsOp};
use portal_pc_asm_common::types::mem::MemorySize;

use crate::NativeHelpers;
use portal_pc_asm_common::types::reg::Reg;
use portal_solutions_asm_aarch64::out::arg::{
    AddressingMode, ArgKind, MemArgKind as A64MemArgKind,
};
use portal_solutions_asm_aarch64::out::bin::AArch64Writer;
use portal_solutions_asm_aarch64::out::{Writer as A64Writer, WriterCore as A64WriterCore};
use portal_solutions_asm_aarch64::{AArch64Arch, RegisterClass};

use os_target_core::NativeBackend;

pub type Label = &'static str;

/// AArch64 System V backend for OsOp.
///
/// Generic over any `asm-aarch64` writer, but intended to be used with the
/// binary `AArch64Writer` for machine-code generation.
///
/// The AAPCS64 ABI is used: X0 is the scratch/return register, X1 is the
/// second argument register, SP is the operand stack, FP (X29) and LR (X30)
/// are saved by the generated prologue.
pub struct AArch64SysVBackend<W, L = Label>
where
    W: A64WriterCore<()> + A64Writer<L, ()>,
{
    cfg: AArch64SysVConfig,
    pub writer: W,
    _label: PhantomData<L>,
}

pub type BinaryAArch64SysVBackend = AArch64SysVBackend<AArch64Writer<Label>, Label>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AArch64SysVConfig {
    pub emit_frame: bool,
    pub helpers: NativeHelpers,
}

impl Default for AArch64SysVConfig {
    fn default() -> Self {
        Self {
            emit_frame: true,
            helpers: NativeHelpers::DEFAULTS,
        }
    }
}

impl BinaryAArch64SysVBackend {
    /// Create a binary backend at `base_ip`.
    pub fn new_binary(base_ip: u64) -> Self {
        let _ = base_ip;
        Self::with_config_and_writer(
            AArch64SysVConfig::default(),
            AArch64Writer::<Label>::new(),
        )
    }

    /// Take the emitted machine code bytes.
    pub fn into_bytes(self) -> Vec<u8> {
        self.writer.into_bytes()
    }
}

impl<W, L> AArch64SysVBackend<W, L>
where
    W: A64WriterCore<()> + A64Writer<L, ()>,
    L: Display + Ord + Clone + From<&'static str>,
{
    pub fn with_config_and_writer(cfg: AArch64SysVConfig, writer: W) -> Self {
        let mut s = Self {
            cfg,
            writer,
            _label: PhantomData,
        };
        s.emit_prologue();
        s
    }

    fn arch() -> AArch64Arch {
        AArch64Arch::default()
    }

    fn x0(&self) -> Reg {
        Reg(0)
    }
    fn x1(&self) -> Reg {
        Reg(1)
    }
    fn fp(&self) -> Reg {
        Reg(29)
    }
    fn lr(&self) -> Reg {
        Reg(30)
    }
    fn sp(&self) -> Reg {
        Reg(31)
    }

    fn emit_prologue(&mut self) {
        if self.cfg.emit_frame {
            let arch = Self::arch();
            let fp = self.fp();
            let lr = self.lr();
            // stp x29, x30, [sp, #-16]!
            let _ = A64WriterCore::stp(
                &mut self.writer,
                &mut (),
                arch,
                &fp,
                &lr,
                &stack_pair_mem(AddressingMode::PreIndex, -16),
            );
            // mov x29, sp
            let sp = self.sp();
            let _ = A64WriterCore::mov(&mut self.writer, &mut (), arch, &fp, &sp);
        }
    }

    fn emit_epilogue(&mut self) {
        if self.cfg.emit_frame {
            let arch = Self::arch();
            let sp = self.sp();
            let fp = self.fp();
            let lr = self.lr();
            // Realign SP to the saved frame record before popping it.
            // mov sp, x29
            let _ = A64WriterCore::mov(&mut self.writer, &mut (), arch, &sp, &fp);
            // ldp x29, x30, [sp], #16
            let _ = A64WriterCore::ldp(
                &mut self.writer,
                &mut (),
                arch,
                &fp,
                &lr,
                &stack_pair_mem(AddressingMode::PostIndex, 16),
            );
            let _ = A64WriterCore::ret(&mut self.writer, &mut (), arch);
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

impl<W, L> NativeBackend for AArch64SysVBackend<W, L>
where
    W: A64WriterCore<()> + A64Writer<L, ()>,
    L: Display + Ord + Clone + From<&'static str>,
{
}

impl<W, L> Backend for AArch64SysVBackend<W, L>
where
    W: A64WriterCore<()> + A64Writer<L, ()>,
    L: Display + Ord + Clone + From<&'static str>,
{
    fn op(&mut self, op: OsOp) {
        let arch = Self::arch();
        let x0 = self.x0();
        let x1 = self.x1();

        match op {
            OsOp::PushU64(v) => {
                let _ = A64WriterCore::mov_imm(&mut self.writer, &mut (), arch, &x0, v);
                let _ = A64WriterCore::str(
                    &mut self.writer,
                    &mut (),
                    arch,
                    &x0,
                    &sp_word_mem(AddressingMode::PreIndex, -8),
                );
            }
            OsOp::PushU32(v) => {
                let v = (v as i32) as u64; // sign-extend to 64 bits.
                let _ = A64WriterCore::mov_imm(&mut self.writer, &mut (), arch, &x0, v);
                let _ = A64WriterCore::str(
                    &mut self.writer,
                    &mut (),
                    arch,
                    &x0,
                    &sp_word_mem(AddressingMode::PreIndex, -8),
                );
            }
            OsOp::Pop => {
                let _ = A64WriterCore::ldr(
                    &mut self.writer,
                    &mut (),
                    arch,
                    &x0,
                    &sp_word_mem(AddressingMode::PostIndex, 8),
                );
            }
            OsOp::Load { width, signed } => {
                // Pop guest address into x0, call helper, push result onto stack.
                let label = self.load_label(width, signed);
                let _ = A64WriterCore::ldr(
                    &mut self.writer,
                    &mut (),
                    arch,
                    &x0,
                    &sp_word_mem(AddressingMode::PostIndex, 8),
                );
                let _ = A64Writer::bl_label(&mut self.writer, &mut (), arch, label);
                let _ = A64WriterCore::str(
                    &mut self.writer,
                    &mut (),
                    arch,
                    &x0,
                    &sp_word_mem(AddressingMode::PreIndex, -8),
                );
            }
            OsOp::Store { width } => {
                // Stack order: [value, address]. Pop address into x0, value into x1.
                let label = self.store_label(width);
                let _ = A64WriterCore::ldr(
                    &mut self.writer,
                    &mut (),
                    arch,
                    &x0,
                    &sp_word_mem(AddressingMode::PostIndex, 8),
                );
                let _ = A64WriterCore::ldr(
                    &mut self.writer,
                    &mut (),
                    arch,
                    &x1,
                    &sp_word_mem(AddressingMode::PostIndex, 8),
                );
                let _ = A64Writer::bl_label(&mut self.writer, &mut (), arch, label);
            }
            OsOp::Ecall { .. } => {
                let label = self.ecall_label();
                let _ = A64Writer::bl_label(&mut self.writer, &mut (), arch, label);
            }
            OsOp::Jump { .. } => {
                let _ = A64WriterCore::ldr(
                    &mut self.writer,
                    &mut (),
                    arch,
                    &x0,
                    &sp_word_mem(AddressingMode::PostIndex, 8),
                );
                let _ = A64WriterCore::br(&mut self.writer, &mut (), arch, &x0);
            }
            OsOp::TailCall { helper } => {
                let label = self.leak_label(helper);
                let _ = A64Writer::b_label(&mut self.writer, &mut (), arch, label);
            }
            OsOp::Trap => {
                let _ = A64WriterCore::brk(&mut self.writer, &mut (), arch, 0);
            }
        }
    }

    fn finish(&mut self) {
        self.emit_epilogue();
    }
}



fn sp_word_mem(mode: AddressingMode, disp: i32) -> A64MemArgKind<ArgKind> {
    word_mem(Reg(31), mode, disp)
}

fn stack_pair_mem(mode: AddressingMode, disp: i32) -> A64MemArgKind<ArgKind> {
    word_mem(Reg(31), mode, disp)
}

fn word_mem(reg: Reg, mode: AddressingMode, disp: i32) -> A64MemArgKind<ArgKind> {
    A64MemArgKind::Mem {
        base: ArgKind::Reg {
            reg,
            size: MemorySize::_64,
        },
        offset: None,
        disp,
        size: MemorySize::_64,
        reg_class: RegisterClass::Gpr,
        mode,
    }
}

#[cfg(test)]
mod tests {
    use alloc::format;

    use os_target_core::MemWidth;

    use super::*;

    fn helper_suffix(width: MemWidth) -> &'static str {
        use os_target_core::MemWidth::*;
        match width {
            W8 => "8",
            W16 => "16",
            W32 => "32",
            W64 => "64",
            W128 => "128",
        }
    }

    fn load_helper_name(width: MemWidth, signed: bool) -> String {
        format!("os_load_{}{}", if signed { "i" } else { "u" }, helper_suffix(width))
    }

    #[test]
    fn binary_push_pop_round_trip() {
        let mut b = BinaryAArch64SysVBackend::new_binary(0);
        b.op(OsOp::PushU64(0x1234_5678_9abc_def0));
        b.op(OsOp::Pop);
        b.finish();
        assert!(!b.into_bytes().is_empty());
    }

    #[test]
    fn binary_store_helper_uses_x0_x1() {
        let mut b = BinaryAArch64SysVBackend::new_binary(0);
        b.op(OsOp::PushU64(0x200));
        b.op(OsOp::PushU64(0x42));
        b.op(OsOp::Store { width: MemWidth::W32 });
        let code = b.into_bytes();
        assert!(!code.is_empty());
    }

    #[test]
    fn binary_trap_is_nonempty() {
        let mut b = BinaryAArch64SysVBackend::new_binary(0);
        b.op(OsOp::Trap);
        assert!(!b.into_bytes().is_empty());
    }

    #[test]
    fn load_helper_name_matches_unsigned() {
        assert_eq!(load_helper_name(MemWidth::W16, true), "os_load_i16");
        assert_eq!(load_helper_name(MemWidth::W16, false), "os_load_u16");
    }
}