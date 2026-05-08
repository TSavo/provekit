// SPDX-License-Identifier: Apache-2.0
//
// Live subprocess transport for the Claude Code CLI. Gated behind the
// `live` feature so CI never tries to spawn `claude` or hit network.
//
// This module is intentionally a sketch: the real wire format used by
// the `claude` CLI's `--json` mode is project-internal to Anthropic
// and may change. The skeleton documents the seam so we can fill it
// in without breaking callers.

use std::process::{Command, Stdio};

use provekit_agent::AgentError;

use crate::Transport;

pub struct SubprocessTransport {
    pub binary: String,
    pub model: Option<String>,
    pub api_key_env: String,
}

impl Default for SubprocessTransport {
    fn default() -> Self {
        Self {
            binary: "claude".into(),
            model: None,
            api_key_env: "ANTHROPIC_API_KEY".into(),
        }
    }
}

impl Transport for SubprocessTransport {
    fn invoke(
        &self,
        method: &str,
        params: &serde_json::Value,
    ) -> Result<serde_json::Value, AgentError> {
        // Sketch: spawn `claude --json` with a structured prompt
        // describing the JSON-RPC method + params. Read the response
        // from stdout; on non-zero exit, surface stderr.
        let request = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": method,
            "params": params,
        });
        let request_str =
            serde_json::to_string(&request).map_err(|e| AgentError::Backend(e.to_string()))?;

        let mut cmd = Command::new(&self.binary);
        cmd.arg("--json")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if let Some(m) = &self.model {
            cmd.arg("--model").arg(m);
        }
        let mut child = cmd
            .spawn()
            .map_err(|e| AgentError::Io(format!("spawn claude: {e}")))?;
        if let Some(stdin) = child.stdin.as_mut() {
            use std::io::Write;
            stdin
                .write_all(request_str.as_bytes())
                .map_err(|e| AgentError::Io(format!("write stdin: {e}")))?;
        }
        let output = child
            .wait_with_output()
            .map_err(|e| AgentError::Io(format!("wait: {e}")))?;
        if !output.status.success() {
            let err = String::from_utf8_lossy(&output.stderr).to_string();
            return Err(AgentError::Backend(format!(
                "claude exited {}: {}",
                output.status, err
            )));
        }
        let stdout = String::from_utf8(output.stdout)
            .map_err(|e| AgentError::Backend(format!("non-utf8 stdout: {e}")))?;
        serde_json::from_str(&stdout)
            .map_err(|e| AgentError::InvalidIr(format!("decode stdout: {e}")))
    }
}
