//! Generate checked-in ABI redirect stub Rust from BridgeSupport specs.

mod generate;
mod generate_c;

pub use generate::{generate, CodegenConfig, GeneratedFile, StubArch};
pub use generate_c::{generate_c, COutputKind};
