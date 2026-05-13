// SPDX-License-Identifier: Apache-2.0
//
// Integration test: lower a small IR fixture and assert the emitted Rust
// source compiles via `rustc --edition 2021 --crate-type lib --emit=metadata`.
//
// Also tests that the JSON-RPC sidecar binary speaks the PEP 1.7.0 protocol
// correctly.

use std::io::Write;
use std::process::{Command, Stdio};

use provekit_realize_rust_core::{emit, BODY_TEMPLATE_PLUGIN_CID, SUGAR_PLUGIN_CID};

// ---------------------------------------------------------------------------
// Unit-level: emit() produces compilable Rust
// ---------------------------------------------------------------------------

/// Write emitted source to a tempfile and compile it with rustc.
fn assert_compiles(source: &str) {
    let dir = tempfile::tempdir().expect("tempdir");
    let src_path = dir.path().join("fixture.rs");
    std::fs::write(&src_path, source).expect("write fixture");

    let output = Command::new("rustc")
        .args([
            "--edition",
            "2021",
            "--crate-type",
            "lib",
            "--emit=metadata",
            "--out-dir",
        ])
        .arg(dir.path())
        .arg(&src_path)
        .output()
        .expect("rustc not found on PATH");

    assert!(
        output.status.success(),
        "rustc failed for:\n{source}\nstderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn identity_concept_compiles() {
    let r = emit(
        "wrap_identity",
        &["x".to_string()],
        &["i64".to_string()],
        "i64",
        "identity",
    );
    assert!(!r.is_stub, "identity should not be a stub");
    assert_compiles(&r.source);
}

#[test]
fn option_concept_compiles() {
    let r = emit(
        "wrap_option",
        &["v".to_string()],
        &["Option<i64>".to_string()],
        "i64",
        "option",
    );
    assert!(!r.is_stub, "option should not be a stub");
    assert_compiles(&r.source);
}

#[test]
fn bool_cell_concept_compiles() {
    let r = emit(
        "wrap_bool_cell",
        &["b".to_string()],
        &["bool".to_string()],
        "bool",
        "bool-cell",
    );
    assert!(!r.is_stub, "bool-cell should not be a stub");
    assert_compiles(&r.source);
}

#[test]
fn unit_concept_compiles() {
    let r = emit("wrap_unit", &[], &[], "()", "unit");
    assert!(!r.is_stub, "unit should not be a stub");
    assert_compiles(&r.source);
}

#[test]
fn unknown_concept_stub_compiles() {
    let r = emit(
        "wrap_unknown",
        &["x".to_string()],
        &["i64".to_string()],
        "i64",
        "concept:unknown-xyz",
    );
    assert!(r.is_stub, "unknown concept should be a stub");
    assert_compiles(&r.source);
}

#[test]
fn list_concept_compiles() {
    // list takes a slice -- use Vec<i64> for the fixture.
    let r = emit(
        "wrap_list",
        &["xs".to_string()],
        &["Vec<i64>".to_string()],
        "i64",
        "list",
    );
    // list template uses `.iter().sum()` which compiles on Vec<i64>.
    assert_compiles(&r.source);
}

#[test]
fn free_concept_compiles() {
    // concept:free dispatches via op_pattern to drop(${val});
    // The emitted body must be compilable Rust.
    let r = emit(
        "free_resource",
        &["v".to_string()],
        &["Box<i64>".to_string()],
        "()",
        "free",
    );
    assert!(!r.is_stub, "concept:free should not be a stub");
    assert!(r.source.contains("drop(v)"), "should contain drop(v)");
    assert_compiles(&r.source);
}

// ---------------------------------------------------------------------------
// CID constants match the menagerie files
// ---------------------------------------------------------------------------

#[test]
fn cid_constants_match_expected_values() {
    assert_eq!(
        SUGAR_PLUGIN_CID,
        "blake3-512:666480f85eafb36d750c4fef4e5df42e33740ceb1f8e0bff2c82743beeccb0aff11d0a65e1c05827782d5c1023b853e5a2cccc3755d5a161c07668e4e7a5ae4a"
    );
    assert_eq!(
        BODY_TEMPLATE_PLUGIN_CID,
        "blake3-512:39bf0c5b81d7769d60e82326a36daeb66241c7527e4ac542b1ce9e4ab40cb19a25a4e4b25e406106341f1c414f7ccc3b7523fab4aa3ee34ddc751f036d26e949"
    );
}

// ---------------------------------------------------------------------------
// JSON-RPC sidecar: describe and invoke
// ---------------------------------------------------------------------------

fn rpc_request(method: &str, params: serde_json::Value) -> String {
    serde_json::to_string(&serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": method,
        "params": params
    }))
    .unwrap()
}

