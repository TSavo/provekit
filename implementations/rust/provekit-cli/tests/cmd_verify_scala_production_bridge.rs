// SPDX-License-Identifier: Apache-2.0
//
// Scala production-bridge gauntlet. The Rust CLI is only the generic
// config/manifest/RPC client here: Scala parsing, ScalaTest assertion
// harvesting, and Scala-term-to-ProofIR normalization live in the Scala kit.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value as Json;
use serial_test::serial;

#[path = "support/contradiction.rs"]
mod contradiction;

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

fn z3_available() -> bool {
    Command::new("z3")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn scala_cli_available() -> bool {
    Command::new("scala-cli")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn unique_dir(suffix: &str) -> PathBuf {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let p = std::env::temp_dir().join(format!("provekit-scala-prod-bridge-{stamp}-{suffix}"));
    fs::create_dir_all(&p).expect("mkdir project");
    p
}

fn toml_path(path: &Path) -> String {
    path.display()
        .to_string()
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
}

fn stage_scala_project(suffix: &str, body_factor: i64) -> PathBuf {
    let root = repo_root();
    let project = unique_dir(suffix);
    fs::create_dir_all(project.join("src")).expect("mkdir src");
    fs::write(
        project.join("src").join("App.scala"),
        format!(
            r#"package app

def double(x: Int): Int = x * {body_factor}
"#
        ),
    )
    .expect("write App.scala");
    fs::write(
        project.join("DoubleSpec.scala"),
        r#"package app

import org.scalatest.funsuite.AnyFunSuite

final class DoubleSpec extends AnyFunSuite {
  test("double three is six") {
    assert(double(3) == 6)
  }
}
"#,
    )
    .expect("write DoubleSpec.scala");

    let provekit = project.join(".provekit");
    fs::create_dir_all(provekit.join("lift").join("scala-source"))
        .expect("mkdir scala-source manifest dir");
    fs::create_dir_all(provekit.join("lift").join("scala-scalatest"))
        .expect("mkdir scala-scalatest manifest dir");
    fs::write(
        provekit.join("config.toml"),
        "[[plugins]]\n\
         name = \"scala-source\"\n\
         surface = \"scala-source\"\n\
         layer = \"verify\"\n\
         \n\
         [[plugins]]\n\
         name = \"scala-scalatest-tests\"\n\
         surface = \"scala-scalatest\"\n\
         layer = \"tests\"\n\
         \n\
         [solvers]\n\
         default = \"z3\"\n\
         \n\
         [solvers.dispatch]\n\
         linear_arithmetic = \"z3\"\n\
         default = \"z3\"\n\
         \n\
         [solvers.z3]\n\
         binary = \"z3\"\n\
         flags = [\"-smt2\", \"-in\"]\n",
    )
    .expect("write config.toml");

    let source_dir = root
        .join("implementations")
        .join("scala")
        .join("provekit-lift-scala-source");
    let command = format!(
        "[\"scala-cli\", \"run\", \"{}\", \"--server=false\", \"--\", \"--rpc\"]",
        toml_path(&source_dir)
    );
    for name in ["scala-source", "scala-scalatest"] {
        fs::write(
            provekit.join("lift").join(name).join("manifest.toml"),
            format!("name = \"{name}\"\ncommand = {command}\nworking_dir = \".\"\n"),
        )
        .unwrap_or_else(|_| panic!("write {name} manifest.toml"));
    }

    project
}

fn run_mint(project: &Path) {
    let out = Command::new(provekit_bin())
        .arg("mint")
        .arg("--project")
        .arg(project)
        .arg("--out")
        .arg(project)
        .arg("--no-attest")
        .arg("--quiet")
        .output()
        .expect("spawn provekit mint");
    assert!(
        out.status.success(),
        "provekit mint must succeed\n  stdout: {}\n  stderr: {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
}

fn run_verify_json_with_code(project: &Path, witness_dir: &Path) -> (Json, i32) {
    let out = Command::new(provekit_bin())
        .arg("verify")
        .arg("--project")
        .arg(project)
        .arg("--emit-witnesses")
        .arg(witness_dir)
        .arg("--json")
        .output()
        .expect("spawn provekit verify");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let receipt = serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("verify JSON parse failed: {e}\nstdout: {stdout}"));
    (receipt, out.status.code().unwrap_or(-1))
}

#[test]
#[serial(scala_cli)]
fn scala_production_path_double_discharges_and_mints_witness() {
    if !scala_cli_available() {
        eprintln!("scala-cli not on PATH: skipping Scala production-bridge positive test");
        return;
    }
    if !z3_available() {
        eprintln!("z3 not on PATH: skipping Scala production-bridge positive test");
        return;
    }
    let project = stage_scala_project("pos", 2);
    run_mint(&project);

    let witnesses = project.join("witnesses-out");
    let (receipt, code) = run_verify_json_with_code(&project, &witnesses);

    assert_eq!(
        receipt["kind"], "verification-receipt",
        "receipt: {receipt}"
    );
    assert_eq!(
        receipt["totalClaims"], 1,
        "exactly one body-bearing Scala callsite must enumerate; receipt: {receipt}"
    );
    let claim = &receipt["claims"].as_array().expect("claims")[0];
    assert_eq!(
        claim["pass"], true,
        "double(3)==6 must discharge through Scala body (* x 2); claim: {claim}"
    );
    assert_eq!(claim["status"], "discharged", "claim: {claim}");
    let solver = claim["dischargingSolver"].as_str().unwrap_or("");
    assert!(
        solver.starts_with("z3@"),
        "discharging solver must be z3; got `{solver}`"
    );
    let witness_cid = claim["witnessCid"].as_str().expect("witness minted");
    assert!(witness_cid.starts_with("blake3-512:"));
    eprintln!("SCALA_PRODUCTION_POSITIVE_WITNESS_CID={witness_cid}");
    assert_eq!(receipt["ok"], true, "receipt: {receipt}");
    assert_eq!(code, 0, "positive run exits clean; got {code}");

    let _ = fs::remove_dir_all(&project);
}

#[test]
#[serial(scala_cli)]
fn scala_production_path_broken_body_fails_unsatisfied_no_witness() {
    if !scala_cli_available() {
        eprintln!("scala-cli not on PATH: skipping Scala production-bridge negative test");
        return;
    }
    if !z3_available() {
        eprintln!("z3 not on PATH: skipping Scala production-bridge negative test");
        return;
    }
    let project = stage_scala_project("neg", 3);
    run_mint(&project);

    let witnesses = project.join("witnesses-out");
    let (receipt, code) = run_verify_json_with_code(&project, &witnesses);

    assert_eq!(receipt["totalClaims"], 1, "receipt: {receipt}");
    let claim = &receipt["claims"].as_array().expect("claims")[0];
    assert_eq!(
        claim["status"], "unsatisfied",
        "broken Scala body x*3 must be UNSATISFIED; claim: {claim}"
    );
    assert_eq!(claim["pass"], false, "claim: {claim}");
    assert!(claim["witnessCid"].is_null(), "claim: {claim}");
    assert_eq!(receipt["ok"], false, "receipt: {receipt}");
    assert_eq!(code, 1, "broken-body claim must exit 1; got {code}");

    let _ = fs::remove_dir_all(&project);
}

#[test]
#[serial(scala_cli)]
fn scala_production_path_refuses_planted_contradictory_implication() {
    if !scala_cli_available() {
        eprintln!("scala-cli not on PATH: skipping Scala contradictory-implication test");
        return;
    }
    if !z3_available() {
        eprintln!("z3 not on PATH: skipping Scala contradictory-implication test");
        return;
    }
    let project = stage_scala_project("contradiction", 2);
    run_mint(&project);

    let (green, green_code) = contradiction::run_prove_json_with_code(&provekit_bin(), &project);
    assert_eq!(
        green_code, 0,
        "base Scala project must prove before planting contradiction; report: {green}"
    );
    contradiction::assert_green_proves_one_bridge(&green, green_code);

    contradiction::plant_contradictory_implication_proof(
        &project.join(".provekit"),
        "scala",
        "scala-tests",
        "scala_parity",
    );
    let (red, red_code) = contradiction::run_prove_json_with_code(&provekit_bin(), &project);
    contradiction::assert_prove_refuses_contradiction(
        &red,
        red_code,
        "scala_parity_requires_positive",
    );

    let _ = fs::remove_dir_all(&project);
}
