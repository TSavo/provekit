// SPDX-License-Identifier: Apache-2.0
//
// Tests for `mint_bridge` and `mint_implication`. Pins:
//
// mint_bridge:
//   - bindingHash  = BLAKE3-512(JCS({sourceLayer, sourceSymbol}))
//   - propertyHash = BLAKE3-512("bridge:" + sourceSymbol)
//   - inputCids[0] == targetContractCid
//
// mint_implication:
//   - bindingHash  = BLAKE3-512(JCS({antecedentHash, consequentHash}))
//   - propertyHash = BLAKE3-512("implication:" + ah + ":" + ch)
//   - inputCids contains both antecedent and consequent CIDs (lex-sorted
//     by the wrapper)
//   - antecedentSlot / consequentSlot are stored verbatim (no validation)

use provekit_canonicalizer::{blake3_512_of, encode_jcs, Value};
use provekit_claim_envelope::{
    mint_bridge, mint_implication, MintBridgeArgs, MintImplicationArgs,
};
use provekit_proof_envelope::Ed25519Seed;

fn seed() -> Ed25519Seed {
    [0x42u8; 32]
}

fn parse(bytes: &[u8]) -> serde_json::Value {
    serde_json::from_slice(bytes).expect("json parse")
}

// ---------------------------------------------------------------------------
// mint_bridge
// ---------------------------------------------------------------------------

fn bridge_args() -> MintBridgeArgs {
    MintBridgeArgs {
        produced_by: "rust-test@1.0".into(),
        produced_at: "2026-04-30T00:00:00.000Z".into(),
        source_symbol: "parseInt".into(),
        source_layer: "ts".into(),
        target_contract_cid: "blake3-512:cccc".into(),
        target_layer: "rust-kit".into(),
        ir_arg_sorts: vec!["String".into()],
        ir_return_sort: "Int".into(),
        notes: String::new(),
        signer_seed: seed(),
    }
}

#[test]
fn bridge_cid_is_blake3_512_prefixed() {
    let m = mint_bridge(&bridge_args());
    assert!(m.cid.starts_with("blake3-512:"));
    assert_eq!(m.cid.len(), "blake3-512:".len() + 128);
}

#[test]
fn bridge_property_hash_is_blake3_of_bridge_prefix_plus_source_symbol() {
    let m = mint_bridge(&bridge_args());
    let env = parse(&m.canonical_bytes);
    let ph = env.get("propertyHash").and_then(|v| v.as_str()).unwrap();
    let expected = blake3_512_of(b"bridge:parseInt");
    assert_eq!(ph, expected);
}

#[test]
fn bridge_binding_hash_is_blake3_of_jcs_source_layer_and_source_symbol() {
    let m = mint_bridge(&bridge_args());
    let env = parse(&m.canonical_bytes);
    let bh = env.get("bindingHash").and_then(|v| v.as_str()).unwrap();

    let v = Value::object([
        ("sourceLayer", Value::string("ts")),
        ("sourceSymbol", Value::string("parseInt")),
    ]);
    let expected = blake3_512_of(encode_jcs(&v).as_bytes());
    assert_eq!(bh, expected);
}

#[test]
fn bridge_input_cids_first_entry_is_target_contract_cid() {
    let m = mint_bridge(&bridge_args());
    let env = parse(&m.canonical_bytes);
    let cids = env
        .get("inputCids")
        .and_then(|v| v.as_array())
        .expect("inputCids array");
    assert_eq!(cids.len(), 1);
    assert_eq!(cids[0].as_str(), Some("blake3-512:cccc"));
}

#[test]
fn bridge_evidence_kind_is_bridge() {
    let m = mint_bridge(&bridge_args());
    let env = parse(&m.canonical_bytes);
    let kind = env.pointer("/evidence/kind").and_then(|v| v.as_str()).unwrap();
    assert_eq!(kind, "bridge");
}

