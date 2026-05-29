// SPDX-License-Identifier: Apache-2.0
//
// Swift production-bridge gauntlet. The Rust CLI is only the generic
// config/manifest/RPC client here: Swift parsing, XCTest assertion harvesting,
// and Swift-term-to-ProofIR normalization all live in Swift kit surfaces.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

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

fn swift_available() -> bool {
    Command::new("swift")
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
    let p = std::env::temp_dir().join(format!("provekit-swift-prod-bridge-{stamp}-{suffix}"));
    fs::create_dir_all(&p).expect("mkdir project");
    p
}

fn toml_path(path: &Path) -> String {
    path.display()
        .to_string()
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
}

fn swift_products() -> (PathBuf, PathBuf) {
    static PRODUCTS: OnceLock<(PathBuf, PathBuf)> = OnceLock::new();
    PRODUCTS
        .get_or_init(|| {
            let swift_root = repo_root().join("implementations").join("swift");
            for product in [
                "provekit-lift-swift-source",
                "provekit-lift-swift-xctest-tests",
            ] {
                let out = Command::new("swift")
                    .current_dir(&swift_root)
                    .args(["build", "--product", product])
                    .output()
                    .unwrap_or_else(|e| panic!("spawn swift build {product}: {e}"));
                assert!(
                    out.status.success(),
                    "swift build {product} failed\nstdout:\n{}\nstderr:\n{}",
                    String::from_utf8_lossy(&out.stdout),
                    String::from_utf8_lossy(&out.stderr)
                );
            }
            (
                swift_root
                    .join(".build")
                    .join("debug")
                    .join("provekit-lift-swift-source"),
                swift_root
                    .join(".build")
                    .join("debug")
                    .join("provekit-lift-swift-xctest-tests"),
            )
        })
        .clone()
}

