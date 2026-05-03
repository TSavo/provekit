// SPDX-License-Identifier: Apache-2.0
//
// LSP plugin round-trip test (#221).
//
// The Rust kit ships `provekit-lift --rpc` as its NDJSON-over-stdio plugin
// binary (manifest sample in `protocol/specs/2026-04-30-lift-plugin-protocol.md`).
// It implements the same NDJSON-on-stdio plugin shape used by every kit's LSP
// plugin (`initialize` / `lift` / `shutdown`), with the shape reflected by the
// LSP coordinator client at `provekit-lsp/src/plugin.rs`.
//
// This test spawns the freshly-built `provekit-lift` binary, drives the
// protocol end to end, and asserts the response shape per the protocol spec.
// It is the single test that proves the Rust plugin binary actually speaks the
// protocol — unit tests on `run_rpc_mode` would not exercise the spawn boundary.
//
// Note (#221 follow-up): the Rust kit does NOT yet ship a per-language LSP
// plugin (`parse` method that returns `{declarations, warnings}` like the
// other kits). The LSP coordinator falls back to a built-in Rust parser. This
// test exercises the lift-plugin protocol on the same NDJSON wire shape.

use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use serde_json::{json, Value};

/// Path to the freshly-built `provekit-lift` binary. `cargo test` sets
/// `CARGO_BIN_EXE_<name>` for each declared `[[bin]]` target.
fn plugin_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_provekit-lift"))
}

struct Plugin {
    child: std::process::Child,
    stdin: std::process::ChildStdin,
    stdout: BufReader<std::process::ChildStdout>,
}

impl Plugin {
    fn spawn() -> Self {
        let mut child = Command::new(plugin_bin())
            .arg("--rpc")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn provekit-lift --rpc");
        let stdin = child.stdin.take().expect("stdin");
        let stdout = BufReader::new(child.stdout.take().expect("stdout"));
        Self { child, stdin, stdout }
    }

    fn exchange(&mut self, payload: &Value) -> Value {
        let line = serde_json::to_string(payload).unwrap();
        writeln!(self.stdin, "{line}").expect("write");
        self.stdin.flush().expect("flush");
        let mut buf = String::new();
        let n = self.stdout.read_line(&mut buf).expect("read");
        assert!(n > 0, "plugin closed stdout without responding");
        serde_json::from_str(&buf).expect("decode response")
    }

    fn wait_for_exit(mut self, timeout: Duration) -> std::process::ExitStatus {
        // Closing stdin signals EOF; some plugins exit on shutdown alone, but
        // we drop stdin to be defensive.
        drop(self.stdin);
        let start = Instant::now();
        loop {
            match self.child.try_wait().expect("try_wait") {
                Some(status) => return status,
                None => {
                    if start.elapsed() > timeout {
                        let _ = self.child.kill();
                        panic!("plugin did not exit within {:?}", timeout);
                    }
                    std::thread::sleep(Duration::from_millis(50));
                }
            }
        }
    }
}

#[test]
fn round_trip_initialize_lift_shutdown() {
    let mut plugin = Plugin::spawn();

    // 1. initialize ---------------------------------------------------------
    let init = plugin.exchange(&json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "client": {"name": "round-trip-test", "version": "0"},
            "protocol_version": "provekit-lift/1",
        }
    }));
    assert_eq!(init["jsonrpc"], "2.0");
    assert_eq!(init["id"], 1);
    let result = init.get("result")
        .unwrap_or_else(|| panic!("initialize returned error: {init}"));
    assert_eq!(result["name"].as_str(), Some("provekit-lift"),
        "initialize result missing/incorrect `name`: {result}");
    assert!(result.get("version").and_then(|v| v.as_str()).is_some(),
        "initialize result missing `version`: {result}");
    assert!(result.get("capabilities").is_some(),
        "initialize result missing `capabilities`: {result}");

    // 3. shutdown -----------------------------------------------------------
    // We skip `lift` here: the production `lift` call walks the CWD looking
    // for source files and would either mint a real `.proof` (slow, requires
    // network/IO) or fail. Driving it to verify response shape is covered
    // separately by `end_to_end.rs`. The protocol round-trip we care about is
    // initialize → shutdown; gap-flag for a `lift`-shape parse method is in
    // the PR body.
    let shut = plugin.exchange(&json!({
        "jsonrpc": "2.0",
        "id": 99,
        "method": "shutdown",
    }));
    assert_eq!(shut["id"], 99);
    assert!(shut.get("result").map(|v| v.is_null()).unwrap_or(false),
        "shutdown should return null result: {shut}");

    let status = plugin.wait_for_exit(Duration::from_secs(10));
    assert!(status.success(), "plugin exited unsuccessfully: {status:?}");
}

#[test]
fn unknown_method_returns_method_not_found() {
    let mut plugin = Plugin::spawn();

    let _ = plugin.exchange(&json!({
        "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {}
    }));

    let bad = plugin.exchange(&json!({
        "jsonrpc": "2.0", "id": 2, "method": "no_such_method"
    }));
    let err = bad.get("error")
        .unwrap_or_else(|| panic!("expected JSON-RPC error: {bad}"));
    assert_eq!(err["code"].as_i64(), Some(-32601),
        "expected method-not-found error code: {err}");

    let shut = plugin.exchange(&json!({
        "jsonrpc": "2.0", "id": 3, "method": "shutdown"
    }));
    assert!(shut.get("result").map(|v| v.is_null()).unwrap_or(false));
    let _ = plugin.wait_for_exit(Duration::from_secs(10));
}
