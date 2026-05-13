// SPDX-License-Identifier: Apache-2.0
//
// Integration test: lower a small IR fixture and assert the emitted C
// source compiles via `cc -x c -std=c11 -fsyntax-only`.
//
// Also tests that the JSON-RPC sidecar binary speaks the PEP 1.7.0 protocol
// correctly.

use std::io::Write;
use std::process::{Command, Stdio};

use provekit_realize_c_core::{emit, BODY_TEMPLATE_PLUGIN_CID, SUGAR_PLUGIN_CID};

// ---------------------------------------------------------------------------
// Compile smoke: emit() produces syntactically valid C11
// ---------------------------------------------------------------------------

/// Write emitted source (wrapped in necessary includes) to a tempfile and
/// check it with cc -x c -std=c11 -fsyntax-only.
///
/// We prepend standard headers because emitted bodies reference int64_t,
/// abort(), assert(), NULL, size_t, etc. In production use the caller's
/// translation unit already includes these; the smoke test just checks syntax.
fn assert_c_parses(source: &str) {
    // Find a C compiler.
    let cc = if Command::new("cc").arg("--version").output().map(|o| o.status.success()).unwrap_or(false) {
        "cc"
    } else if Command::new("clang").arg("--version").output().map(|o| o.status.success()).unwrap_or(false) {
        "clang"
    } else if Command::new("gcc").arg("--version").output().map(|o| o.status.success()).unwrap_or(false) {
        "gcc"
    } else {
        // No C compiler available -- skip the compile check gracefully.
        eprintln!("assert_c_parses: no C compiler found, skipping compile check");
        return;
    };

    let preamble = "\
#include <stdint.h>\n\
#include <stddef.h>\n\
#include <stdlib.h>\n\
#include <assert.h>\n\n";

    let full_source = format!("{preamble}{source}");

    let dir = tempfile::tempdir().expect("tempdir");
    let src_path = dir.path().join("fixture.c");
    std::fs::write(&src_path, &full_source).expect("write fixture");

    let output = Command::new(cc)
        .args(["-x", "c", "-std=c11", "-fsyntax-only"])
        .arg(&src_path)
        .output()
        .expect("cc spawn failed");

    assert!(
        output.status.success(),
        "C syntax check failed for:\n{full_source}\nstderr: {}",
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
    assert_c_parses(&r.source);
}

#[test]
fn unit_concept_compiles() {
    let r = emit("wrap_unit", &[], &[], "()", "unit");
    assert!(!r.is_stub, "unit should not be a stub");
    assert!(r.source.contains("void wrap_unit(void)"), "void sig");
    assert_c_parses(&r.source);
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
    // C emits `int` for bool, body is `return !b;`
    assert_c_parses(&r.source);
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
    assert_c_parses(&r.source);
}

#[test]
fn free_concept_emits_free_not_stub() {
    // concept:free dispatches via op_pattern -> free(${ptr});
    // C is lossless: free() is the natural surface.
    let r = emit(
        "free_resource",
        &["p".to_string()],
        &["void *".to_string()],
        "()",
        "free",
    );
    assert!(!r.is_stub, "concept:free should not be a stub (op_pattern matches)");
    assert!(
        r.source.contains("free(p)"),
        "concept:free body should contain free(p); got:\n{}",
        r.source
    );
    assert_c_parses(&r.source);
}

#[test]
fn pair_concept_compiles() {
    let r = emit(
        "make_pair",
        &["a".to_string(), "b".to_string()],
        &["i64".to_string(), "i64".to_string()],
        "i64",
        "pair",
    );
    assert!(!r.is_stub);
    assert!(r.source.contains("a + b"));
    assert_c_parses(&r.source);
}

#[test]
fn option_concept_compiles() {
    let r = emit(
        "wrap_option",
        &["v".to_string()],
        &["int64_t *".to_string()],
        "i64",
        "option",
    );
    assert!(!r.is_stub);
    assert_c_parses(&r.source);
}

// ---------------------------------------------------------------------------
// CID constants match the menagerie files
// ---------------------------------------------------------------------------

#[test]
fn cid_constants_match_expected_values() {
    assert_eq!(
        SUGAR_PLUGIN_CID,
        "blake3-512:a67012722271aca3a882bc82fa7b92941453e750a65366534ea37ef8eef593921bb3aa33e6b0051bf64531962346014097ef9a99f14f7c3245de1beea6c076dc"
    );
    assert_eq!(
        BODY_TEMPLATE_PLUGIN_CID,
        "blake3-512:44f18ea2725ec26196399f6511bf3887db52db3c0e1356e7e090ff41f893929e177df9786d6639d8015d08641dda54338f83e1bd24a07828b5bf6b33c0d7d329"
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
    let binary = env!("CARGO_BIN_EXE_provekit-realize-c");
    let mut child = Command::new(binary)
        .arg("--rpc")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn provekit-realize-c");

    {
        let stdin = child.stdin.as_mut().expect("stdin");
        for req in requests {
            writeln!(stdin, "{req}").expect("write request");
        }
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
    let req = rpc_request(
        "provekit.plugin.describe",
        serde_json::json!({"runtime_protocol_versions": ["pep/1.7.0"]}),
    );
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

#[test]
fn rpc_invoke_free_concept_not_stub() {
    // concept:free via RPC must return is_stub=false and source containing free(p).
    let req = rpc_request(
        "provekit.plugin.invoke",
        serde_json::json!({
            "function": "free_resource",
            "params": ["p"],
            "param_types": ["void *"],
            "return_type": "()",
            "concept_name": "free"
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
    assert!(!is_stub, "concept:free should not be a stub via RPC");
    let source = result
        .get("result")
        .and_then(|r| r.get("source"))
        .and_then(|v| v.as_str())
        .expect("result.source");
    assert!(source.contains("free(p)"), "source should contain free(p); got: {source}");
}
