//! Shared helper names used by all native [`Backend`](os_target_core::Backend) implementations.

/// Names of the runtime helper functions that back memory and syscall operations.
///
/// The generated machine code calls these symbols; a linker must resolve them
/// against a small architecture-specific runtime shim. Tests can also provide
/// in-image stubs and call [`set_label`](super::set_label) on the underlying
/// writer so the unresolved helper calls are resolved locally.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeHelpers {
    pub load_u8: &'static str,
    pub load_i8: &'static str,
    pub load_u16: &'static str,
    pub load_i16: &'static str,
    pub load_u32: &'static str,
    pub load_i32: &'static str,
    pub load_u64: &'static str,
    pub store_u8: &'static str,
    pub store_u16: &'static str,
    pub store_u32: &'static str,
    pub store_u64: &'static str,
    pub ecall: &'static str,
}

impl NativeHelpers {
    pub const DEFAULTS: Self = Self {
        load_u8: "os_load_u8",
        load_i8: "os_load_i8",
        load_u16: "os_load_u16",
        load_i16: "os_load_i16",
        load_u32: "os_load_u32",
        load_i32: "os_load_i32",
        load_u64: "os_load_u64",
        store_u8: "os_store_u8",
        store_u16: "os_store_u16",
        store_u32: "os_store_u32",
        store_u64: "os_store_u64",
        ecall: "os_ecall",
    };
}

impl Default for NativeHelpers {
    fn default() -> Self {
        Self::DEFAULTS
    }
}