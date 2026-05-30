// SPDX-License-Identifier: Apache-2.0
//
// PYTHON end-to-end production-bridge gauntlet -- the Python analog of
// `cmd_verify_go_production_bridge.rs`. Proves the verification spine is
// LANGUAGE-NEUTRAL for Python: a Python function's body-derived contract lifts
// to ProofIR (`post = result == (* x 2)` plus `formals`) and the verifier
// discharges the obligation through the body via wp + z3, exactly as for Rust,
// Java, and Go.
//
// This drives the REAL verify-facing Python lift surface
// `provekit-lift-python-verify` (module
// `provekit_lift_python_source.verify_rpc`). That surface lifts the real
// Python sources in `examples/python-double`:
//
//   - `double.py`       -> function-contract `post = result == (* x 2)`
//                          (verify-facing dialect: `python:mul` normalized to
//                          `*`, `return_value` -> `result`, `python:return`
//                          wrapper stripped, `Int` sorts from the `: int`
//                          annotation).
//   - `test_double.py`  -> contract `inv = =(double(3), 6)` (Python Layer-0
//                          leaf-assertion harvester).
//
// `provekit mint` AUTO-WRITES the `double -> targetContractCid` bridge (#1443);
// the bridge is TOOL-written, asserted by inspecting `pool.bridges_by_symbol`.
// `provekit verify` then discharges both ways:
//
//   POSITIVE: `double(3) == 6` reduces through the body `(* 3 2) == 6` -> z3
//     discharges -> pass, signed witness, exit 0.
//   NEGATIVE: break the body to `x * 3` -> `(* 3 3) == 6` -> z3 refutes ->
//     Unsatisfied, exit 1, NO witness.
//
// Requires `python3` and `z3` on PATH; skips the solver-dependent asserts if z3
// is absent, and skips entirely (with a loud eprintln) if python3 is absent.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value as Json;

#[path = "support/contradiction.rs"]
mod contradiction;

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

/// The verify-facing Python lifter's source tree, prepended to `sys.path` so
/// the binary resolves to THIS checkout's module (not a stale installed one).
fn python_lift_src() -> PathBuf {
    repo_root()
        .join("implementations")
        .join("python")
        .join("provekit-lift-python-source")
        .join("src")
}

fn z3_available() -> bool {
    Command::new("z3")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn python_available() -> bool {
    Command::new("python3")
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
    let p = std::env::temp_dir().join(format!("provekit-py-prod-bridge-{stamp}-{suffix}"));
    fs::create_dir_all(&p).expect("mkdir project");
    p
}

/// Write a small wrapper shell script that runs the verify-facing Python lift
/// surface with THIS checkout's `src` on `sys.path`, and return its path. The
/// Go test compiles a Go binary; Python is interpreted, so the analog is a
/// stable wrapper invoking `python3 -c "...; run_rpc()"`.
fn build_python_lift_verify() -> PathBuf {
    use std::io::Write as _;
    use std::sync::atomic::{AtomicU64, Ordering};
    // Unique per call: cargo runs the tests in this binary as parallel threads
    // of ONE process, so a `process::id()`-keyed path is SHARED across them.
    // One test exec'ing the wrapper while another truncates it for write =>
    // ETXTBSY (os error 26). An atomic counter gives each call its own path.
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let src = python_lift_src();
    let script = std::env::temp_dir().join(format!(
        "provekit-lift-python-verify-{}-{}.sh",
        std::process::id(),
        SEQ.fetch_add(1, Ordering::Relaxed)
    ));
    let body = format!(
        "#!/bin/sh\nexec python3 -c \"import sys; sys.path.insert(0, '{}'); \
         from provekit_lift_python_source.verify_rpc import run_rpc; run_rpc()\"\n",
        src.display()
    );
    // sync_all + drop the writer fd BEFORE chmod/spawn so `exec` never sees an
    // open writer (the second half of the ETXTBSY guard; see cli_surface.rs).
    {
        let mut f = fs::File::create(&script).expect("create python lift wrapper");
        f.write_all(body.as_bytes())
            .expect("write python lift wrapper");
        f.sync_all().expect("sync python lift wrapper");
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&script).expect("stat wrapper").permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&script, perms).expect("chmod wrapper");
    }
    script
}

