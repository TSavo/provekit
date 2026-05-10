// SPDX-License-Identifier: Apache-2.0

use std::convert::TryFrom;
use std::fmt;
use std::sync::Arc;

use provekit_canonicalizer::{blake3_512_of, encode_jcs, Value as CValue};
use provekit_ir_types::{IrFormula, IrTerm, LetBinding, Sort};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use thiserror::Error;

use crate::compose::{build_memento_value, jcs_bytes_of_value, FunctionContractMemento, Locus};

use super::traits::Canonical;

/// A flat BLAKE3-512 content identifier: `blake3-512:<128 lowercase hex>`.
///
/// `Cid` is the algebra's address type. It has no internal structure beyond
/// the hash algorithm tag and hex digest; any semantic meaning comes from the
/// catalog entry found by `resolve`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Cid(String);

impl Cid {
    /// Parse and validate a BLAKE3-512 CID string.
    pub fn parse(value: impl Into<String>) -> Result<Self, CidError> {
        let value = value.into();
        let Some(hex) = value.strip_prefix("blake3-512:") else {
            return Err(CidError::Invalid(value));
        };
        if hex.len() != 128 || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(CidError::Invalid(value));
        }
        Ok(Self(value))
    }

    pub(crate) fn from_hash_output(value: String) -> Self {
        debug_assert!(Self::parse(value.clone()).is_ok());
        Self(value)
    }

    /// Borrow the self-identifying CID string.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Consume the CID into its string form.
    pub fn into_string(self) -> String {
        self.0
    }
}

impl fmt::Display for Cid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl TryFrom<String> for Cid {
    type Error = CidError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::parse(value)
    }
}

impl TryFrom<&str> for Cid {
    type Error = CidError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Self::parse(value)
    }
}

impl From<Cid> for String {
    fn from(value: Cid) -> Self {
        value.0
    }
}

/// CID parse/shape errors.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum CidError {
    /// The string does not match `blake3-512:<128 hex>`.
    #[error("invalid BLAKE3-512 CID: {0}")]
    Invalid(String),
}

/// Faithful algebra term over operation CIDs.
///
/// This is the paper-16 stratum: cross-compilers can carry it and verifiers
/// may discard it once the lossy contract projection is durable. We introduce
/// `core::Term` instead of extending generated `IrTerm` because `IrTerm::Ctor`
/// is generated from the protocol CDDL and still carries only a bare name.
/// Conversions are provided: `From<IrTerm>` synthesizes a deterministic
/// name-derived op CID for compatibility, while `From<Term> for IrTerm` drops
/// the op CID when crossing back into the historical IR.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum Term {
    /// Operation application grounded by the content ID of the operation.
    #[serde(rename = "op")]
    Op {
        #[serde(rename = "opCid")]
        op_cid: Cid,
        name: String,
        args: Vec<Term>,
    },
    /// Named variable.
    #[serde(rename = "var")]
    Var { name: String },
    /// JSON constant with an IR sort.
    #[serde(rename = "const")]
    Const { value: JsonValue, sort: Sort },
    /// Unit value; encoded as `Ctor("unit")` when converted to `IrTerm`.
    #[serde(rename = "unit")]
    Unit,
}

impl From<IrTerm> for Term {
    fn from(value: IrTerm) -> Self {
        match value {
            IrTerm::Var { name } => Term::Var { name },
            IrTerm::Const { value, sort } => Term::Const { value, sort },
            IrTerm::Ctor { name, args } => Term::Op {
                op_cid: operation_name_cid(&name),
                name,
                args: args.into_iter().map(Term::from).collect(),
            },
            IrTerm::Lambda {
                param_name,
                param_sort,
                body,
            } => Term::Op {
                op_cid: operation_name_cid("lambda"),
                name: "lambda".to_string(),
                args: vec![
                    Term::Const {
                        value: serde_json::json!({
                            "paramName": param_name,
                            "paramSort": param_sort,
                        }),
                        sort: Sort::Primitive {
                            name: "Metadata".to_string(),
                        },
                    },
                    Term::from(*body),
                ],
            },
            IrTerm::Let { bindings, body } => {
                let mut args: Vec<Term> = bindings
                    .into_iter()
                    .map(|binding| Term::Op {
                        op_cid: operation_name_cid("let-binding"),
                        name: binding.name,
                        args: vec![Term::from(binding.bound_term)],
                    })
                    .collect();
                args.push(Term::from(*body));
                Term::Op {
                    op_cid: operation_name_cid("let"),
                    name: "let".to_string(),
                    args,
                }
            }
        }
    }
}

