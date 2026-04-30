// SPDX-License-Identifier: Apache-2.0
//
// provekit-claim-envelope
//
// `mint_contract` / `mint_bridge` / `mint_implication` build a signed
// memento envelope (the universal claim-envelope wrapper around a
// role-specific evidence body). Each returns `MintedEnvelope { canonical_bytes, cid }`.
//
// Mirrors implementations/cpp/provekit/claim-envelope/mint.cpp 1:1 with
// the v1.1.0 hash widening: every hash is BLAKE3-512 (full 64-byte
// digest, hex-encoded) carrying the `"blake3-512:"` prefix. CIDs are
// the same form: NO truncation.
//
// Per-formula hashes (preHash, postHash, invHash) and propertyHash /
// bindingHash are DERIVED here from the caller-supplied formula
// Values, never accepted from the caller. Validators recompute and
// reject mismatches.

use std::sync::Arc;

use provekit_canonicalizer::{blake3_512_of, encode_jcs, Value};
use provekit_proof_envelope::{ed25519_sign_string, Ed25519Seed};

#[derive(Debug, thiserror::Error)]
pub enum ClaimEnvelopeError {
    #[error("mint_contract: at least one of pre/post/inv must be present")]
    EmptyContract,
    #[error("mint_contract: outBinding must not be empty")]
    EmptyOutBinding,
    #[error("claim-envelope: {0}")]
    Other(String),
}

#[derive(Debug, Clone)]
pub struct MintedEnvelope {
    pub canonical_bytes: Vec<u8>,
    pub cid: String,
}

// Schema CIDs — placeholder full-shape blake3-512 strings tagged with
// the role so they don't collide. The catalog itself isn't on
// blake3-512 yet; once it lands these will be the real published
// schema CIDs.
const SCHEMA_CID_CONTRACT: &str =
    "blake3-512:00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000c01";
const SCHEMA_CID_BRIDGE: &str =
    "blake3-512:00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000c03";
const SCHEMA_CID_IMPLICATION: &str =
    "blake3-512:00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000c08";

// ----- DERIVED hash helpers --------------------------------------------------

fn hash_value(v: &Arc<Value>) -> String {
    let bytes = encode_jcs(v);
    blake3_512_of(bytes.as_bytes())
}

fn hash_string(s: &str) -> String {
    blake3_512_of(s.as_bytes())
}

// ----- Wrapper assembly ------------------------------------------------------

fn build_envelope_for_hashing(
    binding_hash: &str,
    property_hash: &str,
    verdict: &str,
    produced_by: &str,
    produced_at: &str,
    input_cids: &[String],
    evidence: Arc<Value>,
) -> Arc<Value> {
    // ORDERING: inputCids MUST be lex-sorted (spec wrapper ORDERING).
    let mut sorted: Vec<String> = input_cids.to_vec();
    sorted.sort();
    let cids_arr: Vec<Arc<Value>> = sorted.into_iter().map(Value::string).collect();

    Value::object([
        ("schemaVersion", Value::string("1")),
        ("bindingHash", Value::string(binding_hash)),
        ("propertyHash", Value::string(property_hash)),
        ("verdict", Value::string(verdict)),
        ("producedBy", Value::string(produced_by)),
        ("producedAt", Value::string(produced_at)),
        ("inputCids", Value::array(cids_arr)),
        ("evidence", evidence),
    ])
}

fn mint_internal(
    binding_hash: &str,
    property_hash: &str,
    verdict: &str,
    produced_by: &str,
    produced_at: &str,
    input_cids: &[String],
    evidence: Arc<Value>,
    signer_seed: &Ed25519Seed,
) -> MintedEnvelope {
    // 1. Build the unsigned canonical envelope; hash it for the CID;
    //    sign the canonical-bytes; re-emit with cid + producerSignature
    //    appended.
    let unsigned_v = build_envelope_for_hashing(
        binding_hash,
        property_hash,
        verdict,
        produced_by,
        produced_at,
        input_cids,
        evidence,
    );
    let unsigned_canonical = encode_jcs(&unsigned_v);
    let cid = blake3_512_of(unsigned_canonical.as_bytes());
    let producer_sig = ed25519_sign_string(signer_seed, unsigned_canonical.as_bytes());

    // Re-emit: clone the unsigned object's entries, append cid and
    // producerSignature. JCS encoder re-sorts at emit time.
    let mut entries: Vec<(String, Arc<Value>)> = match unsigned_v.as_ref() {
        Value::Object(kvs) => kvs.clone(),
        _ => unreachable!("envelope should be an object"),
    };
    entries.push(("cid".into(), Value::string(cid.clone())));
    entries.push((
        "producerSignature".into(),
        Value::string(producer_sig),
    ));
    let signed_v = Arc::new(Value::Object(entries));
    let final_canonical = encode_jcs(&signed_v);
    MintedEnvelope {
        canonical_bytes: final_canonical.into_bytes(),
        cid,
    }
}

