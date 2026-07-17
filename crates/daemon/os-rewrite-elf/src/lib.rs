//! Rewrites an ELF64 little-endian executable's dynamic section to add a
//! `DT_NEEDED` (and optionally `DT_RUNPATH`) entry, always producing new
//! bytes — the input is never mutated.
//!
//! Shared by libc-based Linux and BSDs: both use the same ELF `.dynamic`
//! mechanism, and neither has a code-signing/library-validation gate
//! analogous to macOS Hardened Runtime, so unlike `os-rewrite-macho` this
//! crate needs no signing step.
//!
//! This is new design work — no prior reference implementation exists (the
//! `sandboxd` reference daemon only rewrites Mach-O; its Linux path uses
//! mount namespaces, not binary rewriting). The approach mirrors the Mach-O
//! rewriter's "reuse existing zero padding" strategy, adapted to ELF's own
//! natural reusable slot:
//!
//! - The dynamic array (`PT_DYNAMIC`) is a sequence of `Elf64_Dyn` entries
//!   terminated by the *first* `DT_NULL` the dynamic linker sees. Some
//!   linkers reserve extra all-zero slots after that terminator (default
//!   segment/alignment padding). To add `k` new entries, this crate
//!   repurposes the current terminator slot as the first new entry and the
//!   `k` all-zero slots immediately after it as the remaining new entries
//!   plus a fresh terminator — never touching anything before the original
//!   terminator.
//! - New strings (the needed name, optionally an `$ORIGIN`-relative
//!   `DT_RUNPATH`) are appended into trailing zero padding after `.dynstr`
//!   (located via `DT_STRTAB`/`DT_STRSZ`), the same way the Mach-O rewriter
//!   reuses padding after a replaced load command.
//!
//! First version handles only the "fits in existing slack" case (both for
//! spare dynamic-array slots and `.dynstr` padding) and `ELFCLASS64`
//! little-endian; true segment regrowth and 32-bit ELF are out of scope
//! until a concrete consumer needs them.

const EI_CLASS_OFFSET: usize = 4;
const EI_DATA_OFFSET: usize = 5;
const ELFCLASS64: u8 = 2;
const ELFDATA2LSB: u8 = 1;

const ET_EXEC: u16 = 2;
const ET_DYN: u16 = 3;

const PT_LOAD: u32 = 1;
const PT_DYNAMIC: u32 = 2;

const DT_NULL: u64 = 0;
const DT_NEEDED: u64 = 1;
const DT_STRTAB: u64 = 5;
const DT_STRSZ: u64 = 10;
const DT_RUNPATH: u64 = 0x1d;

const EHDR_SIZE: u64 = 64;
const PHDR_SIZE: u64 = 56;
const DYN_ENTRY_SIZE: u64 = 16;

pub struct ElfRewriteInput<'a> {
    pub original: &'a [u8],
    /// Shared-object soname or path to add as `DT_NEEDED`.
    pub needed_name: String,
    /// Optional `DT_RUNPATH` to add, e.g. `"$ORIGIN/x"` (ELF's
    /// `@executable_path` equivalent) so the dynamic linker finds
    /// `needed_name` beside the rewritten binary.
    pub rpath_entry: Option<String>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum RewriteError {
    /// Not a 64-bit little-endian `ET_EXEC`/`ET_DYN` ELF file.
    NotExecutableOrPie,
    /// No `PT_DYNAMIC` segment — likely statically linked.
    NoDynamicSection,
    /// Not enough spare zero slots after the dynamic array's terminator,
    /// or not enough trailing zero padding after `.dynstr`. Full
    /// regrowth (extending `PT_LOAD`'s file size) is not implemented in
    /// this first version.
    NoFreeStringTableSpace,
    Malformed(&'static str),
}

impl core::fmt::Display for RewriteError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            RewriteError::NotExecutableOrPie => {
                write!(f, "unsupported ELF: expected a 64-bit little-endian executable or PIE")
            }
            RewriteError::NoDynamicSection => write!(f, "no PT_DYNAMIC segment (statically linked?)"),
            RewriteError::NoFreeStringTableSpace => {
                write!(f, "no free dynamic-array slot or .dynstr padding to grow into")
            }
            RewriteError::Malformed(s) => write!(f, "malformed ELF: {s}"),
        }
    }
}

