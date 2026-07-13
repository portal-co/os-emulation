//! Async variants of the OS emulation surface.
//!
//! `os-ctx` defines the synchronous [`os_ctx::Ctx`] / [`os_ctx::OS`] traits.
//! This crate adds `AsyncCtx`, `AsyncOS`, and `AsyncHostApi`, generalized from
//! the pattern `vane-arch` already uses for [`AsyncStackHost`]: register and
//! memory accesses may return a `Future`, and ecalls are dispatched through an
//! async OS personality.

#![no_std]

use os_ctx::{Arg, ArgCell};

/// Per-call asynchronous guest state view.
///
/// Mirrors [`os_ctx::Ctx`], but every access is `async`. Implementations can
/// back state onto a remote host, a slow path, or a yield point.
pub trait AsyncCtx {
    /// Read guest virtual memory into `cell`.
    async fn read(&mut self, addr: u64, cell: &mut (dyn ArgCell + '_));
    /// Write `cell` to guest virtual memory.
    async fn write(&mut self, addr: u64, cell: &mut (dyn Arg + '_));
    /// Read guest register `idx` into `cell`.
    async fn reg(&mut self, idx: u8, cell: &mut (dyn ArgCell + '_));
    /// Write `cell` into guest register `idx`.
    async fn set_reg(&mut self, idx: u8, cell: &mut (dyn Arg + '_));
}

/// Asynchronous OS personality.
///
/// Mirrors [`os_ctx::OS`]; ecalls and OS-managed function calls are `async`.
pub trait AsyncOS<C: AsyncCtx + ?Sized> {
    /// Handle a guest syscall using `ctx` for register/memory access.
    async fn syscall(&mut self, ctx: &mut C) -> i64;
    /// Handle a call to an OS-managed function at `addr`.
    async fn osfuncall(&mut self, addr: u64, ctx: &mut C) -> i64;
}

/// Asynchronous host API boundary.
///
/// Typically implemented by the thin runtime or a remote handler. `nr` and
/// `args` correspond to the guest syscall number and registers; the returned
/// value is a Linux-style result (negative for `-errno`, non-negative for
/// success).
pub trait AsyncHostApi {
    /// Dispatch a syscall to the host.
    async fn syscall(&mut self, nr: u64, args: &[u64]) -> i64;
}