//! Native assembly backends for the shared `OsOp` stack-machine IR.
//!
//! This crate translates OsOp operations into machine code or textual assembly
//! for a given ABI. The initial backend is x86-64 System V ABI using the raw
//! `asm-x86-64` crate (not the deprecated wasm-blitz NaiveAbi).

#![no_std]

extern crate alloc;

#[cfg(feature = "x86_64")]
pub mod x86_64;

#[cfg(feature = "x86_64")]
pub use x86_64::{
    BinaryX86_64SysVBackend, SysVHelpers, TextX86_64SysVBackend, X86_64SysVBackend,
    X86_64SysVConfig,
};