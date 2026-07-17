//! Generator for the minimal C execve-interposition stub linked into
//! guest/target binaries.
//!
//! The stub has no Rust runtime dependency (it must be embeddable into
//! arbitrary target binaries produced by any recompiler/rewriter), but it is
//! now generic over which [`os_transform_core::TransformBackend`] it asks
//! for: the backend id is a generation-time parameter instead of a wire
//! literal hardcoded to speet's AOT recompiler.

/// Emit C source implementing `int <hook_symbol_name>(path, argv, envp)`
/// that consults the daemon (`os-daemon`) over the v2 wire protocol
/// (`os-daemon-protocol`), selecting `backend_id` explicitly, then calls
/// `execve()` on whatever path the daemon returns — falling back to the
/// original `path` unchanged if the daemon is unreachable or returns an
/// error.
///
/// `socket_env_var` is the environment variable checked first for the
/// daemon's Unix socket path (e.g. `"SPEET_RTD_SOCK"` for a speet-shim
/// caller, `"SOEL_DAEMON_SOCK"` for a from-scratch consumer); if unset, the
/// stub falls back to `$XDG_RUNTIME_DIR/soel-daemon.sock`, then
/// `$HOME/.cache/soel/daemon.sock`, then `/tmp/soel-daemon.sock` — the exact
/// chain `os_daemon::default_listen_path` computes on the Rust side, so a
/// hook and daemon that agree on `socket_env_var` (or both leave it unset)
/// always agree on where to connect.
pub fn generate_execve_hook_c(hook_symbol_name: &str, backend_id: &str, socket_env_var: &str) -> String {
    format!(
        r#"#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>
#include <sys/socket.h>
#include <sys/un.h>

static const char *rtd_socket_path(void) {{
    const char *env = getenv("{socket_env_var}");
    if (env && env[0]) return env;
    const char *xdg = getenv("XDG_RUNTIME_DIR");
    if (xdg && xdg[0]) {{
        static char buf[512];
        snprintf(buf, sizeof(buf), "%s/soel-daemon.sock", xdg);
        return buf;
    }}
    const char *home = getenv("HOME");
    if (home && home[0]) {{
        static char buf[512];
        snprintf(buf, sizeof(buf), "%s/.cache/soel/daemon.sock", home);
        return buf;
    }}
    return "/tmp/soel-daemon.sock";
}}

static int wire_write_u32(uint8_t *out, size_t cap, size_t *pos, uint32_t v) {{
    if (*pos + 4 > cap) return -1;
    out[*pos + 0] = (uint8_t)(v);
    out[*pos + 1] = (uint8_t)(v >> 8);
    out[*pos + 2] = (uint8_t)(v >> 16);
    out[*pos + 3] = (uint8_t)(v >> 24);
    *pos += 4;
    return 0;
}}

static int wire_write_str(uint8_t *out, size_t cap, size_t *pos, const char *s) {{
    size_t slen = strlen(s);
    if (wire_write_u32(out, cap, pos, (uint32_t)slen) != 0) return -1;
    if (*pos + slen > cap) return -1;
    memcpy(out + *pos, s, slen);
    *pos += slen;
    return 0;
}}

static int wire_read_u32(const uint8_t *in, size_t len, size_t *pos, uint32_t *out) {{
    if (*pos + 4 > len) return -1;
    *out = (uint32_t)in[*pos]
         | ((uint32_t)in[*pos + 1] << 8)
         | ((uint32_t)in[*pos + 2] << 16)
         | ((uint32_t)in[*pos + 3] << 24);
    *pos += 4;
    return 0;
}}

static int wire_read_str(const uint8_t *in, size_t len, size_t *pos, char *out, size_t out_cap) {{
    uint32_t slen = 0;
    if (wire_read_u32(in, len, pos, &slen) != 0) return -1;
    if (*pos + slen > len || slen + 1 > out_cap) return -1;
    memcpy(out, in + *pos, slen);
    out[slen] = '\0';
    *pos += slen;
    return 0;
}}

static int rtd_obtain_path(const char *guest_path, char *out, size_t out_len) {{
    int fd = socket(AF_UNIX, SOCK_STREAM, 0);
    if (fd < 0) return -1;
    struct sockaddr_un addr;
    memset(&addr, 0, sizeof(addr));
    addr.sun_family = AF_UNIX;
    const char *sock = rtd_socket_path();
    if (strlen(sock) >= sizeof(addr.sun_path)) {{ close(fd); return -1; }}
    strncpy(addr.sun_path, sock, sizeof(addr.sun_path) - 1);
    if (connect(fd, (struct sockaddr *)&addr, sizeof(addr)) != 0) {{ close(fd); return -1; }}

    uint8_t req[1024];
    size_t pos = 0;
    req[pos++] = 2; /* PROTOCOL_VERSION */
    req[pos++] = 2; /* OP_OBTAIN */
    size_t payload_start = pos;
    pos += 4;
    size_t payload_pos = 0;
    uint8_t payload[512];
    if (wire_write_str(payload, sizeof(payload), &payload_pos, guest_path) != 0) {{ close(fd); return -1; }}
    if (wire_write_str(payload, sizeof(payload), &payload_pos, "{backend_id}") != 0) {{ close(fd); return -1; }}
    if (wire_write_u32(req, sizeof(req), &payload_start, (uint32_t)payload_pos) != 0) {{ close(fd); return -1; }}
    if (pos + payload_pos > sizeof(req)) {{ close(fd); return -1; }}
    memcpy(req + pos, payload, payload_pos);
    pos += payload_pos;

    if (write(fd, req, pos) != (ssize_t)pos) {{ close(fd); return -1; }}

    uint8_t hdr[6];
    ssize_t r = read(fd, hdr, sizeof(hdr));
    if (r != (ssize_t)sizeof(hdr)) {{ close(fd); return -1; }}
    uint32_t plen = 0;
    size_t p = 2;
    if (wire_read_u32(hdr, sizeof(hdr), &p, &plen) != 0) {{ close(fd); return -1; }}
    if (hdr[0] != 2 || hdr[1] != 3) {{ close(fd); return -1; }} /* STATUS_READY */

    uint8_t body[2048];
    if (plen > sizeof(body)) {{ close(fd); return -1; }}
    r = read(fd, body, plen);
    close(fd);
    if (r != (ssize_t)plen) return -1;

    /* `exe_path` is the leading string field of the Ready payload; any
       trailing cache_hit/backend fields are ignored here. */
    size_t bp = 0;
    return wire_read_str(body, plen, &bp, out, out_len);
}}

int {hook_symbol_name}(const char *path, char *const argv[], char *const envp[]) {{
    char cached[1024];
    if (rtd_obtain_path(path, cached, sizeof(cached)) == 0) {{
        return execve(cached, argv, envp);
    }}
    return execve(path, argv, envp);
}}
"#
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embeds_the_requested_symbol_name_and_backend_id() {
        let src = generate_execve_hook_c("__speet_execve_hook", "integrated", "SPEET_RTD_SOCK");
        assert!(src.contains("__speet_execve_hook"));
        assert!(src.contains("\"integrated\""));
        assert!(src.contains("SPEET_RTD_SOCK"));
        assert!(src.contains("PROTOCOL_VERSION"));
    }

    #[test]
    fn different_backend_ids_produce_different_source() {
        let a = generate_execve_hook_c("__os_execve_hook", "simple-rewrite", "SOEL_DAEMON_SOCK");
        assert!(a.contains("\"simple-rewrite\""));
        assert!(!a.contains("__speet_execve_hook"));
    }
}
