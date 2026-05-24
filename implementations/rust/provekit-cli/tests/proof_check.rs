// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

use provekit_canonicalizer::blake3_512_of;
use provekit_claim_envelope::{mint_contract, Authoring, MintContractArgs};
use provekit_ir_symbolic::serialize::formula_to_value;
use provekit_ir_symbolic::{forall, gt, must, num, reset_collector, Int};
use provekit_proof_envelope::{
    build_proof_envelope, ed25519_pubkey_string, Ed25519Seed, ProofEnvelopeInput,
};

fn make_unique_dir(suffix: &str) -> PathBuf {
    let base = std::env::temp_dir();
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let p = base.join(format!("provekit-cli-proof-check-{stamp}-{suffix}"));
    fs::create_dir_all(&p).expect("mkdir");
    p
}

fn fixture_proof_file(dir: &PathBuf, name: &str, seed_byte: u8) -> (String, PathBuf) {
    fixture_proof_file_with_metadata(dir, name, seed_byte, None)
}

fn fixture_proof_file_with_metadata(
    dir: &PathBuf,
    name: &str,
    seed_byte: u8,
    metadata: Option<BTreeMap<String, String>>,
) -> (String, PathBuf) {
    reset_collector();
    must("positiveInput", forall(Int(), |n| gt(n, num(0))));
    let declarations = provekit_ir_symbolic::finish();
    let declaration = declarations.first().expect("one declaration");
    let signer_seed: Ed25519Seed = [seed_byte; 32];
    let declared_at = "2026-05-06T00:00:00.000Z";
    let member = mint_contract(&MintContractArgs {
        formals: Vec::new(),
        formal_sorts: Vec::new(),
        contract_name: declaration.name.clone(),
        pre: declaration.pre.as_deref().map(formula_to_value),
        post: declaration.post.as_deref().map(formula_to_value),
        inv: declaration.inv.as_deref().map(formula_to_value),
        out_binding: declaration.out_binding.clone(),
        produced_by: "rust-cli-test@1.0".into(),
        produced_at: declared_at.into(),
        input_cids: vec![],
        authoring: Authoring::KitAuthor {
            author: "rust-cli-test@1.0".into(),
            note: None,
        },
        signer_seed,
    })
    .expect("mint member");
    let mut members = BTreeMap::new();
    members.insert(member.cid, member.canonical_bytes);

    let built = build_proof_envelope(&ProofEnvelopeInput {
        name: name.into(),
        version: "1.0.0".into(),
        binary_cid: None,
        metadata,
        members,
        signer_cid: ed25519_pubkey_string(&signer_seed),
        signer_seed,
        declared_at: declared_at.into(),
    });
    let hex = built.cid.strip_prefix("blake3-512:").expect("cid prefix");
    let path = dir.join(format!("{hex}.proof"));
    fs::write(&path, built.bytes).expect("write proof");
    (built.cid, path)
}

