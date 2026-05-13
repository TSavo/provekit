// SPDX-License-Identifier: Apache-2.0
//
// GENERATED FILE: DO NOT EDIT
// Source: protocol/provekit-ir.cddl
// Generator: provekit-ir-codegen

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum Declaration {
    #[serde(rename = "contract")]
    Contract {
        name: String,
        #[serde(rename = "outBinding")]
        out_binding: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        pre: Option<IrFormula>,
        #[serde(skip_serializing_if = "Option::is_none")]
        post: Option<IrFormula>,
        #[serde(skip_serializing_if = "Option::is_none")]
        inv: Option<IrFormula>,
    },
    #[serde(rename = "bridge")]
    Bridge {
        name: String,
        #[serde(rename = "sourceSymbol")]
        source_symbol: String,
        #[serde(rename = "sourceLayer")]
        source_layer: String,
        #[serde(rename = "sourceContractCid")]
        source_contract_cid: String,
        #[serde(rename = "targetContractCid")]
        target_contract_cid: String,
        #[serde(rename = "targetProofCid")]
        target_proof_cid: String,
        #[serde(rename = "targetLayer")]
        target_layer: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        notes: Option<String>,
    },
}

pub type Document = Vec<Declaration>;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LetBinding {
    pub name: String,
    #[serde(rename = "boundTerm")]
    pub bound_term: IrTerm,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PrimitiveSort {
    pub kind: String,
    pub name: PrimitiveSortName,
}

pub type PrimitiveSortName = String;
// Known values for PrimitiveSortName:
//   "Int"
//   "Real"
//   "Bool"
//   "String"

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BridgeHeaderV14 {
    #[serde(rename = "schemaVersion")]
    pub schema_version: String,
    pub kind: String,
    pub name: String,
    #[serde(rename = "sourceSymbol")]
    pub source_symbol: String,
    #[serde(rename = "sourceLayer")]
    pub source_layer: String,
    #[serde(rename = "sourceContractCid")]
    pub source_contract_cid: String,
    pub target: BridgeTarget,
}

pub type AtomicPredicateName = String;
// Known values for AtomicPredicateName:
//   "="
//   "≠"
//   "<"
//   "≤"
//   ">"
//   "≥"
//   "true"
//   "false"
//   "bvult"
//   "bvule"
//   "bvugt"
//   "bvuge"
//   "bvslt"
//   "bvsle"
//   "bvsgt"
//   "bvsge"

pub type QuantifierKind = String;
// Known values for QuantifierKind:
//   "forall"
//   "exists"

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvidenceCertificate {
    pub tool: String,
    pub version: String,
    #[serde(rename = "formulaHash")]
    pub formula_hash: String,
    #[serde(rename = "proofData")]
    pub proof_data: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BridgeMetadataV14 {
    #[serde(rename = "targetWitnessCid")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_witness_cid: Option<String>,
    #[serde(rename = "targetBinaryCid")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_binary_cid: Option<String>,
    #[serde(rename = "targetLayer")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_layer: Option<String>,
    #[serde(rename = "targetContractSetCid")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_contract_set_cid: Option<String>,
    #[serde(rename = "producedBy")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub produced_by: Option<String>,
    #[serde(rename = "producedAt")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub produced_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum BridgeTarget {
    #[serde(rename = "contract")]
    Contract { cid: String },
    #[serde(rename = "contractSet")]
    ContractSet { cid: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvidenceTerm {
    pub kind: String,
    #[serde(rename = "proofType")]
    pub proof_type: ProofType,
    pub certificate: EvidenceCertificate,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BridgeEnvelope {
    pub signer: String,
    #[serde(rename = "declaredAt")]
    pub declared_at: String,
    pub signature: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Value {
    #[serde(rename = "text")]
    Text,
    #[serde(rename = "number")]
    Number,
    #[serde(rename = "bool")]
    Bool,
    #[serde(rename = "null")]
    Null,
}

pub type ProofType = String;

// NOTE: The `Sort` enum below has been MANUALLY extended beyond the
// codegen output to add Function + Dependent variants per the v1.5.0
// grammar grow (issue #330, rust gap from PR #361), Float per #385,
// and Region per #401 (v1.6.0 grammar grow).
// The codegen (`provekit-ir-codegen`) currently only emits the Primitive
// arm even though the CDDL spec defines a 7-way union. If you regenerate
// this file via `cargo run -p provekit-ir-codegen`, you WILL clobber the
// manual extensions. Re-apply them from this comment block down through
// the closing `}` of the `Sort` enum.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum Sort {
    #[serde(rename = "primitive")]
    Primitive { name: PrimitiveSortName },
    #[serde(rename = "function")]
    Function {
        args: Vec<Sort>,
        #[serde(rename = "return")]
        ret: Box<Sort>,
    },
    #[serde(rename = "dependent")]
    Dependent {
        name: String,
        #[serde(rename = "indexVar")]
        index_var: String,
        #[serde(rename = "indexSort")]
        index_sort: Box<Sort>,
    },
    /// IEEE-754 floating-point sort. `width` is 16, 32, 64, or 128 bits,
    /// matching Charon's FloatTy (F16/F32/F64/F128).
    ///
    /// ## NaN / IEEE-754 semantics: deliberately NOT modelled here
    ///
    /// This sort carries only the bit-width. It does NOT model:
    ///   - NaN equality (NaN ≠ NaN in IEEE 754, but substrate equality
    ///     is structural / total over bit patterns).
    ///   - Ordered vs. unordered comparisons (fcmp oeq vs. fcmp ueq).
    ///   - Positive-zero vs. negative-zero (+0.0 == -0.0 in IEEE 754,
    ///     but they have distinct bit patterns: 0x00000000 vs. 0x80000000
    ///     for f32). The lifter treats them as distinct bit patterns.
    ///   - Denormals, infinities, or rounding modes.
    ///
    /// The lifter preserves the raw IEEE-754 bit pattern as a u64 in the
    /// `IrTerm::Const { value }` field (tagged as `{"__float_bits__": <u64>}`
    /// to distinguish from plain integers). Downstream solvers that have a
    /// float theory can interpret the bit pattern with their own axioms.
    ///
    /// This is tracked for full treatment in #385 / a follow-up RFC.
    #[serde(rename = "float")]
    Float { width: u8 },
    /// Lifetime / region sort for borrow-checker lifetime variables.
    /// `name` is the lifetime name, e.g. `"'a"`, `"'static"`, or a fresh
    /// region variable like `"'r0"` emitted by Charon's region inference.
    ///
    /// ## Semantics
    ///
    /// Region sorts are pre-resolved in composition and MUST NOT reach the
    /// SMT or Coq backends. They exist in the IR as opaque placeholders so
    /// that lifted Rust functions with lifetime parameters can be given
    /// well-typed contracts without silently collapsing lifetimes into a
    /// primitive sort (which would break CID stability and sort-collapse
    /// invariants from #384 A.1).
    ///
    /// JCS-canonical key order: `kind`, `name` (alphabetical).
    /// Prerequisite for #384 C.9 (Outlives predicates).
    #[serde(rename = "region")]
    Region { name: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BridgeDeclarationV14 {
    pub envelope: BridgeEnvelope,
    pub header: BridgeHeaderV14,
    pub metadata: BridgeMetadataV14,
}

// ============================================================
// MANUAL EXTENSION BLOCK -- sort morphism memento (issue #794)
// Source of truth:
//   protocol/specs/2026-05-13-sort-morphism-memento.md §1
//
// This substrate type records transport between two pinned sort CIDs under
// pinned language-signature CIDs. It deliberately carries no language-specific
// conversion runtime.
//
// Locked JCS key order:
//   outer object: envelope, header, metadata
//   envelope: declaredAt, signature, signer
//   header: cid, direction, kind, precision_loss, range_loss,
//     representation_constraints, runtime_guards, schemaVersion,
//     source_language_signature_cid, source_sort_cid,
//     target_language_signature_cid, target_sort_cid
//   metadata: note (omitted when absent), source_url (omitted when absent)
// ============================================================

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SortMorphismMemento {
    pub envelope: SortMorphismEnvelope,
    pub header: SortMorphismHeader,
    pub metadata: SortMorphismMetadata,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SortMorphismEnvelope {
    #[serde(rename = "declaredAt")]
    pub declared_at: String,
    pub signature: String,
    pub signer: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SortMorphismHeader {
    pub cid: String,
    pub direction: MorphismDirection,
    pub kind: String,
    #[serde(rename = "precision_loss")]
    pub precision_loss: PrecisionLoss,
    #[serde(rename = "range_loss")]
    pub range_loss: RangeLoss,
    #[serde(rename = "representation_constraints")]
    pub representation_constraints: Vec<RepresentationConstraint>,
    #[serde(rename = "runtime_guards")]
    pub runtime_guards: Vec<RuntimeGuard>,
    #[serde(rename = "schemaVersion")]
    pub schema_version: String,
    #[serde(rename = "source_language_signature_cid")]
    pub source_language_signature_cid: String,
    #[serde(rename = "source_sort_cid")]
    pub source_sort_cid: String,
    #[serde(rename = "target_language_signature_cid")]
    pub target_language_signature_cid: String,
    #[serde(rename = "target_sort_cid")]
    pub target_sort_cid: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct SortMorphismMetadata {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    #[serde(rename = "source_url")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum MorphismDirection {
    #[serde(rename = "bidirectional")]
    Bidirectional,
    #[serde(rename = "left-to-right")]
    LeftToRight,
    #[serde(rename = "right-to-left")]
    RightToLeft,
}

pub type PrecisionLoss = String;
pub type RangeLoss = String;
pub type RuntimeFailureMode = String;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepresentationConstraint {
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub param: Option<serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeGuard {
    #[serde(rename = "failure_mode")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failure_mode: Option<RuntimeFailureMode>,
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub predicate: Option<String>,
}

// ============================================================
// End manual extension block -- sort morphism memento (issue #794)
// ============================================================

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum IrTerm {
    #[serde(rename = "var")]
    Var { name: String },
    #[serde(rename = "const")]
    Const {
        value: serde_json::Value,
        sort: Sort,
    },
    #[serde(rename = "ctor")]
    Ctor { name: String, args: Vec<IrTerm> },
    #[serde(rename = "lambda")]
    Lambda {
        #[serde(rename = "paramName")]
        param_name: String,
        #[serde(rename = "paramSort")]
        param_sort: Sort,
        body: Box<IrTerm>,
    },
    #[serde(rename = "let")]
    Let {
        bindings: Vec<LetBinding>,
        body: Box<IrTerm>,
    },
}

// NOTE: The `IrFormula` enum below has been MANUALLY extended beyond the
// codegen output to add the `Substitute` and `Apply` variants per the
// wp-as-formula spec (protocol/specs/2026-05-13-wp-as-formula.md §2.3).
// These two are the "wp-rule schema" nodes: they appear inside a
// `wp_rule` term and are reduced away by `libprovekit::wp` before any
// formula reaches a solver backend. The codegen (`provekit-ir-codegen`)
// currently emits only the 8-way union without these arms; if you
// regenerate this file via `cargo run -p provekit-ir-codegen`, you WILL
// clobber the manual extensions. Re-apply them from this comment block
// through the closing `}` of the `IrFormula` enum, keeping the CDDL
// (`protocol/provekit-ir.cddl`) as the source of truth.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum IrFormula {
    #[serde(rename = "atomic")]
    Atomic {
        name: AtomicPredicateName,
        args: Vec<IrTerm>,
    },
    #[serde(rename = "and")]
    And { operands: Vec<IrFormula> },
    #[serde(rename = "or")]
    Or { operands: Vec<IrFormula> },
    #[serde(rename = "not")]
    Not { operands: Vec<IrFormula> },
    #[serde(rename = "implies")]
    Implies { operands: Vec<IrFormula> },
    #[serde(rename = "forall")]
    Forall {
        name: String,
        sort: Sort,
        body: Box<IrFormula>,
    },
    #[serde(rename = "exists")]
    Exists {
        name: String,
        sort: Sort,
        body: Box<IrFormula>,
    },
    #[serde(rename = "choice")]
    Choice {
        #[serde(rename = "varName")]
        var_name: String,
        sort: Sort,
        body: Box<IrFormula>,
    },
    /// `substitute` - an explicit, capture-avoiding, single-variable
    /// substitution on a formula: `target` with `var` replaced by `term`.
    /// This is `Q[result_value := value_expr]` written as a node. It is
    /// needed because in a `wp_rule` schema the `target` is the
    /// postcondition meta-variable `Q`, not yet known; once `target` is
    /// ground the node can always be eliminated by performing the
    /// substitution. JCS-canonical key order: `kind`, `target`, `term`,
    /// `var` (alphabetical, with `kind` first by the tag convention).
    #[serde(rename = "substitute")]
    Substitute {
        target: Box<IrFormula>,
        term: IrTerm,
        var: String,
    },
    /// `apply` - application of a slot-transformer meta-variable
    /// (`wp_<slot>`) to one formula argument: `apply(wp_<slot>, X)` is
    /// "the weakest precondition of the term plugged into slot `<slot>`,
    /// with respect to X." When the evaluator instantiates `wp_<slot>`
    /// with the actual slot transformer the node reduces to a concrete
    /// formula. `fn` is the meta-variable name (`"wp_then_branch"`,
    /// `"wp_body"`, ...); `args` carries the single formula argument
    /// (a one-element list for forward compatibility). JCS-canonical key
    /// order: `args`, `fn`, `kind`.
    #[serde(rename = "apply")]
    Apply {
        args: Vec<IrFormula>,
        #[serde(rename = "fn")]
        r#fn: String,
    },
}

pub type ConnectiveKind = String;
// Known values for ConnectiveKind:
//   "and"
//   "or"
//   "not"
//   "implies"

pub type Term = IrTerm;
pub type Formula = IrFormula;

// ============================================================
// NOTE: Manual extension block -- abstraction layer (issue #71)
// ============================================================
//
// The types below are manually added per the convention established in
// the Sort enum above. They implement the CDDL defined in
// protocol/provekit-ir.cddl §"Abstraction layer" and are sourced from
// protocol/specs/2026-05-15-concept-hub-abstraction-layer.md §2.1-§2.4.
//
// DO NOT regenerate this file via `cargo run -p provekit-ir-codegen`
// without re-applying this block. See the Sort enum NOTE above.
//
// Key-order rule: struct field names mirror the CDDL key names exactly.
// serde_json with the default BTreeMap representation emits keys in
// lexicographic order (JCS canonical order). Fields that are
// `skip_serializing_if = "Option::is_none"` are OMITTED when None;
// the JCS bytes must match `serde_json::to_value` output exactly.
//
// "effects" is ALWAYS serialized (never skipped) even when empty,
// because it is a required field in the CDDL schema.

use std::collections::BTreeMap;

/// A map from loss-dimension name to an `ir-formula` characterizing that
/// dimension's divergence. An absent key means "no loss in that dimension."
///
/// Dimension names (§2.4 of the spec):
///   - "domain_narrowing"    -- inputs the realization cannot accept
///   - "effect_divergence"   -- inputs where observable effect set differs
///   - "structural_divergence" -- how far surface form diverges from the abstraction
///                               (always non-empty for abstraction realizations)
///   - "ub_introduction"     -- inputs where UB is introduced
///   - "value_divergence"    -- inputs where result VALUE differs
///
/// `structural_divergence` is a successor-mint addition per LSP §4.4 relative
/// to the #616 schema. Existing #616 loss-records without it read as
/// structural_divergence = None (formula = ∅). CIDs of previously-minted
/// mementos are NOT affected.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct LossRecord(pub BTreeMap<String, IrFormula>);

/// A single named slot in a `ConceptAbstractionMemento`.
///
/// Locked JCS key order: `name`, `variadic` (variadic omitted when absent).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AbstractionSlot {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub variadic: Option<bool>,
}

/// A hub node at the abstraction tier of the `concept:*` hub.
///
/// Source of truth: protocol/specs/2026-05-15-concept-hub-abstraction-layer.md §2.1
/// CDDL: protocol/provekit-ir.cddl `ConceptAbstractionMemento`
///
/// Locked JCS key order:
///   kind, operator, tier, slots, formal_sorts, result_sort, contract,
///   contract_note (omitted when absent), realizations,
///   superseded_by (omitted when absent), refines (omitted when absent)
///
/// NOTE: `realizations` is Vec (zero-or-more) in PR1. PR2 tightens to
/// one-or-more via a successor mint with `refines = <PR1 schema CID>`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConceptAbstractionMemento {
    pub kind: String, // must be "concept-abstraction"
    pub operator: String,
    pub tier: String, // must be "abstraction"
    pub slots: Vec<AbstractionSlot>,
    #[serde(rename = "formal_sorts")]
    pub formal_sorts: Vec<String>,
    #[serde(rename = "result_sort")]
    pub result_sort: String,
    pub contract: IrFormula,
    #[serde(rename = "contract_note")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub contract_note: Option<String>,
    pub realizations: Vec<String>,
    #[serde(rename = "superseded_by")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub superseded_by: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub refines: Option<String>,
}

/// The post-condition equation in a `RealizationDesugaringMemento`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RealizationPost {
    pub lhs: IrFormula,
    pub rhs: IrFormula,
}

/// A `DesugaringEquationMemento` (2026-05-11) elected into the
/// "abstraction-realization" role.
///
/// Source of truth: protocol/specs/2026-05-15-concept-hub-abstraction-layer.md §2.2
/// CDDL: protocol/provekit-ir.cddl `RealizationDesugaringMemento`
///
/// Locked JCS key order:
///   kind, fn_name, formals, formal_sorts, pre (omitted when absent),
///   post, role, direction, target_lang, loss_record,
///   discharge_receipt (omitted when absent), effects,
///   refines (omitted when absent)
///
/// NOTE: `discharge_receipt` is optional in PR1. PR2 tightens to required.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RealizationDesugaringMemento {
    pub kind: String, // must be "equation"
    #[serde(rename = "fn_name")]
    pub fn_name: String,
    pub formals: Vec<String>,
    #[serde(rename = "formal_sorts")]
    pub formal_sorts: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pre: Option<IrFormula>,
    pub post: RealizationPost,
    pub role: String,      // must be "abstraction-realization"
    pub direction: String, // must be "left-to-right"
    #[serde(rename = "target_lang")]
    pub target_lang: String,
    #[serde(rename = "loss_record")]
    pub loss_record: LossRecord,
    #[serde(rename = "discharge_receipt")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub discharge_receipt: Option<String>,
    pub effects: Vec<String>, // always [] for the equation itself; never skip
    #[serde(skip_serializing_if = "Option::is_none")]
    pub refines: Option<String>,
}

// ============================================================
// End manual extension block -- abstraction layer (issue #71)
// ============================================================

// ============================================================
// NOTE: Manual extension block -- transport gap mementos (issue #66)
// ============================================================
//
// Three new memento types per
// protocol/specs/2026-05-14-transport-gap-and-partial-morphism-protocol.md
// §1.1 (TransportGapMemento), §1.2 (PartialMorphismMemento), §1.4 (LossyMorphismMemento).
// CDDL: protocol/provekit-ir.cddl "Transport gap mementos" block.
//
// Amendment (this PR): `no-such-concept-op` added to GapKind.
//
// Key-order rule: struct field names mirror the CDDL alphabetical key order.
// Fields with `skip_serializing_if = "Option::is_none"` are OMITTED when None.
// Required fields that are always present are never skipped.

/// The `gap_kind` discriminant for a `TransportGapMemento`.
///
/// `no-such-concept-op` is an amendment in this PR: the source-language op
/// has no matching concept:* hub op at all (target_op_cid is absent).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum GapKind {
    #[serde(rename = "arity-shape-mismatch")]
    ArityShapeMismatch,
    #[serde(rename = "divergent-semantics")]
    DivergentSemantics,
    #[serde(rename = "effect-mismatch")]
    EffectMismatch,
    #[serde(rename = "missing-source-op")]
    MissingSourceOp,
    #[serde(rename = "missing-target-construct")]
    MissingTargetConstruct,
    #[serde(rename = "no-such-concept-op")]
    NoSuchConceptOp,
    #[serde(rename = "polymorphic-source-op")]
    PolymorphicSourceOp,
    #[serde(rename = "sort-mismatch")]
    SortMismatch,
    #[serde(rename = "wp-rule-mismatch")]
    WpRuleMismatch,
}

/// The `divergent_tag` sub-discriminant for `gap_kind: "divergent-semantics"`.
///
/// Wire format: a bare JSON string (e.g. `"truncated-vs-floored-modulo"`).
/// Open extension: unknown tags deserialize as `Other(String)`.
///
/// Note: `#[serde(untagged)]` is intentionally NOT used here. With all-unit
/// variants, `untagged` silently ignores `#[serde(rename)]` and serializes
/// every unit variant as `null`. Instead we use `from`/`into` impls that
/// convert through `String` so every variant maps to its spec-defined string.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "String", into = "String")]
pub enum DivergentSemanticsTag {
    BoundedVsUnboundedInteger,
    IntegerVsTrueDivision,
    OverflowBehavior,
    RoundingMode,
    ShortCircuitVsEager,
    TruncatedVsFlooredModulo,
    Other(String),
}

impl From<String> for DivergentSemanticsTag {
    fn from(s: String) -> Self {
        match s.as_str() {
            "bounded-vs-unbounded-integer" => DivergentSemanticsTag::BoundedVsUnboundedInteger,
            "integer-vs-true-division" => DivergentSemanticsTag::IntegerVsTrueDivision,
            "overflow-behavior" => DivergentSemanticsTag::OverflowBehavior,
            "rounding-mode" => DivergentSemanticsTag::RoundingMode,
            "short-circuit-vs-eager" => DivergentSemanticsTag::ShortCircuitVsEager,
            "truncated-vs-floored-modulo" => DivergentSemanticsTag::TruncatedVsFlooredModulo,
            other => DivergentSemanticsTag::Other(other.to_string()),
        }
    }
}

impl From<DivergentSemanticsTag> for String {
    fn from(tag: DivergentSemanticsTag) -> String {
        match tag {
            DivergentSemanticsTag::BoundedVsUnboundedInteger => {
                "bounded-vs-unbounded-integer".to_string()
            }
            DivergentSemanticsTag::IntegerVsTrueDivision => "integer-vs-true-division".to_string(),
            DivergentSemanticsTag::OverflowBehavior => "overflow-behavior".to_string(),
            DivergentSemanticsTag::RoundingMode => "rounding-mode".to_string(),
            DivergentSemanticsTag::ShortCircuitVsEager => "short-circuit-vs-eager".to_string(),
            DivergentSemanticsTag::TruncatedVsFlooredModulo => {
                "truncated-vs-floored-modulo".to_string()
            }
            DivergentSemanticsTag::Other(s) => s,
        }
    }
}

/// A structured delta for a field that differs between source and concept spec.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FieldDelta {
    pub got: serde_json::Value,
    pub want: serde_json::Value,
}

/// The structured diff that explains why a gap exists.
///
/// All fields optional; at least one should be present in practice.
/// Locked JCS key order (alphabetical):
///   divergent_tag, effects_delta, formal_sorts_delta, post_delta, pre_delta,
///   source_supported, wp_rule_delta
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct GapReason {
    #[serde(rename = "divergent_tag")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub divergent_tag: Option<DivergentSemanticsTag>,
    #[serde(rename = "effects_delta")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub effects_delta: Option<FieldDelta>,
    #[serde(rename = "formal_sorts_delta")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub formal_sorts_delta: Option<FieldDelta>,
    #[serde(rename = "post_delta")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub post_delta: Option<FieldDelta>,
    #[serde(rename = "pre_delta")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pre_delta: Option<FieldDelta>,
    #[serde(rename = "source_supported")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_supported: Option<bool>,
    #[serde(rename = "wp_rule_delta")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wp_rule_delta: Option<FieldDelta>,
}

/// Advisory severity tag for a single loss dimension.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum LossSeverityLevel {
    #[serde(rename = "lossless")]
    Lossless,
    #[serde(rename = "lossy-bounded")]
    LossyBounded,
    #[serde(rename = "lossy-unbounded")]
    LossyUnbounded,
    #[serde(rename = "safe-bounded")]
    SafeBounded,
}

/// Per-dimension advisory severity tags.
///
/// A BTreeMap gives alphabetical key order (JCS canonical).
pub type LossSeverity = BTreeMap<String, LossSeverityLevel>;

/// The `option_kind` discriminant for a `ResolutionOption`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResolutionOptionKind {
    #[serde(rename = "accept-permanent")]
    AcceptPermanent,
    #[serde(rename = "add-representation-map")]
    AddRepresentationMap,
    #[serde(rename = "lossy-morphism")]
    LossyMorphism,
    #[serde(rename = "partial-morphism")]
    PartialMorphism,
    #[serde(rename = "re-spec-target-op")]
    ReSpecTargetOp,
    #[serde(rename = "split-target-op")]
    SplitTargetOp,
    #[serde(rename = "statement-level-desugaring")]
    StatementLevelDesugaring,
}

/// The `status` field in a `ResolutionOption`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum OptionStatus {
    #[serde(rename = "chosen")]
    Chosen,
    #[serde(rename = "deferred")]
    Deferred,
    #[serde(rename = "recommended")]
    Recommended,
    #[serde(rename = "rejected")]
    Rejected,
}

/// One entry in `TransportGapMemento.resolution_options`.
///
/// Locked JCS key order (alphabetical):
///   dual_view_cid, loss, loss_severity, option_kind, partial_morphism_cid,
///   precondition, representation_map_delta, respec_target_to, split_targets,
///   status, tradeoff
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolutionOption {
    #[serde(rename = "dual_view_cid")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dual_view_cid: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub loss: Option<LossRecord>,
    #[serde(rename = "loss_severity")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub loss_severity: Option<LossSeverity>,
    #[serde(rename = "option_kind")]
    pub option_kind: ResolutionOptionKind,
    #[serde(rename = "partial_morphism_cid")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub partial_morphism_cid: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub precondition: Option<IrFormula>,
    #[serde(rename = "representation_map_delta")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub representation_map_delta: Option<serde_json::Value>,
    #[serde(rename = "respec_target_to")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub respec_target_to: Option<serde_json::Value>,
    #[serde(rename = "split_targets")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub split_targets: Option<Vec<String>>,
    pub status: OptionStatus,
    pub tradeoff: String,
}

/// A `TransportGapMemento` records why a source-language op has no exact
/// morphism into a concept hub op, plus the resolution options.
///
/// Source of truth:
///   protocol/specs/2026-05-14-transport-gap-and-partial-morphism-protocol.md §1.1
/// CDDL: protocol/provekit-ir.cddl `TransportGapMemento`
///
/// Amendment (this PR): `gap_kind: "no-such-concept-op"` -- source op has no
/// hub op at all; `target_op_cid` is absent in that case.
///
/// Locked JCS key order (alphabetical):
///   fn_name, gap_kind, kind, reason (omitted when absent),
///   reason_note (omitted when absent), resolution_options,
///   schema_version, signature (omitted when absent),
///   source_lang, source_op_cid, target_concept_op,
///   target_op_cid (omitted when absent)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransportGapMemento {
    #[serde(rename = "fn_name")]
    pub fn_name: String,
    #[serde(rename = "gap_kind")]
    pub gap_kind: GapKind,
    pub kind: String, // must be "TransportGapMemento"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<GapReason>,
    #[serde(rename = "reason_note")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason_note: Option<String>,
    #[serde(rename = "resolution_options")]
    pub resolution_options: Vec<ResolutionOption>,
    #[serde(rename = "schema_version")]
    pub schema_version: String, // must be "1"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signature: Option<serde_json::Value>, // tstr | null
    #[serde(rename = "source_lang")]
    pub source_lang: String,
    #[serde(rename = "source_op_cid")]
    pub source_op_cid: String,
    #[serde(rename = "target_concept_op")]
    pub target_concept_op: String,
    #[serde(rename = "target_op_cid")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_op_cid: Option<String>,
}

