// SPDX-License-Identifier: Apache-2.0

use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};

use libprovekit::core::NamedTermDocument;

fn provekit_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_provekit"))
}

fn term_document() -> &'static [u8] {
    br#"{
  "kind": "ir-document",
  "sourceLanguage": "rust",
  "workspaceRoot": "/tmp/provekit-bind-test",
  "ir": [{
    "kind": "bind-lift-entry",
    "file": "src/lib.rs",
    "fn_name": "deposit",
    "fn_line": 14,
    "concept_annotation": "deposit-then-balance",
    "param_names": ["balance", "amount"],
    "param_types": ["i64", "i64"],
    "return_type": "i64",
    "term_shape": {"kind": "body", "stmts": [{"kind": "opaque"}]},
    "term_shape_cid": "blake3-512:11111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111",
    "witnesses": [{
      "role": "post",
      "predicate_text": "out == balance + amount",
      "source_kind": "annotation",
      "line": 14,
      "col": 0
    }]
  }]
}"#
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
fn bind_from_stdin_emits_named_term_document_with_promotion_decisions() {
    let mut child = Command::new(provekit_bin())
        .arg("bind")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn bind");
    child
        .stdin
        .take()
        .expect("stdin")
        .write_all(term_document())
        .expect("write term");
    let output = child.wait_with_output().expect("wait bind");
    assert_success("bind stdin", &output);

    let named: NamedTermDocument =
        serde_json::from_slice(&output.stdout).expect("named term document parses");
    let named = serde_json::to_value(named).expect("named term serializes");
    assert_eq!(named["sourceLanguage"], "rust");
    assert_eq!(
        named["terms"][0]["conceptName"],
        "concept:deposit-then-balance"
    );
    assert_eq!(named["terms"][0]["function"], "deposit");
    assert_eq!(
        named["terms"][0]["witnesses"][0]["predicateText"],
        "out == balance + amount"
    );
    assert_eq!(
        named["promotionDecisionMementos"][0]["header"]["kind"],
        "promotion-decision"
    );
}

#[test]
fn bind_file_and_pipe_forms_are_byte_equivalent() {
    let temp = tempfile::tempdir().expect("tempdir");
    let term = temp.path().join("term.json");
    let named = temp.path().join("named.json");
    fs::write(&term, term_document()).expect("write term");

    let file = Command::new(provekit_bin())
        .arg("bind")
        .arg(&term)
        .arg("-o")
        .arg(&named)
        .output()
        .expect("spawn bind file");
    assert_success("bind file", &file);
    let file_bytes = fs::read(&named).expect("read named file");

    let mut child = Command::new(provekit_bin())
        .arg("bind")
        .arg("-")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn bind pipe");
    child
        .stdin
        .take()
        .expect("stdin")
        .write_all(term_document())
        .expect("write term");
    let pipe = child.wait_with_output().expect("wait bind pipe");
    assert_success("bind pipe", &pipe);

    assert_eq!(pipe.stdout, file_bytes);
}

#[test]
fn bind_does_not_require_or_invoke_language_lower_plugins() {
    let temp = tempfile::tempdir().expect("tempdir");
    let root = temp.path();
    fs::create_dir_all(root.join(".provekit/realize/python")).expect("create realize manifest");
    fs::write(
        root.join(".provekit/realize/python/manifest.toml"),
        "name = \"exploding-python-lower\"\ncommand = [\"false\"]\nlibrary_tag = \"default\"\n",
    )
    .expect("write realize manifest");
    let term = root.join("term.json");
    fs::write(&term, term_document()).expect("write term");

    let output = Command::new(provekit_bin())
        .arg("bind")
        .arg(&term)
        .output()
        .expect("spawn bind");
    assert_success("bind ignores lower plugin", &output);
}
