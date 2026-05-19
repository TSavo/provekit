// SPDX-License-Identifier: Apache-2.0
//
// Load each language kit's PlatformSemanticsDeclaration via JSON-RPC.
//
// Per #1270: the kit IS the authority on its platform semantics. libprovekit
// MUST NOT carry a hardcoded Rust mirror of any kit's declaration. This
// loader spawns the kit binary as a JSON-RPC subprocess (PEP 1.7.0 plugin
// protocol), calls `provekit.plugin.platform_semantics`, and parses the
// response into a `PlatformSemanticsDeclaration`.
//
// The loader is the substrate-uniform replacement for the hardcoded
// `*_kit_declaration()` calls that previously lived in
// `libprovekit/src/core/platform_semantics/{typescript,java,python_*}.rs`
// and the in-tree includes of provekit-realize-rust-core +
// provekit-realize-c-core platform_semantics.rs files.

use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::OnceLock;

use serde_json::{json, Value};

use crate::core::types::PlatformSemanticsDeclaration;

#[derive(Debug, thiserror::Error)]
pub enum PlatformSemanticsLoadError {
    #[error("kit binary {binary} not found on PATH")]
    MissingBinary { binary: String },
    #[error("kit RPC failed: {0}")]
    Failed(String),
    #[error("kit RPC response missing 'result' field: {0}")]
    MissingResult(String),
    #[error("kit RPC result did not parse as PlatformSemanticsDeclaration: {0}")]
    ResultShape(String),
}

/// The canonical command for each lower-target kit. Hardcoded by-target rather
/// than discovered because (a) this dispatcher must work in tests that have
/// not registered a plugin registry, and (b) the kit binaries are themselves
/// pinned by the workspace + plugin protocol; their command names are stable
/// per CCP. Plugin-loader-based resolution remains available for callers that
/// need it; for the platform-semantics specifically the canonical pinned name
/// is enough.
fn kit_command_for(target: &str) -> Option<Vec<String>> {
    match target {
        "rust" => Some(vec!["provekit-realize-rust-core".to_string()]),
        "c" => Some(vec!["provekit-realize-c-core".to_string()]),
        "java" => Some(vec!["provekit-realize-java-core".to_string()]),
        "python" => Some(vec!["provekit-realize-python-core".to_string()]),
        "typescript" => Some(vec!["provekit-realize-typescript-core".to_string()]),
        // Binding-kit variants (per binding_semantics_for_tag dispatcher).
        "aiosqlite" => Some(vec!["provekit-realize-python-aiosqlite".to_string()]),
        "better-sqlite3" => Some(vec!["provekit-realize-typescript-better-sqlite3".to_string()]),
        "pg" => Some(vec!["provekit-realize-typescript-pg".to_string()]),
        "sqlite3" => Some(vec!["provekit-realize-python-sqlite3".to_string()]),
        _ => None,
    }
}

/// Load a kit's PlatformSemanticsDeclaration over JSON-RPC.
///
/// Spawns the kit binary, performs the PEP 1.7.0 initialize handshake, calls
/// `provekit.plugin.platform_semantics`, parses the result, and shuts the
/// subprocess down.
pub fn load_platform_semantics(
    target: &str,
) -> Result<PlatformSemanticsDeclaration, PlatformSemanticsLoadError> {
    let Some(command) = kit_command_for(target) else {
        return Err(PlatformSemanticsLoadError::Failed(format!(
            "no kit command registered for target {target}"
        )));
    };
    load_platform_semantics_for_command(&command, None)
}

/// Same as `load_platform_semantics`, but caches the result per-target for
/// the lifetime of the process. Callers that load the same target many times
/// per run (the dispatcher does) should prefer this.
pub fn load_platform_semantics_cached(
    target: &str,
) -> Result<PlatformSemanticsDeclaration, PlatformSemanticsLoadError> {
    use std::collections::HashMap;
    use std::sync::Mutex;
    static CACHE: OnceLock<Mutex<HashMap<String, PlatformSemanticsDeclaration>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Ok(map) = cache.lock() {
        if let Some(decl) = map.get(target) {
            return Ok(decl.clone());
        }
    }
    let decl = load_platform_semantics(target)?;
    if let Ok(mut map) = cache.lock() {
        map.insert(target.to_string(), decl.clone());
    }
    Ok(decl)
}

