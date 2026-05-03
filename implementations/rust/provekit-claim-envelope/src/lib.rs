// SPDX-License-Identifier: Apache-2.0
//
// provekit-claim-envelope
//
// `mint_contract` / `mint_bridge` / `mint_implication` build a signed
// memento in the v1.2 LAYERED shape introduced by
// `protocol/specs/2026-05-03-substrate-layers-envelope-header-body.md`:
//
//   { "envelope": {...}, "header": {...}, "metadata": {...} }
//
//   * envelope = { signer, declaredAt, signature }
//       The signature is computed over JCS({"header": header, "metadata": metadata}).
//       The envelope's CID (= attestation CID) is BLAKE3-512(JCS(envelope))
//       AFTER the signature has been embedded.
//
//   * header   = substrate-load-bearing data the verifier reads:
//                schemaVersion, kind, cid, plus kind-specific REQUIRED
//                fields (per the kind's normative spec) and the derived
//                hashes (bindingHash, propertyHash, verdict, inputCids)
//                used by the resolve/index pipeline.
//
//   * metadata = everything else (authoring attribution, lifecycle
//                strings like producedBy/producedAt, derived per-formula
//                hashes that are pure tooling convenience). Opaque to
//                the substrate verifier; signed transitively via the
//                envelope.
//
// Per spec §4: v1.1 flat-shape mementos remain valid as historical
// artifacts. New emissions adopt the layered shape and carry
// `schemaVersion: "2"` in the header. The verifier branches on
// `schemaVersion` at load time.

use std::sync::Arc;

use provekit_canonicalizer::{blake3_512_of, encode_jcs, Value};
use provekit_proof_envelope::{ed25519_pubkey_string, ed25519_sign_string, Ed25519Seed};

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
    /// JCS-canonical bytes of the full layered memento
    /// (`{envelope, header, metadata}`).
    pub canonical_bytes: Vec<u8>,
    /// The attestation CID: BLAKE3-512(JCS(envelope)) after the
    /// signature has been embedded. This identifies the SIGNED
    /// attestation and is what goes into the bundle members map.
    pub cid: String,
    /// The content CID: BLAKE3-512(JCS({name, outBinding, pre?, post?, inv?})).
    /// Signer-independent. Two distinct signers attesting to the same
    /// logical contract produce the same `contract_cid`. Only populated
    /// for contract mementos; empty string for bridges and implications.
    /// Per `protocol/specs/2026-05-03-contract-cid-vs-attestation-cid.md` §1.
    pub contract_cid: String,
}

/// The layered-shape schema version stamped into every memento header
/// emitted by this kit. Older flat mementos carry `"1"`; verifiers
/// branch on this string at load time.
pub const LAYERED_SCHEMA_VERSION: &str = "2";

// ---------- DERIVED hash helpers --------------------------------------------

fn hash_value(v: &Arc<Value>) -> String {
    let bytes = encode_jcs(v);
    blake3_512_of(bytes.as_bytes())
}

fn hash_string(s: &str) -> String {
    blake3_512_of(s.as_bytes())
}

// ---------- Envelope assembly -----------------------------------------------

/// Build the JCS-canonical bytes of `{"header": header, "metadata": metadata}`.
/// This is the message the envelope's Ed25519 signature covers (spec §2 R2).
fn signing_bytes(header: &Arc<Value>, metadata: &Arc<Value>) -> Vec<u8> {
    let msg = Value::object([
        ("header", header.clone()),
        ("metadata", metadata.clone()),
    ]);
    encode_jcs(&msg).into_bytes()
}

