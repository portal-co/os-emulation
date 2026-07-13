//! Native assembly backends for the shared `OsOp` stack-machine IR.
//!
//! This crate translates OsOp operations into textual assembly for a given ABI,
//! to be fed to a system assembler / linker. The initial backend is x86-64
//! System V ABI using the raw `asm-x86-64` crate (not the deprecated
//! wasm-blitz NaiveAbi).

#![no_std]

extern crate alloc;

use alloc::string::String;
use core::fmt::Write as FmtWrite;

use os_target_core::{Backend, MemWidth, OsOp};
use portal_pc_asm_common::types::reg::Reg;

use portal_solutions_asm_x86_64::out::{Writer as X64Writer, WriterCore as X64WriterCore};
use portal_solutions_asm_x86_64::X64Arch;

/// Names of the runtime helper functions that back memory/syscall operations.
///
/// The generated assembly calls these symbols; the linker must resolve them
/// against a small SysV-compatible runtime shim.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SysVHelpers {
    pub load_u8: &'static str,
    pub load_i8: &'static str,
    pub load_u16: &'static str,
    pub load_i16: &'static str,
    pub load_u32: &'static str,
    pub load_i32: &'static str,
    pub load_u64: &'static str,
    pub store_u8: &'static str,
    pub store_u16: &'static str,
    pub store_u32: &'static str,
    pub store_u64: &'static str,
    pub ecall: &'static str,
}

impl SysVHelpers {
    /// Default helper names usable with a hand-written SysV runtime shim.
    pub const DEFAULTS: Self = Self {
        load_u8: "os_load_u8",
        load_i8: "os_load_i8",
        load_u16: "os_load_u16",
        load_i16: "os_load_i16",
        load_u32: "os_load_u32",
        load_i32: "os_load_i32",
        load_u64: "os_load_u64",
        store_u8: "os_store_u8",
        store_u16: "os_store_u16",
        store_u32: "os_store_u32",
        store_u64: "os_store_u64",
        ecall: "os_ecall",
    };
}

impl Default for SysVHelpers {
    fn default() -> Self {
        Self::DEFAULTS
    }
}

/// Configuration for the x86-64 SysV ABI textual backend.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct X86_64SysVConfig {
    pub helpers: SysVHelpers,
    /// Function Prologue/Epilogue are omitted by default. Setting this to
    /// `true` emits `push rbp; mov rbp, rsp; ...; pop rbp; ret` around the
    /// generated ops.
    pub emit_frame: bool,
}

impl Default for X86_64SysVConfig {
    fn default() -> Self {
        Self {
            helpers: SysVHelpers::DEFAULTS,
            emit_frame: false,
        }
    }
}

/// x86-64 System V ABI backend for OsOp.
///
/// Uses `asm-x86-64` to emit textual instructions.  RAX is the scratch
/// register and the generated code uses the host stack (`push`/`pop`) as the
/// OsOp operand stack.
pub struct X86_64SysVBackend {
    cfg: X86_64SysVConfig,
    out: String,
}

impl X86_64SysVBackend {
    pub fn new() -> Self {
        Self::with_config(X86_64SysVConfig::default())
    }

    pub fn with_config(cfg: X86_64SysVConfig) -> Self {
        let mut s = Self {
            cfg,
            out: String::new(),
        };
        s.emit_prologue();
        s
    }

    /// Take the emitted assembly text.
    pub fn into_string(self) -> String {
        self.out
    }

    fn writer(&mut self) -> &mut dyn FmtWrite {
        &mut self.out
    }

    fn arch() -> X64Arch {
        X64Arch::default()
    }

    fn rax(&self) -> Reg {
        Reg(0)
    }

    fn rdi(&self) -> Reg {
        Reg(7)
    }

    fn rsi(&self) -> Reg {
        Reg(6)
    }

    fn emit_prologue(&mut self) {
        if self.cfg.emit_frame {
            let _ = writeln!(self.writer(), "push rbp\nmov rbp, rsp");
        }
    }

    fn emit_epilogue(&mut self) {
        if self.cfg.emit_frame {
            let _ = writeln!(self.writer(), "pop rbp\nret");
        }
    }

    fn load_helper(&self, width: MemWidth, signed: bool) -> &'static str {
        match (width, signed) {
            (MemWidth::W8, false) => self.cfg.helpers.load_u8,
            (MemWidth::W8, true) => self.cfg.helpers.load_i8,
            (MemWidth::W16, false) => self.cfg.helpers.load_u16,
            (MemWidth::W16, true) => self.cfg.helpers.load_i16,
            (MemWidth::W32, false) => self.cfg.helpers.load_u32,
            (MemWidth::W32, true) => self.cfg.helpers.load_i32,
            (MemWidth::W64, false) => self.cfg.helpers.load_u64,
            (MemWidth::W64, true) => self.cfg.helpers.load_u64,
            (MemWidth::W128, _) => "os_load_u128", // placeholder
        }
    }

    fn store_helper(&self, width: MemWidth) -> &'static str {
        match width {
            MemWidth::W8 => self.cfg.helpers.store_u8,
            MemWidth::W16 => self.cfg.helpers.store_u16,
            MemWidth::W32 => self.cfg.helpers.store_u32,
            MemWidth::W64 => self.cfg.helpers.store_u64,
            MemWidth::W128 => "os_store_u128", // placeholder
        }
    }
}

