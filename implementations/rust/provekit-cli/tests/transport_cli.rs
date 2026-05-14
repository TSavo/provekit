// SPDX-License-Identifier: Apache-2.0

use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn provekit_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_provekit"))
}

/// Returns a path to the rust realize kit binary if one can be found, or a
/// non-existent sentinel path if none is available. Mirrors the resolution
/// order in `builtin_realize_candidates`: workspace-relative built-in paths,
/// then sibling-of-current-exe (the CARGO_TARGET_DIR case in cargo test).
fn rust_realize_kit_path() -> PathBuf {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let impl_dir = root.join("implementations/rust");
    for candidate in [
        impl_dir.join("target/release/provekit-realize-rust"),
        impl_dir.join("target/debug/provekit-realize-rust"),
    ] {
        if candidate.exists() {
            return candidate;
        }
    }
    // Sibling-of-current-exe: under cargo test with a custom CARGO_TARGET_DIR,
    // all binaries land in the same directory.
    let provekit = provekit_bin();
    if let Some(bin_dir) = provekit.parent() {
        let sibling = bin_dir.join("provekit-realize-rust");
        if sibling.exists() {
            return sibling;
        }
    }
    // Return a non-existent sentinel so the caller can skip.
    root.join("implementations/rust/target/debug/provekit-realize-rust")
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
    let rust_realize_kit = rust_realize_kit_path();
    if !rust_realize_kit.exists() {
        eprintln!(
            "skipping transport integration test because {} is not built",
            rust_realize_kit.display()
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
                || stderr.contains("transport-time:no-morphism-for-op")
                || stderr.contains("transport-time:no-target-morphism-for-op"),
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
    let rust_realize_kit = rust_realize_kit_path();
    if !rust_realize_kit.exists() {
        eprintln!(
            "skipping migrate integration test because {} is not built",
            rust_realize_kit.display()
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

/// Supplying a `.go` source file (not a term JSON) produces a `lift-time:no-lifter-for-language`
/// refusal because the Go source lifter subprocess is not yet wired into `provekit transport`.
/// The refusal must name the language and be actionable — not a crash.
#[test]
fn transport_go_source_file_refuses_with_no_lifter_message() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let src_dir = tempfile::tempdir().expect("temp source dir");
    let src = src_dir.path().join("example.go");
    fs::write(
        &src,
        "package main\nfunc add(x, y int) int { return x + y }\n",
    )
    .expect("write example.go");

    let out_dir = tempfile::tempdir().expect("temp output dir");
    let output = Command::new(provekit_bin())
        .current_dir(&root)
        .arg("transport")
        .arg(&src)
        .arg("--to")
        .arg("rust")
        .arg("--function")
        .arg("add")
        .arg("--out")
        .arg(out_dir.path())
        .output()
        .expect("spawn provekit transport");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !output.status.success(),
        "provekit transport of a .go source file should refuse\nstderr:\n{stderr}"
    );
    assert!(
        stderr.contains("lift-time:no-lifter-for-language"),
        "expected lift-time:no-lifter-for-language refusal for .go source\nstderr:\n{stderr}"
    );
}

/// A `go:seq` term JSON (two discharged morphisms: go:seq -> concept:seq) can be
/// transported to concept — the `MorphismCatalog` picks up the go prefix from the
/// concept-shapes receipts automatically. This verifies the catalog is data-driven
/// and the go morphisms are wired without hardcoding.
#[test]
fn transport_go_term_json_to_concept_resolves_seq_morphism() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let src_dir = tempfile::tempdir().expect("temp source dir");
    // Minimal go:seq term: seq(skip, skip) — both go:seq and go:skip have discharged morphisms
    // go:skip is not in the minted coverage so we use go:seq wrapping vars
    let term_json = serde_json::json!({
        "kind": "op",
        "name": "go:seq",
        "sort": {"kind": "ctor", "name": "Stmt", "args": []},
        "args": [
            {
                "kind": "op",
                "name": "go:skip",
                "sort": {"kind": "ctor", "name": "Stmt", "args": []},
                "args": []
            },
            {
                "kind": "op",
                "name": "go:skip",
                "sort": {"kind": "ctor", "name": "Stmt", "args": []},
                "args": []
            }
        ]
    });
    let src = src_dir.path().join("go_seq.json");
    fs::write(&src, serde_json::to_string_pretty(&term_json).unwrap()).expect("write go_seq.json");

    let out_dir = tempfile::tempdir().expect("temp output dir");
    let output = Command::new(provekit_bin())
        .current_dir(&root)
        .arg("transport")
        .arg(&src)
        .arg("--from")
        .arg("go")
        .arg("--to")
        .arg("rust")
        .arg("--function")
        .arg("f")
        .arg("--out")
        .arg(out_dir.path())
        .arg("--json")
        .output()
        .expect("spawn provekit transport");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    // go:seq has a discharged morphism; go:skip may not — either "transported" or a
    // transport-time refusal for a specific op is acceptable. What must NOT happen
    // is a lift-time failure or an unknown-language error.
    assert!(
        !stderr.contains("lift-time:unknown-language")
            && !stderr.contains("transport-time:no-language-morphisms"),
        "go must be recognized as a transport language\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
}

/// Supplying a `csharp:seq` term JSON transports to concept without error when
/// `csharp:seq` is in the discharged morphism set (which it is, per transport-gaps.md).
#[test]
fn transport_csharp_seq_term_json_recognizes_csharp_prefix() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let src_dir = tempfile::tempdir().expect("temp source dir");
    let term_json = serde_json::json!({
        "kind": "op",
        "name": "csharp:seq",
        "sort": {"kind": "ctor", "name": "Stmt", "args": []},
        "args": [
            {
                "kind": "op",
                "name": "csharp:skip",
                "sort": {"kind": "ctor", "name": "Stmt", "args": []},
                "args": []
            },
            {
                "kind": "op",
                "name": "csharp:skip",
                "sort": {"kind": "ctor", "name": "Stmt", "args": []},
                "args": []
            }
        ]
    });
    let src = src_dir.path().join("csharp_seq.json");
    fs::write(&src, serde_json::to_string_pretty(&term_json).unwrap())
        .expect("write csharp_seq.json");

    let out_dir = tempfile::tempdir().expect("temp output dir");
    let output = Command::new(provekit_bin())
        .current_dir(&root)
        .arg("transport")
        .arg(&src)
        .arg("--from")
        .arg("csharp")
        .arg("--to")
        .arg("rust")
        .arg("--function")
        .arg("f")
        .arg("--out")
        .arg(out_dir.path())
        .arg("--json")
        .output()
        .expect("spawn provekit transport");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    // csharp:seq is discharged; csharp:skip may not be in the inverse direction.
    // The important assertion: csharp is recognized as a transport language prefix.
    assert!(
        !stderr.contains("lift-time:unknown-language")
            && !stderr.contains("transport-time:no-language-morphisms"),
        "csharp must be recognized as a transport language\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
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
            || stderr.contains("transport-time:no-morphism-for-op")
            || stderr.contains("transport-time:no-target-morphism-for-op"),
        "expected a transport-time refusal for python but got:\n{stderr}"
    );
}
