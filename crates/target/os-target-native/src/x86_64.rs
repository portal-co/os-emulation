//! x86-64 System V ABI backend for the shared `OsOp` stack-machine IR.

extern crate alloc;

use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;
use core::fmt::{self, Display, Write as FmtWrite};
use core::marker::PhantomData;

use os_target_core::{Backend, MemWidth, NativeBackend, OsOp};
use portal_pc_asm_common::types::reg::Reg;
use portal_solutions_asm_x86_64::out::iced::IcedWriter;
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

/// Configuration for the x86-64 SysV ABI backend.
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

/// Label type used by all x86-64 native backends.
///
/// Both the text writer and the binary `IcedWriter` accept `&'static str`
/// labels, so helpers and tail-call targets can be emitted as human-readable
/// symbols.
pub type Label = &'static str;

/// Text-output writer used by [`TextX86_64SysVBackend`].
///
/// A local newtype is required because the `writers!` macro implements
/// `WriterCore`/`Writer` for the type in this crate, which would violate the
/// orphan rule if implemented directly on `String`.
#[derive(Default)]
pub struct TextWriter(pub String);

impl fmt::Write for TextWriter {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        self.0.write_str(s)
    }
}

portal_solutions_asm_x86_64::writers!(TextWriter);

impl TextWriter {
    pub fn into_string(self) -> String {
        self.0
    }
}

/// x86-64 System V ABI backend for OsOp.
///
/// Generic over the `asm-x86-64` writer `W`, so the same lowering logic can
/// emit textual assembly (via [`TextWriter`]) or binary machine code (via
/// `IcedWriter<Label>`).
///
/// RAX is the scratch register and the generated code uses the host stack
/// (`push`/`pop`) as the OsOp operand stack.
pub struct X86_64SysVBackend<W, L = Label>
where
    W: X64WriterCore<()> + X64Writer<L, ()>,
{
    cfg: X86_64SysVConfig,
    writer: W,
    _label: PhantomData<L>,
}

/// Text-only convenience alias.
pub type TextX86_64SysVBackend = X86_64SysVBackend<TextWriter, Label>;

/// Binary machine-code convenience alias.
pub type BinaryX86_64SysVBackend = X86_64SysVBackend<IcedWriter<Label>, Label>;

impl TextX86_64SysVBackend {
    /// Create a backend that emits textual assembly.
    pub fn new_text() -> Self {
        Self::with_config(X86_64SysVConfig::default())
    }

    /// Take the emitted assembly text.
    pub fn into_string(self) -> String {
        self.writer.into_string()
    }
}

impl BinaryX86_64SysVBackend {
    /// Create a backend that emits binary machine code, starting at `base_ip`.
    ///
    /// `base_ip` is usually `0` for freshly-mapped test images.
    pub fn new_binary(base_ip: u64) -> Self {
        Self::with_config_and_writer(
            X86_64SysVConfig::default(),
            IcedWriter::<Label>::new(base_ip),
        )
    }

    /// Take the emitted machine code bytes.
    ///
    /// Any unresolved label references are left as zero immediates; callers
    /// that need to link helper stubs can use `into_parts_with_relocs` on the
    /// underlying `IcedWriter` instead.
    pub fn into_bytes(self) -> Vec<u8> {
        self.writer.into_bytes()
    }
}

impl Default for TextX86_64SysVBackend {
    fn default() -> Self {
        Self::new_text()
    }
}

