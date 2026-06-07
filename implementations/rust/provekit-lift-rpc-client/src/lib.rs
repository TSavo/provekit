// SPDX-License-Identifier: Apache-2.0
//
// provekit-lift-rpc-client
//
// THE SEVER (shared transport): the rust `#[requires]`/`#[ensures]`
// contract lifter (`provekit-lift-contracts`) is a real RPC kit
// (`contracts_rpc` bin). The substrate's IN-PROCESS callers
// (`provekit-lift`, `provekit-build`, `provekit-lsp-rust`) USED to
// statically link `provekit_lift_contracts::lift_file`. They now reach
// the lifter THROUGH this crate, which:
//
//   1. RESOLVES the `contracts_rpc` binary path (one resolver, shared by
//      all callers so the determinism-critical resolution can never drift
//      between them).
//   2. SPAWNS it and drives the NDJSON `initialize` / `lift` / `shutdown`
//      protocol (the same protocol the CLI's `cmd_mint` drives).
//   3. Returns the RAW `ir-document` JSON `Value`.
//
// It is a TRUE LEAF: serde_json + std only. It does NOT depend on
// `provekit-ir-symbolic`, so callers that want typed `ContractDecl`s parse
// the returned `ir` array themselves (via `parse_document`), and callers
// that only need the JSON (lsp-rust) forward it directly. It does NOT
// depend on `libprovekit`, so the leaf build/lsp crates stay free of the
// proof/crypto stack their own comments warn against.

use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};

use serde_json::{json, Value};

/// Environment override for the `contracts_rpc` binary path. When set,
/// it wins over the `current_exe`-relative search. Mirrors the
/// absolute-path injection the witness manifests already use (run.sh
/// substitutes the built binary's path).
pub const CONTRACTS_RPC_ENV: &str = "PROVEKIT_CONTRACTS_RPC";

/// The `contracts_rpc` binary name (cargo target name).
const CONTRACTS_RPC_BIN: &str = "contracts_rpc";

#[derive(Debug)]
pub enum RpcClientError {
    BinNotFound(String),
    Spawn(String),
    Io(String),
    Protocol(String),
}

impl std::fmt::Display for RpcClientError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BinNotFound(m) => write!(f, "contracts_rpc binary not found: {m}"),
            Self::Spawn(m) => write!(f, "spawn contracts_rpc: {m}"),
            Self::Io(m) => write!(f, "contracts_rpc io: {m}"),
            Self::Protocol(m) => write!(f, "contracts_rpc protocol: {m}"),
        }
    }
}

impl std::error::Error for RpcClientError {}

/// Resolve the path to the `contracts_rpc` binary.
///
/// Resolution order:
///   1. `PROVEKIT_CONTRACTS_RPC` env var (explicit absolute path override).
///   2. A sibling of the current executable (`<exe_dir>/contracts_rpc`).
///   3. The parent of the current exe dir when that dir is `deps/` — under
///      `cargo test`, integration/unit test binaries run from
///      `target/<profile>/deps/`, but `contracts_rpc` is built into
///      `target/<profile>/`. So we also try `<exe_dir>/../contracts_rpc`.
///
/// All three are deterministic and require no PATH lookup, so the resolved
/// binary is always the one built into the same target dir as the caller.
pub fn resolve_bin() -> Result<PathBuf, RpcClientError> {
    if let Some(p) = std::env::var_os(CONTRACTS_RPC_ENV) {
        let p = PathBuf::from(p);
        if p.exists() {
            return Ok(p);
        }
        return Err(RpcClientError::BinNotFound(format!(
            "{CONTRACTS_RPC_ENV}={} does not exist",
            p.display()
        )));
    }

    let exe = std::env::current_exe()
        .map_err(|e| RpcClientError::BinNotFound(format!("current_exe: {e}")))?;
    let exe_dir = exe
        .parent()
        .ok_or_else(|| RpcClientError::BinNotFound("current_exe has no parent".into()))?;

    // 2: sibling of current exe.
    let sibling = exe_dir.join(CONTRACTS_RPC_BIN);
    if sibling.exists() {
        return Ok(sibling);
    }

    // 3: climb out of `deps/` (cargo-test layout).
    if exe_dir.file_name().and_then(|n| n.to_str()) == Some("deps") {
        if let Some(up) = exe_dir.parent() {
            let candidate = up.join(CONTRACTS_RPC_BIN);
            if candidate.exists() {
                return Ok(candidate);
            }
        }
    }

    Err(RpcClientError::BinNotFound(format!(
        "no `{CONTRACTS_RPC_BIN}` next to {} (set {CONTRACTS_RPC_ENV} to override)",
        exe_dir.display()
    )))
}

