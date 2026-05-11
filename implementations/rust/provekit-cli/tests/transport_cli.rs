// SPDX-License-Identifier: Apache-2.0

use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn provekit_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_provekit"))
}

#[test]
fn help_lists_transport_command_for_program_ports() {
    let output = Command::new(provekit_bin())
        .arg("--help")
        .output()
        .expect("spawn provekit --help");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "provekit --help failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        stdout
            .lines()
            .any(|line| line.trim_start().starts_with("transport ")
                || line.trim_start() == "transport"),
        "program transport needs a public CLI command\nstdout:\n{stdout}"
    );
}

#[test]
fn transport_foo_c_to_rust_artifacts_when_c_projector_is_built() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let projector = root.join("implementations/c/provekit-walk-c/provekit-c11-term-project");
    if !projector.exists() {
        eprintln!(
            "skipping transport integration test because {} is not built",
            projector.display()
        );
        return;
    }

    let out_dir = tempfile::tempdir().expect("temp output dir");
    let output = Command::new(provekit_bin())
        .current_dir(&root)
        .arg("transport")
        .arg("menagerie/c11-language-signature/example/foo.c")
        .arg("--to")
        .arg("rust")
        .arg("--function")
        .arg("foo")
        .arg("--out")
        .arg(out_dir.path())
        .arg("--json")
        .output()
        .expect("spawn provekit transport");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "provekit transport failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    let report: serde_json::Value = serde_json::from_slice(&output.stdout).expect("json report");
    assert_eq!(report["status"], "transported");
    assert!(report["normalizations"]
        .as_array()
        .expect("normalizations")
        .iter()
        .any(|item| item.as_str().unwrap_or("").contains("c11:bop_eq")));

    let rust_source = fs::read_to_string(out_dir.path().join("foo.rs")).expect("foo.rs");
    assert!(rust_source.contains("pub fn foo(x: i32) -> i32"));
    assert!(rust_source.contains("return -22;"));
    assert!(out_dir.path().join("concept.term.json").exists());
    assert!(out_dir.path().join("rust.term.json").exists());
    assert!(out_dir.path().join("roundtrip.concept.term.json").exists());
}
