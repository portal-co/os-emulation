//! Concrete runtime memory/paging backends.
//!
//! These are intentionally small, correct implementations that satisfy the
//! shared `GuestMemory` / `PageTable` traits.  More sophisticated behaviour
//! (permissions, coherency, aliasing) is added lazily as consumers need it.

use crate::{AccessWidth, GuestFault, GuestMemory, PageTable};
use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::vec::Vec;

/// Default 64 KiB page size used by all backends in this file.
pub const DEFAULT_PAGE_SIZE: u64 = 65536;

impl AccessWidth {
    /// Width in bytes for the variants that fit in a `u64`.
    pub fn bytes(&self) -> Option<u8> {
        match self {
            AccessWidth::W8 => Some(1),
            AccessWidth::W16 => Some(2),
            AccessWidth::W32 => Some(4),
            AccessWidth::W64 => Some(8),
            AccessWidth::W128 => None,
        }
    }
}

// ── Helpers ─────────────────────────────────────────────────────────────────

fn read_bytes_at(pages: &BTreeMap<u64, Box<[u8; DEFAULT_PAGE_SIZE as usize]>>, addr: u64, n: usize) -> u64 {
    let mut value: u64 = 0;
    for i in 0..n {
        let byte_addr = addr.wrapping_add(i as u64);
        let page = pages.get(&(byte_addr >> 16));
        let byte = page.map(|p| p[(byte_addr & 0xffff) as usize]).unwrap_or(0);
        value |= (byte as u64) << (8 * i);
    }
    value
}

fn write_bytes_at(pages: &mut BTreeMap<u64, Box<[u8; DEFAULT_PAGE_SIZE as usize]>>, addr: u64, n: usize, value: u64) {
    for i in 0..n {
        let byte_addr = addr.wrapping_add(i as u64);
        let page = pages.entry(byte_addr >> 16).or_insert_with(|| Box::new([0u8; DEFAULT_PAGE_SIZE as usize]));
        page[(byte_addr & 0xffff) as usize] = ((value >> (8 * i)) & 0xff) as u8;
    }
}

fn read_bytes_linear(mem: &[u8], addr: u64, n: usize) -> Result<u64, GuestFault> {
    let base = addr as usize;
    let end = base.saturating_add(n);
    if end > mem.len() || end < base {
        return Err(GuestFault::OutOfBounds);
    }
    let mut value: u64 = 0;
    for i in 0..n {
        value |= (mem[base + i] as u64) << (8 * i);
    }
    Ok(value)
}

fn write_bytes_linear(mem: &mut [u8], addr: u64, n: usize, value: u64) -> Result<(), GuestFault> {
    let base = addr as usize;
    let end = base.saturating_add(n);
    if end > mem.len() || end < base {
        return Err(GuestFault::OutOfBounds);
    }
    for i in 0..n {
        mem[base + i] = ((value >> (8 * i)) & 0xff) as u8;
    }
    Ok(())
}

// ── LegacyOnDemand ───────────────────────────────────────────────────────────

/// Vane-compatible legacy memory: 64 KiB pages allocated on first access.
///
/// Reads from unmapped pages return zero; writes allocate the page.
#[derive(Default, Debug, Clone)]
pub struct LegacyOnDemand {
    pages: BTreeMap<u64, Box<[u8; DEFAULT_PAGE_SIZE as usize]>>,
}

impl LegacyOnDemand {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn page_size(&self) -> u64 {
        DEFAULT_PAGE_SIZE
    }

    pub fn get_page(&mut self, page_num: u64) -> &mut [u8; DEFAULT_PAGE_SIZE as usize] {
        self.pages
            .entry(page_num)
            .or_insert_with(|| Box::new([0u8; DEFAULT_PAGE_SIZE as usize]))
    }
}

impl PageTable for LegacyOnDemand {
    /// Legacy mode uses an identity mapping within the guest physical
    /// address space.
    fn translate(&self, vaddr: u64) -> Result<u64, GuestFault> {
        Ok(vaddr)
    }
}

impl GuestMemory for LegacyOnDemand {
    fn read(&self, addr: u64, width: AccessWidth) -> Result<u64, GuestFault> {
        match width.bytes() {
            Some(n) => Ok(read_bytes_at(&self.pages, addr, n as usize)),
            None => Err(GuestFault::OutOfBounds),
        }
    }

    fn write(&mut self, addr: u64, width: AccessWidth, value: u64) -> Result<(), GuestFault> {
        match width.bytes() {
            Some(n) => {
                write_bytes_at(&mut self.pages, addr, n as usize, value);
                Ok(())
            }
            None => Err(GuestFault::OutOfBounds),
        }
    }
}

// ── SharedPageTable ───────────────────────────────────────────────────────