impl From<Term> for IrTerm {
    fn from(value: Term) -> Self {
        match value {
            Term::Op { name, args, .. } => IrTerm::Ctor {
                name,
                args: args.into_iter().map(IrTerm::from).collect(),
            },
            Term::Var { name } => IrTerm::Var { name },
            Term::Const { value, sort } => IrTerm::Const { value, sort },
            Term::Unit => IrTerm::Ctor {
                name: "unit".to_string(),
                args: vec![],
            },
        }
    }
}

/// The lossy formula stratum reused from `provekit-ir-types`.
///
/// `IrFormula` already has the required shape (`And`, `Or`, `Not`, `Implies`,
/// `Atomic`, `Forall`, `Exists`, `Choice`) and is the type accepted by the
/// existing WP/composition code. It remains the durable paper-9 projection;
/// faithful op-CID texture lives in [`Term`] and may be discarded.
pub type Formula = IrFormula;

/// A function-contract projection reused from `libprovekit::compose`.
///
/// `FunctionContractMemento` is a superset of the requested `Contract` fields:
/// it carries formals, formal sorts, return sort, pre/post formulas, effects,
/// plus existing CID/canonical-byte/locus metadata. Reusing it preserves the
/// established API and lets core composition delegate to the CCP algebra.
pub type Contract = FunctionContractMemento;

/// Content-addressed witness for a resolved claim.
///
/// This is the durable evidence object set by primitive 7. The initial pass
/// stores proof trees, counterexample models, and unknown transcripts as JSON.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum Witness {
    /// Proof tree or SMT-LIB transcript establishing truth.
    #[serde(rename = "proof")]
    Proof { tree: JsonValue },
    /// Model establishing refutation.
    #[serde(rename = "counterexample")]
    Counterexample { model: JsonValue },
    /// Solver/checker transcript for an unknown result.
    #[serde(rename = "unknown")]
    Unknown { transcript: JsonValue },
}

/// Resolution state of a claim.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Verdict {
    /// No discharge has been attempted or recorded.
    Unresolved,
    /// A witness proves the claim.
    Proved,
    /// A witness refutes the claim; this is a finding.
    Refuted,
    /// Discharge ran but could not decide the claim.
    Unknown,
}

/// Semantic domain kind carried by claims.
///
/// The plugin trait is also named `Domain`, so the field type is `DomainKind`
/// to keep Rust's namespace clear while preserving the design's axis.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DomainKind {
    /// Function contract / weakest-precondition domain.
    FunctionContract,
    /// Protocol-evolution paths.
    ProtocolEvolution,
    /// Supply graph paths.
    SupplyGraph,
    /// Bug-pattern findings.
    BugPattern,
    /// License/compliance paths.
    License,
    /// Hardware or machine-code paths.
    Hardware,
    /// Forward-compatible catch-all.
    Other(String),
}

/// Dialect accepted or emitted by a kit.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Dialect {
    /// C source or C-family AST.
    C,
    /// Rust source or Rust MIR/LLBC-derived input.
    Rust,
    /// x86-64 assembly or machine-code view.
    X86_64,
    /// AArch64 assembly or machine-code view.
    AArch64,
    /// WebAssembly.
    Wasm,
    /// JVM bytecode.
    JvmBytecode,
    /// Coq terms/scripts.
    Coq,
    /// SMT-LIB text.
    SmtLib,
    /// Forward-compatible catch-all.
    Other(String),
}

