// SPDX-License-Identifier: Apache-2.0
//
// Proof-file conformance tests. These are the first dogfood target:
// `.proof` bytes -> proof-file-format conformance report.

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use provekit_claim_envelope::{mint_contract, Authoring, MintContractArgs};
use provekit_ir_symbolic::serialize::formula_to_value;
use provekit_ir_symbolic::{forall, gt, must, num, reset_collector, Int};
use provekit_proof_envelope::{build_proof_envelope, Ed25519Seed, ProofEnvelopeInput};
use provekit_verifier::proof_conformance::{validate_proof_file, PFCP_R1_FILENAME_CID};

fn make_unique_dir(suffix: &str) -> PathBuf {
    let base = std::env::temp_dir();
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let p = base.join(format!("provekit-proof-conformance-{stamp}-{suffix}"));
    fs::create_dir_all(&p).expect("mkdir");
    p
}

fn fixture_proof_bytes() -> (String, Vec<u8>) {
    reset_collector();
    must("positiveInput", forall(Int(), |n| gt(n, num(0))));
    let declarations = provekit_ir_symbolic::finish();
    let declaration = declarations.first().expect("one declaration");
    let signer_seed: Ed25519Seed = [0x44u8; 32];
    let declared_at = "2026-05-06T00:00:00.000Z";
    let member = mint_contract(&MintContractArgs {
        contract_name: declaration.name.clone(),
        pre: declaration.pre.as_deref().map(formula_to_value),
        post: declaration.post.as_deref().map(formula_to_value),
        inv: declaration.inv.as_deref().map(formula_to_value),
        out_binding: declaration.out_binding.clone(),
        produced_by: "rust-test@1.0".into(),
        produced_at: declared_at.into(),
        input_cids: vec![],
        authoring: Authoring::KitAuthor {
            author: "rust-test@1.0".into(),
            note: None,
        },
        signer_seed,
    })
    .expect("mint member");
    let mut members = BTreeMap::new();
    members.insert(member.cid, member.canonical_bytes);

    let input = ProofEnvelopeInput {
        name: "@test/proof-conformance".into(),
        version: "1.0.0".into(),
        binary_cid: None,
        metadata: None,
        members,
        signer_cid: "blake3-512:signer".into(),
        signer_seed,
        declared_at: declared_at.into(),
    };
    let built = build_proof_envelope(&input);
    (built.cid, built.bytes)
}

#[test]
fn valid_proof_file_reports_conformant() {
    let dir = make_unique_dir("valid");
    let (cid, bytes) = fixture_proof_bytes();
    let hex = cid.strip_prefix("blake3-512:").expect("cid prefix");
    let path = dir.join(format!("{hex}.proof"));
    fs::write(&path, bytes).expect("write proof");

    let report = validate_proof_file(&path);

    assert!(report.ok(), "{report:#?}");
    assert_eq!(report.file_cid, cid);
    assert_eq!(report.member_count, 1);
    assert!(report.errors.is_empty());
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn filename_cid_mismatch_reports_rule_1() {
    let dir = make_unique_dir("filename-mismatch");
    let (_cid, bytes) = fixture_proof_bytes();
    let bogus_hex = "0".repeat(128);
    let path = dir.join(format!("{bogus_hex}.proof"));
    fs::write(&path, bytes).expect("write proof");

    let report = validate_proof_file(&path);

    assert!(!report.ok(), "{report:#?}");
    assert!(report
        .errors
        .iter()
        .any(|error| error.rule_id == PFCP_R1_FILENAME_CID));
    let _ = fs::remove_dir_all(&dir);
}