/// Shared (nested) paging translation.
///
/// The page table and security directory live inside an underlying
/// `GuestMemory` (usually a `LegacyOnDemand`).  Translation reads entries
/// from that memory.  This matches the rift/r52x/speet shared-memory model
/// used by `vane-arch`.
#[derive(Clone, Copy)]
pub struct SharedPageTable<'a> {
    /// Underlying memory that holds the page tables.
    pub mem: &'a dyn GuestMemory,
    /// Virtual address of the page-table base.
    pub page_table_vaddr: u64,
    /// Virtual address of the security directory.
    pub security_dir_vaddr: u64,
    /// Shared page size (defaults to 64 KiB).
    pub page_size_log2: u8,
    /// Number of levels (2 for single-level, 3 for multi-level).
    pub levels: u8,
    /// Width of page table entries in bits (32 or 64).
    pub pte_width_bits: u8,
}

impl<'a> core::fmt::Debug for SharedPageTable<'a> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("SharedPageTable")
            .field("page_table_vaddr", &self.page_table_vaddr)
            .field("security_dir_vaddr", &self.security_dir_vaddr)
            .field("page_size_log2", &self.page_size_log2)
            .field("levels", &self.levels)
            .field("pte_width_bits", &self.pte_width_bits)
            .finish_non_exhaustive()
    }
}

impl<'a> SharedPageTable<'a> {
    pub fn new(mem: &'a dyn GuestMemory, page_table_vaddr: u64, security_dir_vaddr: u64) -> Self {
        Self {
            mem,
            page_table_vaddr,
            security_dir_vaddr,
            page_size_log2: 16,
            levels: 2,
            pte_width_bits: 64,
        }
    }

    fn page_size(&self) -> u64 {
        1u64 << self.page_size_log2
    }

    fn read_pte(&self, addr: u64) -> Result<u64, GuestFault> {
        if self.pte_width_bits == 64 {
            self.mem.read(addr, AccessWidth::W64)
        } else {
            self.mem.read(addr, AccessWidth::W32).map(|v| v as u64)
        }
    }

    fn translate_2level(&self, vaddr: u64) -> Result<u64, GuestFault> {
        let page_size = self.page_size();
        let page_offset = vaddr & (page_size - 1);
        let page_num = vaddr >> self.page_size_log2;

        let entry_size = (self.pte_width_bits / 8) as u64;
        let entry_addr = self.page_table_vaddr.wrapping_add(page_num * entry_size);
        let page_pointer = self.read_pte(entry_addr)?;

        if self.pte_width_bits == 64 {
            let sec_idx = page_pointer & 0xFFFF;
            let page_base_low48 = page_pointer >> 16;
            let sec_entry_addr = self.security_dir_vaddr.wrapping_add(sec_idx * 4);
            let sec_entry = self.mem.read(sec_entry_addr, AccessWidth::W32)? as u64;
            let page_base_top16 = sec_entry >> 48;
            let phys_page_num = (page_base_top16 << 48) | page_base_low48;
            Ok((phys_page_num << self.page_size_log2) + page_offset)
        } else {
            let sec_idx = page_pointer & 0xFF;
            let page_base_low24 = page_pointer >> 8;
            let sec_entry_addr = self.security_dir_vaddr.wrapping_add(sec_idx * 4);
            let sec_entry = self.mem.read(sec_entry_addr, AccessWidth::W32)? as u64;
            let page_base_top8 = sec_entry >> 24;
            let phys_page_num = (page_base_top8 << 24) | page_base_low24;
            Ok((phys_page_num << self.page_size_log2) + page_offset)
        }
    }

    fn translate_3level(&self, vaddr: u64) -> Result<u64, GuestFault> {
        let page_offset = vaddr & (self.page_size() - 1);
        let entry_size = (self.pte_width_bits / 8) as u64;

        let l3_idx = (vaddr >> 48) & 0xFFFF;
        let l3_entry_addr = self.page_table_vaddr.wrapping_add(l3_idx * entry_size);
        let l2_table_vaddr = self.read_pte(l3_entry_addr)?;

        let l2_idx = (vaddr >> 32) & 0xFFFF;
        let l2_entry_addr = l2_table_vaddr.wrapping_add(l2_idx * entry_size);
        let l1_table_vaddr = self.read_pte(l2_entry_addr)?;

        let l1_idx = (vaddr >> 16) & 0xFFFF;
        let l1_entry_addr = l1_table_vaddr.wrapping_add(l1_idx * entry_size);
        let page_pointer = self.read_pte(l1_entry_addr)?;

        let (sec_idx, page_base_low) = if self.pte_width_bits == 64 {
            ((page_pointer & 0xFFFF), page_pointer >> 16)
        } else {
            ((page_pointer & 0xFF), page_pointer >> 8)
        };
        let sec_entry_addr = self.security_dir_vaddr.wrapping_add(sec_idx * 4);
        let sec_entry = if self.pte_width_bits == 64 {
            self.mem.read(sec_entry_addr, AccessWidth::W32)? as u64
        } else {
            self.mem.read(sec_entry_addr, AccessWidth::W32)? as u64
        };
        let page_base_top = if self.pte_width_bits == 64 {
            sec_entry >> 48
        } else {
            sec_entry >> 24
        };
        let shift = if self.pte_width_bits == 64 { 48 } else { 24 };
        let phys_page_num = (page_base_top << shift) | page_base_low;
        Ok((phys_page_num << self.page_size_log2) + page_offset)
    }
}

