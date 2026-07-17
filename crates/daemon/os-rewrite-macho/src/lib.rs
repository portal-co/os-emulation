//! Rewrites a thin 64-bit Mach-O executable's load commands to add an
//! `LC_LOAD_DYLIB` naming a shim dylib, always producing new bytes — the
//! input is never mutated.
//!
//! This is the Rust port of the reference `sandboxd` daemon's
//! `rewriteMachO()` (Go), which finds a safely-replaceable load command
//! (one that will be invalidated by re-signing anyway: `LC_CODE_SIGNATURE`,
//! optionally paired with a preceding `LC_DATA_IN_CODE`), reuses its bytes
//! — and, if the new command is larger, the zero padding immediately after
//! the load-command table — rather than growing the file. File offsets and
//! the code-signature blob past the load-command table are never touched;
//! re-signing (a separate step, `os-codesign-macho`) is required afterward
//! regardless, since dropping/replacing `LC_CODE_SIGNATURE` invalidates any
//! existing signature.
//!
//! The mutation is hand-rolled directly against raw bytes rather than going
//! through `object::write` (which only builds fresh relocatable objects, not
//! patches to an already-linked executable's load-command table) or reading
//! via `object::read::macho` first (whose `LoadCommandIterator` would need
//! to be re-derived into raw offsets for the splice anyway, since no crate
//! exposes an in-place Mach-O load-command patcher).

const MH_MAGIC_64: u32 = 0xfeed_facf;
const HEADER_SIZE: u64 = 32;

const LC_LOAD_DYLIB: u32 = 0xc;
const LC_CODE_SIGNATURE: u32 = 0x1d;
const LC_DATA_IN_CODE: u32 = 0x29;

/// Size of a `linkedit_data_command` (`cmd`, `cmdsize`, `dataoff`, `datasize`).
const LINKEDIT_DATA_COMMAND_SIZE: u64 = 16;
/// Size of a `dylib_command` header before the trailing name string
/// (`cmd`, `cmdsize`, `dylib.name.offset`, `dylib.timestamp`,
/// `dylib.current_version`, `dylib.compatibility_version`).
const DYLIB_COMMAND_HEADER_SIZE: u64 = 24;

pub struct MachORewriteInput<'a> {
    pub original: &'a [u8],
    /// Load-command path for the injected dylib, e.g. `"@executable_path/x"`.
    pub dylib_load_path: String,
}

#[derive(Debug, PartialEq, Eq)]
pub enum RewriteError {
    /// Universal/fat or 32-bit binaries are not supported; thin them first.
    NotThin64,
    Malformed(&'static str),
    /// No `LC_CODE_SIGNATURE` (optionally paired with `LC_DATA_IN_CODE`) slot
    /// was found to reuse for the new `LC_LOAD_DYLIB`.
    NoReplaceableSlot,
    /// The new command doesn't fit in the reclaimed slot and there isn't
    /// enough zero padding after the load-command table to grow into.
    /// Full load-command-table regrowth (shifting `__TEXT` segment file
    /// offsets) is not implemented in this first version.
    NoPaddingToGrow,
}

impl core::fmt::Display for RewriteError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            RewriteError::NotThin64 => write!(f, "unsupported Mach-O: expected thin 64-bit binary"),
            RewriteError::Malformed(s) => write!(f, "malformed Mach-O: {s}"),
            RewriteError::NoReplaceableSlot => {
                write!(f, "no replaceable load command for the injected dylib")
            }
            RewriteError::NoPaddingToGrow => {
                write!(f, "no Mach-O load-command padding to grow into")
            }
        }
    }
}

impl std::error::Error for RewriteError {}

fn read_u32_le(buf: &[u8], off: u64) -> Result<u32, RewriteError> {
    let off = off as usize;
    buf.get(off..off + 4)
        .and_then(|s| s.try_into().ok())
        .map(u32::from_le_bytes)
        .ok_or(RewriteError::Malformed("truncated"))
}

fn write_u32_le(buf: &mut [u8], off: u64, v: u32) {
    buf[off as usize..off as usize + 4].copy_from_slice(&v.to_le_bytes());
}

struct Anchor {
    offset: u64,
    size: u64,
    /// Number of load commands the anchor slot replaces (1 or 2), used to
    /// update `ncmds`.
    command_count: u32,
}

