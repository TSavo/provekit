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

fn scala_cli_available() -> bool {
    let mut command = Command::new("scala-cli");
    command.arg("version");
    if Path::new("/usr/local/opt/openjdk").exists() {
        command.env("JAVA_HOME", "/usr/local/opt/openjdk");
    }
    command
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

fn scala_env(command: &mut Command) {
    if Path::new("/usr/local/opt/openjdk").exists() {
        command.env("JAVA_HOME", "/usr/local/opt/openjdk");
        let path = std::env::var("PATH").unwrap_or_default();
        command.env("PATH", format!("/usr/local/opt/openjdk/bin:{path}"));
    }
}

fn install_emit_registration(project: &Path, emitter_dir: &Path) {
    let provekit_dir = project.join(".provekit");
    fs::create_dir_all(&provekit_dir).expect("mkdir .provekit");
    fs::write(
        provekit_dir.join("config.toml"),
        "exam_manifest_cid = \"blake3-512:00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000\"\n\
         \n\
         [[plugins]]\n\
         name = \"scala-scalatest\"\n\
         surface = \"scala-scalatest\"\n\
         emit = \"scalatest\"\n",
    )
    .expect("write project config");

    let manifest = project
        .join(".provekit")
        .join("emit")
        .join("scala-scalatest")
        .join("manifest.toml");
    fs::create_dir_all(manifest.parent().unwrap()).expect("mkdir manifest");
    fs::write(
        manifest,
        format!(
            "name = \"scala-scalatest\"\ncommand = [\"scala-cli\", \"run\", \"{}\", \"--server=false\", \"--\", \"--rpc\"]\nworking_dir = \".\"\nprotocol_versions = [\"pep/1.7.0\"]\n",
            emitter_dir
                .display()
                .to_string()
                .replace('\\', "\\\\")
                .replace('"', "\\\"")
        ),
    )
    .expect("write emit manifest");
}

#[test]
fn emit_scala_scalatest_dispatches_real_emitter_and_scala_cli_checks_output() {
    if !scala_cli_available() {
        eprintln!("skipping: scala-cli is unavailable");
        return;
    }

    let temp = tempfile::tempdir().expect("tempdir");
    let project = temp.path().join("project");
    let out_dir = temp.path().join("scala-project");
    fs::create_dir_all(&project).expect("mkdir project");
    fs::create_dir_all(&out_dir).expect("mkdir out");

    let emitter_dir = repo_root()
        .join("implementations")
        .join("scala")
        .join("provekit-emit-scala-scalatest");
    install_emit_registration(&project, &emitter_dir);

    let plan = project.join("plan.json");
    fs::write(
        &plan,
        serde_json::to_vec_pretty(&serde_json::json!({
            "contract_id": "concept:eq",
            "function": "identity",
            "params": ["x"],
            "param_types": ["Int"],
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

    let mut command = Command::new(provekit_bin());
    scala_env(&mut command);
    let output = command
        .arg("emit")
        .arg("--project")
        .arg(&project)
        .arg("--target")
        .arg("scala")
        .arg("--framework")
        .arg("scalatest")
        .arg("--plan")
        .arg(&plan)
        .arg("--out-dir")
        .arg(&out_dir)
        .arg("--compile-check")
        .arg("--json")
        .output()
        .expect("spawn provekit emit scala");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "provekit emit scala failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    let receipt: Value = serde_json::from_str(&stdout).expect("emit stdout is JSON");
    assert_eq!(receipt["ok"], true, "receipt: {receipt}");
    assert_eq!(receipt["targetLanguage"], "scala", "receipt: {receipt}");
    assert_eq!(
        receipt["targetFramework"], "scalatest",
        "receipt: {receipt}"
    );
    assert_eq!(receipt["isComplete"], true, "receipt: {receipt}");
    assert!(
        receipt["emittedArtifactCid"]
            .as_str()
            .unwrap_or("")
            .starts_with("blake3-512:"),
        "receipt: {receipt}"
    );

    let emitted_path = out_dir.join("src/test/scala/ProvekitEmittedSuite.scala");
    let emitted = fs::read_to_string(&emitted_path)
        .unwrap_or_else(|_| panic!("read emitted {}", emitted_path.display()));
    assert!(
        emitted.contains("import org.scalatest.funsuite.AnyFunSuite"),
        "emitted:\n{emitted}"
    );
    assert!(
        emitted.contains("assertResult(2)(2)"),
        "emitted:\n{emitted}"
    );
}
