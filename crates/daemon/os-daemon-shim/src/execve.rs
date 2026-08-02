//! Strong `os_shim_execve` with daemon consult (replaces weak default forward).

use os_abi_spec::AbiFunction;
use os_shim_handler::ShimHandler;

pub const DEFAULT_BACKEND_ID: &str = "integrated";
pub const DEFAULT_SOCKET_ENV: &str = "SPEET_RTD_SOCK";

/// Configurable daemon execve handler for codegen.
#[derive(Debug, Clone)]
pub struct DaemonExecveHandler {
    pub backend_id: String,
    pub socket_env_var: String,
}

impl Default for DaemonExecveHandler {
    fn default() -> Self {
        Self {
            backend_id: DEFAULT_BACKEND_ID.into(),
            socket_env_var: DEFAULT_SOCKET_ENV.into(),
        }
    }
}

impl ShimHandler for DaemonExecveHandler {
    fn symbol(&self) -> &str {
        "execve"
    }

    fn emit_core(&self, _func: &AbiFunction) -> String {
        emit_os_shim_execve(&self.backend_id, &self.socket_env_var)
    }

    fn is_override(&self) -> bool {
        true
    }
}

/// Emit strong `os_shim_execve` C body (no weak attribute).
pub fn emit_os_shim_execve(backend_id: &str, socket_env_var: &str) -> String {
    // Shared wire protocol helpers + daemon consult, adapted from os-daemon-hook.
    format!(
        r#"/* @generated daemon override — execve */
#include "os_shim.h"
#include <stdint.h>
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
    req[pos++] = 2;
    req[pos++] = 2;
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
    if (hdr[0] != 2 || hdr[1] != 3) {{ close(fd); return -1; }}

    uint8_t body[2048];
    if (plen > sizeof(body)) {{ close(fd); return -1; }}
    r = read(fd, body, plen);
    close(fd);
    if (r != (ssize_t)plen) return -1;

    size_t bp = 0;
    return wire_read_str(body, plen, &bp, out, out_len);
}}

int os_shim_execve(const char *path, char *const argv[], char *const envp[]) {{
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
    fn emits_strong_os_shim_execve() {
        let src = emit_os_shim_execve("integrated", "SPEET_RTD_SOCK");
        assert!(src.contains("int os_shim_execve"));
        assert!(!src.contains("__attribute__((weak))"));
        assert!(src.contains("\"integrated\""));
    }
}
