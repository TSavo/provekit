// SPDX-License-Identifier: Apache-2.0

use std::collections::{BTreeMap, BTreeSet};
use std::convert::TryFrom;
use std::fmt;
use std::sync::Arc;

use provekit_canonicalizer::{blake3_512_of, encode_jcs, Value as CValue};
use provekit_ir_types::{IrFormula, IrTerm, LetBinding, Sort};
use serde::{de, Deserialize, Deserializer, Serialize, Serializer};
use serde_json::Value as JsonValue;
use thiserror::Error;

use crate::compose::{build_memento_value, jcs_bytes_of_value, FunctionContractMemento, Locus};

use super::traits::Canonical;

/// A flat BLAKE3-512 content identifier: `blake3-512:<128 lowercase hex>`.
///
/// `Cid` is the algebra's address type. It has no internal structure beyond
/// the hash algorithm tag and hex digest; any semantic meaning comes from the
/// catalog entry found by `resolve`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
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

impl Serialize for Cid {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for Cid {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::parse(value).map_err(de::Error::custom)
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

/// Declared child ordering policy for one operation in a language signature.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ArityShape {
    /// Child order is semantic and index-bearing.
    Positional { arity: usize },
    /// Child slots are named; order in the source vector only assigns slots.
    Named { slots: Vec<AritySlot> },
    /// Child order is not semantic; children are sorted by child CID.
    Set {
        /// Uniform member sort for this set.
        #[serde(default, skip_serializing_if = "slot_sort_is_default")]
        member_sort: SlotSort,
    },
}

/// Evaluation status declared for a child slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SlotEvaluation {
    /// The child participates in normal evaluation.
    #[default]
    Evaluated,
    /// The child is structurally present but its side effects do not fire.
    Unevaluated,
}

/// Declared kind of value addressed by a child slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SlotSort {
    /// A normal algebra term.
    #[default]
    Term,
    /// A type-level child.
    Type,
    /// An identifier/designator child.
    Identifier,
    /// A literal payload child.
    Literal,
}

/// One named child slot in an operation arity declaration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AritySlot {
    /// Canonical slot name.
    pub name: String,
    /// Whether the algebra evaluates this child.
    #[serde(default, skip_serializing_if = "slot_evaluation_is_default")]
    pub evaluation: SlotEvaluation,
    /// The kind of child addressed by this slot.
    #[serde(default, skip_serializing_if = "slot_sort_is_default")]
    pub slot_sort: SlotSort,
    /// Optional nested shape for this slot's child value.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shape: Option<Box<ArityShape>>,
}

impl AritySlot {
    /// Construct an evaluated slot.
    pub fn evaluated(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            evaluation: SlotEvaluation::Evaluated,
            slot_sort: SlotSort::Term,
            shape: None,
        }
    }

    /// Construct an unevaluated slot.
    pub fn unevaluated(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            evaluation: SlotEvaluation::Unevaluated,
            slot_sort: SlotSort::Term,
            shape: None,
        }
    }

    /// Attach an explicit slot sort.
    pub fn with_slot_sort(mut self, slot_sort: SlotSort) -> Self {
        self.slot_sort = slot_sort;
        self
    }

    /// Attach a nested shape to this slot.
    pub fn with_shape(mut self, shape: ArityShape) -> Self {
        self.shape = Some(Box::new(shape));
        self
    }
}

impl ArityShape {
    /// Construct a positional child shape.
    pub fn positional(arity: usize) -> Self {
        Self::Positional { arity }
    }

    /// Construct a named-record child shape.
    pub fn named<const N: usize>(slots: [&str; N]) -> Self {
        Self::Named {
            slots: slots.into_iter().map(AritySlot::evaluated).collect(),
        }
    }

    /// Construct a named-record shape with explicit slot evaluation.
    pub fn named_slots<const N: usize>(slots: [AritySlot; N]) -> Self {
        Self::Named {
            slots: slots.into_iter().collect(),
        }
    }

    /// Construct an unordered-set child shape.
    pub fn set() -> Self {
        Self::Set {
            member_sort: SlotSort::Term,
        }
    }