fn run_rpc_binary(requests: &[String]) -> Vec<serde_json::Value> {
    let binary = env!("CARGO_BIN_EXE_provekit-realize-rust");
    let mut child = Command::new(binary)
        .arg("--rpc")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn provekit-realize-rust");

    {
        let stdin = child.stdin.as_mut().expect("stdin");
        for req in requests {
            writeln!(stdin, "{req}").expect("write request");
        }
        // Send shutdown to allow clean exit.
        let shutdown = rpc_request("provekit.plugin.shutdown", serde_json::Value::Null);
        writeln!(stdin, "{shutdown}").expect("write shutdown");
    }

    let output = child.wait_with_output().expect("wait");
    let stdout = String::from_utf8_lossy(&output.stdout);
    stdout
        .lines()
        .filter(|l| !l.trim().is_empty())
        .filter_map(|l| serde_json::from_str(l).ok())
        .collect()
}

#[test]
fn rpc_describe_returns_sugar_cid() {
    let req = rpc_request("provekit.plugin.describe", serde_json::json!({"runtime_protocol_versions": ["pep/1.7.0"]}));
    let responses = run_rpc_binary(&[req]);
    assert!(!responses.is_empty(), "expected at least one response");
    let result = &responses[0];
    assert!(result.get("error").is_none(), "describe should not error: {result}");
    let cid = result
        .get("result")
        .and_then(|r| r.get("header"))
        .and_then(|h| h.get("cid"))
        .and_then(|c| c.as_str())
        .expect("result.header.cid");
    assert_eq!(cid, SUGAR_PLUGIN_CID);
}

#[test]
fn rpc_invoke_identity_not_stub() {
    let req = rpc_request(
        "provekit.plugin.invoke",
        serde_json::json!({
            "function": "wrap_identity",
            "params": ["x"],
            "param_types": ["i64"],
            "return_type": "i64",
            "concept_name": "identity"
        }),
    );
    let responses = run_rpc_binary(&[req]);
    assert!(!responses.is_empty());
    let result = &responses[0];
    assert!(result.get("error").is_none(), "invoke error: {result}");
    let is_stub = result
        .get("result")
        .and_then(|r| r.get("is_stub"))
        .and_then(|v| v.as_bool())
        .expect("result.is_stub");
    assert!(!is_stub, "identity should not be a stub");
}

#[test]
fn rpc_invoke_unknown_concept_is_stub() {
    let req = rpc_request(
        "provekit.plugin.invoke",
        serde_json::json!({
            "function": "wrap_unknown",
            "params": ["x"],
            "param_types": ["i64"],
            "return_type": "i64",
            "concept_name": "concept:unknown-xyz"
        }),
    );
    let responses = run_rpc_binary(&[req]);
    assert!(!responses.is_empty());
    let result = &responses[0];
    assert!(result.get("error").is_none(), "invoke error: {result}");
    let is_stub = result
        .get("result")
        .and_then(|r| r.get("is_stub"))
        .and_then(|v| v.as_bool())
        .expect("result.is_stub");
    assert!(is_stub, "unknown concept should be a stub");
}
