//! End-to-end test: two stub backends registered under different ids,
//! connected to over a real Unix socket, confirming `Obtain` routes to the
//! right one and `ListBackends` reports both. This is the core regression
//! test proving backend selection is honored, not silently ignored like the
//! old `host_id` field.

use os_daemon::{bind_socket, Daemon};
use os_daemon_protocol::{decode_response, encode_request, read_frame, write_frame, Request, Response};
use os_transform_core::{BackendId, ObtainError, RunAs, Suitability, TransformBackend};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::thread;

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

fn roundtrip(sock: &mut UnixStream, req: &Request) -> Response {
    write_frame(sock, &encode_request(req)).unwrap();
    let frame = read_frame(sock).unwrap();
    decode_response(&frame).unwrap()
}

#[test]
fn obtain_and_list_backends_over_a_real_socket() {
    let dir = tempfile::tempdir().unwrap();
    let socket_path = dir.path().join("daemon.sock");

    let mut daemon = Daemon::new();
    daemon.register(Box::new(StubBackend {
        id: BackendId("a"),
        out: PathBuf::from("/tmp/a-exe"),
    }));
    daemon.register(Box::new(StubBackend {
        id: BackendId("b"),
        out: PathBuf::from("/tmp/b-exe"),
    }));

    let listener = bind_socket(&socket_path).unwrap();
    let server = thread::spawn(move || {
        // Three client connections: two Obtain calls + one ListBackends.
        for _ in 0..3 {
            let stream = listener.incoming().next().unwrap().unwrap();
            let mut reader = stream.try_clone().unwrap();
            let frame = read_frame(&mut reader).unwrap();
            let resp = daemon.handle_frame(&frame);
            let mut sock = stream;
            write_frame(&mut sock, &resp).unwrap();
        }
    });

    let mut sock = UnixStream::connect(&socket_path).unwrap();
    match roundtrip(
        &mut sock,
        &Request::Obtain {
            path: "/bin/ls".into(),
            backend: "a".into(),
        },
    ) {
        Response::Ready { exe_path, backend, .. } => {
            assert_eq!(exe_path, "/tmp/a-exe");
            assert_eq!(backend, "a");
        }
        other => panic!("unexpected: {other:?}"),
    }

    let mut sock = UnixStream::connect(&socket_path).unwrap();
    match roundtrip(
        &mut sock,
        &Request::Obtain {
            path: "/bin/ls".into(),
            backend: "b".into(),
        },
    ) {
        Response::Ready { exe_path, backend, .. } => {
            assert_eq!(exe_path, "/tmp/b-exe");
            assert_eq!(backend, "b");
        }
        other => panic!("unexpected: {other:?}"),
    }

    let mut sock = UnixStream::connect(&socket_path).unwrap();
    match roundtrip(&mut sock, &Request::ListBackends) {
        Response::Backends { ids } => assert_eq!(ids, vec!["a".to_string(), "b".to_string()]),
        other => panic!("unexpected: {other:?}"),
    }

    server.join().unwrap();
}
