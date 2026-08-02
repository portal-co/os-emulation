//! Merged handler registry: BridgeSupport defaults + plugin overrides.

use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;

use os_abi_spec::{AbiFunction, AbiSpec};

use crate::forward::BridgeSupportForwardHandler;

/// Semantic handler for one libc/libSystem symbol.
pub trait ShimHandler {
    fn symbol(&self) -> &str;
    /// Emit the `os_shim_{sym}(…)` C body (host-pointer ABI).
    fn emit_core(&self, func: &AbiFunction) -> String;
    /// Whether this handler replaces a BridgeSupport default (plugin).
    fn is_override(&self) -> bool {
        false
    }
    /// Emit interposer export wrapper C for `{sym}` → `os_shim_{sym}`.
    fn emit_interposer_bridge(&self, func: &AbiFunction) -> String {
        emit_default_interposer_bridge(func, self.is_override())
    }
}

/// Registry merged at codegen / link time.
#[derive(Default)]
pub struct ShimRegistry {
    overrides: BTreeMap<String, Box<dyn ShimHandler>>,
}

impl ShimRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a plugin override (wins over BridgeSupport default).
    pub fn register_override(&mut self, handler: Box<dyn ShimHandler>) {
        self.overrides.insert(handler.symbol().to_string(), handler);
    }

    /// Resolve handler for `sym`: override if present, else default forward.
    pub fn resolve<'a>(&'a self, sym: &str, func: &'a AbiFunction) -> ResolvedHandler<'a> {
        if let Some(h) = self.overrides.get(sym) {
            return ResolvedHandler::Override(h.as_ref());
        }
        let _ = func;
        ResolvedHandler::Default
    }

    /// Whether any handler (default or override) exists for `sym` in `spec`.
    pub fn has_impl(spec: &AbiSpec, registry: &Self, sym: &str) -> bool {
        spec.lookup(sym).is_some() || registry.overrides.contains_key(sym)
    }

    /// Emit core C for `sym` using merged registry + spec lookup.
    pub fn emit_core(&self, spec: &AbiSpec, sym: &str) -> Option<String> {
        let func = spec.lookup(sym)?;
        Some(match self.resolve(sym, func) {
            ResolvedHandler::Override(h) => h.emit_core(func),
            ResolvedHandler::Default => BridgeSupportForwardHandler.emit_core(func),
        })
    }

    /// Emit interposer bridge C for `sym`.
    pub fn emit_interposer_bridge(&self, spec: &AbiSpec, sym: &str) -> Option<String> {
        let func = spec.lookup(sym)?;
        Some(match self.resolve(sym, func) {
            ResolvedHandler::Override(h) => h.emit_interposer_bridge(func),
            ResolvedHandler::Default => emit_default_interposer_bridge(func, false),
        })
    }

    /// All symbols to emit: spec symbols plus any override-only symbols.
    pub fn symbols_for_codegen(&self, spec: &AbiSpec, config_symbols: &[String]) -> Vec<String> {
        let mut out: Vec<String> = config_symbols.to_vec();
        for key in self.overrides.keys() {
            if !out.iter().any(|s| s == key) {
                out.push(key.clone());
            }
        }
        out.sort();
        out.dedup();
        out
    }
}

pub enum ResolvedHandler<'a> {
    Default,
    Override(&'a dyn ShimHandler),
}

/// Default interposer export: public `{sym}` wrapper calling `os_shim_{sym}`.
pub fn emit_default_interposer_bridge(func: &AbiFunction, _is_override: bool) -> String {
    use crate::c_emit::{c_arg_names, c_param_list, c_return_type, os_shim_name};

    let sym = &func.name;
    let shim = os_shim_name(sym);
    let ret_ty = c_return_type(func);
    let params = c_param_list(func);
    let args = c_arg_names(func);
    let wrapper = format!("__os_interpose_{sym}");

    let mut out = format!(
        "/* @generated interposer bridge — {sym} */\n#include \"os_shim.h\"\n#include <dlfcn.h>\n\n"
    );

    out.push_str(&format!(
        "typedef {ret_ty} (*__os_real_{sym}_t)({params});\nstatic __os_real_{sym}_t __os_real_{sym};\n\n"
    ));

    out.push_str(&format!(
        "static __os_real_{sym}_t __os_load_{sym}(void) {{\n    if (!__os_real_{sym}) {{\n        __os_real_{sym} = (__os_real_{sym}_t)dlsym(RTLD_NEXT, \"{sym}\");\n    }}\n    return __os_real_{sym};\n}}\n\n"
    ));

    if ret_ty == "void" {
        out.push_str(&format!(
            "{ret_ty} {wrapper}({params}) {{\n    {shim}({args});\n}}\n\n"
        ));
    } else {
        out.push_str(&format!(
            "{ret_ty} {wrapper}({params}) {{\n    return {shim}({args});\n}}\n\n"
        ));
    }

    // Mach-O interpose tuple (weak, one per symbol TU).
    out.push_str("#if defined(__APPLE__)\n#include <stddef.h>\n");
    out.push_str("struct __interpose_tuple {\n    const void *replacement;\n    const void *replacee;\n};\n");
    out.push_str(&format!(
        "__attribute__((used)) static struct __interpose_tuple __os_interpose_{sym}_tuple\n    __attribute__((section(\"__DATA,__interpose\"))) = {{\n    (const void *)&{wrapper},\n    (const void *)&{sym},\n}};\n#endif\n"
    ));

    // ELF: export wrapper as the public symbol.
    out.push_str(&format!(
        "#if !defined(__APPLE__)\n__attribute__((visibility(\"default\"))) "
    ));
    if ret_ty == "void" {
        out.push_str(&format!(
            "{ret_ty} {sym}({params}) {{\n    {wrapper}({args});\n}}\n#endif\n"
        ));
    } else {
        out.push_str(&format!(
            "{ret_ty} {sym}({params}) {{\n    return {wrapper}({args});\n}}\n#endif\n"
        ));
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use os_abi_spec::{AbiArg, AbiFunction, AbiValueKind};

    struct DummyOverride {
        sym: &'static str,
    }

    impl ShimHandler for DummyOverride {
        fn symbol(&self) -> &str {
            self.sym
        }
        fn emit_core(&self, _func: &AbiFunction) -> String {
            format!("/* override {} */", self.sym)
        }
        fn is_override(&self) -> bool {
            true
        }
    }

    #[test]
    fn override_wins_over_default() {
        let mut reg = ShimRegistry::new();
        reg.register_override(Box::new(DummyOverride { sym: "execve" }));
        let spec = AbiSpec::default();
        assert!(reg.emit_core(&spec, "execve").is_none()); // not in spec
        let f = AbiFunction {
            name: "execve".into(),
            args: vec![],
            retval: AbiArg::void(),
            variadic: false,
        };
        let mut spec = AbiSpec {
            functions: vec![f],
        };
        let core = reg.emit_core(&spec, "execve").unwrap();
        assert!(core.contains("override execve"));
    }
}