/// The homomorphism obligation in a `PartialMorphismMemento`.
///
/// Locked JCS key order (alphabetical): kind, source, target
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PartialHomomorphismObligation {
    pub kind: String,   // must be "wp-refinement-under-precondition"
    pub source: String, // CID
    pub target: String, // CID
}

/// A `PartialMorphismMemento` is a morphism valid under a precondition.
///
/// Source of truth:
///   protocol/specs/2026-05-14-transport-gap-and-partial-morphism-protocol.md §1.2
/// CDDL: protocol/provekit-ir.cddl `PartialMorphismMemento`
///
/// Locked JCS key order (alphabetical):
///   fn_name, gap_memento_cid (omitted when absent),
///   homomorphism_obligation, kind, literal_map, operator_map,
///   renaming_map, representation_map, schema_version,
///   signature (omitted when absent), source_contract_cid,
///   target_shape_cid, validity_precondition
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PartialMorphismMemento {
    #[serde(rename = "fn_name")]
    pub fn_name: String,
    #[serde(rename = "gap_memento_cid")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gap_memento_cid: Option<String>,
    #[serde(rename = "homomorphism_obligation")]
    pub homomorphism_obligation: PartialHomomorphismObligation,
    pub kind: String, // must be "PartialMorphismMemento"
    #[serde(rename = "literal_map")]
    pub literal_map: serde_json::Value,
    #[serde(rename = "operator_map")]
    pub operator_map: serde_json::Value,
    #[serde(rename = "renaming_map")]
    pub renaming_map: serde_json::Value,
    #[serde(rename = "representation_map")]
    pub representation_map: serde_json::Value,
    #[serde(rename = "schema_version")]
    pub schema_version: String, // must be "1"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signature: Option<serde_json::Value>, // tstr | null
    #[serde(rename = "source_contract_cid")]
    pub source_contract_cid: String,
    #[serde(rename = "target_shape_cid")]
    pub target_shape_cid: String,
    #[serde(rename = "validity_precondition")]
    pub validity_precondition: IrFormula,
}

