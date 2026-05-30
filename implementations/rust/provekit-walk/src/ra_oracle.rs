//! Tier 2b native semantic oracle (spec 2026-05-30-callee-resolution-tiers, §2.T2b).
//!
//! The syntactic tiers (T1 path/import resolution, T2a local type-flow) resolve
//! a method call's receiver-defining crate only when it is locally determinable.
//! When they refuse (`callee_crate == None` on a method call), this oracle asks
//! the language's own semantic analyzer, rust-analyzer, "what does this method
//! resolve to, and in which crate is the receiver type defined" via the LSP
//! `textDocument/definition` request, then maps the resolved definition's file
//! path to a defining crate.
//!
//! Soundness (§1.R2, supra omnia rectum): the oracle returns a crate ONLY when
//! the answer is definitive: a single definition location whose path maps to a
//! known crate (sysroot core/alloc/std, collapsed to "std"; or a cargo-registry
//! crate). Ambiguity (more than one distinct crate), an unmappable path (a
//! workspace-local file, a path outside the known roots), a null result, or an
//! unavailable analyzer all yield `None` (refuse). The oracle never guesses a
//! bridge; an unresolved call stays a lift-gap.
//!
//! Availability: the oracle is opt-in behind the `PROVEKIT_RESOLVE_ORACLE`
//! environment variable (`= "rust-analyzer"`). When unset, or when the
//! rust-analyzer binary cannot be located/spawned, `RaOracle::start` returns
//! `None` and every resolution refuses, so the fast path and CI are unaffected
//! by a missing analyzer. This degradation is deterministic: oracle-off and
//! oracle-on-but-absent both reduce to the same Tier-2a behavior.