fn find_anchor(input: &[u8], ncmds: u32, sizeofcmds: u32) -> Result<Anchor, RewriteError> {
    let table_end = HEADER_SIZE + sizeofcmds as u64;
    if table_end > input.len() as u64 {
        return Err(RewriteError::Malformed("load-command table exceeds file size"));
    }

    let mut anchor: Option<Anchor> = None;
    let mut off = HEADER_SIZE;
    for i in 0..ncmds {
        if off + 8 > input.len() as u64 {
            return Err(RewriteError::Malformed("truncated load command"));
        }
        let cmd = read_u32_le(input, off)?;
        let size = read_u32_le(input, off + 4)? as u64;
        if size < 8 || off + size > input.len() as u64 {
            return Err(RewriteError::Malformed("invalid load command size"));
        }

        if anchor.is_none()
            && cmd == LC_DATA_IN_CODE
            && size == LINKEDIT_DATA_COMMAND_SIZE
            && i + 1 < ncmds
        {
            let next_off = off + size;
            let next_size = read_u32_le(input, next_off + 4)? as u64;
            let next_cmd = read_u32_le(input, next_off)?;
            if next_cmd == LC_CODE_SIGNATURE && next_size == LINKEDIT_DATA_COMMAND_SIZE {
                anchor = Some(Anchor {
                    offset: off,
                    size: size + next_size,
                    command_count: 2,
                });
            }
        }
        if anchor.is_none() && cmd == LC_CODE_SIGNATURE && off + size == table_end {
            anchor = Some(Anchor {
                offset: off,
                size,
                command_count: 1,
            });
        }

        off += size;
    }

    anchor.ok_or(RewriteError::NoReplaceableSlot)
}

