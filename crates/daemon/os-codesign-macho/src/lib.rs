//! macOS code-signing for the simple rewriter, implementing the two methods
//! from `hardened-runtime-library-validation-schema.md`:
//!
//! - **Real identity**: sign the shim with a real Developer-ID/enterprise
//!   identity, extract its `TeamIdentifier`/`Identifier`, and embed a
//!   library-constraint plist matching those two fields when signing the
//!   rewritten executable.
//! - **Ad-hoc fallback** (local development only): sign both artifacts ad
//!   hoc, extract the shim's SHA-256 code-directory hash instead, embed a
//!   cdhash-based constraint, and add
//!   `com.apple.security.cs.disable-library-validation` *only* on the
//!   rewritten executable, never the shim.
//!
//! This crate shells out to `/usr/bin/codesign` via `std::process::Command`
//! rather than depending on a pure-Rust code-signing crate. The ecosystem's
//! options in that space are large, `sigstore`-adjacent projects aimed at
//! a different problem (signing/verifying distributed artifacts) — shelling
//! out matches this codebase's minimal-dependency style and is what the
//! reference `sandboxd` implementation itself does.

use std::path::Path;
use std::process::Command;

/// Which of the two schema-doc methods to use.
#[derive(Debug, Clone)]
pub enum SigningIdentity {
    /// A real Developer-ID/Apple-development/enterprise identity string
    /// passed to `codesign --sign`.
    Real(String),
    /// `SANDBOX_CODESIGN_IDENTITY=-` — ad hoc, local development only.
    AdHoc,
}

impl SigningIdentity {
    fn codesign_arg(&self) -> &str {
        match self {
            SigningIdentity::Real(s) => s,
            SigningIdentity::AdHoc => "-",
        }
    }

    fn is_adhoc(&self) -> bool {
        matches!(self, SigningIdentity::AdHoc)
    }
}

/// Entitlements carried forward from the *original* (pre-rewrite) binary,
/// per the schema doc: rewriting must not silently grant or drop these.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct EntitlementPolicy {
    pub allow_jit: bool,
    pub allow_unsigned_executable_memory: bool,
    /// The *original* binary's `get-task-allow` entitlement. Whether it is
    /// actually re-granted also depends on an explicit daemon-level allow
    /// flag — see `sign_rewritten_executable`'s `allow_get_task_allow` arg.
    pub get_task_allow: bool,
}

#[derive(Debug)]
pub enum SignError {
    CodesignNotFound,
    CommandFailed(String),
    ParseFailed(String),
}

impl core::fmt::Display for SignError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            SignError::CodesignNotFound => write!(f, "codesign not found"),
            SignError::CommandFailed(e) => write!(f, "codesign command failed: {e}"),
            SignError::ParseFailed(e) => write!(f, "failed to parse codesign output: {e}"),
        }
    }
}

impl std::error::Error for SignError {}

/// Signing identity extracted from the shim dylib, used to build the
/// rewritten executable's library constraint.
#[derive(Debug, Clone)]
pub struct ShimSigningInfo {
    /// `None` under an ad-hoc identity ("not set").
    pub team_identifier: Option<String>,
    pub identifier: String,
    /// The shim's SHA-256 code-directory hash, as the 40-hex-character
    /// `CandidateCDHash sha256=` value `codesign -dv` reports (macOS
    /// truncates this candidate hash to 20 bytes regardless of the
    /// `sha256` label). Required unconditionally, matching the reference
    /// implementation, which validates it even on the real-identity path.
    pub cdhash_sha256: String,
}

fn run_codesign(args: &[&str]) -> Result<std::process::Output, SignError> {
    Command::new("codesign")
        .args(args)
        .output()
        .map_err(|_| SignError::CodesignNotFound)
}

