// SPDX-License-Identifier: Apache-2.0
//
// RUST end-to-end production-bridge gauntlet. This is the Rust equivalent of
// the Go/Python production bridge tests, but it keeps source parsing in the
// Rust kit surfaces:
//
//   - provekit-walk-rpc, via the rust-walk-contracts surface, derives the
//     function-contract from the real Rust body `x * 2`.
//   - provekit-lift, via the rust-tests surface, harvests the real Rust
//     `#[test]` assertion `assert_eq!(double(3), 6)`.
//
// `provekit mint` merges the normalized ir-document responses and auto-writes
// the `double -> targetContractCid` bridge. `provekit verify` must then report
// exactly one non-vacuous claim, mint a witness for the positive case, and
// refuse to mint one for the broken body.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

use serde_json::Value as Json;

struct RustLifterBins {
    walk_rpc: PathBuf,
    lift: PathBuf,
}

fn provekit_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_provekit"))
}

/// CARGO_MANIFEST_DIR = .../implementations/rust/provekit-cli; three parents up
/// is the repo root.
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

fn rust_workspace_root() -> PathBuf {
    repo_root().join("implementations").join("rust")
}

fn z3_available() -> bool {
    Command::new("z3")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn build_rust_lifter_bins() -> &'static RustLifterBins {
    static BINS: OnceLock<RustLifterBins> = OnceLock::new();
    BINS.get_or_init(|| {
        let workspace = rust_workspace_root();
        for (package, bin) in [
            ("provekit-walk", "provekit-walk-rpc"),
            ("provekit-lift", "provekit-lift"),
        ] {
            let output = Command::new("cargo")
                .current_dir(&workspace)
                .args(["build", "-p", package, "--bin", bin])
                .output()
                .unwrap_or_else(|e| panic!("spawn cargo build -p {package} --bin {bin}: {e}"));
            assert!(
                output.status.success(),
                "cargo build -p {package} --bin {bin} failed\n  stdout: {}\n  stderr: {}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
        }

        let bin_dir = provekit_bin()
            .parent()
            .expect("provekit bin parent")
            .to_path_buf();
        let walk_rpc = bin_dir.join("provekit-walk-rpc");
        let lift = bin_dir.join("provekit-lift");
        assert!(
            walk_rpc.exists(),
            "cargo build produced no {}",
            walk_rpc.display()
        );
        assert!(lift.exists(), "cargo build produced no {}", lift.display());
        RustLifterBins { walk_rpc, lift }
    })
}

fn unique_dir(suffix: &str) -> PathBuf {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let p = std::env::temp_dir().join(format!("provekit-rust-prod-bridge-{stamp}-{suffix}"));
    fs::create_dir_all(&p).expect("mkdir project");
    p
}

fn stage_rust_project(suffix: &str, body_factor: i64) -> PathBuf {
    let example = repo_root().join("examples").join("rust-double");
    let bins = build_rust_lifter_bins();
    let project = unique_dir(suffix);

    fs::copy(example.join("Cargo.toml"), project.join("Cargo.toml")).expect("copy Cargo.toml");
    fs::create_dir_all(project.join("src")).expect("mkdir src");
    let lib_src = format!(
        r#"pub fn double(x: i64) -> i64 {{
    x * {body_factor}
}}

#[cfg(test)]
mod tests {{
    use super::double;

    #[test]
    fn double_three_is_six() {{
        assert_eq!(double(3), 6);
    }}
}}
"#
    );
    fs::write(project.join("src").join("lib.rs"), lib_src).expect("write src/lib.rs");

    let provekit = project.join(".provekit");
    fs::create_dir_all(provekit.join("lift").join("rust-walk-contracts"))
        .expect("mkdir rust-walk-contracts");
    fs::create_dir_all(provekit.join("lift").join("rust-tests")).expect("mkdir rust-tests");
    fs::copy(
        example.join(".provekit").join("config.toml"),
        provekit.join("config.toml"),
    )
    .expect("copy config.toml");

    let walk_manifest = format!(
        "name = \"rust-walk-contracts\"\ncommand = [\"{}\", \"--rpc\"]\nworking_dir = \".\"\n",
        bins.walk_rpc.display()
    );
    fs::write(
        provekit
            .join("lift")
            .join("rust-walk-contracts")
            .join("manifest.toml"),
        walk_manifest,
    )
    .expect("write rust-walk manifest");

    let tests_manifest = format!(
        "name = \"rust-tests\"\ncommand = [\"{}\", \"--rpc\"]\nworking_dir = \".\"\n",
        bins.lift.display()
    );
    fs::write(
        provekit
            .join("lift")
            .join("rust-tests")
            .join("manifest.toml"),
        tests_manifest,
    )
    .expect("write rust-tests manifest");

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
fn rust_mint_auto_writes_body_discharge_bridge_from_real_lifters() {
    let project = stage_rust_project("bridge", 2);
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
        "minted bundle must include the Rust test assertion contract mentioning double"
    );

    let _ = fs::remove_dir_all(&project);
}

#[test]
fn rust_production_path_double_discharges_and_mints_witness() {
    if !z3_available() {
        eprintln!("z3 not on PATH: skipping rust production-bridge positive test");
        return;
    }
    let project = stage_rust_project("pos", 2);
    run_mint(&project);

    let witnesses = project.join("witnesses-out");
    let (receipt, code) = run_verify_json_with_code(&project, &witnesses);

    assert_eq!(
        receipt["kind"], "verification-receipt",
        "receipt: {receipt}"
    );
    assert_eq!(
        receipt["totalClaims"], 1,
        "exactly one body-bearing Rust callsite must enumerate; receipt: {receipt}"
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
    eprintln!("RUST_PRODUCTION_POSITIVE_WITNESS_CID={witness_cid}");
    assert_eq!(receipt["ok"], true, "receipt: {receipt}");
    assert_eq!(code, 0, "positive run exits clean; got {code}");

    let _ = fs::remove_dir_all(&project);
}

#[test]
fn rust_production_path_broken_body_fails_unsatisfied_no_witness() {
    if !z3_available() {
        eprintln!("z3 not on PATH: skipping rust production-bridge negative test");
        return;
    }
    let project = stage_rust_project("neg", 3);
    run_mint(&project);

    let witnesses = project.join("witnesses-out");
    let (receipt, code) = run_verify_json_with_code(&project, &witnesses);

    assert_eq!(receipt["totalClaims"], 1, "receipt: {receipt}");
    let claim = &receipt["claims"].as_array().expect("claims")[0];
    assert_eq!(
        claim["status"], "unsatisfied",
        "broken Rust body x*3 must be UNSATISFIED; claim: {claim}"
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
    eprintln!(
        "RUST_PRODUCTION_NEGATIVE_EXIT_CODE={code} STATUS={}",
        claim["status"]
    );

    let _ = fs::remove_dir_all(&project);
}
