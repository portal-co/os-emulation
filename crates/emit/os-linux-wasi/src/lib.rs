//! `os-linux-wasi` — RV64 Linux syscall to WASI preview1 mapping.
//!
//! This crate is backend-neutral: it describes *which* WASI imports are needed
//! and how RV64 Linux syscalls map to them, but it does not touch the host
//! module-target or index-space machinery.  Consumers that need to reserve
//! actual function indices do so in a backend-specific adapter (e.g. the
//! compatibility shim in `@speet` that calls `ModuleTarget::declare_func_import`).

#![no_std]
extern crate alloc;

use os_syscall_emit::{MemoryStore, ParamSource, SyscallEntry, SyscallTable};

/// The 16-byte memory scratch area offset used for iovec marshalling.
pub const IOVEC_SCRATCH_OFFSET: u32 = 0x200;

/// WASI preview1 function indices required by the Linux→WASI mapping.
///
/// The fields hold absolute function indices in whatever index space the
/// consumer is using (e.g. a `ModuleTarget` or a plugin manifest).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WasiImports {
    /// `wasi_snapshot_preview1::fd_write`
    pub fd_write: u32,
    /// `wasi_snapshot_preview1::fd_read`
    pub fd_read: u32,
    /// `wasi_snapshot_preview1::fd_close`
    pub fd_close: u32,
    /// `wasi_snapshot_preview1::proc_exit`
    pub proc_exit: u32,
}

/// Builder for an RV64 Linux → WASI preview1 syscall table.
pub struct LinuxToWasi {
    /// WASI import indices.
    pub imports: WasiImports,
    /// Offset of the 16-byte iovec scratch area.
    pub iovec_scratch_offset: u32,
}

impl LinuxToWasi {
    pub fn new(imports: WasiImports) -> Self {
        Self {
            imports,
            iovec_scratch_offset: IOVEC_SCRATCH_OFFSET,
        }
    }

    /// Build the `SyscallTable` for the RV64 Linux ABI.
    ///
    /// `xn_local(n)` maps a RISC-V register index to the caller's local/index
    /// representing that register.
    pub fn build_table(&self, xn_local: impl Fn(u8) -> u32) -> SyscallTable {
        let a0_local = xn_local(10);
        let a1_local = xn_local(11);
        let a2_local = xn_local(12);

        let mut entries = alloc::vec::Vec::new();

        // Read (syscall 63)
        entries.push((
            63,
            SyscallEntry {
                func_idx: self.imports.fd_read,
                param_map: alloc::vec![
                    ParamSource::LocalI64AsI32(a0_local),
                    ParamSource::ConstI32(self.iovec_scratch_offset as i32),
                    ParamSource::ConstI32(1),
                    ParamSource::ConstI32((self.iovec_scratch_offset + 8) as i32),
                ],
                saves: alloc::vec![],
                result_local: Some(a0_local),
                negate_nonzero_result: true,
                has_return: true,
                terminates: false,
                memory_stores: alloc::vec![
                    MemoryStore {
                        addr: self.iovec_scratch_offset,
                        value_local: a1_local,
                        value_is_i64: true,
                    },
                    MemoryStore {
                        addr: self.iovec_scratch_offset + 4,
                        value_local: a2_local,
                        value_is_i64: true,
                    },
                ],
                load_mem_on_success: Some(self.iovec_scratch_offset + 8),
            },
        ));

        // Write (syscall 64)
        entries.push((
            64,
            SyscallEntry {
                func_idx: self.imports.fd_write,
                param_map: alloc::vec![
                    ParamSource::LocalI64AsI32(a0_local),
                    ParamSource::ConstI32(self.iovec_scratch_offset as i32),
                    ParamSource::ConstI32(1),
                    ParamSource::ConstI32((self.iovec_scratch_offset + 8) as i32),
                ],
                saves: alloc::vec![],
                result_local: Some(a0_local),
                negate_nonzero_result: true,
                has_return: true,
                terminates: false,
                memory_stores: alloc::vec![
                    MemoryStore {
                        addr: self.iovec_scratch_offset,
                        value_local: a1_local,
                        value_is_i64: true,
                    },
                    MemoryStore {
                        addr: self.iovec_scratch_offset + 4,
                        value_local: a2_local,
                        value_is_i64: true,
                    },
                ],
                load_mem_on_success: Some(self.iovec_scratch_offset + 8),
            },
        ));

        // Close (syscall 57)
        entries.push((
            57,
            SyscallEntry {
                func_idx: self.imports.fd_close,
                param_map: alloc::vec![ParamSource::LocalI64AsI32(a0_local)],
                saves: alloc::vec![],
                result_local: Some(a0_local),
                negate_nonzero_result: true,
                has_return: true,
                terminates: false,
                memory_stores: alloc::vec![],
                load_mem_on_success: None,
            },
        ));

        // Exit (syscall 93)
        entries.push((
            93,
            SyscallEntry {
                func_idx: self.imports.proc_exit,
                param_map: alloc::vec![ParamSource::LocalI64AsI32(a0_local)],
                saves: alloc::vec![],
                result_local: None,
                negate_nonzero_result: false,
                has_return: false,
                terminates: true,
                memory_stores: alloc::vec![],
                load_mem_on_success: None,
            },
        ));

        // ExitGroup (syscall 94)
        entries.push((
            94,
            SyscallEntry {
                func_idx: self.imports.proc_exit,
                param_map: alloc::vec![ParamSource::LocalI64AsI32(a0_local)],
                saves: alloc::vec![],
                result_local: None,
                negate_nonzero_result: false,
                has_return: false,
                terminates: true,
                memory_stores: alloc::vec![],
                load_mem_on_success: None,
            },
        ));

        SyscallTable::new(entries)
    }
}