    /// Construct an unordered-set child shape with a uniform member sort.
    pub fn set_of(member_sort: SlotSort) -> Self {
        Self::Set { member_sort }
    }
}

/// One operation entry inside a language signature catalog.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OperationSignature {
    /// Operation memento CID.
    pub op_cid: Cid,
    /// Child ordering policy declared by the catalog.
    pub arity_shape: ArityShape,
}

/// A resolved language signature with declared operation child shapes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LanguageSignature {
    cid: Cid,
    operations: BTreeMap<String, OperationSignature>,
}

impl LanguageSignature {
    /// Create an empty signature entry addressed by `cid`.
    pub fn new(cid: Cid) -> Self {
        Self {
            cid,
            operations: BTreeMap::new(),
        }
    }

    /// Borrow the language-signature CID.
    pub fn cid(&self) -> &Cid {
        &self.cid
    }

    /// Add or replace one operation shape declaration.
    pub fn with_operation(
        mut self,
        name: impl Into<String>,
        op_cid: Cid,
        arity_shape: ArityShape,
    ) -> Self {
        self.operations.insert(
            name.into(),
            OperationSignature {
                op_cid,
                arity_shape,
            },
        );
        self
    }

    /// Return one operation declaration.
    pub fn operation(&self, name: &str) -> Option<&OperationSignature> {
        self.operations.get(name)
    }

    /// Compute a shape-aware CID for a term in this language signature.
    pub fn term_cid(&self, term: &Term) -> Result<Cid, SignatureCatalogError> {
        let value = self.term_value(term)?;
        Ok(Cid::from_hash_output(blake3_512_of(
            encode_jcs(value.as_ref()).as_bytes(),
        )))
    }

    fn term_value(&self, term: &Term) -> Result<Arc<CValue>, SignatureCatalogError> {
        match term {
            Term::Op { op_cid, name, args } => {
                let operation = self.operations.get(name).ok_or_else(|| {
                    SignatureCatalogError::UnknownOperation { name: name.clone() }
                })?;
                if &operation.op_cid != op_cid {
                    return Err(SignatureCatalogError::OperationCidMismatch {
                        name: name.clone(),
                        expected: operation.op_cid.clone(),
                        actual: op_cid.clone(),
                    });
                }
                let children = self.children_value(args, &operation.arity_shape)?;
                Ok(CValue::object([
                    ("kind", CValue::string("op")),
                    (
                        "signatureCid",
                        CValue::string(self.cid.as_str().to_string()),
                    ),
                    ("opCid", CValue::string(op_cid.as_str().to_string())),
                    ("name", CValue::string(name.clone())),
                    ("arityShape", arity_shape_value(&operation.arity_shape)),
                    ("children", children),
                ]))
            }
            Term::Var { name } => Ok(CValue::object([
                ("kind", CValue::string("var")),
                ("name", CValue::string(name.clone())),
            ])),
            Term::Const { value, sort } => Ok(CValue::object([
                ("kind", CValue::string("const")),
                ("value", json_to_cvalue(value.clone())),
                (
                    "sort",
                    json_to_cvalue(serde_json::to_value(sort).expect("sort serializes")),
                ),
            ])),
            Term::Unit => Ok(CValue::object([("kind", CValue::string("unit"))])),
        }
    }

    fn children_value(
        &self,
        args: &[Term],
        shape: &ArityShape,
    ) -> Result<Arc<CValue>, SignatureCatalogError> {
        match shape {
            ArityShape::Positional { arity } => {
                require_arity(*arity, args.len(), shape)?;
                Ok(CValue::array(
                    args.iter()
                        .map(|arg| self.term_value(arg))
                        .collect::<Result<Vec<_>, _>>()?,
                ))
            }
            ArityShape::Named { slots } => {
                require_arity(slots.len(), args.len(), shape)?;
                Ok(CValue::object(
                    slots
                        .iter()
                        .zip(args)
                        .map(|(slot, arg)| {
                            self.term_value(arg).map(|value| (slot.name.clone(), value))
                        })
                        .collect::<Result<Vec<_>, _>>()?,
                ))
            }
            ArityShape::Set { .. } => {
                let mut children = args
                    .iter()
                    .map(|arg| {
                        let value = self.term_value(arg)?;
                        let cid = Cid::from_hash_output(blake3_512_of(
                            encode_jcs(value.as_ref()).as_bytes(),
                        ));
                        Ok((cid, value))
                    })
                    .collect::<Result<Vec<_>, SignatureCatalogError>>()?;
                children.sort_by(|left, right| left.0.cmp(&right.0));
                Ok(CValue::array(
                    children.into_iter().map(|(_, value)| value).collect(),
                ))
            }
        }
    }
}

