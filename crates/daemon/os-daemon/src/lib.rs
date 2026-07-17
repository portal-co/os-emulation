//! Generic on-the-fly transformation daemon.
//!
//! Unlike a daemon hardcoded to one transformation strategy, `Daemon` holds
//! a registry of [`TransformBackend`] implementations (an AOT recompiler, a
//! dylib/so rewriter, a future JIT, ...) and routes each `Obtain` request to
//! whichever backend the client names on the wire.

use os_daemon_protocol::{decode_request, encode_response, read_frame, write_frame, Request, Response};
use os_transform_core::{ObtainError, RunAs, TransformBackend};
use std::collections::HashMap;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// New canonical env var. Checked before the legacy `speet`-era fallback
/// chain so existing deployments keep working unmodified.
pub const SOCKET_ENV_VAR: &str = "SOEL_DAEMON_SOCK";

pub struct Daemon {
    backends: Mutex<HashMap<&'static str, Box<dyn TransformBackend>>>,
}

impl Default for Daemon {
    fn default() -> Self {
        Self::new()
    }
}

impl Daemon {
    pub fn new() -> Self {
        Self {
            backends: Mutex::new(HashMap::new()),
        }
    }

    /// Register a backend under its own [`os_transform_core::BackendId`].
    /// Registering a second backend under the same id replaces the first.
    pub fn register(&mut self, backend: Box<dyn TransformBackend>) {
        self.backends.get_mut().unwrap().insert(backend.id().0, backend);
    }

    pub fn handle_frame(&self, frame: &[u8]) -> Vec<u8> {
        let req = match decode_request(frame) {
            Ok(r) => r,
            Err(_) => {
                return encode_response(&Response::Error {
                    message: "invalid request".into(),
                });
            }
        };
        encode_response(&self.dispatch(req))
    }

    fn dispatch(&self, req: Request) -> Response {
        match req {
            Request::Ping => Response::Pong,
            Request::Analyze { path, backend } => self.handle_analyze(&path, backend.as_deref()),
            Request::Obtain { path, backend } => self.handle_obtain(&path, &backend),
            Request::ListBackends => {
                let backends = self.backends.lock().unwrap();
                let mut ids: Vec<String> = backends.keys().map(|id| id.to_string()).collect();
                ids.sort();
                Response::Backends { ids }
            }
        }
    }

    fn handle_analyze(&self, path: &str, backend: Option<&str>) -> Response {
        let backends = self.backends.lock().unwrap();
        match backend {
            Some(id) => match backends.get(id) {
                Some(b) => analyze_one(id, b.as_ref(), path),
                None => unknown_backend(id),
            },
            None => {
                // No backend named: try every registered backend, first
                // suitable one wins. This keeps a sensible default for
                // clients that predate `ListBackends`-based selection.
                let mut ids: Vec<&&'static str> = backends.keys().collect();
                ids.sort();
                let mut last = Response::Unsuitable {
                    backend: String::new(),
                    reasons: vec!["no backend registered".to_string()],
                };
                for id in ids {
                    let b = &backends[id];
                    match analyze_one(id, b.as_ref(), path) {
                        r @ Response::Suitable { .. } => return r,
                        other => last = other,
                    }
                }
                last
            }
        }
    }

    fn handle_obtain(&self, path: &str, backend: &str) -> Response {
        let mut backends = self.backends.lock().unwrap();
        let Some(b) = backends.get_mut(backend) else {
            return unknown_backend(backend);
        };
        match b.obtain(Path::new(path)) {
            // `cache_hit` mirrors today's speet-rtd behavior, which also
            // always reports `false` here rather than tracking real cache
            // hits at this layer.
            Ok(RunAs::Exec(exe)) => Response::Ready {
                exe_path: exe.display().to_string(),
                cache_hit: false,
                backend: backend.to_string(),
            },
            Err(ObtainError::Unsuitable(s)) => Response::Unsuitable {
                backend: backend.to_string(),
                reasons: s.reasons,
            },
            Err(ObtainError::TransformFailed(e)) => Response::Error { message: e },
        }
    }

    pub fn run_on_listener(self, listener: UnixListener) -> Result<(), String> {
        for stream in listener.incoming() {
            let stream = stream.map_err(|e| e.to_string())?;
            let _ = handle_client(&self, stream);
        }
        Ok(())
    }
}

fn analyze_one(id: &str, backend: &dyn TransformBackend, path: &str) -> Response {
    match backend.analyze(Path::new(path)) {
        Ok(s) if s.suitable => Response::Suitable {
            backend: id.to_string(),
        },
        Ok(s) => Response::Unsuitable {
            backend: id.to_string(),
            reasons: s.reasons,
        },
        Err(e) => Response::Error { message: e },
    }
}

