// SPDX-License-Identifier: Apache-2.0
//
// End-to-end body-discharge proof for `provekit verify` (PR-22, #1440).
//
// THE LOAD-BEARING TEST. Unlike `cmd_verify_integration.rs` (whose claims
// are body-INDEPENDENT tautologies `n >= n` / `n > n` that discharge in
// pure LIA with no user symbol), this test proves the verification SPINE:
// that `verify` reduces a harvested assertion THROUGH the callee function
// BODY, so the solver sees the body's value-semantics instead of an
// uninterpreted symbol.
//
// The example: `fn double(x) = x * 2`, harvested test `assert_eq!(double(3), 6)`.
//
//   POSITIVE: the body-derived op-contract for `double` carries
//     `post = (result == *(x, 2))`. verify runs `wp(double(3), result==6)`
//     -> `*(3, 2) == 6` -> z3 discharges -> pass, signed witness minted.
//
//   NEGATIVE: break the body to `x * 3` -> `post = (result == *(x, 3))` ->
//     `wp(double(3), result==6)` -> `*(3, 3) == 6` -> z3 finds `9 != 6` SAT
//     on the negation -> Unsatisfied, exit 1, NO witness. The negative case
//     fails HONESTLY (Unsatisfied, not Undecidable) precisely because no
//     uninterpreted symbol survives the body reduction.
//
// Requires `z3` on PATH; skips the solver-dependent asserts otherwise.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use provekit_canonicalizer::{blake3_512_of, encode_jcs};
use provekit_proof_envelope::{
    build_proof_envelope, ed25519_pubkey_string, Ed25519Seed, ProofEnvelopeInput,
};
use serde_json::{json, Value as Json};

fn unique_dir(suffix: &str) -> PathBuf {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let p = std::env::temp_dir().join(format!("provekit-verify-body-{stamp}-{suffix}"));
    fs::create_dir_all(&p).expect("mkdir");
    p
}

/// Compute the v1.1-flat member CID + canonical bytes, exactly as
/// `provekit-verifier::load_all_proofs` re-derives it.
fn flat_member(mut env: Json) -> (String, Vec<u8>) {
    if let Json::Object(map) = &mut env {
        map.remove("cid");
        map.remove("producerSignature");
    }
    let canonical = json_to_canonical_jcs(&env);
    let cid = blake3_512_of(canonical.as_bytes());
    (cid, canonical.into_bytes())
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
            Json::Object(map) => {
                CV::object(map.iter().map(|(k, v)| (k.clone(), to_cv(v))).collect::<Vec<_>>())
            }
        }
    }
    encode_jcs(&to_cv(j))
}

/// An Int constant IR term.
fn int_const(n: i64) -> Json {
    json!({"kind": "const", "value": n, "sort": {"kind": "primitive", "name": "Int"}})
}

/// Publish a `.proof` catalog proving (or refuting) `double(3) == 6`,
/// where `double`'s body is `x * <body_factor>`.
///
/// Three members, in the v1.1-flat `evidence.body` shape:
///   - TARGET contract `double`: the body-derived op-contract. Its `post`
///     is `result == *(x, <body_factor>)` (the lifted body) and it carries
///     `formals: ["x"]`. This is what the catalog resolver projects into an
///     `OpContractInfo` for wp.
///   - SOURCE contract: carries the harvested assertion
///     `=(double(3), 6)` in its `inv` slot (one bridged callsite).
///   - BRIDGE: `sourceSymbol "double" -> targetContractCid <double>`.
fn publish_double_project(suffix: &str, body_factor: i64) -> PathBuf {
    let dir = unique_dir(suffix);
    let proof_dir = dir.join(".provekit");
    fs::create_dir_all(&proof_dir).expect("mkdir .provekit");

    let signer_seed: Ed25519Seed = [0x42u8; 32];
    let declared_at = "2026-05-23T00:00:00.000Z";

    let mut members: BTreeMap<String, Vec<u8>> = BTreeMap::new();

    // The body-derived op-contract for `double`: post = result == *(x, k).
    let target_env = json!({
        "evidence": {
            "kind": "contract",
            "body": {
                "contractName": "double",
                "formals": ["x"],
                "formalSorts": [{"kind": "primitive", "name": "Int"}],
                "post": {
                    "kind": "atomic", "name": "=",
                    "args": [
                        {"kind": "var", "name": "result"},
                        {"kind": "ctor", "name": "*", "args": [
                            {"kind": "var", "name": "x"},
                            int_const(body_factor)
                        ]}
                    ]
                }
            }
        }
    });
    let (target_cid, target_bytes) = flat_member(target_env);
    members.insert(target_cid.clone(), target_bytes);

    // The harvested test assertion `assert_eq!(double(3), 6)` -> inv =(call, 6).
    let source_env = json!({
        "evidence": {
            "kind": "contract",
            "body": {
                "contractName": "double_test",
                "inv": {
                    "kind": "atomic", "name": "=",
                    "args": [
                        {"kind": "ctor", "name": "double", "args": [int_const(3)]},
                        int_const(6)
                    ]
                }
            }
        }
    });
    let (source_cid, source_bytes) = flat_member(source_env);
    members.insert(source_cid, source_bytes);

    // The bridge: double (sourceSymbol) -> the body-derived contract.
    let bridge_env = json!({
        "evidence": {
            "kind": "bridge",
            "body": {
                "sourceSymbol": "double",
                "sourceLayer": "rust",
                "targetContractCid": target_cid,
                "targetLayer": "rust-kit"
            }
        }
    });
    let (bridge_cid, bridge_bytes) = flat_member(bridge_env);
    members.insert(bridge_cid, bridge_bytes);

    let signer_pubkey = ed25519_pubkey_string(&signer_seed);
    let signer_cid = blake3_512_of(signer_pubkey.as_bytes());
    let input = ProofEnvelopeInput {
        name: format!("@test/verify-body-{suffix}"),
        version: "1.0.0".into(),
        binary_cid: None,
        metadata: None,
        members,
        signer_cid,
        signer_seed,
        declared_at: declared_at.into(),
    };
    let built = build_proof_envelope(&input);
    let hex = built.cid.strip_prefix("blake3-512:").unwrap();
    fs::write(proof_dir.join(format!("{hex}.proof")), &built.bytes).expect("write proof");
    dir
}