fn load_platform_semantics_for_command(
    command: &[String],
    working_dir: Option<&PathBuf>,
) -> Result<PlatformSemanticsDeclaration, PlatformSemanticsLoadError> {
    if command.is_empty() {
        return Err(PlatformSemanticsLoadError::Failed(
            "platform-semantics kit command is empty".into(),
        ));
    }

    let mut cmd = Command::new(&command[0]);
    if command.len() > 1 {
        cmd.args(&command[1..]);
    }
    if !command.iter().any(|arg| arg == "--rpc") {
        cmd.arg("--rpc");
    }
    if let Some(working_dir) = working_dir {
        cmd.current_dir(working_dir);
    }
    cmd.stdin(Stdio::piped());
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::inherit());

    let mut child = match cmd.spawn() {
        Ok(child) => child,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Err(PlatformSemanticsLoadError::MissingBinary {
                binary: command[0].clone(),
            });
        }
        Err(error) => {
            return Err(PlatformSemanticsLoadError::Failed(format!(
                "spawn {:?}: {error}",
                command
            )));
        }
    };

    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| PlatformSemanticsLoadError::Failed("stdin unavailable".into()))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| PlatformSemanticsLoadError::Failed("stdout unavailable".into()))?;
    let mut reader = BufReader::new(stdout);

    let init_req = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "client": {"name": "libprovekit-platform-semantics-loader", "version": env!("CARGO_PKG_VERSION")},
            "protocol_version": "pep/1.7.0"
        }
    });
    writeln!(stdin, "{init_req}").map_err(|e| {
        PlatformSemanticsLoadError::Failed(format!("write initialize: {e}"))
    })?;
    let _ = read_response(&mut reader, 1)?;

    let req = json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "provekit.plugin.platform_semantics",
        "params": {}
    });
    writeln!(stdin, "{req}").map_err(|e| {
        PlatformSemanticsLoadError::Failed(format!("write platform_semantics request: {e}"))
    })?;
    let response = read_response(&mut reader, 2)?;

    let shutdown_req = json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "shutdown"
    });
    let _ = writeln!(stdin, "{shutdown_req}");
    drop(stdin);
    let _ = child.wait();

    let result = response
        .get("result")
        .cloned()
        .ok_or_else(|| PlatformSemanticsLoadError::MissingResult(response.to_string()))?;
    serde_json::from_value::<PlatformSemanticsDeclaration>(result)
        .map_err(|e| PlatformSemanticsLoadError::ResultShape(e.to_string()))
}

fn read_response<R: BufRead>(
    reader: &mut R,
    expected_id: i64,
) -> Result<Value, PlatformSemanticsLoadError> {
    let mut line = String::new();
    reader
        .read_line(&mut line)
        .map_err(|e| PlatformSemanticsLoadError::Failed(format!("read response: {e}")))?;
    if line.trim().is_empty() {
        return Err(PlatformSemanticsLoadError::Failed(
            "empty response line".into(),
        ));
    }
    let value: Value = serde_json::from_str(line.trim())
        .map_err(|e| PlatformSemanticsLoadError::Failed(format!("parse response: {e}")))?;
    let id = value.get("id").and_then(Value::as_i64).unwrap_or(-1);
    if id != expected_id {
        return Err(PlatformSemanticsLoadError::Failed(format!(
            "response id mismatch: expected {expected_id}, got {id}"
        )));
    }
    if let Some(error) = value.get("error") {
        return Err(PlatformSemanticsLoadError::Failed(format!(
            "kit returned error: {error}"
        )));
    }
    Ok(value)
}
