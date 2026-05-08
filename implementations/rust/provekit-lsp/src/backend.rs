// JSON-RPC backend communication for the ProvekIt LSP server.
//
// Spawns a configurable backend binary (default: "provekit") and speaks
// NDJSON-over-stdio. All verification is delegated; this module is just
// the transport layer.

use serde::Deserialize;
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

/// A handle to a spawned JSON-RPC backend process.
#[derive(Debug)]
pub struct JsonRpcBackend {
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    _child: Child,
    next_id: u64,
}

/// Result of a verify call.
#[derive(Debug, Clone, Deserialize)]
pub struct VerifyResult {
    pub status: String,
    #[serde(default)]
    pub function: String,
    #[serde(default)]
    pub target_cid: String,
    #[serde(default)]
    pub transfers: Vec<Transfer>,
    #[serde(default)]
    pub error: Option<String>,
    #[serde(default)]
    pub counterexample: Option<Value>,
    #[serde(default)]
    pub suggestion: Option<String>,
    #[serde(default)]
    pub evidence: Option<Value>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Transfer {
    pub domain: String,
    pub cid: String,
}

impl JsonRpcBackend {
    /// Spawn the backend binary and perform the handshake.
    pub async fn spawn(binary: impl AsRef<Path>, args: &[String]) -> Result<Self, String> {
        let binary = binary.as_ref();
        let mut cmd = Command::new(binary);
        cmd.args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit());

        let mut child = cmd
            .spawn()
            .map_err(|e| format!("spawn {}: {}", binary.display(), e))?;

        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| "child stdin missing".to_string())?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| "child stdout missing".to_string())?;

        let mut backend = JsonRpcBackend {
            stdin,
            stdout: BufReader::new(stdout),
            _child: child,
            next_id: 1,
        };

        backend.handshake().await?;
        Ok(backend)
    }

    /// Verify a function against a target contract CID.
    pub async fn verify(
        &mut self,
        function: &str,
        target_cid: &str,
    ) -> Result<VerifyResult, String> {
        let req = json!({
            "jsonrpc": "2.0",
            "id": self.next_id(),
            "method": "provekit.lsp.verify",
            "params": {
                "function": function,
                "target_cid": target_cid,
            }
        });

        let resp = self.exchange(&req).await?;

        if let Some(err) = resp.get("error") {
            let msg = err
                .get("message")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown error");
            return Err(format!("backend error: {}", msg));
        }

        let result = resp.get("result").ok_or("no result in response")?.clone();

        serde_json::from_value(result).map_err(|e| format!("decode verify result: {}", e))
    }

    // ------------------------------------------------------------------
    // internals
    // ------------------------------------------------------------------

    async fn handshake(&mut self) -> Result<(), String> {
        let req = json!({
            "jsonrpc": "2.0",
            "id": self.next_id(),
            "method": "provekit.lsp.handshake",
            "params": {
                "provekit_version": env!("CARGO_PKG_VERSION"),
                "protocol_version": "lsp-1.0"
            }
        });

        let resp = self.exchange(&req).await?;

        if resp.get("error").is_some() {
            return Err("handshake rejected".to_string());
        }

        Ok(())
    }

    async fn exchange(&mut self, req: &Value) -> Result<Value, String> {
        let line = serde_json::to_string(req).map_err(|e| format!("encode: {}", e))?;

        writeln!(self.stdin, "{}", line).map_err(|e| format!("write: {}", e))?;
        self.stdin.flush().map_err(|e| format!("flush: {}", e))?;

        let mut buf = String::new();
        let n = self
            .stdout
            .read_line(&mut buf)
            .map_err(|e| format!("read: {}", e))?;

        if n == 0 {
            return Err("backend closed stdout".to_string());
        }

        serde_json::from_str(&buf).map_err(|e| format!("decode: {}", e))
    }

    fn next_id(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }
}
