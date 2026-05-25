// SPDX-License-Identifier: Apache-2.0

use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};

fn provekit_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_provekit"))
}

const CHECKED_CONTRADICTION: &[u8] = include_bytes!("fixtures/checked_contradiction.json");

#[test]
fn prove_formula_from_stdin_emits_direct_smt_lib_assertion() {
    let mut child = Command::new(provekit_bin())
        .arg("prove")
        .arg("--target")
        .arg("smt-lib")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn provekit prove --target smt-lib");

    child
        .stdin
        .take()
        .expect("stdin")
        .write_all(CHECKED_CONTRADICTION)
        .expect("write formula");

    let output = child.wait_with_output().expect("wait");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "prove failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(stdout.contains("(set-logic ALL)"), "stdout:\n{stdout}");
    assert!(
        stdout.contains("(declare-fun checked (Int) Int)"),
        "stdout:\n{stdout}"
    );
    assert!(stdout.contains("(assert (and "), "stdout:\n{stdout}");
    assert!(stdout.contains("(= (checked 42) 42)"), "stdout:\n{stdout}");
    assert!(
        stdout.contains("(distinct (checked 42) 42)"),
        "stdout:\n{stdout}"
    );
    assert!(stdout.contains("(check-sat)"), "stdout:\n{stdout}");
    assert!(
        !stdout.contains("(assert (not "),
        "lower should emit a direct assertion, not a proof-obligation negation\nstdout:\n{stdout}"
    );
}

#[test]
fn prove_formula_from_stdin_emits_coq() {
    let mut child = Command::new(provekit_bin())
        .arg("prove")
        .arg("--target")
        .arg("coq")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn provekit prove --target coq");

    child
        .stdin
        .take()
        .expect("stdin")
        .write_all(CHECKED_CONTRADICTION)
        .expect("write formula");

    let output = child.wait_with_output().expect("wait");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "prove failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        stdout.contains("Require Import ZArith"),
        "stdout:\n{stdout}"
    );
    assert!(stdout.contains("Goal"), "stdout:\n{stdout}");
}

#[test]
fn lower_verb_is_retired() {
    let output = Command::new(provekit_bin())
        .arg("lower")
        .arg("--target")
        .arg("smt-lib")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("spawn retired lower verb");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !output.status.success(),
        "retired lower verb must be rejected"
    );
    assert!(
        stderr.contains("unrecognized subcommand 'lower'"),
        "stderr should report retired lower verb\n{stderr}"
    );
}