impl Default for X86_64SysVBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl Backend for X86_64SysVBackend {
    fn op(&mut self, op: OsOp) {
        let arch = Self::arch();
        let rax = self.rax();
        let rdi = self.rdi();
        let rsi = self.rsi();

        match op {
            OsOp::PushU64(v) => {
                let _ = X64WriterCore::mov64(self.writer(), &mut (), arch, &rax, v);
                let _ = X64WriterCore::push(self.writer(), &mut (), arch, &rax);
            }
            OsOp::PushU32(v) => {
                // Sign-extend from 32-bit Wasm-style value to 64-bit host
                // register; keep lower 32 bits if caller masks later.
                let _ = X64WriterCore::mov64(self.writer(), &mut (), arch, &rax, v as u64);
                let _ = X64WriterCore::push(self.writer(), &mut (), arch, &rax);
            }
            OsOp::Pop => {
                let _ = X64WriterCore::pop(self.writer(), &mut (), arch, &rax);
            }
            OsOp::Load { width, signed } => {
                let helper = self.load_helper(width, signed);
                // TOS is the guest address; replace it with the loaded value.
                let _ = X64WriterCore::pop(self.writer(), &mut (), arch, &rdi);
                let _ = X64Writer::call_label(self.writer(), &mut (), arch, helper);
                let _ = X64WriterCore::push(self.writer(), &mut (), arch, &rax);
            }
            OsOp::Store { width } => {
                let helper = self.store_helper(width);
                // Stack order: [value, address], address on top.
                let _ = X64WriterCore::pop(self.writer(), &mut (), arch, &rdi); // address
                let _ = X64WriterCore::pop(self.writer(), &mut (), arch, &rsi); // value
                let _ = X64Writer::call_label(self.writer(), &mut (), arch, helper);
            }
            OsOp::Ecall { .. } => {
                let helper = self.cfg.helpers.ecall;
                let _ = X64Writer::call_label(self.writer(), &mut (), arch, helper);
            }
            OsOp::Jump { .. } => {
                let _ = X64WriterCore::pop(self.writer(), &mut (), arch, &rax);
                let _ = X64WriterCore::jmp(self.writer(), &mut (), arch, &rax);
            }
            OsOp::TailCall { helper } => {
                let _ = X64Writer::jmp_label(self.writer(), &mut (), arch, helper);
            }
            OsOp::Trap => {
                let _ = writeln!(self.writer(), "ud2");
            }
        }
    }

    fn finish(&mut self) {
        self.emit_epilogue();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn push_u64_then_pop() {
        let mut b = X86_64SysVBackend::new();
        b.op(OsOp::PushU64(0x1234_5678_9abc_def0));
        b.op(OsOp::Pop);
        let text = b.into_string();
        assert!(text.contains("mov rax, 1311768467463790320"), "text: {}", text);
        assert!(text.contains("push rax"));
        assert!(text.contains("pop rax"));
    }

    #[test]
    fn store_32_swaps_operands_and_calls_helper() {
        let mut b = X86_64SysVBackend::new();
        b.op(OsOp::PushU64(0x200));
        b.op(OsOp::PushU64(0x42));
        b.op(OsOp::Store { width: MemWidth::W32 });
        let text = b.into_string();
        assert!(text.contains("pop rdi"), "address -> rdi");
        assert!(text.contains("pop rsi"), "value -> rsi");
        assert!(text.contains("call os_store_u32"), "store helper call");
    }

    #[test]
    fn jump_uses_indirect_rax() {
        let mut b = X86_64SysVBackend::new();
        b.op(OsOp::PushU64(0x1000));
        b.op(OsOp::Jump { target: 0x1000 });
        let text = b.into_string();
        assert!(text.contains("pop rax"));
        assert!(text.contains("jmp rax"));
    }

    #[test]
    fn tail_call_emits_symbolic_jmp() {
        let mut b = X86_64SysVBackend::new();
        b.op(OsOp::TailCall { helper: "os_dispatch".into() });
        let text = b.into_string();
        assert!(text.contains("jmp os_dispatch"));
    }

    #[test]
    fn trap_emits_ud2() {
        let mut b = X86_64SysVBackend::new();
        b.op(OsOp::Trap);
        assert!(b.into_string().contains("ud2"));
    }

    #[test]
    fn load_with_sign_extend_calls_right_helper() {
        let mut b = X86_64SysVBackend::new();
        b.op(OsOp::PushU64(0x300));
        b.op(OsOp::Load {
            width: MemWidth::W16,
            signed: true,
        });
        let text = b.into_string();
        assert!(text.contains("call os_load_i16"), "text: {}", text);
        assert!(text.contains("push rax"));
    }
}