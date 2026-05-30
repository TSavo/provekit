// SPDX-License-Identifier: Apache-2.0
//
// Tests for `mint_contract`. Pins:
//   - error when all of pre/post/inv are None (EmptyContract)
//   - error when out_binding is empty (EmptyOutBinding)
//   - every (pre, post, inv) combination accepted; produces stable
//     bindingHash + propertyHash for the same input
//   - preHash / postHash / invHash are DERIVED from the formula bytes
//     (caller can't supply them; recomputation catches forgery)
//   - propertyHash = BLAKE3-512(JCS({pre?, post?, inv?, outBinding}))
//   - bindingHash  = BLAKE3-512(JCS({producerId, contractName, propertyHash}))
//   - CID is "blake3-512:" + 128 hex chars

use std::sync::Arc;

use provekit_canonicalizer::{blake3_512_of, encode_jcs, Value};
use provekit_claim_envelope::{
    mint_contract, Authoring, ClaimEnvelopeError, MintContractArgs, MintedEnvelope,
};
use provekit_proof_envelope::Ed25519Seed;

fn seed() -> Ed25519Seed {
    [0x42u8; 32]
}

fn pre_n_gt_0() -> Arc<Value> {
    Value::object([
        ("kind", Value::string("forall")),
        ("name", Value::string("n")),
        (
            "sort",
            Value::object([
                ("kind", Value::string("primitive")),
                ("name", Value::string("Int")),
            ]),
        ),
        (
            "body",
            Value::object([
                ("kind", Value::string("atomic")),
                ("name", Value::string(">")),
                (
                    "args",
                    Value::array(vec![
                        Value::object([
                            ("kind", Value::string("var")),
                            ("name", Value::string("n")),
                        ]),
                        Value::object([
                            ("kind", Value::string("const")),
                            ("value", Value::integer(0)),
                            (
                                "sort",
                                Value::object([
                                    ("kind", Value::string("primitive")),
                                    ("name", Value::string("Int")),
                                ]),
                            ),
                        ]),
                    ]),
                ),
            ]),
        ),
    ])
}

fn post_out_eq_0() -> Arc<Value> {
    // Body: out = 0
    Value::object([
        ("kind", Value::string("atomic")),
        ("name", Value::string("=")),
        (
            "args",
            Value::array(vec![
                Value::object([
                    ("kind", Value::string("var")),
                    ("name", Value::string("out")),
                ]),
                Value::object([
                    ("kind", Value::string("const")),
                    ("value", Value::integer(0)),
                    (
                        "sort",
                        Value::object([
                            ("kind", Value::string("primitive")),
                            ("name", Value::string("Int")),
                        ]),
                    ),
                ]),
            ]),
        ),
    ])
}

fn inv_true() -> Arc<Value> {
    // True formula
    Value::object([
        ("kind", Value::string("and")),
        ("operands", Value::array(vec![])),
    ])
}

fn args_with(
    pre: Option<Arc<Value>>,
    post: Option<Arc<Value>>,
    inv: Option<Arc<Value>>,
) -> MintContractArgs {
    MintContractArgs {
        formals: Vec::new(),
        emit_empty_formals: false,
        formal_sorts: Vec::new(),
        library: None,
        contract_name: "demo".into(),
        pre,
        post,
        inv,
        out_binding: "out".into(),
        produced_by: "rust-test@1.0".into(),
        produced_at: "2026-04-30T00:00:00.000Z".into(),
        input_cids: vec![],
        authoring: Authoring::KitAuthor {
            author: "rust-test@1.0".into(),
            note: None,
        },
        signer_seed: seed(),
    }
}

// ---------------------------------------------------------------------------
// Failure modes
// ---------------------------------------------------------------------------

#[test]
fn errors_when_all_three_clauses_are_none() {
    let args = args_with(None, None, None);
    let r = mint_contract(&args);
    match r {
        Err(ClaimEnvelopeError::EmptyContract) => {}
        other => panic!("expected EmptyContract, got {other:?}"),
    }
}

