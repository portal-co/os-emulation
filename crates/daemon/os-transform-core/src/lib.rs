//! Backend-agnostic contract for on-the-fly binary transformation.
//!
//! `os-transform-core` defines [`TransformBackend`], the trait a transform
//! daemon (`os-daemon`) dispatches to. It exists so the daemon and its wire
//! protocol (`os-daemon-protocol`) never hardcode which transformation
//! strategy produced a runnable artifact — ahead-of-time recompilation,
//! a future JIT, and dylib/so rewriting are all just implementations of
//! this one trait, selected at request time by [`BackendId`].

use std::path::{Path, PathBuf};

/// Stable identifier a client selects on the wire.
///
/// This is a plain string newtype rather than a closed enum because the set
/// of backends is open-ended (new transformation strategies register
/// without changing this crate), the same way `HostApi` deliberately avoids
/// a closed backend enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BackendId(pub &'static str);

impl BackendId {
    /// speet's existing ahead-of-time full-recompile pipeline.
    pub const INTEGRATED_RECOMPILE: BackendId = BackendId("integrated");
    /// The dylib/so load-command rewriter (macOS Mach-O, BSD/libc-Linux ELF).
    pub const SIMPLE_REWRITE: BackendId = BackendId("simple-rewrite");
}

impl core::fmt::Display for BackendId {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.0)
    }
}

/// Whether a backend can usefully transform a given input.
///
/// `reasons` is free-form rather than a fixed set of typed fields because
/// different backends fail for structurally different reasons (an AOT
/// recompiler cares about unresolved imports/function-pointer dependencies;
/// a rewriter cares about whether a load-command slot is free).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Suitability {
    pub suitable: bool,
    pub reasons: Vec<String>,
}

/// Failure to obtain a transformed executable.
#[derive(Debug, Clone)]
pub enum ObtainError {
    Unsuitable(Suitability),
    TransformFailed(String),
}

impl core::fmt::Display for ObtainError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            ObtainError::Unsuitable(s) => write!(f, "unsuitable: {:?}", s.reasons),
            ObtainError::TransformFailed(e) => write!(f, "transform failed: {e}"),
        }
    }
}

impl std::error::Error for ObtainError {}

/// How the caller should run a successfully obtained artifact.
///
/// Every backend in this plan produces `Exec` — a brand-new, cached,
/// directly executable file — because both the AOT recompiler and the
/// simple rewriter are "run in place" designs, never an in-process patch or
/// a separate interpreter loop. The variant exists (rather than just
/// returning a `PathBuf`) so a future JIT/in-process backend can add
/// `InProcess` without changing the trait signature.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RunAs {
    /// Caller should `execve` this path directly.
    Exec(PathBuf),
}

/// A pluggable on-the-fly transformation strategy.
///
/// Implementations never mutate the input at `path` — they always read it
/// and produce a new, separately cached artifact. This holds for both the
/// existing AOT recompiler (speet's `IntegratedNativeRuntime`) and the new
/// dylib/so rewriter.
pub trait TransformBackend: Send {
    /// Stable id this backend registers under; must match the `backend`
    /// string clients pass on the wire.
    fn id(&self) -> BackendId;

    /// Cheap-ish check: can this backend do anything useful with `path`?
    fn analyze(&self, path: &Path) -> Result<Suitability, String>;

    /// Produce (or fetch from cache) a runnable artifact for `path`.
    fn obtain(&mut self, path: &Path) -> Result<RunAs, ObtainError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    struct StubBackend {
        id: BackendId,
        out: PathBuf,
    }

    impl TransformBackend for StubBackend {
        fn id(&self) -> BackendId {
            self.id
        }
        fn analyze(&self, _path: &Path) -> Result<Suitability, String> {
            Ok(Suitability {
                suitable: true,
                reasons: vec![],
            })
        }
        fn obtain(&mut self, _path: &Path) -> Result<RunAs, ObtainError> {
            Ok(RunAs::Exec(self.out.clone()))
        }
    }

    #[test]
    fn backend_id_display() {
        assert_eq!(BackendId::INTEGRATED_RECOMPILE.to_string(), "integrated");
        assert_eq!(BackendId::SIMPLE_REWRITE.to_string(), "simple-rewrite");
    }

    #[test]
    fn stub_backend_roundtrip() {
        let mut b = StubBackend {
            id: BackendId("stub"),
            out: PathBuf::from("/tmp/out"),
        };
        assert_eq!(b.id().0, "stub");
        assert!(b.analyze(Path::new("/tmp/in")).unwrap().suitable);
        match b.obtain(Path::new("/tmp/in")).unwrap() {
            RunAs::Exec(p) => assert_eq!(p, PathBuf::from("/tmp/out")),
        }
    }
}
