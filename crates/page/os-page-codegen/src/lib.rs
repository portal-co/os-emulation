//! `os-page-codegen` — compile-time emitters for guest memory/paging helpers.
//!
//! Backends call these helpers while emitting OS glue.  They produce `OsOp`
//! sequences on a generic `Backend`, so the same page-table description can
//! target WASM, JavaScript, or future native/LLVM emitters.

#![no_std]
extern crate alloc;

use os_build::MemoryCodegen;
use os_page::MemorySpec;
use os_target_core::{Backend, MemWidth, OsOp};

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