#[test]
fn errors_when_out_binding_is_empty() {
    let mut args = args_with(Some(pre_n_gt_0()), None, None);
    args.out_binding = String::new();
    let r = mint_contract(&args);
    match r {
        Err(ClaimEnvelopeError::EmptyOutBinding) => {}
        other => panic!("expected EmptyOutBinding, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// Combination matrix: pre / post / inv individually + together
// ---------------------------------------------------------------------------

#[test]
fn pre_only_succeeds() {
    let m = mint_contract(&args_with(Some(pre_n_gt_0()), None, None)).expect("mint");
    assert!(m.cid.starts_with("blake3-512:"));
}

#[test]
fn post_only_succeeds() {
    let m = mint_contract(&args_with(None, Some(post_out_eq_0()), None)).expect("mint");
    assert!(m.cid.starts_with("blake3-512:"));
}

#[test]
fn inv_only_succeeds() {
    let m = mint_contract(&args_with(None, None, Some(inv_true()))).expect("mint");
    assert!(m.cid.starts_with("blake3-512:"));
}

#[test]
fn pre_post_succeeds() {
    let m =
        mint_contract(&args_with(Some(pre_n_gt_0()), Some(post_out_eq_0()), None)).expect("mint");
    assert!(m.cid.starts_with("blake3-512:"));
}

#[test]
fn pre_inv_succeeds() {
    let m = mint_contract(&args_with(Some(pre_n_gt_0()), None, Some(inv_true()))).expect("mint");
    assert!(m.cid.starts_with("blake3-512:"));
}

#[test]
fn post_inv_succeeds() {
    let m = mint_contract(&args_with(None, Some(post_out_eq_0()), Some(inv_true()))).expect("mint");
    assert!(m.cid.starts_with("blake3-512:"));
}

#[test]
fn pre_post_inv_all_present_succeeds() {
    let m = mint_contract(&args_with(
        Some(pre_n_gt_0()),
        Some(post_out_eq_0()),
        Some(inv_true()),
    ))
    .expect("mint");
    assert!(m.cid.starts_with("blake3-512:"));
}

// ---------------------------------------------------------------------------
// CID shape
// ---------------------------------------------------------------------------

#[test]
fn cid_is_blake3_512_prefixed_and_correct_length() {
    let m = mint_contract(&args_with(Some(pre_n_gt_0()), None, None)).expect("mint");
    assert!(m.cid.starts_with("blake3-512:"));
    assert_eq!(m.cid.len(), "blake3-512:".len() + 128);
}

#[test]
fn cid_matches_blake3_of_jcs_envelope() {
    // Layered shape: the attestation CID is BLAKE3-512(JCS(envelope))
    // where `envelope` is the embedded {signer, declaredAt, signature}
    // sub-object after signing. Verifiers re-derive the CID from the
    // envelope alone; the trust root is the envelope hash, not a
    // strip-and-rehash of the whole memento.
    let m = mint_contract(&args_with(Some(pre_n_gt_0()), None, None)).expect("mint");
    let signed_text = std::str::from_utf8(&m.canonical_bytes).expect("utf8");
    let signed_json: serde_json::Value = serde_json::from_str(signed_text).expect("json parse");
    let envelope = signed_json
        .get("envelope")
        .expect("envelope present")
        .clone();
    let v = json_to_value(&envelope);
    let recomputed = blake3_512_of(encode_jcs(&v).as_bytes());
    assert_eq!(
        m.cid, recomputed,
        "cid must equal blake3_512(JCS(envelope))"
    );
}

fn json_to_value(j: &serde_json::Value) -> Arc<Value> {
    match j {
        serde_json::Value::Null => Value::null(),
        serde_json::Value::Bool(b) => Value::boolean(*b),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Value::integer(i)
            } else if let Some(u) = n.as_u64() {
                Value::integer(u as i64)
            } else if let Some(f) = n.as_f64() {
                Value::integer(f as i64)
            } else {
                Value::integer(0)
            }
        }
        serde_json::Value::String(s) => Value::string(s.clone()),
        serde_json::Value::Array(items) => {
            let v: Vec<_> = items.iter().map(json_to_value).collect();
            Value::array(v)
        }
        serde_json::Value::Object(map) => {
            let entries: Vec<(String, _)> = map
                .iter()
                .map(|(k, v)| (k.clone(), json_to_value(v)))
                .collect();
            Arc::new(Value::Object(entries))
        }
    }
}

// ---------------------------------------------------------------------------
// preHash / postHash / invHash are DERIVED from formula bytes
// ---------------------------------------------------------------------------

fn parse_envelope(m: &MintedEnvelope) -> serde_json::Value {
    serde_json::from_slice(&m.canonical_bytes).expect("json parse")
}

// preHash / postHash / invHash are pure tooling-convenience derivations
// from the formula bytes (the verifier doesn't read them; consumers
// can reconstruct them locally). Layered shape places them in
// `metadata`, where opaque-to-substrate body fields live.

#[test]
fn pre_hash_is_blake3_of_jcs_encoded_pre() {
    let m = mint_contract(&args_with(Some(pre_n_gt_0()), None, None)).expect("mint");
    let env = parse_envelope(&m);
    let metadata = env.pointer("/metadata").expect("metadata").clone();
    let pre_hash = metadata
        .get("preHash")
        .and_then(|v| v.as_str())
        .expect("preHash present");
    let expected = blake3_512_of(encode_jcs(&pre_n_gt_0()).as_bytes());
    assert_eq!(pre_hash, expected);
}

#[test]
fn post_hash_is_blake3_of_jcs_encoded_post() {
    let m = mint_contract(&args_with(None, Some(post_out_eq_0()), None)).expect("mint");
    let env = parse_envelope(&m);
    let metadata = env.pointer("/metadata").expect("metadata").clone();
    let post_hash = metadata
        .get("postHash")
        .and_then(|v| v.as_str())
        .expect("postHash");
    let expected = blake3_512_of(encode_jcs(&post_out_eq_0()).as_bytes());
    assert_eq!(post_hash, expected);
}

#[test]
fn inv_hash_is_blake3_of_jcs_encoded_inv() {
    let m = mint_contract(&args_with(None, None, Some(inv_true()))).expect("mint");
    let env = parse_envelope(&m);
    let metadata = env.pointer("/metadata").expect("metadata").clone();
    let inv_hash = metadata
        .get("invHash")
        .and_then(|v| v.as_str())
        .expect("invHash");
    let expected = blake3_512_of(encode_jcs(&inv_true()).as_bytes());
    assert_eq!(inv_hash, expected);
}

#[test]
fn omitted_clauses_omit_their_hash_fields() {
    let m = mint_contract(&args_with(Some(pre_n_gt_0()), None, None)).expect("mint");
    let env = parse_envelope(&m);
    let metadata = env.pointer("/metadata").expect("metadata");
    assert!(
        metadata.get("preHash").is_some(),
        "preHash should be present"
    );
    assert!(
        metadata.get("postHash").is_none(),
        "postHash should be absent"
    );
    assert!(
        metadata.get("invHash").is_none(),
        "invHash should be absent"
    );
}

// ---------------------------------------------------------------------------
// propertyHash + bindingHash derivation
// ---------------------------------------------------------------------------

#[test]
fn property_hash_is_blake3_of_jcs_pre_post_inv_outbinding() {
    let m = mint_contract(&args_with(
        Some(pre_n_gt_0()),
        Some(post_out_eq_0()),
        Some(inv_true()),
    ))
    .expect("mint");
    let env = parse_envelope(&m);
    let claimed = env
        .pointer("/header/propertyHash")
        .and_then(|v| v.as_str())
        .expect("propertyHash");

    // Recompute: hash(JCS({pre, post, inv, outBinding})) with insertion
    // order pre, post, inv, outBinding (matches mint.rs).
    let v = Arc::new(Value::Object(vec![
        ("pre".into(), pre_n_gt_0()),
        ("post".into(), post_out_eq_0()),
        ("inv".into(), inv_true()),
        ("outBinding".into(), Value::string("out")),
    ]));
    let expected = blake3_512_of(encode_jcs(&v).as_bytes());
    assert_eq!(claimed, expected);
}

#[test]
fn binding_hash_is_blake3_of_jcs_producer_id_contract_name_property_hash() {
    let m = mint_contract(&args_with(Some(pre_n_gt_0()), None, None)).expect("mint");
    let env = parse_envelope(&m);
    let property_hash = env
        .pointer("/header/propertyHash")
        .and_then(|v| v.as_str())
        .unwrap();
    let claimed = env
        .pointer("/header/bindingHash")
        .and_then(|v| v.as_str())
        .expect("bindingHash");

    let v = Value::object([
        ("producerId", Value::string("rust-test@1.0")),
        ("contractName", Value::string("demo")),
        ("propertyHash", Value::string(property_hash)),
    ]);
    let expected = blake3_512_of(encode_jcs(&v).as_bytes());
    assert_eq!(claimed, expected);
}

// ---------------------------------------------------------------------------
// Determinism + sensitivity
// ---------------------------------------------------------------------------

#[test]
fn same_inputs_produce_same_cid() {
    let a = mint_contract(&args_with(Some(pre_n_gt_0()), None, None)).expect("mint");
    let b = mint_contract(&args_with(Some(pre_n_gt_0()), None, None)).expect("mint");
    assert_eq!(a.cid, b.cid);
    assert_eq!(a.canonical_bytes, b.canonical_bytes);
}

#[test]
fn changing_pre_changes_cid() {
    let a = mint_contract(&args_with(Some(pre_n_gt_0()), None, None)).expect("mint");
    // Different pre formula
    let other_pre = Value::object([
        ("kind", Value::string("atomic")),
        ("name", Value::string("=")),
        ("args", Value::array(vec![])),
    ]);
    let b = mint_contract(&args_with(Some(other_pre), None, None)).expect("mint");
    assert_ne!(a.cid, b.cid);
}

#[test]
fn changing_contract_name_changes_binding_hash() {
    let a = mint_contract(&args_with(Some(pre_n_gt_0()), None, None)).expect("mint");
    let mut args_b = args_with(Some(pre_n_gt_0()), None, None);
    args_b.contract_name = "other".into();
    let b = mint_contract(&args_b).expect("mint");

    let env_a = parse_envelope(&a);
    let env_b = parse_envelope(&b);
    let bh_a = env_a
        .pointer("/header/bindingHash")
        .and_then(|v| v.as_str())
        .unwrap();
    let bh_b = env_b
        .pointer("/header/bindingHash")
        .and_then(|v| v.as_str())
        .unwrap();
    assert_ne!(bh_a, bh_b);
}

#[test]
fn changing_producer_id_changes_binding_hash_but_not_property_hash() {
    let a = mint_contract(&args_with(Some(pre_n_gt_0()), None, None)).expect("mint");
    let mut args_b = args_with(Some(pre_n_gt_0()), None, None);
    args_b.produced_by = "other-kit@2.0".into();
    let b = mint_contract(&args_b).expect("mint");

    let env_a = parse_envelope(&a);
    let env_b = parse_envelope(&b);
    let ph_a = env_a
        .pointer("/header/propertyHash")
        .and_then(|v| v.as_str())
        .unwrap();
    let ph_b = env_b
        .pointer("/header/propertyHash")
        .and_then(|v| v.as_str())
        .unwrap();
    assert_eq!(ph_a, ph_b, "propertyHash must be producer-independent");
    let bh_a = env_a
        .pointer("/header/bindingHash")
        .and_then(|v| v.as_str())
        .unwrap();
    let bh_b = env_b
        .pointer("/header/bindingHash")
        .and_then(|v| v.as_str())
        .unwrap();
    assert_ne!(bh_a, bh_b, "bindingHash must depend on producerId");
}

// ---------------------------------------------------------------------------
// Authoring blocks round-trip
// ---------------------------------------------------------------------------

#[test]
fn authoring_kit_author_round_trips() {
    let mut args = args_with(Some(pre_n_gt_0()), None, None);
    args.authoring = Authoring::KitAuthor {
        author: "alice".into(),
        note: Some("hand-authored".into()),
    };
    let m = mint_contract(&args).expect("mint");
    let env = parse_envelope(&m);
    let auth = env.pointer("/metadata/authoring").expect("authoring");
    assert_eq!(
        auth.get("producerKind").and_then(|v| v.as_str()),
        Some("kit-author")
    );
    assert_eq!(auth.get("author").and_then(|v| v.as_str()), Some("alice"));
    assert_eq!(
        auth.get("note").and_then(|v| v.as_str()),
        Some("hand-authored")
    );
}

#[test]
fn authoring_lift_round_trips() {
    let mut args = args_with(Some(pre_n_gt_0()), None, None);
    args.authoring = Authoring::Lift {
        lifter: "lift-kit@1.0".into(),
        evidence: "lifted from rust source".into(),
        source_cid: Some("blake3-512:source".into()),
    };
    let m = mint_contract(&args).expect("mint");
    let env = parse_envelope(&m);
    let auth = env.pointer("/metadata/authoring").expect("authoring");
    assert_eq!(
        auth.get("producerKind").and_then(|v| v.as_str()),
        Some("lift")
    );
    assert_eq!(
        auth.get("lifter").and_then(|v| v.as_str()),
        Some("lift-kit@1.0")
    );
    assert_eq!(
        auth.get("sourceCid").and_then(|v| v.as_str()),
        Some("blake3-512:source")
    );
}

#[test]
fn authoring_llm_round_trips() {
    let mut args = args_with(Some(pre_n_gt_0()), None, None);
    args.authoring = Authoring::Llm {
        llm: "claude".into(),
        llm_version: "opus-4.7".into(),
        prompt_cid: "blake3-512:prompt".into(),
        confidence: 0.9,
        rationale: Some("inferred from docs".into()),
    };
    let m = mint_contract(&args).expect("mint");
    let env = parse_envelope(&m);
    let auth = env.pointer("/metadata/authoring").expect("authoring");
    assert_eq!(
        auth.get("producerKind").and_then(|v| v.as_str()),
        Some("llm")
    );
    assert_eq!(auth.get("llm").and_then(|v| v.as_str()), Some("claude"));
    // confidence was scaled by 1000 and stored as integer
    assert_eq!(auth.get("confidence").and_then(|v| v.as_i64()), Some(900));
}
