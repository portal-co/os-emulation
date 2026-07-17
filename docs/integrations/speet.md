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
| `speet-rtd` (daemon binary/lib) | `os-daemon` + `os-transform-core` |
| `speet-runtime::rtd_protocol` | `os-daemon-protocol` |
| `speet-runtime::execve_hook` | `os-daemon-hook` |

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
cargo test -p speet-rtd -p speet-runtime
```

`os-emulation`'s own daemon crates are tested standalone:

```bash
cd os-emulation
cargo test -p os-daemon-protocol -p os-daemon -p os-transform-core
cargo test -p os-rewrite-macho -p os-codesign-macho   # macOS
cargo test -p os-rewrite-elf                          # Linux/BSD
```

Full E2E corpus tests remain in Speet and verify the whole pipeline end-to-end.

## Daemon and transform backends

`os-transform-core::TransformBackend` is the backend-agnostic contract for
on-the-fly binary transformation: given a target path, produce (or fetch
from cache) a runnable, never-mutate-the-input artifact. `os-daemon` holds a
registry of these backends and dispatches each `Obtain` request to whichever
one the client names — the daemon itself never hardcodes a single
transformation strategy.

Two backends currently implement `TransformBackend`, both living in speet
(mirroring how `os-build::BuildGlue<B>` implementations stay with their
consumer):

- `speet_runtime::IntegratedNativeRuntime` — the existing ahead-of-time
  full-recompile pipeline, registered under `BackendId::INTEGRATED_RECOMPILE`
  (`"integrated"`).
- `speet_rtd::simple_rewrite::SimpleRewriteBackend` — the dylib/so rewriter
  for macOS, Linux, and BSD, registered under `BackendId::SIMPLE_REWRITE`
  (`"simple-rewrite"`) whenever `SIMPLE_REWRITE_SHIM` is configured. It
  builds on OS-neutral `os-emulation` primitives:
  - `os-rewrite-macho` — Mach-O `LC_LOAD_DYLIB` rewriter (macOS).
  - `os-rewrite-elf` — ELF `DT_NEEDED`/`DT_RUNPATH` rewriter (Linux/BSD).
  - `os-codesign-macho` — hardened-runtime codesigning implementing the two
    methods from `hardened-runtime-library-validation-schema.md` (real
    identity + library constraint, or ad-hoc + cdhash fallback for local
    development).

  Every backend's contract is "read-only on the input, always emit a new
  cached output" — the rewriter never patches a binary in place; it stages
  a rewritten (and, on macOS, re-signed) copy under a private cache
  directory and returns that path for the caller to `execve` directly.

`speet-rtd` registers both backends by default; `Request::Obtain{backend}`
selects which one handles a given path, and `Request::ListBackends` reports
what's currently registered. `speet_runtime::execve_hook::generate_execve_hook_c()`
and `os_daemon_hook::generate_execve_hook_c()` generate the same minimal C
stub either way — the backend id is a generation-time parameter, not a wire
literal — so the embedded execve hook works unmodified regardless of which
backend produced the binary it's linked into.

## Current limitations and future work

- `wasm-blitz` direct native tests are deferred until the refactor stabilizes.
- The deprecated `wasm-blitz` `NaiveAbi` (e.g. `__wasm_exn_propagate`) is not
  used by `os-target-native`; only the SysV ABI path is supported.
- Speet's `BuildGlue` implementations for `os-target-native` / direct native
  helpers are scaffolding; real end-to-end emission still flows through
  `WaxBackend<SpeetInstructionSink>` today.