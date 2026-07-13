//! Ingest ABI description files into a structured model for redirect-stub
//! generation.
//!
//! Phase 1 (this crate): parse BridgeSupport XML → [`AbiSpec`]. Phase 2
//! (`os-abi-codegen`) emits checked-in stub code.

#![no_std]

extern crate alloc;

mod bridgesupport;
mod convention;
mod model;

pub use bridgesupport::{parse_bridgesupport, BridgeSupportError};
pub use convention::{CallingConvention};
pub use model::{AbiArg, AbiFunction, AbiSpec, AbiValueKind};