// =============================================================================
// Authoring (typed union mirrored from the C++ kit)
// =============================================================================

#[derive(Debug, Clone)]
pub enum Authoring {
    KitAuthor { author: String, note: Option<String> },
    Lift { lifter: String, evidence: String, source_cid: Option<String> },
    Llm { llm: String, llm_version: String, prompt_cid: String, confidence: f64, rationale: Option<String> },
}

fn authoring_to_value(a: &Authoring) -> Arc<Value> {
    match a {
        Authoring::KitAuthor { author, note } => {
            let mut entries: Vec<(String, Arc<Value>)> = vec![
                ("producerKind".into(), Value::string("kit-author")),
                ("author".into(), Value::string(author.clone())),
            ];
            if let Some(n) = note {
                if !n.is_empty() {
                    entries.push(("note".into(), Value::string(n.clone())));
                }
            }
            Arc::new(Value::Object(entries))
        }
        Authoring::Lift { lifter, evidence, source_cid } => {
            let mut entries: Vec<(String, Arc<Value>)> = vec![
                ("producerKind".into(), Value::string("lift")),
                ("lifter".into(), Value::string(lifter.clone())),
                ("evidence".into(), Value::string(evidence.clone())),
            ];
            if let Some(c) = source_cid {
                if !c.is_empty() {
                    entries.push(("sourceCid".into(), Value::string(c.clone())));
                }
            }
            Arc::new(Value::Object(entries))
        }
        Authoring::Llm { llm, llm_version, prompt_cid, confidence, rationale } => {
            let mut entries: Vec<(String, Arc<Value>)> = vec![
                ("producerKind".into(), Value::string("llm")),
                ("llm".into(), Value::string(llm.clone())),
                ("llmVersion".into(), Value::string(llm_version.clone())),
                ("promptCid".into(), Value::string(prompt_cid.clone())),
                ("confidence".into(), Value::integer((confidence * 1000.0) as i64)),
            ];
            if let Some(r) = rationale {
                if !r.is_empty() {
                    entries.push(("rationale".into(), Value::string(r.clone())));
                }
            }
            Arc::new(Value::Object(entries))
        }
    }
}

// =============================================================================
// mint_contract
// =============================================================================

pub struct MintContractArgs {
    pub contract_name: String,
    pub pre: Option<Arc<Value>>,
    pub post: Option<Arc<Value>>,
    pub inv: Option<Arc<Value>>,
    pub out_binding: String,
    pub produced_by: String,
    pub produced_at: String,
    pub input_cids: Vec<String>,
    pub authoring: Authoring,
    pub signer_seed: Ed25519Seed,
}