impl<'a> PageTable for SharedPageTable<'a> {
    fn translate(&self, vaddr: u64) -> Result<u64, GuestFault> {
        match self.levels {
            2 => self.translate_2level(vaddr),
            3 => self.translate_3level(vaddr),
            _ => Err(GuestFault::Unmapped),
        }
    }
}

// ── LinearHost ──────────────────────────────────────────────────────────────

/// A simple host‑backed linear memory for the thin runtime / JIT fast path.
///
/// `base` lets the same `Vec<u8>` represent a guest address space that does
/// not start at virtual address zero if desired (future use).
#[derive(Debug, Clone)]
pub struct LinearHost {
    pub memory: Vec<u8>,
    pub base: u64,
}

impl LinearHost {
    pub fn new(size: usize) -> Self {
        Self {
            memory: alloc::vec![0u8; size],
            base: 0,
        }
    }

    pub fn with_base(size: usize, base: u64) -> Self {
        Self {
            memory: alloc::vec![0u8; size],
            base,
        }
    }

    fn offset(&self, addr: u64) -> Result<u64, GuestFault> {
        addr.checked_sub(self.base).ok_or(GuestFault::OutOfBounds)
    }
}

impl PageTable for LinearHost {
    /// Linear host memory is identity mapping relative to `base`.
    fn translate(&self, vaddr: u64) -> Result<u64, GuestFault> {
        self.offset(vaddr)
    }
}

impl GuestMemory for LinearHost {
    fn read(&self, addr: u64, width: AccessWidth) -> Result<u64, GuestFault> {
        let off = self.offset(addr)?;
        match width.bytes() {
            Some(n) => read_bytes_linear(&self.memory, off, n as usize),
            None => Err(GuestFault::OutOfBounds),
        }
    }

    fn write(&mut self, addr: u64, width: AccessWidth, value: u64) -> Result<(), GuestFault> {
        let off = self.offset(addr)?;
        match width.bytes() {
            Some(n) => write_bytes_linear(&mut self.memory, off, n as usize, value),
            None => Err(GuestFault::OutOfBounds),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_on_demand_zero_on_read() {
        let mem = LegacyOnDemand::new();
        assert_eq!(mem.read(0x1234, AccessWidth::W32).unwrap(), 0);
        assert_eq!(mem.read(0x1234, AccessWidth::W8).unwrap(), 0);
    }

    #[test]
    fn legacy_on_demand_write_and_read() {
        let mut mem = LegacyOnDemand::new();
        mem.write(0x1234, AccessWidth::W32, 0xDEAD_BEEF).unwrap();
        assert_eq!(mem.read(0x1234, AccessWidth::W32).unwrap(), 0xDEAD_BEEF);
        assert_eq!(mem.read(0x1235, AccessWidth::W8).unwrap(), 0xBE);
    }

    #[test]
    fn linear_host_bounds() {
        let mut mem = LinearHost::new(1024);
        mem.write(0, AccessWidth::W32, 0xCAFE_BABE).unwrap();
        assert_eq!(mem.read(0, AccessWidth::W32).unwrap(), 0xCAFE_BABE);
        assert!(mem.read(1024, AccessWidth::W8).is_err());
    }

    #[test]
    fn shared_page_table_identity_like_vane() {
        // Set up a legacy backing with a single shared PTE entry at vaddr 0.
        let mut legacy = LegacyOnDemand::new();
        // 64-bit PTE layout: low 16 bits = sec_idx, bits [16:63] = physical page number.
        let page_num = 0x10000u64;
        let pte = (page_num << 16) | 1;
        legacy.write(0x0, AccessWidth::W64, pte).unwrap();
        // Security directory entry at sec_idx 1: top 16 bits of physical page.
        legacy.write(0x1000 + 4, AccessWidth::W32, 0).unwrap();

        let shared = SharedPageTable::new(&legacy, 0, 0x1000);
        assert_eq!(shared.translate(0).unwrap(), 0x10000_0000);
        assert_eq!(
            shared.translate(0x1234).unwrap(),
            0x10000_0000 + 0x1234
        );
    }
}