#[test]
fn bridge_body_carries_all_input_fields() {
    let m = mint_bridge(&bridge_args());
    let env = parse(&m.canonical_bytes);
    let body = env.pointer("/evidence/body").unwrap();
    assert_eq!(body.get("sourceSymbol").and_then(|v| v.as_str()), Some("parseInt"));
    assert_eq!(body.get("sourceLayer").and_then(|v| v.as_str()), Some("ts"));
    assert_eq!(
        body.get("targetContractCid").and_then(|v| v.as_str()),
        Some("blake3-512:cccc")
    );
    assert_eq!(body.get("targetLayer").and_then(|v| v.as_str()), Some("rust-kit"));
    assert_eq!(body.get("irReturnSort").and_then(|v| v.as_str()), Some("Int"));
    let arg_sorts = body.get("irArgSorts").and_then(|v| v.as_array()).unwrap();
    assert_eq!(arg_sorts.len(), 1);
    assert_eq!(arg_sorts[0].as_str(), Some("String"));
}

#[test]
fn bridge_notes_omitted_when_empty() {
    let m = mint_bridge(&bridge_args());
    let env = parse(&m.canonical_bytes);
    let body = env.pointer("/evidence/body").unwrap();
    assert!(body.get("notes").is_none());
}

#[test]
fn bridge_notes_included_when_provided() {
    let mut a = bridge_args();
    a.notes = "smoke from kit".into();
    let m = mint_bridge(&a);
    let env = parse(&m.canonical_bytes);
    let body = env.pointer("/evidence/body").unwrap();
    assert_eq!(body.get("notes").and_then(|v| v.as_str()), Some("smoke from kit"));
}

#[test]
fn bridge_is_deterministic() {
    let a = mint_bridge(&bridge_args());
    let b = mint_bridge(&bridge_args());
    assert_eq!(a.cid, b.cid);
    assert_eq!(a.canonical_bytes, b.canonical_bytes);
}

#[test]
fn bridge_changing_source_symbol_changes_property_hash() {
    let mut a = bridge_args();
    let mut b = bridge_args();
    b.source_symbol = "atoi".into();
    let m_a = mint_bridge(&a);
    let m_b = mint_bridge(&b);
    let env_a = parse(&m_a.canonical_bytes);
    let env_b = parse(&m_b.canonical_bytes);
    let ph_a = env_a.get("propertyHash").and_then(|v| v.as_str()).unwrap();
    let ph_b = env_b.get("propertyHash").and_then(|v| v.as_str()).unwrap();
    assert_ne!(ph_a, ph_b);
    a.source_symbol = "x".into();
    let _ = a;
}

// ---------------------------------------------------------------------------
// mint_implication
// ---------------------------------------------------------------------------

fn impl_args() -> MintImplicationArgs {
    MintImplicationArgs {
        produced_by: "z3".into(),
        produced_at: "2026-04-30T00:00:00.000Z".into(),
        antecedent_hash: "blake3-512:aaa".into(),
        consequent_hash: "blake3-512:ccc".into(),
        antecedent_cid: "blake3-512:zzz".into(),
        consequent_cid: "blake3-512:bbb".into(),
        antecedent_slot: "pre".into(),
        consequent_slot: "post".into(),
        prover: "z3@4.13".into(),
        prover_run_ms: 42,
        smt_lib_input: String::new(),
        proof_witness: String::new(),
        signer_seed: seed(),
    }
}

#[test]
fn implication_cid_is_blake3_512_prefixed() {
    let m = mint_implication(&impl_args());
    assert!(m.cid.starts_with("blake3-512:"));
    assert_eq!(m.cid.len(), "blake3-512:".len() + 128);
}

#[test]
fn implication_evidence_kind_is_implication() {
    let m = mint_implication(&impl_args());
    let env = parse(&m.canonical_bytes);
    let kind = env.pointer("/evidence/kind").and_then(|v| v.as_str()).unwrap();
    assert_eq!(kind, "implication");
}

#[test]
fn implication_property_hash_is_blake3_of_implication_prefix_plus_hashes() {
    let m = mint_implication(&impl_args());
    let env = parse(&m.canonical_bytes);
    let ph = env.get("propertyHash").and_then(|v| v.as_str()).unwrap();
    let expected = blake3_512_of(b"implication:blake3-512:aaa:blake3-512:ccc");
    assert_eq!(ph, expected);
}

