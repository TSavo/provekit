// SPDX-License-Identifier: Apache-2.0

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
        let n = self.stdout.read_line(&mut buf).expect("read stdout");
        assert!(n > 0, "plugin closed stdout without responding");
        serde_json::from_str(buf.trim()).expect("decode plugin response")
    }

    fn shutdown(mut self) {
        let _ = self.exchange(&json!({"jsonrpc":"2.0","id":99,"method":"shutdown"}));
        let start = Instant::now();
        loop {
            match self.child.try_wait().expect("try_wait") {
                Some(_) => return,
                None if start.elapsed() > Duration::from_secs(10) => {
                    let _ = self.child.kill();
                    panic!("plugin did not exit after shutdown");
                }
                None => std::thread::sleep(Duration::from_millis(50)),
            }
        }
    }
}

#[test]
fn initialize_then_analyze_document_returns_shared_rust_analysis() {
    let mut plugin = Plugin::spawn();

    let init = plugin.exchange(&json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocol_version": "provekit-lsp-shared/1",
            "workspace_root": "/tmp/provekit-lsp-rust-test"
        }
    }));

    let init_result = init
        .get("result")
        .unwrap_or_else(|| panic!("initialize returned error: {init}"));
    assert_eq!(
        init_result["protocol_version"].as_str(),
        Some("provekit-lsp-shared/1")
    );
    assert_eq!(init_result["kit_id"].as_str(), Some("rust"));
    assert!(
        init_result["capabilities"]["status_kinds"]
            .as_array()
            .unwrap_or_else(|| panic!("missing status_kinds: {init_result}"))
            .iter()
            .any(|kind| kind.as_str() == Some("prove")),
        "initialize should advertise prove status support: {init_result}"
    );

    let source = r#"
#[provekit::sugar(
    concept = "concept:rust-lsp-demo",
    library = "rust/demo",
    loss = []
)]
fn documented_value(x: i64) -> i64 {
    x
}

#[test]
fn value_is_non_negative() {
    let x: i64 = documented_value(42);
    assert!(x >= 0);
}
"#;

    let analysis = plugin.exchange(&json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "analyzeDocument",
        "params": {
            "kit_id": "rust",
            "uri": "file:///tmp/provekit-lsp-rust-test/src/lib.rs",
            "file": "/tmp/provekit-lsp-rust-test/src/lib.rs",
            "text": source,
            "document_version": 7,
            "workspace_root": "/tmp/provekit-lsp-rust-test",
            "accepted_protocol_catalog_cids": [],
            "policy_cids": []
        }
    }));

    let result = analysis
        .get("result")
        .unwrap_or_else(|| panic!("analyzeDocument returned error: {analysis}"));

    assert_eq!(result["kind"].as_str(), Some("lsp-document-analysis"));
    assert_eq!(result["schema_version"].as_str(), Some("1"));
    assert_eq!(result["kit_id"].as_str(), Some("rust"));
    assert_eq!(
        result["uri"].as_str(),
        Some("file:///tmp/provekit-lsp-rust-test/src/lib.rs")
    );
    assert!(
        result["document_cid"]
            .as_str()
            .is_some_and(|cid| cid.starts_with("blake3-512:") && cid.len() > 32),
        "missing document CID: {result}"
    );

    let entries = result["entries"]
        .as_array()
        .unwrap_or_else(|| panic!("entries must be an array: {result}"));
    assert!(
        entries.iter().any(|entry| {
            entry["kind"].as_str() == Some("library-sugar-binding-entry")
                && entry["entry"]["source_function_name"].as_str() == Some("documented_value")
        }),
        "expected sugar binding entry for documented_value: {entries:#?}"
    );
    assert!(
        entries.iter().any(|entry| {
            entry["kind"].as_str() == Some("bind-lift-entry")
                && entry["entry"]["source_function_name"].as_str() == Some("value_is_non_negative")
        }),
        "expected Rust test lift entry for value_is_non_negative: {entries:#?}"
    );
    for entry in entries {
        assert_source_range_in_document(&entry["range"], source);
    }

    let statuses = result["statuses"]
        .as_array()
        .unwrap_or_else(|| panic!("statuses must be an array: {result}"));
    for expected in ["materialize", "emit", "check", "prove"] {
        let status = statuses
            .iter()
            .find(|status| status["kind"].as_str() == Some(expected))
            .unwrap_or_else(|| panic!("missing {expected} status: {statuses:#?}"));
        assert_eq!(status["producer"].as_str(), Some("rust-kit"));
        assert!(
            matches!(
                status["state"].as_str(),
                Some(
                    "available"
                        | "unavailable"
                        | "refused"
                        | "unknown"
                        | "passed"
                        | "failed"
                        | "stale"
                )
            ),
            "invalid state on {expected} status: {status}"
        );
        assert_source_range_in_document(&status["range"], source);
    }

    let diagnostics = result["diagnostics"]
        .as_array()
        .unwrap_or_else(|| panic!("diagnostics must be an array: {result}"));
    for diagnostic in diagnostics {
        let code = diagnostic["code"]
            .as_str()
            .unwrap_or_else(|| panic!("diagnostic missing code: {diagnostic}"));
        assert!(
            code.starts_with("provekit.lsp."),
            "diagnostic code must be stable provekit.lsp.* code: {diagnostic}"
        );
        assert_source_range_in_document(&diagnostic["range"], source);
    }

    plugin.shutdown();
}

fn assert_source_range_in_document(range: &Value, source: &str) {
    let start_line = range["start_line"]
        .as_u64()
        .unwrap_or_else(|| panic!("missing start_line: {range}"));
    let start_col = range["start_col"]
        .as_u64()
        .unwrap_or_else(|| panic!("missing start_col: {range}"));
    let end_line = range["end_line"]
        .as_u64()
        .unwrap_or_else(|| panic!("missing end_line: {range}"));
    let end_col = range["end_col"]
        .as_u64()
        .unwrap_or_else(|| panic!("missing end_col: {range}"));

    assert!(start_line >= 1, "ranges use 1-based lines: {range}");
    assert!(
        (start_line, start_col) <= (end_line, end_col),
        "range start is after range end: {range}"
    );

    let lines: Vec<&str> = source.lines().collect();
    let max_line = lines.len() as u64;
    assert!(
        end_line <= max_line,
        "range end_line {end_line} outside {max_line}-line document: {range}"
    );
    let start_len = lines[(start_line - 1) as usize].len() as u64;
    let end_len = lines[(end_line - 1) as usize].len() as u64;
    assert!(
        start_col <= start_len && end_col <= end_len,
        "range columns outside submitted document: {range}"
    );
}