fn unknown_backend(id: &str) -> Response {
    Response::Error {
        message: format!("unknown backend: {id}"),
    }
}

fn handle_client(daemon: &Daemon, stream: UnixStream) -> Result<(), String> {
    let mut reader = stream.try_clone().map_err(|e| e.to_string())?;
    let frame = read_frame(&mut reader)?;
    let resp = daemon.handle_frame(&frame);
    let mut sock = stream;
    write_frame(&mut sock, &resp)?;
    Ok(())
}

pub fn bind_socket(path: &Path) -> Result<UnixListener, String> {
    if path.exists() {
        let _ = std::fs::remove_file(path);
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    UnixListener::bind(path).map_err(|e| e.to_string())
}

pub fn default_listen_path() -> PathBuf {
    if let Ok(p) = std::env::var(SOCKET_ENV_VAR) {
        return PathBuf::from(p);
    }
    if let Ok(dir) = std::env::var("XDG_RUNTIME_DIR") {
        return PathBuf::from(dir).join("soel-daemon.sock");
    }
    if let Ok(home) = std::env::var("HOME") {
        return PathBuf::from(home).join(".cache/soel/daemon.sock");
    }
    PathBuf::from("/tmp/soel-daemon.sock")
}

#[cfg(test)]
mod tests {
    use super::*;
    use os_transform_core::{BackendId, Suitability};

    struct StubBackend {
        id: BackendId,
        suitable: bool,
        out: PathBuf,
    }

    impl TransformBackend for StubBackend {
        fn id(&self) -> BackendId {
            self.id
        }
        fn analyze(&self, _path: &Path) -> Result<Suitability, String> {
            Ok(Suitability {
                suitable: self.suitable,
                reasons: if self.suitable {
                    vec![]
                } else {
                    vec!["stub says no".to_string()]
                },
            })
        }
        fn obtain(&mut self, _path: &Path) -> Result<RunAs, ObtainError> {
            if self.suitable {
                Ok(RunAs::Exec(self.out.clone()))
            } else {
                Err(ObtainError::Unsuitable(Suitability {
                    suitable: false,
                    reasons: vec!["stub says no".to_string()],
                }))
            }
        }
    }

    fn daemon_with_two_backends() -> Daemon {
        let mut d = Daemon::new();
        d.register(Box::new(StubBackend {
            id: BackendId("a"),
            suitable: true,
            out: PathBuf::from("/tmp/a-out"),
        }));
        d.register(Box::new(StubBackend {
            id: BackendId("b"),
            suitable: true,
            out: PathBuf::from("/tmp/b-out"),
        }));
        d
    }

    #[test]
    fn ping() {
        let d = Daemon::new();
        assert_eq!(d.dispatch(Request::Ping), Response::Pong);
    }

    #[test]
    fn list_backends_returns_all_registered() {
        let d = daemon_with_two_backends();
        match d.dispatch(Request::ListBackends) {
            Response::Backends { ids } => assert_eq!(ids, vec!["a".to_string(), "b".to_string()]),
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn obtain_routes_to_the_named_backend() {
        let d = daemon_with_two_backends();
        match d.dispatch(Request::Obtain {
            path: "/bin/ls".into(),
            backend: "a".into(),
        }) {
            Response::Ready { exe_path, backend, .. } => {
                assert_eq!(exe_path, "/tmp/a-out");
                assert_eq!(backend, "a");
            }
            other => panic!("unexpected: {other:?}"),
        }
        match d.dispatch(Request::Obtain {
            path: "/bin/ls".into(),
            backend: "b".into(),
        }) {
            Response::Ready { exe_path, backend, .. } => {
                assert_eq!(exe_path, "/tmp/b-out");
                assert_eq!(backend, "b");
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn obtain_unknown_backend_is_an_error() {
        let d = daemon_with_two_backends();
        match d.dispatch(Request::Obtain {
            path: "/bin/ls".into(),
            backend: "nope".into(),
        }) {
            Response::Error { message } => assert!(message.contains("nope")),
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn analyze_without_backend_returns_first_suitable() {
        let mut d = Daemon::new();
        d.register(Box::new(StubBackend {
            id: BackendId("unsuitable-one"),
            suitable: false,
            out: PathBuf::from("/tmp/x"),
        }));
        d.register(Box::new(StubBackend {
            id: BackendId("suitable-one"),
            suitable: true,
            out: PathBuf::from("/tmp/y"),
        }));
        match d.dispatch(Request::Analyze {
            path: "/bin/ls".into(),
            backend: None,
        }) {
            Response::Suitable { backend } => assert_eq!(backend, "suitable-one"),
            other => panic!("unexpected: {other:?}"),
        }
    }
}
