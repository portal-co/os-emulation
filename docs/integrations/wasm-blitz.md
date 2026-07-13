# wasm-blitz backend contract

`portal-co/wasm-blitz` is a no_std WebAssembly-to-native compiler. It is a
*future* backend target for `os-emulation`, not a current dependency. This
document records the contract and known blockers so that OS emulation code can
be written against it once the `wasm-blitz` refactor stabilizes.

## Design split

- **WASM frontend contract:** post-refactor `wasm-blitz` will implement the
  `wax-core::InstructionSink` trait for reading / consuming WASM. `os-target-wax`
  therefore already speaks to `wasm-blitz` whenever it emits for a
  `wax-core::InstructionSink` consumer.
- **Direct native backend contract:** `wasm-blitz` native targets currently
  provide both a deprecated `NaiveAbi` and a modern `SysVAbi` path. The OS
  emulation layer will use the `SysVAbi` path exclusively; the `NaiveAbi` path is
  intentionally not supported because it relies on undefined runtime shims such
  as `__wasm_exn_propagate`.

## OS emulation concerns

When OS-glue code is lowered to native machine code, several WASM-isms become
concrete runtime symbols. The native backend must provide these helpers,
escaping or emulating the WASM architectural model:

| WASM assumption | Native helper responsibility |
|---|---|
| `memory.grow` / `memory.size` | runtime page management, exported to the linker |
| 64-bit linear memory base and bounds | runtime exposes `os_memory_base` / `os_memory_bound` |
| Exception propagation (`try_table` / `throw`) | per-ISA `__wasm_exn_propagate` or equivalent |
| Imported host functions (`env::*` / `wasi_snapshot_preview1::*`) | resolved by the thin runtime manifest |
| Second Context Register (SCR) | preserved across helper calls |

## How `os-emulation` will plug in

1. Add a new `WasmBlitzDirect` backend to `os-target-core` / `os-target-native`
   that renders `OsOp` to `asm-arch` / `wasm-blitz` SysV instructions directly
   (without producing a WASM module first).
2. Speet recompiles its guest through `os-target-core` OS glue + the guest
   recompiler frontend; the combined output is fed to `wasm-blitz` native.
3. A small runtime shim provides the symbols in the table above.

## Status

- **Deferred.** No code in `os-emulation` depends on `wasm-blitz` today.
- **WASM-via-`wax-core` path is ready today:** `os-target-wax` can already emit
  `OsOp` as a WASM module that `wasm-blitz` (post-refactor) will consume as an
  `InstructionSink` implementor.
- **Direct native path requires:** `wasm-blitz` SysV ABI exception handling,
  stable direct native API, and agreed per-ISA runtime shim contracts.

## Recommended test plan (future)

1. Compile a minimal `add` / `exit` guest to native via `WasmBlitzDirect` and
   check that `os_ecall` is invoked.
2. Verify `__wasm_exn_propagate` is *not* emitted for OS glue; instead the SysV
   backend emits explicit unwind info or returns an error value.
3. E2E corpus run on the direct native path, gated behind the `wasm-blitz`
   refactor completion.