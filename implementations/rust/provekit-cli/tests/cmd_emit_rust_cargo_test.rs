// SPDX-License-Identifier: Apache-2.0

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;

fn provekit_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_provekit"))
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf()
}

fn build_rust_cargo_test_emitter() -> PathBuf {
    let rust_root = repo_root().join("implementations").join("rust");
    let expected_bin = provekit_bin()
        .parent()
        .expect("provekit bin parent")
        .join("provekit-emit-rust-cargo-test");
    let mut args = vec![
        "build",
        "-p",
        "provekit-emit-rust-cargo-test",
        "--bin",
        "provekit-emit-rust-cargo-test",
    ];
    if expected_bin
        .parent()
        .and_then(|parent| parent.file_name())
        .and_then(|name| name.to_str())
        == Some("release")
    {
        args.push("--release");
    }
    let built = Command::new("cargo")
        .current_dir(&rust_root)
        .args(args)
        .output()
        .expect("spawn cargo build provekit-emit-rust-cargo-test");
    assert!(
        built.status.success(),
        "cargo build provekit-emit-rust-cargo-test failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&built.stdout),
        String::from_utf8_lossy(&built.stderr)
    );
    assert!(
        expected_bin.exists(),
        "missing emitter binary {}",
        expected_bin.display()
    );
    expected_bin
}

fn install_emit_registration(project: &Path, emitter: &Path) {
    let provekit_dir = project.join(".provekit");
    fs::create_dir_all(&provekit_dir).expect("mkdir .provekit");
    fs::write(
        provekit_dir.join("config.toml"),
        "[[plugins]]\n\
         name = \"rust-cargo-test\"\n\
         surface = \"rust-cargo-test\"\n\
         emit = \"cargo-test\"\n",
    )
    .expect("write project config");

    let manifest = project
        .join(".provekit")
        .join("emit")
        .join("rust-cargo-test")
        .join("manifest.toml");
    fs::create_dir_all(manifest.parent().unwrap()).expect("mkdir manifest");
    fs::write(
        manifest,
        format!(
            "name = \"rust-cargo-test\"\ncommand = [\"{}\", \"--rpc\"]\nworking_dir = \".\"\nprotocol_versions = [\"pep/1.7.0\"]\n",
            emitter
                .display()
                .to_string()
                .replace('\\', "\\\\")
                .replace('"', "\\\"")
        ),
    )
    .expect("write emit manifest");
}

fn copy_dir_recursive(src: &Path, dst: &Path) {
    fs::create_dir_all(dst).unwrap_or_else(|_| panic!("mkdir {}", dst.display()));
    for entry in fs::read_dir(src).unwrap_or_else(|_| panic!("read {}", src.display())) {
        let entry = entry.expect("read dir entry");
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());
        if entry.file_type().expect("entry file type").is_dir() {
            copy_dir_recursive(&src_path, &dst_path);
        } else {
            fs::copy(&src_path, &dst_path).unwrap_or_else(|_| {
                panic!("copy {} -> {}", src_path.display(), dst_path.display())
            });
        }
    }
}

