// SPDX-License-Identifier: Apache-2.0
//
// GO end-to-end production-bridge gauntlet -- the Go analog of
// `cmd_verify_production_bridge.rs`. Proves the verification spine is
// LANGUAGE-NEUTRAL for a fourth language: a Go function's body-derived
// contract lifts to ProofIR (`post = result == (* x 2)` plus `formals`) and
// the verifier discharges the obligation through the body via wp + z3,
// exactly as for Rust and Java.
//
// Unlike the Rust production-bridge test (which uses a mock lifter that emits
// the ir-document directly), this drives the REAL Go lifter binary
// `provekit-lift-go-verify` -- the verify-facing `go` lift surface -- built
// from `implementations/go/cmd/provekit-lift-go-verify`. That binary lifts the
// real Go sources in `examples/go-double`:
//
//   - `double.go`      -> function-contract `post = result == (* x 2)`
//                         (verify-facing dialect: `go:mul` normalized to `*`).
//   - `double_test.go` -> contract `inv = =(Double(3), 6)` (Go Layer-0 leaf
//                         assertion harvester).
//
// `provekit mint` AUTO-WRITES the `Double -> targetContractCid` bridge (#1443);
// the bridge is TOOL-written, asserted by inspecting `pool.bridges_by_symbol`.
// `provekit verify` then discharges both ways:
//
//   POSITIVE: `Double(3) == 6` reduces through the body `(* 3 2) == 6` -> z3
//     discharges -> pass, signed witness, exit 0.
//   NEGATIVE: break the body to `x * 3` -> `(* 3 3) == 6` -> z3 refutes ->
//     Unsatisfied, exit 1, NO witness.
//
// Requires `go` and `z3` on PATH; skips the solver-dependent asserts if z3 is
// absent, and skips entirely (with a loud eprintln) if go is absent.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value as Json;

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