fn stage_swift_project(suffix: &str, body_factor: i64) -> PathBuf {
    let (source_bin, tests_bin) = swift_products();
    let project = unique_dir(suffix);
    fs::write(
        project.join("Math.swift"),
        format!("public func double(_ x: Int) -> Int {{\n    return x * {body_factor}\n}}\n"),
    )
    .expect("write Math.swift");
    fs::write(
        project.join("MathTests.swift"),
        r#"import XCTest

final class MathTests: XCTestCase {
    func testDoubleThreeIsSix() {
        XCTAssertEqual(double(3), 6)
    }
}
"#,
    )
    .expect("write MathTests.swift");

    let provekit = project.join(".provekit");
    for surface in ["swift-source", "swift-xctest-tests"] {
        fs::create_dir_all(provekit.join("lift").join(surface))
            .unwrap_or_else(|_| panic!("mkdir manifest dir {surface}"));
    }
    fs::write(
        provekit.join("config.toml"),
        "[[plugins]]\n\
         name = \"swift-source\"\n\
         surface = \"swift-source\"\n\
         layer = \"verify\"\n\
         \n\
         [[plugins]]\n\
         name = \"swift-xctest-tests\"\n\
         surface = \"swift-xctest-tests\"\n\
         layer = \"tests\"\n",
    )
    .expect("write config.toml");
    fs::write(
        provekit
            .join("lift")
            .join("swift-source")
            .join("manifest.toml"),
        format!(
            "name = \"swift-source\"\ncommand = [\"{}\", \"--rpc\"]\nworking_dir = \".\"\n",
            toml_path(&source_bin)
        ),
    )
    .expect("write swift-source manifest");
    fs::write(
        provekit
            .join("lift")
            .join("swift-xctest-tests")
            .join("manifest.toml"),
        format!(
            "name = \"swift-xctest-tests\"\ncommand = [\"{}\", \"--rpc\"]\nworking_dir = \".\"\n",
            toml_path(&tests_bin)
        ),
    )
    .expect("write swift-xctest-tests manifest");

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
        "provekit mint must succeed\nstdout:\n{}\nstderr:\n{}",
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
fn swift_mint_auto_writes_body_discharge_bridge_from_real_lifters() {
    if !swift_available() {
        eprintln!("swift not on PATH: skipping Swift production-bridge bridge-writer test");
        return;
    }
    let project = stage_swift_project("bridge", 2);
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
    assert!(
        pool.mementos.values().any(|member| {
            provekit_verifier::types::memento_kind(member) == Some("contract")
                && provekit_verifier::types::memento_body_field(member, "inv")
                    .is_some_and(|inv| json_contains_str(inv, "double"))
        }),
        "minted bundle must include the Swift XCTest assertion contract mentioning double"
    );

    let _ = fs::remove_dir_all(&project);
}

#[test]
fn swift_production_path_double_discharges_and_mints_witness() {
    if !swift_available() {
        eprintln!("swift not on PATH: skipping Swift production-bridge positive test");
        return;
    }
    if !z3_available() {
        eprintln!("z3 not on PATH: skipping Swift production-bridge positive test");
        return;
    }
    let project = stage_swift_project("pos", 2);
    run_mint(&project);

    let witnesses = project.join("witnesses-out");
    let (receipt, code) = run_verify_json_with_code(&project, &witnesses);

    assert_eq!(
        receipt["kind"], "verification-receipt",
        "receipt: {receipt}"
    );
    assert_eq!(
        receipt["totalClaims"], 1,
        "exactly one body-bearing Swift callsite must enumerate; receipt: {receipt}"
    );
    let claim = &receipt["claims"].as_array().expect("claims")[0];
    assert_eq!(
        claim["pass"], true,
        "double(3)==6 must discharge through Swift body (* x 2); claim: {claim}"
    );
    assert_eq!(claim["status"], "discharged", "claim: {claim}");
    let solver = claim["dischargingSolver"].as_str().unwrap_or("");
    assert!(
        solver.starts_with("z3@"),
        "discharging solver must be z3; got `{solver}`"
    );
    let witness_cid = claim["witnessCid"].as_str().expect("witness minted");
    assert!(witness_cid.starts_with("blake3-512:"));
    eprintln!("SWIFT_PRODUCTION_POSITIVE_WITNESS_CID={witness_cid}");
    assert_eq!(receipt["ok"], true, "receipt: {receipt}");
    assert_eq!(code, 0, "positive run exits clean; got {code}");

    let _ = fs::remove_dir_all(&project);
}

#[test]
fn swift_production_path_broken_body_fails_unsatisfied_no_witness() {
    if !swift_available() {
        eprintln!("swift not on PATH: skipping Swift production-bridge negative test");
        return;
    }
    if !z3_available() {
        eprintln!("z3 not on PATH: skipping Swift production-bridge negative test");
        return;
    }
    let project = stage_swift_project("neg", 3);
    run_mint(&project);

    let witnesses = project.join("witnesses-out");
    let (receipt, code) = run_verify_json_with_code(&project, &witnesses);

    assert_eq!(receipt["totalClaims"], 1, "receipt: {receipt}");
    let claim = &receipt["claims"].as_array().expect("claims")[0];
    assert_eq!(
        claim["status"], "unsatisfied",
        "broken Swift body x*3 must be UNSATISFIED; claim: {claim}"
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
fn swift_production_path_refuses_planted_contradictory_implication() {
    if !swift_available() {
        eprintln!("swift not on PATH: skipping Swift contradictory-implication test");
        return;
    }
    if !z3_available() {
        eprintln!("z3 not on PATH: skipping Swift contradictory-implication test");
        return;
    }
    let project = stage_swift_project("contradiction", 2);
    run_mint(&project);

    let (green, green_code) = contradiction::run_prove_json_with_code(&provekit_bin(), &project);
    assert_eq!(
        green_code, 0,
        "base Swift project must prove before planting contradiction; report: {green}"
    );
    assert_eq!(green["totalCallsites"], 1, "green report: {green}");

    contradiction::plant_contradictory_implication_proof(
        &project.join(".provekit"),
        "swift",
        "swift-tests",
        "swift_parity",
    );
    let (red, red_code) = contradiction::run_prove_json_with_code(&provekit_bin(), &project);
    contradiction::assert_prove_refuses_contradiction(
        &red,
        red_code,
        "swift_parity_requires_positive",
    );

    let _ = fs::remove_dir_all(&project);
}
