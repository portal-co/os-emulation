//! `os-page-codegen` — compile-time emitters for guest memory/paging helpers.
//!
//! Backends call these helpers while emitting OS glue.  They produce `OsOp`
//! sequences on a generic `Backend`, so the same page-table description can
//! target WASM, JavaScript, or future native/LLVM emitters.

#![no_std]
extern crate alloc;

use alloc::format;
use alloc::string::String;
use os_build::MemoryCodegen;
use os_page::MemorySpec;
use os_target_core::{Backend, MemWidth, OsOp};

/// A concrete JavaScript `Backend` that writes statements into a string buffer.
///
/// Generated code assumes a stack machine runtime variable (default `osStack`)
/// and a WASM linear-memory helper `$.get_page(Number(addr))`.  The output is
/// intentionally low-level: it can be embedded directly inside a generated
/// JS function by higher-level code.
pub struct JsBackend<'a> {
    pub out: &'a mut String,
    pub stack: &'a str,
    pub memory: &'a str,
    pub helper_prefix: &'a str,
}

impl<'a> Default for JsBackend<'a> {
    fn default() -> Self {
        Self {
            out: unsafe { &mut *(&mut String::new() as *mut String) },
            stack: "osStack",
            memory: "$._sys('memory')",
            helper_prefix: "",
        }
    }
}

impl<'a> JsBackend<'a> {
    pub fn new(out: &'a mut String) -> Self {
        Self {
            out,
            stack: "osStack",
            memory: "$._sys('memory')",
            helper_prefix: "",
        }
    }

    fn line(&mut self, s: impl core::fmt::Display) {
        use core::fmt::Write;
        let _ = writeln!(self.out, "{}", s);
    }

    fn width_bytes(&self, width: MemWidth) -> u8 {
        use MemWidth::*;
        match width {
            W8 => 1,
            W16 => 2,
            W32 => 4,
            W64 => 8,
            W128 => 16,
        }
    }

    fn memory_read_expr(&self, addr_var: &str, width: MemWidth, signed: bool) -> String {
        let bytes = self.width_bytes(width);
        let get = match (width, signed) {
            (MemWidth::W8, true) => "getInt8(0)",
            (MemWidth::W8, false) => "getUint8(0)",
            (MemWidth::W16, true) => "getInt16(0,true)",
            (MemWidth::W16, false) => "getUint16(0,true)",
            (MemWidth::W32, true) => "getInt32(0,true)",
            (MemWidth::W32, false) => "getUint32(0,true)",
            (MemWidth::W64, _) => "getBigUint64(0,true)",
            (MemWidth::W128, _) => "0n /* W128 not supported */",
        };
        format!(
            "(()=>{{let __dv=new DataView({}.buffer,{}.get_page(Number({})),{});return __dv.{};}})()",
            self.memory, self.helper_prefix, addr_var, bytes, get
        )
    }

    fn memory_write_expr(&self, addr_var: &str, value_var: &str, width: MemWidth) -> String {
        let bytes = self.width_bytes(width);
        let set = match width {
            MemWidth::W8 => "setUint8(0,Number(value)&0xFF)",
            MemWidth::W16 => "setUint16(0,Number(value)&0xFFFF,true)",
            MemWidth::W32 => "setUint32(0,Number(value)&0xFFFFFFFF,true)",
            MemWidth::W64 => "setBigUint64(0,value,true)",
            MemWidth::W128 => "/* W128 not supported */",
        };
        format!(
            "(()=>{{let __dv=new DataView({}.buffer,{}.get_page(Number({})),{});__dv.{};}})();",
            self.memory, self.helper_prefix, addr_var, bytes, set
        )
        .replace("value", value_var)
    }
}

