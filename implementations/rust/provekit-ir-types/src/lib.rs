// SPDX-License-Identifier: Apache-2.0
//
// GENERATED FILE — DO NOT EDIT
// Source: protocol/provekit-ir.cddl
// Generator: provekit-ir-codegen

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvidenceTerm {
    pub kind: String,
    #[serde(rename = "proofType")]
    pub proof_type: ProofType,
    pub certificate: EvidenceCertificate,
}

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

pub type ProofType = String;

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

pub type Document = Vec<Declaration>;

pub type QuantifierKind = String;
// Known values for QuantifierKind:
//   "forall"
//   "exists"

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum Sort {
    #[serde(rename = "primitive")]
    Primitive {
        name: PrimitiveSortName,
    },
}

pub type PrimitiveSortName = String;
// Known values for PrimitiveSortName:
//   "Int"
//   "Real"
//   "Bool"
//   "String"

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

pub type ConnectiveKind = String;
// Known values for ConnectiveKind:
//   "and"
//   "or"
//   "not"
//   "implies"

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvidenceCertificate {
    pub tool: String,
    pub version: String,
    #[serde(rename = "formulaHash")]
    pub formula_hash: String,
    #[serde(rename = "proofData")]
    pub proof_data: String,
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

pub type Term = IrTerm;
pub type Formula = IrFormula;