/// Boundary knob for `Domain::project`.
///
/// This is the paper-9 knob: it decides which faithful texture is discarded
/// when projecting a term into a durable contract. The initial implementation
/// only exposes a named default boundary; richer policies can grow here
/// without changing the primitive signatures.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Boundary {
    /// Human-readable policy name.
    pub name: String,
    /// Whether a projection is expected to discard the faithful term.
    pub discard_faithful_term: bool,
}

impl Default for Boundary {
    fn default() -> Self {
        Self {
            name: "paper-9-default".to_string(),
            discard_faithful_term: false,
        }
    }
}

/// Ed25519 attestation attached by primitive 8, `sign`.
///
/// The attestation is authoring metadata. It is verified when present but is
/// excluded from [`DomainClaim`]'s canonical bytes so verification remains
/// author-independent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Attestation {
    /// Self-identifying public key, `ed25519:<base64>`.
    pub signer: String,
    /// Self-identifying signature, `ed25519:<base64>`.
    pub signature: String,
    /// CID of the unsigned claim bytes covered by the signature.
    #[serde(rename = "signedCid")]
    pub signed_cid: Cid,
}

/// Central IPath type: a domain-specific path from input CIDs to an output CID.
///
/// A `DomainClaim` is unresolved when no witness is present, and resolved when
/// `witness` and `verdict` agree. `term` is the faithful stratum and may be
/// absent after projection; `contract` is the durable WP projection.
#[derive(Debug, Clone)]
pub struct DomainClaim {
    /// Domain polymorphism axis.
    pub domain: DomainKind,
    /// Optional faithful term, discardable for verification.
    pub term: Option<Term>,
    /// Durable lossy projection.
    pub contract: Contract,
    /// Input endpoint CIDs.
    pub from: Vec<Cid>,
    /// Immediate input claim CIDs used to derive this claim.
    pub premises: Vec<Cid>,
    /// Output endpoint CID.
    pub to: Cid,
    /// Optional content-addressed witness.
    pub witness: Option<Witness>,
    /// Current discharge verdict.
    pub verdict: Verdict,
    /// Optional signer attestation, excluded from claim identity.
    pub attestation: Option<Attestation>,
}

impl DomainClaim {
    /// Return `address(self)`: the signer-independent CID of this path value.
    pub fn cid(&self) -> Cid {
        super::primitives::address(self)
    }

    /// Return the canonical bytes used by [`DomainClaim::cid`].
    pub fn canonical_bytes(&self) -> Vec<u8> {
        <Self as Canonical>::canonical_bytes(self)
    }

    /// Clone the claim without courtesy-layer attestation metadata.
    pub fn unsigned(&self) -> Self {
        let mut clone = self.clone();
        clone.attestation = None;
        clone
    }
}

/// A proved domain claim.
///
/// `Truth` is self-verifying by the `verify` verb: resolve the claim bytes,
/// recompute the address, check any signature, then re-walk the witness in
/// `Domain::discharge(Check)` mode.
#[derive(Debug, Clone)]
pub struct Truth(DomainClaim);

/// A refuted domain claim.
///
/// A `Refutation` is the finding type. It is also valid input to a fresh
/// transform, which is what lets droppers operate on negative space.
#[derive(Debug, Clone)]
pub struct Refutation(DomainClaim);

impl Truth {
    /// Borrow the proved claim.
    pub fn claim(&self) -> &DomainClaim {
        &self.0
    }

    /// Consume the wrapper and return the proved claim.
    pub fn into_claim(self) -> DomainClaim {
        self.0
    }
}

impl Refutation {
    /// Borrow the refuted claim.
    pub fn claim(&self) -> &DomainClaim {
        &self.0
    }

    /// Consume the wrapper and return the refuted claim.
    pub fn into_claim(self) -> DomainClaim {
        self.0
    }
}

impl TryFrom<DomainClaim> for Truth {
    type Error = VerdictCoercionError;

    fn try_from(value: DomainClaim) -> Result<Self, Self::Error> {
        if value.verdict == Verdict::Proved {
            Ok(Self(value))
        } else {
            Err(VerdictCoercionError::ExpectedProved {
                actual: value.verdict,
            })
        }
    }
}

