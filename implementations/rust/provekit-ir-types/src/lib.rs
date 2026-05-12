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
