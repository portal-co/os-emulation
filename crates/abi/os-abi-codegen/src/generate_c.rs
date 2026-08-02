//! Generate C core and interposer bridge sources from BridgeSupport + handler registry.

use os_abi_spec::AbiSpec;
use os_daemon_shim::register_daemon_overrides;
use os_shim_handler::ShimRegistry;

use crate::generate::{CodegenConfig, GeneratedFile};

/// Which C outputs to emit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum COutputKind {
    Core,
    InterposerBridge,
}

/// Generate C shim files for `spec` under `config`.
pub fn generate_c(
    spec: &AbiSpec,
    config: &CodegenConfig,
    kinds: &[COutputKind],
    with_daemon: bool,
) -> Result<Vec<GeneratedFile>, String> {
    let symbols = resolve_symbols(spec, config);
    let mut registry = ShimRegistry::new();
    if with_daemon {
        register_daemon_overrides(&mut registry);
    }

    let mut files = Vec::new();
    let emit_core = kinds.contains(&COutputKind::Core);
    let emit_interposer = kinds.contains(&COutputKind::InterposerBridge);

    for sym in &symbols {
        if emit_core {
            if sym == "execve" && with_daemon {
                if let Some(body) = registry.emit_core(spec, sym) {
                    files.push(GeneratedFile {
                        path: format!("core/{sym}.c"),
                        contents: body,
                    });
                }
                continue;
            }
            if let Some(body) = registry.emit_core(spec, sym) {
                files.push(GeneratedFile {
                    path: format!("core/{sym}.c"),
                    contents: body,
                });
            }
        }
        if emit_interposer {
            if let Some(body) = registry.emit_interposer_bridge(spec, sym) {
                files.push(GeneratedFile {
                    path: format!("interposer/{sym}.c"),
                    contents: body,
                });
            }
        }
    }

    if emit_core && with_daemon && !symbols.iter().any(|s| s == "execve") {
        // Daemon override for execve even if not in BridgeSupport subset.
        if let Some(body) = registry.emit_core(spec, "execve") {
            files.push(GeneratedFile {
                path: "core/execve.c".into(),
                contents: body,
            });
        }
    }

    Ok(files)
}

fn resolve_symbols(spec: &AbiSpec, config: &CodegenConfig) -> Vec<String> {
    if config.all_symbols {
        return spec.symbol_names().into_iter().map(str::to_string).collect();
    }
    config.symbols.clone()
}

#[cfg(test)]
mod tests {
    use super::*;
    use os_abi_spec::parse_bridgesupport;

    const SAMPLE: &str = r#"<?xml version="1.0"?>
<signatures version="1.0">
  <function name="write">
    <arg type="i"/>
    <arg type="^v"/>
    <arg type="Q"/>
    <retval type="q"/>
  </function>
  <function name="exit">
    <arg type="i"/>
    <retval type="v"/>
  </function>
</signatures>"#;

    #[test]
    fn generates_weak_core_c() {
        let spec = parse_bridgesupport(SAMPLE).unwrap();
        let files = generate_c(
            &spec,
            &CodegenConfig {
                source_label: "test".into(),
                ..Default::default()
            },
            &[COutputKind::Core],
            false,
        )
        .unwrap();
        let write = files.iter().find(|f| f.path == "core/write.c").unwrap();
        assert!(write.contents.contains("__attribute__((weak))"));
        assert!(write.contents.contains("os_shim_write"));
    }

    #[test]
    fn daemon_execve_is_strong_override() {
        let spec = parse_bridgesupport(
            r#"<?xml version="1.0"?><signatures version="1.0">
  <function name="execve">
    <arg type="*"/>
    <arg type="^*"/>
    <arg type="^*"/>
    <retval type="i"/>
  </function>
</signatures>"#,
        )
        .unwrap();
        let files = generate_c(
            &spec,
            &CodegenConfig {
                symbols: vec!["execve".into()],
                source_label: "test".into(),
                ..Default::default()
            },
            &[COutputKind::Core],
            true,
        )
        .unwrap();
        let execve = files.iter().find(|f| f.path == "core/execve.c").unwrap();
        assert!(execve.contents.contains("int os_shim_execve"));
        assert!(!execve.contents.contains("__attribute__((weak))"));
    }
}