impl TryFrom<DomainClaim> for Refutation {
    type Error = VerdictCoercionError;

    fn try_from(value: DomainClaim) -> Result<Self, Self::Error> {
        if value.verdict == Verdict::Refuted {
            Ok(Self(value))
        } else {
            Err(VerdictCoercionError::ExpectedRefuted {
                actual: value.verdict,
            })
        }
    }
}

/// Error when coercing a raw claim into a verdict-refined newtype.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum VerdictCoercionError {
    /// `Truth` requires `Verdict::Proved`.
    #[error("expected proved claim, got {actual:?}")]
    ExpectedProved { actual: Verdict },
    /// `Refutation` requires `Verdict::Refuted`.
    #[error("expected refuted claim, got {actual:?}")]
    ExpectedRefuted { actual: Verdict },
}

/// Closed input universe for transforms.
///
/// A source file, a spec, a raw claim, a `Truth`, a `Refutation`, or a faithful
/// term can all be transformed again. This closure is what lets proofs,
/// findings, and synthesized completions re-enter the algebra.
#[derive(Debug, Clone)]
pub enum Input {
    /// Dialect-specific source bytes.
    Source { dialect: Dialect, bytes: Vec<u8> },
    /// JSON spec input; `mint` is `transform` over this variant.
    Spec(JsonValue),
    /// Raw claim input.
    Claim(DomainClaim),
    /// Proved claim input.
    Truth(Truth),
    /// Refuted finding input.
    Refutation(Refutation),
    /// Faithful term input.
    Term(Term),
}

impl Canonical for String {
    fn canonical_bytes(&self) -> Vec<u8> {
        jcs_bytes_for_json(JsonValue::String(self.clone()))
    }
}

impl Canonical for &str {
    fn canonical_bytes(&self) -> Vec<u8> {
        jcs_bytes_for_json(JsonValue::String((*self).to_string()))
    }
}

impl Canonical for Cid {
    fn canonical_bytes(&self) -> Vec<u8> {
        jcs_bytes_for_json(JsonValue::String(self.0.clone()))
    }
}

impl Canonical for Term {
    fn canonical_bytes(&self) -> Vec<u8> {
        jcs_bytes_for_serialize(self)
    }
}

impl Canonical for IrFormula {
    fn canonical_bytes(&self) -> Vec<u8> {
        jcs_bytes_for_serialize(self)
    }
}

impl Canonical for FunctionContractMemento {
    fn canonical_bytes(&self) -> Vec<u8> {
        if !self.canonical_bytes.is_empty() {
            self.canonical_bytes.clone()
        } else {
            jcs_bytes_of_value(&build_memento_value(self))
        }
    }
}

impl Canonical for Witness {
    fn canonical_bytes(&self) -> Vec<u8> {
        jcs_bytes_for_serialize(self)
    }
}

impl Canonical for DomainClaim {
    fn canonical_bytes(&self) -> Vec<u8> {
        let value = domain_claim_to_value(self);
        encode_jcs(&value).into_bytes()
    }
}

pub(crate) fn jcs_bytes_for_serialize<T: Serialize>(value: &T) -> Vec<u8> {
    let json = serde_json::to_value(value).expect("core value serializes to JSON");
    jcs_bytes_for_json(json)
}

pub(crate) fn jcs_bytes_for_json(value: JsonValue) -> Vec<u8> {
    encode_jcs(&json_to_cvalue(value)).into_bytes()
}

