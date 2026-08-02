fn main() {
    let include = std::path::Path::new("include");
    let generated = std::path::Path::new("src/generated");
    let mut build = cc::Build::new();
    build.include(include);
    for entry in [
        "write.c",
        "exit.c",
        "printf.c",
        "putchar.c",
        "strlen.c",
        "getenv.c",
        "execve.c",
    ] {
        build.file(generated.join(entry));
    }
    build.compile("os_shim_core");
}