fn provekit_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_provekit"))
}

fn z3_available() -> bool {
    Command::new("z3")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
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

/// POSITIVE: `double(x) = x*2`, `assert_eq!(double(3), 6)`. The body
/// reduction yields `*(3, 2) == 6`; z3 discharges; a signed witness is
/// minted. This is the proof that verify discharges a real BODY-obligation
/// (not a body-independent tautology).
#[test]
fn verify_double_body_discharges_and_mints_witness() {
    if !z3_available() {
        eprintln!("z3 not on PATH: skipping body-discharge positive test");
        return;
    }
    let project = publish_double_project("pos", 2);
    let witnesses = project.join("witnesses-out");
    let (receipt, code) = run_verify_json_with_code(&project, &witnesses);

    assert_eq!(receipt["kind"], "verification-receipt");
    assert_eq!(
        receipt["totalClaims"], 1,
        "exactly one body-bearing callsite enumerated; receipt: {receipt}"
    );
    let claim = &receipt["claims"].as_array().expect("claims")[0];

    // The body reduction left a concrete LIA formula `*(3,2) == 6` — NO
    // uninterpreted `double` symbol. So z3 discharges it.
    assert_eq!(
        claim["pass"], true,
        "double(3)==6 must discharge through the body x*2; claim: {claim}"
    );
    assert_eq!(
        claim["status"], "discharged",
        "positive body-obligation must be discharged (not undecidable); claim: {claim}"
    );

    let solver = claim["dischargingSolver"].as_str().unwrap_or("");
    assert!(
        solver.starts_with("z3@"),
        "discharging solver must be z3; got `{solver}`"
    );

    // A signed witness was minted. Capture its CID for the #1440 report:
    //   cargo test verify_double_body_discharges -- --nocapture
    let witness_cid = claim["witnessCid"].as_str().expect("witness minted");
    assert!(witness_cid.starts_with("blake3-512:"));
    eprintln!("POSITIVE_WITNESS_CID={witness_cid}");

    let hex = witness_cid.trim_start_matches("blake3-512:");
    let wpath = witnesses.join(format!("witness-{hex}.json"));
    let bytes = fs::read_to_string(&wpath).expect("witness file written");
    let witness: provekit_ir_types::WitnessMemento =
        serde_json::from_str(&bytes).expect("witness re-parses");
    assert_eq!(witness.outcome, "pass");
    assert!(witness.signature.is_some(), "witness must be signed");

    assert_eq!(receipt["ok"], true);
    assert_eq!(code, 0, "positive run exits clean; got {code}");

    let _ = fs::remove_dir_all(&project);
}

/// NEGATIVE: break the body to `double(x) = x*3`. The body reduction
/// yields `*(3, 3) == 6`; z3 finds `9 != 6` SAT on the negation ->
/// Unsatisfied, exit 1, NO witness. This is THE load-bearing proof: the
/// negative case must fail HONESTLY (Unsatisfied, not Undecidable),
/// because no uninterpreted symbol survives the body reduction.
#[test]
fn verify_double_broken_body_fails_unsatisfied_no_witness() {
    if !z3_available() {
        eprintln!("z3 not on PATH: skipping body-discharge negative test");
        return;
    }
    let project = publish_double_project("neg", 3);
    let witnesses = project.join("witnesses-out");
    let (receipt, code) = run_verify_json_with_code(&project, &witnesses);

    assert_eq!(receipt["totalClaims"], 1, "receipt: {receipt}");
    let claim = &receipt["claims"].as_array().expect("claims")[0];

    // The decisive assertion: a BROKEN body makes the claim UNSATISFIED,
    // not UNDECIDABLE. Undecidable here would mean the callee stayed an
    // uninterpreted symbol (the #1440 bug); Unsatisfied means the body was
    // reduced and z3 actually refuted `*(3,3) == 6`.
    assert_eq!(
        claim["status"], "unsatisfied",
        "broken body x*3 must be UNSATISFIED (not undecidable); claim: {claim}"
    );
    assert_eq!(
        claim["pass"], false,
        "broken-body claim must not pass; claim: {claim}"
    );

    // No witness for a violated claim.
    assert!(
        claim["witnessCid"].is_null(),
        "no witness may be minted for a violated claim; claim: {claim}"
    );
    assert_eq!(receipt["ok"], false);

    let witness_files: Vec<_> = fs::read_dir(&witnesses)
        .map(|rd| rd.filter_map(|e| e.ok()).collect())
        .unwrap_or_default();
    assert!(
        witness_files.is_empty(),
        "witness dir must be empty for a violated claim; found {} files",
        witness_files.len()
    );

    // Exit 1 = EXIT_VERIFY_FAIL (a hard violation, not a solver miss/exit 3).
    assert_eq!(
        code, 1,
        "broken-body claim must exit 1 (EXIT_VERIFY_FAIL, not 3=undecidable); got {code}"
    );
    eprintln!("NEGATIVE_EXIT_CODE={code} STATUS={}", claim["status"]);

    let _ = fs::remove_dir_all(&project);
}
