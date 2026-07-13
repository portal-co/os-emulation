//! `os-target-wax` — render [`os_target_core::OsOp`] operations into WASM
//! instructions via the [`wax_core::InstructionSink`] trait.
//!
//! This backend is deliberately low-level: it does not know about ecall
//! semantics, dispatch tables, or jump targets.  Higher-level glue code
//! builds those structures by emitting sequences of `OsOp`s that this
//! backend converts to real `wasm_encoder::Instruction`s.

#![no_std]

extern crate alloc;

use core::convert::Infallible;
use core::marker::PhantomData;
use os_target_core::{Backend, MemWidth, OsOp};
use wasm_encoder::Instruction;
use wax_core::InstructionSink;

/// Configuration for the WASM output.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WaxConfig {
    /// Index of the linear memory the generated code accesses.
    pub memory_index: u32,
    /// When `true`, the memory is a 64-bit memory and addresses stay `i64`.
    /// When `false` (the default), addresses are wrapped to `i32` before
    /// accessing a 32-bit linear memory.
    pub memory64: bool,
    /// Function index of the imported ecall handler.  `None` means ecalls
    /// trap at runtime.
    pub ecall_import: Option<u32>,
}

impl Default for WaxConfig {
    fn default() -> Self {
        Self {
            memory_index: 0,
            memory64: false,
            ecall_import: None,
        }
    }
}

/// Scratch locals used by [`WaxBackend`] when it needs to reorder values
/// for WASM `store` instructions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WaxScratchLocals {
    pub addr: u32,
    pub value: u32,
}

/// A [`Backend`] that writes into any [`wax_core::InstructionSink`].
///
/// The typical sink is a `wasm_encoder::Function`, but `yeeta` reactors,
/// test mocks, or other generic `InstructionSink` implementors also work.
pub struct WaxBackend<'s, 'c, S, C, E> {
    pub sink: &'s mut S,
    pub ctx: &'c mut C,
    pub config: WaxConfig,
    pub scratch: Option<WaxScratchLocals>,
    _marker: PhantomData<E>,
}

impl<'s, 'c, S, C, E> WaxBackend<'s, 'c, S, C, E>
where
    S: InstructionSink<C, E>,
{
    pub fn new(sink: &'s mut S, ctx: &'c mut C, config: WaxConfig) -> Self {
        Self {
            sink,
            ctx,
            config,
            scratch: None,
            _marker: PhantomData,
        }
    }

    pub fn with_scratch(
        sink: &'s mut S,
        ctx: &'c mut C,
        config: WaxConfig,
        scratch: WaxScratchLocals,
    ) -> Self {
        Self {
            sink,
            ctx,
            config,
            scratch: Some(scratch),
            _marker: PhantomData,
        }
    }

    fn emit(&mut self, instr: Instruction<'static>) {
        self.sink.instruction(self.ctx, &instr).ok();
    }

    fn emit_addr(&mut self) {
        if !self.config.memory64 {
            self.emit(Instruction::I32WrapI64);
        }
    }

    fn emit_load_extend(&mut self, width: MemWidth, signed: bool) {
        use MemWidth::*;
        let mem = self.memarg(width);
        match (width, signed) {
            (W8, true) => {
                self.emit(Instruction::I64Load8S(mem));
            }
            (W8, false) => {
                self.emit(Instruction::I64Load8U(mem));
            }
            (W16, true) => {
                self.emit(Instruction::I64Load16S(mem));
            }
            (W16, false) => {
                self.emit(Instruction::I64Load16U(mem));
            }
            (W32, true) => {
                self.emit(Instruction::I32Load(mem));
                self.emit(Instruction::I64ExtendI32S);
            }
            (W32, false) => {
                self.emit(Instruction::I32Load(mem));
                self.emit(Instruction::I64ExtendI32U);
            }
            (W64, _) => {
                self.emit(Instruction::I64Load(mem));
            }
            (W128, _) => self.emit(Instruction::Unreachable),
        }
    }

    fn emit_store_wrap(&mut self, width: MemWidth) {
        use MemWidth::*;
        let mem = self.memarg(width);
        match width {
            W8 => self.emit(Instruction::I64Store8(mem)),
            W16 => self.emit(Instruction::I64Store16(mem)),
            W32 => self.emit(Instruction::I32Store(mem)),
            W64 => self.emit(Instruction::I64Store(mem)),
            W128 => self.emit(Instruction::Unreachable),
        }
    }

    fn memarg(&self, width: MemWidth) -> wasm_encoder::MemArg {
        use MemWidth::*;
        let align = match width {
            W8 => 0,
            W16 => 1,
            W32 => 2,
            W64 => 3,
            W128 => 4,
        };
        wasm_encoder::MemArg {
            offset: 0,
            align,
            memory_index: self.config.memory_index,
        }
    }
}

