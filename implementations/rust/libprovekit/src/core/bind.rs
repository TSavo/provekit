// SPDX-License-Identifier: Apache-2.0

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use provekit_canonicalizer::blake3_512_of;
use provekit_ir_types::{
    ExamManifestMemento, GapKind, IrFormula, IrTerm, OptionStatus, PromotionDecisionEnvelope,
    PromotionDecisionHeader, PromotionDecisionMemento, PromotionDecisionMetadata, PromotionGate,
    PromotionResult, ResolutionOption, ResolutionOptionKind, Sort, TransportGapMemento,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value as Json};
use thiserror::Error;

use crate::proofir_bridge::CatalogIndex;

use super::primitives::address;
use super::traits::{Kit, KitError};
use super::types::{
    memento_from_parts, Cid, Contract, Dialect, DomainClaim, DomainKind, Input, Term, Verdict,
};

const CONCEPT_BIND_RESULT: &str = "concept:bind-result";
const CONCEPT_BIND_RESULT_CID: &str = "blake3-512:22dcd7895fd7abee9d9f34893b5ab9513b4801c0244a64e7a8c5180bba313f3b116d045b0aa3377f39bd892e020a1bd99d4bc60547b11fd7131fbe2f7e33dd75";
const CONCEPT_OP_APPLICATION: &str = "concept:op-application";
const CONCEPT_SEQ: &str = "concept:seq";

/// Options for the substrate bind pass.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BindOptions {
    /// Source language hint for diagnostics and named-term metadata.
    pub lang: String,
    /// Optional exam manifest used to cite refusal questions in gap records.
    pub exam_manifest: Option<ExamManifestMemento>,
}

impl Default for BindOptions {
    fn default() -> Self {
        Self {
            lang: "auto".to_string(),
            exam_manifest: None,
        }
    }
}

/// Core Kit adapter for the existing substrate binder.
#[derive(Debug, Clone, Default)]
pub struct BindKit {
    options: BindOptions,
}

impl BindKit {
    /// Build a bind Kit using the supplied binder options.
    pub fn new(options: BindOptions) -> Self {
        Self { options }
    }

    fn bind_term_from_input(&self, input: &Input) -> Result<DomainClaim, BindError> {
        let Input::Term(term) = input else {
            return Err(BindError::Failed(
                "bind kit expects Input::Term".to_string(),
            ));
        };
        let term_json = term_json_from_term(term)?;
        let realize_sidecar_hint = realize_sidecar_hint(term_json)?;
        let hashed_term = strip_realize_sidecar_from_lift_term(term.clone());
        let named = bind_term_document(term_json, &self.options)?;
        let named_cid = named_term_document_cid(&named)?;
        let payload = bind_result_payload(term.clone(), &named)?;
        let payload_value = serde_json::to_value(&payload).map_err(|error| {
            BindError::Failed(format!("serialize bind result payload: {error}"))
        })?;
        let payload_cid = address(&payload);
        let mut contract = bind_response_contract(&payload_value, &payload_cid);
        contract.concept_hint = realize_sidecar_hint;

        Ok(DomainClaim {
            domain: DomainKind::Other("bind".to_string()),
            contract,
            artifacts: vec![named_cid.clone()],
            from: vec![address(&hashed_term)],
            premises: vec![],
            to: payload_cid,
            witness: None,
            payload: Some(payload),
            verdict: Verdict::Unresolved,
            attestation: None,
        })
    }
}

impl Kit for BindKit {
    fn dialect(&self) -> Dialect {
        Dialect::Other("bind-default".to_string())
    }

    fn transform(&self, input: &Input) -> Result<DomainClaim, KitError> {
        self.bind_term_from_input(input)
            .map_err(|error| KitError::Transformation(error.to_string()))
    }

    fn prove(&self, claim: DomainClaim) -> Result<DomainClaim, KitError> {
        Ok(claim)
    }

    fn parse(&self, input: &Input) -> Result<Term, KitError> {
        self.transform(input)?
            .payload
            .ok_or_else(|| KitError::Serialization("bind claim missing term payload".to_string()))
    }

    fn serialize(&self, term: &Term) -> Result<Input, KitError> {
        Ok(Input::Term(term.clone()))
    }
}

/// Errors from the substrate bind Kit.
#[derive(Debug, Error)]
pub enum BindError {
    /// Binding the term document failed.
    #[error("{0}")]
    Failed(String),
}

impl From<String> for BindError {
    fn from(value: String) -> Self {
        Self::Failed(value)
    }
}

