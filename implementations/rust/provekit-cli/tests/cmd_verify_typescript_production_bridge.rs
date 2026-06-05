// SPDX-License-Identifier: Apache-2.0
//
// TypeScript production-bridge gauntlet. The Rust CLI is only the generic
// config/manifest/RPC client here: TypeScript parsing, Vitest assertion
// harvesting, and TS-term-to-ProofIR normalization all live in the TS kit.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value as Json;

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

fn typescript_env_enabled() -> bool {
    std::env::var("BCARGO_TYPESCRIPT_ENV").map_or(true, |value| value != "0")
}

fn skip_when_typescript_env_disabled(test_name: &str) -> bool {
    if typescript_env_enabled() {
        return false;
    }
    eprintln!("skipping: BCARGO_TYPESCRIPT_ENV=0 for {test_name}");
    true
}

fn node_available() -> bool {
    Command::new("node")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn tsx_cli() -> Option<PathBuf> {
    let path = repo_root()
        .join("node_modules")
        .join("tsx")
        .join("dist")
        .join("cli.mjs");
    path.exists().then_some(path)
}

fn unique_dir(suffix: &str) -> PathBuf {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let p = std::env::temp_dir().join(format!("provekit-ts-prod-bridge-{stamp}-{suffix}"));
    fs::create_dir_all(&p).expect("mkdir project");
    p
}

fn stage_typescript_project(suffix: &str, body_factor: i64) -> PathBuf {
    let tsx = tsx_cli().expect("tsx CLI must exist; run make build-ts first");
    let project = unique_dir(suffix);
    fs::create_dir_all(project.join("src")).expect("mkdir src");
    fs::write(
        project.join("src").join("double.ts"),
        format!("export function double(x: number): number {{\n  return x * {body_factor};\n}}\n"),
    )
    .expect("write src/double.ts");
    fs::write(
        project.join("double.test.ts"),
        r#"import { expect, it } from "vitest";
import { double } from "./src/double";

it("double three is six", () => {
  expect(double(3)).toBe(6);
});
"#,
    )
    .expect("write double.test.ts");

    let provekit = project.join(".provekit");
    fs::create_dir_all(provekit.join("lift").join("typescript-source"))
        .expect("mkdir typescript-source manifest dir");
    fs::create_dir_all(provekit.join("lift").join("typescript-vitest-tests"))
        .expect("mkdir typescript-vitest-tests manifest dir");
    fs::write(
        provekit.join("config.toml"),
        "[[plugins]]\n\
         name = \"typescript-source\"\n\
         surface = \"typescript-source\"\n\
         layer = \"verify\"\n\
         \n\
         [[plugins]]\n\
         name = \"typescript-vitest-tests\"\n\
         surface = \"typescript-vitest-tests\"\n",
    )
    .expect("write config.toml");

    let ts_source_bin = repo_root()
        .join("implementations")
        .join("typescript")
        .join("src")
        .join("lift")
        .join("typescript-source")
        .join("bin.ts");
    let vitest_bin = repo_root()
        .join("implementations")
        .join("typescript")
        .join("src")
        .join("lift")
        .join("vitest-tests-bin.ts");

    fs::write(
        provekit
            .join("lift")
            .join("typescript-source")
            .join("manifest.toml"),
        format!(
            "name = \"typescript-source\"\ncommand = [\"node\", \"{}\", \"{}\", \"--rpc\"]\nworking_dir = \".\"\n",
            tsx.display(),
            ts_source_bin.display()
        ),
    )
    .expect("write typescript-source manifest");
    fs::write(
        provekit
            .join("lift")
            .join("typescript-vitest-tests")
            .join("manifest.toml"),
        format!(
            "name = \"typescript-vitest-tests\"\ncommand = [\"node\", \"{}\", \"{}\", \"--rpc\"]\nworking_dir = \".\"\n",
            tsx.display(),
            vitest_bin.display()
        ),
    )
    .expect("write typescript-vitest-tests manifest");

    project
}

fn run_mint(project: &Path) {
    let out = Command::new(provekit_bin())
        .arg("mint")
        .arg("--project")
        .arg(project)
        .arg("--out")
        .arg(project)
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

fn json_contains_str(value: &Json, needle: &str) -> bool {
    match value {
        Json::String(s) => s == needle,
        Json::Array(values) => values.iter().any(|v| json_contains_str(v, needle)),
        Json::Object(map) => map.values().any(|v| json_contains_str(v, needle)),
        _ => false,
    }
}

#[test]
fn typescript_mint_auto_writes_body_discharge_bridge_from_real_lifters() {
    if skip_when_typescript_env_disabled("TypeScript production-bridge bridge-writer test") {
        return;
    }
    if !node_available() {
        eprintln!("node not on PATH: skipping TypeScript production-bridge bridge-writer test");
        return;
    }
    if tsx_cli().is_none() {
        eprintln!("tsx not installed at repo root: skipping; run make build-ts first");
        return;
    }
    let project = stage_typescript_project("bridge", 2);
    run_mint(&project);

    let pool = provekit_verifier::load_all_proofs::run(&project);
    assert!(
        pool.load_errors.is_empty(),
        "tool-minted bundle must load cleanly: {:?}",
        pool.load_errors
    );

    let bridge = pool.bridges_by_symbol.get("double").unwrap_or_else(|| {
        panic!(
            "mint must auto-write + index a bridge with sourceSymbol=double; indexed: {:?}",
            pool.bridges_by_symbol.keys().collect::<Vec<_>>()
        )
    });
    let target_cid = provekit_verifier::types::memento_body_field(bridge, "targetContractCid")
        .and_then(|v| v.as_str())
        .expect("bridge must carry targetContractCid")
        .to_string();
    let target = pool.mementos.get(&target_cid).unwrap_or_else(|| {
        panic!("bridge.targetContractCid {target_cid} must resolve to a member")
    });
    assert_eq!(
        provekit_verifier::types::memento_kind(target),
        Some("contract"),
        "bridge target must be a contract memento"
    );
    let formals = provekit_verifier::types::memento_body_field(target, "formals")
        .and_then(|v| v.as_array())
        .expect("tool-written op-contract must carry formals");
    assert_eq!(
        formals.first().and_then(|v| v.as_str()),
        Some("x"),
        "op-contract formals must be [x]"
    );
    assert!(
        provekit_verifier::types::memento_body_field(target, "post").is_some(),
        "op-contract must carry the body-derived post"
    );

    let saw_callsite_contract = pool.mementos.values().any(|member| {
        provekit_verifier::types::memento_kind(member) == Some("contract")
            && provekit_verifier::types::memento_body_field(member, "inv")
                .is_some_and(|inv| json_contains_str(inv, "double"))
    });
    assert!(
        saw_callsite_contract,
        "minted bundle must include the TypeScript Vitest assertion contract mentioning double"
    );

    let _ = fs::remove_dir_all(&project);
}

#[test]
fn typescript_production_path_double_discharges_and_mints_witness() {
    if skip_when_typescript_env_disabled("TypeScript production-bridge positive test") {
        return;
    }
    if !node_available() {
        eprintln!("node not on PATH: skipping TypeScript production-bridge positive test");
        return;
    }
    if tsx_cli().is_none() {
        eprintln!("tsx not installed at repo root: skipping; run make build-ts first");
        return;
    }
    if !z3_available() {
        eprintln!("z3 not on PATH: skipping TypeScript production-bridge positive test");
        return;
    }
    let project = stage_typescript_project("pos", 2);
    run_mint(&project);

    let witnesses = project.join("witnesses-out");
    let (receipt, code) = run_verify_json_with_code(&project, &witnesses);

    assert_eq!(
        receipt["kind"], "verification-receipt",
        "receipt: {receipt}"
    );
    assert_eq!(
        receipt["totalClaims"], 1,
        "exactly one body-bearing TypeScript callsite must enumerate; receipt: {receipt}"
    );
    let claim = &receipt["claims"].as_array().expect("claims")[0];
    assert_eq!(
        claim["pass"], true,
        "double(3)==6 must discharge through body (* x 2); claim: {claim}"
    );
    assert_eq!(claim["status"], "discharged", "claim: {claim}");
    let solver = claim["dischargingSolver"].as_str().unwrap_or("");
    assert!(
        solver.starts_with("z3@"),
        "discharging solver must be z3; got `{solver}`"
    );
    let witness_cid = claim["witnessCid"].as_str().expect("witness minted");
    assert!(witness_cid.starts_with("blake3-512:"));
    eprintln!("TYPESCRIPT_PRODUCTION_POSITIVE_WITNESS_CID={witness_cid}");
    assert_eq!(receipt["ok"], true, "receipt: {receipt}");
    assert_eq!(code, 0, "positive run exits clean; got {code}");

    let _ = fs::remove_dir_all(&project);
}

#[test]
fn typescript_production_path_broken_body_fails_unsatisfied_no_witness() {
    if skip_when_typescript_env_disabled("TypeScript production-bridge negative test") {
        return;
    }
    if !node_available() {
        eprintln!("node not on PATH: skipping TypeScript production-bridge negative test");
        return;
    }
    if tsx_cli().is_none() {
        eprintln!("tsx not installed at repo root: skipping; run make build-ts first");
        return;
    }
    if !z3_available() {
        eprintln!("z3 not on PATH: skipping TypeScript production-bridge negative test");
        return;
    }
    let project = stage_typescript_project("neg", 3);
    run_mint(&project);

    let witnesses = project.join("witnesses-out");
    let (receipt, code) = run_verify_json_with_code(&project, &witnesses);

    assert_eq!(receipt["totalClaims"], 1, "receipt: {receipt}");
    let claim = &receipt["claims"].as_array().expect("claims")[0];
    assert_eq!(
        claim["status"], "unsatisfied",
        "broken TypeScript body x*3 must be UNSATISFIED; claim: {claim}"
    );
    assert_eq!(claim["pass"], false, "claim: {claim}");
    assert!(
        claim["witnessCid"].is_null(),
        "no witness for a violated claim; claim: {claim}"
    );
    assert_eq!(receipt["ok"], false, "receipt: {receipt}");
    let witness_files: Vec<_> = fs::read_dir(&witnesses)
        .map(|rd| rd.filter_map(|e| e.ok()).collect())
        .unwrap_or_default();
    assert!(
        witness_files.is_empty(),
        "witness dir must be empty for a violated claim; found {} files",
        witness_files.len()
    );
    assert_eq!(
        code, 1,
        "broken-body claim must exit 1 (EXIT_VERIFY_FAIL); got {code}"
    );

    let _ = fs::remove_dir_all(&project);
}

#[test]
fn typescript_production_path_refuses_planted_contradictory_implication() {
    if skip_when_typescript_env_disabled("TypeScript contradictory-implication test") {
        return;
    }
    if !node_available() {
        eprintln!("node not on PATH: skipping TypeScript contradictory-implication test");
        return;
    }
    if tsx_cli().is_none() {
        eprintln!("tsx not installed at repo root: skipping; run make build-ts first");
        return;
    }
    if !z3_available() {
        eprintln!("z3 not on PATH: skipping TypeScript contradictory-implication test");
        return;
    }
    let project = stage_typescript_project("contradiction", 2);
    run_mint(&project);

    let (green, green_code) = contradiction::run_prove_json_with_code(&provekit_bin(), &project);
    assert_eq!(
        green_code, 0,
        "base TypeScript project must prove before planting contradiction; report: {green}"
    );
    contradiction::assert_green_proves_one_bridge(&green, green_code);

    contradiction::plant_contradictory_implication_proof(
        &project.join(".provekit"),
        "typescript",
        "typescript-tests",
        "typescript_parity",
    );
    let (red, red_code) = contradiction::run_prove_json_with_code(&provekit_bin(), &project);
    contradiction::assert_prove_refuses_contradiction(
        &red,
        red_code,
        "typescript_parity_requires_positive",
    );

    let _ = fs::remove_dir_all(&project);
}