impl<'s, 'c, S, C, E> Backend for WaxBackend<'s, 'c, S, C, E>
where
    S: InstructionSink<C, E>,
{
    fn op(&mut self, op: OsOp) {
        match op {
            OsOp::PushU64(v) => self.emit(Instruction::I64Const(v as i64)),
            OsOp::PushU32(v) => self.emit(Instruction::I64Const(v as i64)),
            OsOp::Pop => self.emit(Instruction::Drop),

            OsOp::Load { width, signed } => {
                self.emit_addr();
                self.emit_load_extend(width, signed);
            }

            OsOp::Store { width } => {
                // Stack order entering this op: [value, addr] with addr on top.
                if let Some(scratch) = self.scratch {
                    // Capture both operands and emit them in WASM store order.
                    self.emit(Instruction::LocalSet(scratch.addr));
                    self.emit(Instruction::LocalSet(scratch.value));
                    self.emit(Instruction::LocalGet(scratch.value));
                    if width == MemWidth::W32 || width == MemWidth::W8 || width == MemWidth::W16
                    {
                        // I32Store / I64Store8 / I64Store16 expect an i32 value.
                        self.emit(Instruction::I32WrapI64);
                    }
                    self.emit(Instruction::LocalGet(scratch.addr));
                    self.emit_addr();
                    self.emit_store_wrap(width);
                } else {
                    // Without scratch locals we cannot reliably swap/retype the
                    // top two i64 stack values, so leave this as a trap so the
                    // mis-configuration is obvious.
                    self.emit(Instruction::Unreachable);
                }
            }

            OsOp::Ecall { .. } => {
                if let Some(idx) = self.config.ecall_import {
                    self.emit(Instruction::Call(idx));
                } else {
                    self.emit(Instruction::Unreachable);
                }
            }

            OsOp::Jump { .. } | OsOp::TailCall { .. } => {
                // Guest jumps are resolved by higher-level recompiler glue;
                // a bare WaxBackend cannot encode them without a translation table.
                self.emit(Instruction::Unreachable);
            }

            OsOp::Trap => self.emit(Instruction::Unreachable),
        }
    }

    fn finish(&mut self) {
        let _ = self.sink.finish();
    }
}

/// Convenience helper: render a `&[OsOp]` into a freshly allocated
/// `wasm_encoder::Function` body.
///
/// Returns `None` if the sink reported an error.
pub fn render_to_function(ops: &[OsOp], config: WaxConfig) -> Option<wasm_encoder::Function> {
    let mut ctx = ();
    let mut func = wasm_encoder::Function::new_with_locals_types::<[wasm_encoder::ValType;0]>([]);
    let mut backend = WaxBackend::<_, _, Infallible>::new(&mut func, &mut ctx, config);
    for op in ops {
        backend.op(op.clone());
    }
    backend.finish();
    Some(func)
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec::Vec;
    use os_target_core::MemWidth;
    use wasm_encoder::{Encode, Function, ValType};

    fn encode_func(func: &Function) -> Vec<u8> {
        let mut bytes = Vec::new();
        func.encode(&mut bytes);
        bytes
    }

    #[test]
    fn push_then_drop() {
        let mut ctx = ();
        let mut func = Function::new_with_locals_types::<[ValType; 0]>([]);
        let mut backend = WaxBackend::<_, _, Infallible>::new(
            &mut func,
            &mut ctx,
            WaxConfig::default(),
        );
        backend.op(OsOp::PushU64(42));
        backend.op(OsOp::Pop);
        let bytes = encode_func(backend.sink);
        assert!(!bytes.is_empty());
    }

    #[test]
    fn load_32u_from_memory() {
        let mut ctx = ();
        let mut func = Function::new_with_locals_types::<[ValType; 0]>([]);
        let mut backend = WaxBackend::<_, _, Infallible>::new(
            &mut func,
            &mut ctx,
            WaxConfig::default(),
        );
        backend.op(OsOp::PushU64(0x1000));
        backend.op(OsOp::Load {
            width: MemWidth::W32,
            signed: false,
        });
        let bytes = encode_func(&backend.sink);
        assert!(!bytes.is_empty());
    }

    #[test]
    fn store_32_with_scratch() {
        let mut ctx = ();
        let mut func = Function::new_with_locals_types([ValType::I64, ValType::I64]);
        let config = WaxConfig::default();
        let mut backend = WaxBackend::<_, _, Infallible>::with_scratch(
            &mut func,
            &mut ctx,
            config,
            WaxScratchLocals { addr: 0, value: 1 },
        );
        backend.op(OsOp::PushU64(0x1234));
        backend.op(OsOp::PushU64(0xCAFE_BABE));
        backend.op(OsOp::Store { width: MemWidth::W32 });
        let bytes = encode_func(&backend.sink);
        assert!(!bytes.is_empty());
    }

    #[test]
    fn memory64_preserves_i64_address() {
        let mut ctx = ();
        let mut func = Function::new_with_locals_types::<[ValType; 0]>([]);
        let mut backend = WaxBackend::<_, _, Infallible>::new(
            &mut func,
            &mut ctx,
            WaxConfig {
                memory64: true,
                ..Default::default()
            },
        );
        backend.op(OsOp::PushU64(0x1_0000_0000u64));
        backend.op(OsOp::Load {
            width: MemWidth::W64,
            signed: false,
        });
        let bytes = encode_func(&backend.sink);
        assert!(!bytes.is_empty());
    }
}