pub fn mint_contract(args: &MintContractArgs) -> Result<MintedEnvelope, ClaimEnvelopeError> {
    if args.pre.is_none() && args.post.is_none() && args.inv.is_none() {
        return Err(ClaimEnvelopeError::EmptyContract);
    }
    if args.out_binding.is_empty() {
        return Err(ClaimEnvelopeError::EmptyOutBinding);
    }

    // Build evidence.body. Insertion order mirrors C++ kit.
    let mut body_kvs: Vec<(String, Arc<Value>)> = vec![
        ("contractName".into(), Value::string(args.contract_name.clone())),
        ("outBinding".into(), Value::string(args.out_binding.clone())),
    ];
    if let Some(pre) = &args.pre {
        body_kvs.push(("pre".into(), pre.clone()));
        body_kvs.push(("preHash".into(), Value::string(hash_value(pre))));
    }
    if let Some(post) = &args.post {
        body_kvs.push(("post".into(), post.clone()));
        body_kvs.push(("postHash".into(), Value::string(hash_value(post))));
    }
    if let Some(inv) = &args.inv {
        body_kvs.push(("inv".into(), inv.clone()));
        body_kvs.push(("invHash".into(), Value::string(hash_value(inv))));
    }
    body_kvs.push(("authoring".into(), authoring_to_value(&args.authoring)));
    let body = Arc::new(Value::Object(body_kvs));

    let evidence = Value::object([
        ("kind", Value::string("contract")),
        ("schema", Value::string(SCHEMA_CID_CONTRACT)),
        ("body", body),
    ]);

    // DERIVED:
    //   propertyHash = hash(canonical({pre?, post?, inv?, outBinding}))
    //   bindingHash  = hash(canonical({producerId, contractName, propertyHash}))
    let mut ph_kvs: Vec<(String, Arc<Value>)> = Vec::new();
    if let Some(pre) = &args.pre {
        ph_kvs.push(("pre".into(), pre.clone()));
    }
    if let Some(post) = &args.post {
        ph_kvs.push(("post".into(), post.clone()));
    }
    if let Some(inv) = &args.inv {
        ph_kvs.push(("inv".into(), inv.clone()));
    }
    ph_kvs.push((
        "outBinding".into(),
        Value::string(args.out_binding.clone()),
    ));
    let property_hash = hash_value(&Arc::new(Value::Object(ph_kvs)));

    let bh_obj = Value::object([
        ("producerId", Value::string(args.produced_by.clone())),
        ("contractName", Value::string(args.contract_name.clone())),
        ("propertyHash", Value::string(property_hash.clone())),
    ]);
    let binding_hash = hash_value(&bh_obj);

    Ok(mint_internal(
        &binding_hash,
        &property_hash,
        "holds",
        &args.produced_by,
        &args.produced_at,
        &args.input_cids,
        evidence,
        &args.signer_seed,
    ))
}

// =============================================================================
// mint_bridge
// =============================================================================

pub struct MintBridgeArgs {
    pub produced_by: String,
    pub produced_at: String,
    pub source_symbol: String,
    pub source_layer: String,
    pub target_contract_cid: String,
    pub target_layer: String,
    pub ir_arg_sorts: Vec<String>,
    pub ir_return_sort: String,
    pub notes: String,
    pub signer_seed: Ed25519Seed,
}

pub fn mint_bridge(args: &MintBridgeArgs) -> MintedEnvelope {
    let arg_sorts: Vec<Arc<Value>> = args
        .ir_arg_sorts
        .iter()
        .map(|s| Value::string(s.clone()))
        .collect();

    let mut body_kvs: Vec<(String, Arc<Value>)> = vec![
        ("sourceSymbol".into(), Value::string(args.source_symbol.clone())),
        ("sourceLayer".into(), Value::string(args.source_layer.clone())),
        ("targetContractCid".into(), Value::string(args.target_contract_cid.clone())),
        ("targetLayer".into(), Value::string(args.target_layer.clone())),
        ("irArgSorts".into(), Value::array(arg_sorts)),
        ("irReturnSort".into(), Value::string(args.ir_return_sort.clone())),
    ];
    if !args.notes.is_empty() {
        body_kvs.push(("notes".into(), Value::string(args.notes.clone())));
    }
    let body = Arc::new(Value::Object(body_kvs));

    let evidence = Value::object([
        ("kind", Value::string("bridge")),
        ("schema", Value::string(SCHEMA_CID_BRIDGE)),
        ("body", body),
    ]);

    // DERIVED per spec:
    //   bindingHash  = hash(canonical({sourceLayer, sourceSymbol}))
    //   propertyHash = hash("bridge:" || sourceSymbol)
    let bh_obj = Value::object([
        ("sourceLayer", Value::string(args.source_layer.clone())),
        ("sourceSymbol", Value::string(args.source_symbol.clone())),
    ]);
    let binding_hash = hash_value(&bh_obj);
    let property_hash = hash_string(&format!("bridge:{}", args.source_symbol));

    mint_internal(
        &binding_hash,
        &property_hash,
        "holds",
        &args.produced_by,
        &args.produced_at,
        &[args.target_contract_cid.clone()],
        evidence,
        &args.signer_seed,
    )
}

// =============================================================================
// mint_implication
// =============================================================================

