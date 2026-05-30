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
    let msg = Value::object([("header", header.clone()), ("metadata", metadata.clone())]);
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
fn build_header(
    kind: &str,
    header_cid: &str,
    kind_specific: Vec<(String, Arc<Value>)>,
) -> Arc<Value> {
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
    KitAuthor {
        author: String,
        note: Option<String>,
    },
    Lift {
        lifter: String,
        evidence: String,
        source_cid: Option<String>,
    },
    Llm {
        llm: String,
        llm_version: String,
        prompt_cid: String,
        confidence: f64,
        rationale: Option<String>,
    },
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
        Authoring::Lift {
            lifter,
            evidence,
            source_cid,
        } => {
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
        Authoring::Llm {
            llm,
            llm_version,
            prompt_cid,
            confidence,
            rationale,
        } => {
            let mut entries: Vec<(String, Arc<Value>)> = vec![
                ("producerKind".into(), Value::string("llm")),
                ("llm".into(), Value::string(llm.clone())),
                ("llmVersion".into(), Value::string(llm_version.clone())),
                ("promptCid".into(), Value::string(prompt_cid.clone())),
                (
                    "confidence".into(),
                    Value::integer((confidence * 1000.0) as i64),
                ),
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
    /// Formal parameter names of the function this contract describes, in
    /// declaration order. Body-derived op-contracts (the verification-spine
    /// target the `body_discharge::CatalogResolver` consumes, #1440/#1436)
    /// REQUIRE this: the resolver substitutes a harvested call's argument
    /// into the matching formal of the body-derived `post`, so without
    /// `formals` it returns `None` and the callee stays uninterpreted
    /// (Undecidable). Empty for non-function contracts (LIA tautologies,
    /// cross-language refinement targets); the field is then omitted from
    /// the header so those mementos keep their current bytes/CIDs.
    pub formals: Vec<String>,
    /// Emit `formals: []` (and `formalSorts: []`) when the vector is
    /// empty. Presence is load-bearing for zero-arg body-derived
    /// op-contracts: absent `formals` means "not body-derived", while
    /// present empty `formals` means "body-bearing function with no
    /// parameters".
    pub emit_empty_formals: bool,
    /// Sorts of the formals, parallel to `formals`. Carried alongside
    /// `formals` so the resolver can name the value slots; omitted from the
    /// header when empty.
    pub formal_sorts: Vec<Arc<Value>>,
    /// The crate / library this contract belongs to (the project's
    /// `platform_profile.library`). Carried as a metadata axis so a consumer
    /// that vendors this proof can tell THIS crate's `foo` from a same-named
    /// `foo` in another crate (the Tier-1 cross-crate disambiguation). A
    /// metadata field, not part of the contract CID: it does not change what
    /// is proven, only how a call site resolves to it. `None` omits the key.
    pub library: Option<String>,
}

// =============================================================================
// mint_authority
// =============================================================================

pub struct MintAuthorityArgs {
    pub principal: String,
    pub key: String,
    pub scope_kind: String,
    pub scope: String,
    pub parent_authority_cid: Option<String>,
    pub produced_by: String,
    pub produced_at: String,
    pub signer_seed: Ed25519Seed,
}

fn authority_content_cid(args: &MintAuthorityArgs) -> String {
    let mut kvs: Vec<(String, Arc<Value>)> = vec![
        ("principal".into(), Value::string(args.principal.clone())),
        ("key".into(), Value::string(args.key.clone())),
        ("scopeKind".into(), Value::string(args.scope_kind.clone())),
        ("scope".into(), Value::string(args.scope.clone())),
    ];
    if let Some(parent) = &args.parent_authority_cid {
        kvs.push(("parentAuthorityCid".into(), Value::string(parent.clone())));
    }
    hash_value(&Arc::new(Value::Object(kvs)))
}

pub fn mint_authority(args: &MintAuthorityArgs) -> Result<MintedEnvelope, ClaimEnvelopeError> {
    if args.principal.is_empty() {
        return Err(ClaimEnvelopeError::Other(
            "mint_authority: principal must not be empty".into(),
        ));
    }
    if !args.key.starts_with("ed25519:") {
        return Err(ClaimEnvelopeError::Other(
            "mint_authority: key must be an inline ed25519 public key".into(),
        ));
    }
    if args.scope_kind.is_empty() || args.scope.is_empty() {
        return Err(ClaimEnvelopeError::Other(
            "mint_authority: scopeKind and scope must not be empty".into(),
        ));
    }

    let header_cid = authority_content_cid(args);
    let mut input_cids = Vec::new();
    if let Some(parent) = &args.parent_authority_cid {
        input_cids.push(parent.clone());
    }
    input_cids.sort();
    let input_arr: Vec<Arc<Value>> = input_cids.into_iter().map(Value::string).collect();
    let mut kind_specific: Vec<(String, Arc<Value>)> = vec![
        ("principal".into(), Value::string(args.principal.clone())),
        ("key".into(), Value::string(args.key.clone())),
        ("scopeKind".into(), Value::string(args.scope_kind.clone())),
        ("scope".into(), Value::string(args.scope.clone())),
    ];
    if let Some(parent) = &args.parent_authority_cid {
        kind_specific.push(("parentAuthorityCid".into(), Value::string(parent.clone())));
    }
    kind_specific.push(("verdict".into(), Value::string("holds")));
    kind_specific.push(("inputCids".into(), Value::array(input_arr)));

    let header = build_header("authority", &header_cid, kind_specific);
    let metadata = Arc::new(Value::Object(vec![
        ("producedBy".into(), Value::string(args.produced_by.clone())),
        ("producedAt".into(), Value::string(args.produced_at.clone())),
        (
            "authorityClaim".into(),
            Value::string(format!(
                "{} controls {} for {}:{}",
                args.principal, args.key, args.scope_kind, args.scope
            )),
        ),
    ]));

    Ok(assemble_layered(
        header,
        metadata,
        &args.produced_at,
        &args.signer_seed,
        String::new(),
    ))
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
    // Body-derived op-contracts carry their formals as part of contract
    // identity: two functions with the same `post` but different formal
    // names are different contracts (the resolver substitutes by formal
    // name). Omitted when empty unless `emit_empty_formals` marks the
    // zero-arg body-derived case, so non-function contracts keep their
    // existing content CIDs unchanged.
    if !args.formals.is_empty() || args.emit_empty_formals {
        let formals_arr: Vec<Arc<Value>> = args
            .formals
            .iter()
            .map(|f| Value::string(f.clone()))
            .collect();
        kvs.push(("formals".into(), Value::array(formals_arr)));
    }
    if !args.formal_sorts.is_empty() || args.emit_empty_formals {
        kvs.push((
            "formalSorts".into(),
            Value::array(args.formal_sorts.clone()),
        ));
    }
    let v = Arc::new(Value::Object(kvs));
    blake3_512_of(encode_jcs(&v).as_bytes())
}

// Keep the private alias for internal use within this module.
fn contract_content_cid(args: &MintContractArgs) -> String {
    contract_cid(args)
}

/// Compute the DERIVED `propertyHash` for a contract header.
///
/// This is the hash of the contract properties the verifier indexes:
/// present `pre`/`post`/`inv` slots plus the output binding. It is
/// intentionally separate from `contract_cid`, which also includes the
/// contract name and identifies the signer-independent contract content.
pub fn contract_property_hash(args: &MintContractArgs) -> String {
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
    ph_kvs.push(("outBinding".into(), Value::string(args.out_binding.clone())));
    hash_value(&Arc::new(Value::Object(ph_kvs)))
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
    let property_hash = contract_property_hash(args);

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
    // Body-derived op-contract slots: `formals` (+ `formalSorts`) ride in
    // the header so `body_discharge::CatalogResolver` (which reads the
    // header via `memento_body` for v1.2-layered mementos) can project them
    // into `OpContractInfo` value slots. Omitted when empty unless
    // `emit_empty_formals` marks a zero-arg body-derived op-contract, so
    // non-function contracts are byte-identical to their pre-#1436 form.
    if !args.formals.is_empty() || args.emit_empty_formals {
        let formals_arr: Vec<Arc<Value>> = args
            .formals
            .iter()
            .map(|f| Value::string(f.clone()))
            .collect();
        kind_specific.push(("formals".into(), Value::array(formals_arr)));
    }
    if !args.formal_sorts.is_empty() || args.emit_empty_formals {
        kind_specific.push((
            "formalSorts".into(),
            Value::array(args.formal_sorts.clone()),
        ));
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
    if let Some(library) = &args.library {
        if !library.is_empty() {
            metadata_kvs.push(("library".into(), Value::string(library.clone())));
        }
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
    /// Forward pin (BridgeDeclaration.ConsequentBundlePinned, NORMATIVE):
    /// the CID of the `.proof` bundle that is allowed to discharge this
    /// bridge's target contract. `Some(bundle)` pins a CROSS-bundle target
    /// (a dependency proof); the verifier refuses any contract member not
    /// drawn from that bundle. `None` means SELF-pinned: the target must be
    /// a co-member of this bridge's own bundle. There is no unpinned path;
    /// `None` is enforced as same-bundle membership, not skipped.
    pub target_proof_cid: Option<String>,
}

/// Compute the content CID of a bridge declaration (signer-independent).
fn bridge_content_cid(args: &MintBridgeArgs) -> String {
    let arg_sorts: Vec<Arc<Value>> = args
        .ir_arg_sorts
        .iter()
        .map(|s| Value::string(s.clone()))
        .collect();
    let mut fields: Vec<(&str, Arc<Value>)> = vec![
        ("sourceSymbol", Value::string(args.source_symbol.clone())),
        ("sourceLayer", Value::string(args.source_layer.clone())),
        (
            "targetContractCid",
            Value::string(args.target_contract_cid.clone()),
        ),
        ("targetLayer", Value::string(args.target_layer.clone())),
        ("irArgSorts", Value::array(arg_sorts)),
        ("irReturnSort", Value::string(args.ir_return_sort.clone())),
    ];
    // The pin is part of bridge identity: a bridge that pins bundle A and one
    // that pins bundle B (same target contract) are DIFFERENT bridges. Only
    // emit the key when Some, so a self-pinned (None) bridge's CID is the
    // pin-free identity. encode_jcs sorts keys, so insertion order is moot.
    if let Some(ref bundle) = args.target_proof_cid {
        fields.push(("targetProofCid", Value::string(bundle.clone())));
    }
    let v = Value::object(fields);
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
    let mut kind_specific: Vec<(String, Arc<Value>)> = vec![
        (
            "sourceSymbol".into(),
            Value::string(args.source_symbol.clone()),
        ),
        (
            "sourceLayer".into(),
            Value::string(args.source_layer.clone()),
        ),
        (
            "targetContractCid".into(),
            Value::string(args.target_contract_cid.clone()),
        ),
        (
            "targetLayer".into(),
            Value::string(args.target_layer.clone()),
        ),
        ("irArgSorts".into(), Value::array(arg_sorts)),
        (
            "irReturnSort".into(),
            Value::string(args.ir_return_sort.clone()),
        ),
        ("verdict".into(), Value::string("holds")),
        ("bindingHash".into(), Value::string(binding_hash)),
        ("propertyHash".into(), Value::string(property_hash)),
        (
            "inputCids".into(),
            Value::array(vec![Value::string(args.target_contract_cid.clone())]),
        ),
    ];
    // Forward pin into the body so the verifier (enumerate_callsites ->
    // resolve_target) can enforce ConsequentBundlePinned. Omitted when None
    // (self-pinned: the verifier enforces same-bundle co-membership instead).
    if let Some(ref bundle) = args.target_proof_cid {
        kind_specific.push(("targetProofCid".into(), Value::string(bundle.clone())));
    }

    let header = build_header("bridge", &header_cid, kind_specific);

    let mut metadata_kvs: Vec<(String, Arc<Value>)> = vec![
        ("producedBy".into(), Value::string(args.produced_by.clone())),
        ("producedAt".into(), Value::string(args.produced_at.clone())),
    ];
    if !args.notes.is_empty() {
        metadata_kvs.push(("notes".into(), Value::string(args.notes.clone())));
    }
    let metadata = Arc::new(Value::Object(metadata_kvs));

    assemble_layered(
        header,
        metadata,
        &args.produced_at,
        &args.signer_seed,
        String::new(),
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
    pub additional_input_cids: Vec<String>,
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
        (
            "antecedentHash",
            Value::string(args.antecedent_hash.clone()),
        ),
        (
            "consequentHash",
            Value::string(args.consequent_hash.clone()),
        ),
        ("antecedentCid", Value::string(args.antecedent_cid.clone())),
        ("consequentCid", Value::string(args.consequent_cid.clone())),
        (
            "antecedentSlot",
            Value::string(args.antecedent_slot.clone()),
        ),
        (
            "consequentSlot",
            Value::string(args.consequent_slot.clone()),
        ),
    ]);
    blake3_512_of(encode_jcs(&v).as_bytes())
}

pub fn mint_implication(args: &MintImplicationArgs) -> MintedEnvelope {
    // DERIVED per spec:
    //   bindingHash  = hash(canonical({antecedentHash, consequentHash}))
    //   propertyHash = hash("implication:" || antecedentHash || ":" || consequentHash)
    let bh_obj = Value::object([
        (
            "antecedentHash",
            Value::string(args.antecedent_hash.clone()),
        ),
        (
            "consequentHash",
            Value::string(args.consequent_hash.clone()),
        ),
    ]);
    let binding_hash = hash_value(&bh_obj);
    let property_hash = hash_string(&format!(
        "implication:{}:{}",
        args.antecedent_hash, args.consequent_hash
    ));

    let header_cid = implication_content_cid(args);
    let mut input_cids = vec![args.antecedent_cid.clone(), args.consequent_cid.clone()];
    input_cids.extend(args.additional_input_cids.iter().cloned());
    input_cids.sort();
    let input_arr: Vec<Arc<Value>> = input_cids.into_iter().map(Value::string).collect();

    let kind_specific: Vec<(String, Arc<Value>)> = vec![
        (
            "antecedentHash".into(),
            Value::string(args.antecedent_hash.clone()),
        ),
        (
            "consequentHash".into(),
            Value::string(args.consequent_hash.clone()),
        ),
        (
            "antecedentCid".into(),
            Value::string(args.antecedent_cid.clone()),
        ),
        (
            "consequentCid".into(),
            Value::string(args.consequent_cid.clone()),
        ),
        (
            "antecedentSlot".into(),
            Value::string(args.antecedent_slot.clone()),
        ),
        (
            "consequentSlot".into(),
            Value::string(args.consequent_slot.clone()),
        ),
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
        metadata_kvs.push((
            "smtLibInput".into(),
            Value::string(args.smt_lib_input.clone()),
        ));
    }
    if !args.proof_witness.is_empty() {
        metadata_kvs.push((
            "proofWitness".into(),
            Value::string(args.proof_witness.clone()),
        ));
    }
    let metadata = Arc::new(Value::Object(metadata_kvs));

    assemble_layered(
        header,
        metadata,
        &args.produced_at,
        &args.signer_seed,
        String::new(),
    )
}

// =============================================================================
// mint_witness
// =============================================================================

pub struct MintWitnessArgs {
    pub claim_kind: String,
    pub claim_body_cid: String,
    pub verifier_cid: String,
    pub policy_cid: String,
    pub evidence_root_cid: String,
    pub input_cids: Vec<String>,
    pub produced_by: String,
    pub produced_at: String,
    pub claim_body: Arc<Value>,
    pub evidence: Arc<Value>,
    pub signer_seed: Ed25519Seed,
}

fn witness_content_cid(args: &MintWitnessArgs) -> String {
    let mut input_cids = args.input_cids.clone();
    input_cids.sort();
    let input_arr: Vec<Arc<Value>> = input_cids.into_iter().map(Value::string).collect();
    let v = Value::object([
        ("claimKind", Value::string(args.claim_kind.clone())),
        ("claimBodyCid", Value::string(args.claim_body_cid.clone())),
        ("verifierCid", Value::string(args.verifier_cid.clone())),
        ("policyCid", Value::string(args.policy_cid.clone())),
        (
            "evidenceRootCid",
            Value::string(args.evidence_root_cid.clone()),
        ),
        ("inputCids", Value::array(input_arr)),
    ]);
    blake3_512_of(encode_jcs(&v).as_bytes())
}

pub fn mint_witness(args: &MintWitnessArgs) -> Result<MintedEnvelope, ClaimEnvelopeError> {
    if args.claim_kind.is_empty()
        || args.claim_body_cid.is_empty()
        || args.verifier_cid.is_empty()
        || args.policy_cid.is_empty()
        || args.evidence_root_cid.is_empty()
    {
        return Err(ClaimEnvelopeError::Other(
            "mint_witness: claim kind, body CID, verifier CID, policy CID, and evidence root CID must not be empty".into(),
        ));
    }

    let header_cid = witness_content_cid(args);
    let mut sorted_inputs = args.input_cids.clone();
    sorted_inputs.sort();
    let input_arr: Vec<Arc<Value>> = sorted_inputs.into_iter().map(Value::string).collect();
    let header = build_header(
        "witness",
        &header_cid,
        vec![
            ("claimKind".into(), Value::string(args.claim_kind.clone())),
            (
                "claimBodyCid".into(),
                Value::string(args.claim_body_cid.clone()),
            ),
            ("verdict".into(), Value::string("holds")),
            (
                "verifierCid".into(),
                Value::string(args.verifier_cid.clone()),
            ),
            ("policyCid".into(), Value::string(args.policy_cid.clone())),
            (
                "evidenceRootCid".into(),
                Value::string(args.evidence_root_cid.clone()),
            ),
            ("inputCids".into(), Value::array(input_arr)),
        ],
    );

    let metadata = Arc::new(Value::Object(vec![
        ("producedBy".into(), Value::string(args.produced_by.clone())),
        ("producedAt".into(), Value::string(args.produced_at.clone())),
        ("claimBody".into(), args.claim_body.clone()),
        ("evidence".into(), args.evidence.clone()),
    ]));

    Ok(assemble_layered(
        header,
        metadata,
        &args.produced_at,
        &args.signer_seed,
        String::new(),
    ))
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
            formals: Vec::new(),
            emit_empty_formals: false,
            formal_sorts: Vec::new(),
            library: None,
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
                    Value::object([("kind", Value::string("var")), ("name", Value::string("n"))]),
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
        ]);
        let args = MintContractArgs {
            formals: Vec::new(),
            emit_empty_formals: false,
            formal_sorts: Vec::new(),
            library: None,
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

    #[test]
    fn contract_property_hash_matches_minted_header() {
        let post = Value::object([
            ("kind", Value::string("atomic")),
            ("name", Value::string("ok")),
            ("args", Value::array(vec![Value::string("out")])),
        ]);
        let args = MintContractArgs {
            formals: Vec::new(),
            emit_empty_formals: false,
            formal_sorts: Vec::new(),
            library: None,
            contract_name: "checked_add_u8.postcondition".into(),
            pre: None,
            post: Some(post),
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

        let expected = contract_property_hash(&args);
        let m = mint_contract(&args).expect("mint");
        let env: serde_json::Value =
            serde_json::from_slice(&m.canonical_bytes).expect("parse memento");
        let actual = env
            .pointer("/header/propertyHash")
            .and_then(|v| v.as_str())
            .expect("header.propertyHash");

        assert_eq!(actual, expected);
    }

    #[test]
    fn mint_authority_emits_key_scope_and_parent_link() {
        let authority_key = ed25519_pubkey_string(&[0x22; 32]);
        let args = MintAuthorityArgs {
            principal: "bridgeworks.software".into(),
            key: authority_key.clone(),
            scope_kind: "contract".into(),
            scope: "checked_add_u8.postcondition".into(),
            parent_authority_cid: Some("blake3-512:parent".into()),
            produced_by: "test".into(),
            produced_at: "2026-05-08T00:00:00.000Z".into(),
            signer_seed: dummy_seed(),
        };

        let minted = mint_authority(&args).expect("mint authority");
        let env: serde_json::Value =
            serde_json::from_slice(&minted.canonical_bytes).expect("parse authority");

        assert_eq!(
            env.pointer("/header/kind").and_then(|v| v.as_str()),
            Some("authority")
        );
        assert_eq!(
            env.pointer("/header/principal").and_then(|v| v.as_str()),
            Some("bridgeworks.software")
        );
        assert_eq!(
            env.pointer("/header/key").and_then(|v| v.as_str()),
            Some(authority_key.as_str())
        );
        assert_eq!(
            env.pointer("/header/scopeKind").and_then(|v| v.as_str()),
            Some("contract")
        );
        assert_eq!(
            env.pointer("/header/scope").and_then(|v| v.as_str()),
            Some("checked_add_u8.postcondition")
        );
        assert_eq!(
            env.pointer("/header/inputCids/0").and_then(|v| v.as_str()),
            Some("blake3-512:parent")
        );
        assert!(minted.cid.starts_with("blake3-512:"));
    }
}
