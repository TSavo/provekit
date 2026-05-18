// SPDX-License-Identifier: Apache-2.0

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use libprovekit::core::{named_term_document_from_bind_payload, Term};

fn provekit_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_provekit"))
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

fn manifest_command(path: &Path) -> String {
    path.display()
        .to_string()
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
}

fn install_fixture_project(root: &Path) {
    fs::create_dir_all(root.join("src")).expect("create src dir");
    fs::write(
        root.join("src/lib.rs"),
        "pub fn add_one(x: i64) -> i64 {\n    x + 1\n}\n",
    )
    .expect("write source");

    let lift = root.join("lift-rust.py");
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
        result = {
            "name": "test-rust-lift",
            "protocol_version": "pep/1.7.0",
            "capabilities": {"surfaces": ["rust"]},
        }
    elif method == "lift":
        result = {
            "kind": "ir-document",
            "diagnostics": [],
            "ir": [{
                "kind": "bind-lift-entry",
                "file": "src/lib.rs",
                "fn_name": "add_one",
                "fn_line": 1,
                "concept_annotation": "add-one",
                "param_names": ["x"],
                "param_types": ["i64"],
                "return_type": "i64",
                "term_shape": {"kind": "call", "op": "add-one"},
                "term_shape_cid": "blake3-512:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                "witnesses": [{
                    "role": "post",
                    "predicate_text": "out == x + 1",
                    "source_kind": "annotation",
                    "line": 1,
                    "col": 0
                }]
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

    let lower = root.join("lower-python.py");
    write_script(
        &lower,
        r##"#!/usr/bin/env python3
import json
import sys

for line in sys.stdin:
    request = json.loads(line)
    request_id = request.get("id")
    params = request.get("params", {})
    function = params.get("function", "unknown")
    args = ", ".join(params.get("params", []))
    source = f"# concept: {params.get('concept_name', '')}\ndef {function}({args}):\n    raise NotImplementedError(\"provekit test lower\")\n"
    print(json.dumps({
        "jsonrpc": "2.0",
        "id": request_id,
        "result": {
            "source": source,
            "is_stub": True,
            "extension": "py",
            "observed_loss_record": {},
            "used_sugars": []
        }
    }), flush=True)
"##,
    );

    let lift_manifest = root.join(".provekit/lift/rust");
    fs::create_dir_all(&lift_manifest).expect("create lift manifest dir");
    fs::write(
        lift_manifest.join("manifest.toml"),
        format!(
            "name = \"test-rust-lift\"\ncommand = [\"python3\", \"{}\"]\nworking_dir = \".\"\n",
            manifest_command(&lift)
        ),
    )
    .expect("write lift manifest");

    fs::write(
        root.join(".provekit/config.toml"),
        "[authoring.lift]\nsurface = \"rust\"\n",
    )
    .expect("write config");

    let lower_manifest = root.join(".provekit/realize/python");
    fs::create_dir_all(&lower_manifest).expect("create lower manifest dir");
    fs::write(
        lower_manifest.join("manifest.toml"),
        format!(
            "name = \"test-python-lower\"\ncommand = [\"python3\", \"{}\"]\nlibrary_tag = \"default\"\nworking_dir = \".\"\n",
            manifest_command(&lower)
        ),
    )
    .expect("write lower manifest");
}

fn install_name_lifecycle_project(root: &Path) {
    fs::create_dir_all(root.join("src")).expect("create src dir");
    fs::write(
        root.join("src/lib.rs"),
        "pub fn add_one(x: i64) -> i64 {\n    x + 1\n}\n",
    )
    .expect("write source");

    let lift = root.join("lift-rust-lifecycle.py");
    write_script(
        &lift,
        r##"#!/usr/bin/env python3
import json
import pathlib
import sys

ROOT = pathlib.Path(__file__).parent
SOURCE = ROOT / "src" / "lib.rs"

def concept_name():
    concept = "add-one"
    lines = SOURCE.read_text().splitlines()
    for idx, line in enumerate(lines):
        if "pub fn add_one" not in line:
            continue
        cursor = idx - 1
        while cursor >= 0:
            stripped = lines[cursor].strip()
            if not stripped:
                break
            if stripped.startswith("#["):
                cursor -= 1
                continue
            if stripped.startswith("//"):
                body = stripped[2:].strip()
                if body.startswith("concept:"):
                    raw = body[len("concept:"):].strip().split()
                    if raw:
                        name = raw[0]
                        if name.startswith("concept:"):
                            name = name[len("concept:"):]
                        if name:
                            concept = name
                    break
                cursor -= 1
                continue
            break
    return concept

for line in sys.stdin:
    request = json.loads(line)
    method = request.get("method")
    request_id = request.get("id")
    if method == "initialize":
        result = {
            "name": "test-rust-lift-lifecycle",
            "protocol_version": "pep/1.7.0",
            "capabilities": {"surfaces": ["rust"]},
        }
    elif method == "lift":
        result = {
            "kind": "ir-document",
            "diagnostics": [],
            "ir": [{
                "kind": "bind-lift-entry",
                "file": "src/lib.rs",
                "fn_name": "add_one",
                "fn_line": 1,
                "concept_annotation": concept_name(),
                "param_names": ["x"],
                "param_types": ["i64"],
                "return_type": "i64",
                "term_shape": {"kind": "call", "op": "add-one"},
                "term_shape_cid": "blake3-512:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
                "witnesses": []
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

    let lower = root.join("lower-rust-lifecycle.py");
    write_script(
        &lower,
        r##"#!/usr/bin/env python3
import json
import sys

for line in sys.stdin:
    request = json.loads(line)
    request_id = request.get("id")
    params = request.get("params", {})
    function = params.get("function", "add_one")
    concept = params.get("concept_name", params.get("conceptName", ""))
    source = f"// concept: {concept}\npub fn {function}(x: i64) -> i64 {{\n    x + 1\n}}\n"
    print(json.dumps({
        "jsonrpc": "2.0",
        "id": request_id,
        "result": {
            "source": source,
            "is_stub": False,
            "extension": "rs",
            "observed_loss_record": {},
            "used_sugars": []
        }
    }), flush=True)
"##,
    );

    let lift_manifest = root.join(".provekit/lift/rust");
    fs::create_dir_all(&lift_manifest).expect("create lift manifest dir");
    fs::write(
        lift_manifest.join("manifest.toml"),
        format!(
            "name = \"test-rust-lift-lifecycle\"\ncommand = [\"python3\", \"{}\"]\nworking_dir = \".\"\n",
            manifest_command(&lift)
        ),
    )
    .expect("write lift manifest");

    fs::write(
        root.join(".provekit/config.toml"),
        "[authoring.lift]\nsurface = \"rust\"\n",
    )
    .expect("write config");

    let lower_manifest = root.join(".provekit/realize/rust");
    fs::create_dir_all(&lower_manifest).expect("create lower manifest dir");
    fs::write(
        lower_manifest.join("manifest.toml"),
        format!(
            "name = \"test-rust-lower-lifecycle\"\ncommand = [\"python3\", \"{}\"]\nlibrary_tag = \"default\"\nworking_dir = \".\"\n",
            manifest_command(&lower)
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

fn run_provekit_stdin(label: &str, args: &[&str], input: &[u8]) -> Vec<u8> {
    let mut child = Command::new(provekit_bin())
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap_or_else(|error| panic!("spawn {label}: {error}"));
    child
        .stdin
        .take()
        .expect("stdin")
        .write_all(input)
        .unwrap_or_else(|error| panic!("write {label} stdin: {error}"));
    let output = child
        .wait_with_output()
        .unwrap_or_else(|error| panic!("wait {label}: {error}"));
    assert_success(label, &output);
    output.stdout
}

#[test]
fn lift_bind_lower_pipe_and_file_forms_emit_byte_equivalent_python() {
    let temp = tempfile::tempdir().expect("tempdir");
    let project = temp.path().join("project");
    install_fixture_project(&project);

    let lift_pipe = Command::new(provekit_bin())
        .arg("lift")
        .arg(&project)
        .output()
        .expect("spawn lift pipe");
    assert_success("pipe lift", &lift_pipe);

    let mut bind_pipe = Command::new(provekit_bin())
        .arg("bind")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn bind pipe");
    bind_pipe
        .stdin
        .take()
        .expect("bind stdin")
        .write_all(&lift_pipe.stdout)
        .expect("write bind stdin");
    let bind_pipe = bind_pipe.wait_with_output().expect("wait bind pipe");
    assert_success("pipe bind", &bind_pipe);

    let mut lower_pipe = Command::new(provekit_bin())
        .arg("lower")
        .arg("--target")
        .arg("python")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn lower pipe");
    lower_pipe
        .stdin
        .take()
        .expect("lower stdin")
        .write_all(&bind_pipe.stdout)
        .expect("write lower stdin");
    let lower_pipe = lower_pipe.wait_with_output().expect("wait lower pipe");
    assert_success("pipe lower", &lower_pipe);

    let term = temp.path().join("term.json");
    let named = temp.path().join("named.json");
    let canonical_file = temp.path().join("canonical_file.py");

    let lift_file = Command::new(provekit_bin())
        .arg("lift")
        .arg(&project)
        .arg("-o")
        .arg(&term)
        .output()
        .expect("spawn lift file");
    assert_success("file lift", &lift_file);

    let bind_file = Command::new(provekit_bin())
        .arg("bind")
        .arg(&term)
        .arg("-o")
        .arg(&named)
        .output()
        .expect("spawn bind file");
    assert_success("file bind", &bind_file);

    let lower_file = Command::new(provekit_bin())
        .arg("lower")
        .arg("--target")
        .arg("python")
        .arg(&named)
        .arg("-o")
        .arg(&canonical_file)
        .output()
        .expect("spawn lower file");
    assert_success("file lower", &lower_file);

    let file_bytes = fs::read(&canonical_file).expect("read file lower output");
    assert_eq!(lower_pipe.stdout, file_bytes);
    assert_eq!(
        String::from_utf8_lossy(&file_bytes),
        "# concept: concept:add-one\ndef add_one(x):\n    raise NotImplementedError(\"provekit test lower\")\n"
    );
}

#[test]
fn lift_bind_edit_comment_relift_bind_promotes_user_concept_name() {
    let temp = tempfile::tempdir().expect("tempdir");
    let project = temp.path().join("project");
    install_name_lifecycle_project(&project);

    let lift_initial = Command::new(provekit_bin())
        .arg("lift")
        .arg(&project)
        .output()
        .expect("spawn initial lift");
    assert_success("initial lift", &lift_initial);
    let bind_initial = run_provekit_stdin("initial bind", &["bind"], &lift_initial.stdout);
    let lower_initial = run_provekit_stdin(
        "initial lower",
        &["lower", "--target", "rust"],
        &bind_initial,
    );

    let bound_source = String::from_utf8(lower_initial).expect("lowered source utf8");
    assert!(
        bound_source.contains("// concept: concept:add-one"),
        "initial lower must expose editable concept comment: {bound_source}"
    );
    let edited_source = bound_source.replace("// concept: concept:add-one", "// concept: my-thing");
    fs::write(project.join("src/lib.rs"), edited_source).expect("write edited source");

    let lift_edited = Command::new(provekit_bin())
        .arg("lift")
        .arg(&project)
        .output()
        .expect("spawn edited lift");
    assert_success("edited lift", &lift_edited);
    let bind_edited = run_provekit_stdin("edited bind", &["bind"], &lift_edited.stdout);

    let payload: Term = serde_json::from_slice(&bind_edited).expect("bind payload parses");
    let named = named_term_document_from_bind_payload(&payload)
        .expect("bind payload recovers named term document");
    assert_eq!(named.terms[0].concept_name, "concept:my-thing");
}
