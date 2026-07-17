# os-emulation

Shared OS Emulation Layer (SOEL) for `@speet`, `@vane`, and future consumers.

This repository lives under `portal-co/os-emulation`. During local development it is patched in at the root `portal-hot/.cargo/config.toml` so consumers use these local crates instead of the Git copies.

See `../plans/shared-os-emulation-plan.md` for the current plan and phase breakdown.

## Current crates

- `crates/runtime/os-ctx` — guest `OS` / `Ctx` runtime traits.
- `crates/runtime/os-async` — async `AsyncOS` / `AsyncCtx` / `AsyncHostApi` surface.
- `crates/runtime/os-host-api` — pluggable host API surface (`HostApi`, `ImportManifest`).
- `crates/target/os-target-core` — speet-neutral `OsOp` stack-machine IR and `Backend` trait.
- `crates/target/os-target-wax` — `WaxBackend<T: InstructionSink>` for WASM-like sinks.
- `crates/target/os-target-native` — x86-64 SysV textual native-assembly backend.
- `crates/build/os-build` — compiler-glue contract (`BuildGlue<B>`, `MemoryCodegen`, `SyscallCodegen`, `RedirectCodegen`).
- `crates/abi/os-abi-spec` — generic ABI description model and `CallingConvention`.
- `crates/abi/os-abi-stubs` — checked-in redirect stub registry and native emitters.
- `crates/abi/os-abi-codegen` — generator that turns `AbiSpec` into checked-in stub Rust sources.
- `crates/page/os-page` — shared runtime memory/paging traits and `MemorySpec`.
- `crates/page/os-page-codegen` — compile-time memory/paging emitters (scaffolding).
- `crates/emit/os-syscall-emit` — generic syscall dispatch table data model.
- `crates/emit/os-linux-wasi` — RV64 Linux → WASI preview1 syscall mapping.
- `crates/daemon/os-transform-core` — backend-agnostic `TransformBackend` trait for on-the-fly binary transformation (AOT recompile, JIT, dylib/so rewrite).
- `crates/daemon/os-daemon-protocol` — wire protocol for the transform daemon, with explicit backend selection.
- `crates/daemon/os-daemon` — generic Unix-socket daemon dispatching to registered `TransformBackend`s.
- `crates/daemon/os-daemon-hook` — generator for the minimal, backend-agnostic C execve-interposition stub linked into guest/target binaries.
- `crates/daemon/os-rewrite-macho` — Mach-O `LC_LOAD_DYLIB` rewriter for macOS; always emits a new cached executable.
- `crates/daemon/os-rewrite-elf` — ELF `DT_NEEDED`/rpath rewriter shared by libc-based Linux and BSDs.
- `crates/daemon/os-codesign-macho` — macOS hardened-runtime codesigning (real-identity + library-constraint, or ad-hoc + cdhash fallback) for rewritten executables.

## Consumer integration docs

- `docs/integrations/speet.md` — Speet ahead-of-time recompiler integration.
- `docs/integrations/vane.md` — Vane JS / interpreter surface.
- `docs/integrations/wasm-blitz.md` — Future wasm-blitz native backend contract.
- `docs/integrations/private-os-handlers.md` — Future private kernel contract.