/// Errors from shape-aware language signature use.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum SignatureCatalogError {
    /// A term names an operation not present in the signature.
    #[error("language signature is missing operation `{name}`")]
    UnknownOperation { name: String },
    /// The term's op CID does not match the catalog entry for the name.
    #[error("operation `{name}` CID mismatch: expected {expected}, got {actual}")]
    OperationCidMismatch {
        name: String,
        expected: Cid,
        actual: Cid,
    },
    /// The number of supplied children does not match the declared shape.
    #[error("arity shape {shape:?} expected {expected} children, got {actual}")]
    ArityMismatch {
        shape: ArityShape,
        expected: usize,
        actual: usize,
    },
}

fn require_arity(
    expected: usize,
    actual: usize,
    shape: &ArityShape,
) -> Result<(), SignatureCatalogError> {
    if expected == actual {
        Ok(())
    } else {
        Err(SignatureCatalogError::ArityMismatch {
            shape: shape.clone(),
            expected,
            actual,
        })
    }
}

fn arity_shape_value(shape: &ArityShape) -> Arc<CValue> {
    match shape {
        ArityShape::Positional { arity } => CValue::object([
            ("kind", CValue::string("positional")),
            ("arity", CValue::integer(*arity as i64)),
        ]),
        ArityShape::Named { slots } => CValue::object([
            ("kind", CValue::string("named")),
            (
                "slots",
                CValue::array(
                    slots
                        .iter()
                        .map(|slot| {
                            let mut fields = vec![("name", CValue::string(slot.name.clone()))];
                            if slot.evaluation == SlotEvaluation::Unevaluated {
                                fields.push(("evaluation", CValue::string("unevaluated")));
                            }
                            if slot.slot_sort != SlotSort::Term {
                                fields.push((
                                    "slot_sort",
                                    CValue::string(match slot.slot_sort {
                                        SlotSort::Term => "term",
                                        SlotSort::Type => "type",
                                        SlotSort::Identifier => "identifier",
                                        SlotSort::Literal => "literal",
                                    }),
                                ));
                            }
                            if let Some(shape) = &slot.shape {
                                fields.push(("shape", arity_shape_value(shape)));
                            }
                            CValue::object(fields)
                        })
                        .collect(),
                ),
            ),
        ]),
        ArityShape::Set { member_sort } => {
            let mut fields = vec![("kind", CValue::string("set"))];
            if *member_sort != SlotSort::Term {
                fields.push((
                    "member_sort",
                    CValue::string(match member_sort {
                        SlotSort::Term => "term",
                        SlotSort::Type => "type",
                        SlotSort::Identifier => "identifier",
                        SlotSort::Literal => "literal",
                    }),
                ));
            }
            CValue::object(fields)
        }
    }
}

fn slot_evaluation_is_default(evaluation: &SlotEvaluation) -> bool {
    *evaluation == SlotEvaluation::Evaluated
}