/// `codesign -dv --verbose=4 path 2>&1` merges stdout and stderr, since
/// `codesign -dv` writes its report to stderr.
fn combined_output(out: &std::process::Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

/// Whether `path` currently carries a valid, strict-verifying signature.
pub fn verify_signed(path: &Path) -> bool {
    Command::new("codesign")
        .arg("--verify")
        .arg("--strict")
        .arg(path)
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Reads the *original* (pre-rewrite) binary's carried-forward entitlements.
/// Returns an error (matching the reference implementation) if the
/// original isn't validly signed at all; callers should default to
/// [`EntitlementPolicy::default`] on error, exactly like the reference
/// daemon's `orig, _ := readOriginalEntitlements(src)`.
pub fn read_original_entitlements(original_path: &Path) -> Result<EntitlementPolicy, SignError> {
    if !verify_signed(original_path) {
        return Err(SignError::CommandFailed(format!(
            "{} does not have a valid signature",
            original_path.display()
        )));
    }
    let out = run_codesign(&[
        "-d",
        "--entitlements",
        ":-",
        original_path.to_str().ok_or_else(|| {
            SignError::ParseFailed("non-UTF8 path".to_string())
        })?,
    ])?;
    if !out.status.success() {
        return Err(SignError::CommandFailed(combined_output(&out)));
    }
    let plist = combined_output(&out);
    Ok(EntitlementPolicy {
        allow_jit: plist_bool(&plist, "com.apple.security.cs.allow-jit"),
        allow_unsigned_executable_memory: plist_bool(
            &plist,
            "com.apple.security.cs.allow-unsigned-executable-memory",
        ),
        get_task_allow: plist_bool(&plist, "com.apple.security.get-task-allow"),
    })
}

/// Extracts `TeamIdentifier`/`Identifier`/cdhash from an already-signed
/// artifact's `codesign -dv --verbose=4` report.
pub fn signing_info(path: &Path) -> Result<ShimSigningInfo, SignError> {
    let out = run_codesign(&[
        "-dv",
        "--verbose=4",
        path.to_str()
            .ok_or_else(|| SignError::ParseFailed("non-UTF8 path".to_string()))?,
    ])?;
    if !out.status.success() {
        return Err(SignError::CommandFailed(format!(
            "inspect signature: {}",
            combined_output(&out)
        )));
    }
    let report = combined_output(&out);

    let mut team = None;
    let mut identifier = None;
    let mut cdhash = None;
    for line in report.lines() {
        if let Some(v) = line.strip_prefix("TeamIdentifier=") {
            team = Some(v.trim().to_string());
        } else if let Some(v) = line.strip_prefix("Identifier=") {
            identifier = Some(v.trim().to_string());
        } else if let Some(v) = line.strip_prefix("CandidateCDHash sha256=") {
            cdhash = Some(v.trim().to_string());
        }
    }

    let identifier = identifier
        .filter(|s| !s.is_empty())
        .ok_or_else(|| SignError::ParseFailed("no signing identifier".to_string()))?;
    let cdhash = cdhash.ok_or_else(|| {
        SignError::ParseFailed("no usable SHA-256 code-directory hash".to_string())
    })?;
    if cdhash.len() != 40 || !cdhash.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(SignError::ParseFailed(
            "no usable SHA-256 code-directory hash".to_string(),
        ));
    }

    Ok(ShimSigningInfo {
        team_identifier: team.filter(|t| t != "not set"),
        identifier,
        cdhash_sha256: cdhash,
    })
}

/// Signs the shim dylib with `identity` (plain `codesign --sign`, no
/// entitlements/constraints — only the rewritten executable carries
/// those), then returns its [`ShimSigningInfo`].
pub fn sign_shim(
    shim_path: &Path,
    identity: &SigningIdentity,
    keychain: Option<&str>,
) -> Result<ShimSigningInfo, SignError> {
    let path_str = shim_path
        .to_str()
        .ok_or_else(|| SignError::ParseFailed("non-UTF8 path".to_string()))?;
    let mut args: Vec<&str> = vec![
        "--force",
        "--sign",
        identity.codesign_arg(),
        "--timestamp=none",
        "--options",
        "runtime",
    ];
    if let Some(kc) = keychain {
        args.push("--keychain");
        args.push(kc);
    }
    args.push(path_str);
    let out = run_codesign(&args)?;
    if !out.status.success() {
        return Err(SignError::CommandFailed(combined_output(&out)));
    }
    signing_info(shim_path)
}

/// Signs the rewritten executable per the schema doc's two methods,
/// embedding a library constraint that matches `shim_info` and carrying
/// forward `carried_entitlements` (with `get-task-allow` additionally
/// gated on `allow_get_task_allow`, matching the daemon's own
/// `--allow-get-task-allow` flag). `work_dir` is where the transient
/// `entitlements.plist`/`library-constraint.plist` are written — callers
/// should use a private (`0700`) staging directory, per the schema doc.
pub fn sign_rewritten_executable(
    exe_path: &Path,
    shim_info: &ShimSigningInfo,
    identity: &SigningIdentity,
    carried_entitlements: &EntitlementPolicy,
    allow_get_task_allow: bool,
    work_dir: &Path,
    keychain: Option<&str>,
) -> Result<(), SignError> {
    if identity.is_adhoc() && shim_info.cdhash_sha256.is_empty() {
        return Err(SignError::ParseFailed(
            "shim has no SHA-256 code-directory hash".to_string(),
        ));
    }
    if !identity.is_adhoc() && shim_info.team_identifier.is_none() {
        return Err(SignError::ParseFailed(
            "shim has no TeamIdentifier; sign it with a real identity or use SigningIdentity::AdHoc"
                .to_string(),
        ));
    }

    let task_allow = carried_entitlements.get_task_allow && allow_get_task_allow;
    let mut entitlements = format!(
        concat!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?><plist version=\"1.0\"><dict>",
            "<key>com.apple.security.cs.allow-jit</key><{}/>",
            "<key>com.apple.security.cs.allow-unsigned-executable-memory</key><{}/>",
            "<key>com.apple.security.get-task-allow</key><{}/>",
        ),
        bool_tag(carried_entitlements.allow_jit),
        bool_tag(carried_entitlements.allow_unsigned_executable_memory),
        bool_tag(task_allow),
    );

    let constraint = if identity.is_adhoc() {
        let decoded = hex_decode(&shim_info.cdhash_sha256)
            .ok_or_else(|| SignError::ParseFailed("invalid shim cdhash".to_string()))?;
        if decoded.len() != 20 {
            return Err(SignError::ParseFailed("invalid shim cdhash length".to_string()));
        }
        entitlements.push_str("<key>com.apple.security.cs.disable-library-validation</key><true/>");
        format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?><plist version=\"1.0\"><dict><key>cdhash</key><data>{}</data></dict></plist>",
            base64_encode(&decoded)
        )
    } else {
        let team = shim_info.team_identifier.as_deref().unwrap_or("");
        format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?><plist version=\"1.0\"><dict><key>team-identifier</key><string>{}</string><key>signing-identifier</key><string>{}</string></dict></plist>",
            xml_escape(team),
            xml_escape(&shim_info.identifier)
        )
    };
    entitlements.push_str("</dict></plist>");

    let entitlements_path = work_dir.join("entitlements.plist");
    let constraint_path = work_dir.join("library-constraint.plist");
    std::fs::write(&entitlements_path, entitlements)
        .map_err(|e| SignError::CommandFailed(e.to_string()))?;
    std::fs::write(&constraint_path, constraint)
        .map_err(|e| SignError::CommandFailed(e.to_string()))?;

    let exe_str = exe_path
        .to_str()
        .ok_or_else(|| SignError::ParseFailed("non-UTF8 path".to_string()))?;
    let entitlements_str = entitlements_path
        .to_str()
        .ok_or_else(|| SignError::ParseFailed("non-UTF8 path".to_string()))?;
    let constraint_str = constraint_path
        .to_str()
        .ok_or_else(|| SignError::ParseFailed("non-UTF8 path".to_string()))?;

    let mut args: Vec<&str> = vec![
        "--force",
        "--sign",
        identity.codesign_arg(),
        "--timestamp=none",
        "--options",
        "runtime",
        "--entitlements",
        entitlements_str,
        "--library-constraint",
        constraint_str,
    ];
    if let Some(kc) = keychain {
        args.push("--keychain");
        args.push(kc);
    }
    args.push(exe_str);

    let out = run_codesign(&args)?;
    if !out.status.success() {
        return Err(SignError::CommandFailed(format!(
            "hardened codesign with library constraint: {}",
            combined_output(&out)
        )));
    }
    Ok(())
}