/// The homomorphism obligation in a `LossyMorphismMemento`.
///
/// Locked JCS key order (alphabetical): kind, source, target
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LossyHomomorphismObligation {
    pub kind: String,   // must be "wp-refinement-into-coarsening"
    pub source: String, // CID
    pub target: String, // CID
}

/// A `LossyMorphismMemento` is a morphism with characterized loss.
///
/// Source of truth:
///   protocol/specs/2026-05-14-transport-gap-and-partial-morphism-protocol.md §1.4
/// CDDL: protocol/provekit-ir.cddl `LossyMorphismMemento`
///
/// Locked JCS key order (alphabetical):
///   coarsening_kind, fn_name, gap_memento_cid (omitted when absent),
///   homomorphism_obligation, kind, literal_map, loss, loss_severity,
///   operator_map, renaming_map, representation_map, schema_version,
///   signature (omitted when absent), source_contract_cid, target_shape_cid
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LossyMorphismMemento {
    #[serde(rename = "coarsening_kind")]
    pub coarsening_kind: String, // "quotient-target-sort" | "drop-target-precondition" | "widen-target-postcondition" | open tstr
    #[serde(rename = "fn_name")]
    pub fn_name: String,
    #[serde(rename = "gap_memento_cid")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gap_memento_cid: Option<String>,
    #[serde(rename = "homomorphism_obligation")]
    pub homomorphism_obligation: LossyHomomorphismObligation,
    pub kind: String, // must be "LossyMorphismMemento"
    #[serde(rename = "literal_map")]
    pub literal_map: serde_json::Value,
    pub loss: LossRecord,
    #[serde(rename = "loss_severity")]
    pub loss_severity: LossSeverity,
    #[serde(rename = "operator_map")]
    pub operator_map: serde_json::Value,
    #[serde(rename = "renaming_map")]
    pub renaming_map: serde_json::Value,
    #[serde(rename = "representation_map")]
    pub representation_map: serde_json::Value,
    #[serde(rename = "schema_version")]
    pub schema_version: String, // must be "1"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signature: Option<serde_json::Value>, // tstr | null
    #[serde(rename = "source_contract_cid")]
    pub source_contract_cid: String,
    #[serde(rename = "target_shape_cid")]
    pub target_shape_cid: String,
}

// ============================================================
// End manual extension block -- transport gap mementos (issue #66)
// ============================================================

// ============================================================
// MANUAL EXTENSION BLOCK -- concept-site layer (PR-A of multi-PR landing)
// Source of truth:
//   protocol/specs/2026-05-12-concept-site-memento.md §1
//   protocol/specs/2026-05-15-concept-hub-abstraction-layer.md §2.4 (loss-record)
//
// This block adds the ConceptSiteMemento substrate primitive: the
// content-addressed binding between a user code-site and a catalog concept,
// carrying a verdict in the trichotomy {exact, loudly-bounded-lossy, refuse}
// and a per-dimension loss_record characterizing any non-empty loss.
//
// Per JCS canonicalization (2026-04-30-canonicalization-grammar.md), serde
// field order MUST equal the locked alphabetical order from the spec §3.1
// inside each object. Optional fields are omitted from the serialized JSON
// when None.
//
// The CID-determining bytes for a ConceptSiteMemento are JCS(header) with
// `cid` elided; that JCS encoding lives in provekit-claim-envelope
// (provekit-ir-types has no JCS encoder). Byte-pin tests for the CID
// belong in that crate; this crate carries serde round-trip tests only.
// ============================================================

/// The byte span inside a canonical source artifact identifying the code-site.
///
/// Locked JCS key order: end, start.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CodeSiteSpan {
    pub end: u64,
    pub start: u64,
}

/// A user code-site: which function it lives in, which source it lives in,
/// and where inside that source.
///
/// Source of truth: 2026-05-12-concept-site-memento.md §1 `code-site`.
///
/// Locked JCS key order: function_term_cid, source_cid, span.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CodeSite {
    #[serde(rename = "function_term_cid")]
    pub function_term_cid: String,
    #[serde(rename = "source_cid")]
    pub source_cid: String,
    pub span: CodeSiteSpan,
}

