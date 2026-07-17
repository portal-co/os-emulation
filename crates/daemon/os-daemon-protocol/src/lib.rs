//! Hand-rolled binary wire codec for the generic transform daemon
//! (`os-daemon`).
//!
//! Frame shape: `[u8 version][u8 tag][u32 LE payload_len][payload…]`.
//!
//! This is protocol version 2. Version 1 (`speet_runtime::rtd_protocol`)
//! carried an `Obtain{path, host_id}` request whose `host_id` field was
//! parsed but never honored by the server — a backend-selection field that
//! silently did nothing. Version 2 makes backend selection an explicit,
//! required, and honored part of the wire contract instead of continuing to
//! carry a field that lies about what it does.

use std::io::{Read, Write};

pub const PROTOCOL_VERSION: u8 = 2;

pub const OP_PING: u8 = 0;
pub const OP_ANALYZE: u8 = 1;
pub const OP_OBTAIN: u8 = 2;
pub const OP_LIST_BACKENDS: u8 = 3;

pub const STATUS_PONG: u8 = 0;
pub const STATUS_SUITABLE: u8 = 1;
pub const STATUS_UNSUITABLE: u8 = 2;
pub const STATUS_READY: u8 = 3;
pub const STATUS_ERROR: u8 = 4;
pub const STATUS_BACKENDS: u8 = 5;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WireError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Request {
    Ping,
    /// `backend: None` asks every registered backend and returns the first
    /// suitable one (keeps a natural default for `ListBackends`-unaware
    /// clients); `Some(id)` asks one specific backend.
    Analyze {
        path: String,
        backend: Option<String>,
    },
    /// `backend` selects which registered `TransformBackend` handles the
    /// request; unlike v1's `host_id`, this is required and honored.
    Obtain {
        path: String,
        backend: String,
    },
    ListBackends,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Response {
    Pong,
    Suitable {
        backend: String,
    },
    Unsuitable {
        backend: String,
        reasons: Vec<String>,
    },
    Ready {
        exe_path: String,
        cache_hit: bool,
        backend: String,
    },
    Error {
        message: String,
    },
    Backends {
        ids: Vec<String>,
    },
}

pub fn encode_request(req: &Request) -> Vec<u8> {
    let mut payload = Vec::new();
    let tag = match req {
        Request::Ping => OP_PING,
        Request::Analyze { path, backend } => {
            encode_string(path, &mut payload);
            encode_option_string(backend.as_deref(), &mut payload);
            OP_ANALYZE
        }
        Request::Obtain { path, backend } => {
            encode_string(path, &mut payload);
            encode_string(backend, &mut payload);
            OP_OBTAIN
        }
        Request::ListBackends => OP_LIST_BACKENDS,
    };
    encode_frame(tag, &payload)
}

pub fn decode_request(buf: &[u8]) -> Result<Request, WireError> {
    let (tag, payload) = decode_frame(buf)?;
    match tag {
        OP_PING if payload.is_empty() => Ok(Request::Ping),
        OP_ANALYZE => {
            let (path, rest) = decode_string(payload)?;
            let (backend, rest) = decode_option_string(rest)?;
            if !rest.is_empty() {
                return Err(WireError);
            }
            Ok(Request::Analyze { path, backend })
        }
        OP_OBTAIN => {
            let (path, rest) = decode_string(payload)?;
            let (backend, rest) = decode_string(rest)?;
            if !rest.is_empty() {
                return Err(WireError);
            }
            Ok(Request::Obtain { path, backend })
        }
        OP_LIST_BACKENDS if payload.is_empty() => Ok(Request::ListBackends),
        _ => Err(WireError),
    }
}

pub fn encode_response(resp: &Response) -> Vec<u8> {
    let mut payload = Vec::new();
    let tag = match resp {
        Response::Pong => STATUS_PONG,
        Response::Suitable { backend } => {
            encode_string(backend, &mut payload);
            STATUS_SUITABLE
        }
        Response::Unsuitable { backend, reasons } => {
            encode_string(backend, &mut payload);
            encode_string_vec(reasons, &mut payload);
            STATUS_UNSUITABLE
        }
        Response::Ready {
            exe_path,
            cache_hit,
            backend,
        } => {
            encode_string(exe_path, &mut payload);
            payload.push(u8::from(*cache_hit));
            encode_string(backend, &mut payload);
            STATUS_READY
        }
        Response::Error { message } => {
            encode_string(message, &mut payload);
            STATUS_ERROR
        }
        Response::Backends { ids } => {
            encode_string_vec(ids, &mut payload);
            STATUS_BACKENDS
        }
    };
    encode_frame(tag, &payload)
}

pub fn decode_response(buf: &[u8]) -> Result<Response, WireError> {
    let (tag, payload) = decode_frame(buf)?;
    match tag {
        STATUS_PONG if payload.is_empty() => Ok(Response::Pong),
        STATUS_SUITABLE => {
            let (backend, rest) = decode_string(payload)?;
            if !rest.is_empty() {
                return Err(WireError);
            }
            Ok(Response::Suitable { backend })
        }
        STATUS_UNSUITABLE => {
            let (backend, rest) = decode_string(payload)?;
            let (reasons, rest) = decode_string_vec(rest)?;
            if !rest.is_empty() {
                return Err(WireError);
            }
            Ok(Response::Unsuitable { backend, reasons })
        }
        STATUS_READY => {
            let (exe_path, rest) = decode_string(payload)?;
            if rest.is_empty() {
                return Err(WireError);
            }
            let cache_hit = rest[0] != 0;
            let (backend, rest) = decode_string(&rest[1..])?;
            if !rest.is_empty() {
                return Err(WireError);
            }
            Ok(Response::Ready {
                exe_path,
                cache_hit,
                backend,
            })
        }
        STATUS_ERROR => {
            let (message, rest) = decode_string(payload)?;
            if !rest.is_empty() {
                return Err(WireError);
            }
            Ok(Response::Error { message })
        }
        STATUS_BACKENDS => {
            let (ids, rest) = decode_string_vec(payload)?;
            if !rest.is_empty() {
                return Err(WireError);
            }
            Ok(Response::Backends { ids })
        }
        _ => Err(WireError),
    }
}

pub fn read_frame<R: Read>(r: &mut R) -> Result<Vec<u8>, String> {
    let mut hdr = [0u8; 6];
    r.read_exact(&mut hdr).map_err(|e| e.to_string())?;
    let len = u32::from_le_bytes(hdr[2..6].try_into().unwrap()) as usize;
    let mut frame = Vec::with_capacity(6 + len);
    frame.extend_from_slice(&hdr);
    if len > 0 {
        let mut payload = vec![0u8; len];
        r.read_exact(&mut payload).map_err(|e| e.to_string())?;
        frame.extend_from_slice(&payload);
    }
    Ok(frame)
}

pub fn write_frame<W: Write>(w: &mut W, frame: &[u8]) -> Result<(), String> {
    w.write_all(frame).map_err(|e| e.to_string())?;
    w.flush().map_err(|e| e.to_string())
}

fn encode_frame(tag: u8, payload: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(6 + payload.len());
    out.push(PROTOCOL_VERSION);
    out.push(tag);
    out.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    out.extend_from_slice(payload);
    out
}

fn decode_frame(buf: &[u8]) -> Result<(u8, &[u8]), WireError> {
    if buf.len() < 6 || buf[0] != PROTOCOL_VERSION {
        return Err(WireError);
    }
    let tag = buf[1];
    let len = u32::from_le_bytes(buf[2..6].try_into().unwrap()) as usize;
    if buf.len() != 6 + len {
        return Err(WireError);
    }
    Ok((tag, &buf[6..]))
}

fn encode_string(s: &str, out: &mut Vec<u8>) {
    let bytes = s.as_bytes();
    out.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
    out.extend_from_slice(bytes);
}

fn decode_string(input: &[u8]) -> Result<(String, &[u8]), WireError> {
    if input.len() < 4 {
        return Err(WireError);
    }
    let len = u32::from_le_bytes(input[0..4].try_into().unwrap()) as usize;
    let rest = &input[4..];
    if rest.len() < len {
        return Err(WireError);
    }
    let s = std::str::from_utf8(&rest[..len]).map_err(|_| WireError)?;
    Ok((s.to_string(), &rest[len..]))
}

/// `Some(s)` encodes as `1` then the string; `None` encodes as a lone `0`
/// byte (distinct from a zero-length string, which is still tag `1` with a
/// zero-length payload).
fn encode_option_string(s: Option<&str>, out: &mut Vec<u8>) {
    match s {
        None => out.push(0),
        Some(s) => {
            out.push(1);
            encode_string(s, out);
        }
    }
}

fn decode_option_string(input: &[u8]) -> Result<(Option<String>, &[u8]), WireError> {
    if input.is_empty() {
        return Err(WireError);
    }
    match input[0] {
        0 => Ok((None, &input[1..])),
        1 => {
            let (s, rest) = decode_string(&input[1..])?;
            Ok((Some(s), rest))
        }
        _ => Err(WireError),
    }
}

fn encode_string_vec(items: &[String], out: &mut Vec<u8>) {
    out.extend_from_slice(&(items.len() as u32).to_le_bytes());
    for item in items {
        encode_string(item, out);
    }
}

fn decode_string_vec(input: &[u8]) -> Result<(Vec<String>, &[u8]), WireError> {
    if input.len() < 4 {
        return Err(WireError);
    }
    let len = u32::from_le_bytes(input[0..4].try_into().unwrap()) as usize;
    let mut rest = &input[4..];
    let mut items = Vec::with_capacity(len);
    for _ in 0..len {
        let (s, r) = decode_string(rest)?;
        items.push(s);
        rest = r;
    }
    Ok((items, rest))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ping_roundtrip() {
        let frame = encode_request(&Request::Ping);
        assert_eq!(decode_request(&frame).unwrap(), Request::Ping);
        let resp = encode_response(&Response::Pong);
        assert_eq!(decode_response(&resp).unwrap(), Response::Pong);
    }

    #[test]
    fn analyze_roundtrip_with_and_without_backend() {
        let frame = encode_request(&Request::Analyze {
            path: "/bin/ls".into(),
            backend: None,
        });
        assert_eq!(
            decode_request(&frame).unwrap(),
            Request::Analyze {
                path: "/bin/ls".into(),
                backend: None,
            }
        );

        let frame = encode_request(&Request::Analyze {
            path: "/bin/ls".into(),
            backend: Some("integrated".into()),
        });
        assert_eq!(
            decode_request(&frame).unwrap(),
            Request::Analyze {
                path: "/bin/ls".into(),
                backend: Some("integrated".into()),
            }
        );
    }

    #[test]
    fn obtain_roundtrip() {
        let frame = encode_request(&Request::Obtain {
            path: "/bin/ls".into(),
            backend: "integrated".into(),
        });
        match decode_request(&frame).unwrap() {
            Request::Obtain { path, backend } => {
                assert_eq!(path, "/bin/ls");
                assert_eq!(backend, "integrated");
            }
            _ => panic!("expected obtain"),
        }
    }

    #[test]
    fn ready_roundtrip() {
        let resp = encode_response(&Response::Ready {
            exe_path: "/tmp/x".into(),
            cache_hit: true,
            backend: "simple-rewrite".into(),
        });
        match decode_response(&resp).unwrap() {
            Response::Ready {
                exe_path,
                cache_hit,
                backend,
            } => {
                assert_eq!(exe_path, "/tmp/x");
                assert!(cache_hit);
                assert_eq!(backend, "simple-rewrite");
            }
            _ => panic!("expected ready"),
        }
    }

    #[test]
    fn list_backends_roundtrip() {
        let frame = encode_request(&Request::ListBackends);
        assert_eq!(decode_request(&frame).unwrap(), Request::ListBackends);

        let resp = encode_response(&Response::Backends {
            ids: vec!["integrated".into(), "simple-rewrite".into()],
        });
        match decode_response(&resp).unwrap() {
            Response::Backends { ids } => {
                assert_eq!(ids, vec!["integrated".to_string(), "simple-rewrite".to_string()]);
            }
            _ => panic!("expected backends"),
        }
    }

    #[test]
    fn wrong_protocol_version_rejected() {
        let mut frame = encode_request(&Request::Ping);
        frame[0] = 1; // v1
        assert_eq!(decode_request(&frame), Err(WireError));
    }
}