/// Copy the in-repo `examples/python-double` into a fresh tempdir, write the
/// `python` lift manifest pointing at the wrapper script, and (for the negative
/// case) mutate the body multiplier. Returns the tempdir project.
fn stage_python_project(suffix: &str, lift_script: &Path, body_factor: i64) -> PathBuf {
    let example = repo_root().join("examples").join("python-double");
    let project = unique_dir(suffix);

    // double.py: the library, with the body multiplier (2 honest, 3 broken).
    let double_src = format!("def double(x: int) -> int:\n    return x * {body_factor}\n");
    fs::write(project.join("double.py"), double_src).expect("write double.py");

    // test_double.py: copied verbatim (the harvested `double(3) == 6`).
    fs::copy(
        example.join("test_double.py"),
        project.join("test_double.py"),
    )
    .expect("copy test_double.py");

    // .provekit/config.toml: copied verbatim.
    let provekit = project.join(".provekit");
    fs::create_dir_all(provekit.join("lift").join("python")).expect("mkdir .provekit/lift/python");
    fs::copy(
        example.join(".provekit").join("config.toml"),
        provekit.join("config.toml"),
    )
    .expect("copy config.toml");

    // .provekit/lift/python/manifest.toml: point command[0] at the wrapper.
    let manifest = format!(
        "name = \"python\"\ncommand = [\"{}\", \"--rpc\"]\nworking_dir = \".\"\n",
        lift_script.display()
    );
    fs::write(
        provekit.join("lift").join("python").join("manifest.toml"),
        manifest,
    )
    .expect("write manifest.toml");

    project
}

fn copy_dir_recursive(src: &Path, dst: &Path) {
    fs::create_dir_all(dst).unwrap_or_else(|_| panic!("mkdir {}", dst.display()));
    for entry in fs::read_dir(src).unwrap_or_else(|_| panic!("read {}", src.display())) {
        let entry = entry.expect("read dir entry");
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());
        if entry.file_type().expect("entry file type").is_dir() {
            copy_dir_recursive(&src_path, &dst_path);
        } else {
            fs::copy(&src_path, &dst_path).unwrap_or_else(|_| {
                panic!("copy {} -> {}", src_path.display(), dst_path.display())
            });
        }
    }
}