/// A pointer to a `WitnessMemento` with a per-site confidence interval.
///
/// `ci_basis_points` is an integer in [0, 10000]; 9500 means 95.00%
/// confidence at this site under the recorded witness policy. Witness
/// propagation per the spec §0.3: tests attached to ONE site become
/// witnesses at the concept level and propagate by reference through
/// `concept_cid` to every binding citing the concept.
///
/// Locked JCS key order: ci_basis_points, witness_cid.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WitnessRef {
    #[serde(rename = "ci_basis_points")]
    pub ci_basis_points: u16,
    #[serde(rename = "witness_cid")]
    pub witness_cid: String,
}

/// The discharge verdict for a binding. Exactly one of three.
///
/// Per the spec §2 trichotomy and §1.2 verdict-consistency table.
/// Silent contract-dropping is NOT in the substrate's vocabulary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Discharge {
    /// The discharge method: "wp" | "witness" | "wp+witness".
    pub method: String,
    /// Required iff verdict == "refuse"; OMITTED otherwise.
    #[serde(rename = "refusal_reason")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub refusal_reason: Option<String>,
    /// "exact" | "loudly-bounded-lossy" | "refuse".
    pub verdict: String,
    /// CID of a MorphismDischargeReceipt. OMITTED iff verdict == "refuse".
    #[serde(rename = "discharge_receipt_cid")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub discharge_receipt_cid: Option<String>,
    /// Per the 2026-05-15 §2.4 five-dimension loss-record. An empty map
    /// is valid (means "no loss in any dimension"; required for `exact`).
    #[serde(rename = "loss_record")]
    pub loss_record: LossRecord,
}

/// The three producer CIDs for a `ConceptSiteMemento`.
///
/// Locked JCS key order: clusterer_cid, discharger_cid, lifter_cid.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConceptSiteProvenance {
    #[serde(rename = "clusterer_cid")]
    pub clusterer_cid: String,
    #[serde(rename = "discharger_cid")]
    pub discharger_cid: String,
    #[serde(rename = "lifter_cid")]
    pub lifter_cid: String,
}

/// The content-addressed binding between a user code-site and a catalog
/// concept, carrying a discharge verdict in the trichotomy.
///
/// Source of truth: protocol/specs/2026-05-12-concept-site-memento.md §1
///
/// Locked JCS key order (header fields, alphabetical):
///   cid, code_site, concept_cid, discharge, kind, local_contract_cid,
///   provenance, realization_mode_hint (omitted when absent),
///   schemaVersion, witnesses.
///
/// This struct represents the `header` layer per
/// 2026-05-03-substrate-layers-envelope-header-body.md; envelope + metadata
/// layers are carried by the wrapping envelope structures defined in
/// provekit-claim-envelope.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConceptSiteMemento {
    /// DERIVED: BLAKE3-512 over JCS(header) with `cid` elided.
    pub cid: String,
    #[serde(rename = "code_site")]
    pub code_site: CodeSite,
    /// The `ConceptAbstractionMemento.cid` this site binds to.
    #[serde(rename = "concept_cid")]
    pub concept_cid: String,
    pub discharge: Discharge,
    /// MUST be "concept-site".
    pub kind: String,
    /// The `FunctionContractMemento.cid` for the user-lifted contract.
    #[serde(rename = "local_contract_cid")]
    pub local_contract_cid: String,
    pub provenance: ConceptSiteProvenance,
    /// Non-normative deployment-policy hint: "witness" | "emitter" | "monitor".
    /// OMITTED when the discharger does not opinion.
    #[serde(rename = "realization_mode_hint")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub realization_mode_hint: Option<String>,
    /// MUST be "1".
    #[serde(rename = "schemaVersion")]
    pub schema_version: String,
    /// Per-site witness samples with confidence intervals. MAY be empty.
    pub witnesses: Vec<WitnessRef>,
}

// ============================================================
// End manual extension block -- concept-site layer (PR-A)
// ============================================================

// ============================================================
// Manual extension: parametric realization mementos (issue #801)
// Source of truth:
//   protocol/specs/2026-05-13-parametric-realization.md §1
//
// This block adds durable substrate shapes for two-stage realization:
// a reusable catalog template and a per-site selection receipt. It does
// not implement realization lookup, sort-slot solving, effect transforms,
// loss evaluation, or language-kit emission.
//
// Per JCS canonicalization, field order MUST equal the locked alphabetical
// order in the spec. JSON-valued fields are intentionally opaque.
// ============================================================

pub type Cid = String;

/// A required sort-morphism slot in a parametric realization.
///
/// Locked JCS key order: slot_name, source_type_variable,
/// target_type_variable.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SlotDescriptor {
    #[serde(rename = "slot_name")]
    pub slot_name: String,
    #[serde(rename = "source_type_variable")]
    pub source_type_variable: String,
    #[serde(rename = "target_type_variable")]
    pub target_type_variable: String,
}

/// A required effect transform slot in a parametric realization.
///
/// Locked JCS key order: concept_effect, slot_name, target_effect.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EffectSlotDescriptor {
    #[serde(rename = "concept_effect")]
    pub concept_effect: String,
    #[serde(rename = "slot_name")]
    pub slot_name: String,
    #[serde(rename = "target_effect")]
    pub target_effect: String,
}

/// A reusable catalog template for realizing a concept pattern into a
/// target-language pattern.
///
/// Source of truth: protocol/specs/2026-05-13-parametric-realization.md §1.1
///
/// Locked JCS key order:
///   body_template_cids, concept_pattern, effect_transform_slots,
///   loss_record_template, provenance_cid, required_sort_morphism_slots,
///   sugar_cids, target_pattern, type_variables.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ParametricRealizationMemento {
    #[serde(rename = "body_template_cids")]
    pub body_template_cids: Vec<Cid>,
    #[serde(rename = "concept_pattern")]
    pub concept_pattern: serde_json::Value,
    #[serde(rename = "effect_transform_slots")]
    pub effect_transform_slots: Vec<EffectSlotDescriptor>,
    #[serde(rename = "loss_record_template")]
    pub loss_record_template: serde_json::Value,
    #[serde(rename = "provenance_cid")]
    pub provenance_cid: Cid,
    #[serde(rename = "required_sort_morphism_slots")]
    pub required_sort_morphism_slots: Vec<SlotDescriptor>,
    #[serde(rename = "sugar_cids")]
    pub sugar_cids: Vec<Cid>,
    #[serde(rename = "target_pattern")]
    pub target_pattern: serde_json::Value,
    #[serde(rename = "type_variables")]
    pub type_variables: Vec<String>,
}

/// A per-site selection receipt that instantiates a
/// `ParametricRealizationMemento`.
///
/// Source of truth: protocol/specs/2026-05-13-parametric-realization.md §1.2
///
/// Locked JCS key order:
///   candidate_set_cid, concept_site_cid, effect_occurrence_transform,
///   loss_function_cid, observation_wrapper_cid, provenance_cid,
///   selected_candidate_cid, selected_realization_cid, sort_morphism_cids,
///   total_loss_record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RealizationPlanMemento {
    #[serde(rename = "candidate_set_cid")]
    pub candidate_set_cid: Cid,
    #[serde(rename = "concept_site_cid")]
    pub concept_site_cid: Cid,
    #[serde(rename = "effect_occurrence_transform")]
    pub effect_occurrence_transform: serde_json::Value,
    #[serde(rename = "loss_function_cid")]
    pub loss_function_cid: Cid,
    #[serde(rename = "observation_wrapper_cid")]
    pub observation_wrapper_cid: Option<Cid>,
    #[serde(rename = "provenance_cid")]
    pub provenance_cid: Cid,
    #[serde(rename = "selected_candidate_cid")]
    pub selected_candidate_cid: Cid,
    #[serde(rename = "selected_realization_cid")]
    pub selected_realization_cid: Cid,
    #[serde(rename = "sort_morphism_cids")]
    pub sort_morphism_cids: Vec<Cid>,
    #[serde(rename = "total_loss_record")]
    pub total_loss_record: serde_json::Value,
}

/// Returned when a `ParametricRealizationMemento` violates one of its
/// load-time invariants per `2026-05-13-parametric-realization.md`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParametricRealizationError {
    /// Spec §1.1 CDDL: `type_variables: [+ tstr]` (non-empty).
    EmptyTypeVariables,
    /// Spec §1.1 CDDL: `required_sort_morphism_slots: [+ slot-descriptor]`
    /// (non-empty).
    EmptyRequiredSortMorphismSlots,
    /// A slot descriptor references a type variable not in
    /// `type_variables`. This would be unresolvable at instantiation
    /// time.
    SlotReferencesUnknownTypeVariable {
        slot_name: String,
        variable: String,
        side: &'static str,
    },
}

impl std::fmt::Display for ParametricRealizationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyTypeVariables => f.write_str(
                "ParametricRealizationMemento: type_variables is empty (spec §1.1 CDDL requires [+ tstr])",
            ),
            Self::EmptyRequiredSortMorphismSlots => f.write_str(
                "ParametricRealizationMemento: required_sort_morphism_slots is empty (spec §1.1 CDDL requires [+ slot-descriptor])",
            ),
            Self::SlotReferencesUnknownTypeVariable {
                slot_name,
                variable,
                side,
            } => write!(
                f,
                "ParametricRealizationMemento: slot {slot_name:?} {side}_type_variable {variable:?} is not in type_variables",
            ),
        }
    }
}

impl std::error::Error for ParametricRealizationError {}

impl ParametricRealizationMemento {
    /// Check load-time invariants per spec §1.1.
    ///
    /// Enforced:
    /// 1. `type_variables` non-empty (CDDL `[+ tstr]`)
    /// 2. `required_sort_morphism_slots` non-empty (CDDL `[+ slot-descriptor]`)
    /// 3. every slot's `source_type_variable` and `target_type_variable`
    ///    is a member of `type_variables`
    pub fn validate(&self) -> Result<(), ParametricRealizationError> {
        if self.type_variables.is_empty() {
            return Err(ParametricRealizationError::EmptyTypeVariables);
        }
        if self.required_sort_morphism_slots.is_empty() {
            return Err(ParametricRealizationError::EmptyRequiredSortMorphismSlots);
        }
        let known: std::collections::HashSet<&str> =
            self.type_variables.iter().map(String::as_str).collect();
        for slot in &self.required_sort_morphism_slots {
            if !known.contains(slot.source_type_variable.as_str()) {
                return Err(ParametricRealizationError::SlotReferencesUnknownTypeVariable {
                    slot_name: slot.slot_name.clone(),
                    variable: slot.source_type_variable.clone(),
                    side: "source",
                });
            }
            if !known.contains(slot.target_type_variable.as_str()) {
                return Err(ParametricRealizationError::SlotReferencesUnknownTypeVariable {
                    slot_name: slot.slot_name.clone(),
                    variable: slot.target_type_variable.clone(),
                    side: "target",
                });
            }
        }
        Ok(())
    }
}

/// Returned when a `RealizationPlanMemento` does not match its cited
/// `ParametricRealizationMemento`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RealizationPlanError {
    /// The plan's `sort_morphism_cids` length must equal the
    /// realization's `required_sort_morphism_slots` length: one CID
    /// per slot.
    SortMorphismCountMismatch { expected: usize, actual: usize },
    /// The realization itself was malformed; this plan cannot be
    /// validated against it.
    RealizationInvalid(ParametricRealizationError),
}