pub(crate) fn json_to_cvalue(value: JsonValue) -> Arc<CValue> {
    match value {
        JsonValue::Null => CValue::null(),
        JsonValue::Bool(value) => CValue::boolean(value),
        JsonValue::Number(number) => {
            if let Some(value) = number.as_i64() {
                CValue::integer(value)
            } else if let Some(value) = number.as_u64() {
                match i64::try_from(value) {
                    Ok(value) => CValue::integer(value),
                    Err(_) => CValue::object([(
                        "__provekit_non_i64_number__",
                        CValue::string(number.to_string()),
                    )]),
                }
            } else {
                CValue::object([(
                    "__provekit_non_i64_number__",
                    CValue::string(number.to_string()),
                )])
            }
        }
        JsonValue::String(value) => CValue::string(value),
        JsonValue::Array(items) => CValue::array(items.into_iter().map(json_to_cvalue).collect()),
        JsonValue::Object(map) => CValue::object(
            map.into_iter()
                .map(|(key, value)| (key, json_to_cvalue(value))),
        ),
    }
}

pub(crate) fn contract_to_cvalue(contract: &Contract) -> Arc<CValue> {
    let bytes = <Contract as Canonical>::canonical_bytes(contract);
    match serde_json::from_slice::<JsonValue>(&bytes) {
        Ok(value) => json_to_cvalue(value),
        Err(_) => build_memento_value(contract),
    }
}

pub(crate) fn formula_true() -> IrFormula {
    IrFormula::Atomic {
        name: "true".to_string(),
        args: vec![],
    }
}

pub(crate) fn any_sort() -> Sort {
    Sort::Primitive {
        name: "Any".to_string(),
    }
}

pub(crate) fn memento_from_parts(
    fn_name: String,
    formals: Vec<String>,
    formal_sorts: Vec<Sort>,
    return_sort: Sort,
    pre: IrFormula,
    post: IrFormula,
    body_cid: Option<String>,
) -> Contract {
    let effects = crate::compose::EffectSet::empty();
    let locus = Locus::unknown();
    let value = crate::compose::build_value(
        &fn_name,
        &formals,
        &formal_sorts,
        &return_sort,
        &pre,
        &post,
        body_cid.as_deref(),
        &effects,
        &locus,
        &[],
    );
    let canonical_bytes = jcs_bytes_of_value(&value);
    let cid = crate::compose::cid_of_value(&value);
    Contract {
        fn_name,
        formals,
        formal_sorts,
        formal_regions: vec![],
        return_sort,
        return_region: None,
        pre,
        post,
        body_cid,
        effects,
        locus,
        canonical_bytes,
        cid,
        auto_minted_mementos: vec![],
    }
}

fn domain_claim_to_value(claim: &DomainClaim) -> Arc<CValue> {
    let from_values: Vec<Arc<CValue>> = claim
        .from
        .iter()
        .map(|cid| CValue::string(cid.as_str().to_string()))
        .collect();
    let premise_values: Vec<Arc<CValue>> = claim
        .premises
        .iter()
        .map(|cid| CValue::string(cid.as_str().to_string()))
        .collect();
    let term_value = match &claim.term {
        Some(term) => json_to_cvalue(serde_json::to_value(term).expect("term serializes")),
        None => CValue::null(),
    };
    let witness_value = match &claim.witness {
        Some(witness) => json_to_cvalue(serde_json::to_value(witness).expect("witness serializes")),
        None => CValue::null(),
    };

    CValue::object([
        ("contract", contract_to_cvalue(&claim.contract)),
        (
            "domain",
            json_to_cvalue(serde_json::to_value(&claim.domain).expect("domain serializes")),
        ),
        ("from", CValue::array(from_values)),
        ("premises", CValue::array(premise_values)),
        ("term", term_value),
        ("to", CValue::string(claim.to.as_str().to_string())),
        (
            "verdict",
            json_to_cvalue(serde_json::to_value(claim.verdict).expect("verdict serializes")),
        ),
        ("witness", witness_value),
    ])
}

fn operation_name_cid(name: &str) -> Cid {
    let value = CValue::object([
        ("kind", CValue::string("operation-name")),
        ("name", CValue::string(name.to_string())),
    ]);
    Cid::from_hash_output(blake3_512_of(encode_jcs(&value).as_bytes()))
}

#[allow(dead_code)]
fn _let_binding_from_term(name: String, bound_term: Term) -> LetBinding {
    LetBinding {
        name,
        bound_term: IrTerm::from(bound_term),
    }
}