impl std::error::Error for RewriteError {}

fn read_u16_le(buf: &[u8], off: u64) -> Result<u16, RewriteError> {
    let off = off as usize;
    buf.get(off..off + 2)
        .and_then(|s| s.try_into().ok())
        .map(u16::from_le_bytes)
        .ok_or(RewriteError::Malformed("truncated"))
}

fn read_u32_le(buf: &[u8], off: u64) -> Result<u32, RewriteError> {
    let off = off as usize;
    buf.get(off..off + 4)
        .and_then(|s| s.try_into().ok())
        .map(u32::from_le_bytes)
        .ok_or(RewriteError::Malformed("truncated"))
}

fn read_u64_le(buf: &[u8], off: u64) -> Result<u64, RewriteError> {
    let off = off as usize;
    buf.get(off..off + 8)
        .and_then(|s| s.try_into().ok())
        .map(u64::from_le_bytes)
        .ok_or(RewriteError::Malformed("truncated"))
}

fn write_u64_le(buf: &mut [u8], off: u64, v: u64) {
    buf[off as usize..off as usize + 8].copy_from_slice(&v.to_le_bytes());
}

struct ProgramHeader {
    p_type: u32,
    p_offset: u64,
    p_vaddr: u64,
    p_filesz: u64,
}

fn read_program_headers(input: &[u8]) -> Result<Vec<ProgramHeader>, RewriteError> {
    let e_phoff = read_u64_le(input, 32)?;
    let e_phentsize = read_u16_le(input, 54)? as u64;
    let e_phnum = read_u16_le(input, 56)? as u64;
    if e_phentsize < PHDR_SIZE {
        return Err(RewriteError::Malformed("unexpected phentsize"));
    }
    let mut headers = Vec::with_capacity(e_phnum as usize);
    for i in 0..e_phnum {
        let off = e_phoff + i * e_phentsize;
        headers.push(ProgramHeader {
            p_type: read_u32_le(input, off)?,
            p_offset: read_u64_le(input, off + 8)?,
            p_vaddr: read_u64_le(input, off + 16)?,
            p_filesz: read_u64_le(input, off + 32)?,
        });
    }
    Ok(headers)
}

/// Translates a virtual address to a file offset via the `PT_LOAD` segment
/// that contains it.
fn vaddr_to_file_offset(headers: &[ProgramHeader], vaddr: u64) -> Result<u64, RewriteError> {
    headers
        .iter()
        .filter(|h| h.p_type == PT_LOAD)
        .find(|h| vaddr >= h.p_vaddr && vaddr < h.p_vaddr + h.p_filesz)
        .map(|h| h.p_offset + (vaddr - h.p_vaddr))
        .ok_or(RewriteError::Malformed("DT_STRTAB vaddr not in any PT_LOAD segment"))
}

struct DynamicSection {
    /// File offset of the dynamic array.
    offset: u64,
    /// File offset of the first `DT_NULL` entry (the logical terminator).
    terminator_offset: u64,
    /// End of the `PT_DYNAMIC` segment's file bytes (`offset + filesz`).
    segment_end: u64,
    strtab_file_offset: u64,
    strsz: u64,
}

fn find_dynamic_section(input: &[u8], headers: &[ProgramHeader]) -> Result<DynamicSection, RewriteError> {
    let dyn_seg = headers
        .iter()
        .find(|h| h.p_type == PT_DYNAMIC)
        .ok_or(RewriteError::NoDynamicSection)?;

    let mut strtab_vaddr = None;
    let mut strsz = None;
    let mut terminator_offset = None;
    let mut off = dyn_seg.p_offset;
    let seg_end = dyn_seg.p_offset + dyn_seg.p_filesz;
    while off + DYN_ENTRY_SIZE <= seg_end {
        let tag = read_u64_le(input, off)?;
        let val = read_u64_le(input, off + 8)?;
        if tag == DT_NULL {
            terminator_offset = Some(off);
            break;
        }
        if tag == DT_STRTAB {
            strtab_vaddr = Some(val);
        } else if tag == DT_STRSZ {
            strsz = Some(val);
        }
        off += DYN_ENTRY_SIZE;
    }

    let terminator_offset = terminator_offset.ok_or(RewriteError::Malformed("dynamic array has no DT_NULL terminator"))?;
    let strtab_vaddr = strtab_vaddr.ok_or(RewriteError::Malformed("no DT_STRTAB"))?;
    let strsz = strsz.ok_or(RewriteError::Malformed("no DT_STRSZ"))?;
    let strtab_file_offset = vaddr_to_file_offset(headers, strtab_vaddr)?;

    Ok(DynamicSection {
        offset: dyn_seg.p_offset,
        terminator_offset,
        segment_end: seg_end,
        strtab_file_offset,
        strsz,
    })
}