impl std::fmt::Display for RealizationPlanError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SortMorphismCountMismatch { expected, actual } => write!(
                f,
                "RealizationPlanMemento: sort_morphism_cids.len() = {actual}, expected {expected} (one per realization required_sort_morphism_slots entry)",
            ),
            Self::RealizationInvalid(inner) => write!(f, "RealizationPlanMemento: cited realization is invalid: {inner}"),
        }
    }
}

impl std::error::Error for RealizationPlanError {}

impl RealizationPlanMemento {
    /// Check the plan-vs-realization slot-count invariant per spec §1.2.
    ///
    /// The plan's `sort_morphism_cids` is the ordered list of CIDs
    /// filling the realization's `required_sort_morphism_slots`; the
    /// lengths MUST match.
    pub fn validate_against(
        &self,
        realization: &ParametricRealizationMemento,
    ) -> Result<(), RealizationPlanError> {
        realization
            .validate()
            .map_err(RealizationPlanError::RealizationInvalid)?;
        let expected = realization.required_sort_morphism_slots.len();
        let actual = self.sort_morphism_cids.len();
        if expected != actual {
            return Err(RealizationPlanError::SortMorphismCountMismatch { expected, actual });
        }
        Ok(())
    }
}

// ============================================================
// End manual extension block -- parametric realization mementos
// ============================================================

// ============================================================
// Manual extension: compound-contract layer (PR-A of compound spec)
// Source of truth:
//   protocol/specs/2026-05-13-compound-contract-memento.md §1, §2, §3
//   AMENDS protocol/specs/2026-05-12-concept-site-memento.md §1.1 and §5.4
//
// This block adds the EvidenceMemento and CompoundContractMemento
// substrate primitives. The compound is the new convergence point for
// every contract-source the substrate can lift from a user's codebase:
// annotations, test assertions, type signatures, docstrings, loop
// invariants, implicit-effect call-sites, native contract surfaces
// (JML / Zod / Spring / pydantic / OpenAPI), structurally synthesized
// wp_rules, empirical witnesses, and (future) review comments.
//
// Trichotomy is enforced at TWO levels:
//   * per-evidence: each evidence's discharge verdict against the
//     concept's wp_rule (recorded in the discharge receipt, PR-F).
//   * compound: derived from per-evidence verdicts under the recorded
//     aggregation_strategy (§2 of the spec).
//
// v0 wires only `AggregationStrategy::Conjunction`; the other two
// strategies are spec'd but `unimplemented!` here. The Rust enum carries
// all three variants so downstream callers can name them; only the
// conjunction discharge path is functional in v0.
//
// CID-determining bytes for both mementos are JCS(header) with `cid`
// elided; that JCS encoder lives in provekit-claim-envelope. Byte-pin
// tests for the compound CID belong there; THIS crate carries serde
// round-trip tests only.
// ============================================================

/// One point inside a source span: 1-based line, 0-indexed col (UTF-8 bytes; spec §1.1).
///
/// Locked JCS key order: col, line.
///
/// Note: line/col (not byte offsets) -- evidence often comes from
/// docstrings, test sources, or native contract surfaces where byte
/// offsets are unstable across re-formatting. This diverges from
/// `CodeSiteSpan` (which uses byte offsets, anchored at the function
/// boundary where formatters do not move text); see the compound spec
/// §1.1 for rationale.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceLocatorPoint {
    pub col: u32,
    pub line: u32,
}

/// A span inside a source artifact, as a (start, end) pair of
/// `SourceLocatorPoint`s.
///
/// Locked JCS key order: end, start.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceLocatorSpan {
    pub end: SourceLocatorPoint,
    pub start: SourceLocatorPoint,
}

/// Provenance for one piece of evidence: which source artifact it was
/// extracted from, and where inside that artifact.
///
/// Locked JCS key order: source_cid, span.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceLocator {
    #[serde(rename = "source_cid")]
    pub source_cid: String,
    pub span: SourceLocatorSpan,
}

/// The kind of contract-evidence source. Open enum; unknown labels
/// MUST be accepted at shape level (per spec §5.1) and are carried as
/// `Other(String)`.
///
/// Wire format: a bare JSON string (e.g., `"test-assertion"`).
///
/// Mirrors the `DivergentSemanticsTag` pattern: `#[serde(from = "String",
/// into = "String")]` with explicit kebab-case mapping. (Using
/// `#[serde(untagged)]` with all-unit variants silently ignores
/// `#[serde(rename)]` and serializes every unit variant as `null`, so we
/// must go through `String`.)
///
/// The ten canonical labels are documented in spec §10. `Other(String)`
/// is the open-extension placeholder; downstream consumers decide how
/// to treat unknown kinds.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "String", into = "String")]
pub enum SourceKind {
    Annotation,
    TestAssertion,
    TypeSignature,
    Docstring,
    LoopInvariant,
    ImplicitEffect,
    NativeSurface,
    StructuralSynthesis,
    EmpiricalWitness,
    ReviewComment,
    Other(String),
}

impl From<String> for SourceKind {
    fn from(s: String) -> Self {
        match s.as_str() {
            "annotation" => SourceKind::Annotation,
            "test-assertion" => SourceKind::TestAssertion,
            "type-signature" => SourceKind::TypeSignature,
            "docstring" => SourceKind::Docstring,
            "loop-invariant" => SourceKind::LoopInvariant,
            "implicit-effect" => SourceKind::ImplicitEffect,
            "native-surface" => SourceKind::NativeSurface,
            "structural-synthesis" => SourceKind::StructuralSynthesis,
            "empirical-witness" => SourceKind::EmpiricalWitness,
            "review-comment" => SourceKind::ReviewComment,
            _ => SourceKind::Other(s),
        }
    }
}

impl From<SourceKind> for String {
    fn from(k: SourceKind) -> String {
        match k {
            SourceKind::Annotation => "annotation".to_string(),
            SourceKind::TestAssertion => "test-assertion".to_string(),
            SourceKind::TypeSignature => "type-signature".to_string(),
            SourceKind::Docstring => "docstring".to_string(),
            SourceKind::LoopInvariant => "loop-invariant".to_string(),
            SourceKind::ImplicitEffect => "implicit-effect".to_string(),
            SourceKind::NativeSurface => "native-surface".to_string(),
            SourceKind::StructuralSynthesis => "structural-synthesis".to_string(),
            SourceKind::EmpiricalWitness => "empirical-witness".to_string(),
            SourceKind::ReviewComment => "review-comment".to_string(),
            SourceKind::Other(s) => s,
        }
    }
}

/// How per-evidence verdicts compose into the compound's verdict.
///
/// v0 NORMATIVE: only `Conjunction` is wired in the discharger
/// (PR-F). `BestConfidence` and `LoudlyBoundedDisjunction` are spec'd
/// (compound spec §2.2 and §2.3) and round-trip serde here, but a
/// discharger that encounters them MUST `unimplemented!` until PR-F+1.
///
/// Wire format: a bare JSON string. Same `from/into String` pattern as
/// `SourceKind`. `Other(String)` is the open-extension placeholder.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "String", into = "String")]
pub enum AggregationStrategy {
    Conjunction,
    BestConfidence,
    LoudlyBoundedDisjunction,
    Other(String),
}

impl From<String> for AggregationStrategy {
    fn from(s: String) -> Self {
        match s.as_str() {
            "conjunction" => AggregationStrategy::Conjunction,
            "best-confidence" => AggregationStrategy::BestConfidence,
            "loudly-bounded-disjunction" => AggregationStrategy::LoudlyBoundedDisjunction,
            _ => AggregationStrategy::Other(s),
        }
    }
}

impl From<AggregationStrategy> for String {
    fn from(s: AggregationStrategy) -> String {
        match s {
            AggregationStrategy::Conjunction => "conjunction".to_string(),
            AggregationStrategy::BestConfidence => "best-confidence".to_string(),
            AggregationStrategy::LoudlyBoundedDisjunction => {
                "loudly-bounded-disjunction".to_string()
            }
            AggregationStrategy::Other(s) => s,
        }
    }
}

/// One piece of contract evidence from one source. Content-addressed.
///
/// Source of truth: protocol/specs/2026-05-13-compound-contract-memento.md §1.1
///
/// This is the `header` layer per the substrate envelope/header/metadata
/// layering (`2026-05-03-substrate-layers-envelope-header-body.md`); the
/// envelope (`declaredAt`, `signature`, `signer`) and metadata (`note`)
/// are carried by the wrapping envelope structures in
/// `provekit-claim-envelope`.
///
/// Locked JCS key order (alphabetical):
///   cid, confidence_basis_points, extension_fields, kind, lifter_cid,
///   predicate, schemaVersion, source_kind, source_locator.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvidenceMemento {
    /// DERIVED: BLAKE3-512 over JCS(header) with `cid` elided.
    pub cid: String,
    /// Lifter's prior on this evidence (0..10000, basis points).
    /// 10000 for static-derived (annotations, type signatures); lower
    /// for grammar-extracted or sampled (docstrings, empirical witnesses).
    #[serde(rename = "confidence_basis_points")]
    pub confidence_basis_points: u16,
    /// Per-kind structured metadata. Keys and values participate in
    /// the CID (open-extension under deterministic addressing).
    #[serde(rename = "extension_fields")]
    pub extension_fields: BTreeMap<String, serde_json::Value>,
    /// MUST be "evidence".
    pub kind: String,
    /// CID of the lifter binary or rule-set that emitted this evidence.
    /// Reserved sentinel `blake3-512:` ++ 128 hex `0`s for the
    /// backward-compat auto-promotion path (compound spec §4.4).
    /// All-zeros is provably not a real BLAKE3-512 output (P ≈ 2^-512).
    /// Pass-1 CID validation accepts it without a special-case exception.
    #[serde(rename = "lifter_cid")]
    pub lifter_cid: String,
    /// The asserted predicate (an `IrFormula`).
    pub predicate: IrFormula,
    /// MUST be "1".
    #[serde(rename = "schemaVersion")]
    pub schema_version: String,
    /// The kind of source this evidence was extracted from.
    #[serde(rename = "source_kind")]
    pub source_kind: SourceKind,
    /// Where this evidence was extracted from.
    #[serde(rename = "source_locator")]
    pub source_locator: SourceLocator,
}

/// A reference to an `EvidenceMemento` with a per-compound weight.
///
/// Under `Conjunction` (v0) the weight is informational. Under the
/// spec'd strategies (`BestConfidence`, `LoudlyBoundedDisjunction`) it
/// is consulted during verdict derivation. The weight participates in
/// the compound's CID either way (different weights = different
/// compound bytes = different CID).
///
/// Locked JCS key order: evidence_cid, weight_basis_points.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvidenceRef {
    #[serde(rename = "evidence_cid")]
    pub evidence_cid: String,
    #[serde(rename = "weight_basis_points")]
    pub weight_basis_points: u16,
}

