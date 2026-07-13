//! Structured ABI model produced from BridgeSupport and future formats.

use alloc::string::String;
use alloc::vec::Vec;

/// High-level classification of an argument or return value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AbiValueKind {
    Void,
    Scalar,
    Pointer,
    FunctionPointer,
    /// Objective-C object / id-like pointer (BridgeSupport `type="@..."`).
    Object,
    /// Unrecognized or unsupported BridgeSupport encoding.
    Unknown(String),
}

/// One parameter or return slot in an ABI function signature.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AbiArg {
    pub kind: AbiValueKind,
    /// BridgeSupport `type` attribute when present (e.g. `i`, `*`, `^?`).
    pub bridgesupport_type: Option<String>,
    pub function_pointer: bool,
    pub pointer: bool,
}

impl AbiArg {
    pub fn void() -> Self {
        Self {
            kind: AbiValueKind::Void,
            bridgesupport_type: None,
            function_pointer: false,
            pointer: false,
        }
    }
}

/// One C/Objective-C callable symbol from an ABI description file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AbiFunction {
    pub name: String,
    pub args: Vec<AbiArg>,
    pub retval: AbiArg,
    pub variadic: bool,
}

/// Parsed ABI description for one framework or library surface.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AbiSpec {
    pub functions: Vec<AbiFunction>,
}

impl AbiSpec {
    /// All symbol names in this spec, sorted and deduplicated.
    pub fn symbol_names(&self) -> Vec<&str> {
        let mut names: Vec<&str> = self.functions.iter().map(|f| f.name.as_str()).collect();
        names.sort_unstable();
        names.dedup();
        names
    }

    pub fn lookup(&self, name: &str) -> Option<&AbiFunction> {
        self.functions.iter().find(|f| f.name == name)
    }

    /// Whether `name` accepts at least one function-pointer argument.
    pub fn accepts_function_pointers(&self, name: &str) -> bool {
        self.lookup(name)
            .map(|f| {
                f.args
                    .iter()
                    .any(|a| a.function_pointer || a.kind == AbiValueKind::FunctionPointer)
            })
            .unwrap_or(false)
    }

    /// Symbols whose signatures include function-pointer parameters — the
    /// set `speet-runtime::suitability` will eventually move off the blanket
    /// deny path once codegen exists for them.
    pub fn fn_ptr_symbols(&self) -> Vec<&str> {
        self.functions
            .iter()
            .filter(|f| {
                f.args.iter().any(|a| {
                    a.function_pointer || a.kind == AbiValueKind::FunctionPointer
                })
            })
            .map(|f| f.name.as_str())
            .collect()
    }
}
