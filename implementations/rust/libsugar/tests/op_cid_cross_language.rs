// SPDX-License-Identifier: Apache-2.0

use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};

use libsugar::canonical::op_cid_from_shape;
use serde_json::{json, Value};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("..")
}

fn python_op_cid_from_shape(shape: &Value) -> String {
    let root = repo_root();
    let py_path = [
        root.join("implementations/python/provekit-lift-py-tests/src"),
        root.join("implementations/python/provekit-lift-python-source/src"),
    ]
    .into_iter()
    .map(|path| path.to_string_lossy().into_owned())
    .collect::<Vec<_>>()
    .join(":");

    let mut child = Command::new("python3")
        .env("PYTHONPATH", py_path)
        .arg("-c")
        .arg(
            "import json, sys\n\
             from provekit_lift_py_tests.op_cid import op_cid_from_shape\n\
             print(op_cid_from_shape(json.loads(sys.stdin.read())))\n",
        )
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn python3 for op_cid agreement");

    {
        let stdin = child.stdin.as_mut().expect("python stdin");
        stdin
            .write_all(shape.to_string().as_bytes())
            .expect("write shape JSON to python");
    }

    let output = child.wait_with_output().expect("wait for python op_cid");
    assert!(
        output.status.success(),
        "python op_cid helper failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout)
        .expect("python stdout utf8")
        .trim()
        .to_string()
}

#[test]
fn op_cid_from_shape_is_byte_identical_in_rust_and_python_for_local_operator() {
    let shape = json!({
        "kind": "local-operator",
        "name": "concept:add"
    });

    let rust = op_cid_from_shape(&shape).expect("rust op_cid");
    let python = python_op_cid_from_shape(&shape);

    assert_eq!(rust, python);
}

#[test]
fn op_cid_from_shape_is_byte_identical_in_rust_and_python_for_grammar_operator() {
    let sort = |name: &str| json!({ "kind": "ctor", "name": name, "args": [] });
    let shape = json!({
        "kind": "grammar-op",
        "formalSorts": [sort("Stmt"), sort("Stmt")],
        "returnSort": sort("Stmt"),
        "pre": { "kind": "atomic", "name": "true", "args": [] },
        "post": {
            "arity": ["Stmt", "Stmt"],
            "result": "Stmt",
            "wpRule": {
                "kind": "apply",
                "fn": "wp_slot_0",
                "args": [{
                    "kind": "apply",
                    "fn": "wp_slot_1",
                    "args": [{ "kind": "var", "name": "Q" }]
                }]
            }
        },
        "effects": [{ "kind": "effect-polymorphic", "rule": "union(slot_0.effects, slot_1.effects)" }]
    });

    let rust = op_cid_from_shape(&shape).expect("rust op_cid");
    let python = python_op_cid_from_shape(&shape);
    let registry = libsugar::core::grammar_op_cid("concept:seq")
        .expect("concept:seq grammar op")
        .to_string();

    assert_eq!(rust, python);
    assert_eq!(rust, registry);
}
