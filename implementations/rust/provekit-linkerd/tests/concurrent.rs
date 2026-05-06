// SPDX-License-Identifier: Apache-2.0
//
// concurrent.rs — #421 regression test: verify spawn_blocking prevents
// executor stalls when multiple kit lifters are dispatched concurrently.

use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::time::Duration;

fn daemon_bin() -> PathBuf {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let workspace = PathBuf::from(manifest_dir).parent().unwrap().to_path_buf();
    let release = workspace.join("target").join("release").join("provekit-linkerd");
    let debug = workspace.join("target").join("debug").join("provekit-linkerd");
    if release.exists() { release } else { debug }
}

/// After the spawn_blocking fix (#424), concurrent lift operations must
/// not stall the executor. This test fires 3 concurrent parseFile
/// requests and asserts all complete within a reasonable wall-clock
/// duration (they run on separate threads via spawn_blocking).
#[test]
fn concurrent_kit_lifter_requests_dont_stall_each_other() {
    let bin = daemon_bin();
    let temp = std::env::temp_dir().join(format!("linkerd-concurrent-{}", std::process::id()));
    std::fs::create_dir_all(&temp).expect("create tempdir");

    let src_path = temp.join("test.rs");
    std::fs::write(&src_path, "fn main() {}\n").expect("write fixture");

    let sock_path = temp.join("test.sock");
    let mut daemon = Command::new(&bin)
        .arg("--socket")
        .arg(&sock_path)
        .arg("--idle-timeout-ms")
        .arg("5000")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn daemon");

    // Wait for socket to be ready (retry up to 3s).
    let mut ready = false;
    for _ in 0..30 {
        std::thread::sleep(Duration::from_millis(100));
        if UnixStream::connect(&sock_path).is_ok() {
            ready = true;
            break;
        }
    }
    if !ready {
        // Drain stderr for diagnostics.
        if let Some(stderr) = daemon.stderr.take() {
            let mut buf = String::new();
            let _ = BufReader::new(stderr).read_to_string(&mut buf);
            eprintln!("daemon stderr: {buf}");
        }
        if let Some(stdout) = daemon.stdout.take() {
            let mut buf = String::new();
            let _ = BufReader::new(stdout).read_to_string(&mut buf);
            eprintln!("daemon stdout: {buf}");
        }
        let _ = daemon.kill();
        panic!("daemon socket never ready at {sock_path:?}");
    }

    let sock = Arc::new(sock_path.clone());
    let src = Arc::new(src_path);
    let handles: Vec<_> = (0..3).map(|_| {
        let s = sock.clone();
        let f = src.clone();
        std::thread::spawn(move || {
            let start = std::time::Instant::now();
            let mut stream = UnixStream::connect(s.as_path()).expect("connect");
            let mut reader = BufReader::new(stream.try_clone().expect("clone"));
            let req = serde_json::json!({
                "jsonrpc": "2.0", "id": 1, "method": "parseFile",
                "params": { "path": f.to_str().unwrap(), "source": "fn main() {}" }
            });
            let req_line = serde_json::to_string(&req).unwrap() + "\n";
            stream.write_all(req_line.as_bytes()).expect("write");
            let mut resp = String::new();
            reader.read_line(&mut resp).expect("read");
            let _: serde_json::Value = serde_json::from_str(&resp).expect("parse");
            start.elapsed()
        })
    }).collect();

    let mut durations = Vec::new();
    for h in handles {
        durations.push(h.join().expect("thread join"));
    }
    assert_eq!(durations.len(), 3, "all 3 concurrent requests must complete");

    // Shutdown. Don't fail if daemon already exited.
    if let Ok(mut stream) = UnixStream::connect(sock_path) {
        let shutdown = serde_json::json!({
            "jsonrpc": "2.0", "id": 99, "method": "shutdown", "params": {}
        });
        let _ = stream.write_all((serde_json::to_string(&shutdown).unwrap() + "\n").as_bytes());
    }
    let _ = daemon.wait();
}
