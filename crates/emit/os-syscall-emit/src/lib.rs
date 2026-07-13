//! `os-syscall-emit` — generic, backend-neutral syscall dispatch tables.
//!
//! This crate owns the data model that every OS-emulation backend uses to
//! describe a syscall: the argument sources, saves, memory stores, result
//! handling, and termination behaviour.  It deliberately contains **no**
//! backend-specific rendering — that lives in `speet-syscall` (WASM inline
//! dispatch), future native backends, etc.

#![no_std]
extern crate alloc;

use alloc::vec::Vec;

// ── Param source ────────────────────────────────────────────────────────────

/// Describes how to produce one parameter for a handler function.
#[derive(Debug, Clone)]
pub enum ParamSource {
    /// Read an `i64` local and wrap it to `i32` for the handler.
    LocalI64AsI32(u32),
    /// Read an `i32` local directly (no truncation needed).
    LocalI32(u32),
    /// Push a constant `i32` value (e.g. a fixed fd or flags value).
    ConstI32(i32),
    /// Push a constant `i64` value.
    ConstI64(i64),
}

// ── Save pair ─────────────────────────────────────────────────────────────────

/// A single save operation: spill a local to a global before the call.
#[derive(Debug, Clone)]
pub struct SavePair {
    /// Source local index (read with `local.get`).
    pub local_idx: u32,
    /// Destination global index (written with `global.set`).
    pub global_idx: u32,
}

// ── Memory store ──────────────────────────────────────────────────────────────

/// Writes a local value to a specific linear-memory address before the call.
#[derive(Debug, Clone)]
pub struct MemoryStore {
    /// Memory address to write to.
    pub addr: u32,
    /// Source local index (read with `local.get`).
    pub value_local: u32,
    /// If `true`, the local has type `i64` and must be wrapped to `i32` before storing.
    pub value_is_i64: bool,
}

// ── SyscallEntry ─────────────────────────────────────────────────────────────

/// One handler for a specific Linux syscall number.
#[derive(Debug, Clone)]
pub struct SyscallEntry {
    /// Backend-specific function/index of the handler (e.g. a WASI import).
    pub func_idx: u32,
    /// How to produce each parameter the handler expects.
    pub param_map: Vec<ParamSource>,
    /// Globals to spill before marshalling handler params.
    pub saves: Vec<SavePair>,
    /// If `Some(local)`: store the handler's result value into this guest local.
    pub result_local: Option<u32>,
    /// If `true`, negate a non-zero return value before storing it.
    pub negate_nonzero_result: bool,
    /// If `true`, the handler returns a value that needs to be stashed or dropped.
    pub has_return: bool,
    /// If `true`, this syscall never returns.
    pub terminates: bool,
    /// Memory writes to perform before marshalling handler params.
    pub memory_stores: Vec<MemoryStore>,
    /// If `Some(offset)`: load a value from this memory offset on success.
    pub load_mem_on_success: Option<u32>,
}

// ── SyscallTable ─────────────────────────────────────────────────────────────

/// Complete dispatch table: sorted list of `(syscall_number, SyscallEntry)`.
pub struct SyscallTable {
    entries: Vec<(u64, SyscallEntry)>,
}

impl SyscallTable {
    /// Construct from an unsorted slice of `(syscall_number, entry)` pairs.
    pub fn new(mut entries: Vec<(u64, SyscallEntry)>) -> Self {
        entries.sort_unstable_by_key(|(n, _)| *n);
        Self { entries }
    }

    /// Return the sorted entries slice.
    pub fn entries(&self) -> &[(u64, SyscallEntry)] {
        &self.entries
    }
}