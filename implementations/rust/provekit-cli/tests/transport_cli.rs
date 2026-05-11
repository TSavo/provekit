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

/// Python and Go do not have discharged morphisms for the arithmetic and control-flow
/// ops used by `sum_to` (their op specs carry `pre: true` rather than the
/// `no_signed_overflow` precondition required by the concept hub). The CLI must
/// refuse loudly rather than silently produce wrong output.
#[test]
fn transport_sum_to_c_to_python_and_go_refuses_loudly_when_c_projector_is_built() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let projector = root.join("implementations/c/provekit-walk-c/provekit-c11-term-project");
    if !projector.exists() {
        eprintln!(
            "skipping transport integration test because {} is not built",
            projector.display()
        );
        return;
    }

    let src_dir = tempfile::tempdir().expect("temp source dir");
    let src = src_dir.path().join("sum_to.c");
    fs::write(
        &src,
        "int sum_to(int n){ int s=0; int i=0; while(i<n){ s=s+i; i=i+1; } return s; }\n",
    )
    .expect("write sum_to.c");

    for target in ["python", "go"] {
        let out_dir = tempfile::tempdir().expect("temp output dir");
        let output = Command::new(provekit_bin())
            .current_dir(&root)
            .arg("transport")
            .arg(&src)
            .arg("--to")
            .arg(target)
            .arg("--function")
            .arg("sum_to")
            .arg("--out")
            .arg(out_dir.path())
            .output()
            .expect("spawn provekit transport");

        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            !output.status.success(),
            "provekit transport to {target} should refuse but succeeded\nstderr:\n{stderr}"
        );
        assert!(
            stderr.contains("transport-time:no-target-morphisms")
                || stderr.contains("transport-time:no-morphism-for-op"),
            "expected a transport-time refusal for {target} but got:\n{stderr}"
        );
    }
}

/// Python does not have discharged morphisms for `foo`'s ops (eq, if/return on
/// sign-overflow-constrained ints). The migrate alias must refuse rather than silently
/// produce incorrect output. Transport to rust IS discharged.
#[test]
fn migrate_alias_transports_foo_c_to_rust_when_c_projector_is_built() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let projector = root.join("implementations/c/provekit-walk-c/provekit-c11-term-project");
    if !projector.exists() {
        eprintln!(
            "skipping migrate integration test because {} is not built",
            projector.display()
        );
        return;
    }

    let out_dir = tempfile::tempdir().expect("temp output dir");
    let output = Command::new(provekit_bin())
        .current_dir(&root)
        .arg("migrate")
        .arg("menagerie/cross-language-port/foo.c")
        .arg("--to")
        .arg("rust")
        .arg("--function")
        .arg("foo")
        .arg("--out")
        .arg(out_dir.path())
        .arg("--json")
        .output()
        .expect("spawn provekit migrate");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "provekit migrate to rust failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    let rust_source = fs::read_to_string(out_dir.path().join("foo.rs")).expect("foo.rs");
    assert!(rust_source.contains("pub fn foo(x: i32) -> i32"));
    assert!(rust_source.contains("return -22;"));
}

/// Python does not have discharged morphisms for the ops in `foo.c`; the CLI must refuse.
#[test]
fn migrate_alias_refuses_foo_c_to_python_when_c_projector_is_built() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let projector = root.join("implementations/c/provekit-walk-c/provekit-c11-term-project");
    if !projector.exists() {
        eprintln!(
            "skipping migrate integration test because {} is not built",
            projector.display()
        );
        return;
    }

    let out_dir = tempfile::tempdir().expect("temp output dir");
    let output = Command::new(provekit_bin())
        .current_dir(&root)
        .arg("migrate")
        .arg("menagerie/cross-language-port/foo.c")
        .arg("--to")
        .arg("python")
        .arg("--function")
        .arg("foo")
        .arg("--out")
        .arg(out_dir.path())
        .output()
        .expect("spawn provekit migrate");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !output.status.success(),
        "provekit migrate to python should refuse but succeeded\nstderr:\n{stderr}"
    );
    assert!(
        stderr.contains("transport-time:no-target-morphisms")
            || stderr.contains("transport-time:no-morphism-for-op"),
        "expected a transport-time refusal for python but got:\n{stderr}"
    );
}
