# SOEL consumer integration docs

This directory contains integration guides for each major consumer of the
Shared OS Emulation Layer (`os-emulation`).

- [`speet.md`](speet.md) — how the Speet ahead-of-time recompiler uses the
  shared OS crates, including the compatibility shim strategy and `BuildGlue<B>`
  wiring.
- [`vane.md`](vane.md) — Vane's current riscv64 JS / interpreter surface,
  `OsOp → StackOp` lowering, async support, and JS memory helpers.
- [`wasm-blitz.md`](wasm-blitz.md) — future backend contract for the
  `portal-co/wasm-blitz` native compiler; intentionally deferred until its
  refactor stabilizes.
- [`private-os-handlers.md`](private-os-handlers.md) — placeholder contract for
  third-party / private kernel implementations.