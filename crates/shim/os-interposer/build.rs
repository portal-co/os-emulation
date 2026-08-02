fn main() {
    let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let core_include = manifest_dir.join("../os-shim-core/include");
    let core_generated = manifest_dir.join("../os-shim-core/src/generated");
    let interposer_generated = manifest_dir.join("src/generated");
    let out_dir = std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap());

    let mut build = cc::Build::new();
    build
        .include(&core_include)
        .file(interposer_generated.join("interposer_init.c"));

    for name in [
        "write.c",
        "exit.c",
        "printf.c",
        "putchar.c",
        "strlen.c",
        "getenv.c",
        "execve.c",
    ] {
        build.file(core_generated.join(name));
        build.file(interposer_generated.join(name));
    }

    if std::env::var("CARGO_FEATURE_DAEMON").is_ok() {
        use os_abi_spec::{AbiArg, AbiFunction, AbiValueKind};
        use os_shim_handler::ShimHandler;
        let func = AbiFunction {
            name: "execve".into(),
            args: vec![
                AbiArg {
                    kind: AbiValueKind::Pointer,
                    bridgesupport_type: Some("*".into()),
                    function_pointer: false,
                    pointer: true,
                },
                AbiArg {
                    kind: AbiValueKind::Pointer,
                    bridgesupport_type: Some("^*".into()),
                    function_pointer: false,
                    pointer: true,
                },
                AbiArg {
                    kind: AbiValueKind::Pointer,
                    bridgesupport_type: Some("^*".into()),
                    function_pointer: false,
                    pointer: true,
                },
            ],
            retval: AbiArg {
                kind: AbiValueKind::Scalar,
                bridgesupport_type: Some("i".into()),
                function_pointer: false,
                pointer: false,
            },
            variadic: false,
        };
        let src = os_daemon_shim::DaemonExecveHandler::default().emit_core(&func);
        let daemon_path = out_dir.join("execve_daemon.c");
        std::fs::write(&daemon_path, src).expect("write execve_daemon.c");
        build.file(daemon_path);
    }

    build.shared_flag(true);
    build.compile("os_interposer");

    println!("cargo:rustc-env=OS_INTERPOSER_OUT_DIR={}", out_dir.display());
    println!("cargo:rustc-link-search=native={}", out_dir.display());
    println!("cargo:rustc-link-lib=dylib=os_interposer");
}