#[test]
fn proof_check_emits_witness_json_for_valid_proof() {
    let dir = make_unique_dir("valid");
    let (cid, path) = fixture_proof_file(&dir, "@test/proof-check-subject", 0x45);
    let (format_proof_cid, format_proof_path) =
        fixture_proof_file(&dir, "@provekit/proof-format", 0x46);

    let output = Command::new(env!("CARGO_BIN_EXE_provekit"))
        .arg("proof")
        .arg("check")
        .arg(&path)
        .arg("--proof-format-proof")
        .arg(&format_proof_path)
        .arg("--policy")
        .arg("builtin:proof-format-v0")
        .arg("--json")
        .output()
        .expect("run provekit proof check");

    assert!(
        output.status.success(),
        "status={:?}\nstdout={}\nstderr={}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).expect("witness JSON");
    assert_eq!(json["kind"], "ProofFormatConformanceWitness");
    assert_eq!(json["result"], true);
    assert_eq!(json["subject_cid"], cid);
    assert_eq!(json["format_proof_cid"], format_proof_cid);
    assert_eq!(json["policy_cid"], "builtin:proof-format-v0");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn proof_implements_emits_program_protocol_witness() {
    let dir = make_unique_dir("implements");
    let (fixture_cid, fixture_path) =
        fixture_proof_file(&dir, "@provekit/proof-format-fixture", 0x47);
    let fixture_name = fixture_path
        .file_name()
        .and_then(|s| s.to_str())
        .expect("fixture filename");
    let manifest = serde_json::json!({
        "fixtures": [{
            "name": "valid-proof-format-fixture",
            "cid": fixture_cid,
            "path": fixture_name,
            "expected": true
        }]
    })
    .to_string();
    let mut metadata = BTreeMap::new();
    metadata.insert("provekit.proofProtocol.fixtures.v0".to_string(), manifest);
    let (protocol_cid, protocol_path) =
        fixture_proof_file_with_metadata(&dir, "@provekit/proof-format", 0x48, Some(metadata));
    let program = PathBuf::from(env!("CARGO_BIN_EXE_provekit"));
    let program_cid = blake3_512_of(&fs::read(&program).expect("read program"));

    let output = Command::new(&program)
        .arg("proof")
        .arg("implements")
        .arg("--program")
        .arg(&program)
        .arg("--proof-protocol")
        .arg(&protocol_path)
        .arg("--policy")
        .arg("builtin:proof-format-v0")
        .arg("--json")
        .output()
        .expect("run provekit proof implements");

    assert!(
        output.status.success(),
        "status={:?}\nstdout={}\nstderr={}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let json: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("implementation witness JSON");
    assert_eq!(json["kind"], "ProofProtocolImplementationWitness");
    assert_eq!(json["result"], true);
    assert_eq!(json["program_cid"], program_cid);
    assert_eq!(json["proof_protocol_cid"], protocol_cid);
    assert_eq!(json["policy_cid"], "builtin:proof-format-v0");
    assert_eq!(json["fixture_count"], 1);
    assert_eq!(json["checks"][0]["kind"], "ProofFormatConformanceWitness");
    assert_eq!(json["checks"][0]["subject_cid"], fixture_cid);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn proof_mint_protocol_generates_positive_and_negative_fixture_corpus() {
    let dir = make_unique_dir("mint-protocol");
    let program = PathBuf::from(env!("CARGO_BIN_EXE_provekit"));

    let mint = Command::new(&program)
        .arg("proof")
        .arg("mint-protocol")
        .arg("--out-dir")
        .arg(&dir)
        .arg("--json")
        .output()
        .expect("run provekit proof mint-protocol");

    assert!(
        mint.status.success(),
        "status={:?}\nstdout={}\nstderr={}",
        mint.status.code(),
        String::from_utf8_lossy(&mint.stdout),
        String::from_utf8_lossy(&mint.stderr)
    );
    let minted: serde_json::Value =
        serde_json::from_slice(&mint.stdout).expect("mint protocol JSON");
    assert_eq!(minted["kind"], "ProofProtocolCorpus");
    assert_eq!(minted["fixture_count"], 2);
    assert!(minted["fixtures"]
        .as_array()
        .expect("fixtures")
        .iter()
        .any(|fixture| fixture["expected"] == false));

    let protocol_path = PathBuf::from(minted["protocol_path"].as_str().expect("protocol_path"));
    assert!(protocol_path.exists(), "protocol proof should be written");

    let implements = Command::new(&program)
        .arg("proof")
        .arg("implements")
        .arg("--program")
        .arg(&program)
        .arg("--proof-protocol")
        .arg(&protocol_path)
        .arg("--policy")
        .arg("builtin:proof-format-v0")
        .arg("--json")
        .output()
        .expect("run generated corpus through implements");

    assert!(
        implements.status.success(),
        "status={:?}\nstdout={}\nstderr={}",
        implements.status.code(),
        String::from_utf8_lossy(&implements.stdout),
        String::from_utf8_lossy(&implements.stderr)
    );
    let witness: serde_json::Value =
        serde_json::from_slice(&implements.stdout).expect("implementation witness JSON");
    assert_eq!(witness["kind"], "ProofProtocolImplementationWitness");
    assert_eq!(witness["result"], true);
    assert_eq!(witness["fixture_count"], 2);
    assert!(witness["checks"]
        .as_array()
        .expect("checks")
        .iter()
        .any(|check| check["result"] == false));
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn checked_in_proof_protocol_corpus_is_usable() {
    let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("rust workspace")
        .parent()
        .expect("implementations")
        .parent()
        .expect("repo root")
        .to_path_buf();
    let corpus_dir = repo_root.join("protocol/conformance/proof-protocol");
    let protocol_cid =
        fs::read_to_string(corpus_dir.join("proof-protocol.cid.txt")).expect("read protocol cid");
    let protocol_cid = protocol_cid.trim();
    let protocol_path = corpus_dir.join(format!("{protocol_cid}.proof"));
    assert!(
        protocol_path.exists(),
        "checked-in protocol proof missing: {}",
        protocol_path.display()
    );

    let program = PathBuf::from(env!("CARGO_BIN_EXE_provekit"));
    let output = Command::new(&program)
        .arg("proof")
        .arg("implements")
        .arg("--program")
        .arg(&program)
        .arg("--proof-protocol")
        .arg(&protocol_path)
        .arg("--policy")
        .arg("builtin:proof-format-v0")
        .arg("--json")
        .output()
        .expect("run checked-in corpus through implements");

    assert!(
        output.status.success(),
        "status={:?}\nstdout={}\nstderr={}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let witness: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("implementation witness JSON");
    assert_eq!(witness["kind"], "ProofProtocolImplementationWitness");
    assert_eq!(witness["result"], true);
    assert_eq!(witness["proof_protocol_cid"], protocol_cid);
    assert_eq!(witness["fixture_count"], 2);
}
