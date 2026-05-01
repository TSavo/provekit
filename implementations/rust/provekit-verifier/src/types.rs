// SPDX-License-Identifier: Apache-2.0
//
// Pipeline types. Mirrors implementations/cpp/.../verifier/types.hpp.

use std::collections::BTreeMap;

use serde_json::Value as Json;

#[derive(Debug, Clone)]
pub struct LoadError {
    pub proof_path: String,
    pub reason: String,
}

#[derive(Debug, Default, Clone)]
pub struct MementoPool {
    /// CID -> the canonical-bytes-decoded memento envelope (as JSON).
    pub mementos: BTreeMap<String, Json>,
    /// sourceSymbol (IR ctor name) -> bridge envelope JSON.
    pub bridges_by_symbol: BTreeMap<String, Json>,
    pub load_errors: Vec<LoadError>,
}

#[derive(Debug, Default, Clone)]
pub struct CallSite {
    pub bridge_ir_name: String,
    pub bridge_target_cid: String,
    pub bridge_source_layer: String,
    pub bridge_target_layer: String,
    pub property_name: String,
    pub property_cid: String,
    pub arg_term: Option<Json>,
}

#[derive(Debug, Default, Clone)]
pub struct ResolvedProperty {
    pub cid: String,
    pub ir_formula: Option<Json>,
    pub ir_kit_version: String,
}

#[derive(Debug, Clone)]
pub struct Obligation {
    pub property_cid: String,
    pub ir_kit_version: String,
    pub ir_formula: Json,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObligationVerdict {
    Discharged,
    Unsatisfied,
    Undecidable,
    Disagreement,
}

impl ObligationVerdict {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Discharged => "discharged",
            Self::Unsatisfied => "unsatisfied",
            Self::Undecidable => "undecidable",
            Self::Disagreement => "disagreement",
        }
    }
}

#[derive(Debug, Clone)]
pub struct ReportRow {
    pub callsite: CallSite,
    pub status: String,
    pub reason: String,
}

#[derive(Debug, Default, Clone)]
pub struct Report {
    pub total_callsites: usize,
    pub discharged: usize,
    pub violations: usize,
    pub rows: Vec<ReportRow>,
    pub load_errors: Vec<LoadError>,
}
