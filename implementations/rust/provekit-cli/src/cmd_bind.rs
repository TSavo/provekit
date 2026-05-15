// SPDX-License-Identifier: Apache-2.0
//
// `provekit bind`: substrate-only algebra pass.
//
// Input is ProofIR term JSON, normally the `ir-document` emitted by
// `provekit lift`. Output is JCS-canonical named-term JSON containing the
// clustered names plus PromotionDecisionMementos. This module deliberately
// does not import the per-language kit dispatch layer: source parsing belongs
// to `lift`, and target source emission belongs to `lower`.

use std::collections::{BTreeMap, BTreeSet};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use clap::Parser;
use owo_colors::OwoColorize;
use provekit_canonicalizer::blake3_512_of;
use provekit_ir_types::{
    PromotionDecisionEnvelope, PromotionDecisionHeader, PromotionDecisionMemento,
    PromotionDecisionMetadata, PromotionGate, PromotionResult,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value as Json};

use crate::{EXIT_OK, EXIT_USER_ERROR};

#[derive(Parser, Debug, Clone)]
pub struct BindArgs {
    /// ProofIR term JSON. Reads stdin when omitted or `-`.
    pub input: Option<PathBuf>,

    /// Output file. Writes stdout when omitted or `-`.
    #[arg(short = 'o', long = "output")]
    pub output: Option<PathBuf>,

    /// Legacy migration root. Kept for cmd_bind_migrate compatibility.
    #[arg(long, alias = "project", default_value = ".")]
    pub root: PathBuf,

    /// Source language hint for diagnostics and named-term metadata.
    #[arg(long, default_value = "auto")]
    pub lang: String,

    /// Legacy threshold hint. The substrate binder records it in metadata.
    #[arg(long, default_value = "1")]
    pub threshold: usize,

    /// Legacy rewrite flag. No effect in the four-verb model.
    #[arg(long, default_value = "invisible", value_parser = parse_rewrite)]
    pub rewrite: RewriteShape,

    /// Legacy observation mode flag. No effect in the four-verb model.
    #[arg(long, value_delimiter = ',', default_value = "monitor", value_parser = parse_mode)]
    pub mode: Vec<RuntimeMode>,

    /// Legacy target-language flag. Use `provekit lower --target=<lang>`.
    #[arg(long)]
    pub target_language: Option<String>,

    /// Source library surface for migration rewrite.
    #[arg(long)]
    pub library_from: Option<String>,

    /// Target library surface for migration rewrite.
    #[arg(long)]
    pub library_to: Option<String>,

    /// Source directory for migration rewrite.
    #[arg(long)]
    pub source_dir: Option<PathBuf>,

    /// Output directory for migration rewrite.
    #[arg(long)]
    pub out_dir: Option<PathBuf>,

    /// Receipt path for migration rewrite.
    #[arg(long)]
    pub receipt: Option<PathBuf>,

    /// Fixture sqlite database for row-shape witnesses during migration.
    #[arg(long)]
    pub witness_fixture: Option<PathBuf>,

    /// Write migrated source to out-dir. Without this flag the migration path is a dry run.
    #[arg(long)]
    pub write: bool,

    /// Suppress non-error diagnostics.
    #[arg(long)]
    pub quiet: bool,

