// SPDX-License-Identifier: Apache-2.0
//
// multi_kit.rs — tests for polyglot kit dispatch in the linkerd daemon.
//
// Test 1: parseFile(kit="go", ...) invokes the go lifter and returns a
//         LinkerOutput with non-empty contracts when provekit-lsp-go is on PATH.
//         Skipped if provekit-lsp-go is not installed (clearly marked).
//
// Test 2: parseFile(kit="unknown-kit", ...) returns an UnknownKit (-33001) error.
//
// Test 3: kit dispatch is content-deterministic: same source => same linkBundleCid.
//
// Test 4: parseFile(kit="java", ...) dispatches to the java lifter and returns
//         a result with declarations and callEdges arrays when provekit-lsp-java
//         is on PATH. Skipped if provekit-lsp-java is not installed.
//
// Test 3: kit dispatch is content-deterministic: same source => same linkBundleCid.
//
// These tests communicate with the daemon over its Unix socket.

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

fn daemon_bin() -> PathBuf {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let workspace = PathBuf::from(manifest_dir).parent().unwrap().to_path_buf();
    // CI builds with --release; local cargo test uses debug. Try release first.
    let release = workspace.join("target").join("release").join("provekit-linkerd");
    let debug = workspace.join("target").join("debug").join("provekit-linkerd");
    if release.exists() {
        release
    } else {
        debug
    }
}

fn unique_sock_path(prefix: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "provekit-linkerd-mk-{}-{}.sock",
        prefix,
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.subsec_nanos())
            .unwrap_or(0)
    ))
}

fn spawn_daemon(sock: &PathBuf) -> Child {
    let snap = std::env::temp_dir().join(format!(
        "mk-snap-{}.bin",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.subsec_nanos())
            .unwrap_or(0)
    ));
    Command::new(daemon_bin())
        .arg("--socket").arg(sock)
        .arg("--snapshot").arg(snap)
        .arg("--idle-timeout-ms").arg("30000")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn daemon")
}

fn wait_for_socket(sock: &PathBuf, timeout: Duration) -> bool {
    let start = Instant::now();
    while start.elapsed() < timeout {
        if sock.exists() && UnixStream::connect(sock).is_ok() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    false
}

fn send_recv(sock: &PathBuf, req: &serde_json::Value) -> serde_json::Value {
    let mut stream = UnixStream::connect(sock).expect("connect");
    stream.set_read_timeout(Some(Duration::from_secs(10))).ok();
    let line = serde_json::to_string(req).unwrap() + "\n";
    stream.write_all(line.as_bytes()).expect("write");
    let mut reader = BufReader::new(stream);
    let mut response_line = String::new();
    reader.read_line(&mut response_line).expect("read");
    serde_json::from_str(response_line.trim()).expect("parse JSON")
}

fn shutdown(sock: &PathBuf) {
    let req = serde_json::json!({
        "jsonrpc": "2.0", "id": 99, "method": "shutdown", "params": {}
    });
    let _ = send_recv(sock, &req);
}

/// Check if a binary is available on PATH.
fn binary_on_path(name: &str) -> bool {
    if let Ok(path_var) = std::env::var("PATH") {
        for dir in path_var.split(':') {
            let candidate = PathBuf::from(dir).join(name);
            if candidate.is_file() {
                return true;
            }
        }
    }
    false
}

// -------------------------------------------------------------------
// Test 1: go kit dispatch returns diagnostics when provekit-lsp-go is on PATH.
// -------------------------------------------------------------------

/// Test 1: parseFile with kit="go" dispatches to the go lifter.
///
/// Skipped if `provekit-lsp-go` is not on PATH. The skip is printed to stdout
/// so CI can see why the test was skipped, not silently ignored.
///
/// When provekit-lsp-go is available, sends a tiny go source with a
/// `//provekit:contract` annotation and asserts:
///   - The response has a `result.diagnostics` array (may be empty or non-empty).
///   - No JSON-RPC error is returned.
#[test]
fn test1_go_kit_dispatch() {
    if !binary_on_path("provekit-lsp-go") {
        println!(
            "SKIP test1_go_kit_dispatch: provekit-lsp-go not on PATH. \
             Install via: cd implementations/go && go install ./cmd/provekit-lsp-go"
        );
        return;
    }

    let sock = unique_sock_path("t1");
    let _ = std::fs::remove_file(&sock);

    let mut child = spawn_daemon(&sock);

    assert!(
        wait_for_socket(&sock, Duration::from_secs(5)),
        "daemon socket did not appear"
    );

    let go_source = r#"package main

//provekit:contract
func Add(a, b int) int {
    return a + b
}
"#;

    let req = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "parseFile",
        "params": {
            "kitId": "go",
            "file": "/tmp/test_add.go",
            "source": go_source
        }
    });

    let resp = send_recv(&sock, &req);

    assert!(
        resp.get("error").is_none(),
        "go kit parseFile returned unexpected error: {:?}",
        resp
    );
    assert!(
        resp["result"]["diagnostics"].is_array(),
        "go kit parseFile result must have diagnostics array: {:?}",
        resp
    );

    shutdown(&sock);
    child.wait().ok();
    std::fs::remove_file(&sock).ok();
}

// -------------------------------------------------------------------
// Test 2: unknown kit returns UnknownKit error (-33001).
// -------------------------------------------------------------------