/// Scans a `codesign -d --entitlements :-` plist for
/// `<key>{key}</key><true/>`, tolerating whitespace between the tags.
/// Hand-rolled rather than pulling in a general XML parser: only three
/// fixed, known keys are ever looked up, on output this process itself
/// produced by running `codesign`.
fn plist_bool(xml: &str, key: &str) -> bool {
    let marker = format!("<key>{key}</key>");
    let Some(idx) = xml.find(&marker) else {
        return false;
    };
    xml[idx + marker.len()..].trim_start().starts_with("<true/>")
}

fn bool_tag(v: bool) -> &'static str {
    if v {
        "true"
    } else {
        "false"
    }
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

fn hex_decode(s: &str) -> Option<Vec<u8>> {
    if s.len() % 2 != 0 {
        return None;
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).ok())
        .collect()
}

const BASE64_ALPHABET: &[u8; 64] =
    b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

fn base64_encode(data: &[u8]) -> String {
    let mut out = String::with_capacity((data.len() + 2) / 3 * 4);
    for chunk in data.chunks(3) {
        let b0 = chunk[0];
        let b1 = *chunk.get(1).unwrap_or(&0);
        let b2 = *chunk.get(2).unwrap_or(&0);
        let n = ((b0 as u32) << 16) | ((b1 as u32) << 8) | b2 as u32;
        out.push(BASE64_ALPHABET[((n >> 18) & 0x3f) as usize] as char);
        out.push(BASE64_ALPHABET[((n >> 12) & 0x3f) as usize] as char);
        out.push(if chunk.len() > 1 {
            BASE64_ALPHABET[((n >> 6) & 0x3f) as usize] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            BASE64_ALPHABET[(n & 0x3f) as usize] as char
        } else {
            '='
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plist_bool_finds_true_and_false() {
        let xml = "<dict><key>com.apple.security.cs.allow-jit</key><true/><key>com.apple.security.get-task-allow</key><false/></dict>";
        assert!(plist_bool(xml, "com.apple.security.cs.allow-jit"));
        assert!(!plist_bool(xml, "com.apple.security.get-task-allow"));
        assert!(!plist_bool(xml, "com.apple.security.cs.missing"));
    }

    #[test]
    fn base64_roundtrip_matches_known_vectors() {
        assert_eq!(base64_encode(b""), "");
        assert_eq!(base64_encode(b"f"), "Zg==");
        assert_eq!(base64_encode(b"fo"), "Zm8=");
        assert_eq!(base64_encode(b"foo"), "Zm9v");
        assert_eq!(base64_encode(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn hex_decode_round_trips() {
        let bytes = hex_decode("deadbeef").unwrap();
        assert_eq!(bytes, vec![0xde, 0xad, 0xbe, 0xef]);
        assert!(hex_decode("xyz").is_none());
        assert!(hex_decode("abc").is_none()); // odd length
    }

    #[test]
    fn xml_escape_handles_reserved_chars() {
        assert_eq!(xml_escape("A&B<C>\"D'"), "A&amp;B&lt;C&gt;&quot;D&apos;");
    }

    #[test]
    fn sign_rewritten_executable_rejects_adhoc_without_cdhash() {
        let shim_info = ShimSigningInfo {
            team_identifier: None,
            identifier: "com.example.shim".to_string(),
            cdhash_sha256: String::new(),
        };
        let dir = tempfile::tempdir().unwrap();
        let result = sign_rewritten_executable(
            Path::new("/nonexistent"),
            &shim_info,
            &SigningIdentity::AdHoc,
            &EntitlementPolicy::default(),
            false,
            dir.path(),
            None,
        );
        assert!(matches!(result, Err(SignError::ParseFailed(_))));
    }

    #[test]
    fn sign_rewritten_executable_rejects_real_identity_without_team() {
        let shim_info = ShimSigningInfo {
            team_identifier: None,
            identifier: "com.example.shim".to_string(),
            cdhash_sha256: "0".repeat(40),
        };
        let dir = tempfile::tempdir().unwrap();
        let result = sign_rewritten_executable(
            Path::new("/nonexistent"),
            &shim_info,
            &SigningIdentity::Real("Developer ID Application: Example".to_string()),
            &EntitlementPolicy::default(),
            false,
            dir.path(),
            None,
        );
        assert!(matches!(result, Err(SignError::ParseFailed(_))));
    }
}