    /// PEP 1.7.0 plugin flags retained only for migration compatibility.
    #[command(flatten)]
    pub plugins: crate::PluginFlags,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RewriteShape {
    Annotate,
    Canonical,
    Invisible,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuntimeMode {
    Monitor,
    Emitter,
    Witness,
    Gate,
}

fn parse_rewrite(s: &str) -> Result<RewriteShape, String> {
    match s {
        "annotate" => Ok(RewriteShape::Annotate),
        "canonical" => Ok(RewriteShape::Canonical),
        "invisible" => Ok(RewriteShape::Invisible),
        other => Err(format!(
            "unknown rewrite shape '{other}'; expected annotate, canonical, or invisible"
        )),
    }
}

fn parse_mode(s: &str) -> Result<RuntimeMode, String> {
    match s {
        "monitor" => Ok(RuntimeMode::Monitor),
        "emitter" => Ok(RuntimeMode::Emitter),
        "witness" => Ok(RuntimeMode::Witness),
        "gate" => Ok(RuntimeMode::Gate),
        other => Err(format!(
            "unknown runtime mode '{other}'; expected monitor, emitter, witness, or gate"
        )),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BindLiftEntry {
    #[serde(default)]
    pub kind: String,
    pub file: String,
    pub fn_name: String,
    #[serde(default)]
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
pub struct NamedTerm {
    #[serde(rename = "conceptName")]
    pub concept_name: String,
    #[serde(rename = "dischargeVerdict")]
    pub discharge_verdict: String,
    pub file: String,
    pub function: String,
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

pub fn run(args: BindArgs) -> u8 {
    if is_migration_request(&args) {
        return crate::cmd_bind_migrate::run(args);
    }

    let raw = match read_input(args.input.as_ref()) {
        Ok(raw) => raw,
        Err(error) => {
            eprintln!("{}: {error}", "error".red().bold());
            return EXIT_USER_ERROR;
        }
    };
    let term_json: Json = match serde_json::from_slice(&raw) {
        Ok(value) => value,
        Err(error) => {
            eprintln!("{}: parse ProofIR term JSON: {error}", "error".red().bold());
            return EXIT_USER_ERROR;
        }
    };
    let named = match bind_term_document(&term_json, &args) {
        Ok(named) => named,
        Err(error) => {
            eprintln!("{}: {error}", "error".red().bold());
            return EXIT_USER_ERROR;
        }
    };
    let jcs = match libprovekit::canonical::serializable_jcs(&named) {
        Ok(jcs) => jcs,
        Err(error) => {
            eprintln!(
                "{}: canonicalize named term JSON: {error}",
                "error".red().bold()
            );
            return EXIT_USER_ERROR;
        }
    };
    if let Err(error) = write_output(args.output.as_ref(), jcs.as_bytes()) {
        eprintln!("{}: {error}", "error".red().bold());
        return EXIT_USER_ERROR;
    }
    if !args.quiet
        && args
            .output
            .as_ref()
            .is_some_and(|path| path.as_os_str() != "-")
    {
        eprintln!("bind: wrote named-term JSON");
    }
    EXIT_OK
}

fn is_migration_request(args: &BindArgs) -> bool {
    args.library_from.is_some()
        || args.library_to.is_some()
        || args.source_dir.is_some()
        || args.out_dir.is_some()
        || args.receipt.is_some()
        || args.witness_fixture.is_some()
        || args.write
}

fn read_input(path: Option<&PathBuf>) -> Result<Vec<u8>, String> {
    match path {
        Some(path) if path.as_os_str() != "-" => {
            std::fs::read(path).map_err(|e| format!("read {}: {e}", path.display()))
        }
        _ => {
            let mut bytes = Vec::new();
            std::io::stdin()
                .read_to_end(&mut bytes)
                .map_err(|e| format!("read stdin: {e}"))?;
            Ok(bytes)
        }
    }
}

fn write_output(path: Option<&PathBuf>, bytes: &[u8]) -> Result<(), String> {
    match path {
        Some(path) if path.as_os_str() != "-" => {
            std::fs::write(path, bytes).map_err(|e| format!("write {}: {e}", path.display()))
        }
        _ => {
            let mut stdout = std::io::stdout().lock();
            stdout
                .write_all(bytes)
                .map_err(|e| format!("write stdout: {e}"))
        }
    }
}

fn bind_term_document(term_json: &Json, args: &BindArgs) -> Result<NamedTermDocument, String> {
    let entries = bind_lift_entries(term_json)?;
    let source_language = source_language(term_json, args);
    let workspace_root = workspace_root(term_json);

    let catalog = seed_catalog();
    let mut seen_names: BTreeSet<String> = BTreeSet::new();
    let mut terms = Vec::with_capacity(entries.len());
    let mut decisions = Vec::new();
    let mut operation_namer = UnnamedConceptNamer::default();
    for (idx, entry) in entries.into_iter().enumerate() {
        let concept_name = concept_name_for(&entry, idx + 1, &catalog);
        let name = unique_name(&concept_name, &mut seen_names);
        let term_shape_cid = if entry.term_shape_cid.trim().is_empty() {
            libprovekit::canonical::json_cid(&entry.term_shape)
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
        terms.push(NamedTerm {
            concept_name,
            discharge_verdict: if witnesses.is_empty() {
                "loudly-bounded-lossy".to_string()
            } else {
                "exact".to_string()
            },
            file: entry.file,
            function: entry.fn_name,
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

    Ok(NamedTermDocument {
        kind: "named-term-document".to_string(),
        promotion_decision_mementos: decisions,
        schema_version: "1".to_string(),
        source_language,
        terms,
        workspace_root,
    })
}

fn bind_lift_entries(term_json: &Json) -> Result<Vec<BindLiftEntry>, String> {
    if term_json.get("kind").and_then(Json::as_str) == Some("named-term-document") {
        return Err("input is already named; bind expects ProofIR term JSON from lift".to_string());
    }
    let ir = term_json
        .get("ir")
        .and_then(Json::as_array)
        .ok_or_else(|| "ProofIR document missing `ir` array".to_string())?;
    let mut out = Vec::new();
    for item in ir {
        if item.get("kind").and_then(Json::as_str) != Some("bind-lift-entry") {
            continue;
        }
        let entry = serde_json::from_value::<BindLiftEntry>(item.clone())
            .map_err(|e| format!("parse bind-lift-entry: {e}"))?;
        out.push(entry);
    }
    Ok(out)
}

fn source_language(term_json: &Json, args: &BindArgs) -> String {
    term_json
        .get("sourceLanguage")
        .or_else(|| term_json.get("surface"))
        .and_then(Json::as_str)
        .map(str::to_string)
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| {
            if args.lang == "auto" {
                "unknown".to_string()
            } else {
                args.lang.clone()
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
) -> Result<Option<NamedTermTree>, String> {
    let Some(operation_kind) = operation_kind(value) else {
        return Ok(None);
    };
    let operation_shape = operation_lookup_shape(&operation_kind);
    let shape_cid = libprovekit::canonical::json_cid(&operation_shape)
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
) -> Result<Vec<NamedTermTree>, String> {
    let mut out = Vec::new();
    collect_child_operation_trees(value, catalog, namer, &mut out)?;
    Ok(out)
}

fn collect_child_operation_trees(
    value: &Json,
    catalog: &Catalog,
    namer: &mut UnnamedConceptNamer,
    out: &mut Vec<NamedTermTree>,
) -> Result<(), String> {
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
) -> Result<(), String> {
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
        "exit" => "return",
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

fn site_cid(entry: &BindLiftEntry, name: &str, term_shape_cid: &str) -> Result<String, String> {
    let value = json!({
        "file": entry.file,
        "function": entry.fn_name,
        "name": name,
        "termShapeCid": term_shape_cid,
    });
    libprovekit::canonical::json_cid(&value).map_err(|e| e.to_string())
}

fn promotion_decisions(
    candidate_cid: &str,
    promoted_cid: &str,
    site_memento_cid: &str,
    witnesses: &[NamedWitness],
) -> Result<Vec<PromotionDecisionMemento>, String> {
    witnesses
        .iter()
        .enumerate()
        .map(|(idx, witness)| {
            let evidence_cid = libprovekit::canonical::json_cid(&json!({
                "predicate": witness.predicate,
                "predicateText": witness.predicate_text,
                "role": witness.role,
                "siteMementoCid": site_memento_cid,
                "sourceKind": witness.source_kind,
            }))
            .map_err(|e| e.to_string())?;
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
                .map_err(|err| err.to_string())?;
            decision.validate().map_err(|err| err.to_string())?;
            Ok(decision)
        })
        .collect()
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
    if let Ok(shape_cid) = libprovekit::canonical::json_cid(&doc) {
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
            if let Ok(shape_cid) =
                libprovekit::canonical::json_cid(&operation_lookup_shape(&operator))
            {
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
    let mut paths: Vec<PathBuf> = walkdir::WalkDir::new(dir)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_file())
        .map(|entry| entry.into_path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.ends_with(suffix))
        })
        .collect();
    paths.sort();
    paths
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
        let args = BindArgs {
            input: None,
            output: None,
            root: PathBuf::from("."),
            lang: "rust".to_string(),
            threshold: 1,
            rewrite: RewriteShape::Invisible,
            mode: vec![RuntimeMode::Monitor],
            target_language: None,
            library_from: None,
            library_to: None,
            source_dir: None,
            out_dir: None,
            receipt: None,
            witness_fixture: None,
            write: false,
            quiet: true,
            plugins: crate::PluginFlags::default(),
        };
        let named = bind_term_document(&term, &args).expect("bind succeeds");
        assert_eq!(named.kind, "named-term-document");
        assert_eq!(named.terms[0].concept_name, "concept:demo");
        assert_eq!(named.terms[0].function, "f");
        assert_eq!(named.promotion_decision_mementos.len(), 1);
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
        let args = BindArgs {
            input: None,
            output: None,
            root: PathBuf::from("."),
            lang: "rust".to_string(),
            threshold: 1,
            rewrite: RewriteShape::Invisible,
            mode: vec![RuntimeMode::Monitor],
            target_language: None,
            library_from: None,
            library_to: None,
            source_dir: None,
            out_dir: None,
            receipt: None,
            witness_fixture: None,
            write: false,
            quiet: true,
            plugins: crate::PluginFlags::default(),
        };

        let named = bind_term_document(&term, &args).expect("bind succeeds");
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