/// A content-addressed aggregation of `EvidenceMemento`s for one
/// function. The convergence point for every contract source the
/// substrate can lift.
///
/// Source of truth: protocol/specs/2026-05-13-compound-contract-memento.md §1.2
///
/// CID-determining: `aggregation_strategy`, `composed_post`,
/// `composed_pre`, `evidences` (sorted by evidence_cid at JCS time),
/// `function_term_cid`, `kind`, `schemaVersion`. Changing one evidence's
/// bytes rolls its CID, which rolls this compound's CID, which rolls
/// every binding citing this compound.
///
/// `composed_pre` and `composed_post` are DERIVED-AND-STORED (cached
/// truth-source duality). Validators MUST recompute under the recorded
/// strategy and reject on mismatch; that recompute requires a JCS
/// encoder and lives in provekit-claim-envelope.
///
/// Locked JCS key order (alphabetical):
///   aggregation_strategy, cid, composed_post, composed_pre, evidences,
///   function_term_cid, kind, schemaVersion.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompoundContractMemento {
    /// How per-evidence verdicts compose into the compound's verdict.
    /// v0: only `Conjunction` is wired.
    #[serde(rename = "aggregation_strategy")]
    pub aggregation_strategy: AggregationStrategy,
    /// DERIVED: BLAKE3-512 over JCS(header) with `cid` elided.
    pub cid: String,
    /// DERIVED-AND-STORED: the aggregated post-condition.
    /// Validators MUST recompute and reject on mismatch.
    #[serde(rename = "composed_post")]
    pub composed_post: IrFormula,
    /// DERIVED-AND-STORED: the aggregated pre-condition.
    /// Validators MUST recompute and reject on mismatch.
    #[serde(rename = "composed_pre")]
    pub composed_pre: IrFormula,
    /// References to the constituent `EvidenceMemento`s.
    /// MUST be sorted by `evidence_cid` ascending in the JCS bytes
    /// (Rust constructors MAY preserve insertion order; the JCS encoder
    /// sorts at canonicalization time).
    /// MAY be empty (degenerate compound; composed_pre/post = `true`/`true`).
    pub evidences: Vec<EvidenceRef>,
    /// The `FunctionContractMemento.cid` of the function this compound
    /// is the contract for.
    #[serde(rename = "function_term_cid")]
    pub function_term_cid: String,
    /// MUST be "compound-contract".
    pub kind: String,
    /// MUST be "1".
    #[serde(rename = "schemaVersion")]
    pub schema_version: String,
}

// ============================================================
// End manual extension block -- compound-contract layer (PR-A)
// ============================================================

// ============================================================
// Manual extension: observation-wrapper memento (#804)
// Source of truth:
//   protocol/specs/2026-05-13-effect-occurrence-memento.md §1
//   protocol/specs/2026-05-13-observation-wrapper-memento.md §1, §7
//
// This is substrate shape only. It records wrapper/object separation and
// exposes fail-closed validation of the frame invariants. Runtime wrapper
// emission and target syntax generation are intentionally out of scope.
//
// Locked JCS key order:
//   EffectOccurrence:
//     args, discharge_key, locator, occurrence_kind, role, signature_cid
//   ObservationWrapperMemento:
//     emitted_artifact_cid, mode, object_fcm_cid, observer_effects,
//     preservation_claim_cid, provenance_cid, wrapper_fcm_cid
// ============================================================

/// A promoted semantic effect payload carried by `FunctionContractMemento.effects`.
///
/// This is the minimal substrate shape from
/// `2026-05-13-effect-occurrence-memento.md` §1.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EffectOccurrence {
    pub args: serde_json::Value,
    #[serde(rename = "discharge_key")]
    pub discharge_key: String,
    pub locator: serde_json::Value,
    #[serde(rename = "occurrence_kind")]
    pub occurrence_kind: String,
    pub role: String,
    #[serde(rename = "signature_cid")]
    pub signature_cid: String,
}

// NOTE: an earlier draft of this module declared a public
// `FunctionContractMemento { effects }` stub. That name collides with the
// real FCM surface (pre/post/formals/body) defined elsewhere; if a caller
// deserialized a full FCM into the stub, serde would silently drop the
// other fields and a subsequent reserialize would lose them. To avoid the
// footgun, the wrapper-validation API now takes effect slices directly.
// The caller is responsible for extracting effects from whatever
// FCM-shaped object they hold.

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InvariantViolation {
    UnknownMode { mode: String },
    UnimplementedExtensionMode { mode: String },
    MissingPreservationClaim,
    EmptyObserverEffects,
    ObserverEffectOnObject { effect: EffectOccurrence },
    ObserverEffectMissingFromWrapper { effect: EffectOccurrence },
}

/// Durable substrate object recording a wrapper relationship between an
/// unchanged object function and a wrapper function that carries observer
/// effects.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObservationWrapperMemento {
    #[serde(rename = "emitted_artifact_cid")]
    pub emitted_artifact_cid: String,
    pub mode: String,
    #[serde(rename = "object_fcm_cid")]
    pub object_fcm_cid: String,
    #[serde(rename = "observer_effects")]
    pub observer_effects: Vec<EffectOccurrence>,
    #[serde(rename = "preservation_claim_cid")]
    pub preservation_claim_cid: String,
    #[serde(rename = "provenance_cid")]
    pub provenance_cid: String,
    #[serde(rename = "wrapper_fcm_cid")]
    pub wrapper_fcm_cid: String,
}

impl ObservationWrapperMemento {
    /// Check the master-frame invariants for this wrapper against caller-supplied
    /// effect surfaces. The caller passes the object-FCM and wrapper-FCM effect
    /// arrays directly; this API intentionally avoids accepting an `FCM` struct
    /// in order to remove the footgun of silently dropping non-`effects` fields.
    ///
    /// `allowed_extension_modes` is the caller's allowlist of namespaced extension
    /// modes (e.g. `["acme:probe"]`). Per spec §7, unimplemented extension modes
    /// MUST fail closed; passing `&[]` accepts only the core monitor/witness/dispatcher
    /// modes. A namespaced mode that is well-formed but absent from the allowlist
    /// returns `UnimplementedExtensionMode`.
    pub fn validate(
        &self,
        object_effects: &[EffectOccurrence],
        wrapper_effects: &[EffectOccurrence],
        allowed_extension_modes: &[&str],
    ) -> Result<(), InvariantViolation> {
        match classify_mode(&self.mode, allowed_extension_modes) {
            ModeClassification::Core | ModeClassification::AllowedExtension => {}
            ModeClassification::UnknownExtension => {
                return Err(InvariantViolation::UnimplementedExtensionMode {
                    mode: self.mode.clone(),
                });
            }
            ModeClassification::Unknown => {
                return Err(InvariantViolation::UnknownMode {
                    mode: self.mode.clone(),
                });
            }
        }

        if self.preservation_claim_cid.is_empty() {
            return Err(InvariantViolation::MissingPreservationClaim);
        }

        // Spec CDDL: observer_effects = [+ effect-occurrence] (non-empty).
        // The vacuous-truth case where the dual-invariant loops below pass on an
        // empty list must be rejected explicitly.
        if self.observer_effects.is_empty() {
            return Err(InvariantViolation::EmptyObserverEffects);
        }

        for effect in &self.observer_effects {
            if object_effects.iter().any(|object| object == effect) {
                return Err(InvariantViolation::ObserverEffectOnObject {
                    effect: effect.clone(),
                });
            }
        }

        for effect in &self.observer_effects {
            if !wrapper_effects.iter().any(|wrapper| wrapper == effect) {
                return Err(InvariantViolation::ObserverEffectMissingFromWrapper {
                    effect: effect.clone(),
                });
            }
        }

        Ok(())
    }
}

// ============================================================
// Manual extension: promotion-decision memento (issue #791)
// Source of truth:
//   protocol/specs/2026-05-13-promotion-decision-memento.md §1 and §4
//
// This block adds the substrate-only PromotionDecisionMemento. It carries
// CIDs and structured decision payloads, but does not interpret source
// language syntax or perform evidence extraction.
//
// `header.cid` is DERIVED from JCS(header without cid) and BLAKE3-512.
// `evidence_cids` are sorted for CID derivation as required by §4.
// ============================================================

/// Envelope layer for a `PromotionDecisionMemento`.
///
/// Locked JCS key order: declaredAt, signature, signer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PromotionDecisionEnvelope {
    #[serde(rename = "declaredAt")]
    pub declared_at: String,
    pub signature: String,
    pub signer: String,
}

/// Promotion gate label. Canonical labels are known, and namespaced
/// extensions are carried as strings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "String", into = "String")]
pub enum PromotionGate {
    Human,
    Proof,
    Property,
    Threshold,
    Other(String),
}

impl From<String> for PromotionGate {
    fn from(s: String) -> Self {
        match s.as_str() {
            "human" => PromotionGate::Human,
            "proof" => PromotionGate::Proof,
            "property" => PromotionGate::Property,
            "threshold" => PromotionGate::Threshold,
            _ => PromotionGate::Other(s),
        }
    }
}

impl From<PromotionGate> for String {
    fn from(gate: PromotionGate) -> String {
        match gate {
            PromotionGate::Human => "human".to_string(),
            PromotionGate::Proof => "proof".to_string(),
            PromotionGate::Property => "property".to_string(),
            PromotionGate::Threshold => "threshold".to_string(),
            PromotionGate::Other(s) => s,
        }
    }
}

/// Promotion result. The v1 CDDL closes this set.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PromotionResult {
    #[serde(rename = "admitted")]
    Admitted,
    #[serde(rename = "rejected")]
    Rejected,
    #[serde(rename = "deferred")]
    Deferred,
}

/// Header layer for a `PromotionDecisionMemento`.
///
/// Locked JCS key order:
///   candidate_cid, cid, decider_cid, decision_payload, evidence_cids,
///   gate, kind, policy_cid, promoted_cid, result, schemaVersion.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PromotionDecisionHeader {
    #[serde(rename = "candidate_cid")]
    pub candidate_cid: String,
    /// DERIVED: BLAKE3-512 over JCS(header) with `cid` elided.
    pub cid: String,
    #[serde(rename = "decider_cid")]
    pub decider_cid: String,
    #[serde(rename = "decision_payload")]
    pub decision_payload: serde_json::Value,
    #[serde(rename = "evidence_cids")]
    pub evidence_cids: Vec<String>,
    pub gate: PromotionGate,
    /// MUST be "promotion-decision".
    pub kind: String,
    #[serde(rename = "policy_cid")]
    pub policy_cid: String,
    #[serde(rename = "promoted_cid")]
    pub promoted_cid: String,
    pub result: PromotionResult,
    /// MUST be "1".
    #[serde(rename = "schemaVersion")]
    pub schema_version: String,
}

/// Metadata layer for a `PromotionDecisionMemento`.
///
/// Locked JCS key order: counterexample_cids, note, source_url.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct PromotionDecisionMetadata {
    #[serde(rename = "counterexample_cids")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub counterexample_cids: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    #[serde(rename = "source_url")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_url: Option<String>,
}

/// A content-addressed promotion decision connecting a candidate,
/// evidence set, policy, decider, and promoted artifact.
///
/// Locked JCS key order: envelope, header, metadata.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PromotionDecisionMemento {
    pub envelope: PromotionDecisionEnvelope,
    pub header: PromotionDecisionHeader,
    pub metadata: PromotionDecisionMetadata,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PromotionDecisionCanonicalizationError {
    message: String,
}

impl PromotionDecisionCanonicalizationError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl std::fmt::Display for PromotionDecisionCanonicalizationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for PromotionDecisionCanonicalizationError {}

impl From<serde_json::Error> for PromotionDecisionCanonicalizationError {
    fn from(err: serde_json::Error) -> Self {
        Self::new(err.to_string())
    }
}

