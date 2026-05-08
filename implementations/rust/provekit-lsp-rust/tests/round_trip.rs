// SPDX-License-Identifier: Apache-2.0
//
// Integration test: spawn `provekit-lsp-rust`, drive it via NDJSON,
// assert protocol contract per spec.

use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use serde_json::{json, Value};

fn plugin_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_provekit-lsp-rust"))
}

struct Plugin {
    child: std::process::Child,
    stdin: std::process::ChildStdin,
    stdout: BufReader<std::process::ChildStdout>,
}

impl Plugin {
    fn spawn() -> Self {
        let mut child = Command::new(plugin_bin())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn provekit-lsp-rust");
        let stdin = child.stdin.take().expect("stdin");
        let stdout = BufReader::new(child.stdout.take().expect("stdout"));
        Self {
            child,
            stdin,
            stdout,
        }
    }

    fn exchange(&mut self, payload: &Value) -> Value {
        let line = serde_json::to_string(payload).unwrap();
        writeln!(self.stdin, "{line}").expect("write to plugin stdin");
        self.stdin.flush().expect("flush");
        let mut buf = String::new();
        let n = self
            .stdout
            .read_line(&mut buf)
            .expect("read from plugin stdout");
        assert!(n > 0, "plugin closed stdout without responding");
        serde_json::from_str(buf.trim()).expect("decode plugin response as JSON")
    }

    fn wait_for_exit(mut self, timeout: Duration) -> std::process::ExitStatus {
        drop(self.stdin);
        let start = Instant::now();
        loop {
            match self.child.try_wait().expect("try_wait") {
                Some(status) => return status,
                None => {
                    if start.elapsed() > timeout {
                        let _ = self.child.kill();
                        panic!("plugin did not exit within {timeout:?}");
                    }
                    std::thread::sleep(Duration::from_millis(50));
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// 1. initialize returns a capabilities envelope
// ---------------------------------------------------------------------------

#[test]
fn initialize_returns_capabilities() {
    let mut plugin = Plugin::spawn();

    let resp = plugin.exchange(&json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {}
    }));

    assert_eq!(resp["jsonrpc"], "2.0");
    assert_eq!(resp["id"], 1);

    let result = resp
        .get("result")
        .unwrap_or_else(|| panic!("initialize returned error: {resp}"));

    assert_eq!(
        result["name"].as_str(),
        Some("provekit-lsp-rust"),
        "wrong plugin name: {result}"
    );
    assert!(
        result.get("version").and_then(|v| v.as_str()).is_some(),
        "missing version: {result}"
    );
    let caps = result["capabilities"]
        .as_array()
        .unwrap_or_else(|| panic!("missing capabilities array: {result}"));
    assert!(
        caps.iter().any(|c| c.as_str() == Some("parse")),
        "capabilities must include 'parse': {caps:?}"
    );

    // Tidy shutdown.
    let _ = plugin.exchange(&json!({"jsonrpc":"2.0","id":99,"method":"shutdown"}));
    let _ = plugin.wait_for_exit(Duration::from_secs(10));
}

// ---------------------------------------------------------------------------
// 2. parse over a fixture emits at least one contract declaration
// ---------------------------------------------------------------------------

#[test]
fn parse_fixture_emits_contracts() {
    let source = r#"
#[test]
fn value_is_non_negative() {
    let x: i64 = 42;
    assert!(x >= 0);
}
"#;

    let mut plugin = Plugin::spawn();

    let _ = plugin.exchange(&json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {}
    }));

    let resp = plugin.exchange(&json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "parse",
        "params": {
            "path": "simple.rs",
            "source": source
        }
    }));

    assert_eq!(resp["jsonrpc"], "2.0");
    assert_eq!(resp["id"], 2);

    let result = resp
        .get("result")
        .unwrap_or_else(|| panic!("parse returned error: {resp}"));

    let decls = result["declarations"]
        .as_array()
        .unwrap_or_else(|| panic!("declarations must be an array: {result}"));

    assert!(
        result
            .get("diagnostics")
            .and_then(|d| d.as_array())
            .is_some(),
        "parse result must expose a diagnostics array for LSP forward propagation: {result}"
    );

    assert!(
        !decls.is_empty(),
        "expected at least one contract declaration from fixture; got empty array.\nfull result: {result}"
    );

    // Each declaration must have a `name` field.
    for d in decls {
        assert!(
            d.get("name").and_then(|n| n.as_str()).is_some(),
            "declaration missing 'name': {d}"
        );
    }

    // Tidy shutdown.
    let _ = plugin.exchange(&json!({"jsonrpc":"2.0","id":99,"method":"shutdown"}));
    let _ = plugin.wait_for_exit(Duration::from_secs(10));
}

// ---------------------------------------------------------------------------
// 3. Unknown method returns JSON-RPC -32601
// ---------------------------------------------------------------------------

#[test]
fn unknown_method_returns_method_not_found() {
    let mut plugin = Plugin::spawn();

    let _ = plugin.exchange(&json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {}
    }));

    let resp = plugin.exchange(&json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "no_such_method"
    }));

    let err = resp
        .get("error")
        .unwrap_or_else(|| panic!("expected JSON-RPC error, got: {resp}"));
    assert_eq!(
        err["code"].as_i64(),
        Some(-32601),
        "expected method-not-found error code (-32601): {err}"
    );

    // Tidy shutdown.
    let _ = plugin.exchange(&json!({"jsonrpc":"2.0","id":99,"method":"shutdown"}));
    let _ = plugin.wait_for_exit(Duration::from_secs(10));
}

// ---------------------------------------------------------------------------
// 4. shutdown exits cleanly (exit code 0)
// ---------------------------------------------------------------------------

#[test]
fn shutdown_exits_cleanly() {
    let mut plugin = Plugin::spawn();

    let _ = plugin.exchange(&json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {}
    }));

    let resp = plugin.exchange(&json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "shutdown"
    }));

    assert_eq!(resp["id"], 2);
    assert!(
        resp.get("result").map(|v| v.is_null()).unwrap_or(false),
        "shutdown should return null result, got: {resp}"
    );

    let status = plugin.wait_for_exit(Duration::from_secs(10));
    assert!(
        status.success(),
        "plugin exited with non-zero status: {status:?}"
    );
}
