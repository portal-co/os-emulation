//! Native assembly backends for the shared `OsOp` stack-machine IR.
//!
//! This crate translates OsOp operations into machine code or textual assembly
//! for a given ABI. The initial backend is x86-64 System V ABI using the raw
//! `asm-x86-64` crate (not the deprecated wasm-blitz NaiveAbi).

#![no_std]

extern crate alloc;

pub mod native_helpers;

#[cfg(feature = "x86_64")]
pub mod x86_64;
#[cfg(feature = "aarch64")]
pub mod aarch64;
#[cfg(feature = "riscv64")]
pub mod riscv64;

#[cfg(feature = "x86_64")]
pub use x86_64::{
    BinaryX86_64SysVBackend, SysVHelpers, TextX86_64SysVBackend, X86_64SysVBackend,
    X86_64SysVConfig,
};
#[cfg(feature = "aarch64")]
pub use aarch64::{AArch64SysVBackend, BinaryAArch64SysVBackend};
#[cfg(feature = "riscv64")]
pub use riscv64::{BinaryRiscv64Backend, Riscv64Backend};

pub use native_helpers::NativeHelpers;