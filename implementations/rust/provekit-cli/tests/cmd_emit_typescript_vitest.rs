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

fn node_available() -> bool {
    Command::new("node")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn typescript_vitest_emitter_available() -> bool {
    let emitter = repo_root()
        .join("implementations")
        .join("typescript")
        .join("provekit-emit-typescript-vitest");
    Command::new("node")
        .current_dir(&emitter)
        .args([
            "-e",
            "require('./src/emitter'); process.exit(require('fs').existsSync('./node_modules/vitest/vitest.mjs') ? 0 : 1);",
        ])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn install_emit_registration(project: &Path) {
    let emitter_main = repo_root()
        .join("implementations")
        .join("typescript")
        .join("provekit-emit-typescript-vitest")
        .join("src")
        .join("main.js");

    let provekit_dir = project.join(".provekit");
    fs::create_dir_all(&provekit_dir).expect("mkdir .provekit");
    fs::write(
        provekit_dir.join("config.toml"),
        "exam_manifest_cid = \"blake3-512:00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000\"\n\
         \n\
         [[plugins]]\n\
         name = \"typescript-vitest\"\n\
         surface = \"typescript-vitest\"\n\
         emit = \"vitest\"\n",
    )
    .expect("write project config");

    let manifest = project
        .join(".provekit")
        .join("emit")
        .join("typescript-vitest")
        .join("manifest.toml");
    fs::create_dir_all(manifest.parent().unwrap()).expect("mkdir manifest");
    fs::write(
        manifest,
        format!(
            "name = \"typescript-vitest\"\ncommand = [\"node\", \"{}\", \"--rpc\"]\nworking_dir = \".\"\nprotocol_versions = [\"pep/1.7.0\"]\n",
            emitter_main
                .display()
                .to_string()
                .replace('\\', "\\\\")
                .replace('"', "\\\"")
        ),
    )
    .expect("write emit manifest");
}

#[test]
fn emit_typescript_vitest_dispatches_real_emitter_and_vitest_checks_output() {
    if !node_available() {
        eprintln!("skipping: node not on PATH");
        return;
    }
    if !typescript_vitest_emitter_available() {
        eprintln!(
            "skipping: TypeScript Vitest emitter deps unavailable; run `make build-ts` first"
        );
        return;
    }

    let temp = tempfile::tempdir().expect("tempdir");
    let project = temp.path().join("project");
    let out_dir = temp.path().join("out");
    fs::create_dir_all(&project).expect("mkdir project");
    fs::create_dir_all(&out_dir).expect("mkdir out");
    install_emit_registration(&project);

    let plan = project.join("plan.json");
    fs::write(
        &plan,
        serde_json::to_vec_pretty(&serde_json::json!({
            "contract_id": "concept:eq",
            "function": "identity",
            "params": ["x"],
            "param_types": ["number"],
            "return_type": "number",
            "predicates": [{
                "kind": "atomic",
                "name": "concept:eq",
                "args": [
                    {"kind": "const", "value": 2, "sort": {"kind": "primitive", "name": "Int"}},
                    {"kind": "const", "value": 2, "sort": {"kind": "primitive", "name": "Int"}}
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
        .arg("typescript")
        .arg("--framework")
        .arg("vitest")
        .arg("--plan")
        .arg(&plan)
        .arg("--out-dir")
        .arg(&out_dir)
        .arg("--compile-check")
        .arg("--json")
        .output()
        .expect("spawn provekit emit typescript");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "provekit emit typescript failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    let receipt: Value = serde_json::from_str(&stdout).expect("emit stdout is JSON");
    assert_eq!(receipt["ok"], true, "receipt: {receipt}");
    assert_eq!(
        receipt["targetLanguage"], "typescript",
        "receipt: {receipt}"
    );
    assert_eq!(receipt["targetFramework"], "vitest", "receipt: {receipt}");
    assert_eq!(receipt["isComplete"], true, "receipt: {receipt}");
    assert_eq!(receipt["compileCheck"]["ok"], true, "receipt: {receipt}");
    assert!(
        receipt["emittedArtifactCid"]
            .as_str()
            .unwrap_or("")
            .starts_with("blake3-512:"),
        "receipt: {receipt}"
    );

    let emitted_path = out_dir.join("provekit_identity.test.ts");
    let emitted = fs::read_to_string(&emitted_path)
        .unwrap_or_else(|_| panic!("read emitted {}", emitted_path.display()));
    assert!(
        emitted.contains("describe(\"provekit contract identity\""),
        "emitted:\n{emitted}"
    );
    assert!(
        emitted.contains("expect(2).toEqual(2);"),
        "emitted:\n{emitted}"
    );
}
