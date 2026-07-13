//! `os-page` — shared runtime memory/paging traits and data model.
//!
//! This crate is intentionally the *runtime* half of the memory system.
//! The compile-time emitters live in `os-page-codegen` (and `os-build`
//! carries the compile-time `GuestMemory` trait they consume).

#![no_std]
extern crate alloc;

/// High-level memory-system description shared across compile-time and
/// runtime memory layers.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MemorySpec {
    /// log2 of the base page size (e.g. 16 for 64 KiB).
    pub page_size_log2: u8,
    /// Number of page-table levels.
    pub levels: u8,
    /// Width of physical addresses in bits.
    pub physical_address_bits: u8,
}

impl MemorySpec {
    /// The canonical 64 KiB WASM-style page size.
    pub const WASM_64K: Self = Self {
        page_size_log2: 16,
        levels: 2,
        physical_address_bits: 56,
    };

    /// Page size in bytes derived from `page_size_log2`.
    pub fn page_size(&self) -> u64 {
        1u64 << self.page_size_log2
    }
}

/// Width of a memory access, mirrored here so `os-page` consumers need not
/// depend on `os-target-core` unless they are also emitting code.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AccessWidth {
    W8,
    W16,
    W32,
    W64,
    W128,
}

/// Reasons a memory access may fault.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GuestFault {
    OutOfBounds,
    PermissionDenied,
    Unmapped,
}

/// Runtime page-table / address-translation interface.
pub trait PageTable {
    /// Translate a guest virtual address to a physical offset.
    ///
    /// Returns `Err(GuestFault)` on unmapped/protected pages.
    fn translate(&self, vaddr: u64) -> Result<u64, GuestFault>;
}

/// Runtime guest-memory interface.
///
/// This is the *runtime* counterpart to `os_build::GuestMemory<B>`.  Keep the
/// names distinct by importing `os_page::memory::GuestMemory` versus
/// `os_build::GuestMemory`.
pub trait GuestMemory {
    /// Read a value of the given width from the guest address space.
    fn read(&self, addr: u64, width: AccessWidth) -> Result<u64, GuestFault>;
    /// Write a value of the given width to the guest address space.
    fn write(&mut self, addr: u64, width: AccessWidth, value: u64) -> Result<(), GuestFault>;
}

/// Concrete runtime backends will live in `crates/page/os-page/src/backends.rs`.
/// They are added behind feature flags as `@speet` and `@vane` adopt them.
pub mod backends;