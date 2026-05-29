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

fn go_available() -> bool {
    Command::new("go")
        .arg("version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn build_go_testify_emitter() -> PathBuf {
    let go_root = repo_root().join("implementations").join("go");
    let out = std::env::temp_dir().join(format!("provekit-emit-go-testify-{}", std::process::id()));
    let built = Command::new("go")
        .current_dir(&go_root)
        .args([
            "build",
            "-o",
            out.to_str().expect("utf8 path"),
            "./provekit-emit-go-testify/cmd/provekit-emit-go-testify",
        ])
        .output()
        .expect("spawn go build");
    assert!(
        built.status.success(),
        "go build provekit-emit-go-testify failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&built.stdout),
        String::from_utf8_lossy(&built.stderr)
    );
    out
}

fn install_emit_registration(project: &Path, emitter: &Path) {
    let provekit_dir = project.join(".provekit");
    fs::create_dir_all(&provekit_dir).expect("mkdir .provekit");
    fs::write(
        provekit_dir.join("config.toml"),
        "exam_manifest_cid = \"blake3-512:00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000\"\n\
         \n\
         [[plugins]]\n\
         name = \"go-testify\"\n\
         surface = \"go-testify\"\n\
         emit = \"testify\"\n",
    )
    .expect("write project config");

    let manifest = project
        .join(".provekit")
        .join("emit")
        .join("go-testify")
        .join("manifest.toml");
    fs::create_dir_all(manifest.parent().unwrap()).expect("mkdir manifest");
    fs::write(
        manifest,
        format!(
            "name = \"go-testify\"\ncommand = [\"{}\", \"--rpc\"]\nworking_dir = \".\"\nprotocol_versions = [\"pep/1.7.0\"]\n",
            emitter
                .display()
                .to_string()
                .replace('\\', "\\\\")
                .replace('"', "\\\"")
        ),
    )
    .expect("write emit manifest");
}

#[test]
fn emit_go_testify_dispatches_separate_emitter_and_compile_checks() {
    if !go_available() {
        eprintln!("skipping: go not on PATH");
        return;
    }

    let temp = tempfile::tempdir().expect("tempdir");
    let project = temp.path().join("project");
    let out_dir = temp.path().join("out");
    fs::create_dir_all(&project).expect("mkdir project");
    fs::create_dir_all(&out_dir).expect("mkdir out");
    fs::write(
        out_dir.join("go.mod"),
        "module example.com/provekit_emit\n\n\
         go 1.22\n\n\
         require github.com/stretchr/testify v1.9.0\n",
    )
    .expect("write go.mod");

    let emitter = build_go_testify_emitter();
    install_emit_registration(&project, &emitter);

    let plan = project.join("plan.json");
    fs::write(
        &plan,
        serde_json::to_vec_pretty(&serde_json::json!({
            "contract_id": "concept:eq",
            "package_name": "sample",
            "function": "Id",
            "params": ["x"],
            "param_types": ["int"],
            "return_type": "int",
            "predicates": [{
                "kind": "atomic",
                "name": "concept:eq",
                "args": [
                    {"kind": "var", "name": "x"},
                    {"kind": "var", "name": "x"}
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
        .arg("go")
        .arg("--framework")
        .arg("testify")
        .arg("--plan")
        .arg(&plan)
        .arg("--out-dir")
        .arg(&out_dir)
        .arg("--compile-check")
        .arg("--json")
        .output()
        .expect("spawn provekit emit");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "provekit emit failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    let receipt: Value = serde_json::from_str(&stdout).expect("emit stdout is JSON");
    assert_eq!(receipt["ok"], true, "receipt: {receipt}");
    assert_eq!(receipt["targetLanguage"], "go", "receipt: {receipt}");
    assert_eq!(receipt["targetFramework"], "testify", "receipt: {receipt}");
    assert_eq!(receipt["isComplete"], true, "receipt: {receipt}");
    assert!(
        receipt["emittedArtifactCid"]
            .as_str()
            .unwrap_or("")
            .starts_with("blake3-512:"),
        "receipt: {receipt}"
    );

    let emitted_path = out_dir.join("provekit_emitted_test.go");
    let emitted = fs::read_to_string(&emitted_path)
        .unwrap_or_else(|_| panic!("read emitted {}", emitted_path.display()));
    assert!(emitted.contains("package sample"), "emitted:\n{emitted}");
    assert!(
        emitted.contains("\"testing\""),
        "emitted must import testing:\n{emitted}"
    );
    assert!(
        emitted.contains("\"github.com/stretchr/testify/assert\""),
        "testify emitter must import assert:\n{emitted}"
    );
    assert!(
        emitted.contains("assert.Equal(t, x, x)"),
        "testify emitter must render assert.Equal:\n{emitted}"
    );
}