use std::collections::HashMap;
use std::io::{BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::time::{Duration, Instant};

use serde_json::{json, Value};

/// A position to resolve: 0-based LSP line and 0-based character of the method
/// identifier, in the given absolute file path.
#[derive(Debug, Clone)]
pub struct ResolveQuery {
    pub abs_path: PathBuf,
    pub lsp_line: u32,
    pub lsp_col: u32,
}

/// The oracle handle: a live rust-analyzer LSP subprocess plus the bookkeeping
/// to send requests and correlate responses. Dropped (and the child killed) at
/// the end of one `lift_implications` run.
pub struct RaOracle {
    child: Child,
    stdin: ChildStdin,
    reader: BufReader<ChildStdout>,
    root: PathBuf,
    next_id: i64,
    opened: std::collections::HashSet<PathBuf>,
}

/// How long to wait for rust-analyzer to finish loading the workspace before
/// issuing resolution queries. The cold load runs cargo/rustc over the project,
/// which can take minutes on a large workspace; this is generous on purpose.
const INDEX_WAIT: Duration = Duration::from_secs(240);
/// How long to wait for a single `textDocument/definition` response.
const DEFINITION_WAIT: Duration = Duration::from_secs(30);

impl RaOracle {
    /// Start the oracle for `workspace_root`, or return `None` to refuse.
    ///
    /// Returns `None` (and the caller falls back to Tier-2a refusal) when:
    ///   - `PROVEKIT_RESOLVE_ORACLE` is not exactly `"rust-analyzer"`; or
    ///   - the rust-analyzer binary cannot be located or spawned.
    pub fn start(workspace_root: &Path) -> Option<RaOracle> {
        let switch = std::env::var("PROVEKIT_RESOLVE_ORACLE").unwrap_or_default();
        if switch != "rust-analyzer" {
            return None;
        }
        let bin = locate_rust_analyzer()?;
        let mut child = Command::new(&bin)
            .current_dir(workspace_root)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .ok()?;
        let stdin = child.stdin.take()?;
        let stdout = child.stdout.take()?;
        let reader = BufReader::new(stdout);
        let mut oracle = RaOracle {
            child,
            stdin,
            reader,
            root: workspace_root.to_path_buf(),
            next_id: 0,
            opened: std::collections::HashSet::new(),
        };
        if oracle.initialize().is_none() {
            let _ = oracle.child.kill();
            return None;
        }
        Some(oracle)
    }

    fn initialize(&mut self) -> Option<()> {
        let root_uri = path_to_uri(&self.root);
        let id = self.send_request(
            "initialize",
            json!({
                "processId": std::process::id(),
                "rootUri": root_uri,
                "capabilities": {
                    "textDocument": { "definition": { "linkSupport": true } },
                    "window": { "workDoneProgress": true }
                },
                "workspaceFolders": [ { "uri": root_uri, "name": "provekit-target" } ],
            }),
        )?;
        // Drain until the initialize response arrives.
        self.wait_for_response(id, INDEX_WAIT)?;
        self.send_notification("initialized", json!({}));
        // Let rust-analyzer load + index the workspace before we query. We wait
        // for an indexing-complete progress signal, or for quiescence.
        self.wait_until_indexed(INDEX_WAIT);
        Some(())
    }

    fn ensure_open(&mut self, abs_path: &Path) -> Option<()> {
        if self.opened.contains(abs_path) {
            return Some(());
        }
        let text = std::fs::read_to_string(abs_path).ok()?;
        let uri = path_to_uri(abs_path);
        self.send_notification(
            "textDocument/didOpen",
            json!({
                "textDocument": {
                    "uri": uri,
                    "languageId": "rust",
                    "version": 1,
                    "text": text,
                }
            }),
        );
        self.opened.insert(abs_path.to_path_buf());
        Some(())
    }

    /// Resolve one query to a definitive crate, or `None` (refuse).
    pub fn resolve_crate(&mut self, q: &ResolveQuery) -> Option<String> {
        self.ensure_open(&q.abs_path)?;
        let uri = path_to_uri(&q.abs_path);
        let id = self.send_request(
            "textDocument/definition",
            json!({
                "textDocument": { "uri": uri },
                "position": { "line": q.lsp_line, "character": q.lsp_col },
            }),
        )?;
        let resp = self.wait_for_response(id, DEFINITION_WAIT)?;
        let result = resp.get("result")?;
        crate_from_definition_result(result)
    }

    fn send_request(&mut self, method: &str, params: Value) -> Option<i64> {
        self.next_id += 1;
        let id = self.next_id;
        let msg = json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params });
        self.write_message(&msg)?;
        Some(id)
    }

    fn send_notification(&mut self, method: &str, params: Value) {
        let msg = json!({ "jsonrpc": "2.0", "method": method, "params": params });
        let _ = self.write_message(&msg);
    }

    fn write_message(&mut self, msg: &Value) -> Option<()> {
        let body = serde_json::to_vec(msg).ok()?;
        write!(self.stdin, "Content-Length: {}\r\n\r\n", body.len()).ok()?;
        self.stdin.write_all(&body).ok()?;
        self.stdin.flush().ok()?;
        Some(())
    }

    /// Read one LSP message (framed by Content-Length). Returns `None` on EOF or
    /// a malformed frame.
    fn read_message(&mut self) -> Option<Value> {
        // Read headers up to the blank line.
        let mut header = Vec::new();
        let mut byte = [0u8; 1];
        loop {
            if self.reader.read(&mut byte).ok()? == 0 {
                return None;
            }
            header.push(byte[0]);
            if header.ends_with(b"\r\n\r\n") {
                break;
            }
            if header.len() > 1 << 16 {
                return None;
            }
        }
        let header_str = String::from_utf8_lossy(&header);
        let mut len = 0usize;
        for line in header_str.split("\r\n") {
            if let Some(rest) = line
                .to_ascii_lowercase()
                .strip_prefix("content-length:")
            {
                len = rest.trim().parse().ok()?;
            }
        }
        if len == 0 {
            return None;
        }
        let mut body = vec![0u8; len];
        self.reader.read_exact(&mut body).ok()?;
        serde_json::from_slice(&body).ok()
    }

    /// Pump messages until one with `id == want` carrying `result`/`error`, or
    /// until the deadline. Server-to-client requests/notifications are ignored.
    fn wait_for_response(&mut self, want: i64, timeout: Duration) -> Option<Value> {
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            let msg = self.read_message()?;
            if msg.get("id").and_then(|v| v.as_i64()) == Some(want)
                && (msg.get("result").is_some() || msg.get("error").is_some())
            {
                return Some(msg);
            }
        }
        None
    }

    /// Pump progress notifications until rust-analyzer signals that indexing /
    /// cache priming has ended, or until the deadline. Best-effort: returns once
    /// an `end` progress for an indexing/caching token is seen.
    fn wait_until_indexed(&mut self, timeout: Duration) {
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            let Some(msg) = self.read_message() else {
                return;
            };
            if msg.get("method").and_then(|v| v.as_str()) == Some("$/progress") {
                let value = msg.get("params").and_then(|p| p.get("value"));
                let kind = value
                    .and_then(|v| v.get("kind"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let token = msg
                    .get("params")
                    .and_then(|p| p.get("token"))
                    .map(|t| t.to_string())
                    .unwrap_or_default()
                    .to_ascii_lowercase();
                let title = value
                    .and_then(|v| v.get("title"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_ascii_lowercase();
                if kind == "end"
                    && (token.contains("index")
                        || token.contains("cache")
                        || token.contains("roots")
                        || title.contains("index")
                        || title.contains("roots scanned")
                        || title.contains("loading"))
                {
                    return;
                }
            }
        }
    }
}

impl Drop for RaOracle {
    fn drop(&mut self) {
        // Best-effort graceful shutdown, then kill.
        let _ = self.send_request("shutdown", json!(null));
        self.send_notification("exit", json!(null));
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Locate the rust-analyzer binary: prefer the explicit override, then PATH,
/// then `rustup which`. Returns `None` when none is runnable.
fn locate_rust_analyzer() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("PROVEKIT_RUST_ANALYZER") {
        if !p.is_empty() {
            return Some(PathBuf::from(p));
        }
    }
    // `rustup which rust-analyzer` gives the toolchain-resolved path even when
    // the bare `rust-analyzer` proxy is not on PATH.
    if let Ok(out) = Command::new("rustup")
        .args(["which", "rust-analyzer"])
        .output()
    {
        if out.status.success() {
            let path = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if !path.is_empty() && Path::new(&path).exists() {
                return Some(PathBuf::from(path));
            }
        }
    }
    // Fall back to the bare name and let spawn resolve it via PATH.
    if Command::new("rust-analyzer")
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
    {
        return Some(PathBuf::from("rust-analyzer"));
    }
    None
}

/// Map a `textDocument/definition` result to a single defining crate, or
/// `None` (refuse). Accepts `Location`, `Location[]`, or `LocationLink[]`.
/// Refuses on: empty result, any location whose path is unmappable, or more
/// than one distinct normalized crate across the locations.
fn crate_from_definition_result(result: &Value) -> Option<String> {
    let locations: Vec<&Value> = match result {
        Value::Array(a) => a.iter().collect(),
        Value::Object(_) => vec![result],
        _ => return None,
    };
    if locations.is_empty() {
        return None;
    }
    let mut crates = std::collections::BTreeSet::new();
    for loc in locations {
        let uri = loc
            .get("uri")
            .or_else(|| loc.get("targetUri"))
            .and_then(|v| v.as_str())?;
        // Every location must map to a known crate; an unmappable one is a
        // refuse, not a "skip and trust the rest". Soundness over coverage.
        let krate = crate_from_uri(uri)?;
        crates.insert(normalize_crate(&krate));
    }
    if crates.len() == 1 {
        crates.into_iter().next()
    } else {
        // Ambiguous dispatch across distinct crates: refuse.
        None
    }
}

/// Map a definition-target file URI to its defining crate name, or `None` when
/// the path is outside the known roots (sysroot / cargo registry). A
/// workspace-local path is intentionally `None`: Tier 2a already resolves the
/// locally-determinable receivers, and Tier 2b must not invent a workspace-crate
/// key it cannot be sure of.
pub fn crate_from_uri(uri: &str) -> Option<String> {
    let path = uri.strip_prefix("file://").unwrap_or(uri);
    // sysroot rust-src: .../library/{core,alloc,std,...}/...
    if let Some(name) = segment_after(path, "/library/") {
        return Some(name);
    }
    // cargo registry: .../registry/src/<host>/<crate>-<version>/...
    if let Some(idx) = path.find("/registry/src/") {
        let rest = &path[idx + "/registry/src/".len()..];
        // skip the <host> segment
        let mut parts = rest.splitn(2, '/');
        let _host = parts.next();
        if let Some(after_host) = parts.next() {
            if let Some(pkg) = after_host.split('/').next() {
                // pkg is `<crate>-<version>`; strip the trailing `-<semver>`.
                if let Some(name) = strip_version_suffix(pkg) {
                    return Some(name);
                }
            }
        }
    }
    // git checkouts and path/workspace deps are not resolved here: refuse.
    None
}

/// Return the first path segment immediately after `marker`.
fn segment_after(path: &str, marker: &str) -> Option<String> {
    let idx = path.find(marker)?;
    let rest = &path[idx + marker.len()..];
    rest.split('/').next().map(|s| s.to_string())
}

/// `serde_json-1.0.117` -> `serde_json`; `base64-0.22.1` -> `base64`. Strips the
/// last `-<digit...>` component that begins a semver.
fn strip_version_suffix(pkg: &str) -> Option<String> {
    let idx = pkg.rfind('-')?;
    let (name, ver) = pkg.split_at(idx);
    // ver starts with '-'; the next char must be a digit for this to be a
    // version suffix.
    if ver.len() >= 2 && ver.as_bytes()[1].is_ascii_digit() && !name.is_empty() {
        Some(name.to_string())
    } else {
        Some(pkg.to_string())
    }
}

/// Collapse the standard-library facade crates to the single `std` label the
/// rust-std shim publishes its contracts under. `String::to_string` resolves to
/// `alloc`, `Option::unwrap` to `core`; both are re-exported through `std`, so
/// the collapse is sound. Any other crate name passes through unchanged.
pub fn normalize_crate(krate: &str) -> String {
    match krate {
        "core" | "alloc" | "std" | "proc_macro" => "std".to_string(),
        other => other.replace('-', "_"),
    }
}

/// Build the file:// URI for an absolute path (LSP wants forward-slash URIs).
fn path_to_uri(path: &Path) -> String {
    let s = path.to_string_lossy();
    format!("file://{}", s)
}

/// Resolve a batch of queries grouped by file, reusing one warm oracle session.
/// Returns a map from (file, lsp_line, lsp_col) to the resolved crate. Queries
/// that refuse are simply absent from the map.
pub fn resolve_batch(
    workspace_root: &Path,
    queries: &[ResolveQuery],
) -> HashMap<(PathBuf, u32, u32), String> {
    let mut out = HashMap::new();
    if queries.is_empty() {
        return out;
    }
    let Some(mut oracle) = RaOracle::start(workspace_root) else {
        return out;
    };
    for q in queries {
        if let Some(krate) = oracle.resolve_crate(q) {
            out.insert((q.abs_path.clone(), q.lsp_line, q.lsp_col), krate);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn crate_from_uri_maps_sysroot_library_to_crate() {
        let u = "file:///Users/x/.rustup/toolchains/stable/lib/rustlib/src/rust/library/alloc/src/string.rs";
        assert_eq!(crate_from_uri(u).as_deref(), Some("alloc"));
        let u2 = "file:///opt/rust/library/core/src/option.rs";
        assert_eq!(crate_from_uri(u2).as_deref(), Some("core"));
    }

    #[test]
    fn crate_from_uri_maps_registry_crate() {
        let u = "file:///home/u/.cargo/registry/src/index.crates.io-6f17d22bba15001f/serde_json-1.0.117/src/lib.rs";
        assert_eq!(crate_from_uri(u).as_deref(), Some("serde_json"));
        let u2 = "file:///home/u/.cargo/registry/src/github.com-1ecc6299db9ec823/base64-0.22.1/src/lib.rs";
        assert_eq!(crate_from_uri(u2).as_deref(), Some("base64"));
    }

    #[test]
    fn crate_from_uri_refuses_workspace_local() {
        let u = "file:///Users/x/provekit/implementations/rust/provekit-cli/src/main.rs";
        assert_eq!(crate_from_uri(u), None);
    }

    #[test]
    fn normalize_collapses_std_facade() {
        assert_eq!(normalize_crate("core"), "std");
        assert_eq!(normalize_crate("alloc"), "std");
        assert_eq!(normalize_crate("std"), "std");
        assert_eq!(normalize_crate("serde_json"), "serde_json");
        assert_eq!(normalize_crate("my-dep"), "my_dep");
    }

    #[test]
    fn definition_result_refuses_on_empty() {
        assert_eq!(crate_from_definition_result(&json!([])), None);
        assert_eq!(crate_from_definition_result(&json!(null)), None);
    }

    #[test]
    fn definition_result_resolves_single_location() {
        let r = json!([{
            "uri": "file:///opt/rust/library/alloc/src/string.rs",
            "range": {}
        }]);
        assert_eq!(crate_from_definition_result(&r).as_deref(), Some("std"));
    }

    #[test]
    fn definition_result_refuses_ambiguous_crates() {
        let r = json!([
            { "uri": "file:///x/registry/src/h/foo-1.0.0/src/lib.rs", "range": {} },
            { "uri": "file:///x/registry/src/h/bar-2.0.0/src/lib.rs", "range": {} }
        ]);
        assert_eq!(crate_from_definition_result(&r), None);
    }

    #[test]
    fn definition_result_refuses_when_one_location_unmappable() {
        // One known crate, one workspace-local: must refuse, not trust the known.
        let r = json!([
            { "uri": "file:///opt/rust/library/core/src/option.rs", "range": {} },
            { "uri": "file:///ws/provekit/src/main.rs", "range": {} }
        ]);
        assert_eq!(crate_from_definition_result(&r), None);
    }

    #[test]
    fn locationlink_targeturi_is_read() {
        let r = json!([{
            "targetUri": "file:///opt/rust/library/alloc/src/vec/mod.rs",
            "targetRange": {},
            "targetSelectionRange": {}
        }]);
        assert_eq!(crate_from_definition_result(&r).as_deref(), Some("std"));
    }
}
