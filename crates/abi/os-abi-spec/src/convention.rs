//! Minimal calling-convention description used to marshal arguments and
//! results around redirected calls.
//!
//! This is intentionally small and backend-agnostic. Native arch crates
//! (e.g. Speet's x86_64/aarch64 recompilers) supply per-symbol conventions
//! via higher-level registries, and the OS-emulation layer reasons about
//! only the local slots.

use alloc::vec::Vec;

/// Description of argument/result local-slot assignments for one
/// redirected call.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CallingConvention {
    /// Local indices (register slots) holding the callee's arguments, in
    /// order.
    pub arg_locals: Vec<u32>,
    /// Per-argument: whether the register's (64-bit) value must be narrowed
    /// with `i32.wrap_i64` before the call.
    pub arg_wrap_i32: Vec<bool>,
    /// Local index the return value should be stored into, if any.
    pub result_local: Option<u32>,
    /// Whether the call's WASM result is `i32` and must be widened with
    /// `i64.extend_i32_u` before storing into `result_local`.
    pub result_extend_i32: bool,
}

impl CallingConvention {
    /// Whether argument index `i` needs narrowing.
    pub fn wraps_i32(&self, i: usize) -> bool {
        self.arg_wrap_i32.get(i).copied().unwrap_or(false)
    }
}
