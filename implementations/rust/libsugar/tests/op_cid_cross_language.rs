// SPDX-License-Identifier: Apache-2.0

use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};

use libsugar::canonical::{local_op_cid, local_operator_shape, op_cid_from_shape};
use serde_json::{json, Value};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("..")
}

fn run_python_op_cid_helper(script: &str, input: &Value) -> String {
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
        .arg(script)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn python3 for op_cid agreement");

    {
        let stdin = child.stdin.as_mut().expect("python stdin");
        stdin
            .write_all(input.to_string().as_bytes())
            .expect("write JSON to python");
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

fn python_op_cid_from_shape(shape: &Value) -> String {
    run_python_op_cid_helper(
        "import json, sys\n\
         from provekit_lift_py_tests.op_cid import op_cid_from_shape\n\
         print(op_cid_from_shape(json.loads(sys.stdin.read())))\n",
        shape,
    )
}

fn python_local_operator_shape(name: &str) -> Value {
    let output = run_python_op_cid_helper(
        "import json, sys\n\
         from provekit_lift_py_tests.op_cid import local_operator_shape\n\
         params = json.loads(sys.stdin.read())\n\
         print(json.dumps(local_operator_shape(params['name']), separators=(',', ':'), sort_keys=True))\n",
        &json!({ "name": name }),
    );
    serde_json::from_str(&output).expect("python local_operator_shape JSON")
}

fn python_local_op_cid(name: &str) -> String {
    run_python_op_cid_helper(
        "import json, sys\n\
         from provekit_lift_py_tests.op_cid import local_op_cid\n\
         params = json.loads(sys.stdin.read())\n\
         print(local_op_cid(params['name']))\n",
        &json!({ "name": name }),
    )
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
fn local_operator_shape_strips_concept_prefix_before_op_cid() {
    let bare_shape = json!({
        "kind": "local-operator",
        "name": "add"
    });

    let rust_shape = local_operator_shape("concept:add");
    let python_shape = python_local_operator_shape("concept:add");
    let rust = local_op_cid("concept:add").expect("rust bare local op_cid");
    let python = python_local_op_cid("concept:add");

    assert_eq!(rust_shape, bare_shape);
    assert_eq!(python_shape, bare_shape);
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