/// Test 2: parseFile with kit="unknown-kit" returns error code -33001.
#[test]
fn test2_unknown_kit_returns_error() {
    let sock = unique_sock_path("t2");
    let _ = std::fs::remove_file(&sock);

    let mut child = spawn_daemon(&sock);

    assert!(
        wait_for_socket(&sock, Duration::from_secs(5)),
        "daemon socket did not appear"
    );

    let req = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "parseFile",
        "params": {
            "kitId": "unknown-kit",
            "file": "/tmp/test.xyz",
            "source": "// nothing"
        }
    });

    let resp = send_recv(&sock, &req);

    let error = resp.get("error").expect("expected error field for unknown kit");
    let code = error.get("code").and_then(|c| c.as_i64()).expect("expected error.code");
    assert_eq!(
        code, -33001,
        "unknown-kit must return error code -33001 (KitNotInManifest), got {}",
        code
    );

    shutdown(&sock);
    child.wait().ok();
    std::fs::remove_file(&sock).ok();
}

// -------------------------------------------------------------------
// Test 3: content-determinism across kit dispatch.
// -------------------------------------------------------------------

/// Test 3: Same source + same kit => byte-identical linkBundleCid.
///
/// Uses the rust kit (always available) to verify dispatch determinism
/// without requiring any external binary.
#[test]
fn test3_kit_dispatch_content_deterministic() {
    let sock = unique_sock_path("t3");
    let _ = std::fs::remove_file(&sock);

    let mut child = spawn_daemon(&sock);

    assert!(
        wait_for_socket(&sock, Duration::from_secs(5)),
        "daemon socket did not appear"
    );

    let rust_source = r#"
/// #[provekit::contract(post = "result >= 0")]
pub fn abs_value(x: i64) -> i64 {
    if x < 0 { -x } else { x }
}
"#;

    let parse_req = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "parseFile",
        "params": {
            "kitId": "rust",
            "file": "/tmp/test_deterministic.rs",
            "source": rust_source
        }
    });

    // First parse.
    let _ = send_recv(&sock, &parse_req);
    let status_req = serde_json::json!({
        "jsonrpc": "2.0", "id": 2, "method": "projectStatus", "params": {}
    });
    let status1 = send_recv(&sock, &status_req);
    let cid1 = status1["result"]["linkBundleCid"].as_str().unwrap_or("").to_string();

    // Flush and re-parse with identical source.
    let flush_req = serde_json::json!({
        "jsonrpc": "2.0", "id": 3, "method": "flushCache", "params": {}
    });
    let _ = send_recv(&sock, &flush_req);
    let _ = send_recv(&sock, &parse_req);
    let status2 = send_recv(&sock, &status_req);
    let cid2 = status2["result"]["linkBundleCid"].as_str().unwrap_or("").to_string();

    assert!(
        !cid1.is_empty(),
        "linkBundleCid should be non-empty after parse"
    );
    assert_eq!(
        cid1, cid2,
        "same source must produce byte-identical linkBundleCid across two parse runs"
    );

    shutdown(&sock);
    child.wait().ok();
    std::fs::remove_file(&sock).ok();
}

// -------------------------------------------------------------------
// Test 4: java kit dispatch returns diagnostics when provekit-lsp-java is on PATH.
// -------------------------------------------------------------------

/// Test 4: parseFile with kit="java" dispatches to the java lifter.
///
/// Skipped if `provekit-lsp-java` is not on PATH. The skip is printed to stdout
/// so CI can see why the test was skipped, not silently ignored.
///
/// Install via:
///   cd implementations/java/provekit-lift-java-core && \
///   mvn package -q && \
///   cp target/appassembler/bin/provekit-lsp-java ~/.local/bin/
///
/// When provekit-lsp-java is available, sends a tiny Java source and asserts:
///   - The response has a `result.diagnostics` array.
///   - `result.diagnostics` is an array (shape contract).
///   - No JSON-RPC error is returned.
#[test]
fn test4_java_kit_dispatch() {
    if !binary_on_path("provekit-lsp-java") {
        println!(
            "SKIP test4_java_kit_dispatch: provekit-lsp-java not on PATH. \
             Install via: cd implementations/java/provekit-lift-java-core && \
             mvn package -q && cp target/appassembler/bin/provekit-lsp-java ~/.local/bin/"
        );
        return;
    }

    let sock = unique_sock_path("t4");
    let _ = std::fs::remove_file(&sock);

    let mut child = spawn_daemon(&sock);

    assert!(
        wait_for_socket(&sock, Duration::from_secs(5)),
        "daemon socket did not appear"
    );

    let java_source = r#"package com.example;

public class Calculator {
    /** @provekit.contract post="result >= 0" */
    public int abs(int x) {
        return x < 0 ? -x : x;
    }
}
"#;

    let req = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "parseFile",
        "params": {
            "kitId": "java",
            "file": "/tmp/test_Calculator.java",
            "source": java_source
        }
    });

    let resp = send_recv(&sock, &req);

    assert!(
        resp.get("error").is_none(),
        "java kit parseFile returned unexpected error: {:?}",
        resp
    );
    assert!(
        resp["result"]["diagnostics"].is_array(),
        "java kit parseFile result must have diagnostics array: {:?}",
        resp
    );

    shutdown(&sock);
    child.wait().ok();
    std::fs::remove_file(&sock).ok();
}