impl<W, L> X86_64SysVBackend<W, L>
where
    W: X64WriterCore<()> + X64Writer<L, ()>,
    L: Display + Ord + Clone + From<&'static str>,
{
    pub fn with_config(cfg: X86_64SysVConfig) -> Self
    where
        W: Default,
    {
        Self::with_config_and_writer(cfg, W::default())
    }

    pub fn with_config_and_writer(cfg: X86_64SysVConfig, writer: W) -> Self {
        let mut s = Self {
            cfg,
            writer,
            _label: PhantomData,
        };
        s.emit_prologue();
        s
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

    fn rbp(&self) -> Reg {
        Reg(5)
    }

    fn rsp(&self) -> Reg {
        Reg(4)
    }

    fn emit_prologue(&mut self) {
        if self.cfg.emit_frame {
            let arch = Self::arch();
            let rbp = self.rbp();
            let rsp = self.rsp();
            let _ = X64WriterCore::push(&mut self.writer, &mut (), arch, &rbp);
            let _ = X64WriterCore::mov(&mut self.writer, &mut (), arch, &rbp, &rsp);
        }
    }

    fn emit_epilogue(&mut self) {
        if self.cfg.emit_frame {
            let arch = Self::arch();
            let rbp = self.rbp();
            let _ = X64WriterCore::pop(&mut self.writer, &mut (), arch, &rbp);
            let _ = X64WriterCore::ret(&mut self.writer, &mut (), arch);
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

    /// Convert a dynamically-created helper name into a label.
    ///
    /// This leaks the string so that the label stays valid for the lifetime of
    /// the generated code. Generated code is short-lived, so the leak is
    /// acceptable.
    fn leak_label(&self, s: String) -> L {
        L::from(Box::leak(s.into_boxed_str()))
    }
}

impl<W, L> NativeBackend for X86_64SysVBackend<W, L>
where
    W: X64WriterCore<()> + X64Writer<L, ()>,
    L: Display + Ord + Clone + From<&'static str>,
{
}

impl<W, L> Backend for X86_64SysVBackend<W, L>
where
    W: X64WriterCore<()> + X64Writer<L, ()>,
    L: Display + Ord + Clone + From<&'static str>,
{
    fn op(&mut self, op: OsOp) {
        let arch = Self::arch();
        let rax = self.rax();
        let rdi = self.rdi();
        let rsi = self.rsi();

        match op {
            OsOp::PushU64(v) => {
                let _ = X64WriterCore::mov64(&mut self.writer, &mut (), arch, &rax, v);
                let _ = X64WriterCore::push(&mut self.writer, &mut (), arch, &rax);
            }
            OsOp::PushU32(v) => {
                // Sign-extend from 32-bit Wasm-style value to 64-bit host
                // register; keep lower 32 bits if caller masks later.
                let _ = X64WriterCore::mov64(&mut self.writer, &mut (), arch, &rax, v as u64);
                let _ = X64WriterCore::push(&mut self.writer, &mut (), arch, &rax);
            }
            OsOp::Pop => {
                let _ = X64WriterCore::pop(&mut self.writer, &mut (), arch, &rax);
            }
            OsOp::Load { width, signed } => {
                let helper = self.load_helper(width, signed);
                // TOS is the guest address; replace it with the loaded value.
                let _ = X64WriterCore::pop(&mut self.writer, &mut (), arch, &rdi);
                let _ = X64Writer::call_label(&mut self.writer, &mut (), arch, L::from(helper));
                let _ = X64WriterCore::push(&mut self.writer, &mut (), arch, &rax);
            }
            OsOp::Store { width } => {
                let helper = self.store_helper(width);
                // Stack order: [value, address], address on top.
                let _ = X64WriterCore::pop(&mut self.writer, &mut (), arch, &rdi); // address
                let _ = X64WriterCore::pop(&mut self.writer, &mut (), arch, &rsi); // value
                let _ = X64Writer::call_label(&mut self.writer, &mut (), arch, L::from(helper));
            }
            OsOp::Ecall { .. } => {
                let helper = self.cfg.helpers.ecall;
                let _ = X64Writer::call_label(&mut self.writer, &mut (), arch, L::from(helper));
            }
            OsOp::Jump { .. } => {
                let _ = X64WriterCore::pop(&mut self.writer, &mut (), arch, &rax);
                let _ = X64WriterCore::jmp(&mut self.writer, &mut (), arch, &rax);
            }
            OsOp::TailCall { helper } => {
                let label = self.leak_label(helper);
                let _ = X64Writer::jmp_label(&mut self.writer, &mut (), arch, label);
            }
            OsOp::Trap => {
                // UD2: x86-64 undefined instruction. Works both as raw bytes
                // for unicorn and as `.byte` directives for text diagnostics.
                let _ = X64WriterCore::db(&mut self.writer, &mut (), arch, &[0x0F, 0x0B]);
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
    fn text_push_pop_prologue_epilogue() {
        let mut b = TextX86_64SysVBackend::with_config(X86_64SysVConfig {
            emit_frame: true,
            ..Default::default()
        });
        b.op(OsOp::PushU64(0x1234_5678_9abc_def0));
        b.op(OsOp::Pop);
        b.finish();
        let text = b.writer.into_string();
        assert!(text.contains("push rbp"));
        assert!(text.contains("mov rbp, rsp"));
        assert!(text.contains("pop rbp"));
        assert!(text.contains("ret"));
        assert!(text.contains("push rax"));
        assert!(text.contains("pop rax"));
    }

    #[test]
    fn text_store_swaps_operands_and_calls_helper() {
        let mut b = TextX86_64SysVBackend::new_text();
        b.op(OsOp::PushU64(0x200));
        b.op(OsOp::PushU64(0x42));
        b.op(OsOp::Store { width: MemWidth::W32 });
        let text = b.into_string();
        assert!(text.contains("pop rdi"), "address -> rdi");
        assert!(text.contains("pop rsi"), "value -> rsi");
        assert!(text.contains("call os_store_u32"), "store helper call");
    }

    #[test]
    fn text_load_with_sign_extend_calls_right_helper() {
        let mut b = TextX86_64SysVBackend::new_text();
        b.op(OsOp::PushU64(0x300));
        b.op(OsOp::Load {
            width: MemWidth::W16,
            signed: true,
        });
        let text = b.into_string();
        assert!(text.contains("call os_load_i16"), "text: {}", text);
        assert!(text.contains("push rax"));
    }

    #[test]
    fn text_trap_emits_ud2_bytes() {
        let mut b = TextX86_64SysVBackend::new_text();
        b.op(OsOp::Trap);
        let text = b.into_string();
        assert!(text.contains(".byte 0x0f, 0x0b"), "text: {}", text);
    }
}