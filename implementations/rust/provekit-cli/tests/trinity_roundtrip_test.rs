// SPDX-License-Identifier: Apache-2.0

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

fn provekit_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_provekit"))
}

fn trinity_fixture_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("trinity_roundtrip")
}

fn copy_dir(src: &Path, dst: &Path) {
    let _ = fs::create_dir_all(dst);
    let Ok(entries) = fs::read_dir(src) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let dest = dst.join(entry.file_name());
        if path.is_dir() {
            copy_dir(&path, &dest);
        } else {
            let _ = fs::copy(&path, &dest);
        }
    }
}

fn write_script(path: &Path, contents: &str) {
    fs::write(path, contents).expect("write script");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(path).expect("metadata").permissions();
        perms.set_mode(0o755);
        fs::set_permissions(path, perms).expect("chmod script");
    }
}

fn command_arg(path: &Path) -> String {
    path.display()
        .to_string()
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
}

fn install_pipeline_plugins(root: &Path) {
    let lift = root.join("trinity-lift.py");
    write_script(
        &lift,
        r##"#!/usr/bin/env python3
import json
import sys

for line in sys.stdin:
    request = json.loads(line)
    method = request.get("method")
    request_id = request.get("id")
    if method == "initialize":
        result = {"name": "trinity-lift", "protocol_version": "pep/1.7.0", "capabilities": {"surfaces": ["rust"]}}
    elif method == "lift":
        result = {
            "kind": "ir-document",
            "diagnostics": [],
            "ir": [{
                "kind": "bind-lift-entry",
                "file": "src/lib.rs",
                "fn_name": "fetch_user",
                "fn_line": 1,
                "concept_annotation": "http-request",
                "param_names": ["id"],
                "param_types": ["i64"],
                "return_type": "String",
                "term_shape": {"kind": "body", "stmts": [{"kind": "opaque"}]},
                "term_shape_cid": "blake3-512:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
                "witnesses": [{"role": "post", "predicate_text": "out != \"\"", "source_kind": "annotation"}]
            }]
        }
    elif method == "shutdown":
        result = {}
    else:
        print(json.dumps({"jsonrpc": "2.0", "id": request_id, "error": {"message": "unknown method"}}), flush=True)
        continue
    print(json.dumps({"jsonrpc": "2.0", "id": request_id, "result": result}), flush=True)
    if method == "shutdown":
        break
"##,
    );
    let lower = root.join("trinity-lower-python.py");
    write_script(
        &lower,
        r##"#!/usr/bin/env python3
import json
import sys

for line in sys.stdin:
    request = json.loads(line)
    request_id = request.get("id")
    method = request.get("method")
    if method != "provekit.plugin.invoke":
        print(json.dumps({
            "jsonrpc": "2.0",
            "id": request_id,
            "error": {"code": -32601, "message": f"METHOD_NOT_FOUND: {method}"}
        }), flush=True)
        continue
    params = request.get("params", {})
    source = f"# concept: {params.get('concept_name', '')}\ndef {params.get('function', 'f')}({', '.join(params.get('params', []))}):\n    raise NotImplementedError(\"trinity lower\")\n"
    print(json.dumps({"jsonrpc": "2.0", "id": request_id, "result": {"source": source, "is_stub": True, "extension": "py", "observed_loss_record": {}, "used_sugars": []}}), flush=True)
"##,
    );

    let lift_manifest = root.join(".provekit/lift/rust");
    fs::create_dir_all(&lift_manifest).expect("create lift manifest");
    fs::write(
        lift_manifest.join("manifest.toml"),
        format!(
            "name = \"trinity-lift\"\ncommand = [\"python3\", \"{}\"]\nworking_dir = \".\"\n",
            command_arg(&lift)
        ),
    )
    .expect("write lift manifest");
    fs::write(
        root.join(".provekit/config.toml"),
        "[authoring.lift]\nsurface = \"rust\"\n",
    )
    .expect("write config");

    let lower_manifest = root.join(".provekit/realize/python");
    fs::create_dir_all(&lower_manifest).expect("create lower manifest");
    fs::write(
        lower_manifest.join("manifest.toml"),
        format!(
            "name = \"trinity-lower-python\"\ncommand = [\"python3\", \"{}\"]\nlibrary_tag = \"default\"\nworking_dir = \".\"\n",
            command_arg(&lower)
        ),
    )
    .expect("write lower manifest");
}

fn assert_success(label: &str, output: &std::process::Output) {
    assert!(
        output.status.success(),
        "{label} failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn trinity_fixture_uses_lift_bind_lower_pipeline_shape() {
    let temp = tempfile::tempdir().expect("tempdir");
    let project = temp.path().join("trinity");
    copy_dir(&trinity_fixture_root(), &project);
    install_pipeline_plugins(&project);

    let lift = Command::new(provekit_bin())
        .arg("lift")
        .arg(&project)
        .output()
        .expect("spawn lift");
    assert_success("lift", &lift);

    let mut bind = Command::new(provekit_bin())
        .arg("bind")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn bind");
    bind.stdin
        .take()
        .expect("bind stdin")
        .write_all(&lift.stdout)
        .expect("write bind stdin");
    let bind = bind.wait_with_output().expect("wait bind");
    assert_success("bind", &bind);

    let mut lower = Command::new(provekit_bin())
        .arg("lower")
        .arg("--target")
        .arg("python")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn lower");
    lower
        .stdin
        .take()
        .expect("lower stdin")
        .write_all(&bind.stdout)
        .expect("write lower stdin");
    let lower = lower.wait_with_output().expect("wait lower");
    assert_success("lower", &lower);

    let py = String::from_utf8(lower.stdout).expect("python utf8");
    assert!(py.contains("# concept: concept:http-request"), "{py}");
    assert!(py.contains("def fetch_user(id):"), "{py}");
}