impl<'a> Backend for JsBackend<'a> {
    fn op(&mut self, op: OsOp) {
        match op {
            OsOp::PushU64(v) => self.line(format!("{}.push({}n);", self.stack, v)),
            OsOp::PushU32(v) => self.line(format!("{}.push({}n);", self.stack, v)),
            OsOp::Pop => self.line(format!("{}.pop();", self.stack)),
            OsOp::Load { width, signed } => {
                self.line(format!("let __addr = {}.pop();", self.stack));
                let expr = self.memory_read_expr("__addr", width, signed);
                self.line(format!("{}.push({});", self.stack, expr));
            }
            OsOp::Store { width } => {
                self.line(format!("let __val = {}.pop(); let __addr = {}.pop();", self.stack, self.stack));
                self.line(self.memory_write_expr("__addr", "__val", width));
            }
            OsOp::Ecall { may_await } if may_await => {
                self.line(format!(
                    "let __ecall = {s}.pop(); let __ret = await {}.ecall(__ecall); {s}.push(__ret);",
                    self.helper_prefix, s = self.stack
                ));
            }
            OsOp::Ecall { .. } => {
                self.line(format!(
                    "let __ecall = {s}.pop(); let __ret = {}.ecall(__ecall); {s}.push(__ret);",
                    self.helper_prefix, s = self.stack
                ));
            }
            OsOp::Jump { target } => self.line(format!(
                "throw new Error('jump to 0x' + ({}).toString(16) + ' not resolved');",
                target
            )),
            OsOp::TailCall { helper } => self.line(format!(
                "return {}.tailCall('{}', {s});",
                self.helper_prefix, helper, s = self.stack
            )),
            OsOp::Trap => self.line("throw new Error('os trap');"),
        }
    }
}

/// Emit the default `data(addr)` / `mem[addr]` access for a JS-style backend.
///
/// The address is expected on the backend's value stack.  This is a placeholder
/// shape: real backends will push helper-references or inline the page walk.
pub fn emit_js_data_access<B: Backend>(backend: &mut B, width: MemWidth, _spec: &MemorySpec) {
    backend.op(OsOp::Load {
        width,
        signed: false,
    });
}

/// Emit the default Wasm-style load/store helper for a `wax-core` sink.
///
/// Like `emit_js_data_access`, this is a shape implementation; the full page walk
/// lives in a future `os-page-codegen::WaxMemoryCodegen`.
pub fn emit_wax_memory_access<B: Backend>(backend: &mut B, op: &os_build::MemoryAccessOp) {
    if op.write {
        backend.op(OsOp::Store { width: op.width });
    } else {
        backend.op(OsOp::Load {
            width: op.width,
            signed: op.signed,
        });
    }
}

/// A generic implementation of `MemoryCodegen<B>` that emits the simplest
/// possible `OsOp` shapes for any backend.  Real consumers will replace this
/// with backend-aware emitters as they are implemented.
pub struct MinimalMemoryCodegen;

impl<B: Backend> MemoryCodegen<B> for MinimalMemoryCodegen {
    fn emit_memory_access(&mut self, backend: &mut B, op: &os_build::MemoryAccessOp) {
        emit_wax_memory_access(backend, op);
    }

    fn emit_page_table_glue(&mut self, _backend: &mut B, _spec: &MemorySpec) {
        // Placeholder for future page-table helper emission.
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use os_target_core::MemWidth;

    #[test]
    fn js_backend_push_pop() {
        let mut out = String::new();
        let mut backend = JsBackend::new(&mut out);
        backend.op(OsOp::PushU64(42));
        backend.op(OsOp::Pop);
        let s = out.as_str();
        assert!(s.contains("osStack.push(42n)"));
        assert!(s.contains("osStack.pop()"));
    }

    #[test]
    fn js_backend_load_writes_dataview() {
        let mut out = String::new();
        let mut backend = JsBackend::new(&mut out);
        backend.op(OsOp::PushU64(0x1000));
        backend.op(OsOp::Load {
            width: MemWidth::W32,
            signed: false,
        });
        let s = out.as_str();
        assert!(s.contains("let __addr = osStack.pop();"));
        assert!(s.contains("getUint32(0,true)"));
        assert!(s.contains("new DataView"));
    }

    #[test]
    fn js_backend_store_swaps_operands() {
        let mut out = String::new();
        let mut backend = JsBackend::new(&mut out);
        backend.op(OsOp::PushU64(0x1234));
        backend.op(OsOp::PushU64(0xCAFE_BABE));
        backend.op(OsOp::Store { width: MemWidth::W32 });
        let s = out.as_str();
        assert!(s.contains("let __val = osStack.pop(); let __addr = osStack.pop();"));
        assert!(s.contains("setUint32(0,Number(__val)&0xFFFFFFFF,true)"));
    }

    #[test]
    fn minimal_memory_codegen_wax_load() {
        let mut backend: alloc::vec::Vec<OsOp> = alloc::vec::Vec::new();
        let mut codegen = MinimalMemoryCodegen;
        let op = os_build::MemoryAccessOp {
            width: MemWidth::W8,
            write: false,
            signed: true,
        };
        codegen.emit_memory_access(&mut backend, &op);
        assert!(backend.iter().any(|o| matches!(o, OsOp::Load { width: MemWidth::W8, signed: true })));
    }
}