/// Drive the `contracts_rpc` lift kit over NDJSON and return the raw
/// `ir-document` JSON `Value` (the `result` of the `lift` call).
///
/// `workspace_root` is the directory the kit walks/reads relative to.
/// `source_paths` are relative paths within it; pass an empty slice to
/// have the kit walk the whole workspace (mirrors the pre-sever
/// `lift_path` whole-workspace walk).
///
/// The returned `Value` is the lift result envelope:
///   { "kind": "ir-document", "ir": [ <contract>, ... ], "diagnostics": [...], ... }
pub fn invoke_lift(
    workspace_root: &std::path::Path,
    source_paths: &[String],
) -> Result<Value, RpcClientError> {
    let bin = resolve_bin()?;
    invoke_lift_with_bin(&bin, workspace_root, source_paths)
}

/// Same as [`invoke_lift`] but with an explicit binary path (used by tests
/// and when the caller has already resolved the bin once).
pub fn invoke_lift_with_bin(
    bin: &std::path::Path,
    workspace_root: &std::path::Path,
    source_paths: &[String],
) -> Result<Value, RpcClientError> {
    let mut child = Command::new(bin)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .map_err(|e| RpcClientError::Spawn(format!("{}: {e}", bin.display())))?;

    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| RpcClientError::Io("stdin unavailable".into()))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| RpcClientError::Io("stdout unavailable".into()))?;
    let mut reader = BufReader::new(stdout);

    let workspace_root_str = workspace_root.display().to_string();

    // 1: initialize
    let init = json!({
        "jsonrpc": "2.0", "id": 1, "method": "initialize",
        "params": { "workspace_root": workspace_root_str }
    });
    write_line(&mut stdin, &init)?;
    let _ = read_response(&mut reader, 1)?;

    // 2: lift
    let lift = json!({
        "jsonrpc": "2.0", "id": 2, "method": "lift",
        "params": {
            "workspace_root": workspace_root_str,
            "source_paths": source_paths,
        }
    });
    write_line(&mut stdin, &lift)?;
    let lift_resp = read_response(&mut reader, 2)?;

    // 3: shutdown
    let shutdown = json!({"jsonrpc": "2.0", "id": 3, "method": "shutdown"});
    let _ = write_line(&mut stdin, &shutdown);
    drop(stdin);

    let status = child
        .wait()
        .map_err(|e| RpcClientError::Io(format!("wait: {e}")))?;
    if !status.success() {
        return Err(RpcClientError::Protocol(format!(
            "contracts_rpc exited {status}"
        )));
    }

    lift_resp
        .get("result")
        .cloned()
        .ok_or_else(|| RpcClientError::Protocol("lift response missing `result`".into()))
}

fn write_line(stdin: &mut std::process::ChildStdin, v: &Value) -> Result<(), RpcClientError> {
    let s = serde_json::to_string(v).map_err(|e| RpcClientError::Io(e.to_string()))?;
    stdin
        .write_all(s.as_bytes())
        .and_then(|()| stdin.write_all(b"\n"))
        .and_then(|()| stdin.flush())
        .map_err(|e| RpcClientError::Io(e.to_string()))
}

/// Read NDJSON response lines until one matches `expect_id`. Skips blank
/// lines. Errors if the stream ends first.
fn read_response<R: BufRead>(reader: &mut R, expect_id: i64) -> Result<Value, RpcClientError> {
    loop {
        let mut line = String::new();
        let n = reader
            .read_line(&mut line)
            .map_err(|e| RpcClientError::Io(e.to_string()))?;
        if n == 0 {
            return Err(RpcClientError::Protocol(format!(
                "stream ended before response id={expect_id}"
            )));
        }
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let v: Value = serde_json::from_str(trimmed)
            .map_err(|e| RpcClientError::Protocol(format!("bad json line: {e}")))?;
        if v.get("id").and_then(Value::as_i64) == Some(expect_id) {
            if let Some(err) = v.get("error") {
                return Err(RpcClientError::Protocol(format!("rpc error: {err}")));
            }
            return Ok(v);
        }
        // Different id (or notification): keep reading.
    }
}