fn slot_sort_is_default(slot_sort: &SlotSort) -> bool {
    *slot_sort == SlotSort::Term
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
/// `witness` and `verdict` agree. Faithful language-specific terms live in
/// addressed artifacts owned by kits; the primitive carries only their CIDs.
#[derive(Debug, Clone)]
pub struct DomainClaim {
    /// Domain polymorphism axis.
    pub domain: DomainKind,
    /// Durable lossy projection.
    pub contract: Contract,
    /// Kit-owned artifacts used to derive this claim.
    pub artifacts: Vec<Cid>,
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

    /// Union this claim with another claim in the same domain.
    pub fn union(&self, other: &Self) -> Result<Self, super::primitives::ComposeError> {
        super::primitives::compose(self, other)
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

/// A composed transformation path.
///
/// A path is a set of algebra steps. Each step names a kit transform, its
/// input, and the step names it depends on. Executing a command is still one
/// `Kit(Input) -> DomainClaim`; `Input::Path` carries the algebra needed for
/// that kit to compose subordinate transforms when necessary.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Path {
    /// Algebra steps available to the transform.
    pub algebra: Vec<PathAlgebra>,
}

impl Path {
    /// Return `address(self)`: the CID of this language-neutral composition value.
    pub fn cid(&self) -> Cid {
        super::primitives::address(self)
    }

    /// Find one named algebra step.
    pub fn step(&self, name: &str) -> Option<&PathAlgebra> {
        self.algebra.iter().find(|step| step.name == name)
    }

    /// Return algebra steps in dependency order.
    pub fn ordered_steps(&self) -> Result<Vec<&PathAlgebra>, PathError> {
        let mut steps = BTreeMap::new();
        for step in &self.algebra {
            if steps.insert(step.name.clone(), step).is_some() {
                return Err(PathError::DuplicateStep {
                    name: step.name.clone(),
                });
            }
        }

        let mut incoming = BTreeMap::new();
        let mut outgoing: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        for step in &self.algebra {
            let mut dependencies = BTreeSet::new();
            for dependency in &step.depends_on {
                if !steps.contains_key(dependency) {
                    return Err(PathError::MissingDependency {
                        step: step.name.clone(),
                        dependency: dependency.clone(),
                    });
                }
                dependencies.insert(dependency.clone());
                outgoing
                    .entry(dependency.clone())
                    .or_default()
                    .insert(step.name.clone());
            }
            incoming.insert(step.name.clone(), dependencies);
        }

        let mut ready: BTreeSet<String> = incoming
            .iter()
            .filter_map(|(name, dependencies)| {
                if dependencies.is_empty() {
                    Some(name.clone())
                } else {
                    None
                }
            })
            .collect();
        let mut ordered = Vec::with_capacity(self.algebra.len());

        while let Some(name) = ready.iter().next().cloned() {
            ready.remove(&name);
            ordered.push(*steps.get(&name).expect("ready step exists"));

            for dependent in outgoing.get(&name).into_iter().flatten() {
                let dependencies = incoming
                    .get_mut(dependent)
                    .expect("dependent step has incoming set");
                dependencies.remove(&name);
                if dependencies.is_empty() {
                    ready.insert(dependent.clone());
                }
            }
        }

        if ordered.len() != self.algebra.len() {
            let step = incoming
                .iter()
                .find_map(|(name, dependencies)| {
                    if dependencies.is_empty() {
                        None
                    } else {
                        Some(name.clone())
                    }
                })
                .unwrap_or_default();
            return Err(PathError::Cycle { step });
        }

        Ok(ordered)
    }

    /// Return steps that no other step depends on.
    pub fn terminal_steps(&self) -> Vec<&PathAlgebra> {
        let dependencies: BTreeSet<&str> = self
            .algebra
            .iter()
            .flat_map(|step| step.depends_on.iter().map(String::as_str))
            .collect();
        let mut terminals: Vec<&PathAlgebra> = self
            .algebra
            .iter()
            .filter(|step| !dependencies.contains(step.name.as_str()))
            .collect();
        terminals.sort_by(|left, right| left.name.cmp(&right.name));
        terminals
    }
}

/// One algebra step inside a [`Path`].
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PathAlgebra {
    /// Stable step name inside the path.
    pub name: String,
    /// Kit transform to apply.
    pub kit: String,
    /// Input artifact CIDs to that transform.
    pub inputs: Vec<Cid>,
    /// Names of prerequisite algebra steps.
    pub depends_on: Vec<String>,
}

/// Invalid path algebra.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum PathError {
    /// Two steps share the same stable name.
    #[error("path contains duplicate step `{name}`")]
    DuplicateStep { name: String },
    /// A step names a prerequisite that does not exist in the path.
    #[error("path step `{step}` depends on missing step `{dependency}`")]
    MissingDependency { step: String, dependency: String },
    /// The dependency graph contains a cycle.
    #[error("path dependency cycle includes step `{step}`")]
    Cycle { step: String },
}

/// Stable top-level kind for serialized path documents.
pub const PATH_DOCUMENT_KIND: &str = "provekit-path/v1";

/// Serializable, language-neutral path plus the materialized input catalog it
/// needs to execute.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PathDocument {
    /// Wire kind/version for disk and cross-runtime transport.
    pub kind: String,
    /// The CID-able path algebra.
    pub path: Path,
    /// Materialized inputs keyed by the CIDs referenced from path steps.
    #[serde(default)]
    pub inputs: Vec<PathInputBinding>,
}

