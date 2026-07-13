# Vane integration guide

Vane is a first-class consumer of the Shared OS Emulation Layer (SOEL). The
integration proceeds in three slices, matching Vane's own supported surfaces:

1. **Runtime memory** — `vane-arch::Mem` implements `os-page::GuestMemory`.
2. **Async OS callbacks** — `vane-arch::AsyncOsHost` implements
   `vane-arch::AsyncStackHost` via `os-async` traits.
3. **Compile-time lowering** — `vane-target-core::StackOpBackend` lowers
   `os-target-core::OsOp` into Vane's existing `StackOp` IR, and
   `vane-target-js::CoreJsMemoryCodegen` emits JS memory helpers via
   `os-page-codegen::JsBackend`.

Vane's WebAssembly JIT and full `BuildGlue<_>` integration remain aspirational
and are gated on completing the riscv64 JS/interpreter surface first.

## Runtime memory

`os-page` defines `GuestMemory` and `PageTable` traits. `vane-arch::Mem` uses its
existing `read_byte` / `write_byte` / `get_page` helpers to implement
`GuestMemory::read` / `write` for `W8`, `W16`, `W32`, and `W64` accesses.

```rust
// vane/crates/vane-arch/src/lib.rs
impl os_page::GuestMemory for vane_arch::Mem {
    // ...
}
```

Run the Vane memory tests:

```bash
cd vane
cargo test -p vane-arch
```

## Async OS support

`os-async` mirrors `os-ctx` with `AsyncCtx`, `AsyncOS`, and `AsyncHostApi`.
`vane-arch` gates async support behind the `async-host` feature and provides an
`AsyncOsHost` adapter that turns `AsyncStackHost` calls into `AsyncCtx` memory /
register access and `AsyncOS::syscall` invocations.

```bash
cd vane
cargo test -p vane-arch --features async-host
```

## Compile-time lowering: `OsOp` → `StackOp`

Vane's interpreter and JS renderer already consume a shared stack IR called
`StackOp`. Rather than forcing an isomorphism between `OsOp` and `StackOp`, the
`StackOpBackend` implements `os-target_core::Backend` one-way: shared OS glue emits
`OsOp`, Vane consumes `StackOp`, and `StackOpBackend` performs the lowering.

```rust
use vane_target_core::StackOpBackend;
use os_target_core::{Backend, OsOp, MemWidth};

let mut b = StackOpBackend::new();
b.op(OsOp::PushU64(0x1000));
b.op(OsOp::Load { width: MemWidth::W32, signed: true });
let stack_ops = b.into_ops();
```

Supported mappings:

| `OsOp` | Lowered `StackOp`s |
|---|---|
| `PushU64` / `PushU32` | `PushImm` |
| `Pop` | `StoreReg(0)` (discard) |
| `Load { width, signed }` | `LoadMem { width, signed }` |
| `Store { width }` | `StoreMem { width }` |
| `Ecall` | `Ecall` |
| `Jump { target }` | `PushImm(target)` + `TailCall` |
| `TailCall { helper }` | `Log(helper)` + `TailCall` |
| `Trap` | `Trap(_)` |

`W128` loads / stores currently lower to `Trap` because `StackOp` only models
B1..B8.

## JavaScript memory helpers

`vane-target-js::mem::CoreJsMemoryCodegen` implements
`os-build::MemoryCodegen<JsBackend<'_>>`, producing the same `data(addr)` /
`DataView` pattern used by Vane's existing legacy JS renderer:

```js
data=(v=>{let p=$.get_page(Number(v));return new DataView($._sys('memory').buffer,p);})
```

Per-access loads and stores go through `os-page-codegen::JsBackend`, which
emits the same `getUint32` / `setUint16` / etc. sequences.

Run the JS target tests:

```bash
cd vane
cargo test -p vane-target-js
```

## Current limitations and future work

- **Shared paging.** `CoreJsMemoryCodegen` currently emits a legacy `data()`
  helper only for specs equal to `MemorySpec::WASM_64K`. Shared / multi-level
  page-table walks emit a run-time `throw` so the gap is visible.
- **`BuildGlue<JsBackend>`.** The full recompiler-glue contract
  (`emit_jump_to_address`, `emit_dispatch_entry`, ...) is not yet implemented
  for Vane. Once the current riscv64 JS/interpreter surface is stable it can be
  built on top of `StackOpBackend` and `CoreJsMemoryCodegen`.
- **Vane WASM JIT.** `vane-target-wasm` and a `BuildGlue<WaxBackend<_>>` path
  remain aspirational.