// SPDX-License-Identifier: Apache-2.0
//
// JSON-RPC over stdio subprocess client. Wraps a binary that speaks
// the protocol defined in protocol/specs/2026-04-30-ir-compiler-protocol.md
// behind the same IrCompiler trait used for in-process Rust impls.
//
// Framing: line-delimited JSON. One request per stdin line, one
// response per stdout line. stderr is the plugin's logging channel
// and is intentionally not consumed here.

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::Mutex;

use serde_json::{json, Value as Json};

use crate::{Capabilities, CompileError, CompiledFormula, IrCompiler, PROTOCOL_VERSION};

/// JSON-RPC subprocess wrapper. The child is spawned on construction,
/// the handshake is performed once, capabilities are cached. Subsequent
/// `compile` calls reuse the long-lived process.
pub struct JsonRpcCompiler {
    binary: PathBuf,
    cached_caps: Capabilities,
    inner: Mutex<ChildIo>,
    next_id: Mutex<u64>,
}

struct ChildIo {
    _child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
}

impl JsonRpcCompiler {
    /// Spawn the subprocess and perform the handshake. Returns an
    /// error if the binary cannot be launched or rejects the protocol
    /// version.
    pub fn spawn(binary: impl AsRef<Path>) -> Result<Self, CompileError> {
        let binary = binary.as_ref().to_path_buf();
        let mut child = Command::new(&binary)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .map_err(|e| CompileError::Transport(format!("spawn {}: {e}", binary.display())))?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| CompileError::Transport("child stdin missing".into()))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| CompileError::Transport("child stdout missing".into()))?;

        let mut io = ChildIo {
            _child: child,
            stdin,
            stdout: BufReader::new(stdout),
        };

        let caps = handshake(&mut io)?;

        Ok(Self {
            binary,
            cached_caps: caps,
            inner: Mutex::new(io),
            next_id: Mutex::new(2),
        })
    }

    /// Path to the binary backing this compiler.
    pub fn binary_path(&self) -> &Path {
        &self.binary
    }
}

impl IrCompiler for JsonRpcCompiler {
    fn compile(&self, ir: &Json, dialect: &str) -> Result<CompiledFormula, CompileError> {
        let id = {
            let mut g = self.next_id.lock().unwrap();
            let v = *g;
            *g += 1;
            v
        };
        let req = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "sugar.ir.compile",
            "params": {
                "ir_json": ir,
                "target_dialect": dialect,
            }
        });
        let mut io = self.inner.lock().unwrap();
        let resp = exchange(&mut io, &req)?;
        if let Some(err) = resp.get("error") {
            return Err(rpc_error_to_compile_error(err));
        }
        let result = resp
            .get("result")
            .ok_or_else(|| CompileError::Transport("no result in compile response".into()))?;
        serde_json::from_value::<CompiledFormula>(result.clone())
            .map_err(|e| CompileError::Transport(format!("compile result decode: {e}")))
    }

    fn capabilities(&self) -> Capabilities {
        self.cached_caps.clone()
    }
}

fn handshake(io: &mut ChildIo) -> Result<Capabilities, CompileError> {
    let req = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "sugar.ir.handshake",
        "params": {
            "sugar_version": env!("CARGO_PKG_VERSION"),
            "protocol_version": PROTOCOL_VERSION,
        }
    });
    let resp = exchange(io, &req)?;
    if let Some(err) = resp.get("error") {
        return Err(rpc_error_to_compile_error(err));
    }
    let result = resp
        .get("result")
        .ok_or_else(|| CompileError::Transport("no result in handshake".into()))?;
    let caps: Capabilities = serde_json::from_value(result.clone())
        .map_err(|e| CompileError::Transport(format!("handshake decode: {e}")))?;
    if caps.protocol_version != PROTOCOL_VERSION {
        return Err(CompileError::Transport(format!(
            "protocol version mismatch: plugin reports {}, expected {}",
            caps.protocol_version, PROTOCOL_VERSION
        )));
    }
    Ok(caps)
}

fn exchange(io: &mut ChildIo, req: &Json) -> Result<Json, CompileError> {
    let line =
        serde_json::to_string(req).map_err(|e| CompileError::Transport(format!("encode: {e}")))?;
    writeln!(io.stdin, "{line}").map_err(|e| CompileError::Transport(format!("write: {e}")))?;
    io.stdin
        .flush()
        .map_err(|e| CompileError::Transport(format!("flush: {e}")))?;

    let mut buf = String::new();
    let n = io
        .stdout
        .read_line(&mut buf)
        .map_err(|e| CompileError::Transport(format!("read: {e}")))?;
    if n == 0 {
        return Err(CompileError::Transport("plugin closed stdout".into()));
    }
    serde_json::from_str(&buf).map_err(|e| CompileError::Transport(format!("decode: {e}")))
}

fn rpc_error_to_compile_error(err: &Json) -> CompileError {
    let code = err.get("code").and_then(|v| v.as_i64()).unwrap_or(0);
    let msg = err
        .get("message")
        .and_then(|v| v.as_str())
        .unwrap_or("(no message)")
        .to_string();
    let data = err
        .get("data")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    match code {
        2000 => CompileError::UnsupportedDialect(data.unwrap_or(msg)),
        2001 => CompileError::UnsupportedSort(data.unwrap_or(msg)),
        2002 => CompileError::UnsupportedPredicate(data.unwrap_or(msg)),
        2003 => CompileError::MalformedIr(data.unwrap_or(msg)),
        2004 => CompileError::Internal(data.unwrap_or(msg)),
        _ => CompileError::Transport(format!("rpc error {code}: {msg}")),
    }
}
