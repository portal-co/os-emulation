//! BridgeSupport → C type and symbol naming helpers.

use os_abi_spec::{AbiArg, AbiFunction, AbiValueKind};

/// `os_shim_{sym}` — the shared core entry point name.
pub fn os_shim_name(sym: &str) -> String {
    format!("os_shim_{sym}")
}

/// Host libc symbol to call from a default forward stub.
pub fn c_host_symbol(sym: &str) -> &str {
    match sym {
        "exit" => "_exit",
        other => other,
    }
}

/// C parameter type for one BridgeSupport arg slot.
pub fn c_param_type(arg: &AbiArg) -> &'static str {
    match arg.kind {
        AbiValueKind::Void => "void",
        AbiValueKind::FunctionPointer => "void *",
        AbiValueKind::Pointer | AbiValueKind::Object => "void *",
        AbiValueKind::Scalar | AbiValueKind::Unknown(_) => match arg.bridgesupport_type.as_deref() {
            Some("q" | "Q" | "d" | "D" | "L" | "l") => "long",
            Some("*") => "const char *",
            _ => "int",
        },
    }
}

/// C return type for a BridgeSupport function.
pub fn c_return_type(func: &AbiFunction) -> &'static str {
    if func.retval.kind == AbiValueKind::Void {
        return "void";
    }
    match func.retval.kind {
        AbiValueKind::Pointer | AbiValueKind::Object => "void *",
        AbiValueKind::FunctionPointer => "void *",
        AbiValueKind::Scalar | AbiValueKind::Unknown(_) => {
            match func.retval.bridgesupport_type.as_deref() {
                Some("q" | "Q" | "d" | "D" | "L" | "l") => "long",
                Some("*") => "char *",
                _ => "int",
            }
        }
        AbiValueKind::Void => "void",
    }
}

/// Comma-separated C parameter list `(type name, …)`.
pub fn c_param_list(func: &AbiFunction) -> String {
    if func.args.is_empty() {
        return String::from("void");
    }
    func.args
        .iter()
        .enumerate()
        .map(|(i, arg)| format!("{} a{i}", c_param_type(arg)))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Comma-separated argument names `a0, a1, …`.
pub fn c_arg_names(func: &AbiFunction) -> String {
    if func.args.is_empty() {
        return String::new();
    }
    (0..func.args.len())
        .map(|i| format!("a{i}"))
        .collect::<Vec<_>>()
        .join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use os_abi_spec::{AbiArg, AbiFunction, AbiValueKind};

    #[test]
    fn exit_maps_to_underscore_exit() {
        assert_eq!(c_host_symbol("exit"), "_exit");
        assert_eq!(c_host_symbol("write"), "write");
    }

    #[test]
    fn os_shim_name_prefixes_symbol() {
        assert_eq!(os_shim_name("write"), "os_shim_write");
    }

    #[test]
    fn void_return_for_exit() {
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
        assert_eq!(c_return_type(&f), "void");
    }
}