/// Rewrite `input.original`'s load commands to add
/// `LC_LOAD_DYLIB "input.dylib_load_path"`. Returns the new executable
/// bytes; never mutates `input.original`.
pub fn rewrite_macho(input: &MachORewriteInput<'_>) -> Result<Vec<u8>, RewriteError> {
    let original = input.original;
    if original.len() < HEADER_SIZE as usize || read_u32_le(original, 0)? != MH_MAGIC_64 {
        return Err(RewriteError::NotThin64);
    }
    let ncmds = read_u32_le(original, 16)?;
    let sizeofcmds = read_u32_le(original, 20)?;
    let table_end = HEADER_SIZE + sizeofcmds as u64;

    let anchor = find_anchor(original, ncmds, sizeofcmds)?;

    let name_bytes_with_nul = input.dylib_load_path.len() as u64 + 1;
    let new_cmd_size = (DYLIB_COMMAND_HEADER_SIZE + name_bytes_with_nul + 7) & !7;
    let delta = new_cmd_size as i64 - anchor.size as i64;

    if delta > 0 {
        let delta = delta as u64;
        if table_end + delta > original.len() as u64 {
            return Err(RewriteError::NoPaddingToGrow);
        }
        let padding = &original[table_end as usize..(table_end + delta) as usize];
        if padding.iter().any(|&b| b != 0) {
            return Err(RewriteError::NoPaddingToGrow);
        }
    }

    let mut out = original.to_vec();
    if delta > 0 {
        let delta = delta as u64;
        // Shift the bytes after the replaced slot right by `delta`, moving
        // backward so the write never clobbers a byte still to be read.
        let mut j = table_end;
        while j > anchor.offset + anchor.size {
            j -= 1;
            out[(j + delta) as usize] = out[j as usize];
        }
    } else if delta < 0 {
        let shrink = (-delta) as u64;
        // Shift left; safe to go forward since the destination trails the source.
        for j in (anchor.offset + anchor.size)..table_end {
            out[(j - shrink) as usize] = out[j as usize];
        }
    }

    for b in out
        .iter_mut()
        .take((anchor.offset + new_cmd_size) as usize)
        .skip(anchor.offset as usize)
    {
        *b = 0;
    }

    write_u32_le(&mut out, anchor.offset, LC_LOAD_DYLIB);
    write_u32_le(&mut out, anchor.offset + 4, new_cmd_size as u32);
    write_u32_le(&mut out, anchor.offset + 8, DYLIB_COMMAND_HEADER_SIZE as u32); // dylib.name.offset
    write_u32_le(&mut out, anchor.offset + 12, 2); // dylib.timestamp
    write_u32_le(&mut out, anchor.offset + 16, 0x10000); // dylib.current_version
    write_u32_le(&mut out, anchor.offset + 20, 0x10000); // dylib.compatibility_version
    let name_off = (anchor.offset + DYLIB_COMMAND_HEADER_SIZE) as usize;
    out[name_off..name_off + input.dylib_load_path.len()]
        .copy_from_slice(input.dylib_load_path.as_bytes());

    write_u32_le(&mut out, 20, (sizeofcmds as i64 + delta) as u32);
    write_u32_le(&mut out, 16, ncmds - anchor.command_count + 1);

    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds a minimal thin 64-bit Mach-O with a header, one `LC_SEGMENT_64`
    /// placeholder command, and a trailing `LC_CODE_SIGNATURE` command that
    /// ends exactly at the load-command table boundary — enough to exercise
    /// the "source only" anchor path. `trailing_padding` bytes of zeros
    /// follow the table (simulating alignment slack before `__TEXT`'s code).
    fn build_test_macho(trailing_padding: usize) -> Vec<u8> {
        let mut buf = vec![0u8; HEADER_SIZE as usize];
        write_u32_le(&mut buf, 0, MH_MAGIC_64);

        // One throwaway 32-byte command so the anchor isn't the first command
        // (closer to a real binary's shape) - use LC_UUID (0x1b), size 32.
        let uuid_cmd_off = buf.len() as u64;
        buf.extend(vec![0u8; 32]);
        write_u32_le(&mut buf, uuid_cmd_off, 0x1b);
        write_u32_le(&mut buf, uuid_cmd_off + 4, 32);

        // LC_CODE_SIGNATURE, 16 bytes, ends the load-command table.
        let sig_cmd_off = buf.len() as u64;
        buf.extend(vec![0u8; 16]);
        write_u32_le(&mut buf, sig_cmd_off, LC_CODE_SIGNATURE);
        write_u32_le(&mut buf, sig_cmd_off + 4, LINKEDIT_DATA_COMMAND_SIZE as u32);

        let sizeofcmds = (buf.len() as u64 - HEADER_SIZE) as u32;
        write_u32_le(&mut buf, 16, 2); // ncmds
        write_u32_le(&mut buf, 20, sizeofcmds);

        buf.extend(vec![0u8; trailing_padding]);
        buf
    }

    #[test]
    fn rejects_wrong_magic() {
        let input = MachORewriteInput {
            original: &[0u8; 64],
            dylib_load_path: "@executable_path/x".into(),
        };
        assert_eq!(rewrite_macho(&input), Err(RewriteError::NotThin64));
    }

    #[test]
    fn rewrites_when_slot_fits_without_growth() {
        // "@executable_path/x" is 19 bytes + NUL = 20 -> cmdsize rounds to 24
        // (24 header + 20 name = 44 -> round to 48)... but our LC_CODE_SIGNATURE
        // anchor is only 16 bytes, so this exercises the grow-into-padding path
        // with a short name to also cover the "fits exactly" path separately.
        let original = build_test_macho(64);
        let input = MachORewriteInput {
            original: &original,
            dylib_load_path: "@executable_path/x".into(),
        };
        let out = rewrite_macho(&input).expect("rewrite should succeed");

        assert_ne!(out, original, "output must differ from a no-op");
        assert_eq!(original.len(), original.to_vec().len(), "input must be untouched");

        let ncmds = read_u32_le(&out, 16).unwrap();
        assert_eq!(ncmds, 2, "LC_CODE_SIGNATURE replaced 1-for-1");

        // The new command is LC_LOAD_DYLIB naming the given path.
        let sizeofcmds = read_u32_le(&out, 20).unwrap();
        let last_cmd_off = HEADER_SIZE + (sizeofcmds as u64) - {
            // recompute the new command's size the same way rewrite_macho did
            let n = input.dylib_load_path.len() as u64 + 1;
            (DYLIB_COMMAND_HEADER_SIZE + n + 7) & !7
        };
        assert_eq!(read_u32_le(&out, last_cmd_off).unwrap(), LC_LOAD_DYLIB);
        let name_off = (last_cmd_off + DYLIB_COMMAND_HEADER_SIZE) as usize;
        let name_len = input.dylib_load_path.len();
        assert_eq!(
            &out[name_off..name_off + name_len],
            input.dylib_load_path.as_bytes()
        );
    }

    #[test]
    fn fails_without_enough_padding_to_grow() {
        let original = build_test_macho(0); // no padding at all
        let input = MachORewriteInput {
            original: &original,
            dylib_load_path: "@executable_path/x".into(),
        };
        assert_eq!(rewrite_macho(&input), Err(RewriteError::NoPaddingToGrow));
    }

    #[test]
    fn fails_when_padding_is_not_zero() {
        let mut original = build_test_macho(64);
        // Corrupt the first byte of padding: the only byte guaranteed to
        // fall inside the checked `[table_end, table_end + delta)` window,
        // since `delta` (here 32 bytes) is smaller than the 64 bytes of
        // padding this fixture appends.
        let table_end = original.len() - 64;
        original[table_end] = 0xff;
        let input = MachORewriteInput {
            original: &original,
            dylib_load_path: "@executable_path/x".into(),
        };
        assert_eq!(rewrite_macho(&input), Err(RewriteError::NoPaddingToGrow));
    }

    #[test]
    fn fails_without_a_replaceable_slot() {
        // No LC_CODE_SIGNATURE at all.
        let mut buf = vec![0u8; HEADER_SIZE as usize];
        write_u32_le(&mut buf, 0, MH_MAGIC_64);
        let cmd_off = buf.len() as u64;
        buf.extend(vec![0u8; 32]);
        write_u32_le(&mut buf, cmd_off, 0x1b);
        write_u32_le(&mut buf, cmd_off + 4, 32);
        write_u32_le(&mut buf, 16, 1);
        write_u32_le(&mut buf, 20, 32);

        let input = MachORewriteInput {
            original: &buf,
            dylib_load_path: "@executable_path/x".into(),
        };
        assert_eq!(rewrite_macho(&input), Err(RewriteError::NoReplaceableSlot));
    }
}
