//! Compiler/builder glue contract for shared OS emulation code generation.
//!
//! `os-build` defines the `BuildGlue<B: Backend>` trait set. Concrete
//! recompilers (Speet's `MegabinaryBuilder`, Vane's JS JIT, future Vane
//! WASM JIT) implement `BuildGlue` for each backend they support. OS
//! crates (`os-page-codegen`, `os-abi-codegen`, `os-syscall-emit`) call
//! `BuildGlue` methods, which emit [`OsOp`] operations onto the passed-in
//! `Backend`.

#![no_std]

extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;
use os_target_core::{Backend, GuestAddr, MemWidth};
pub use os_page::MemorySpec;
pub use os_syscall_emit::{MemoryStore, ParamSource, SavePair, SyscallEntry, SyscallTable};

// ---------------------------------------------------------------------------
// Compile-time memory abstraction
// ---------------------------------------------------------------------------

/// Compile-time memory abstraction used to reason about address spaces,
/// page properties, and access sizes while emitting code.
///
/// This is the *emit-side* view of memory; the separate runtime
/// implementation that backs generated code lives in `os-page`.
pub trait GuestMemory<B: Backend> {
    /// Readable page size in bytes for the address space being reasoned
    /// about during code generation.
    fn emit_page_size(&self) -> u64;
    /// Emit the address translation / data access sequence for a read of
    /// the requested width at the current top-of-stack address.
    fn emit_load(&mut self, backend: &mut B, width: MemWidth, signed: bool);
    /// Emit the address translation / data access sequence for a write of
    /// the requested width at the current top-of-stack address.
    fn emit_store(&mut self, backend: &mut B, width: MemWidth);
}

// ---------------------------------------------------------------------------
// Shared data-model types used by codegen traits
// ---------------------------------------------------------------------------

/// A single guest memory access operation.
#[derive(Debug, Clone, Copy)]
pub struct MemoryAccessOp {
    pub width: MemWidth,
    pub write: bool,
    pub signed: bool,
}

/// One `_dispatch(hash_id, argc, argv)` megabinary entry.
#[derive(Debug, Clone, Copy)]
pub struct DispatchEntry {
    pub hash_id: u64,
    pub guest_address: GuestAddr,
}

/// Redirect hook description used while emitting a PLT stub.
#[derive(Debug, Clone)]
pub struct PltRedirect {
    pub guest_symbol: String,
    pub import_name: String,
    pub import_module: String,
}

/// Specification of all OS glue that must be reserved before emitting a
/// compiled binary (imports, data segments, stub functions, etc.).
#[derive(Debug, Clone, Default)]
pub struct OsGlueSpec {
    pub redirects: Vec<PltRedirect>,
    pub memory: MemorySpec,
}

/// Minimal placeholder for an ABI function surface. `os-abi-spec` owns the
/// fully fleshed-out type; `os-build` only needs it in signatures.
#[derive(Debug, Clone, Default)]
pub struct AbiSpec;

// ---------------------------------------------------------------------------
// Codegen supertraits
// ---------------------------------------------------------------------------

/// Compile-time memory code generator.
pub trait MemoryCodegen<B: Backend> {
    /// Emit a guest memory access sequence for the given operation.
    fn emit_memory_access(&mut self, backend: &mut B, op: &MemoryAccessOp);

    /// Emit page-table / paging-walk setup (required glue, not per-access).
    fn emit_page_table_glue(&mut self, backend: &mut B, spec: &MemorySpec);
}

/// Compile-time syscall/osfuncall code generator.
pub trait SyscallCodegen<B: Backend> {
    /// Emit syscall-number dispatch for the given syscall table.
    fn emit_syscall_dispatch(&mut self, backend: &mut B, table: &SyscallTable);

    /// Emit a host-call stub for a redirected ABI function.
    fn emit_osfuncall_stub(&mut self, backend: &mut B, spec: &AbiSpec, symbol: &str);
}

/// Compile-time redirect stub code generator.
pub trait RedirectCodegen<B: Backend> {
    /// Emit a PLT-hook stub for the given redirect description.
    fn emit_redirect_stub(&mut self, backend: &mut B, redirect: &PltRedirect);
}

// ---------------------------------------------------------------------------
// BuildGlue: the compiler-glue contract
// ---------------------------------------------------------------------------

/// Implemented by concrete recompilers/builders that emit OS emulation
/// glue code for a backend `B`.
pub trait BuildGlue<B: Backend>:
    GuestMemory<B> + MemoryCodegen<B> + SyscallCodegen<B> + RedirectCodegen<B>
{
    /// Emit the `OsOp` sequence that jumps to a guest address.
    ///
    /// The target address is encoded as a backend-agnostic value; the
    /// concrete backend decides how to materialize it (WASM `br_table` / tail
    /// call, JS continuation, native jump table, etc.).
    fn emit_jump_to_address(&mut self, backend: &mut B, target: GuestAddr);

    /// Reserve import slots, stub functions, and data segments described by
    /// `spec`, returning an error if the builder cannot satisfy a request.
    fn reserve_os_glue(&mut self, backend: &mut B, spec: &OsGlueSpec) -> Result<(), GlueError>;

    /// Emit the `_dispatch(hash_id, argc, argv)` entry point as a sequence
    /// of `OsOp`s.
    fn emit_dispatch_entry(&mut self, backend: &mut B, entries: &[DispatchEntry]);

    /// Emit a PLT-hook redirect stub.
    ///
    /// Default implementation delegates to [`RedirectCodegen`].
    fn emit_plt_stub(&mut self, backend: &mut B, redirect: &PltRedirect) {
        <Self as RedirectCodegen<B>>::emit_redirect_stub(self, backend, redirect);
    }

    /// Emit all memory-glue helpers (e.g. `data(addr)`, page-table walk) for
    /// the given `MemorySpec`.
    fn emit_memory_glue(&mut self, backend: &mut B, spec: &MemorySpec) -> Result<(), GlueError>;
}

/// Errors returned by `BuildGlue` reservation / emission.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GlueError {
    NotSupported(alloc::borrow::Cow<'static, str>),
    OutOfSlots,
}

impl core::fmt::Display for GlueError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            GlueError::NotSupported(s) => write!(f, "glue operation not supported: {s}"),
            GlueError::OutOfSlots => write!(f, "out of OS glue slots"),
        }
    }
}