/// Assemble a layered memento, sign it, and compute the attestation CID
/// (= BLAKE3-512(JCS(envelope-with-signature))). Returns the JCS-canonical
/// bytes of the full `{envelope, header, metadata}` object alongside the CID.
/// `content_cid` is the signer-independent contract CID (empty for bridges/implications).
fn assemble_layered(
    header: Arc<Value>,
    metadata: Arc<Value>,
    declared_at: &str,
    signer_seed: &Ed25519Seed,
    content_cid: String,
) -> MintedEnvelope {
    let signer = ed25519_pubkey_string(signer_seed);
    let signing_msg = signing_bytes(&header, &metadata);
    let signature = ed25519_sign_string(signer_seed, &signing_msg);

    // Build the envelope object with the embedded signature; its JCS
    // hash is the attestation CID.
    let envelope = Value::object([
        ("signer", Value::string(signer.clone())),
        ("declaredAt", Value::string(declared_at.to_string())),
        ("signature", Value::string(signature.clone())),
    ]);
    let envelope_jcs = encode_jcs(&envelope);
    let attestation_cid = blake3_512_of(envelope_jcs.as_bytes());

    let memento = Value::object([
        ("envelope", envelope),
        ("header", header),
        ("metadata", metadata),
    ]);
    let memento_jcs = encode_jcs(&memento);

    MintedEnvelope {
        canonical_bytes: memento_jcs.into_bytes(),
        cid: attestation_cid,
        contract_cid: content_cid,
    }
}

