use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn provekit_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_provekit"))
}

fn write_fake_recognizer_project(project: &Path) -> PathBuf {
    let src_dir = project.join("src");
    fs::create_dir_all(&src_dir).expect("create src dir");
    fs::write(src_dir.join("lib.rs"), "fn value() -> i32 { 1 }\n").expect("write source");

    let manifest_dir = project.join(".provekit/lift/no-proof-reader");
    fs::create_dir_all(&manifest_dir).expect("create manifest dir");

    let plugin = project.join("fake_recognizer.py");
    fs::write(
        &plugin,
        r#"import json
import os
import pathlib
import sys

line = sys.stdin.readline()
request = json.loads(line)
pathlib.Path(os.environ["PROVEKIT_CAPTURE_REQUEST"]).write_text(
    json.dumps(request, sort_keys=True),
    encoding="utf-8",
)
print(json.dumps({
    "jsonrpc": "2.0",
    "id": request.get("id"),
    "result": {"tags": []},
}), flush=True)
"#,
    )
    .expect("write fake recognizer");

    let captured_request = project.join("captured_request.json");
    fs::write(
        manifest_dir.join("manifest.toml"),
        format!(
            "name = \"no-proof-reader\"\ncommand = [\"env\", \"PROVEKIT_CAPTURE_REQUEST={}\", \"python3\", \"{}\"]\nworking_dir = \".\"\n",
            captured_request.display(),
            plugin.display()
        ),
    )
    .expect("write manifest");

    captured_request
}

#[test]
fn recognize_rejects_cli_binding_proof_paths() {
    let project = tempfile::tempdir().expect("tempdir");
    let binding = project.path().join("bindings.proof");
    fs::write(&binding, r#"{"members":[]}"#).expect("write proof");
    let _captured_request = write_fake_recognizer_project(project.path());

    let output = Command::new(provekit_bin())
        .arg("recognize")
        .arg("--project")
        .arg(project.path())
        .arg("--surface")
        .arg("no-proof-reader")
        .arg("--source")
        .arg("src/lib.rs")
        .arg("--binding")
        .arg(&binding)
        .arg("--json")
        .output()
        .expect("spawn provekit recognize");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !output.status.success(),
        "recognize must reject CLI proof-path bindings; stderr:\n{stderr}"
    );
    assert!(
        stderr.contains("--binding")
            && (stderr.contains("unexpected") || stderr.contains("unrecognized")),
        "recognize should reject --binding at the CLI boundary, got stderr:\n{stderr}"
    );
}

#[test]
fn recognize_rpc_request_does_not_include_cli_binding_templates() {
    let project = tempfile::tempdir().expect("tempdir");
    let captured_request = write_fake_recognizer_project(project.path());

    let output = Command::new(provekit_bin())
        .arg("recognize")
        .arg("--project")
        .arg(project.path())
        .arg("--surface")
        .arg("no-proof-reader")
        .arg("--source")
        .arg("src/lib.rs")
        .arg("--json")
        .output()
        .expect("spawn provekit recognize");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "recognize failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    let request_text = fs::read_to_string(&captured_request).expect("read captured request");
    let request: serde_json::Value =
        serde_json::from_str(&request_text).expect("captured request parses");
    let params = request
        .get("params")
        .and_then(|value| value.as_object())
        .expect("recognize params object");
    assert!(
        !params.contains_key("binding_templates"),
        "CLI must not manufacture recognizer binding_templates; kits own proof/template resolution over RPC: {request}"
    );
}
