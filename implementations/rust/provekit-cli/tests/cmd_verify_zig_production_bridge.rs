// SPDX-License-Identifier: Apache-2.0
//
// Zig production-bridge gauntlet. The Rust CLI is only the generic
// config/manifest/RPC client here: Zig parsing, std.testing assertion
// harvesting, implication lifting, and Zig-term-to-ProofIR normalization all
// live in Zig lifter surfaces.

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

fn zig_available() -> bool {
    Command::new("zig")
        .arg("version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn unique_dir(suffix: &str) -> PathBuf {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let p = std::env::temp_dir().join(format!("provekit-zig-prod-bridge-{stamp}-{suffix}"));
    fs::create_dir_all(&p).expect("mkdir project");
    p
}

fn toml_path(path: &Path) -> String {
    path.display()
        .to_string()
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
}

fn zig_command(cache_root: &Path) -> String {
    format!(
        "[\"env\", \"ZIG_GLOBAL_CACHE_DIR={}\", \"ZIG_LOCAL_CACHE_DIR={}\", \"zig\", \"build\", \"run\", \"--\", \"--rpc\"]",
        toml_path(&cache_root.join("zig-global-cache")),
        toml_path(&cache_root.join("zig-local-cache")),
    )
}

fn stage_zig_project(suffix: &str, body_factor: i64) -> PathBuf {
    let root = repo_root();
    let project = unique_dir(suffix);
    fs::write(
        project.join("math.zig"),
        format!(
            r#"const std = @import("std");

pub fn double(x: i64) i64 {{
    return x * {body_factor};
}}

test "double three is six" {{
    const actual = double(3);
    try std.testing.expectEqual(@as(i64, 6), actual);
}}
"#
        ),
    )
    .expect("write math.zig");

    let provekit = project.join(".provekit");
    for surface in ["zig-source", "zig-tests", "zig-implications"] {
        fs::create_dir_all(provekit.join("lift").join(surface))
            .unwrap_or_else(|_| panic!("mkdir manifest dir {surface}"));
    }
    fs::write(
        provekit.join("config.toml"),
        "[[plugins]]\n\
         name = \"zig-source\"\n\
         surface = \"zig-source\"\n\
         layer = \"verify\"\n\
         \n\
         [[plugins]]\n\
         name = \"zig-tests\"\n\
         surface = \"zig-tests\"\n\
         layer = \"tests\"\n\
         \n\
         [[plugins]]\n\
         name = \"zig-implications\"\n\
         surface = \"zig-implications\"\n\
         layer = \"implications\"\n",
    )
    .expect("write config.toml");

    let source_dir = root
        .join("implementations")
        .join("zig")
        .join("provekit-lift-zig-source");
    let tests_dir = root
        .join("implementations")
        .join("zig")
        .join("provekit-lift-zig-tests");
    let cache_root = project.join(".provekit").join("zig-cache");
    fs::create_dir_all(&cache_root).expect("mkdir zig cache");
    let command = zig_command(&cache_root);

    fs::write(
        provekit
            .join("lift")
            .join("zig-source")
            .join("manifest.toml"),
        format!(
            "name = \"zig-source\"\ncommand = {command}\nworking_dir = \"{}\"\n",
            toml_path(&source_dir)
        ),
    )
    .expect("write zig-source manifest");
    fs::write(
        provekit
            .join("lift")
            .join("zig-tests")
            .join("manifest.toml"),
        format!(
            "name = \"zig-tests\"\ncommand = {command}\nworking_dir = \"{}\"\n",
            toml_path(&tests_dir)
        ),
    )
    .expect("write zig-tests manifest");
    fs::write(
        provekit
            .join("lift")
            .join("zig-implications")
            .join("manifest.toml"),
        format!(
            "name = \"zig-implications\"\ncommand = {command}\nworking_dir = \"{}\"\nmethod = \"provekit.plugin.lift_implications\"\nphase = \"consumer\"\n",
            toml_path(&tests_dir)
        ),
    )
    .expect("write zig-implications manifest");

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

fn json_contains_str(value: &Json, needle: &str) -> bool {
    match value {
        Json::String(s) => s == needle,
        Json::Array(values) => values.iter().any(|v| json_contains_str(v, needle)),
        Json::Object(map) => map.values().any(|v| json_contains_str(v, needle)),
        _ => false,
    }
}

#[test]
fn zig_mint_auto_writes_body_discharge_bridge_and_implications_from_real_lifters() {
    if !zig_available() {
        eprintln!("zig not on PATH: skipping Zig production-bridge bridge-writer test");
        return;
    }
    let project = stage_zig_project("bridge", 2);
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
    assert!(
        pool.mementos.values().any(|member| {
            provekit_verifier::types::memento_kind(member) == Some("contract")
                && provekit_verifier::types::memento_body_field(member, "inv")
                    .is_some_and(|inv| json_contains_str(inv, "double"))
        }),
        "minted bundle must include the Zig std.testing assertion contract mentioning double"
    );
    assert!(
        pool.mementos
            .values()
            .any(|member| provekit_verifier::types::memento_kind(member) == Some("implication")),
        "zig-implications lifter output must be minted into implication mementos"
    );

    let _ = fs::remove_dir_all(&project);
}

#[test]
fn zig_production_path_double_discharges_and_mints_witness() {
    if !zig_available() {
        eprintln!("zig not on PATH: skipping Zig production-bridge positive test");
        return;
    }
    if !z3_available() {
        eprintln!("z3 not on PATH: skipping Zig production-bridge positive test");
        return;
    }
    let project = stage_zig_project("pos", 2);
    run_mint(&project);

    let witnesses = project.join("witnesses-out");
    let (receipt, code) = run_verify_json_with_code(&project, &witnesses);

    assert_eq!(receipt["totalClaims"], 1, "receipt: {receipt}");
    let claim = &receipt["claims"].as_array().expect("claims")[0];
    assert_eq!(
        claim["pass"], true,
        "double(3)==6 must discharge through Zig body (* x 2); claim: {claim}"
    );
    assert_eq!(claim["status"], "discharged", "claim: {claim}");
    assert_eq!(receipt["ok"], true, "receipt: {receipt}");
    assert_eq!(code, 0, "positive run exits clean; got {code}");

    let _ = fs::remove_dir_all(&project);
}

#[test]
fn zig_production_path_broken_body_fails_unsatisfied_no_witness() {
    if !zig_available() {
        eprintln!("zig not on PATH: skipping Zig production-bridge negative test");
        return;
    }
    if !z3_available() {
        eprintln!("z3 not on PATH: skipping Zig production-bridge negative test");
        return;
    }
    let project = stage_zig_project("neg", 3);
    run_mint(&project);

    let witnesses = project.join("witnesses-out");
    let (receipt, code) = run_verify_json_with_code(&project, &witnesses);

    assert_eq!(receipt["totalClaims"], 1, "receipt: {receipt}");
    let claim = &receipt["claims"].as_array().expect("claims")[0];
    assert_eq!(
        claim["status"], "unsatisfied",
        "broken Zig body x*3 must be UNSATISFIED; claim: {claim}"
    );
    assert_eq!(claim["pass"], false, "claim: {claim}");
    assert!(claim["witnessCid"].is_null(), "claim: {claim}");
    assert_eq!(receipt["ok"], false, "receipt: {receipt}");
    assert_eq!(code, 1, "broken-body claim must exit 1; got {code}");

    let _ = fs::remove_dir_all(&project);
}

#[test]
fn zig_production_path_refuses_planted_contradictory_implication() {
    if !zig_available() {
        eprintln!("zig not on PATH: skipping Zig contradictory-implication test");
        return;
    }
    if !z3_available() {
        eprintln!("z3 not on PATH: skipping Zig contradictory-implication test");
        return;
    }
    let project = stage_zig_project("contradiction", 2);
    run_mint(&project);

    let (green, green_code) = contradiction::run_prove_json_with_code(&provekit_bin(), &project);
    assert_eq!(
        green_code, 0,
        "base Zig project must prove before planting contradiction; report: {green}"
    );
    contradiction::assert_green_proves_one_bridge(&green, green_code);

    contradiction::plant_contradictory_implication_proof(
        &project.join(".provekit"),
        "zig",
        "zig-tests",
        "zig_parity",
    );
    let (red, red_code) = contradiction::run_prove_json_with_code(&provekit_bin(), &project);
    contradiction::assert_prove_refuses_contradiction(
        &red,
        red_code,
        "zig_parity_requires_positive",
    );

    let _ = fs::remove_dir_all(&project);
}
