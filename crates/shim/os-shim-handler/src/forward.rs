//! Default BridgeSupport → weak libc forward handler.

use os_abi_spec::AbiFunction;

use crate::c_emit::{
    c_arg_names, c_host_symbol, c_param_list, c_return_type, os_shim_name,
};
use crate::ShimHandler;

/// Emits a weak `os_shim_{sym}` that forwards to the real host libc symbol.
pub struct BridgeSupportForward;

impl ShimHandler for BridgeSupportForward {
    fn symbol(&self) -> &str {
        unreachable!("BridgeSupportForward is a factory, not a single-symbol handler")
    }

    fn emit_core(&self, func: &AbiFunction) -> String {
        emit_weak_forward(func)
    }

    fn is_override(&self) -> bool {
        false
    }
}

impl BridgeSupportForward {
    /// Build a per-symbol handler instance.
    pub fn for_symbol(_sym: &str) -> BridgeSupportForwardHandler {
        BridgeSupportForwardHandler
    }
}

/// Per-symbol default forward handler (stateless).
pub struct BridgeSupportForwardHandler;

impl ShimHandler for BridgeSupportForwardHandler {
    fn symbol(&self) -> &str {
        unreachable!("use emit_core with AbiFunction")
    }

    fn emit_core(&self, func: &AbiFunction) -> String {
        emit_weak_forward(func)
    }
}

/// Emit weak `os_shim_{sym}` forwarding to host `{sym}` (or `_exit` for exit).
pub fn emit_weak_forward(func: &AbiFunction) -> String {
    let sym = &func.name;
    let shim = os_shim_name(sym);
    let host = c_host_symbol(sym);
    let ret_ty = c_return_type(func);
    let params = c_param_list(func);
    let args = c_arg_names(func);

    let mut out = format!(
        "/* @generated weak forward — {sym} */\n#include \"os_shim.h\"\n#include <unistd.h>\n#include <stdio.h>\n#include <stdlib.h>\n#include <string.h>\n\n"
    );

    if ret_ty == "void" {
        out.push_str(&format!(
            "__attribute__((weak)) void {shim}({params}) {{\n    {host}({args});\n}}\n"
        ));
    } else {
        out.push_str(&format!(
            "__attribute__((weak)) {ret_ty} {shim}({params}) {{\n    return ({ret_ty}){host}({args});\n}}\n"
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use os_abi_spec::{AbiArg, AbiFunction, AbiValueKind};

    fn write_fn() -> AbiFunction {
        AbiFunction {
            name: "write".into(),
            args: vec![
                AbiArg {
                    kind: AbiValueKind::Scalar,
                    bridgesupport_type: Some("i".into()),
                    function_pointer: false,
                    pointer: false,
                },
                AbiArg {
                    kind: AbiValueKind::Pointer,
                    bridgesupport_type: Some("^v".into()),
                    function_pointer: false,
                    pointer: true,
                },
                AbiArg {
                    kind: AbiValueKind::Scalar,
                    bridgesupport_type: Some("Q".into()),
                    function_pointer: false,
                    pointer: false,
                },
            ],
            retval: AbiArg {
                kind: AbiValueKind::Scalar,
                bridgesupport_type: Some("q".into()),
                function_pointer: false,
                pointer: false,
            },
            variadic: false,
        }
    }

    #[test]
    fn emits_weak_write_forward() {
        let src = emit_weak_forward(&write_fn());
        assert!(src.contains("__attribute__((weak))"));
        assert!(src.contains("os_shim_write"));
        assert!(src.contains("return (long)write("));
    }

    #[test]
    fn exit_forwards_to_underscore_exit() {
        let f = AbiFunction {
            name: "exit".into(),
            args: vec![AbiArg {
                kind: AbiValueKind::Scalar,
                bridgesupport_type: Some("i".into()),
                function_pointer: false,
                pointer: false,
            }],
            retval: AbiArg::void(),
            variadic: false,
        };
        let src = emit_weak_forward(&f);
        assert!(src.contains("_exit(a0)"));
    }
}
