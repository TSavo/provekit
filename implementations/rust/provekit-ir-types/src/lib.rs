// SPDX-License-Identifier: Apache-2.0
//
// GENERATED FILE — DO NOT EDIT
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
    Contract {
        cid: String,
    },
    #[serde(rename = "contractSet")]
    ContractSet {
        cid: String,
    },
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
// grammar grow (issue #330, rust gap from PR #361), and Float per #385.
// The codegen (`provekit-ir-codegen`) currently only emits the Primitive
// arm even though the CDDL spec defines a 6-way union. If you regenerate
// this file via `cargo run -p provekit-ir-codegen`, you WILL clobber the
// manual extensions. Re-apply them from this comment block down through
// the closing `}` of the `Sort` enum.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum Sort {
    #[serde(rename = "primitive")]
    Primitive {
        name: PrimitiveSortName,
    },
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
    /// ## NaN / IEEE-754 semantics — deliberately NOT modelled here
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
    Float {
        width: u8,
    },
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
    Var {
        name: String,
    },
    #[serde(rename = "const")]
    Const {
        value: serde_json::Value,
        sort: Sort,
    },
    #[serde(rename = "ctor")]
    Ctor {
        name: String,
        args: Vec<IrTerm>,
    },
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
    And {
        operands: Vec<IrFormula>,
    },
    #[serde(rename = "or")]
    Or {
        operands: Vec<IrFormula>,
    },
    #[serde(rename = "not")]
    Not {
        operands: Vec<IrFormula>,
    },
    #[serde(rename = "implies")]
    Implies {
        operands: Vec<IrFormula>,
    },
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