fn is_zero_u64(value: &u64) -> bool {
    *value == 0
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BindLiftEntry {
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub file: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub fn_name: String,
    #[serde(default, skip_serializing_if = "is_zero_u64")]
    pub fn_line: u64,
    #[serde(default)]
    pub attr_pre: Option<String>,
    #[serde(default)]
    pub attr_post: Option<String>,
    #[serde(default)]
    pub concept_annotation: Option<String>,
    #[serde(default)]
    pub param_names: Vec<String>,
    #[serde(default)]
    pub param_types: Vec<String>,
    #[serde(default)]
    pub return_type: String,
    #[serde(default)]
    pub operand_bindings: Vec<Json>,
    #[serde(default)]
    pub source_function_name: Option<String>,
    #[serde(default)]
    pub term_shape: Json,
    #[serde(default)]
    pub term_shape_cid: String,
    #[serde(default)]
    pub witnesses: Vec<BindContractWitness>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BindContractWitness {
    #[serde(default)]
    pub role: String,
    #[serde(default)]
    pub predicate: Option<Json>,
    #[serde(default)]
    pub predicate_text: Option<String>,
    #[serde(default)]
    pub source_kind: String,
    #[serde(default)]
    pub confidence_basis_points: Option<u16>,
    #[serde(default)]
    pub line: Option<u64>,
    #[serde(default)]
    pub col: Option<u64>,
    #[serde(default)]
    pub extension_fields: BTreeMap<String, Json>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NamedTermDocument {
    #[serde(rename = "candidateClusterManifest", default)]
    pub candidate_cluster_manifest: CandidateClusterManifest,
    #[serde(default, rename = "gapRecords", skip_serializing_if = "Vec::is_empty")]
    pub gap_records: Vec<Json>,
    pub kind: String,
    #[serde(rename = "promotionDecisionMementos")]
    pub promotion_decision_mementos: Vec<PromotionDecisionMemento>,
    #[serde(rename = "schemaVersion")]
    pub schema_version: String,
    #[serde(rename = "sourceLanguage")]
    pub source_language: String,
    pub terms: Vec<NamedTerm>,
    #[serde(rename = "workspaceRoot", skip_serializing_if = "Option::is_none")]
    pub workspace_root: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CandidateClusterManifest {
    pub clusters: Vec<CandidateCluster>,
    pub kind: String,
    #[serde(rename = "schemaVersion")]
    pub schema_version: String,
    #[serde(rename = "totalCandidates")]
    pub total_candidates: u64,
}

impl Default for CandidateClusterManifest {
    fn default() -> Self {
        Self {
            clusters: Vec::new(),
            kind: "candidate-cluster-manifest".to_string(),
            schema_version: "1".to_string(),
            total_candidates: 0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CandidateCluster {
    #[serde(rename = "candidateCids")]
    pub candidate_cids: Vec<String>,
    #[serde(rename = "candidateCount")]
    pub candidate_count: u64,
    #[serde(rename = "conceptCluster")]
    pub concept_cluster: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NamedTerm {
    #[serde(rename = "conceptName")]
    pub concept_name: String,
    #[serde(rename = "dischargeVerdict")]
    pub discharge_verdict: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub file: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub function: String,
    #[serde(
        default,
        rename = "fnNameSugar",
        skip_serializing_if = "Option::is_none"
    )]
    pub fn_name_sugar: Option<String>,
    pub name: String,
    #[serde(
        default,
        rename = "namedTermTree",
        skip_serializing_if = "Option::is_none"
    )]
    pub named_term_tree: Option<NamedTermTree>,
    #[serde(rename = "paramTypes")]
    pub param_types: Vec<String>,
    pub params: Vec<String>,
    #[serde(rename = "returnType")]
    pub return_type: String,
    #[serde(rename = "siteMementoCid")]
    pub site_memento_cid: String,
    #[serde(rename = "termShape")]
    pub term_shape: Json,
    #[serde(rename = "termShapeCid")]
    pub term_shape_cid: String,
    pub witnesses: Vec<NamedWitness>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NamedTermTree {
    pub args: Vec<NamedTermTree>,
    #[serde(rename = "conceptName")]
    pub concept_name: String,
    #[serde(rename = "operationKind")]
    pub operation_kind: String,
    #[serde(rename = "shapeCid")]
    pub shape_cid: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NamedWitness {
    pub predicate: Json,
    #[serde(rename = "predicateText")]
    pub predicate_text: String,
    pub role: String,
    #[serde(rename = "sourceKind")]
    pub source_kind: String,
}

/// Bind a lifted ProofIR term document into a named-term document.
pub fn bind_term_document(
    term_json: &Json,
    options: &BindOptions,
) -> Result<NamedTermDocument, BindError> {
    let entries = bind_lift_entries(term_json)?;
    let source_language = source_language(term_json, options);
    let workspace_root = workspace_root(term_json);

    let catalog = seed_catalog();
    let mut seen_names: BTreeSet<String> = BTreeSet::new();
    let mut terms = Vec::with_capacity(entries.len());
    let mut decisions = Vec::new();
    let mut gap_records = Vec::new();
    let mut operation_namer = UnnamedConceptNamer::default();
    for (idx, entry) in entries.into_iter().enumerate() {
        let concept_name = concept_name_for(&entry, idx + 1, &catalog);
        let name = unique_name(&concept_name, &mut seen_names);
        let term_shape_cid = if entry.term_shape_cid.trim().is_empty() {
            crate::canonical::json_cid(&entry.term_shape)
                .map_err(|e| format!("cid term shape for {}: {e}", entry.fn_name))?
        } else {
            entry.term_shape_cid.clone()
        };
        let site_memento_cid = site_cid(&entry, &name, &term_shape_cid)?;
        let witnesses = named_witnesses(&entry);
        let promoted_cid = blake3_512_of(format!("provekit-bind/promoted/{name}").as_bytes());
        let named_term_tree =
            named_operation_tree(&entry.term_shape, &catalog, &mut operation_namer)?;
        decisions.extend(promotion_decisions(
            &term_shape_cid,
            &promoted_cid,
            &site_memento_cid,
            &witnesses,
        )?);
        if witnesses.is_empty() {
            gap_records.push(wp_rule_synthesis_gap_record(
                &source_language,
                &term_shape_cid,
                &concept_name,
                options.exam_manifest.as_ref(),
            )?);
        }
        let fn_name = entry.fn_name;
        terms.push(NamedTerm {
            concept_name,
            discharge_verdict: if witnesses.is_empty() {
                "loudly-bounded-lossy".to_string()
            } else {
                "exact".to_string()
            },
            file: entry.file,
            function: fn_name.clone(),
            fn_name_sugar: if fn_name.is_empty() {
                None
            } else {
                Some(fn_name)
            },
            name,
            named_term_tree,
            param_types: entry.param_types,
            params: entry.param_names,
            return_type: if entry.return_type.is_empty() {
                "()".to_string()
            } else {
                entry.return_type
            },
            site_memento_cid,
            term_shape: entry.term_shape,
            term_shape_cid,
            witnesses,
        });
    }

    let candidate_cluster_manifest = candidate_cluster_manifest(&terms);

    Ok(NamedTermDocument {
        candidate_cluster_manifest,
        gap_records,
        kind: "named-term-document".to_string(),
        promotion_decision_mementos: decisions,
        schema_version: "1".to_string(),
        source_language,
        terms,
        workspace_root,
    })
}

/// Return the canonical named-term document CID emitted by `cmd_bind`.
pub fn named_term_document_cid(named: &NamedTermDocument) -> Result<Cid, BindError> {
    let canonical = bind_payload_named_term_document(named);
    let cid = crate::canonical::serializable_cid(&canonical)
        .map_err(|error| BindError::Failed(format!("cid named term JSON: {error}")))?;
    Cid::try_from(cid).map_err(|error| BindError::Failed(error.to_string()))
}

fn bind_payload_named_term_document(named: &NamedTermDocument) -> NamedTermDocument {
    let mut canonical = named.clone();
    for term in &mut canonical.terms {
        term.function.clear();
        term.fn_name_sugar = None;
    }
    canonical
}

fn bind_payload_wire_named_term_document(named: &NamedTermDocument) -> NamedTermDocument {
    let mut wire = named.clone();
    for term in &mut wire.terms {
        term.function.clear();
        // fn_name_sugar is preserved: it carries the source fn name as a
        // non-CID-affecting annotation on the citation (Option C sugar layer)
    }
    wire
}

pub fn concept_bind_result_cid() -> Cid {
    Cid::try_from(CONCEPT_BIND_RESULT_CID).expect("concept:bind-result CID is pinned")
}

pub fn bind_result_payload(
    original_term: Term,
    named: &NamedTermDocument,
) -> Result<Term, BindError> {
    let catalog = ConceptOpCatalog::load()?;
    // Wire form: function cleared (preserving #1093) but fn_name_sugar kept for
    // the lower pipeline to recover the source function name without affecting
    // the named-term-document CID (see named_term_document_cid / bind_payload_named_term_document)
    let wire_named = bind_payload_wire_named_term_document(named);
    let original_term = strip_realize_sidecar_from_lift_term(original_term);
    let named_form_binding = named_term_document_op_tree(&wire_named, &catalog)?;
    Ok(Term::Op {
        op_cid: concept_bind_result_cid(),
        name: CONCEPT_BIND_RESULT.to_string(),
        args: vec![bind_payload_source_term(original_term), named_form_binding],
    })
}

fn bind_payload_source_term(mut term: Term) -> Term {
    strip_bind_payload_source_function_from_term(&mut term);
    term
}

fn strip_bind_payload_source_function_from_term(term: &mut Term) {
    match term {
        Term::Const { value, .. } => strip_bind_payload_source_function_from_value(value),
        Term::Op { args, .. } => {
            for arg in args {
                strip_bind_payload_source_function_from_term(arg);
            }
        }
        Term::Var { .. } | Term::Unit => {}
    }
}

fn strip_bind_payload_source_function_from_value(value: &mut Json) {
    match value {
        Json::Array(values) => {
            for value in values {
                strip_bind_payload_source_function_from_value(value);
            }
        }
        Json::Object(object) => {
            if object.get("kind").and_then(Json::as_str) == Some("bind-lift-entry") {
                object.remove("fn_name");
                object.remove("fnName");
                object.remove("function");
            }
            for value in object.values_mut() {
                strip_bind_payload_source_function_from_value(value);
            }
        }
        _ => {}
    }
}

fn strip_realize_sidecar_from_lift_term(term: Term) -> Term {
    let Term::Const { mut value, sort } = term else {
        return term;
    };
    if let Some(entries) = value.get_mut("ir").and_then(Json::as_array_mut) {
        for entry in entries {
            if let Some(object) = entry.as_object_mut() {
                object.remove("operand_bindings");
                object.remove("operandBindings");
                object.remove("source_function_name");
                object.remove("sourceFunctionName");
            }
        }
    }
    Term::Const { value, sort }
}

pub fn named_term_document_from_bind_payload(
    payload: &Term,
) -> Result<NamedTermDocument, BindError> {
    match payload {
        Term::Const { value, .. } => serde_json::from_value(value.clone())
            .map_err(|error| BindError::Failed(format!("parse named term JSON: {error}"))),
        Term::Op { name, args, .. } if name == CONCEPT_BIND_RESULT => {
            let named_form_binding = args.get(1).ok_or_else(|| {
                BindError::Failed("bind-result payload missing named form binding".to_string())
            })?;
            named_term_document_from_op_tree(named_form_binding)
        }
        _ => Err(BindError::Failed(
            "bind payload is neither named-term JSON nor bind-result op tree".to_string(),
        )),
    }
}

fn term_json_from_term(term: &Term) -> Result<&Json, BindError> {
    match term {
        Term::Const { value, .. } => Ok(value),
        _ => Err(BindError::Failed(
            "bind kit expects a ProofIR JSON const term".to_string(),
        )),
    }
}

fn bind_response_contract(payload: &Json, payload_cid: &Cid) -> Contract {
    let pre = IrFormula::Atomic {
        name: "true".to_string(),
        args: vec![],
    };
    let post = IrFormula::Atomic {
        name: "=".to_string(),
        args: vec![
            IrTerm::Var {
                name: "result".to_string(),
            },
            IrTerm::Const {
                value: payload.clone(),
                sort: primitive_sort("Term"),
            },
        ],
    };

    memento_from_parts(
        "bind::default::bind-result-op-tree".to_string(),
        vec!["term".to_string()],
        vec![primitive_sort("LiftPluginResponse")],
        primitive_sort("Term"),
        pre,
        post,
        Some(payload_cid.as_str().to_string()),
    )
}

fn bind_lift_entries(term_json: &Json) -> Result<Vec<BindLiftEntry>, BindError> {
    if term_json.get("kind").and_then(Json::as_str) == Some("named-term-document") {
        return Err(BindError::Failed(
            "input is already named; bind expects ProofIR term JSON from lift".to_string(),
        ));
    }
    let ir = term_json
        .get("ir")
        .and_then(Json::as_array)
        .ok_or_else(|| BindError::Failed("ProofIR document missing `ir` array".to_string()))?;
    let mut out = Vec::new();
    for item in ir {
        if item.get("kind").and_then(Json::as_str) != Some("bind-lift-entry") {
            continue;
        }
        let entry = serde_json::from_value::<BindLiftEntry>(item.clone())
            .map_err(|e| BindError::Failed(format!("parse bind-lift-entry: {e}")))?;
        out.push(entry);
    }
    Ok(out)
}

fn realize_sidecar_hint(term_json: &Json) -> Result<Option<String>, BindError> {
    let entries = bind_lift_entries(term_json)?;
    let mut sidecar_terms = Vec::new();
    for entry in entries {
        if entry.operand_bindings.is_empty() && entry.source_function_name.is_none() {
            continue;
        }
        sidecar_terms.push(json!({
            "function": entry.fn_name,
            "operand_bindings": entry.operand_bindings,
            "source_function_name": entry.source_function_name,
        }));
    }
    if sidecar_terms.is_empty() {
        return Ok(None);
    }
    let sidecar = json!({
        "kind": "provekit-realize-sidecar",
        "terms": sidecar_terms,
    });
    serde_json::to_string(&sidecar)
        .map(|text| Some(format!("provekit-realize-sidecar:{text}")))
        .map_err(|error| BindError::Failed(format!("serialize realize sidecar: {error}")))
}

fn source_language(term_json: &Json, options: &BindOptions) -> String {
    term_json
        .get("sourceLanguage")
        .or_else(|| term_json.get("surface"))
        .and_then(Json::as_str)
        .map(str::to_string)
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| {
            if options.lang == "auto" {
                "unknown".to_string()
            } else {
                options.lang.clone()
            }
        })
}

fn workspace_root(term_json: &Json) -> Option<String> {
    term_json
        .get("workspaceRoot")
        .or_else(|| term_json.get("workspace_root"))
        .and_then(Json::as_str)
        .map(str::to_string)
}

fn concept_name_for(entry: &BindLiftEntry, ordinal: usize, catalog: &Catalog) -> String {
    if let Some(annotation) = entry.concept_annotation.as_ref().map(|name| {
        if name.starts_with("concept:") {
            name.clone()
        } else {
            format!("concept:{name}")
        }
    }) {
        return annotation;
    }
    let shape = TermShape::from_kit(entry.term_shape.clone(), entry.term_shape_cid.clone());
    catalog
        .match_shape(&shape.shape_cid(), &shape)
        .map(|entry| entry.name.clone())
        .unwrap_or_else(|| format!("UNNAMED-CONCEPT-{ordinal:x}"))
}

#[derive(Debug, Default)]
struct UnnamedConceptNamer {
    next: usize,
}

impl UnnamedConceptNamer {
    fn next(&mut self) -> String {
        self.next += 1;
        format!("UNNAMED-CONCEPT-{:x}", self.next)
    }
}

fn named_operation_tree(
    value: &Json,
    catalog: &Catalog,
    namer: &mut UnnamedConceptNamer,
) -> Result<Option<NamedTermTree>, BindError> {
    let Some(operation_kind) = operation_kind(value) else {
        return Ok(None);
    };
    let operation_shape = operation_lookup_shape(&operation_kind);
    let shape_cid = crate::canonical::json_cid(&operation_shape)
        .map_err(|e| format!("cid operation shape `{operation_kind}`: {e}"))?;
    let shape = TermShape::from_kit(operation_shape, shape_cid.clone());
    let concept_name = catalog
        .match_shape(&shape.shape_cid(), &shape)
        .map(|entry| entry.name.clone())
        .unwrap_or_else(|| namer.next());
    let args = child_operation_trees(value, catalog, namer)?;
    Ok(Some(NamedTermTree {
        args,
        concept_name,
        operation_kind,
        shape_cid,
    }))
}

fn child_operation_trees(
    value: &Json,
    catalog: &Catalog,
    namer: &mut UnnamedConceptNamer,
) -> Result<Vec<NamedTermTree>, BindError> {
    let mut out = Vec::new();
    collect_child_operation_trees(value, catalog, namer, &mut out)?;
    Ok(out)
}

fn collect_child_operation_trees(
    value: &Json,
    catalog: &Catalog,
    namer: &mut UnnamedConceptNamer,
    out: &mut Vec<NamedTermTree>,
) -> Result<(), BindError> {
    match value {
        Json::Array(values) => {
            for child in values {
                collect_operation_tree_or_descendants(child, catalog, namer, out)?;
            }
        }
        Json::Object(object) => {
            for (key, child) in object {
                if key == "kind" || key == "op" {
                    continue;
                }
                collect_operation_tree_or_descendants(child, catalog, namer, out)?;
            }
        }
        _ => {}
    }
    Ok(())
}

fn collect_operation_tree_or_descendants(
    value: &Json,
    catalog: &Catalog,
    namer: &mut UnnamedConceptNamer,
    out: &mut Vec<NamedTermTree>,
) -> Result<(), BindError> {
    if let Some(tree) = named_operation_tree(value, catalog, namer)? {
        out.push(tree);
        return Ok(());
    }
    collect_child_operation_trees(value, catalog, namer, out)
}

fn operation_kind(value: &Json) -> Option<String> {
    let raw_kind = value.get("kind").and_then(Json::as_str)?.trim();
    if raw_kind.is_empty() {
        return None;
    }
    let raw_kind = raw_kind
        .rsplit_once(':')
        .map_or(raw_kind, |(_, suffix)| suffix);
    let normalized = match raw_kind {
        "body" | "block" => "seq",
        "if" => "conditional",
        "let" => "decl",
        "bin" => value
            .get("op")
            .and_then(Json::as_str)
            .and_then(binary_operator_kind)
            .unwrap_or("bin"),
        "rel" => value
            .get("op")
            .and_then(Json::as_str)
            .and_then(binary_operator_kind)
            .unwrap_or("rel"),
        other => other,
    };
    Some(normalized.replace('_', "-"))
}

fn binary_operator_kind(op: &str) -> Option<&'static str> {
    match op {
        "+" => Some("add"),
        "-" => Some("sub"),
        "*" => Some("mul"),
        "/" => Some("div"),
        "%" => Some("mod"),
        "==" => Some("eq"),
        "!=" => Some("ne"),
        "<" => Some("lt"),
        "<=" => Some("le"),
        ">" => Some("gt"),
        ">=" => Some("ge"),
        "&&" => Some("and"),
        "||" => Some("or"),
        _ => None,
    }
}

fn operation_lookup_shape(operation_kind: &str) -> Json {
    json!({
        "kind": "operation-shape",
        "operator": operation_kind,
    })
}

fn unique_name(concept_name: &str, seen: &mut BTreeSet<String>) -> String {
    let base = concept_name
        .strip_prefix("concept:")
        .unwrap_or(concept_name)
        .to_string();
    if seen.insert(base.clone()) {
        return base;
    }
    for idx in 2usize.. {
        let candidate = format!("{base}-{idx}");
        if seen.insert(candidate.clone()) {
            return candidate;
        }
    }
    unreachable!("unbounded unique-name loop")
}

fn named_witnesses(entry: &BindLiftEntry) -> Vec<NamedWitness> {
    let mut witnesses = Vec::new();
    if entry.witnesses.is_empty() {
        if let Some(pre) = entry.attr_pre.as_deref() {
            witnesses.push(named_witness("pre", pre, "annotation"));
        }
        if let Some(post) = entry.attr_post.as_deref() {
            witnesses.push(named_witness("post", post, "annotation"));
        }
        return witnesses;
    }
    entry
        .witnesses
        .iter()
        .map(|witness| {
            let predicate_text = witness
                .predicate_text
                .clone()
                .or_else(|| witness.predicate.as_ref().map(Json::to_string))
                .unwrap_or_else(|| "true".to_string());
            NamedWitness {
                predicate: witness
                    .predicate
                    .clone()
                    .unwrap_or_else(|| json!({"kind": "text", "text": predicate_text})),
                predicate_text,
                role: if witness.role.trim().is_empty() {
                    "unknown".to_string()
                } else {
                    witness.role.clone()
                },
                source_kind: if witness.source_kind.trim().is_empty() {
                    "unspecified".to_string()
                } else {
                    witness.source_kind.clone()
                },
            }
        })
        .collect()
}

fn named_witness(role: &str, predicate_text: &str, source_kind: &str) -> NamedWitness {
    NamedWitness {
        predicate: json!({"kind": "text", "text": predicate_text}),
        predicate_text: predicate_text.to_string(),
        role: role.to_string(),
        source_kind: source_kind.to_string(),
    }
}

struct ConceptOpCatalog {
    index: CatalogIndex,
}

impl ConceptOpCatalog {
    fn load() -> Result<Self, BindError> {
        let root = find_concept_shapes_root().ok_or_else(|| {
            BindError::Failed("concept-shapes catalog root not found".to_string())
        })?;
        let index = CatalogIndex::from_catalog_root(root.join("catalog"))
            .map_err(|error| BindError::Failed(format!("load concept-shapes catalog: {error}")))?;
        Ok(Self { index })
    }

    fn required_cid(&self, name: &str) -> Result<Cid, BindError> {
        self.cid(name).ok_or_else(|| {
            BindError::Failed(format!(
                "concept op `{name}` missing from concept-shapes catalog"
            ))
        })
    }

    fn cid(&self, name: &str) -> Option<Cid> {
        self.index
            .op_definition_cid(name)
            .and_then(|cid| Cid::try_from(cid).ok())
    }

    fn resolved_name_and_cid(&self, name: &str) -> Result<(String, Cid), BindError> {
        if let Some(cid) = self.cid(name) {
            return Ok((name.to_string(), cid));
        }
        if !name.starts_with("concept:") {
            let concept_name = format!("concept:{name}");
            if let Some(cid) = self.cid(&concept_name) {
                return Ok((concept_name, cid));
            }
        }
        Ok((
            CONCEPT_OP_APPLICATION.to_string(),
            self.required_cid(CONCEPT_OP_APPLICATION)?,
        ))
    }
}

fn named_term_document_op_tree(
    named: &NamedTermDocument,
    catalog: &ConceptOpCatalog,
) -> Result<Term, BindError> {
    let mut terms = named
        .terms
        .iter()
        .enumerate()
        .map(|(idx, term)| named_term_op_tree(named, term, catalog, vec![idx]))
        .collect::<Result<Vec<_>, _>>()?;

    match terms.len() {
        0 => Ok(Term::Op {
            op_cid: catalog.required_cid(CONCEPT_OP_APPLICATION)?,
            name: CONCEPT_OP_APPLICATION.to_string(),
            args: vec![document_metadata_term(named)?],
        }),
        1 => Ok(terms.remove(0)),
        _ => {
            let mut args = vec![document_metadata_term(named)?];
            args.extend(terms);
            Ok(Term::Op {
                op_cid: catalog.required_cid(CONCEPT_SEQ)?,
                name: CONCEPT_SEQ.to_string(),
                args,
            })
        }
    }
}

fn named_term_op_tree(
    document: &NamedTermDocument,
    term: &NamedTerm,
    catalog: &ConceptOpCatalog,
    term_position: Vec<usize>,
) -> Result<Term, BindError> {
    if let Some(tree) = &term.named_term_tree {
        return named_tree_op_tree(document, term, tree, catalog, term_position);
    }
    let (resolved_name, op_cid) = catalog.resolved_name_and_cid(CONCEPT_OP_APPLICATION)?;
    let args_cid = term_args_cid(&[])?;
    let metadata = named_term_citation_term(
        document,
        term,
        None,
        &resolved_name,
        &op_cid,
        &term_position,
        &args_cid,
    )?;
    Ok(Term::Op {
        op_cid,
        name: resolved_name,
        args: vec![metadata],
    })
}

fn named_tree_op_tree(
    document: &NamedTermDocument,
    term: &NamedTerm,
    tree: &NamedTermTree,
    catalog: &ConceptOpCatalog,
    term_position: Vec<usize>,
) -> Result<Term, BindError> {
    let (resolved_name, op_cid) = catalog.resolved_name_and_cid(&tree.concept_name)?;
    let children = tree
        .args
        .iter()
        .enumerate()
        .map(|(idx, child)| {
            let mut child_position = term_position.clone();
            child_position.push(idx);
            named_tree_op_tree(document, term, child, catalog, child_position)
        })
        .collect::<Result<Vec<_>, _>>()?;
    let args_cid = term_args_cid(&children)?;
    let metadata = named_term_citation_term(
        document,
        term,
        Some(tree),
        &resolved_name,
        &op_cid,
        &term_position,
        &args_cid,
    )?;
    let mut args = Vec::with_capacity(children.len() + 1);
    args.push(metadata);
    args.extend(children);
    Ok(Term::Op {
        op_cid,
        name: resolved_name,
        args,
    })
}

fn term_args_cid(args: &[Term]) -> Result<String, BindError> {
    let value = serde_json::to_value(args)
        .map_err(|error| BindError::Failed(format!("serialize term args: {error}")))?;
    crate::canonical::json_cid(&value).map_err(|error| BindError::Failed(error.to_string()))
}

fn document_metadata_term(named: &NamedTermDocument) -> Result<Term, BindError> {
    Ok(Term::Const {
        value: document_metadata_value(named)?,
        sort: primitive_sort("NamedTermDocumentMetadata"),
    })
}

fn named_term_citation_term(
    document: &NamedTermDocument,
    term: &NamedTerm,
    tree: Option<&NamedTermTree>,
    resolved_name: &str,
    op_cid: &Cid,
    term_position: &[usize],
    args_cid: &str,
) -> Result<Term, BindError> {
    let citation_kind = if term_position.len() == 1 {
        "named-term-citation"
    } else {
        "concept-citation"
    };
    let mut value = json!({
        "kind": citation_kind,
        "argsCid": args_cid,
        "conceptCid": op_cid.as_str(),
        "resolvedConceptName": resolved_name,
        "termPosition": term_position,
    });
    if citation_kind == "named-term-citation" {
        value["term"] = serde_json::to_value(term).map_err(|error| {
            BindError::Failed(format!("serialize named term citation: {error}"))
        })?;
        value["document"] = document_metadata_value(document)?;
    }
    if let Some(tree) = tree {
        value["conceptName"] = Json::String(tree.concept_name.clone());
        value["operationKind"] = Json::String(tree.operation_kind.clone());
        value["shapeCid"] = Json::String(tree.shape_cid.clone());
    } else {
        value["conceptName"] = Json::String(term.concept_name.clone());
        value["operationKind"] = Json::String("op-application".to_string());
        value["shapeCid"] = Json::String(term.term_shape_cid.clone());
    }
    Ok(Term::Const {
        value,
        sort: primitive_sort("ConceptCitation"),
    })
}

fn document_metadata_value(named: &NamedTermDocument) -> Result<Json, BindError> {
    let mut value = json!({
        "candidateClusterManifest": serde_json::to_value(&named.candidate_cluster_manifest)
            .map_err(|error| BindError::Failed(format!("serialize candidate cluster manifest: {error}")))?,
        "kind": named.kind.clone(),
        "promotionDecisionMementos": serde_json::to_value(&named.promotion_decision_mementos)
            .map_err(|error| BindError::Failed(format!("serialize promotion decisions: {error}")))?,
        "schemaVersion": named.schema_version.clone(),
        "sourceLanguage": named.source_language.clone(),
        "workspaceRoot": named.workspace_root.clone(),
    });
    if !named.gap_records.is_empty() {
        value["gapRecords"] = serde_json::to_value(&named.gap_records)
            .map_err(|error| BindError::Failed(format!("serialize gap records: {error}")))?;
    }
    Ok(value)
}

fn candidate_cluster_manifest(terms: &[NamedTerm]) -> CandidateClusterManifest {
    let mut by_concept: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for term in terms {
        by_concept
            .entry(term.concept_name.clone())
            .or_default()
            .push(term.term_shape_cid.clone());
    }
    let clusters = by_concept
        .into_iter()
        .map(|(concept_cluster, mut candidate_cids)| {
            candidate_cids.sort();
            CandidateCluster {
                candidate_count: candidate_cids.len() as u64,
                candidate_cids,
                concept_cluster,
            }
        })
        .collect();
    CandidateClusterManifest {
        clusters,
        kind: "candidate-cluster-manifest".to_string(),
        schema_version: "1".to_string(),
        total_candidates: terms.len() as u64,
    }
}

fn named_term_document_from_op_tree(term: &Term) -> Result<NamedTermDocument, BindError> {
    let mut citations = Vec::new();
    collect_named_term_citations(term, &mut citations);
    citations.sort_by(|left, right| left.0.cmp(&right.0));
    let first = citations.first().ok_or_else(|| {
        BindError::Failed("bind-result op tree has no named-term citation".to_string())
    })?;
    let document = first.1.get("document").ok_or_else(|| {
        BindError::Failed("named-term citation missing document metadata".to_string())
    })?;
    let promotion_decision_mementos = document
        .get("promotionDecisionMementos")
        .cloned()
        .map(serde_json::from_value)
        .transpose()
        .map_err(|error| BindError::Failed(format!("parse promotion decisions: {error}")))?
        .unwrap_or_default();
    let gap_records = document
        .get("gapRecords")
        .cloned()
        .map(serde_json::from_value)
        .transpose()
        .map_err(|error| BindError::Failed(format!("parse gap records: {error}")))?
        .unwrap_or_default();
    let parsed_candidate_cluster_manifest = document
        .get("candidateClusterManifest")
        .cloned()
        .map(serde_json::from_value)
        .transpose()
        .map_err(|error| BindError::Failed(format!("parse candidate cluster manifest: {error}")))?;
    let terms = citations
        .into_iter()
        .map(|(_, citation)| {
            let value = citation
                .get("term")
                .cloned()
                .ok_or_else(|| BindError::Failed("named-term citation missing term".to_string()))?;
            serde_json::from_value::<NamedTerm>(value)
                .map_err(|error| BindError::Failed(format!("parse named term citation: {error}")))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let candidate_cluster_manifest =
        parsed_candidate_cluster_manifest.unwrap_or_else(|| candidate_cluster_manifest(&terms));
    Ok(NamedTermDocument {
        candidate_cluster_manifest,
        gap_records,
        kind: document
            .get("kind")
            .and_then(Json::as_str)
            .unwrap_or("named-term-document")
            .to_string(),
        promotion_decision_mementos,
        schema_version: document
            .get("schemaVersion")
            .and_then(Json::as_str)
            .unwrap_or("1")
            .to_string(),
        source_language: document
            .get("sourceLanguage")
            .and_then(Json::as_str)
            .unwrap_or("unknown")
            .to_string(),
        terms,
        workspace_root: document
            .get("workspaceRoot")
            .and_then(Json::as_str)
            .map(str::to_string),
    })
}

fn collect_named_term_citations<'a>(term: &'a Term, out: &mut Vec<(Vec<usize>, &'a Json)>) {
    let Term::Op { args, .. } = term else {
        return;
    };
    if let Some(Term::Const { value, .. }) = args.first() {
        if value.get("kind").and_then(Json::as_str) == Some("named-term-citation") {
            out.push((term_position_from_citation(value), value));
        }
    }
    for arg in args {
        collect_named_term_citations(arg, out);
    }
}

fn term_position_from_citation(value: &Json) -> Vec<usize> {
    value
        .get("termPosition")
        .and_then(Json::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Json::as_u64)
                .map(|value| value as usize)
                .collect()
        })
        .unwrap_or_default()
}

fn site_cid(_entry: &BindLiftEntry, name: &str, term_shape_cid: &str) -> Result<String, BindError> {
    let value = json!({
        "name": name,
        "termShapeCid": term_shape_cid,
    });
    crate::canonical::json_cid(&value).map_err(|e| BindError::Failed(e.to_string()))
}

fn promotion_decisions(
    candidate_cid: &str,
    promoted_cid: &str,
    site_memento_cid: &str,
    witnesses: &[NamedWitness],
) -> Result<Vec<PromotionDecisionMemento>, BindError> {
    witnesses
        .iter()
        .enumerate()
        .map(|(idx, witness)| {
            let evidence_cid = crate::canonical::json_cid(&json!({
                "predicate": witness.predicate,
                "predicateText": witness.predicate_text,
                "role": witness.role,
                "siteMementoCid": site_memento_cid,
                "sourceKind": witness.source_kind,
            }))
            .map_err(|e| BindError::Failed(e.to_string()))?;
            let mut decision = PromotionDecisionMemento {
                envelope: PromotionDecisionEnvelope {
                    declared_at: "2026-05-15T00:00:00.000Z".to_string(),
                    signature: String::new(),
                    signer: "builtin:provekit-bind".to_string(),
                },
                header: PromotionDecisionHeader {
                    candidate_cid: candidate_cid.to_string(),
                    cid: String::new(),
                    decider_cid: "builtin:provekit-bind".to_string(),
                    decision_payload: json!({
                        "evidence_count": 1,
                        "ordinal": idx,
                        "result": "admitted"
                    }),
                    evidence_cids: vec![evidence_cid],
                    gate: PromotionGate::Proof,
                    kind: "promotion-decision".to_string(),
                    policy_cid: "builtin:provekit-bind/default-policy".to_string(),
                    promoted_cid: promoted_cid.to_string(),
                    result: PromotionResult::Admitted,
                    schema_version: "1".to_string(),
                },
                metadata: PromotionDecisionMetadata {
                    counterexample_cids: None,
                    note: Some("bind admitted lifted evidence into named term".to_string()),
                    source_url: None,
                },
            };
            decision.header.cid = decision
                .recompute_header_cid()
                .map_err(|err| BindError::Failed(err.to_string()))?;
            decision
                .validate()
                .map_err(|err| BindError::Failed(err.to_string()))?;
            Ok(decision)
        })
        .collect()
}

fn wp_rule_synthesis_gap_record(
    source_lang: &str,
    source_op_cid: &str,
    concept_name: &str,
    exam_manifest: Option<&ExamManifestMemento>,
) -> Result<Json, BindError> {
    let target_concept_op = normalize_concept_name(concept_name);
    let (exam_question_cid, exam_manifest_cid) = crate::exam_manifest::exam_question_citation(
        exam_manifest,
        "morphism",
        &target_concept_op,
        source_lang,
        "bind",
    );
    let gap = TransportGapMemento {
        exam_manifest_cid,
        exam_question_cid,
        fn_name: format!(
            "gap:{}:bind:to:{}:wp-rule",
            source_lang,
            target_concept_op.trim_start_matches("concept:")
        ),
        gap_kind: GapKind::WpRuleMismatch,
        kind: "TransportGapMemento".to_string(),
        reason: None,
        reason_note: Some(
            "bind refused to synthesize a wp_rule without lifted contract evidence".to_string(),
        ),
        resolution_options: vec![ResolutionOption {
            dual_view_cid: None,
            loss: None,
            loss_severity: None,
            option_kind: ResolutionOptionKind::AcceptPermanent,
            partial_morphism_cid: None,
            precondition: None,
            representation_map_delta: None,
            respec_target_to: None,
            split_targets: None,
            status: OptionStatus::Deferred,
            tradeoff:
                "provide source evidence or a catalog wp_rule before treating the bind as exact"
                    .to_string(),
        }],
        schema_version: "1".to_string(),
        signature: None,
        source_lang: source_lang.to_string(),
        source_op_cid: source_op_cid.to_string(),
        target_concept_op,
        target_op_cid: None,
    };
    serde_json::to_value(gap)
        .map_err(|error| BindError::Failed(format!("serialize wp_rule gap: {error}")))
}

fn normalize_concept_name(name: &str) -> String {
    if name.starts_with("concept:") {
        name.to_string()
    } else {
        format!("concept:{name}")
    }
}

#[derive(Debug, Clone)]
struct TermShape {
    value: Json,
    cid_cached: String,
}

impl TermShape {
    fn from_kit(value: Json, cid: String) -> Self {
        Self {
            value,
            cid_cached: cid,
        }
    }

    fn shape_cid(&self) -> String {
        self.cid_cached.clone()
    }

    fn classify(&self) -> &'static str {
        classify_value(&self.value)
    }
}

fn classify_value(value: &Json) -> &'static str {
    let kind = value.get("kind").and_then(Json::as_str).unwrap_or("");
    if kind != "body" {
        return "unknown";
    }
    let stmts = value
        .get("stmts")
        .and_then(Json::as_array)
        .map(|arr| arr.as_slice())
        .unwrap_or(&[]);
    let mut has_loop = false;
    let mut has_if = false;
    for stmt in stmts {
        let kind = stmt.get("kind").and_then(Json::as_str).unwrap_or("");
        match kind {
            "while" | "for" => has_loop = true,
            "if" => has_if = true,
            _ => {}
        }
    }
    if has_loop {
        "retry-loop"
    } else if has_if {
        "guard-then-commit"
    } else {
        "unknown"
    }
}

#[derive(Debug, Clone)]
struct CatalogEntry {
    name: String,
    shape_cid: String,
    classification: &'static str,
}

#[derive(Debug, Clone)]
struct Catalog {
    entries: Vec<CatalogEntry>,
}

impl Catalog {
    fn match_shape(&self, shape_cid: &str, shape: &TermShape) -> Option<&CatalogEntry> {
        if let Some(entry) = self
            .entries
            .iter()
            .find(|entry| entry.shape_cid == shape_cid)
        {
            return Some(entry);
        }
        let classification = shape.classify();
        if classification == "unknown" {
            return None;
        }
        self.entries
            .iter()
            .find(|entry| entry.classification == classification)
    }
}

fn seed_catalog() -> Catalog {
    let mut entries = Vec::new();
    if let Some(root) = find_concept_shapes_root() {
        entries.extend(load_catalog_abstractions(&root));
        entries.extend(load_catalog_specs(&root));
    }
    entries.extend(legacy_classification_entries());
    Catalog { entries }
}

fn legacy_classification_entries() -> Vec<CatalogEntry> {
    vec![
        CatalogEntry {
            name: "concept:retry-with-bounded-attempts".to_string(),
            shape_cid: String::new(),
            classification: "retry-loop",
        },
        CatalogEntry {
            name: "concept:guard-then-commit".to_string(),
            shape_cid: String::new(),
            classification: "guard-then-commit",
        },
    ]
}

fn find_concept_shapes_root() -> Option<PathBuf> {
    let mut starts = Vec::new();
    if let Ok(cwd) = std::env::current_dir() {
        starts.push(cwd);
    }
    if let Some(manifest_dir) = option_env!("CARGO_MANIFEST_DIR") {
        starts.push(PathBuf::from(manifest_dir));
    }
    for start in starts {
        for ancestor in start.ancestors() {
            let candidate = ancestor.join("menagerie").join("concept-shapes");
            if candidate.is_dir() {
                return Some(candidate);
            }
        }
    }
    None
}

fn load_catalog_abstractions(concept_shapes_root: &Path) -> Vec<CatalogEntry> {
    let dir = concept_shapes_root.join("catalog").join("abstractions");
    catalog_json_files(&dir, ".json")
        .into_iter()
        .filter_map(|path| load_catalog_abstraction(&path))
        .collect()
}

fn load_catalog_abstraction(path: &Path) -> Option<CatalogEntry> {
    let doc = read_json_file(path)?;
    let name = doc
        .get("memento")
        .and_then(|memento| memento.get("operator"))
        .and_then(Json::as_str)?
        .to_string();
    let shape_cid = doc
        .get("cid")
        .and_then(Json::as_str)
        .map(str::to_string)
        .or_else(|| shape_cid_from_abstraction_filename(path))?;
    Some(CatalogEntry {
        name,
        shape_cid,
        classification: "catalog-shape",
    })
}

fn load_catalog_specs(concept_shapes_root: &Path) -> Vec<CatalogEntry> {
    let dir = concept_shapes_root.join("specs");
    catalog_json_files(&dir, ".spec.json")
        .into_iter()
        .flat_map(|path| load_catalog_spec(&path))
        .collect()
}

fn load_catalog_spec(path: &Path) -> Vec<CatalogEntry> {
    let Some(doc) = read_json_file(path) else {
        return Vec::new();
    };
    let Some(name) = doc
        .get("fn_name")
        .and_then(Json::as_str)
        .map(str::to_string)
    else {
        return Vec::new();
    };
    let mut entries = Vec::new();
    if let Ok(shape_cid) = crate::canonical::json_cid(&doc) {
        entries.push(CatalogEntry {
            name: name.clone(),
            shape_cid,
            classification: "catalog-shape",
        });
    }
    if name.starts_with("concept:") {
        if let Some(operator) = doc
            .get("post")
            .and_then(|post| post.get("operator"))
            .and_then(Json::as_str)
            .map(|operator| operator.replace('_', "-"))
        {
            if let Ok(shape_cid) = crate::canonical::json_cid(&operation_lookup_shape(&operator)) {
                entries.push(CatalogEntry {
                    name,
                    shape_cid,
                    classification: "catalog-shape",
                });
            }
        }
    }
    entries
}

fn catalog_json_files(dir: &Path, suffix: &str) -> Vec<PathBuf> {
    if !dir.is_dir() {
        return Vec::new();
    }
    let mut paths = Vec::new();
    collect_catalog_json_files(dir, suffix, &mut paths);
    paths.sort();
    paths
}

fn collect_catalog_json_files(dir: &Path, suffix: &str, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.filter_map(Result::ok) {
        let path = entry.path();
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if file_type.is_dir() {
            collect_catalog_json_files(&path, suffix, out);
        } else if file_type.is_file()
            && path
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.ends_with(suffix))
        {
            out.push(path);
        }
    }
}

fn read_json_file(path: &Path) -> Option<Json> {
    let bytes = std::fs::read(path).ok()?;
    serde_json::from_slice(&bytes).ok()
}

fn shape_cid_from_abstraction_filename(path: &Path) -> Option<String> {
    let file_name = path.file_name()?.to_str()?;
    let (_, cid_hex_with_suffix) = file_name.split_once(".blake3-512:")?;
    let cid_hex = cid_hex_with_suffix.strip_suffix(".json")?;
    Some(format!("blake3-512:{cid_hex}"))
}

fn primitive_sort(name: &str) -> Sort {
    Sort::Primitive {
        name: name.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bind_names_lifted_entries_without_plugin_dispatch() {
        let term = json!({
            "kind": "ir-document",
            "workspaceRoot": "/tmp/demo",
            "ir": [{
                "kind": "bind-lift-entry",
                "file": "src/lib.rs",
                "fn_name": "f",
                "concept_annotation": "demo",
                "param_names": ["x"],
                "param_types": ["i64"],
                "return_type": "i64",
                "term_shape": {"kind": "op", "name": "demo"},
                "witnesses": [{
                    "role": "post",
                    "predicate_text": "out == x",
                    "source_kind": "annotation"
                }]
            }]
        });
        let named = bind_term_document(
            &term,
            &BindOptions {
                lang: "rust".to_string(),
                exam_manifest: None,
            },
        )
        .expect("bind succeeds");
        assert_eq!(named.kind, "named-term-document");
        assert_eq!(named.terms[0].concept_name, "concept:demo");
        assert_eq!(named.terms[0].function, "f");
        assert_eq!(named.promotion_decision_mementos.len(), 1);
    }

    #[test]
    fn bind_claim_cid_ignores_realize_sidecar_symbols() {
        fn bind_claim(lhs: &str, rhs: &str) -> DomainClaim {
            let term = json!({
                "kind": "ir-document",
                "workspaceRoot": "/tmp/demo",
                "ir": [{
                    "kind": "bind-lift-entry",
                    "file": "src/lib.rs",
                    "fn_name": "add",
                    "concept_annotation": "add",
                    "param_names": ["x", "y"],
                    "param_types": ["i64", "i64"],
                    "return_type": "i64",
                    "term_shape": {
                        "kind": "op",
                        "name": "add",
                        "args": [
                            {"kind": "var", "name": "left"},
                            {"kind": "var", "name": "right"}
                        ]
                    },
                    "operand_bindings": [
                        {"position": [0], "symbol": lhs},
                        {"position": [1], "symbol": rhs}
                    ],
                    "source_function_name": "add"
                }]
            });
            let input = Input::Term(Term::Const {
                value: term,
                sort: provekit_ir_types::Sort::Primitive {
                    name: "json".to_string(),
                },
            });
            BindKit::new(BindOptions {
                lang: "rust".to_string(),
                exam_manifest: None,
            })
            .bind_term_from_input(&input)
            .expect("bind succeeds")
        }

        let xy = bind_claim("x", "y");
        let yx = bind_claim("y", "x");

        assert_ne!(xy.contract.concept_hint, yx.contract.concept_hint);
        assert_eq!(xy.to, yx.to, "bind-result payload CID must ignore sidecar");
        assert_eq!(xy.cid(), yx.cid(), "bind claim CID must ignore sidecar");
    }

    #[test]
    fn site_cid_ignores_source_file_provenance() {
        let entry = BindLiftEntry {
            fn_name: "deposit".to_string(),
            ..serde_json::from_value(json!({})).expect("default entry deserializes")
        };
        let mut entry_with_file = entry.clone();
        entry_with_file.file = "src/lib.rs".to_string();
        let mut entry_with_source_function = entry.clone();
        entry_with_source_function.fn_name = "depositSource".to_string();
        let term_shape_cid = "blake3-512:11111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111";

        assert_eq!(
            site_cid(&entry, "concept:deposit", term_shape_cid).expect("site cid"),
            site_cid(&entry_with_file, "concept:deposit", term_shape_cid).expect("site cid")
        );
        assert_eq!(
            site_cid(&entry, "concept:deposit", term_shape_cid).expect("site cid"),
            site_cid(
                &entry_with_source_function,
                "concept:deposit",
                term_shape_cid
            )
            .expect("site cid")
        );
    }

    #[test]
    fn bind_payload_cid_ignores_source_function_name() {
        fn lifted_add(fn_name: &str) -> Term {
            Term::Const {
                value: json!({
                    "kind": "ir-document",
                    "sourceLanguage": "rust",
                    "ir": [{
                        "kind": "bind-lift-entry",
                        "fn_name": fn_name,
                        "concept_annotation": "add",
                        "param_names": ["x", "y"],
                        "param_types": ["i64", "i64"],
                        "return_type": "i64",
                        "term_shape": {"kind": "bin", "op": "+"},
                        "witnesses": []
                    }]
                }),
                sort: primitive_sort("LiftPluginResponse"),
            }
        }

        let kit = BindKit::new(BindOptions {
            lang: "rust".to_string(),
            exam_manifest: None,
        });

        let add_claim = kit
            .transform(&Input::Term(lifted_add("add")))
            .expect("bind add succeeds");
        let adder_claim = kit
            .transform(&Input::Term(lifted_add("adder")))
            .expect("bind adder succeeds");

        // The named-term-document CID (artifacts) must be stable across renames (#1093)
        assert_eq!(
            add_claim.artifacts, adder_claim.artifacts,
            "source function name must not affect the named-term-document CID"
        );
        // The payload CID (to) and payload bytes legitimately differ because fn_name_sugar
        // rides in the wire citations as a non-CID-affecting annotation at the citation level.
        // Verify that the recovered named-term-document has function="" in both cases,
        // which is the load-bearing #1093 invariant.
        let add_payload = add_claim.payload.as_ref().expect("add claim has payload");
        let adder_payload = adder_claim
            .payload
            .as_ref()
            .expect("adder claim has payload");
        let add_named = named_term_document_from_bind_payload(add_payload)
            .expect("recover add named term document");
        let adder_named = named_term_document_from_bind_payload(adder_payload)
            .expect("recover adder named term document");
        assert!(
            add_named.terms[0].function.is_empty(),
            "recovered function must be empty for add (fn name lives in fn_name_sugar)"
        );
        assert!(
            adder_named.terms[0].function.is_empty(),
            "recovered function must be empty for adder (fn name lives in fn_name_sugar)"
        );
        assert_eq!(
            add_named.terms[0].fn_name_sugar.as_deref(),
            Some("add"),
            "fn_name_sugar carries the source function name in the wire form"
        );
        assert_eq!(
            adder_named.terms[0].fn_name_sugar.as_deref(),
            Some("adder"),
            "fn_name_sugar carries the source function name in the wire form"
        );
    }

    #[test]
    fn named_term_document_cid_ignores_source_function_name() {
        fn document(function: &str) -> NamedTermDocument {
            NamedTermDocument {
                candidate_cluster_manifest: CandidateClusterManifest::default(),
                gap_records: vec![],
                kind: "named-term-document".to_string(),
                promotion_decision_mementos: vec![],
                schema_version: "1".to_string(),
                source_language: "rust".to_string(),
                terms: vec![NamedTerm {
                    concept_name: "concept:add".to_string(),
                    discharge_verdict: "loudly-bounded-lossy".to_string(),
                    file: String::new(),
                    function: function.to_string(),
                    fn_name_sugar: None,
                    name: "add".to_string(),
                    named_term_tree: None,
                    param_types: vec!["i64".to_string(), "i64".to_string()],
                    params: vec!["x".to_string(), "y".to_string()],
                    return_type: "i64".to_string(),
                    site_memento_cid: "blake3-512:22222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222".to_string(),
                    term_shape: json!({"kind": "bin", "op": "+"}),
                    term_shape_cid: "blake3-512:33333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333".to_string(),
                    witnesses: vec![],
                }],
                workspace_root: None,
            }
        }

        assert_eq!(
            named_term_document_cid(&document("add")).expect("add document cid"),
            named_term_document_cid(&document("adder")).expect("adder document cid")
        );
    }

    #[test]
    fn named_term_document_omits_empty_source_provenance_fields() {
        let document = NamedTermDocument {
            candidate_cluster_manifest: CandidateClusterManifest::default(),
            gap_records: vec![],
            kind: "named-term-document".to_string(),
            promotion_decision_mementos: vec![],
            schema_version: "1".to_string(),
            source_language: "rust".to_string(),
            terms: vec![NamedTerm {
                concept_name: "concept:deposit".to_string(),
                discharge_verdict: "loudly-bounded-lossy".to_string(),
                file: String::new(),
                function: String::new(),
                fn_name_sugar: None,
                name: "concept:deposit".to_string(),
                named_term_tree: None,
                param_types: vec!["i64".to_string()],
                params: vec!["balance".to_string()],
                return_type: "i64".to_string(),
                site_memento_cid: "blake3-512:22222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222".to_string(),
                term_shape: json!({}),
                term_shape_cid: "blake3-512:33333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333".to_string(),
                witnesses: vec![],
            }],
            workspace_root: None,
        };

        let value = serde_json::to_value(&document).expect("named term document serializes");

        assert!(
            value["terms"][0].get("file").is_none(),
            "empty file provenance should not serialize into named term JSON: {value}"
        );
        assert!(
            value["terms"][0].get("function").is_none(),
            "empty function provenance should not serialize into named term JSON: {value}"
        );
    }

    #[test]
    fn bind_lift_entry_omits_default_fn_line_when_serialized() {
        let entry: BindLiftEntry =
            serde_json::from_value(json!({"fn_name": "deposit"})).expect("entry deserializes");

        let value = serde_json::to_value(&entry).expect("bind lift entry serializes");

        assert!(
            value.get("fn_line").is_none(),
            "default fn_line should not serialize: {value}"
        );
    }

    #[test]
    fn seed_catalog_loads_real_concept_shape_catalog() {
        let catalog = seed_catalog();
        assert!(
            catalog.entries.len() > 10,
            "catalog should load real concept-shape entries, got {}",
            catalog.entries.len()
        );
        assert!(
            catalog
                .entries
                .iter()
                .any(|entry| entry.name == "concept:identity"),
            "catalog should include concept:identity"
        );
        assert!(
            catalog
                .entries
                .iter()
                .any(|entry| entry.name == "concept:new"),
            "catalog should include algorithm-tier concept:new"
        );
    }

    #[test]
    fn catalog_matches_loaded_shape_cid_before_legacy_classification() {
        let catalog = seed_catalog();
        let identity_shape_cid = "blake3-512:6920f6e26184ca316f3dce6c02690b515c11b3d96d3b476bb5abe67cb55e1885031484c3add8a5f26b630e305ad3fe41eed10acca2e141898f9d6629c278867f";
        let unknown_shape = TermShape::from_kit(
            json!({
                "kind": "body",
                "stmts": []
            }),
            identity_shape_cid.to_string(),
        );
        let matched = catalog
            .match_shape(identity_shape_cid, &unknown_shape)
            .expect("identity CID should match before classify fallback");
        assert_eq!(matched.name, "concept:identity");
    }

    #[test]
    fn bind_names_blake3_512_of_operations_from_catalog() {
        let term = json!({
            "kind": "ir-document",
            "sourceLanguage": "rust",
            "workspaceRoot": "/tmp/provekit-bind-test",
            "ir": [{
                "kind": "bind-lift-entry",
                "file": "implementations/rust/provekit-canonicalizer/src/hash.rs",
                "fn_name": "blake3_512_of",
                "param_names": ["bytes"],
                "param_types": ["& [u8]"],
                "return_type": "String",
                "term_shape": {
                    "kind": "body",
                    "stmts": [
                        {"kind": "let"},
                        {"kind": "call"},
                        {"kind": "let"},
                        {"kind": "call"},
                        {"kind": "let"},
                        {"kind": "let"},
                        {"kind": "call"},
                        {"kind": "call"},
                        {"kind": "opaque"}
                    ]
                },
                "witnesses": []
            }]
        });

        let named = bind_term_document(
            &term,
            &BindOptions {
                lang: "rust".to_string(),
                exam_manifest: None,
            },
        )
        .expect("bind succeeds");
        let named_json = serde_json::to_value(&named).expect("named term serializes");
        let tree = named_json["terms"][0]
            .get("namedTermTree")
            .expect("operation-level named term tree is emitted");
        let nested_names = serde_json::to_string(tree).expect("tree stringifies");
        let mut operation_concepts = Vec::new();
        collect_tree_concept_names(tree, &mut operation_concepts);
        operation_concepts.sort();
        operation_concepts.dedup();
        eprintln!(
            "operation-level matches for blake3_512_of: {}",
            operation_concepts.join(", ")
        );

        assert!(
            nested_names.contains("\"conceptName\":\"concept:call\""),
            "blake3_512_of call operations should match catalog concept:call; tree={nested_names}"
        );
        assert!(
            tree.get("args")
                .and_then(Json::as_array)
                .is_some_and(|args| !args.is_empty()),
            "operation tree should retain recursive children; tree={tree}"
        );
    }

    fn collect_tree_concept_names(tree: &Json, out: &mut Vec<String>) {
        if let Some(name) = tree.get("conceptName").and_then(Json::as_str) {
            if name.starts_with("concept:") {
                out.push(name.to_string());
            }
        }
        if let Some(args) = tree.get("args").and_then(Json::as_array) {
            for arg in args {
                collect_tree_concept_names(arg, out);
            }
        }
    }
}
