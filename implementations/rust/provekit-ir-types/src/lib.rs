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
    /// `substitute` — an explicit, capture-avoiding, single-variable
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
    /// `apply` — application of a slot-transformer meta-variable
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
    pub kind: String,      // must be "concept-abstraction"
    pub operator: String,
    pub tier: String,      // must be "abstraction"
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
    pub kind: String,            // must be "equation"
    #[serde(rename = "fn_name")]
    pub fn_name: String,
    pub formals: Vec<String>,
    #[serde(rename = "formal_sorts")]
    pub formal_sorts: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pre: Option<IrFormula>,
    pub post: RealizationPost,
    pub role: String,            // must be "abstraction-realization"
    pub direction: String,       // must be "left-to-right"
    #[serde(rename = "target_lang")]
    pub target_lang: String,
    #[serde(rename = "loss_record")]
    pub loss_record: LossRecord,
    #[serde(rename = "discharge_receipt")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub discharge_receipt: Option<String>,
    pub effects: Vec<String>,    // always [] for the equation itself; never skip
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
    pub kind: String,           // must be "TransportGapMemento"
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
    pub kind: String,           // must be "PartialMorphismMemento"
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
    pub kind: String,           // must be "LossyMorphismMemento"
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