impl PathDocument {
    /// Build a path document and address each materialized input.
    pub fn from_path_and_inputs(path: Path, inputs: Vec<Input>) -> Result<Self, PathDocumentError> {
        let mut bindings = Vec::with_capacity(inputs.len());
        for input in inputs {
            bindings.push(PathInputBinding {
                cid: super::primitives::address(&input),
                input: PathInputMaterial::try_from_input(input)?,
            });
        }
        Ok(Self {
            kind: PATH_DOCUMENT_KIND.to_string(),
            path,
            inputs: bindings,
        })
    }

    /// Validate and materialize the typed input catalog entries.
    pub fn materialized_inputs(&self) -> Result<Vec<(Cid, Input)>, PathDocumentError> {
        if self.kind != PATH_DOCUMENT_KIND {
            return Err(PathDocumentError::InvalidKind {
                expected: PATH_DOCUMENT_KIND,
                actual: self.kind.clone(),
            });
        }

        let mut out = Vec::with_capacity(self.inputs.len());
        for binding in &self.inputs {
            let input = binding.input.to_input();
            let actual = super::primitives::address(&input);
            if actual != binding.cid {
                return Err(PathDocumentError::InputCidMismatch {
                    expected: binding.cid.clone(),
                    actual,
                });
            }
            out.push((binding.cid.clone(), input));
        }
        Ok(out)
    }
}

/// One materialized input entry in a [`PathDocument`].
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PathInputBinding {
    /// CID referenced from `PathAlgebra.inputs`.
    pub cid: Cid,
    /// Materialized input value.
    pub input: PathInputMaterial,
}

/// Disk-safe subset of [`Input`] used by path documents.
///
/// The first durable shape only needs command specs. Claim/truth/refutation
/// inputs should move here once their contract representation has a stable
/// language-neutral wire form.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum PathInputMaterial {
    /// JSON command/spec input.
    Spec { value: JsonValue },
}

impl PathInputMaterial {
    fn try_from_input(input: Input) -> Result<Self, PathDocumentError> {
        match input {
            Input::Spec(value) => Ok(Self::Spec { value }),
            other => Err(PathDocumentError::UnsupportedInputMaterial {
                kind: input_kind(&other).to_string(),
            }),
        }
    }

    fn to_input(&self) -> Input {
        match self {
            Self::Spec { value } => Input::Spec(value.clone()),
        }
    }
}

/// Serialized path document validation errors.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum PathDocumentError {
    /// The top-level `kind` is not the current path document kind.
    #[error("path document kind `{actual}` is not `{expected}`")]
    InvalidKind {
        /// Expected kind.
        expected: &'static str,
        /// Actual kind.
        actual: String,
    },
    /// A materialized input's bytes do not address to its declared CID.
    #[error("path input declared as `{expected}` materialized as `{actual}`")]
    InputCidMismatch {
        /// Declared CID.
        expected: Cid,
        /// Actual CID after canonicalization.
        actual: Cid,
    },
    /// The input variant is not yet part of the disk-safe path catalog.
    #[error("path documents currently support materialized spec inputs, not `{kind}`")]
    UnsupportedInputMaterial {
        /// Input variant name.
        kind: String,
    },
}

fn input_kind(input: &Input) -> &'static str {
    match input {
        Input::Source { .. } => "source",
        Input::Spec(_) => "spec",
        Input::Claim(_) => "claim",
        Input::Truth(_) => "truth",
        Input::Refutation(_) => "refutation",
        Input::Term(_) => "term",
        Input::Path(_) => "path",
    }
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
    /// Composed path input.
    Path(Box<Path>),
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