impl PromotionDecisionMemento {
    /// Serialize the whole memento through the repo JCS encoder.
    pub fn to_jcs_string(&self) -> Result<String, PromotionDecisionCanonicalizationError> {
        let json = serde_json::to_value(self)?;
        let canonical = serde_json_to_canonical_value(&json)?;
        Ok(provekit_canonicalizer::encode_jcs(&canonical))
    }

    /// Recompute `header.cid` per §4.
    pub fn recompute_header_cid(&self) -> Result<String, PromotionDecisionCanonicalizationError> {
        let mut header = serde_json::to_value(&self.header)?;
        let serde_json::Value::Object(ref mut object) = header else {
            return Err(PromotionDecisionCanonicalizationError::new(
                "promotion header did not serialize as an object",
            ));
        };

        object.remove("cid");
        if let Some(serde_json::Value::Array(evidence_cids)) = object.get_mut("evidence_cids") {
            evidence_cids.sort_by(|left, right| left.as_str().cmp(&right.as_str()));
        }

        let canonical = serde_json_to_canonical_value(&header)?;
        let jcs = provekit_canonicalizer::encode_jcs(&canonical);
        Ok(provekit_canonicalizer::blake3_512_of(jcs.as_bytes()))
    }

    /// Check load-time invariants per spec §1 CDDL.
    ///
    /// `evidence_cids` is declared as `[+ cid]` (non-empty) in the spec.
    /// An evidence-free promotion would contradict the master-frame
    /// admissibility-spine framing (#796): every irreversible substrate
    /// claim needs a memento that says what was observed. Empty evidence
    /// is no evidence; admission MUST fail closed.
    pub fn validate(&self) -> Result<(), PromotionDecisionInvariantError> {
        if self.header.evidence_cids.is_empty() {
            return Err(PromotionDecisionInvariantError::EmptyEvidenceCids);
        }
        Ok(())
    }
}

enum ModeClassification {
    /// Core wrapper-mode from the spec: monitor / witness / dispatcher.
    Core,
    /// Namespaced extension that the caller explicitly admitted via the allowlist.
    AllowedExtension,
    /// Well-formed `<namespace>:<kind>` shape but absent from the allowlist.
    UnknownExtension,
    /// Neither a core mode nor a well-formed namespaced extension.
    Unknown,
}

fn classify_mode(mode: &str, allowed_extension_modes: &[&str]) -> ModeClassification {
    if matches!(mode, "monitor" | "witness" | "dispatcher") {
        return ModeClassification::Core;
    }
    // Spec: extension modes are `<namespace>:<kind>` with EXACTLY one
    // colon, both segments non-empty. `a:b:c` is not a valid namespaced
    // extension.
    match mode.split_once(':') {
        Some((namespace, extension))
            if !namespace.is_empty() && !extension.is_empty() && !extension.contains(':') =>
        {
            if allowed_extension_modes.iter().any(|m| *m == mode) {
                ModeClassification::AllowedExtension
            } else {
                ModeClassification::UnknownExtension
            }
        }
        _ => ModeClassification::Unknown,
    }
}

/// Returned when a `PromotionDecisionMemento` violates a load-time
/// invariant per `2026-05-13-promotion-decision-memento.md` §1.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PromotionDecisionInvariantError {
    /// Spec §1 CDDL: `evidence_cids: [+ cid]` (non-empty). The master
    /// frame requires every admission to cite what was observed.
    EmptyEvidenceCids,
}

impl std::fmt::Display for PromotionDecisionInvariantError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyEvidenceCids => f.write_str(
                "PromotionDecisionMemento: evidence_cids is empty (spec §1 CDDL requires [+ cid]; admissibility-spine #796 forbids evidence-free admission)",
            ),
        }
    }
}

impl std::error::Error for PromotionDecisionInvariantError {}

fn serde_json_to_canonical_value(
    value: &serde_json::Value,
) -> Result<std::sync::Arc<provekit_canonicalizer::Value>, PromotionDecisionCanonicalizationError> {
    use provekit_canonicalizer::Value as CanonicalValue;

    match value {
        serde_json::Value::Null => Ok(CanonicalValue::null()),
        serde_json::Value::Bool(b) => Ok(CanonicalValue::boolean(*b)),
        serde_json::Value::Number(n) => {
            let Some(integer) = n.as_i64() else {
                return Err(PromotionDecisionCanonicalizationError::new(format!(
                    "unsupported non-i64 JSON number in promotion decision: {n}"
                )));
            };
            Ok(CanonicalValue::integer(integer))
        }
        serde_json::Value::String(s) => Ok(CanonicalValue::string(s.clone())),
        serde_json::Value::Array(items) => {
            let converted = items
                .iter()
                .map(serde_json_to_canonical_value)
                .collect::<Result<Vec<_>, _>>()?;
            Ok(CanonicalValue::array(converted))
        }
        serde_json::Value::Object(object) => {
            let converted = object
                .iter()
                .map(|(key, value)| Ok((key.clone(), serde_json_to_canonical_value(value)?)))
                .collect::<Result<Vec<_>, PromotionDecisionCanonicalizationError>>()?;
            Ok(CanonicalValue::object(converted))
        }
    }
}

// ============================================================
// End manual extension block -- observation-wrapper memento (#804)
// ============================================================

// ============================================================
// End manual extension block -- promotion-decision memento
// ============================================================

// ============================================================
// MANUAL EXTENSION BLOCK -- DomainClaim normalization (PR-A)
// Source of truth:
//   protocol/specs/2026-05-13-domain-claim-normalization.md §1
//
// The canonical wire-form `k(I) = t` surface for the substrate's
// verifier. Every memento type that can be verified projects onto a
// `DomainClaim` via an `Into<DomainClaim>` impl. The verifier consumes
// only `DomainClaim`s; per-memento-type dispatch moves out of the
// verifier into thin From-impls.
//
// NOTE ON NAMING COLLISION: a `DomainClaim` type also exists in
// `libprovekit::core::types`. That type is the IN-MEMORY AGGREGATE used
// by libprovekit primitives (`compose`, `address`, `discharge`).
// The type defined here is the WIRE FORM. The two coexist; see spec §0.1
// and the PR roadmap (§6, PR-D considers renaming the libprovekit one).
//
// Per JCS canonicalization (2026-04-30-canonicalization-grammar.md),
// serde field order MUST equal alphabetical order. Optional fields are
// omitted from the serialized JSON when None.
//
// The `signature` field is REQUIRED in the wire form but is REPLACED by
// the empty string when computing the CID-determining bytes (signer-
// independent addressing). The CID computation itself lives in
// `provekit-claim-envelope` (this crate has no JCS encoder).
// ============================================================

/// The trichotomy verdict kind for a `DomainClaim`. Exactly one of three.
///
/// Source of truth: spec §1.
///
/// - `Exact`: `k(I) = t` with `loss_record` empty in every dimension.
/// - `LoudlyBoundedLossy`: `k(I) = t` modulo the loss bounded by `loss_record`.
///   `loss_record` is non-empty in at least one dimension.
/// - `Refuse`: `k(I)` cannot be shown to equal `t` under any tractable
///   loss-record. `refusal_reason` is required.
///
/// Serializes as the kebab-case wire strings `"exact"`,
/// `"loudly-bounded-lossy"`, `"refuse"`, matching
/// `ConceptSiteMemento.discharge.verdict` exactly.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum VerdictKind {
    #[serde(rename = "exact")]
    Exact,
    #[serde(rename = "loudly-bounded-lossy")]
    LoudlyBoundedLossy,
    #[serde(rename = "refuse")]
    Refuse,
}

/// The verdict body of a `DomainClaim`.
///
/// Locked JCS key order: `discharge_receipt_cid` (omitted when absent),
/// `kind`, `loss_record`, `refusal_reason` (omitted when absent).
///
/// Consistency invariants (spec §1.2) -- enforced by validators, not by
/// serde:
///   * `kind == Exact`               => `loss_record` empty,
///                                       `discharge_receipt_cid` present,
///                                       `refusal_reason` absent.
///   * `kind == LoudlyBoundedLossy`  => `loss_record` non-empty,
///                                       `discharge_receipt_cid` present,
///                                       `refusal_reason` absent.
///   * `kind == Refuse`              => `discharge_receipt_cid` absent,
///                                       `refusal_reason` present.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VerdictBody {
    #[serde(rename = "discharge_receipt_cid")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub discharge_receipt_cid: Option<String>,
    pub kind: VerdictKind,
    #[serde(rename = "loss_record")]
    pub loss_record: LossRecord,
    #[serde(rename = "refusal_reason")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub refusal_reason: Option<String>,
}

/// Provenance metadata for a `DomainClaim`.
///
/// Lean by design: the rich provenance lives on the source memento. The
/// `DomainClaim` carries only the minimum required for the verifier to
/// record who staked which claim and when.
///
/// Locked JCS key order: `declared_at`, `signer`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DomainClaimProvenance {
    #[serde(rename = "declared_at")]
    pub declared_at: String,
    pub signer: String,
}

/// The canonical wire-form `k(I) = t` surface of the substrate.
///
/// Source of truth: protocol/specs/2026-05-13-domain-claim-normalization.md §1
///
/// Locked JCS key order (alphabetical):
///   `input_cid`, `kind`, `kit_cid`, `provenance`, `signature`, `truth_cid`,
///   `verdict`.
///
/// The CID-determining bytes are JCS(claim with `signature` replaced by the
/// empty string) -- see spec §3.1. CID computation lives in
/// `provekit-claim-envelope`.
///
/// Wire-name semantics (spec §1.1):
///   * `kit_cid` = `k`, the operation that produced the claim.
///   * `input_cid` = `I`, the artifact the operation was applied to.
///   * `truth_cid` = `t`, the canonical truth claim (concept, target source, ...).
///
/// `kind` is always `"domain-claim"`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DomainClaim {
    #[serde(rename = "input_cid")]
    pub input_cid: String,
    pub kind: String,
    #[serde(rename = "kit_cid")]
    pub kit_cid: String,
    pub provenance: DomainClaimProvenance,
    pub signature: String,
    #[serde(rename = "truth_cid")]
    pub truth_cid: String,
    pub verdict: VerdictBody,
}

impl DomainClaim {
    /// The canonical `kind` discriminator for a `DomainClaim` wire object.
    pub const KIND: &'static str = "domain-claim";

    /// Construct an unsigned `DomainClaim`. The `signature` field is set
    /// to the empty string; envelope-layer signers fill it in at mint
    /// time. The CID-determining bytes treat the empty-string signature
    /// as the elided placeholder (spec §3.1).
    pub fn unsigned(
        kit_cid: String,
        input_cid: String,
        truth_cid: String,
        verdict: VerdictBody,
        provenance: DomainClaimProvenance,
    ) -> Self {
        Self {
            input_cid,
            kind: Self::KIND.to_string(),
            kit_cid,
            provenance,
            signature: String::new(),
            truth_cid,
            verdict,
        }
    }
}