fn rewrite_manifest_command(manifest: &Path, command: &Path) {
    let text = fs::read_to_string(manifest)
        .unwrap_or_else(|_| panic!("read checked-in manifest {}", manifest.display()));
    let escaped = command
        .display()
        .to_string()
        .replace('\\', "\\\\")
        .replace('"', "\\\"");
    let rewritten = text
        .lines()
        .map(|line| {
            if line.trim_start().starts_with("command = ") {
                format!("command = [\"{escaped}\", \"--rpc\"]")
            } else {
                line.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    fs::write(manifest, format!("{rewritten}\n"))
        .unwrap_or_else(|_| panic!("write manifest {}", manifest.display()));
}

fn write_double_emit_plan(plan: &Path) {
    fs::write(
        plan,
        serde_json::to_vec_pretty(&serde_json::json!({
            "contract_id": "concept:eq",
            "function": "double",
            "params": ["x"],
            "param_types": ["i64"],
            "return_type": "i64",
            "predicates": [{
                "kind": "atomic",
                "name": "=",
                "args": [
                    {"kind": "ctor", "name": "double", "args": [
                        {"kind": "const", "value": 3, "sort": {"kind": "primitive", "name": "i64"}}
                    ]},
                    {"kind": "const", "value": 6, "sort": {"kind": "primitive", "name": "i64"}}
                ]
            }]
        }))
        .expect("encode plan"),
    )
    .expect("write plan");
}

fn write_cargo_test_project(project: &Path) -> PathBuf {
    fs::create_dir_all(project.join("src")).expect("mkdir src");
    fs::write(
        project.join("Cargo.toml"),
        "[package]\nname = \"provekit-rust-emit-check\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )
    .expect("write Cargo.toml");
    fs::write(
        project.join("src").join("lib.rs"),
        "pub fn add(a: i64, b: i64) -> i64 {\n    a + b\n}\n\ninclude!(\"provekit_emitted.rs\");\n",
    )
    .expect("write src/lib.rs");
    project.join("src")
}

#[test]
fn emit_rust_cargo_test_dispatches_real_emitter_and_cargo_checks_output() {
    let temp = tempfile::tempdir().expect("tempdir");
    let project = temp.path().join("project");
    fs::create_dir_all(&project).expect("mkdir project");

    let emitter = build_rust_cargo_test_emitter();
    install_emit_registration(&project, &emitter);
    let out_dir = write_cargo_test_project(&project);

    let plan = project.join("plan.json");
    fs::write(
        &plan,
        serde_json::to_vec_pretty(&serde_json::json!({
            "contract_id": "concept:eq",
            "function": "add",
            "params": ["a", "b"],
            "param_types": ["i64", "i64"],
            "return_type": "i64",
            "predicates": [{
                "kind": "atomic",
                "name": "=",
                "args": [
                    {"kind": "ctor", "name": "add", "args": [
                        {"kind": "const", "value": 2, "sort": {"kind": "primitive", "name": "i64"}},
                        {"kind": "const", "value": 3, "sort": {"kind": "primitive", "name": "i64"}}
                    ]},
                    {"kind": "const", "value": 5, "sort": {"kind": "primitive", "name": "i64"}}
                ]
            }]
        }))
        .expect("encode plan"),
    )
    .expect("write plan");

    let output = Command::new(provekit_bin())
        .arg("emit")
        .arg("--project")
        .arg(&project)
        .arg("--target")
        .arg("rust")
        .arg("--framework")
        .arg("cargo-test")
        .arg("--plan")
        .arg(&plan)
        .arg("--out-dir")
        .arg(&out_dir)
        .arg("--compile-check")
        .arg("--json")
        .output()
        .expect("spawn provekit emit rust");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "provekit emit rust failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    let receipt: Value = serde_json::from_str(&stdout).expect("emit stdout is JSON");
    assert_eq!(receipt["ok"], true, "receipt: {receipt}");
    assert_eq!(receipt["targetLanguage"], "rust", "receipt: {receipt}");
    assert_eq!(
        receipt["targetFramework"], "cargo-test",
        "receipt: {receipt}"
    );
    assert_eq!(receipt["compileCheck"]["ok"], true, "receipt: {receipt}");
    assert_eq!(
        receipt["compileCheck"]["command"], "cargo test --quiet",
        "receipt: {receipt}"
    );

    let emitted_path = out_dir.join("provekit_emitted.rs");
    let emitted = fs::read_to_string(&emitted_path)
        .unwrap_or_else(|_| panic!("read emitted {}", emitted_path.display()));
    assert!(
        emitted.contains("assert_eq!(add(2, 3), 5);"),
        "emitted:\n{emitted}"
    );
}

#[test]
fn emit_rust_cargo_test_uses_checked_in_rust_double_registration() {
    let temp = tempfile::tempdir().expect("tempdir");
    let project = temp.path().join("rust-double");
    copy_dir_recursive(&repo_root().join("examples").join("rust-double"), &project);
    let manifest = project
        .join(".provekit")
        .join("emit")
        .join("rust-cargo-test")
        .join("manifest.toml");
    assert!(
        manifest.exists(),
        "checked-in rust-double fixture must register the rust-cargo-test emitter at {}",
        manifest.display()
    );

    let emitter = build_rust_cargo_test_emitter();
    rewrite_manifest_command(&manifest, &emitter);
    let out_dir = project.join("src");

    let plan = project.join("plan.json");
    write_double_emit_plan(&plan);

    let output = Command::new(provekit_bin())
        .arg("emit")
        .arg("--project")
        .arg(&project)
        .arg("--target")
        .arg("rust")
        .arg("--framework")
        .arg("cargo-test")
        .arg("--plan")
        .arg(&plan)
        .arg("--out-dir")
        .arg(&out_dir)
        .arg("--compile-check")
        .arg("--json")
        .output()
        .expect("spawn provekit emit rust");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "checked-in Rust fixture registration must drive cargo-test emit\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    let receipt: Value = serde_json::from_str(&stdout).expect("emit stdout is JSON");
    assert_eq!(receipt["ok"], true, "receipt: {receipt}");
    assert_eq!(receipt["surface"], "rust-cargo-test", "receipt: {receipt}");
    assert_eq!(receipt["targetLanguage"], "rust", "receipt: {receipt}");
    assert_eq!(
        receipt["targetFramework"], "cargo-test",
        "receipt: {receipt}"
    );
    assert_eq!(receipt["compileCheck"]["ok"], true, "receipt: {receipt}");
    assert_eq!(receipt["isComplete"], true, "receipt: {receipt}");
}
