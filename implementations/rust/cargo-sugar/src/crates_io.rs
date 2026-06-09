//! Resolve and download a crate's last published version from crates.io.
//!
//! Network plumbing is shelled to `curl` and `tar` (already required on any CI
//! runner) to avoid pulling an HTTP/TLS stack into the workspace. The URL
//! construction and JSON parsing are pure functions, unit-tested without network.

use std::path::Path;
use std::process::Command;

const USER_AGENT: &str = "cargo-sugar (behavioral semver; github.com/TSavo/sugar)";

fn api_url(name: &str) -> String {
    format!("https://crates.io/api/v1/crates/{name}")
}

fn download_url(name: &str, version: &str) -> String {
    format!("https://crates.io/api/v1/crates/{name}/{version}/download")
}

/// The latest publishable version from the crates.io crate metadata JSON:
/// prefer `crate.max_stable_version`, fall back to `crate.newest_version`.
fn parse_latest(json: &str) -> Option<String> {
    let v: serde_json::Value = serde_json::from_str(json).ok()?;
    let krate = v.get("crate")?;
    krate
        .get("max_stable_version")
        .and_then(|s| s.as_str())
        .or_else(|| krate.get("newest_version").and_then(|s| s.as_str()))
        .map(str::to_string)
}

fn curl(args: &[&str]) -> Result<Vec<u8>, String> {
    let out = Command::new("curl")
        .args(["-sSL", "-A", USER_AGENT])
        .args(args)
        .output()
        .map_err(|e| format!("curl: {e} (curl is required)"))?;
    if !out.status.success() {
        return Err(format!(
            "curl failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    Ok(out.stdout)
}

pub fn latest_version(name: &str) -> Result<String, String> {
    let body = curl(&[&api_url(name)])?;
    let text = String::from_utf8_lossy(&body);
    parse_latest(&text).ok_or_else(|| format!("crates.io: no published version found for `{name}`"))
}

/// Download `name`@`version` and extract its crate root directly into `dst`
/// (strips the `<name>-<version>/` top-level dir so `dst/Cargo.toml` exists).
pub fn download_and_extract(name: &str, version: &str, dst: &Path) -> Result<(), String> {
    let bytes = curl(&[&download_url(name, version)])?;
    // crates.io serves a gzip-compressed tar (`.crate`). Pipe it to tar.
    use std::io::Write;
    let mut tar = Command::new("tar")
        .args(["-xz", "--strip-components=1", "-C", &dst.to_string_lossy()])
        .stdin(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("tar: {e}"))?;
    tar.stdin
        .take()
        .expect("tar stdin")
        .write_all(&bytes)
        .map_err(|e| format!("tar stdin: {e}"))?;
    if !tar.wait().map_err(|e| format!("tar wait: {e}"))?.success() {
        return Err(format!("tar failed to extract {name}@{version}"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_api_and_download_urls() {
        assert_eq!(api_url("serde"), "https://crates.io/api/v1/crates/serde");
        assert_eq!(
            download_url("serde", "1.0.203"),
            "https://crates.io/api/v1/crates/serde/1.0.203/download"
        );
    }

    #[test]
    fn parses_max_stable_version() {
        let json = r#"{"crate":{"id":"serde","max_stable_version":"1.0.203","newest_version":"1.0.204-beta"}}"#;
        assert_eq!(parse_latest(json).as_deref(), Some("1.0.203"));
    }

    #[test]
    fn falls_back_to_newest_when_no_stable() {
        let json = r#"{"crate":{"id":"x","newest_version":"0.1.0-alpha"}}"#;
        assert_eq!(parse_latest(json).as_deref(), Some("0.1.0-alpha"));
    }

    #[test]
    fn rejects_garbage() {
        assert_eq!(parse_latest("not json"), None);
        assert_eq!(parse_latest("{}"), None);
    }
}
