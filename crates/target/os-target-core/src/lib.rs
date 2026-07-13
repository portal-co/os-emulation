//! Speet-neutral, OS-emulation target abstractions.
//!
//! `os-target-core` defines the `OsOp` stack-machine IR and the `Backend`
//! trait that concrete code generators (WASM, JavaScript, native, LLVM)
//! implement. OS-layer traits in `os-build`, `os-page-codegen`, and
//! `os-abi-codegen` emit `OsOp` operations onto a `Backend` instead of
//! returning backend-specific associated types.

#![no_std]

extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;

/// Guest virtual or physical address.
pub type GuestAddr = u64;

/// Width of a memory access.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MemWidth {
    W8,
    W16,
    W32,
    W64,
    W128,
}

/// Explicit stack-machine operations used to express guest OS glue.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OsOp {
    /// Push an unsigned 64-bit scalar onto the value stack.
    PushU64(u64),
    /// Push an unsigned 32-bit scalar onto the value stack.
    PushU32(u32),
    /// Pop the top value from the stack.
    Pop,
    /// Load a value of the given width from guest memory at the address
    /// currently on top of the value stack.
    Load { width: MemWidth, signed: bool },
    /// Store a value of the given width to guest memory at the address
    /// currently on top of the value stack; the value and address are
    /// expected in stack order (value pushed first, address on top).
    Store { width: MemWidth },
    /// Guest syscall / host-call control transfer. The `may_await` flag is
    /// set when the backend must support async host delegation.
    Ecall { may_await: bool },
    /// Unconditional branch to a guest address.
    Jump { target: GuestAddr },
    /// Stop with a fault/trap.
    Trap,
    /// Tail-call a host/guest helper identified by a symbolic label.
    TailCall { helper: String },
}

/// Target backend that consumes [`OsOp`] operations.
pub trait Backend: Sized {
    /// Emit one stack operation.
    ///
    /// The backend mutates its own internal state (byte buffer, instruction
    /// counter, string buffer, etc.). It never exposes backend-specific
    /// handles back to the caller.
    fn op(&mut self, op: OsOp);

    /// Optional: signal that a block of OS-glue operations is complete and
    /// the backend may finalize bookkeeping for this unit.
    ///
    /// The default implementation is a no-op.
    fn finish(&mut self) {}
}

impl Backend for Vec<OsOp> {
    fn op(&mut self, op: OsOp) {
        self.push(op);
    }
}

/// Capability subtrait for backends that emit native machine code.
///
/// Such backends expose native registers, direct memory access, and native
/// function call/return enough to implement OS glue without relying on a
/// managed runtime or WASM engine. The actual register/memory emission is
/// performed by the implementor; this trait merely marks the capability.
pub trait NativeBackend: Backend {}

/// Capability subtrait for backends that emit WebAssembly or WASM-derived
/// code, exposing locals, imports, calls, and memory control.
pub trait WasmBackend: Backend {}

/// Capability subtrait for backends that support WASM-GC features such as
/// structs, arrays, references, and casts.
pub trait WasmGcBackend: WasmBackend {}

/// Capability subtrait for backends that emit JavaScript or a JavaScript
/// interface, exposing JS string literals, helper calls, `await`, and property
/// access.
pub trait JsBackend: Backend {}

/// Capability subtrait for backends that emit direct JVM/DEX/ECMAScript-like
/// bytecode (future).
pub trait JadeByteBackend: Backend {}