/// Rewrite `input.original`'s dynamic section to add `DT_NEEDED` (and
/// optionally `DT_RUNPATH`). Returns the new executable bytes; never
/// mutates `input.original`.
pub fn rewrite_elf(input: &ElfRewriteInput<'_>) -> Result<Vec<u8>, RewriteError> {
    let original = input.original;
    if original.len() < EHDR_SIZE as usize
        || &original[0..4] != b"\x7fELF"
        || original[EI_CLASS_OFFSET] != ELFCLASS64
        || original[EI_DATA_OFFSET] != ELFDATA2LSB
    {
        return Err(RewriteError::NotExecutableOrPie);
    }
    let e_type = read_u16_le(original, 16)?;
    if e_type != ET_EXEC && e_type != ET_DYN {
        return Err(RewriteError::NotExecutableOrPie);
    }

    let headers = read_program_headers(original)?;
    let dyn_section = find_dynamic_section(original, &headers)?;

    let entries_needed: u64 = 1 + input.rpath_entry.is_some() as u64;
    if dyn_section.terminator_offset + DYN_ENTRY_SIZE * (entries_needed + 1) > dyn_section.segment_end {
        return Err(RewriteError::NoFreeStringTableSpace);
    }
    for i in 1..=entries_needed {
        let slot = dyn_section.terminator_offset + DYN_ENTRY_SIZE * i;
        if read_u64_le(original, slot)? != 0 || read_u64_le(original, slot + 8)? != 0 {
            return Err(RewriteError::NoFreeStringTableSpace);
        }
    }

    // Stage new strings after any bytes .dynstr already uses, requiring
    // that range to be entirely zero (unused) in the original file.
    let strings_start = dyn_section.strtab_file_offset + dyn_section.strsz;
    let mut new_strings = Vec::new();
    new_strings.extend_from_slice(input.needed_name.as_bytes());
    new_strings.push(0);
    let needed_str_off = dyn_section.strsz;
    let rpath_str_off = dyn_section.strsz + new_strings.len() as u64;
    if let Some(rpath) = &input.rpath_entry {
        new_strings.extend_from_slice(rpath.as_bytes());
        new_strings.push(0);
    }
    let strings_end = strings_start + new_strings.len() as u64;
    if strings_end > original.len() as u64 {
        return Err(RewriteError::NoFreeStringTableSpace);
    }
    let padding = &original[strings_start as usize..strings_end as usize];
    if padding.iter().any(|&b| b != 0) {
        return Err(RewriteError::NoFreeStringTableSpace);
    }

    let mut out = original.to_vec();
    out[strings_start as usize..strings_end as usize].copy_from_slice(&new_strings);

    write_u64_le(&mut out, dyn_section.terminator_offset, DT_NEEDED);
    write_u64_le(&mut out, dyn_section.terminator_offset + 8, needed_str_off);
    if input.rpath_entry.is_some() {
        let slot = dyn_section.terminator_offset + DYN_ENTRY_SIZE;
        write_u64_le(&mut out, slot, DT_RUNPATH);
        write_u64_le(&mut out, slot + 8, rpath_str_off);
    }
    // The slot immediately after the newly written entries is already all
    // zero (checked above) and becomes the new DT_NULL terminator as-is.

    // DT_STRSZ must grow to cover the appended strings.
    let mut off = dyn_section.offset;
    loop {
        let tag = read_u64_le(&out, off)?;
        if tag == DT_STRSZ {
            write_u64_le(&mut out, off + 8, dyn_section.strsz + new_strings.len() as u64);
            break;
        }
        if tag == DT_NULL {
            break;
        }
        off += DYN_ENTRY_SIZE;
    }

    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds a minimal ELF64 LE `ET_EXEC` with one `PT_LOAD` segment
    /// covering the whole file (so `.dynstr`'s vaddr trivially maps back to
    /// its file offset) and a `PT_DYNAMIC` segment with `DT_STRTAB`,
    /// `DT_STRSZ`, and `extra_null_slots` spare all-zero entries after the
    /// terminator. `dynstr_padding` zero bytes follow `.dynstr`'s
    /// `strtab_len` used bytes.
    fn build_test_elf(strtab_len: u64, dynstr_padding: u64, extra_null_slots: u64) -> Vec<u8> {
        const LOAD_VADDR: u64 = 0x1000;

        // Layout: [ehdr][phdr x2][dynstr (strtab_len + padding)][dynamic array].
        let ehdr_end = EHDR_SIZE;
        let phdr_off = ehdr_end;
        let phdr_end = phdr_off + PHDR_SIZE * 2;
        let dynstr_off = phdr_end;
        let dynstr_used_end = dynstr_off + strtab_len;
        let dynstr_end = dynstr_used_end + dynstr_padding;
        let dyn_off = dynstr_end;

        // dynamic array: DT_STRTAB, DT_STRSZ, DT_NULL, then extra_null_slots
        // more all-zero (0,0) slots.
        let dyn_entry_count = 3 + extra_null_slots;
        let dyn_filesz = DYN_ENTRY_SIZE * dyn_entry_count;
        let total_len = dyn_off + dyn_filesz;

        let mut buf = vec![0u8; total_len as usize];
        buf[0..4].copy_from_slice(b"\x7fELF");
        buf[EI_CLASS_OFFSET] = ELFCLASS64;
        buf[EI_DATA_OFFSET] = ELFDATA2LSB;
        buf[16..18].copy_from_slice(&ET_EXEC.to_le_bytes());
        buf[32..40].copy_from_slice(&phdr_off.to_le_bytes()); // e_phoff
        buf[54..56].copy_from_slice(&(PHDR_SIZE as u16).to_le_bytes()); // e_phentsize
        buf[56..58].copy_from_slice(&2u16.to_le_bytes()); // e_phnum

        // PT_LOAD covering the whole file, vaddr == file offset (+LOAD_VADDR base).
        let load_off = phdr_off;
        buf[load_off as usize..load_off as usize + 4].copy_from_slice(&PT_LOAD.to_le_bytes());
        write_u64_le(&mut buf, load_off + 8, 0); // p_offset
        write_u64_le(&mut buf, load_off + 16, LOAD_VADDR); // p_vaddr
        write_u64_le(&mut buf, load_off + 32, total_len); // p_filesz

        // PT_DYNAMIC.
        let pdyn_off = phdr_off + PHDR_SIZE;
        buf[pdyn_off as usize..pdyn_off as usize + 4].copy_from_slice(&PT_DYNAMIC.to_le_bytes());
        write_u64_le(&mut buf, pdyn_off + 8, dyn_off); // p_offset
        write_u64_le(&mut buf, pdyn_off + 16, LOAD_VADDR + dyn_off); // p_vaddr
        write_u64_le(&mut buf, pdyn_off + 32, dyn_filesz); // p_filesz

        // Dynamic array entries.
        write_u64_le(&mut buf, dyn_off, DT_STRTAB);
        write_u64_le(&mut buf, dyn_off + 8, LOAD_VADDR + dynstr_off);
        write_u64_le(&mut buf, dyn_off + DYN_ENTRY_SIZE, DT_STRSZ);
        write_u64_le(&mut buf, dyn_off + DYN_ENTRY_SIZE + 8, strtab_len);
        // Terminator (and remaining extra_null_slots) already zero from `vec![0; ...]`.

        // Fill the "used" part of dynstr with a non-zero byte (an existing
        // string) so only the padding region is actually zero.
        for b in buf[dynstr_off as usize..dynstr_used_end as usize].iter_mut() {
            *b = b'a';
        }

        buf
    }

    #[test]
    fn rejects_non_elf() {
        let input = ElfRewriteInput {
            original: &[0u8; 64],
            needed_name: "libfoo.so".into(),
            rpath_entry: None,
        };
        assert_eq!(rewrite_elf(&input), Err(RewriteError::NotExecutableOrPie));
    }

    #[test]
    fn rejects_statically_linked_binary() {
        // Valid ELF header/PT_LOAD but no PT_DYNAMIC.
        let mut buf = build_test_elf(8, 32, 0);
        // Corrupt PT_DYNAMIC's p_type to something else (e.g. PT_NOTE = 4).
        let phdr_off = EHDR_SIZE;
        let pdyn_off = phdr_off + PHDR_SIZE;
        buf[pdyn_off as usize..pdyn_off as usize + 4].copy_from_slice(&4u32.to_le_bytes());
        let input = ElfRewriteInput {
            original: &buf,
            needed_name: "libfoo.so".into(),
            rpath_entry: None,
        };
        assert_eq!(rewrite_elf(&input), Err(RewriteError::NoDynamicSection));
    }

    #[test]
    fn adds_dt_needed_when_slack_available() {
        let original = build_test_elf(8, 32, 1);
        let input = ElfRewriteInput {
            original: &original,
            needed_name: "libshim.so".into(),
            rpath_entry: None,
        };
        let out = rewrite_elf(&input).expect("rewrite should succeed");
        assert_ne!(out, original);
        assert_eq!(original, build_test_elf(8, 32, 1), "input must be untouched");

        // Walk the dynamic array in `out` looking for our new DT_NEEDED.
        // DT_STRTAB always appears before DT_NEEDED here (rewrite_elf only
        // ever appends at/after the original terminator), so a single
        // forward pass sees it first.
        let headers = read_program_headers(&out).unwrap();
        let dyn_seg = headers.iter().find(|h| h.p_type == PT_DYNAMIC).unwrap();
        let mut off = dyn_seg.p_offset;
        let mut found_name = None;
        let mut strtab_vaddr = None;
        loop {
            let tag = read_u64_le(&out, off).unwrap();
            let val = read_u64_le(&out, off + 8).unwrap();
            if tag == DT_NULL {
                break;
            }
            if tag == DT_STRTAB {
                strtab_vaddr = Some(val);
            }
            if tag == DT_NEEDED {
                let strtab_file_off = vaddr_to_file_offset(&headers, strtab_vaddr.unwrap()).unwrap();
                let name_off = (strtab_file_off + val) as usize;
                let end = out[name_off..].iter().position(|&b| b == 0).unwrap();
                found_name = Some(String::from_utf8(out[name_off..name_off + end].to_vec()).unwrap());
            }
            off += DYN_ENTRY_SIZE;
        }
        assert_eq!(found_name.as_deref(), Some("libshim.so"));
    }

    #[test]
    fn adds_dt_needed_and_dt_runpath_together() {
        let original = build_test_elf(8, 64, 2);
        let input = ElfRewriteInput {
            original: &original,
            needed_name: "libshim.so".into(),
            rpath_entry: Some("$ORIGIN/x".into()),
        };
        let out = rewrite_elf(&input).expect("rewrite should succeed");

        let headers = read_program_headers(&out).unwrap();
        let dyn_seg = headers.iter().find(|h| h.p_type == PT_DYNAMIC).unwrap();
        let mut off = dyn_seg.p_offset;
        let mut tags = Vec::new();
        loop {
            let tag = read_u64_le(&out, off).unwrap();
            if tag == DT_NULL {
                break;
            }
            tags.push(tag);
            off += DYN_ENTRY_SIZE;
        }
        assert!(tags.contains(&DT_NEEDED));
        assert!(tags.contains(&DT_RUNPATH));
    }

    #[test]
    fn fails_without_enough_spare_dynamic_slots() {
        let original = build_test_elf(8, 32, 0); // no spare slots at all
        let input = ElfRewriteInput {
            original: &original,
            needed_name: "libshim.so".into(),
            rpath_entry: None,
        };
        assert_eq!(rewrite_elf(&input), Err(RewriteError::NoFreeStringTableSpace));
    }

    #[test]
    fn fails_without_enough_dynstr_padding() {
        let original = build_test_elf(8, 0, 1); // no dynstr padding at all
        let input = ElfRewriteInput {
            original: &original,
            needed_name: "libshim.so".into(),
            rpath_entry: None,
        };
        assert_eq!(rewrite_elf(&input), Err(RewriteError::NoFreeStringTableSpace));
    }
}