/// Helper: build a header object from a vector of (key, value) pairs.
/// Always prepends `schemaVersion`, `kind`, `cid` in that order; the
/// kind-specific REQUIRED header fields follow.
fn build_header(kind: &str, header_cid: &str, kind_specific: Vec<(String, Arc<Value>)>) -> Arc<Value> {
    let mut entries: Vec<(String, Arc<Value>)> = Vec::with_capacity(3 + kind_specific.len());
    entries.push((
        "schemaVersion".into(),
        Value::string(LAYERED_SCHEMA_VERSION),
    ));
    entries.push(("kind".into(), Value::string(kind.to_string())));
    entries.push(("cid".into(), Value::string(header_cid.to_string())));
    entries.extend(kind_specific);
    Arc::new(Value::Object(entries))
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

/// Compute the **content** CID of a contract (signer-independent).
///
/// Per `protocol/specs/2026-05-03-contract-cid-vs-attestation-cid.md` §1,
/// this is the BLAKE3-512 of the JCS encoding of the contract's
/// substrate-load-bearing fields: `name`, `outBinding`, and any of
/// `pre`/`post`/`inv` that are present. Two distinct signers attesting
/// to the same logical contract produce the same `contractCid`.
///
/// This value goes in `header.cid` of the minted layered memento and is
/// also available directly without minting via this public function.
///
/// Per spec naming convention (`contract_cid(decl)` for Rust).
pub fn contract_cid(args: &MintContractArgs) -> String {
    let mut kvs: Vec<(String, Arc<Value>)> = vec![
        ("name".into(), Value::string(args.contract_name.clone())),
        ("outBinding".into(), Value::string(args.out_binding.clone())),
    ];
    if let Some(pre) = &args.pre {
        kvs.push(("pre".into(), pre.clone()));
    }
    if let Some(post) = &args.post {
        kvs.push(("post".into(), post.clone()));
    }
    if let Some(inv) = &args.inv {
        kvs.push(("inv".into(), inv.clone()));
    }
    let v = Arc::new(Value::Object(kvs));
    blake3_512_of(encode_jcs(&v).as_bytes())
}

// Keep the private alias for internal use within this module.
fn contract_content_cid(args: &MintContractArgs) -> String {
    contract_cid(args)
}

/// Compute the **contract set CID** from a slice of already-computed
/// `contractCid` strings (each `blake3-512:<128 hex>` produced by
/// `contract_cid()`).
///
/// Per `protocol/specs/2026-05-03-contract-set-extension.md` §1:
///   contractSetCid := "blake3-512:" || hex(BLAKE3-512(JCS(<sorted contractCids>)))
///
/// The sort is lexicographic on the raw `blake3-512:hex` strings, making
/// the result order-independent. Two kits enumerating the same contracts
/// in different order produce byte-identical `contractSetCid` values.
pub fn compute_contract_set_cid(mut contract_cids: Vec<String>) -> String {
    contract_cids.sort();
    let arr: Vec<Arc<Value>> = contract_cids.into_iter().map(Value::string).collect();
    let v = Value::array(arr);
    let jcs = encode_jcs(&v);
    blake3_512_of(jcs.as_bytes())
}

pub fn mint_contract(args: &MintContractArgs) -> Result<MintedEnvelope, ClaimEnvelopeError> {
    if args.pre.is_none() && args.post.is_none() && args.inv.is_none() {
        return Err(ClaimEnvelopeError::EmptyContract);
    }
    if args.out_binding.is_empty() {
        return Err(ClaimEnvelopeError::EmptyOutBinding);
    }

    // DERIVED:
    //   propertyHash = hash(JCS({pre?, post?, inv?, outBinding}))
    //   bindingHash  = hash(JCS({producerId, contractName, propertyHash}))
    //
    // These ride in the header because the verifier uses them to index
    // and resolve callsites; they are substrate-load-bearing despite
    // being derivable (see spec §1 "kind-specific REQUIRED header
    // fields").
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

    // Header: schemaVersion + kind + cid + kind-specific REQUIRED fields.
    let header_cid = contract_content_cid(args);
    let mut kind_specific: Vec<(String, Arc<Value>)> = vec![
        ("name".into(), Value::string(args.contract_name.clone())),
        ("outBinding".into(), Value::string(args.out_binding.clone())),
    ];
    if let Some(pre) = &args.pre {
        kind_specific.push(("pre".into(), pre.clone()));
    }
    if let Some(post) = &args.post {
        kind_specific.push(("post".into(), post.clone()));
    }
    if let Some(inv) = &args.inv {
        kind_specific.push(("inv".into(), inv.clone()));
    }
    kind_specific.push(("verdict".into(), Value::string("holds")));
    kind_specific.push(("bindingHash".into(), Value::string(binding_hash)));
    kind_specific.push(("propertyHash".into(), Value::string(property_hash)));
    let mut sorted_inputs: Vec<String> = args.input_cids.clone();
    sorted_inputs.sort();
    let inputs_arr: Vec<Arc<Value>> = sorted_inputs.into_iter().map(Value::string).collect();
    kind_specific.push(("inputCids".into(), Value::array(inputs_arr)));

    let header = build_header("contract", &header_cid, kind_specific);

    // Metadata: producer attribution + per-formula derived hashes
    // (purely tooling convenience; not used by the substrate verifier).
    let mut metadata_kvs: Vec<(String, Arc<Value>)> = vec![
        ("authoring".into(), authoring_to_value(&args.authoring)),
        ("producedBy".into(), Value::string(args.produced_by.clone())),
        ("producedAt".into(), Value::string(args.produced_at.clone())),
    ];
    if let Some(pre) = &args.pre {
        metadata_kvs.push(("preHash".into(), Value::string(hash_value(pre))));
    }
    if let Some(post) = &args.post {
        metadata_kvs.push(("postHash".into(), Value::string(hash_value(post))));
    }
    if let Some(inv) = &args.inv {
        metadata_kvs.push(("invHash".into(), Value::string(hash_value(inv))));
    }
    let metadata = Arc::new(Value::Object(metadata_kvs));

    Ok(assemble_layered(
        header,
        metadata,
        &args.produced_at,
        &args.signer_seed,
        header_cid,
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

/// Compute the content CID of a bridge declaration (signer-independent).
fn bridge_content_cid(args: &MintBridgeArgs) -> String {
    let arg_sorts: Vec<Arc<Value>> = args
        .ir_arg_sorts
        .iter()
        .map(|s| Value::string(s.clone()))
        .collect();
    let v = Value::object([
        ("sourceSymbol", Value::string(args.source_symbol.clone())),
        ("sourceLayer", Value::string(args.source_layer.clone())),
        ("targetContractCid", Value::string(args.target_contract_cid.clone())),
        ("targetLayer", Value::string(args.target_layer.clone())),
        ("irArgSorts", Value::array(arg_sorts)),
        ("irReturnSort", Value::string(args.ir_return_sort.clone())),
    ]);
    blake3_512_of(encode_jcs(&v).as_bytes())
}

pub fn mint_bridge(args: &MintBridgeArgs) -> MintedEnvelope {
    let arg_sorts: Vec<Arc<Value>> = args
        .ir_arg_sorts
        .iter()
        .map(|s| Value::string(s.clone()))
        .collect();

    // DERIVED per spec:
    //   bindingHash  = hash(canonical({sourceLayer, sourceSymbol}))
    //   propertyHash = hash("bridge:" || sourceSymbol)
    let bh_obj = Value::object([
        ("sourceLayer", Value::string(args.source_layer.clone())),
        ("sourceSymbol", Value::string(args.source_symbol.clone())),
    ]);
    let binding_hash = hash_value(&bh_obj);
    let property_hash = hash_string(&format!("bridge:{}", args.source_symbol));

    let header_cid = bridge_content_cid(args);
    let kind_specific: Vec<(String, Arc<Value>)> = vec![
        ("sourceSymbol".into(), Value::string(args.source_symbol.clone())),
        ("sourceLayer".into(), Value::string(args.source_layer.clone())),
        ("targetContractCid".into(), Value::string(args.target_contract_cid.clone())),
        ("targetLayer".into(), Value::string(args.target_layer.clone())),
        ("irArgSorts".into(), Value::array(arg_sorts)),
        ("irReturnSort".into(), Value::string(args.ir_return_sort.clone())),
        ("verdict".into(), Value::string("holds")),
        ("bindingHash".into(), Value::string(binding_hash)),
        ("propertyHash".into(), Value::string(property_hash)),
        (
            "inputCids".into(),
            Value::array(vec![Value::string(args.target_contract_cid.clone())]),
        ),
    ];

    let header = build_header("bridge", &header_cid, kind_specific);

    let mut metadata_kvs: Vec<(String, Arc<Value>)> = vec![
        ("producedBy".into(), Value::string(args.produced_by.clone())),
        ("producedAt".into(), Value::string(args.produced_at.clone())),
    ];
    if !args.notes.is_empty() {
        metadata_kvs.push(("notes".into(), Value::string(args.notes.clone())));
    }
    let metadata = Arc::new(Value::Object(metadata_kvs));

    assemble_layered(header, metadata, &args.produced_at, &args.signer_seed, String::new())
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

fn implication_content_cid(args: &MintImplicationArgs) -> String {
    let v = Value::object([
        ("antecedentHash", Value::string(args.antecedent_hash.clone())),
        ("consequentHash", Value::string(args.consequent_hash.clone())),
        ("antecedentCid", Value::string(args.antecedent_cid.clone())),
        ("consequentCid", Value::string(args.consequent_cid.clone())),
        ("antecedentSlot", Value::string(args.antecedent_slot.clone())),
        ("consequentSlot", Value::string(args.consequent_slot.clone())),
    ]);
    blake3_512_of(encode_jcs(&v).as_bytes())
}

pub fn mint_implication(args: &MintImplicationArgs) -> MintedEnvelope {
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

    let header_cid = implication_content_cid(args);
    let mut input_cids = vec![args.antecedent_cid.clone(), args.consequent_cid.clone()];
    input_cids.sort();
    let input_arr: Vec<Arc<Value>> = input_cids.into_iter().map(Value::string).collect();

    let kind_specific: Vec<(String, Arc<Value>)> = vec![
        ("antecedentHash".into(), Value::string(args.antecedent_hash.clone())),
        ("consequentHash".into(), Value::string(args.consequent_hash.clone())),
        ("antecedentCid".into(), Value::string(args.antecedent_cid.clone())),
        ("consequentCid".into(), Value::string(args.consequent_cid.clone())),
        ("antecedentSlot".into(), Value::string(args.antecedent_slot.clone())),
        ("consequentSlot".into(), Value::string(args.consequent_slot.clone())),
        ("verdict".into(), Value::string("holds")),
        ("bindingHash".into(), Value::string(binding_hash)),
        ("propertyHash".into(), Value::string(property_hash)),
        ("inputCids".into(), Value::array(input_arr)),
    ];

    let header = build_header("implication", &header_cid, kind_specific);

    let mut metadata_kvs: Vec<(String, Arc<Value>)> = vec![
        ("producedBy".into(), Value::string(args.produced_by.clone())),
        ("producedAt".into(), Value::string(args.produced_at.clone())),
        ("prover".into(), Value::string(args.prover.clone())),
        ("proverRunMs".into(), Value::integer(args.prover_run_ms)),
    ];
    if !args.smt_lib_input.is_empty() {
        metadata_kvs.push(("smtLibInput".into(), Value::string(args.smt_lib_input.clone())));
    }
    if !args.proof_witness.is_empty() {
        metadata_kvs.push(("proofWitness".into(), Value::string(args.proof_witness.clone())));
    }
    let metadata = Arc::new(Value::Object(metadata_kvs));

    assemble_layered(header, metadata, &args.produced_at, &args.signer_seed, String::new())
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
