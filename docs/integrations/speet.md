# Speet integration guide

Speet is the reference consumer of the Shared OS Emulation Layer (SOEL). The
migration is staged to keep the existing thin-runtime and recompile pipelines
working while ownership of OS-agnostic surfaces moves into `os-emulation`.

## Compatibility shim strategy

Speet keeps its original crate names as compatibility shims while the real
implementation lives in `os-emulation`:

| Speet compatibility shim | Backing `os-emulation` crate |
|---|---|
| `osctx` | `os-ctx` |
| `speet-host-api` | `os-host-api` |
| `speet-abi-spec` | `os-abi-spec` |
| `speet-abi-stubs` | `os-abi-stubs` |
| `speet-abi-codegen` | `os-abi-codegen` |
| `speet-syscall` | `os-syscall-emit` |
| `speet-linux-wasi` | `os-linux-wasi` |

Each shim is a tiny `Cargo.toml` plus a `src/lib.rs` that re-exports the
`os-emulation` crate (`pub use os_*::*;`). Speet's internal crates therefore
reference Git dependencies on `portal-co/os-emulation`, and the top-level
`portal-hot/.cargo/config.toml` `[patch]` entries point those dependencies back
at the local checkout.

## Compiler glue: `BuildGlue<B>` in `speet-module-builder`

`speet-module-builder::MegabinaryBuilder` implements `os-build::BuildGlue<B>`
for any `B: os_target_core::Backend`. The implementation answers
recompiler-level OS questions:

- `emit_jump_to_address` — emits `OsOp::PushU64(guest_addr); OsOp::Jump { target }`.
- `emit_memory_glue` / `emit_page_table_glue` — emits memory helper sequences.
- `reserve_os_glue` / `emit_dispatch_entry` — shapes the `_dispatch(hash_id, argc, argv)` megabinary entry.
- `emit_plt_stub` — emits redirect stubs for ABI symbols.

The generic implementation is exercised by unit tests in `speet-module-builder`.

```bash
cd speet
cargo test -p speet-module-builder
cargo check -p speet-module-builder
```

## Syscall surfaces

- **`os-syscall-emit`** holds the backend-neutral `SyscallTable` / `SyscallEntry`
  data model.
- **`speet-syscall`** re-exports those types and keeps `WasmSyscallDispatcher`,
  which renders inline `br_table` ecall dispatch.
- **`os-linux-wasi`** provides the RV64 Linux → WASI preview1 mapping.
- **`speet-linux-wasi`** re-exports `LinuxToWasi`, `WasiImports`, and
  `IOVEC_SCRATCH_OFFSET`, plus a `WasiImportsExt` trait that preserves the old
  `register` / `declare` methods for existing call sites.

## Backends

Speet selects the actual code-generation backend outside of `os-emulation`:

- **Current native path** — Speet is generic over `wax-core::InstructionSink`.
  The `os-target-wax` crate provides a `WaxBackend<T: InstructionSink>` that
  turns `OsOp` operations into `wax-core` instructions. Speet then links the
  generated object through its existing LLVM / `lld` pipeline.
- **Future direct native path** — `os-target-native` prints x86-64 System V
  assembly text from `OsOp`. It is intended for small runtime helpers and as a
  stop-gap until `wasm-blitz` direct native output stabilizes.
- **Future `wasm-blitz` direct native** — once the `portal-co/wasm-blitz`
  refactor lands, a `WasmBlitzDirect` backend can be added to `os-target-core`
  and consumed by Speet without changing the `OsOp` surface.

## Testing

Speet's existing tests become the consumer test suite once everything links:

```bash
cd speet
cargo check --all
cargo test -p speet-module-builder
cargo test -p speet-syscall
cargo test -p speet-linux-wasi
```

Full E2E corpus tests remain in Speet and verify the whole pipeline end-to-end.

## Current limitations and future work

- `wasm-blitz` direct native tests are deferred until the refactor stabilizes.
- The deprecated `wasm-blitz` `NaiveAbi` (e.g. `__wasm_exn_propagate`) is not
  used by `os-target-native`; only the SysV ABI path is supported.
- Speet's `BuildGlue` implementations for `os-target-native` / direct native
  helpers are scaffolding; real end-to-end emission still flows through
  `WaxBackend<SpeetInstructionSink>` today.