// SPDX-License-Identifier: Apache-2.0
//
// Rust project implication sweep. These fixtures already mint callable
// Rust contracts; their project configs must include the consumer
// implication surface so prove is not vacuous.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

use provekit_canonicalizer::{blake3_512_of, encode_jcs};
use provekit_proof_envelope::{
    build_proof_envelope, ed25519_pubkey_string, Ed25519Seed, ProofEnvelopeInput,
};
use serde_json::{json, Value as Json};

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

fn build_rust_lifter_bins() {
    static BUILT: OnceLock<()> = OnceLock::new();
    BUILT.get_or_init(|| {
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
    });
}

fn unique_dir(suffix: &str) -> PathBuf {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let p = std::env::temp_dir().join(format!("provekit-rust-project-imp-{stamp}-{suffix}"));
    fs::create_dir_all(&p).expect("mkdir");
    p
}

fn run_mint(project: &Path, out_dir: &Path) {
    let output = Command::new(provekit_bin())
        .arg("mint")
        .arg("--project")
        .arg(project)
        .arg("--out")
        .arg(out_dir)
        .arg("--no-attest")
        .arg("--quiet")
        .arg("--json")
        .output()
        .expect("spawn provekit mint");
    assert!(
        output.status.success(),
        "provekit mint failed for {}\nstdout:\n{}\nstderr:\n{}",
        project.display(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn run_prove_json(out_dir: &Path) -> (Json, i32) {
    let output = Command::new(provekit_bin())
        .arg("prove")
        .arg(out_dir)
        .arg("--json")
        .output()
        .expect("spawn provekit prove");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let report: Json = serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("prove JSON parse failed: {e}\nstdout: {stdout}"));
    (report, output.status.code().unwrap_or(-1))
}

fn json_contains_str(value: &Json, needle: &str) -> bool {
    match value {
        Json::String(s) => s == needle,
        Json::Array(values) => values.iter().any(|v| json_contains_str(v, needle)),
        Json::Object(map) => map.values().any(|v| json_contains_str(v, needle)),
        _ => false,
    }
}

fn int_sort() -> Json {
    json!({"kind": "primitive", "name": "Int"})
}

fn int_const(n: i64) -> Json {
    json!({"kind": "const", "value": n, "sort": int_sort()})
}

fn var(name: &str) -> Json {
    json!({"kind": "var", "name": name})
}

fn json_to_canonical_jcs(j: &Json) -> String {
    fn to_cv(j: &Json) -> std::sync::Arc<provekit_canonicalizer::Value> {
        use provekit_canonicalizer::Value as CV;
        match j {
            Json::Null => CV::null(),
            Json::Bool(b) => CV::boolean(*b),
            Json::Number(n) => CV::integer(n.as_i64().unwrap_or(0)),
            Json::String(s) => CV::string(s.clone()),
            Json::Array(items) => CV::array(items.iter().map(to_cv).collect()),
            Json::Object(map) => CV::object(
                map.iter()
                    .map(|(k, v)| (k.clone(), to_cv(v)))
                    .collect::<Vec<_>>(),
            ),
        }
    }
    encode_jcs(&to_cv(j))
}

fn flat_member(mut env: Json) -> (String, Vec<u8>) {
    if let Json::Object(map) = &mut env {
        map.remove("cid");
        map.remove("producerSignature");
    }
    let canonical = json_to_canonical_jcs(&env);
    let cid = blake3_512_of(canonical.as_bytes());
    (cid, canonical.into_bytes())
}

fn write_proof(dir: &Path, name: &str, members: BTreeMap<String, Vec<u8>>) -> String {
    fs::create_dir_all(dir).expect("mkdir proof dir");
    let signer_seed: Ed25519Seed = [0x51u8; 32];
    let signer_pubkey = ed25519_pubkey_string(&signer_seed);
    let signer_cid = blake3_512_of(signer_pubkey.as_bytes());
    let built = build_proof_envelope(&ProofEnvelopeInput {
        name: name.to_string(),
        version: "1.0.0".into(),
        binary_cid: None,
        metadata: None,
        members,
        signer_cid,
        signer_seed,
        declared_at: "2026-05-29T00:00:00.000Z".into(),
    });
    let hex = built.cid.strip_prefix("blake3-512:").unwrap();
    fs::write(dir.join(format!("{hex}.proof")), &built.bytes).expect("write proof");
    built.cid
}

fn plant_contradictory_implication_proof(proof_dir: &Path) {
    let producer_env = json!({
        "evidence": {
            "kind": "contract",
            "body": {
                "contractName": "provekit_self_produces_zero",
                "post": {
                    "kind": "atomic",
                    "name": "=",
                    "args": [var("result"), int_const(0)]
                }
            }
        }
    });
    let (producer_cid, producer_bytes) = flat_member(producer_env);

    let consumer_env = json!({
        "evidence": {
            "kind": "contract",
            "body": {
                "contractName": "provekit_self_requires_positive",
                "formals": ["x"],
                "formalSorts": [int_sort()],
                "pre": {
                    "kind": "atomic",
                    "name": ">",
                    "args": [var("x"), int_const(0)]
                }
            }
        }
    });
    let (consumer_cid, consumer_bytes) = flat_member(consumer_env);

    let source_env = json!({
        "evidence": {
            "kind": "contract",
            "body": {
                "contractName": "provekit_self_contradictory_callsite",
                "inv": {
                    "kind": "atomic",
                    "name": "observed",
                    "args": [{
                        "kind": "ctor",
                        "name": "provekit_self_requires_positive",
                        "args": [{
                            "kind": "ctor",
                            "name": "provekit_self_produces_zero",
                            "args": []
                        }]
                    }]
                }
            }
        }
    });
    let (source_cid, source_bytes) = flat_member(source_env);

    let producer_bridge_env = json!({
        "evidence": {
            "kind": "bridge",
            "body": {
                "sourceSymbol": "provekit_self_produces_zero",
                "sourceLayer": "rust",
                "targetContractCid": producer_cid,
                "targetLayer": "rust-tests"
            }
        }
    });
    let (producer_bridge_cid, producer_bridge_bytes) = flat_member(producer_bridge_env);

    let consumer_bridge_env = json!({
        "evidence": {
            "kind": "bridge",
            "body": {
                "sourceSymbol": "provekit_self_requires_positive",
                "sourceLayer": "rust",
                "targetContractCid": consumer_cid,
                "targetLayer": "rust-tests"
            }
        }
    });
    let (consumer_bridge_cid, consumer_bridge_bytes) = flat_member(consumer_bridge_env);

    let mut members = BTreeMap::new();
    members.insert(producer_cid, producer_bytes);
    members.insert(consumer_cid, consumer_bytes);
    members.insert(source_cid, source_bytes);
    members.insert(producer_bridge_cid, producer_bridge_bytes);
    members.insert(consumer_bridge_cid, consumer_bridge_bytes);
    write_proof(
        proof_dir,
        "@provekit/self-contradictory-implication",
        members,
    );
}

#[test]
fn configured_rust_shims_emit_nonvacuous_implication_claims() {
    if !z3_available() {
        eprintln!("z3 not on PATH: skipping Rust project implication sweep");
        return;
    }
    build_rust_lifter_bins();

    let repo = repo_root();
    for project_rel in [
        "examples/provekit-shim-blake3-rust",
        "examples/provekit-shim-postgres",
        "examples/provekit-shim-rfc8785-jcs-rust",
        "examples/provekit-shim-rusqlite",
        "examples/provekit-shim-rust-std",
        "examples/provekit-shim-serde-json-rust",
    ] {
        let project = repo.join(project_rel);
        let out_dir = unique_dir(project_rel.rsplit('/').next().unwrap_or("project"));
        run_mint(&project, &out_dir);
        let (report, code) = run_prove_json(&out_dir);
        assert_eq!(
            code, 0,
            "{} should prove cleanly once implication bridges are minted; report: {report}",
            project_rel
        );
        let total = report["totalCallsites"].as_u64().unwrap_or(0);
        assert!(
            total > 0,
            "{} must not prove vacuously; report: {report}",
            project_rel
        );
        assert_eq!(report["violations"], 0, "{} report: {report}", project_rel);
        assert_eq!(
            report["discharged"], report["totalCallsites"],
            "{} report: {report}",
            project_rel
        );
        let _ = fs::remove_dir_all(out_dir);
    }
}

#[test]
fn voltron_demo_surfaces_nonvacuous_implication_refusals() {
    if !z3_available() {
        eprintln!("z3 not on PATH: skipping Voltron implication proof");
        return;
    }
    build_rust_lifter_bins();

    let repo = repo_root();
    let project = repo.join("examples/voltron-demo");
    let out_dir = unique_dir("voltron-demo");
    run_mint(&project, &out_dir);
    let (report, code) = run_prove_json(&out_dir);
    assert_eq!(
        code, 1,
        "Voltron has body-bearing implication claims that must refuse instead of passing vacuously; report: {report}"
    );
    let total = report["totalCallsites"].as_u64().unwrap_or(0);
    assert!(
        total > 0,
        "Voltron must not prove vacuously; report: {report}"
    );
    assert!(
        report["violations"].as_u64().unwrap_or(0) > 0,
        "Voltron must surface non-discharged claims; report: {report}"
    );
    assert_eq!(
        report["discharged"], 0,
        "Voltron must not sign witnesses for refused claims; report: {report}"
    );
    let rows = report["rows"].as_array().expect("rows");
    assert!(
        rows.iter().any(|row| {
            row["bridge"] == "insert_event"
                && row["reason"].as_str().is_some_and(|reason| {
                    reason.contains("body-discharge") && reason.contains("refuse")
                })
        }),
        "Voltron must surface the body-discharge refusal for insert_event; report: {report}"
    );
    let _ = fs::remove_dir_all(out_dir);
}

#[test]
fn rust_stdlib_shim_closes_option_result_constructor_lift_gaps() {
    build_rust_lifter_bins();

    let repo = repo_root();
    let shim = repo.join("examples/provekit-shim-rust-std");
    assert!(
        shim.join("Cargo.toml").exists(),
        "Rust stdlib lift-gaps must close through a real shim package at {}",
        shim.display()
    );

    let project = unique_dir("rust-std-consumer");
    let out_dir = unique_dir("rust-std-out");
    fs::create_dir_all(project.join("src")).expect("mkdir src");
    fs::write(
        project.join("Cargo.toml"),
        r#"[package]
name = "rust-std-consumer"
version = "0.1.0"
edition = "2021"
"#,
    )
    .expect("write Cargo.toml");
    fs::write(
        project.join("src/lib.rs"),
        r#"pub fn parse_flag(flag: bool) -> Result<Option<i32>, &'static str> {
    if flag {
        Ok(Some(1))
    } else {
        Err("disabled")
    }
}
"#,
    )
    .expect("write src/lib.rs");

    let provekit = project.join(".provekit");
    fs::create_dir_all(provekit.join("lift").join("rust-stdlib-contracts"))
        .expect("mkdir rust-stdlib-contracts manifest dir");
    fs::create_dir_all(provekit.join("lift").join("rust-implications"))
        .expect("mkdir rust-implications manifest dir");

    let bin_dir = provekit_bin()
        .parent()
        .expect("provekit bin parent")
        .to_path_buf();
    let walk_rpc = bin_dir.join("provekit-walk-rpc");
    fs::write(
        provekit.join("config.toml"),
        format!(
            r#"[[plugins]]
name = "rust-stdlib-contracts"
surface = "rust-stdlib-contracts"
workspace_override = "{}"
emit = "ir-document"

[[plugins]]
name = "rust-implications"
surface = "rust-implications"
"#,
            shim.display()
        ),
    )
    .expect("write config.toml");
    fs::write(
        provekit
            .join("lift")
            .join("rust-stdlib-contracts")
            .join("manifest.toml"),
        format!(
            "name = \"rust-stdlib-contracts\"\ncommand = [\"{}\", \"--rpc\"]\nworking_dir = \".\"\n",
            walk_rpc.display()
        ),
    )
    .expect("write rust-stdlib-contracts manifest");
    fs::write(
        provekit
            .join("lift")
            .join("rust-implications")
            .join("manifest.toml"),
        format!(
            "name = \"rust-implications\"\ncommand = [\"{}\", \"--rpc\"]\nworking_dir = \".\"\nmethod = \"provekit.plugin.lift_implications\"\nphase = \"consumer\"\n",
            walk_rpc.display()
        ),
    )
    .expect("write rust-implications manifest");

    let output = Command::new(provekit_bin())
        .arg("mint")
        .arg("--project")
        .arg(&project)
        .arg("--out")
        .arg(&out_dir)
        .arg("--no-attest")
        .arg("--quiet")
        .arg("--json")
        .output()
        .expect("spawn provekit mint");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "stdlib shim mint should succeed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    let report: Json = serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("mint JSON parse failed: {e}\nstdout: {stdout}"));
    assert!(
        !json_contains_str(&report["lift"], "lift-gap")
            && !json_contains_str(&report["lift"], "no-contract-for-callee"),
        "stdlib shim should close constructor lift-gaps, not report them: {report}"
    );

    let proof_files: Vec<PathBuf> = fs::read_dir(&out_dir)
        .expect("read out dir")
        .filter_map(|entry| {
            let path = entry.expect("out dir entry").path();
            (path.extension().and_then(|s| s.to_str()) == Some("proof")).then_some(path)
        })
        .collect();
    let pool = provekit_verifier::load_all_proofs::run_with_files(
        Path::new("/no-such-project"),
        &proof_files,
    );
    assert!(
        pool.load_errors.is_empty(),
        "stdlib shim proof bundle must load cleanly: {:?}",
        pool.load_errors
    );
    for symbol in ["Some", "Ok", "Err"] {
        let bridge = pool.bridges_by_symbol.get(symbol).unwrap_or_else(|| {
            panic!(
                "stdlib constructor `{symbol}` should have an implication bridge; indexed: {:?}",
                pool.bridges_by_symbol.keys().collect::<Vec<_>>()
            )
        });
        let target_cid = provekit_verifier::types::memento_body_field(bridge, "targetContractCid")
            .and_then(|v| v.as_str())
            .expect("bridge targetContractCid");
        let target = pool
            .mementos
            .get(target_cid)
            .unwrap_or_else(|| panic!("bridge target cid {target_cid} must resolve"));
        assert_eq!(
            provekit_verifier::types::memento_kind(target).as_deref(),
            Some("contract"),
            "bridge target for `{symbol}` must be a contract"
        );
    }

    let _ = fs::remove_dir_all(project);
    let _ = fs::remove_dir_all(out_dir);
}

#[test]
fn provekit_cli_self_application_proves_green_then_refuses_planted_contradiction() {
    if !z3_available() {
        eprintln!("z3 not on PATH: skipping provekit-cli self-application proof");
        return;
    }
    build_rust_lifter_bins();

    let repo = repo_root();
    let project = repo.join("implementations/rust/provekit-cli");
    let out_dir = unique_dir("provekit-cli");
    run_mint(&project, &out_dir);
    let (report, code) = run_prove_json(&out_dir);
    assert_eq!(
        code, 0,
        "provekit-cli should prove cleanly through its checked-in .provekit config; report: {report}"
    );
    let total = report["totalCallsites"].as_u64().unwrap_or(0);
    assert!(
        total > 0,
        "provekit-cli must not prove itself vacuously; report: {report}"
    );
    assert_eq!(report["violations"], 0, "provekit-cli report: {report}");
    assert_eq!(
        report["discharged"], report["totalCallsites"],
        "provekit-cli report: {report}"
    );
    plant_contradictory_implication_proof(&out_dir);

    let (report, code) = run_prove_json(&out_dir);
    assert_eq!(
        code, 1,
        "a planted contradictory implication must make the self proof fail; report: {report}"
    );
    assert!(
        report["totalCallsites"].as_u64().unwrap_or(0) > 0,
        "contradiction test must not be vacuous; report: {report}"
    );
    assert!(
        report["violations"].as_u64().unwrap_or(0) > 0,
        "contradictory implication must report a violation; report: {report}"
    );
    assert!(
        report["rows"]
            .as_array()
            .expect("rows")
            .iter()
            .any(|row| row["bridge"] == "provekit_self_requires_positive"
                && row["status"] == "unsatisfied"),
        "provekit_self_requires_positive(provekit_self_produces_zero()) must be unsatisfied; report: {report}"
    );
    let _ = fs::remove_dir_all(out_dir);
}