pub struct MintImplicationArgs {
    pub produced_by: String,
    pub produced_at: String,
    pub antecedent_hash: String,
    pub consequent_hash: String,
    pub antecedent_cid: String,
    pub consequent_cid: String,
    pub antecedent_slot: String,
    pub consequent_slot: String,
    pub prover: String,
    pub prover_run_ms: i64,
    pub smt_lib_input: String,
    pub proof_witness: String,
    pub signer_seed: Ed25519Seed,
}

pub fn mint_implication(args: &MintImplicationArgs) -> MintedEnvelope {
    let mut body_kvs: Vec<(String, Arc<Value>)> = vec![
        ("antecedentHash".into(), Value::string(args.antecedent_hash.clone())),
        ("consequentHash".into(), Value::string(args.consequent_hash.clone())),
        ("antecedentCid".into(), Value::string(args.antecedent_cid.clone())),
        ("consequentCid".into(), Value::string(args.consequent_cid.clone())),
        ("antecedentSlot".into(), Value::string(args.antecedent_slot.clone())),
        ("consequentSlot".into(), Value::string(args.consequent_slot.clone())),
        ("prover".into(), Value::string(args.prover.clone())),
        ("proverRunMs".into(), Value::integer(args.prover_run_ms)),
    ];
    if !args.smt_lib_input.is_empty() {
        body_kvs.push(("smtLibInput".into(), Value::string(args.smt_lib_input.clone())));
    }
    if !args.proof_witness.is_empty() {
        body_kvs.push(("proofWitness".into(), Value::string(args.proof_witness.clone())));
    }
    let body = Arc::new(Value::Object(body_kvs));

    let evidence = Value::object([
        ("kind", Value::string("implication")),
        ("schema", Value::string(SCHEMA_CID_IMPLICATION)),
        ("body", body),
    ]);

    // DERIVED per spec:
    //   bindingHash  = hash(canonical({antecedentHash, consequentHash}))
    //   propertyHash = hash("implication:" || antecedentHash || ":" || consequentHash)
    let bh_obj = Value::object([
        ("antecedentHash", Value::string(args.antecedent_hash.clone())),
        ("consequentHash", Value::string(args.consequent_hash.clone())),
    ]);
    let binding_hash = hash_value(&bh_obj);
    let property_hash = hash_string(&format!(
        "implication:{}:{}",
        args.antecedent_hash, args.consequent_hash
    ));

    let input_cids = vec![args.antecedent_cid.clone(), args.consequent_cid.clone()];

    mint_internal(
        &binding_hash,
        &property_hash,
        "holds",
        &args.produced_by,
        &args.produced_at,
        &input_cids,
        evidence,
        &args.signer_seed,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dummy_seed() -> Ed25519Seed {
        [0x42; 32]
    }

    #[test]
    fn empty_contract_rejected() {
        let args = MintContractArgs {
            contract_name: "x".into(),
            pre: None,
            post: None,
            inv: None,
            out_binding: "out".into(),
            produced_by: "test".into(),
            produced_at: "2026-04-30T00:00:00.000Z".into(),
            input_cids: vec![],
            authoring: Authoring::KitAuthor {
                author: "test".into(),
                note: None,
            },
            signer_seed: dummy_seed(),
        };
        let r = mint_contract(&args);
        assert!(matches!(r, Err(ClaimEnvelopeError::EmptyContract)));
    }

    #[test]
    fn cid_is_blake3_512_prefixed() {
        let pre = Value::object([
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
                        ("sort", Value::object([
                            ("kind", Value::string("primitive")),
                            ("name", Value::string("Int")),
                        ])),
                    ]),
                ]),
            ),
        ]);
        let args = MintContractArgs {
            contract_name: "parseInt".into(),
            pre: Some(pre),
            post: None,
            inv: None,
            out_binding: "out".into(),
            produced_by: "rust-kit@1.0".into(),
            produced_at: "2026-04-30T00:00:00.000Z".into(),
            input_cids: vec![],
            authoring: Authoring::KitAuthor {
                author: "rust-kit@1.0".into(),
                note: None,
            },
            signer_seed: dummy_seed(),
        };
        let m = mint_contract(&args).expect("mint");
        assert!(m.cid.starts_with("blake3-512:"));
        assert_eq!(m.cid.len(), "blake3-512:".len() + 128);
    }
}
