# os-emulation

Shared OS Emulation Layer (SOEL) for `@speet`, `@vane`, and future consumers.

This repository lives under `portal-co/os-emulation`. During local development it is patched in at the root `portal-hot/.cargo/config.toml` so consumers use these local crates instead of the Git copies.

See `../plans/shared-os-emulation-plan.md` for the current plan and phase breakdown.

## Current crates

- `crates/runtime/os-ctx` — guest `OS` / `Ctx` runtime traits.
- `crates/runtime/os-async` — async `AsyncOS` / `AsyncCtx` / `AsyncHostApi` surface.
- `crates/runtime/os-host-api` — pluggable host API surface (`HostApi`, `ImportManifest`).
- `crates/target/os-target-core` — speet-neutral `OsOp` stack-machine IR and `Backend` trait.
- `crates/build/os-build` — compiler-glue contract (`BuildGlue<B>`, `MemoryCodegen`, `SyscallCodegen`, `RedirectCodegen`).
- `crates/abi/os-abi-spec` — generic ABI description model and `CallingConvention`.
- `crates/abi/os-abi-stubs` — checked-in redirect stub registry and native emitters.
- `crates/abi/os-abi-codegen` — generator that turns `AbiSpec` into checked-in stub Rust sources.
- `crates/page/os-page` — shared runtime memory/paging traits and `MemorySpec`.
- `crates/page/os-page-codegen` — compile-time memory/paging emitters (scaffolding).
- `crates/emit/os-syscall-emit` — generic syscall dispatch table data model.
- `crates/emit/os-linux-wasi` — RV64 Linux → WASI preview1 syscall mapping.