impl Canonical for Path {
    fn canonical_bytes(&self) -> Vec<u8> {
        encode_jcs(&path_to_value(self)).into_bytes()
    }
}

impl Canonical for Input {
    fn canonical_bytes(&self) -> Vec<u8> {
        encode_jcs(&input_to_value(self)).into_bytes()
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
        concept_hint: None,
    }
}

fn domain_claim_to_value(claim: &DomainClaim) -> Arc<CValue> {
    let mut artifacts = claim.artifacts.clone();
    artifacts.sort();
    artifacts.dedup();
    let artifact_values: Vec<Arc<CValue>> = artifacts
        .iter()
        .map(|cid| CValue::string(cid.as_str().to_string()))
        .collect();
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
    let witness_value = match &claim.witness {
        Some(witness) => json_to_cvalue(serde_json::to_value(witness).expect("witness serializes")),
        None => CValue::null(),
    };

    CValue::object([
        ("artifacts", CValue::array(artifact_values)),
        ("contract", contract_to_cvalue(&claim.contract)),
        (
            "domain",
            json_to_cvalue(serde_json::to_value(&claim.domain).expect("domain serializes")),
        ),
        ("from", CValue::array(from_values)),
        ("premises", CValue::array(premise_values)),
        ("to", CValue::string(claim.to.as_str().to_string())),
        (
            "verdict",
            json_to_cvalue(serde_json::to_value(claim.verdict).expect("verdict serializes")),
        ),
        ("witness", witness_value),
    ])
}

fn input_to_value(input: &Input) -> Arc<CValue> {
    match input {
        Input::Source { dialect, bytes } => CValue::object([
            ("kind", CValue::string("source")),
            (
                "dialect",
                json_to_cvalue(serde_json::to_value(dialect).expect("dialect serializes")),
            ),
            (
                "bytes",
                CValue::array(
                    bytes
                        .iter()
                        .map(|byte| CValue::integer(i64::from(*byte)))
                        .collect(),
                ),
            ),
        ]),
        Input::Spec(value) => CValue::object([
            ("kind", CValue::string("spec")),
            ("value", json_to_cvalue(value.clone())),
        ]),
        Input::Claim(claim) => CValue::object([
            ("kind", CValue::string("claim")),
            ("claim", domain_claim_to_value(claim)),
        ]),
        Input::Truth(truth) => CValue::object([
            ("kind", CValue::string("truth")),
            ("claim", domain_claim_to_value(truth.claim())),
        ]),
        Input::Refutation(refutation) => CValue::object([
            ("kind", CValue::string("refutation")),
            ("claim", domain_claim_to_value(refutation.claim())),
        ]),
        Input::Term(term) => CValue::object([
            ("kind", CValue::string("term")),
            (
                "term",
                json_to_cvalue(serde_json::to_value(term).expect("term serializes")),
            ),
        ]),
        Input::Path(path) => CValue::object([
            ("kind", CValue::string("path")),
            ("path", path_to_value(path.as_ref())),
        ]),
    }
}

fn path_to_value(path: &Path) -> Arc<CValue> {
    let mut steps: Vec<&PathAlgebra> = path.algebra.iter().collect();
    steps.sort_by(|left, right| left.name.cmp(&right.name));
    CValue::array(steps.into_iter().map(path_algebra_to_value).collect())
}

fn path_algebra_to_value(step: &PathAlgebra) -> Arc<CValue> {
    let mut depends_on = step.depends_on.clone();
    depends_on.sort();
    depends_on.dedup();
    let mut inputs = step.inputs.clone();
    inputs.sort();
    inputs.dedup();
    CValue::object([
        (
            "inputs",
            CValue::array(
                inputs
                    .into_iter()
                    .map(|cid| CValue::string(cid.as_str().to_string()))
                    .collect(),
            ),
        ),
        ("kit", CValue::string(step.kit.clone())),
        ("name", CValue::string(step.name.clone())),
        (
            "dependsOn",
            CValue::array(depends_on.into_iter().map(CValue::string).collect()),
        ),
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