fn z3_available() -> bool {
    Command::new("z3")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn go_available() -> bool {
    Command::new("go")
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
    let p = std::env::temp_dir().join(format!("provekit-go-prod-bridge-{stamp}-{suffix}"));
    fs::create_dir_all(&p).expect("mkdir project");
    p
}

/// Build the verify-facing Go lift binary once, into a stable per-process
/// path, and return it. Panics on build failure (a real regression).
fn build_go_lift_verify() -> PathBuf {
    let go_module = repo_root().join("implementations").join("go");
    let out = std::env::temp_dir().join(format!("provekit-lift-go-verify-{}", std::process::id()));
    let status = Command::new("go")
        .current_dir(&go_module)
        .args([
            "build",
            "-o",
            out.to_str().expect("utf8 out path"),
            "./cmd/provekit-lift-go-verify",
        ])
        .output()
        .expect("spawn go build");
    assert!(
        status.status.success(),
        "go build provekit-lift-go-verify failed\n  stdout: {}\n  stderr: {}",
        String::from_utf8_lossy(&status.stdout),
        String::from_utf8_lossy(&status.stderr)
    );
    assert!(
        out.exists(),
        "go build produced no binary at {}",
        out.display()
    );
    out
}

/// Copy the in-repo `examples/go-double` into a fresh tempdir, rewrite the
/// `go` lift manifest's `command` to the built binary path, and (for the
/// negative case) mutate the body multiplier. Returns the tempdir project.
fn stage_go_project(suffix: &str, lift_bin: &Path, body_factor: i64) -> PathBuf {
    let example = repo_root().join("examples").join("go-double");
    let project = unique_dir(suffix);

    // double.go: the library, with the body multiplier (2 honest, 3 broken).
    let double_src =
        format!("package sample\n\nfunc Double(x int) int {{\n\treturn x * {body_factor}\n}}\n");
    fs::write(project.join("double.go"), double_src).expect("write double.go");

    // double_test.go: copied verbatim (the harvested `Double(3) == 6`).
    fs::copy(
        example.join("double_test.go"),
        project.join("double_test.go"),
    )
    .expect("copy double_test.go");
    fs::copy(example.join("go.mod"), project.join("go.mod")).expect("copy go.mod");

    // .provekit/config.toml: copied verbatim.
    let provekit = project.join(".provekit");
    fs::create_dir_all(provekit.join("lift").join("go")).expect("mkdir .provekit/lift/go");
    fs::copy(
        example.join(".provekit").join("config.toml"),
        provekit.join("config.toml"),
    )
    .expect("copy config.toml");

    // .provekit/lift/go/manifest.toml: rewrite command[0] to the built binary.
    let manifest = format!(
        "name = \"go\"\ncommand = [\"{}\", \"--rpc\"]\nworking_dir = \".\"\n",
        lift_bin.display()
    );
    fs::write(
        provekit.join("lift").join("go").join("manifest.toml"),
        manifest,
    )
    .expect("write manifest.toml");

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

/// THE bridge-writer assertion (language-neutral, runs without z3): the TOOL
/// wrote a `bridge` member for `Double` whose `targetContractCid` resolves to
/// a `contract` member carrying the body-derived `formals` + `post`. No
/// hand-bridging anywhere; the bridge came from `provekit mint`.
#[test]
fn go_mint_auto_writes_body_discharge_bridge() {
    if !go_available() {
        eprintln!("go not on PATH: skipping go production-bridge bridge-writer test");
        return;
    }
    let lift_bin = build_go_lift_verify();
    let project = stage_go_project("bridge", &lift_bin, 2);
    run_mint(&project);

    let pool = provekit_verifier::load_all_proofs::run(&project);
    assert!(
        pool.load_errors.is_empty(),
        "tool-minted bundle must load cleanly: {:?}",
        pool.load_errors
    );

    let bridge = pool.bridges_by_symbol.get("Double").unwrap_or_else(|| {
        panic!(
            "mint must auto-write + index a bridge with sourceSymbol=Double; indexed: {:?}",
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

    let _ = fs::remove_dir_all(&project);
}

/// POSITIVE end-to-end: real Go lifter, tool-written bridge, verify discharges
/// through the body `(* x 2)`, signed witness, exit 0.
#[test]
fn go_production_path_double_discharges_and_mints_witness() {
    if !go_available() {
        eprintln!("go not on PATH: skipping go production-bridge positive test");
        return;
    }
    if !z3_available() {
        eprintln!("z3 not on PATH: skipping go production-bridge positive test");
        return;
    }
    let lift_bin = build_go_lift_verify();
    let project = stage_go_project("pos", &lift_bin, 2);
    run_mint(&project);

    let witnesses = project.join("witnesses-out");
    let (receipt, code) = run_verify_json_with_code(&project, &witnesses);

    assert_eq!(
        receipt["kind"], "verification-receipt",
        "receipt: {receipt}"
    );
    assert_eq!(
        receipt["totalClaims"], 1,
        "exactly one body-bearing callsite (the tool-written bridge made Double(3) enumerate); receipt: {receipt}"
    );
    let claim = &receipt["claims"].as_array().expect("claims")[0];
    assert_eq!(
        claim["pass"], true,
        "Double(3)==6 must discharge through body (* x 2); claim: {claim}"
    );
    assert_eq!(claim["status"], "discharged", "claim: {claim}");

    let solver = claim["dischargingSolver"].as_str().unwrap_or("");
    assert!(
        solver.starts_with("z3@"),
        "discharging solver must be z3; got `{solver}`"
    );

    let witness_cid = claim["witnessCid"].as_str().expect("witness minted");
    assert!(witness_cid.starts_with("blake3-512:"));
    eprintln!("GO_PRODUCTION_POSITIVE_WITNESS_CID={witness_cid}");

    assert_eq!(receipt["ok"], true, "receipt: {receipt}");
    assert_eq!(code, 0, "positive run exits clean; got {code}");

    let _ = fs::remove_dir_all(&project);
}

/// NEGATIVE end-to-end: broken body `x * 3`, verify Unsatisfied, exit 1, no
/// witness. The decisive proof the tool-written bridge does not vacuously
/// pass: a real violation is caught honestly.
#[test]
fn go_production_path_broken_body_fails_unsatisfied_no_witness() {
    if !go_available() {
        eprintln!("go not on PATH: skipping go production-bridge negative test");
        return;
    }
    if !z3_available() {
        eprintln!("z3 not on PATH: skipping go production-bridge negative test");
        return;
    }
    let lift_bin = build_go_lift_verify();
    let project = stage_go_project("neg", &lift_bin, 3);
    run_mint(&project);

    let witnesses = project.join("witnesses-out");
    let (receipt, code) = run_verify_json_with_code(&project, &witnesses);

    assert_eq!(receipt["totalClaims"], 1, "receipt: {receipt}");
    let claim = &receipt["claims"].as_array().expect("claims")[0];
    assert_eq!(
        claim["status"], "unsatisfied",
        "broken body x*3 must be UNSATISFIED (not undecidable); claim: {claim}"
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
        "broken-body claim must exit 1 (EXIT_VERIFY_FAIL, not 3=undecidable); got {code}"
    );
    eprintln!(
        "GO_PRODUCTION_NEGATIVE_EXIT_CODE={code} STATUS={}",
        claim["status"]
    );

    let _ = fs::remove_dir_all(&project);
}
