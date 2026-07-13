# Private OS handlers contract (future work)

This document is a placeholder for the contract that third-party / private
kernel implementations must satisfy to plug into the Shared OS Emulation
Layer (SOEL).

## Scope

- **In scope:** how a private kernel implements `os-ctx::OS` and
  `os-host-api::HostApi`, declares syscall surface compatibility, and exposes
  snapshot / restore / policy hooks.
- **Out of scope:** any specific kernel implementation. The public SOEL crates
  define traits; concrete kernels live in separate repositories or in-tree
  modules with their own security review.

## Why private handlers matter

The open-source OS emulation crates (Linux ABI, WASI preview1, etc.) implement
centrally reviewed policies. Production deployments may need stricter or
organization-specific behavior:

- Mandatory access-control checks before every `mmap` / `mprotect`.
- Custom agent-pause / snapshot-audit behavior.
- Audit logging that routes syscalls to an external SIEM.
- Proprietary trusted-IO paths.

Private handlers satisfy these requirements without requiring them in the public
crates.

## Required trait surface

A private kernel must implement:

- `os-ctx::OS` — dispatch syscalls and `osfuncall` hooks.
- `os-ctx::Ctx` — guest register / memory view during a syscall.
- `os-host-api::HostApi` — ambient host capability plumbing (if ambient linking
  is required).
- `os-async::AsyncOS` / `os-async::AsyncCtx` — if the private kernel supports
  async host delegation.
- `os-manifest` schema (when defined) — declare syscall whitelists, hash
  registry, signing keys, and snapshot metadata format.

## Packaging expectations

- Private kernel crates may depend on public `os-emulation` crates via the
  published Git versions.
- They must *not* copy or fork `os-ctx`, `os-host-api`, or `os-build` into their
  own namespaces, because that defeats compatibility with shared code generators.
- They must not re-export `wax-core` traits through `os-emulation` public APIs;
  consumers must depend on `portal-co/wax` directly.

## Testing and certification

- A private kernel should run the `os-emulation` trait mock tests.
- It should pass a minimal Linux ABI fixture (`read`/`write`/`exit` from a
  small statically linked binary).
- Integration test harnesses are expected to live alongside the private kernel,
  referencing `os-emulation` test utilities once those are published.

## Status

- **Future work.** No implementation or tests exist yet.
- The trait surface (`os-ctx`, `os-host-api`, `os-async`) is already stabilizing
  so that future private kernels can be written against it without further churn.