fn rewrite_manifest_command(manifest: &Path, command: &Path) {
    let text = fs::read_to_string(manifest)
        .unwrap_or_else(|_| panic!("read checked-in manifest {}", manifest.display()));
    let escaped = command
        .display()
        .to_string()
        .replace('\\', "\\\\")
        .replace('"', "\\\"");
    let rewritten = text
        .lines()
        .map(|line| {
            if line.trim_start().starts_with("command = ") {
                format!("command = [\"{escaped}\", \"--rpc\"]")
            } else {
                line.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    fs::write(manifest, format!("{rewritten}\n"))
        .unwrap_or_else(|_| panic!("write manifest {}", manifest.display()));
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
fn python_production_path_uses_checked_in_python_double_registration() {
    if !python_available() {
        eprintln!("python3 not on PATH: skipping checked-in python production-bridge test");
        return;
    }
    if !z3_available() {
        eprintln!("z3 not on PATH: skipping checked-in python production-bridge test");
        return;
    }
    let lift_script = build_python_lift_verify();
    let project = unique_dir("checked-in-registration");
    let example = repo_root().join("examples").join("python-double");
    fs::copy(example.join("double.py"), project.join("double.py")).expect("copy double.py");
    fs::copy(
        example.join("test_double.py"),
        project.join("test_double.py"),
    )
    .expect("copy test_double.py");
    copy_dir_recursive(&example.join(".provekit"), &project.join(".provekit"));
    rewrite_manifest_command(
        &project
            .join(".provekit")
            .join("lift")
            .join("python")
            .join("manifest.toml"),
        &lift_script,
    );

    run_mint(&project);

    let witnesses = project.join("witnesses-out");
    let (receipt, code) = run_verify_json_with_code(&project, &witnesses);
    assert_eq!(receipt["totalClaims"], 1, "receipt: {receipt}");
    assert_eq!(receipt["ok"], true, "receipt: {receipt}");
    assert_eq!(
        code, 0,
        "checked-in Python route must prove; receipt: {receipt}"
    );

    let _ = fs::remove_dir_all(&project);
}

/// THE bridge-writer assertion (language-neutral, runs without z3): the TOOL
/// wrote a `bridge` member for `double` whose `targetContractCid` resolves to a
/// `contract` member carrying the body-derived `formals` + `post`. No
/// hand-bridging anywhere; the bridge came from `provekit mint`.
#[test]
fn python_mint_auto_writes_body_discharge_bridge() {
    if !python_available() {
        eprintln!("python3 not on PATH: skipping python production-bridge bridge-writer test");
        return;
    }
    let lift_script = build_python_lift_verify();
    let project = stage_python_project("bridge", &lift_script, 2);
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

    let _ = fs::remove_dir_all(&project);
}

/// POSITIVE end-to-end: real Python lifter, tool-written bridge, verify
/// discharges through the body `(* x 2)`, signed witness, exit 0.
#[test]
fn python_production_path_double_discharges_and_mints_witness() {
    if !python_available() {
        eprintln!("python3 not on PATH: skipping python production-bridge positive test");
        return;
    }
    if !z3_available() {
        eprintln!("z3 not on PATH: skipping python production-bridge positive test");
        return;
    }
    let lift_script = build_python_lift_verify();
    let project = stage_python_project("pos", &lift_script, 2);
    run_mint(&project);

    let witnesses = project.join("witnesses-out");
    let (receipt, code) = run_verify_json_with_code(&project, &witnesses);

    assert_eq!(
        receipt["kind"], "verification-receipt",
        "receipt: {receipt}"
    );
    assert_eq!(
        receipt["totalClaims"], 1,
        "exactly one body-bearing callsite (the tool-written bridge made double(3) enumerate); receipt: {receipt}"
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
    eprintln!("PYTHON_PRODUCTION_POSITIVE_WITNESS_CID={witness_cid}");

    assert_eq!(receipt["ok"], true, "receipt: {receipt}");
    assert_eq!(code, 0, "positive run exits clean; got {code}");

    let _ = fs::remove_dir_all(&project);
}

/// NEGATIVE end-to-end: broken body `x * 3`, verify Unsatisfied, exit 1, no
/// witness. The decisive proof the tool-written bridge does not vacuously pass:
/// a real violation is caught honestly.
#[test]
fn python_production_path_broken_body_fails_unsatisfied_no_witness() {
    if !python_available() {
        eprintln!("python3 not on PATH: skipping python production-bridge negative test");
        return;
    }
    if !z3_available() {
        eprintln!("z3 not on PATH: skipping python production-bridge negative test");
        return;
    }
    let lift_script = build_python_lift_verify();
    let project = stage_python_project("neg", &lift_script, 3);
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
        "PYTHON_PRODUCTION_NEGATIVE_EXIT_CODE={code} STATUS={}",
        claim["status"]
    );

    let _ = fs::remove_dir_all(&project);
}

#[test]
fn python_production_path_refuses_planted_contradictory_implication() {
    if !python_available() {
        eprintln!("python3 not on PATH: skipping python contradictory-implication test");
        return;
    }
    if !z3_available() {
        eprintln!("z3 not on PATH: skipping python contradictory-implication test");
        return;
    }
    let lift_script = build_python_lift_verify();
    let project = stage_python_project("contradiction", &lift_script, 2);
    run_mint(&project);

    let (green, green_code) = contradiction::run_prove_json_with_code(&provekit_bin(), &project);
    assert_eq!(
        green_code, 0,
        "base Python project must prove before planting contradiction; report: {green}"
    );
    assert_eq!(green["totalCallsites"], 1, "green report: {green}");

    contradiction::plant_contradictory_implication_proof(
        &project.join(".provekit"),
        "python",
        "python-tests",
        "python_parity",
    );
    let (red, red_code) = contradiction::run_prove_json_with_code(&provekit_bin(), &project);
    contradiction::assert_prove_refuses_contradiction(
        &red,
        red_code,
        "python_parity_requires_positive",
    );

    let _ = fs::remove_dir_all(&project);
}