/// Errors that may occur projecting a memento onto the wire-form `DomainClaim`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DomainClaimConversionError {
    /// The source memento carries a verdict string not in the trichotomy.
    /// This indicates a bug in the source-memento validator (spec §2.4).
    InvalidVerdictString(String),
    /// A `RealizationDesugaringMemento` was offered standalone (not via a
    /// citing `ConceptSiteMemento`). Standalone realizations are catalog
    /// entries and are not directly verifiable through the `DomainClaim`
    /// surface (spec §2.2).
    StandaloneRealization,
    /// A bare `FunctionContractMemento` was offered directly to the verifier
    /// surface. Bare contracts have no inline discharge and MUST be wrapped
    /// in a `ConceptSiteMemento` binding or `CompoundContractMemento` (PR #716)
    /// before they can be projected onto `DomainClaim` (spec §2.3).
    UnboundContract,
}

impl std::fmt::Display for DomainClaimConversionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidVerdictString(s) => write!(
                f,
                "invalid verdict string on source memento: {s:?} \
                 (expected one of \"exact\", \"loudly-bounded-lossy\", \"refuse\")"
            ),
            Self::StandaloneRealization => write!(
                f,
                "RealizationDesugaringMemento is standalone (not cited by a \
                 ConceptSiteMemento); standalone realizations are catalog \
                 entries and do not project directly onto DomainClaim"
            ),
            Self::UnboundContract => write!(
                f,
                "FunctionContractMemento is unbound: bare contracts have no \
                 inline discharge and must be wrapped in a ConceptSiteMemento \
                 or CompoundContractMemento before projecting onto DomainClaim \
                 (spec §2.3)"
            ),
        }
    }
}

impl std::error::Error for DomainClaimConversionError {}

/// Parse a `Discharge.verdict` wire-string into a `VerdictKind`.
///
/// This parser is INFALLIBLE on well-formed source mementos: the source
/// memento's own validator rejects out-of-trichotomy verdict strings before
/// the memento is minted. A failure here indicates a substrate invariant
/// violation upstream (spec §2.4).
fn parse_verdict_kind(s: &str) -> Result<VerdictKind, DomainClaimConversionError> {
    match s {
        "exact" => Ok(VerdictKind::Exact),
        "loudly-bounded-lossy" => Ok(VerdictKind::LoudlyBoundedLossy),
        "refuse" => Ok(VerdictKind::Refuse),
        other => Err(DomainClaimConversionError::InvalidVerdictString(
            other.to_string(),
        )),
    }
}

/// `ConceptSiteMemento -> DomainClaim` (spec §2.1).
///
/// Mapping:
///   * `kit_cid`   <- `provenance.discharger_cid`
///   * `input_cid` <- `code_site.source_cid`
///   * `truth_cid` <- `concept_cid`
///   * `verdict`   <- `discharge` (trichotomy preserved by construction)
///
/// The produced claim is UNSIGNED: `signature` is the empty string and
/// `provenance.signer` / `provenance.declared_at` carry zero-value
/// placeholders. Envelope-layer signers fill these in at mint time before
/// computing the wire-form CID.
impl TryFrom<&ConceptSiteMemento> for DomainClaim {
    type Error = DomainClaimConversionError;

    fn try_from(m: &ConceptSiteMemento) -> Result<Self, Self::Error> {
        let kind = parse_verdict_kind(&m.discharge.verdict)?;
        let verdict = VerdictBody {
            discharge_receipt_cid: m.discharge.discharge_receipt_cid.clone(),
            kind,
            loss_record: m.discharge.loss_record.clone(),
            refusal_reason: m.discharge.refusal_reason.clone(),
        };
        let provenance = DomainClaimProvenance {
            declared_at: String::new(),
            signer: String::new(),
        };
        Ok(Self::unsigned(
            m.provenance.discharger_cid.clone(),
            m.code_site.source_cid.clone(),
            m.concept_cid.clone(),
            verdict,
            provenance,
        ))
    }
}

// ============================================================
// End manual extension block -- DomainClaim normalization (PR-A)
// ============================================================

// ============================================================
// MANUAL EXTENSION BLOCK -- ObligationReceiptMemento substrate (#800)
// Source of truth:
//   protocol/specs/2026-05-13-obligation-receipt-memento.md §1, §3.7
//
// This is a substrate-only outcome record. It describes a backend result
// after that backend has run; it does not invoke solvers, parse backend
// artifacts, or encode promotion/admission policy.
//
// Optional CID fields are explicit JSON nulls when absent so the durable
// wire shape is stable. Serde field order is locked to the JCS alphabetical
// key order used for CID construction.
// ============================================================

/// A durable substrate record for one proof-obligation backend outcome.
///
/// Locked JCS key order:
///   `artifact_cids`, `backend_cid`, `backend_version`,
///   `counterexample_cid`, `input_formula_cid`, `model_or_trace_cid`,
///   `obligation_cid`, `provenance_cid`, `receipt_kind`,
///   `tactic_script_cid`, `verdict`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObligationReceiptMemento {
    /// Durable proof, witness, log, transcript, or related artifact CIDs.
    #[serde(rename = "artifact_cids")]
    pub artifact_cids: Vec<String>,
    /// Backend, solver, checker, tactic engine, or verifier-stage CID.
    #[serde(rename = "backend_cid")]
    pub backend_cid: String,
    /// Backend-reported version string.
    #[serde(rename = "backend_version")]
    pub backend_version: String,
    /// Counterexample artifact CID when this is a counterexample receipt.
    #[serde(rename = "counterexample_cid")]
    pub counterexample_cid: Option<String>,
    /// CID of the canonical formula handed to the backend.
    #[serde(rename = "input_formula_cid")]
    pub input_formula_cid: String,
    /// Optional model, proof trace, partial trace, log, or transcript CID.
    #[serde(rename = "model_or_trace_cid")]
    pub model_or_trace_cid: Option<String>,
    /// CID of the obligation whose outcome is recorded.
    #[serde(rename = "obligation_cid")]
    pub obligation_cid: String,
    /// CID of the receipt-production provenance record.
    #[serde(rename = "provenance_cid")]
    pub provenance_cid: String,
    /// One of the canonical receipt kinds in §3.7.
    #[serde(rename = "receipt_kind")]
    pub receipt_kind: String,
    /// Tactic or proof-script artifact CID when this is a tactic receipt.
    #[serde(rename = "tactic_script_cid")]
    pub tactic_script_cid: Option<String>,
    /// Backend or synthesis verdict string.
    pub verdict: String,
}

/// Validation failures for the §3.7 receipt-kind, verdict, and artifact matrix.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InvalidReceiptError {
    /// The receipt kind has no known matrix row in this substrate crate.
    UnknownReceiptKind(String),
    /// The verdict is not allowed for this receipt kind.
    InvalidVerdictForKind {
        receipt_kind: String,
        verdict: String,
    },
    /// A CID field required by the matrix is absent.
    MissingRequiredCidField {
        receipt_kind: String,
        field: &'static str,
    },
    /// A CID field forbidden by the matrix is present.
    ForbiddenCidField {
        receipt_kind: String,
        field: &'static str,
    },
    /// `backend-disagreement` requires citations to the disagreeing receipts.
    MissingDisagreementArtifacts,
}

impl std::fmt::Display for InvalidReceiptError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownReceiptKind(kind) => {
                write!(f, "unknown obligation receipt kind: {kind:?}")
            }
            Self::InvalidVerdictForKind {
                receipt_kind,
                verdict,
            } => write!(
                f,
                "invalid obligation receipt verdict {verdict:?} for receipt_kind {receipt_kind:?}"
            ),
            Self::MissingRequiredCidField {
                receipt_kind,
                field,
            } => write!(
                f,
                "missing required CID field {field:?} for receipt_kind {receipt_kind:?}"
            ),
            Self::ForbiddenCidField {
                receipt_kind,
                field,
            } => write!(
                f,
                "forbidden CID field {field:?} is present for receipt_kind {receipt_kind:?}"
            ),
            Self::MissingDisagreementArtifacts => write!(
                f,
                "backend-disagreement receipts require non-empty artifact_cids citing the disagreeing receipts"
            ),
        }
    }
}

impl std::error::Error for InvalidReceiptError {}

impl ObligationReceiptMemento {
    /// Validate the normative §3.7 receipt-kind × verdict × artifact matrix.
    ///
    /// This method deliberately stays substrate-only: it checks field
    /// presence and allowed labels, but it does not invoke solvers or inspect
    /// backend-specific artifact formats.
    pub fn validate(&self) -> Result<(), InvalidReceiptError> {
        match self.receipt_kind.as_str() {
            "discharged" => {
                self.require_verdict_in(&["unsat", "sat"])?;
                self.forbid_cid("counterexample_cid", self.counterexample_cid.is_some())?;
                if self.verdict == "sat" {
                    self.require_cid("model_or_trace_cid", self.model_or_trace_cid.is_some())?;
                }
            }
            "counterexample" => {
                self.require_verdict_in(&["sat"])?;
                self.require_cid("counterexample_cid", self.counterexample_cid.is_some())?;
            }
            "tactic" => {
                self.require_verdict_in(&["unsat", "unknown"])?;
                self.require_cid("tactic_script_cid", self.tactic_script_cid.is_some())?;
                self.forbid_cid("counterexample_cid", self.counterexample_cid.is_some())?;
            }
            "inconclusive" => {
                self.require_verdict_in(&[
                    "unknown",
                    "timeout",
                    "budget-exhausted",
                    "backend-disagreement",
                ])?;
                if self.verdict == "backend-disagreement" && self.artifact_cids.is_empty() {
                    return Err(InvalidReceiptError::MissingDisagreementArtifacts);
                }
                self.forbid_cid("counterexample_cid", self.counterexample_cid.is_some())?;
                self.forbid_cid("tactic_script_cid", self.tactic_script_cid.is_some())?;
            }
            "refused" => {
                if self.verdict != "malformed-artifact" && !is_namespaced_extension(&self.verdict) {
                    return Err(self.invalid_verdict());
                }
                self.forbid_cid("counterexample_cid", self.counterexample_cid.is_some())?;
                self.forbid_cid("model_or_trace_cid", self.model_or_trace_cid.is_some())?;
                self.forbid_cid("tactic_script_cid", self.tactic_script_cid.is_some())?;
            }
            other => {
                return Err(InvalidReceiptError::UnknownReceiptKind(other.to_string()));
            }
        }

        Ok(())
    }

    fn require_verdict_in(&self, allowed: &[&str]) -> Result<(), InvalidReceiptError> {
        if allowed.iter().any(|allowed| *allowed == self.verdict) {
            Ok(())
        } else {
            Err(self.invalid_verdict())
        }
    }

    fn require_cid(&self, field: &'static str, present: bool) -> Result<(), InvalidReceiptError> {
        if present {
            Ok(())
        } else {
            Err(InvalidReceiptError::MissingRequiredCidField {
                receipt_kind: self.receipt_kind.clone(),
                field,
            })
        }
    }

    fn forbid_cid(&self, field: &'static str, present: bool) -> Result<(), InvalidReceiptError> {
        if present {
            Err(InvalidReceiptError::ForbiddenCidField {
                receipt_kind: self.receipt_kind.clone(),
                field,
            })
        } else {
            Ok(())
        }
    }

    fn invalid_verdict(&self) -> InvalidReceiptError {
        InvalidReceiptError::InvalidVerdictForKind {
            receipt_kind: self.receipt_kind.clone(),
            verdict: self.verdict.clone(),
        }
    }
}

fn is_namespaced_extension(value: &str) -> bool {
    let Some((namespace, name)) = value.split_once('/') else {
        return false;
    };
    !namespace.is_empty() && !name.is_empty()
}

// ============================================================
// End manual extension block -- ObligationReceiptMemento substrate (#800)
// ============================================================