#[test]
fn implication_binding_hash_is_blake3_of_jcs_antecedent_consequent_hashes() {
    let m = mint_implication(&impl_args());
    let env = parse(&m.canonical_bytes);
    let bh = env.get("bindingHash").and_then(|v| v.as_str()).unwrap();

    let v = Value::object([
        ("antecedentHash", Value::string("blake3-512:aaa")),
        ("consequentHash", Value::string("blake3-512:ccc")),
    ]);
    let expected = blake3_512_of(encode_jcs(&v).as_bytes());
    assert_eq!(bh, expected);
}

#[test]
fn implication_input_cids_contain_both_antecedent_and_consequent_lex_sorted() {
    let m = mint_implication(&impl_args());
    let env = parse(&m.canonical_bytes);
    let cids = env.get("inputCids").and_then(|v| v.as_array()).expect("array");
    // antecedent_cid="zzz", consequent_cid="bbb"; envelope wrapper sorts.
    assert_eq!(cids.len(), 2);
    assert_eq!(cids[0].as_str(), Some("blake3-512:bbb"));
    assert_eq!(cids[1].as_str(), Some("blake3-512:zzz"));
}

#[test]
fn implication_body_carries_slots_verbatim() {
    let m = mint_implication(&impl_args());
    let env = parse(&m.canonical_bytes);
    let body = env.pointer("/evidence/body").unwrap();
    assert_eq!(body.get("antecedentSlot").and_then(|v| v.as_str()), Some("pre"));
    assert_eq!(body.get("consequentSlot").and_then(|v| v.as_str()), Some("post"));
}

#[test]
fn implication_smt_input_omitted_when_empty() {
    let m = mint_implication(&impl_args());
    let env = parse(&m.canonical_bytes);
    let body = env.pointer("/evidence/body").unwrap();
    assert!(body.get("smtLibInput").is_none());
    assert!(body.get("proofWitness").is_none());
}

#[test]
fn implication_smt_input_included_when_provided() {
    let mut a = impl_args();
    a.smt_lib_input = "(declare-const x Int)\n(check-sat)".into();
    a.proof_witness = "(unsat)".into();
    let m = mint_implication(&a);
    let env = parse(&m.canonical_bytes);
    let body = env.pointer("/evidence/body").unwrap();
    assert_eq!(
        body.get("smtLibInput").and_then(|v| v.as_str()),
        Some("(declare-const x Int)\n(check-sat)")
    );
    assert_eq!(body.get("proofWitness").and_then(|v| v.as_str()), Some("(unsat)"));
}

#[test]
fn implication_prover_run_ms_round_trips() {
    let m = mint_implication(&impl_args());
    let env = parse(&m.canonical_bytes);
    let body = env.pointer("/evidence/body").unwrap();
    assert_eq!(body.get("proverRunMs").and_then(|v| v.as_i64()), Some(42));
}

#[test]
fn implication_is_deterministic() {
    let a = mint_implication(&impl_args());
    let b = mint_implication(&impl_args());
    assert_eq!(a.cid, b.cid);
}

#[test]
fn implication_changing_antecedent_hash_changes_property_hash() {
    let a = mint_implication(&impl_args());
    let mut other = impl_args();
    other.antecedent_hash = "blake3-512:DIFFERENT".into();
    let b = mint_implication(&other);
    let env_a = parse(&a.canonical_bytes);
    let env_b = parse(&b.canonical_bytes);
    assert_ne!(
        env_a.get("propertyHash").and_then(|v| v.as_str()).unwrap(),
        env_b.get("propertyHash").and_then(|v| v.as_str()).unwrap()
    );
}

#[test]
fn implication_envelope_carries_producer_signature() {
    let m = mint_implication(&impl_args());
    let env = parse(&m.canonical_bytes);
    let sig = env.get("producerSignature").and_then(|v| v.as_str()).unwrap();
    assert!(sig.starts_with("ed25519:"));
}

#[test]
fn bridge_envelope_carries_producer_signature() {
    let m = mint_bridge(&bridge_args());
    let env = parse(&m.canonical_bytes);
    let sig = env.get("producerSignature").and_then(|v| v.as_str()).unwrap();
    assert!(sig.starts_with("ed25519:"));
}
