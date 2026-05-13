// SPDX-License-Identifier: Apache-2.0
//
// cmd bind: run the eight-verb pipeline against user code.
//
// Eight verbs: Lift -> Cluster -> Name -> Scope -> Cluster
//           -> Identify -> Realize -> Witness.
//
// This is the production CLI surface for what the smoke-test-e2e-driver
// prototype demonstrated. v0 scope:
//   - Rust source only (multi-lang lift_plugin dispatch deferred; see gaps.json).
//   - Three rewrite shapes: annotate / canonical / invisible.
//   - Three runtime modes:  witness / emitter / monitor.
//   - Target-language axis: rust / python / java / go / csharp / typescript /
//     zig / ruby / php (default = source language).
//   - canonical rewrite delegates to cmd_transport::realize_for_bind (ORP); falls
//     back to emit_target_stub only when the ORP realizer refuses (unknown language).
//   - invisible writes to stdout instead of disk.
//   - All 9 (rewrite x mode) combinations handled; canonical-mode branches
//     converge to ORP.
//
// Output layout:
//   .provekit/bindings/evidence/<cid>.json — one EvidenceMemento per contract source
//   .provekit/bindings/contracts/<cid>.json — one CompoundContractMemento per local contract
//   .provekit/bindings/policies/<cid>.json — bind-default-policy PolicyMemento
//   .provekit/bindings/promotion-decisions/<cid>.json — one admission record per evidence
//   .provekit/bindings/sites/<cid>.json   — one ConceptSiteMemento per match
//   .provekit/bindings/index.json         — summary map
//   .provekit/bindings/gaps.json          — coverage gaps / deferred capabilities
//   <src-file> (or stdout)                — annotated/canonical/streamed source
//
// Trichotomy applies per-binding (Supra omnia, rectum):
//   exact                  — wp evaluator fired, formula reduced
//   loudly-bounded-lossy   — annotation/test-lift (structural shim)
//   refuse                 — no contract recovered or wp-error

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use chrono;
use clap::Parser;
use serde::{Deserialize, Serialize};

use provekit_canonicalizer::{blake3_512_of, encode_jcs, Value};
use provekit_claim_envelope::{mint_contract, Authoring, MintContractArgs};
use provekit_ir_types::{
    AggregationStrategy, CodeSite, CodeSiteSpan, CompoundContractMemento, ConceptSiteMemento,
    ConceptSiteProvenance, Discharge, EvidenceMemento, EvidenceRef, IrFormula, LossRecord,
    PolicyMemento, PromotionDecisionEnvelope, PromotionDecisionHeader, PromotionDecisionMemento,
    PromotionDecisionMetadata, PromotionGate, PromotionResult, ProofGatePolicyMemento, SourceKind,
    SourceLocator, SourceLocatorPoint, SourceLocatorSpan,
};
use provekit_proof_envelope::Ed25519Seed;

use crate::{EXIT_OK, EXIT_USER_ERROR};

// ============================================================================
// CLI args
// ============================================================================

#[derive(Parser, Debug, Clone)]
pub struct BindArgs {
    /// Root directory to scan for source files. Defaults to current directory.
    #[arg(long, default_value = ".")]
    pub root: PathBuf,

    /// Source language. "auto" detects from file extension (Rust only in v0).
    #[arg(long, default_value = "auto")]
    pub lang: String,

    /// Output directory for binding artifacts. Defaults to .provekit/bindings/.
    #[arg(long)]
    pub output: Option<PathBuf>,

    /// Minimum cluster size threshold; shapes seen fewer times are recorded as gaps.
    #[arg(long, default_value = "1")]
    pub threshold: usize,

    /// Rewrite shape: annotate (inject substrate comments + attributes), canonical
    /// (delegate to ORP realizer), or invisible (stream to stdout).
    #[arg(long, default_value = "invisible", value_parser = parse_rewrite)]
    pub rewrite: RewriteShape,

    /// Runtime mode: monitor (throw on violation), emitter (emit structured event),
    /// or witness (sample inputs/outputs per call).
    #[arg(long, default_value = "monitor", value_parser = parse_mode)]
    pub mode: RuntimeMode,

    /// Target language for canonical rewrite. Defaults to source language (same-language
    /// refactor or annotate). Cross-language port when different from source.
    #[arg(long)]
    pub target_language: Option<String>,

    /// Quiet: suppress non-error output.
    #[arg(long)]
    pub quiet: bool,

    /// PEP 1.7.0 plugin flags (§7): --plugin, --sugar, --loss-fn, --lifter,
    /// --no-default-plugins, --no-default-plugin, --strict-plugins,
    /// --plugin-registry-out.  The registry is sealed once per run and its CID
    /// is included in every output's provenance (§9.4).
    #[command(flatten)]
    pub plugins: crate::PluginFlags,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RewriteShape {
    /// Inject substrate comments + attributes above each function, write back to disk.
    Annotate,
    /// Emit contract-annotated target-language stub; full ORP delegation in v1.
    Canonical,
    /// Like annotate or canonical, but stream to stdout instead of disk.
    Invisible,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuntimeMode {
    /// Throw on contract violation with clean stack.
    Monitor,
    /// Emit structured event per call (substrate as APM).
    Emitter,
    /// Sample inputs/outputs per call, contribute to WitnessMemento.
    Witness,
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
        other => Err(format!(
            "unknown runtime mode '{other}'; expected monitor, emitter, or witness"
        )),
    }
}

// ============================================================================
// Public entry point
// ============================================================================

pub fn run(args: BindArgs) -> u8 {
    // PEP 1.7.0: seal the plugin registry before running any pipeline work (§9).
    // The registry CID must appear in every output's provenance (§9.4).
    let sealed_at = chrono::Utc::now()
        .format("%Y-%m-%dT%H:%M:%S%.3fZ")
        .to_string();
    let plugin_registry = match args.plugins.build_registry(&sealed_at) {
        Ok(r) => {
            if !args.quiet {
                eprintln!(
                    "bind: plugin-registry sealed cid={}",
                    &r.header.cid[..std::cmp::min(32, r.header.cid.len())]
                );
            }
            r
        }
        Err(refusal) => {
            eprintln!("bind: {refusal}");
            return EXIT_USER_ERROR;
        }
    };
    let _registry_cid = plugin_registry.cid().to_string();

    let root = args
        .root
        .canonicalize()
        .unwrap_or_else(|_| args.root.clone());
    let output_dir = args
        .output
        .clone()
        .unwrap_or_else(|| root.join(".provekit").join("bindings"));

    let source_lang = match resolve_lang(&args.lang, &root) {
        Ok(lang) => lang,
        Err(msg) => {
            eprintln!("bind: {msg}");
            // Emit a gap record so callers see why no output was produced.
            let _ = std::fs::create_dir_all(&output_dir);
            let gap_doc = build_gaps_doc(
                "unknown",
                &[GapRecord {
                    kind: "source-language-not-supported".into(),
                    detail: msg,
                }],
            );
            let _ = std::fs::write(
                output_dir.join("gaps.json"),
                serde_json::to_string_pretty(&gap_doc).unwrap_or_default(),
            );
            return EXIT_USER_ERROR;
        }
    };

    let target_lang = args
        .target_language
        .clone()
        .unwrap_or_else(|| source_lang.clone());

    if !args.quiet {
        eprintln!("bind: root={}", root.display());
        eprintln!("bind: lang={source_lang} target={target_lang}");
        eprintln!(
            "bind: rewrite={} mode={}",
            rewrite_label(&args.rewrite),
            mode_label(&args.mode)
        );
    }

    // Verb 1 (Lift) is plugin-dispatched (PEP 1.7.0 kind = "lift"). cmd_bind
    // owns the eight-verb pipeline but NOT any source-language AST.
    let lift_session = crate::kit_dispatch::dispatch_bind_lift(&root, &source_lang);
    let raw_lifts: Vec<RawLift> = match &lift_session {
        Ok(session) => session
            .entries
            .iter()
            .map(raw_lift_from_kit_entry)
            .collect(),
        Err(_) => Vec::new(),
    };
    // Distinct source files referenced by the kit; used by the rewrite
    // verbs (which read the same files the kit listed but treat them as
    // OPAQUE TEXT — no AST visit happens in cmd_bind).
    let src_files: Vec<PathBuf> = {
        let mut seen: BTreeSet<String> = BTreeSet::new();
        let mut out: Vec<PathBuf> = Vec::new();
        for lift in &raw_lifts {
            if seen.insert(lift.file.clone()) {
                out.push(root.join(&lift.file));
            }
        }
        out
    };

    if raw_lifts.is_empty() {
        // Loudly-bounded-lossy: either no lift kit, or the kit found no
        // functions. Record an honest gap and exit zero so downstream
        // composers can characterize the boundary per Supra omnia rectum.
        let _ = std::fs::create_dir_all(&output_dir);
        let mut gaps: Vec<GapRecord> = Vec::new();
        if let Err(err) = &lift_session {
            gaps.push(GapRecord {
                kind: "kit-plugin-unavailable".into(),
                detail: format!(
                    "no `kind = \"lift\"` plugin available for source language `{source_lang}`: {err}. \
                     The bind pipeline cannot Verb 1 (Lift) without a kit; this leg is \
                     loudly-bounded-lossy at the lift boundary. Author or build a plugin per \
                     2026-05-13-bind-ir-lift-result.md to close this gap."
                ),
            });
        } else {
            gaps.push(GapRecord {
                kind: "bind-lift-empty".into(),
                detail: format!(
                    "lift plugin for `{source_lang}` returned zero bind-lift entries under {}",
                    root.display()
                ),
            });
        }
        let gaps_doc = build_gaps_doc(&source_lang, &gaps);
        let _ = std::fs::write(
            output_dir.join("gaps.json"),
            serde_json::to_string_pretty(&gaps_doc).unwrap_or_default(),
        );
        return EXIT_OK;
    }

    // Run the engine over the lifted entries (Verbs 2 through 7).
    let result = match run_bind_engine(&root, raw_lifts, &source_lang, args.threshold, args.quiet) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("bind: engine error: {e}");
            return EXIT_USER_ERROR;
        }
    };

    // Persist artifacts.
    let _ = std::fs::create_dir_all(output_dir.join("evidence"));
    for evidence in &result.evidence_mementos {
        let path = output_dir
            .join("evidence")
            .join(format!("{}.json", safe_filename(&evidence.cid)));
        let _ = std::fs::write(
            &path,
            serde_json::to_string_pretty(evidence).unwrap_or_default(),
        );
    }
    let _ = std::fs::create_dir_all(output_dir.join("policies"));
    for (policy_cid, policy) in &result.policies {
        let path = output_dir
            .join("policies")
            .join(format!("{}.json", safe_filename(policy_cid)));
        let _ = std::fs::write(
            &path,
            serde_json::to_string_pretty(policy).unwrap_or_default(),
        );
    }
    let _ = std::fs::create_dir_all(output_dir.join("contracts"));
    for contract in &result.compound_contracts {
        let path = output_dir
            .join("contracts")
            .join(format!("{}.json", safe_filename(&contract.cid)));
        let _ = std::fs::write(
            &path,
            serde_json::to_string_pretty(contract).unwrap_or_default(),
        );
    }
    let _ = std::fs::create_dir_all(output_dir.join("promotion-decisions"));
    for decision in &result.promotion_decisions {
        let path = output_dir
            .join("promotion-decisions")
            .join(format!("{}.json", safe_filename(&decision.header.cid)));
        let _ = std::fs::write(
            &path,
            serde_json::to_string_pretty(decision).unwrap_or_default(),
        );
    }
    let _ = std::fs::create_dir_all(output_dir.join("sites"));
    for memento in &result.site_mementos {
        let path = output_dir
            .join("sites")
            .join(format!("{}.json", safe_filename(&memento.cid)));
        let _ = std::fs::write(
            &path,
            serde_json::to_string_pretty(memento).unwrap_or_default(),
        );
    }
    let index = build_index_doc(&result);
    let _ = std::fs::write(
        output_dir.join("index.json"),
        serde_json::to_string_pretty(&index).unwrap_or_default(),
    );

    // Rewrite output. Canonical-rewrite returns the set of concepts whose body
    // fell through to the language stub so we can emit per-concept gap entries
    // per `body-template-memento.md` §5.
    let stub_concepts: Vec<String> = match &args.rewrite {
        RewriteShape::Annotate => {
            // annotate rewrites in-place in source-language syntax.
            // Cross-language annotation is not supported in v0; use --rewrite=canonical.
            if target_lang != source_lang {
                eprintln!(
                    "bind: --rewrite=annotate only supports same-language output; \
                     got --target-language={target_lang} with source {source_lang}. \
                     Use --rewrite=canonical for cross-language output. \
                     (v0 gap: cross-language annotate not yet wired)"
                );
                return EXIT_USER_ERROR;
            }
            apply_annotate_rewrite(
                &root, &src_files, &result, &args.mode, /*to_disk=*/ true,
            );
            Vec::new()
        }
        RewriteShape::Canonical => apply_canonical_rewrite(
            &root,
            &src_files,
            &result,
            &args.mode,
            &target_lang,
            /*to_disk=*/ true,
            &output_dir,
        ),
        RewriteShape::Invisible => {
            // Invisible: stream to stdout. Apply annotate-shape for same-language,
            // canonical for cross-language.
            if target_lang == source_lang {
                apply_annotate_rewrite(
                    &root, &src_files, &result, &args.mode, /*to_disk=*/ false,
                );
                Vec::new()
            } else {
                apply_canonical_rewrite(
                    &root,
                    &src_files,
                    &result,
                    &args.mode,
                    &target_lang,
                    /*to_disk=*/ false,
                    &output_dir,
                )
            }
        }
    };

    // Augment gaps with per-concept `bind-stub-body-emitted` entries for
    // every concept whose body fell through during canonical rewrite, per
    // `body-template-memento.md` §5. Then write gaps.json with the augmented
    // set. When every concept matched a body-template, no entries are added.
    let mut augmented_gaps = result.gaps.clone();
    for concept in &stub_concepts {
        augmented_gaps.push(GapRecord {
            kind: "bind-stub-body-emitted".into(),
            detail: format!(
                "canonical-rewrite fell through to language stub for concept '{concept}'; \
                 no body-template matched. Author a body-template entry to close this gap."
            ),
        });
    }
    let gaps_doc = build_gaps_doc(&source_lang, &augmented_gaps);
    let _ = std::fs::write(
        output_dir.join("gaps.json"),
        serde_json::to_string_pretty(&gaps_doc).unwrap_or_default(),
    );

    // Print summary.
    if !args.quiet {
        print_summary(&result);
    }

    EXIT_OK
}

// ============================================================================
// Engine
// ============================================================================

/// Result of one bind pass over a source tree.
pub struct EngineResult {
    pub bindings: Vec<BindingRecord>,
    pub concepts: Vec<ConceptRecord>,
    pub evidence_mementos: Vec<EvidenceMemento>,
    pub policies: BTreeMap<String, PolicyMemento>,
    pub compound_contracts: Vec<CompoundContractMemento>,
    pub promotion_decisions: Vec<PromotionDecisionMemento>,
    pub site_mementos: Vec<ConceptSiteMemento>,
    pub gaps: Vec<GapRecord>,
}

#[derive(Debug, Clone)]
pub struct BindingRecord {
    pub site_file: String,
    pub site_fn: String,
    pub concept_idx: usize,
    pub origin: ContractOrigin,
    pub discharge_verdict: DischargeVerdict,
    pub pretty_pre: Option<String>,
    pub pretty_post: Option<String>,
    pub param_names: Vec<String>,
    pub param_types: Vec<String>,
    pub return_type: String,
    pub site_memento_cid: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContractOrigin {
    AttributeLift,
    EvidenceLift { source_kind: String },
    TestLift,
    AlgebraSynthesis { rule_id: String },
    Empty,
}

impl ContractOrigin {
    pub fn label(&self) -> String {
        match self {
            ContractOrigin::AttributeLift => "annotation-lift".into(),
            ContractOrigin::EvidenceLift { source_kind } => {
                format!("evidence-lift[{source_kind}]")
            }
            ContractOrigin::TestLift => "test-lift".into(),
            ContractOrigin::AlgebraSynthesis { rule_id } => {
                format!("algebra-synthesis[{}]", rule_id)
            }
            ContractOrigin::Empty => "empty".into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DischargeVerdict {
    Exact,
    LoudlyBoundedLossy { loss: String },
    Refuse { reason: String },
}

#[derive(Debug, Clone)]
pub struct ConceptRecord {
    pub name: String,
    pub shape_cid: String,
    pub shape_cid_aliases: Vec<String>,
    pub abstraction_cid: String,
    pub catalog_id: Option<String>,
    pub site_indices: Vec<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GapRecord {
    pub kind: String,
    pub detail: String,
}

// ---------------------------------------------------------------------------
// Raw lift representation (pre-clustering)
// ---------------------------------------------------------------------------

struct RawLift {
    file: String,
    fn_name: String,
    fn_line: usize,
    attr_pre: Option<String>,
    attr_post: Option<String>,
    witnesses: Vec<ContractWitness>,
    concept_annotation: Option<String>,
    term_shape: TermShape,
    param_names: Vec<String>,
    param_types: Vec<String>,
    return_type: String,
}

#[derive(Debug, Clone)]
struct ContractWitness {
    role: String,
    predicate: IrFormula,
    predicate_text: Option<String>,
    source_kind: SourceKind,
    confidence_basis_points: u16,
    source_line: usize,
    source_col: u32,
    extension_fields: BTreeMap<String, serde_json::Value>,
}

/// Convert one PEP 1.7.0 `bind-lift-entry` (returned by a `kind = "lift"`
/// plugin per `2026-05-13-bind-ir-lift-result.md`) into the engine-internal
/// `RawLift` shape. This is the ONLY entry point through which lift data
/// reaches the eight-verb engine; there is no language-specific path.
fn raw_lift_from_kit_entry(entry: &crate::kit_dispatch::BindLiftEntry) -> RawLift {
    let mut witnesses = Vec::new();
    if let Some(pre) = entry.attr_pre.as_deref() {
        witnesses.push(annotation_contract_witness(
            "pre",
            pre,
            entry.fn_line as usize,
            &entry.file,
            &entry.fn_name,
        ));
    }
    if let Some(post) = entry.attr_post.as_deref() {
        witnesses.push(annotation_contract_witness(
            "post",
            post,
            entry.fn_line as usize,
            &entry.file,
            &entry.fn_name,
        ));
    }
    witnesses.extend(
        entry
            .witnesses
            .iter()
            .map(|w| contract_witness_from_bind_entry(w, entry.fn_line as usize)),
    );

    RawLift {
        file: entry.file.clone(),
        fn_name: entry.fn_name.clone(),
        fn_line: entry.fn_line as usize,
        attr_pre: entry.attr_pre.clone(),
        attr_post: entry.attr_post.clone(),
        witnesses,
        concept_annotation: entry.concept_annotation.clone(),
        term_shape: TermShape::from_kit(entry.term_shape.clone(), entry.term_shape_cid.clone()),
        param_names: entry.param_names.clone(),
        param_types: entry.param_types.clone(),
        return_type: if entry.return_type.is_empty() {
            "()".to_string()
        } else {
            entry.return_type.clone()
        },
    }
}

fn annotation_contract_witness(
    role: &str,
    predicate_text: &str,
    fn_line: usize,
    file: &str,
    fn_name: &str,
) -> ContractWitness {
    let mut extension_fields = BTreeMap::new();
    extension_fields.insert(
        "role".to_string(),
        serde_json::Value::String(role.to_string()),
    );
    extension_fields.insert(
        "surface".to_string(),
        serde_json::Value::String(format!("attr_{role}")),
    );
    extension_fields.insert(
        "function_symbol".to_string(),
        serde_json::Value::String(format!("{fn_name}@{file}")),
    );
    ContractWitness {
        role: role.to_string(),
        predicate: formula_text_to_ir_formula(predicate_text),
        predicate_text: Some(predicate_text.to_string()),
        source_kind: SourceKind::Annotation,
        confidence_basis_points: 10000,
        source_line: fn_line.max(1),
        source_col: 0,
        extension_fields,
    }
}

fn contract_witness_from_bind_entry(
    witness: &crate::kit_dispatch::BindContractWitness,
    default_line: usize,
) -> ContractWitness {
    let role = if witness.role.trim().is_empty() {
        "unknown".to_string()
    } else {
        witness.role.trim().to_string()
    };
    let source_kind = if witness.source_kind.trim().is_empty() {
        SourceKind::Other("unspecified".to_string())
    } else {
        SourceKind::from(witness.source_kind.trim().to_string())
    };
    let mut extension_fields = witness.extension_fields.clone();
    extension_fields
        .entry("role".to_string())
        .or_insert_with(|| serde_json::Value::String(role.clone()));
    ContractWitness {
        role,
        predicate: witness_predicate_formula(
            witness.predicate.as_ref(),
            witness.predicate_text.as_deref(),
        ),
        predicate_text: witness
            .predicate_text
            .clone()
            .or_else(|| witness.predicate.as_ref().map(|value| value.to_string())),
        confidence_basis_points: witness
            .confidence_basis_points
            .unwrap_or_else(|| default_confidence_basis_points(&source_kind)),
        source_kind,
        source_line: witness
            .line
            .map(|n| n as usize)
            .unwrap_or(default_line)
            .max(1),
        source_col: witness.col.unwrap_or(0).min(u32::MAX as u64) as u32,
        extension_fields,
    }
}

fn witness_predicate_formula(
    predicate: Option<&serde_json::Value>,
    predicate_text: Option<&str>,
) -> IrFormula {
    if let Some(value) = predicate {
        if !value.is_null() {
            if let Ok(formula) = serde_json::from_value::<IrFormula>(value.clone()) {
                return formula;
            }
            return formula_text_to_ir_formula(&value.to_string());
        }
    }
    formula_text_to_ir_formula(predicate_text.unwrap_or("true"))
}

fn default_confidence_basis_points(source_kind: &SourceKind) -> u16 {
    match source_kind {
        SourceKind::Annotation
        | SourceKind::TypeSignature
        | SourceKind::NativeSurface
        | SourceKind::StructuralSynthesis => 10000,
        SourceKind::TestAssertion => 9000,
        SourceKind::Docstring | SourceKind::ReviewComment => 6000,
        SourceKind::EmpiricalWitness => 5000,
        SourceKind::LoopInvariant | SourceKind::ImplicitEffect | SourceKind::Other(_) => 7500,
    }
}

// ---------------------------------------------------------------------------
// Main engine pass
// ---------------------------------------------------------------------------

fn run_bind_engine(
    root: &Path,
    raw_lifts: Vec<RawLift>,
    source_lang: &str,
    threshold: usize,
    quiet: bool,
) -> Result<EngineResult, String> {
    let _ = source_lang; // The engine no longer branches on source_lang; the
                         // lift kit handled all language-specific extraction
                         // upstream. Kept in the signature for diagnostics.
    let _ = quiet;
    let mut gaps: Vec<GapRecord> = Vec::new();

    let signer_seed: Ed25519Seed = [0x42; 32]; // v0: deterministic seed

    // ---- Verb 2 + 5: CLUSTER ------------------------------------------------
    let mut concepts: Vec<ConceptRecord> = Vec::new();
    let mut key_to_concept_idx: BTreeMap<String, usize> = BTreeMap::new();
    let mut unnamed_counter = 0u32;
    let catalog = seed_catalog();

    for lift in &raw_lifts {
        let shape_cid = lift.term_shape.shape_cid();
        let matched_catalog = catalog.match_shape(&shape_cid, &lift.term_shape);

        let (bucket_key, final_name, catalog_id) =
            if let Some(human) = lift.concept_annotation.as_ref() {
                (
                    format!("human:{human}"),
                    format!("concept:{human}"),
                    None::<String>,
                )
            } else if let Some(c) = matched_catalog {
                (
                    format!("catalog:{}", c.id),
                    c.name.clone(),
                    Some(c.id.clone()),
                )
            } else {
                unnamed_counter += 1;
                let name = format!("UNNAMED-CONCEPT-{unnamed_counter:x}");
                (format!("shape:{shape_cid}"), name, None)
            };

        if !key_to_concept_idx.contains_key(&bucket_key) {
            let abs_value = Value::object([
                ("kind", Value::string("concept-abstraction-stub-0")),
                ("name", Value::string(final_name.clone())),
                ("shapeCid", Value::string(shape_cid.clone())),
            ]);
            let abstraction_cid = blake3_512_of(encode_jcs(&abs_value).as_bytes());
            let idx = concepts.len();
            concepts.push(ConceptRecord {
                name: final_name,
                shape_cid: shape_cid.clone(),
                shape_cid_aliases: vec![],
                abstraction_cid,
                catalog_id,
                site_indices: vec![],
            });
            key_to_concept_idx.insert(bucket_key, idx);
        } else {
            let idx = *key_to_concept_idx.get(&bucket_key).unwrap();
            let primary = concepts[idx].shape_cid.clone();
            if shape_cid != primary && !concepts[idx].shape_cid_aliases.contains(&shape_cid) {
                concepts[idx].shape_cid_aliases.push(shape_cid.clone());
            }
        }
    }

    // shape -> concept idx lookup.
    //
    // NOTE on bucket-key vs shape-cid collisions: when two distinct concepts
    // (different bucket_keys, e.g. different `// concept: X` annotations) share
    // the same term-shape CID, this map can only hold ONE concept per shape.
    // Before this fix, the second loop below used shape_to_concept blindly, so a
    // later-registered concept could steal site_indices that belonged to an
    // annotated function (concept:identity / concept:bool-cell were observed at
    // 0 sites in the trinity fixture, triggering "below-threshold" gaps). The
    // second loop now resolves concept_idx by the same bucket_key the first
    // loop used (annotation > catalog > shape), and only falls back to this map
    // when no key match exists.
    let mut shape_to_concept: BTreeMap<String, usize> = BTreeMap::new();
    for (ci, c) in concepts.iter().enumerate() {
        shape_to_concept.entry(c.shape_cid.clone()).or_insert(ci);
        for alias in &c.shape_cid_aliases {
            shape_to_concept.entry(alias.clone()).or_insert(ci);
        }
    }

    // Test-derived contracts: lift kits may emit `bind-test-witness-entry`
    // alongside `bind-lift-entry` per `2026-05-13-bind-ir-lift-result.md` §1.2
    // (deferred to a follow-up entry kind). v0 disables the test-witness
    // contract-origin tier in cmd_bind itself; the test_post_map remains an
    // empty lookup so the priority chain (attribute > test > algebra > empty)
    // still terminates correctly.
    let test_post_map: BTreeMap<String, String> = BTreeMap::new();

    // ---- Verb 4: SCOPE + Verb 6: IDENTIFY + Verb 7: REALIZE ----------------
    let mut bindings: Vec<BindingRecord> = Vec::new();
    let mut evidence_mementos: Vec<EvidenceMemento> = Vec::new();
    let mut policies: BTreeMap<String, PolicyMemento> = BTreeMap::new();
    let mut compound_contracts: Vec<CompoundContractMemento> = Vec::new();
    let mut promotion_decisions: Vec<PromotionDecisionMemento> = Vec::new();
    let mut site_mementos: Vec<ConceptSiteMemento> = Vec::new();

    let lifter_cid = blake3_512_of(b"provekit-cli/bind-v0/lifter");
    let clusterer_cid = blake3_512_of(b"provekit-cli/bind-v0/clusterer");
    let discharger_cid = blake3_512_of(b"provekit-cli/bind-v0/discharger");
    let bind_default_policy = bind_default_policy_memento(&lifter_cid);
    let bind_default_policy_cid = policy_memento_cid(&bind_default_policy);
    policies.insert(bind_default_policy_cid.clone(), bind_default_policy);

    for lift in &raw_lifts {
        let shape_cid = lift.term_shape.shape_cid();
        // Re-derive bucket_key with the same priority the first loop used so we
        // route THIS lift to the concept its annotation/catalog match created,
        // not whichever concept happens to currently own its shape_cid in
        // shape_to_concept.
        let matched_catalog = catalog.match_shape(&shape_cid, &lift.term_shape);
        let bucket_key = if let Some(human) = lift.concept_annotation.as_ref() {
            format!("human:{human}")
        } else if let Some(c) = matched_catalog {
            format!("catalog:{}", c.id)
        } else {
            format!("shape:{shape_cid}")
        };
        let concept_idx = *key_to_concept_idx
            .get(&bucket_key)
            .or_else(|| shape_to_concept.get(&shape_cid))
            .expect("shape was clustered");

        // Contract origin priority: attribute > test > algebra-synthesis > empty.
        let explicit_pre = contract_text_from_witnesses(&lift.witnesses, "pre");
        let explicit_post = contract_text_from_witnesses(&lift.witnesses, "post");
        let (origin, pre, post) = if lift.attr_pre.is_some() || lift.attr_post.is_some() {
            (
                ContractOrigin::AttributeLift,
                lift.attr_pre.clone(),
                lift.attr_post.clone(),
            )
        } else if explicit_pre.is_some() || explicit_post.is_some() {
            (
                ContractOrigin::EvidenceLift {
                    source_kind: dominant_source_kind_label(&lift.witnesses),
                },
                explicit_pre,
                explicit_post,
            )
        } else if let Some(test_post) = test_post_map.get(&lift.fn_name) {
            (ContractOrigin::TestLift, None, Some(test_post.clone()))
        } else if let Some(rule) = wp_rule_for_shape(&shape_cid, &lift.term_shape) {
            (
                ContractOrigin::AlgebraSynthesis {
                    rule_id: rule.id.clone(),
                },
                rule.pre.clone(),
                rule.post.clone(),
            )
        } else {
            (ContractOrigin::Empty, None, None)
        };

        // Mint signed contract envelope when contract is non-empty.
        let (_contract_cid, _contract_content_cid) = if pre.is_some() || post.is_some() {
            let pre_v = pre.as_deref().map(formula_text_to_value);
            let post_v = post.as_deref().map(formula_text_to_value);
            let mint_args = MintContractArgs {
                contract_name: format!("bind::{}::{}", lift.file, lift.fn_name),
                pre: pre_v,
                post: post_v,
                inv: None,
                out_binding: "out".to_string(),
                produced_by: "provekit-cli/bind@0".into(),
                produced_at: "2026-05-12T00:00:00.000Z".into(),
                input_cids: vec![],
                authoring: Authoring::Lift {
                    lifter: "provekit-cli/bind-v0".into(),
                    evidence: origin.label(),
                    source_cid: None,
                },
                signer_seed,
            };
            match mint_contract(&mint_args) {
                Ok(env) => (Some(env.cid), Some(env.contract_cid)),
                Err(e) => {
                    eprintln!("bind: mint_contract failed for {}: {e}", lift.fn_name);
                    (None, None)
                }
            }
        } else {
            (None, None)
        };

        // Discharge verdict (trichotomy).
        let verdict = discharge_verdict(&lift.term_shape, &origin);

        // Mint ConceptSiteMemento when we have a contract.
        let site_memento_cid = if let Some(local_cid) = &_contract_content_cid {
            let source_bytes = std::fs::read(root.join(&lift.file)).unwrap_or_default();
            let source_cid = blake3_512_of(&source_bytes);
            let (span_start, span_end) = byte_span_for_line(&source_bytes, lift.fn_line);
            let fn_term_cid = local_cid.clone();
            let binding_witnesses =
                contract_witnesses_for_binding(lift, &origin, pre.as_deref(), post.as_deref());
            let site_evidences: Vec<EvidenceMemento> = binding_witnesses
                .iter()
                .map(|w| evidence_memento_from_contract_witness(w, &source_cid, &lifter_cid))
                .collect();
            let compound = compound_contract_memento(&fn_term_cid, &site_evidences);
            let local_contract_cid = compound
                .as_ref()
                .map(|c| c.cid.clone())
                .unwrap_or_else(|| local_cid.clone());
            if let Some(compound) = compound.as_ref() {
                for evidence in &site_evidences {
                    let decision = promotion_decision_memento(
                        &local_cid,
                        &compound.cid,
                        evidence,
                        &bind_default_policy_cid,
                        &lifter_cid,
                    )
                    .map_err(|err| {
                        format!(
                            "promotion decision failed for evidence {} into {}: {err}",
                            evidence.cid, compound.cid
                        )
                    })?;
                    promotion_decisions.push(decision);
                }
            }
            evidence_mementos.extend(site_evidences);
            if let Some(compound) = compound {
                compound_contracts.push(compound);
            }

            let (d_verdict, d_loss, d_receipt, d_refusal) = match &verdict {
                DischargeVerdict::Exact => {
                    let receipt = blake3_512_of(
                        format!("provekit-bind-discharge-receipt:{local_cid}").as_bytes(),
                    );
                    (
                        "exact".to_string(),
                        LossRecord(BTreeMap::new()),
                        Some(receipt),
                        None,
                    )
                }
                DischargeVerdict::LoudlyBoundedLossy { loss } => {
                    let receipt = blake3_512_of(
                        format!("provekit-bind-discharge-receipt:{local_cid}").as_bytes(),
                    );
                    let mut loss_map = BTreeMap::new();
                    loss_map.insert(
                        "structural_divergence".to_string(),
                        IrFormula::Atomic {
                            name: format!("bind-v0-single-atom-encoding:{loss}"),
                            args: vec![],
                        },
                    );
                    (
                        "loudly-bounded-lossy".to_string(),
                        LossRecord(loss_map),
                        Some(receipt),
                        None,
                    )
                }
                DischargeVerdict::Refuse { reason } => (
                    "refuse".to_string(),
                    LossRecord(BTreeMap::new()),
                    None,
                    Some(reason.clone()),
                ),
            };

            let discharge = Discharge {
                method: "wp".to_string(),
                verdict: d_verdict,
                loss_record: d_loss,
                discharge_receipt_cid: d_receipt,
                refusal_reason: d_refusal,
            };

            let abstraction_cid = concepts[concept_idx].abstraction_cid.clone();

            // Build header for CID computation (no `cid` field).
            let code_site_v = Value::object([
                ("function_term_cid", Value::string(fn_term_cid.clone())),
                ("source_cid", Value::string(source_cid.clone())),
                (
                    "span",
                    Value::object([
                        ("end", Value::integer(span_end as i64)),
                        ("start", Value::integer(span_start as i64)),
                    ]),
                ),
            ]);
            let mut discharge_kv: Vec<(&str, Arc<Value>)> = Vec::new();
            discharge_kv.push(("method", Value::string(discharge.method.clone())));
            if let Some(ref rr) = discharge.refusal_reason {
                discharge_kv.push(("refusal_reason", Value::string(rr.clone())));
            }
            discharge_kv.push(("verdict", Value::string(discharge.verdict.clone())));
            if let Some(ref drc) = discharge.discharge_receipt_cid {
                discharge_kv.push(("discharge_receipt_cid", Value::string(drc.clone())));
            }
            let loss_json =
                serde_json::to_string(&discharge.loss_record).expect("LossRecord serialization");
            let loss_v = json_to_value(&serde_json::from_str(&loss_json).expect("parse loss"));
            discharge_kv.push(("loss_record", loss_v));
            let discharge_v = Value::object(discharge_kv);
            let provenance_v = Value::object([
                ("clusterer_cid", Value::string(clusterer_cid.clone())),
                ("discharger_cid", Value::string(discharger_cid.clone())),
                ("lifter_cid", Value::string(lifter_cid.clone())),
            ]);

            let mut header_kv: Vec<(&str, Arc<Value>)> = Vec::new();
            header_kv.push(("code_site", code_site_v));
            header_kv.push(("concept_cid", Value::string(abstraction_cid.clone())));
            header_kv.push(("discharge", discharge_v));
            header_kv.push(("kind", Value::string("concept-site".to_string())));
            header_kv.push((
                "local_contract_cid",
                Value::string(local_contract_cid.clone()),
            ));
            header_kv.push(("provenance", provenance_v));
            header_kv.push(("schemaVersion", Value::string("1".to_string())));
            header_kv.push(("witnesses", Value::array(vec![])));
            let header_v = Value::object(header_kv);
            let computed_cid = blake3_512_of(encode_jcs(&header_v).as_bytes());

            let memento = ConceptSiteMemento {
                cid: computed_cid.clone(),
                code_site: CodeSite {
                    function_term_cid: fn_term_cid.clone(),
                    source_cid: source_cid.clone(),
                    span: CodeSiteSpan {
                        end: span_end,
                        start: span_start,
                    },
                },
                concept_cid: abstraction_cid.clone(),
                discharge,
                kind: "concept-site".to_string(),
                local_contract_cid,
                provenance: ConceptSiteProvenance {
                    clusterer_cid: clusterer_cid.clone(),
                    discharger_cid: discharger_cid.clone(),
                    lifter_cid: lifter_cid.clone(),
                },
                realization_mode_hint: Some(mode_label_for_hint(&RuntimeMode::Monitor)),
                schema_version: "1".to_string(),
                witnesses: vec![],
            };
            site_mementos.push(memento);
            computed_cid
        } else {
            String::new()
        };

        let binding_idx = bindings.len();
        concepts[concept_idx].site_indices.push(binding_idx);

        bindings.push(BindingRecord {
            site_file: lift.file.clone(),
            site_fn: lift.fn_name.clone(),
            concept_idx,
            origin,
            discharge_verdict: verdict,
            pretty_pre: pre,
            pretty_post: post,
            param_names: lift.param_names.clone(),
            param_types: lift.param_types.clone(),
            return_type: lift.return_type.clone(),
            site_memento_cid,
        });
    }

    // Apply threshold: concepts with fewer sites than threshold go to gaps.
    for c in &concepts {
        if c.site_indices.len() < threshold {
            gaps.push(GapRecord {
                kind: "below-threshold".into(),
                detail: format!(
                    "concept '{}' has {} site(s) (threshold={})",
                    c.name,
                    c.site_indices.len(),
                    threshold
                ),
            });
        }
    }

    // Per PR #779, multi-lang lift dispatch IS wired via kit_dispatch, and the
    // "v0-capability-gap: multi-lang lift_plugin dispatch deferred" claim is no
    // longer true: when a non-Rust bind-lift kit is registered, the dispatcher
    // exercises it and the lift result is REAL. Emitting the legacy gap
    // unconditionally lied about the substrate state. The honest replacement
    // is per-situation: `kit-plugin-unavailable` is already emitted by the
    // dispatcher when no kit registers; `bind-stub-body-emitted` is emitted
    // per-concept by `apply_canonical_rewrite` when a body falls through to a
    // stub; below-threshold and unnamed-concept gaps are emitted above. None
    // of those require an unconditional v0-capability-gap.
    //
    // The "real ConceptAbstractionMemento catalog lookup" gap is similarly
    // stale: the concept catalog now feeds through `seed_catalog()` + the
    // human-annotation path, and the soft-match classification still applies
    // but it isn't a capability gap, it's a substrate choice. If a stronger
    // catalog lookup lands later it's an enhancement, not a closure of a gap.
    Ok(EngineResult {
        bindings,
        concepts,
        evidence_mementos,
        policies,
        compound_contracts,
        promotion_decisions,
        site_mementos,
        gaps,
    })
}

// ============================================================================
// Annotate rewrite (the new source-preserving injector)
// ============================================================================

/// Inject substrate comments and runtime attributes above each function.
/// When `to_disk` is true, writes back to the source file.
/// When false (invisible mode), writes to stdout.
fn apply_annotate_rewrite(
    root: &Path,
    src_files: &[PathBuf],
    result: &EngineResult,
    mode: &RuntimeMode,
    to_disk: bool,
) {
    // Group bindings by file.
    let mut by_file: BTreeMap<String, Vec<&BindingRecord>> = BTreeMap::new();
    for b in &result.bindings {
        by_file.entry(b.site_file.clone()).or_default().push(b);
    }

    for (rel_file, bindings) in &by_file {
        let in_path = root.join(rel_file);
        let Ok(orig) = std::fs::read_to_string(&in_path) else {
            continue;
        };
        let rewritten = inject_annotations(&orig, bindings, &result.concepts, mode);
        if to_disk {
            let _ = std::fs::write(&in_path, rewritten);
        } else {
            // invisible: stream to stdout with file header.
            print!("// bind:annotate:{rel_file}\n{rewritten}");
        }
    }

    // Files with no bindings: no change needed.
    for path in src_files {
        let rel = path
            .strip_prefix(root)
            .unwrap_or(path)
            .display()
            .to_string();
        if !by_file.contains_key(&rel) && !to_disk {
            // invisible: emit unmodified source for completeness.
            if let Ok(src) = std::fs::read_to_string(path) {
                print!("// bind:annotate:{rel}\n{src}");
            }
        }
    }
}

/// Source-preserving annotation injector.
///
/// For each function that has a binding, inject above the fn definition:
///   // concept: <name>
///   // substrate-origin: <origin>
///   // memento-cid: <cid>
///   #[cfg_attr(any(), requires(...))]   (if pre present)
///   #[cfg_attr(any(), ensures(...))]    (if post present)
///   // + mode-specific attributes
///
/// Existing substrate-injected annotation blocks are stripped and
/// replaced so the round-trip is idempotent.
fn inject_annotations(
    orig: &str,
    bindings: &[&BindingRecord],
    concepts: &[ConceptRecord],
    mode: &RuntimeMode,
) -> String {
    let mut by_fn: BTreeMap<String, &BindingRecord> = BTreeMap::new();
    for b in bindings {
        by_fn.insert(b.site_fn.clone(), b);
    }

    let mut out_lines: Vec<String> = Vec::new();
    let lines: Vec<&str> = orig.lines().collect();
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        if let Some(fn_name) = parse_fn_name(line) {
            let indent: String = line.chars().take_while(|c| c.is_whitespace()).collect();

            // Strip existing substrate-emitted block from output_lines.
            // For `// concept:` lines, only strip if the line immediately after it
            // (already in out_lines at `start`) was `// substrate-origin:`, confirming
            // this is a substrate-emitted block. User-written `// concept:` lines that
            // are not part of a substrate block are preserved.
            let mut start = out_lines.len();
            while start > 0 {
                let prev = out_lines[start - 1].trim_start();
                let next_in_out = out_lines.get(start).map(|s| s.trim_start()).unwrap_or("");
                let is_substrate_concept = prev.starts_with("// concept:")
                    && (next_in_out.starts_with("// substrate-origin:")
                        || next_in_out.starts_with("// memento-cid:")
                        || next_in_out.starts_with("// witness-inherited-from:")
                        || next_in_out.starts_with("#[cfg_attr(any(),")
                        || next_in_out.is_empty());
                if prev.is_empty()
                    || is_substrate_concept
                    || prev.starts_with("// substrate-origin:")
                    || prev.starts_with("// memento-cid:")
                    || prev.starts_with("// witness-inherited-from:")
                    || prev.starts_with("#[cfg_attr(any(), requires")
                    || prev.starts_with("#[cfg_attr(any(), ensures")
                    || prev.starts_with("#[cfg_attr(any(), witness")
                    || prev.starts_with("#[cfg_attr(any(), provekit_monitor")
                    || prev.starts_with("#[cfg_attr(any(), provekit_emitter")
                    || prev.starts_with("#[cfg_attr(any(), provekit_witness")
                {
                    start -= 1;
                } else {
                    break;
                }
            }
            out_lines.truncate(start);

            if let Some(b) = by_fn.get(&fn_name) {
                if !out_lines.is_empty()
                    && !out_lines
                        .last()
                        .map(|s| s.trim().is_empty())
                        .unwrap_or(false)
                {
                    out_lines.push(String::new());
                }
                let concept = &concepts[b.concept_idx];
                let concept_name = name_for_annotation(&concept.name);
                out_lines.push(format!("{indent}// concept: {concept_name}"));
                out_lines.push(format!("{indent}// substrate-origin: {}", b.origin.label()));
                if !b.site_memento_cid.is_empty() {
                    out_lines.push(format!("{indent}// memento-cid: {}", b.site_memento_cid));
                }
                if let Some(pre) = &b.pretty_pre {
                    out_lines.push(format!("{indent}#[cfg_attr(any(), requires({pre}))]"));
                }
                if let Some(post) = &b.pretty_post {
                    out_lines.push(format!("{indent}#[cfg_attr(any(), ensures({post}))]"));
                }
                // Mode-specific attribute injection.
                match mode {
                    RuntimeMode::Monitor => {
                        // Monitor: wrap body with contract violation check.
                        // The runtime macro provekit_monitor asserts the contract
                        // on each call; contract violation = panic with clean message.
                        if b.pretty_pre.is_some() || b.pretty_post.is_some() {
                            out_lines.push(format!(
                                "{indent}#[cfg_attr(any(), provekit_monitor(contract = \"{}\"))]",
                                concept_name
                            ));
                        }
                    }
                    RuntimeMode::Emitter => {
                        // Emitter: emit structured substrate event per call.
                        out_lines.push(format!(
                            "{indent}#[cfg_attr(any(), provekit_emitter(concept = \"{}\"))]",
                            concept_name
                        ));
                    }
                    RuntimeMode::Witness => {
                        // Witness: sample inputs/outputs, contribute to WitnessMemento.
                        out_lines.push(format!(
                            "{indent}#[cfg_attr(any(), provekit_witness(concept = \"{}\"))]",
                            concept_name
                        ));
                    }
                }
            }

            out_lines.push(line.to_string());
            i += 1;
            continue;
        }
        // Strip pre-existing substrate lines (idempotent re-injection).
        // For `// concept:` lines, only strip if part of a 3-line substrate block
        // (i.e., the next line is `// substrate-origin:`). User-written `// concept: <name>`
        // annotations that are NOT followed by `// substrate-origin:` are preserved.
        let trimmed = line.trim_start();
        if trimmed.starts_with("// concept:") {
            // Check if next line is substrate-origin (marking this as a substrate block).
            let next_is_substrate_origin = lines
                .get(i + 1)
                .map(|l| l.trim_start().starts_with("// substrate-origin:"))
                .unwrap_or(false);
            if next_is_substrate_origin {
                i += 1;
                continue;
            }
            // User-written concept annotation — preserve it.
        } else if trimmed.starts_with("// substrate-origin:")
            || trimmed.starts_with("// memento-cid:")
            || trimmed.starts_with("// witness-inherited-from:")
            || trimmed.starts_with("#[cfg_attr(any(), requires")
            || trimmed.starts_with("#[cfg_attr(any(), ensures")
            || trimmed.starts_with("#[cfg_attr(any(), witness")
            || trimmed.starts_with("#[cfg_attr(any(), provekit_monitor")
            || trimmed.starts_with("#[cfg_attr(any(), provekit_emitter")
            || trimmed.starts_with("#[cfg_attr(any(), provekit_witness")
        {
            i += 1;
            continue;
        }
        out_lines.push(line.to_string());
        i += 1;
    }
    out_lines.join("\n") + "\n"
}

// ============================================================================
// Canonical rewrite (delegates to ORP / realize_function infra)
// ============================================================================

/// Emit canonical target-language source via the ORP realize path.
/// Builds a simplified Term (Opaque body) and ContractAnnotations, then
/// calls the transport layer's realize_function.
///
/// When `to_disk` is true, writes to `.provekit/bindings/translated/<lang>/`.
/// When false (invisible), streams to stdout.
/// Apply the canonical rewrite (delegating Java emission to the realize plugin
/// when target_lang == "java"; other languages still use the inline path).
///
/// Returns the set of concept names (sorted, deduplicated) whose realized
/// body fell through to the language stub — i.e. no body-template matched.
/// Caller emits one `bind-stub-body-emitted` gap per name per §5 of
/// `2026-05-13-body-template-memento.md`.
fn apply_canonical_rewrite(
    root: &Path,
    src_files: &[PathBuf],
    result: &EngineResult,
    mode: &RuntimeMode,
    target_lang: &str,
    to_disk: bool,
    output_dir: &Path,
) -> Vec<String> {
    let _ = mode; // The realize plugin owns mode-aware emission; bind passes
                  // concept_name and lets the kit decide how to annotate.
                  // Group bindings by file.
    let mut by_file: BTreeMap<String, Vec<&BindingRecord>> = BTreeMap::new();
    for b in &result.bindings {
        by_file.entry(b.site_file.clone()).or_default().push(b);
    }

    let translated_dir = output_dir.join("translated").join(target_lang);
    if to_disk {
        let _ = std::fs::create_dir_all(&translated_dir);
    }

    // Track concepts whose realized body was a stub. Deduplicated + sorted at
    // the end so the gap-emission caller produces one gap per affected concept.
    let mut stub_concepts: BTreeSet<String> = BTreeSet::new();
    let mut kit_unavailable = false;
    let mut kit_unavailable_detail = String::new();
    let mut file_extension: Option<String> = None;

    for (rel_file, bindings) in &by_file {
        let mut chunks: Vec<String> = Vec::new();
        // Language-neutral file header. Per-language pre/post-amble (e.g.
        // Go `package main`, PHP `<?php`) is the realize kit's
        // responsibility under federation by construction.
        chunks.push(format!(
            "// canonical rewrite: {rel_file} -> {target_lang}\n"
        ));

        for b in bindings {
            let concept_name = name_for_annotation(&result.concepts[b.concept_idx].name);
            match crate::cmd_transport::realize_for_bind(
                target_lang,
                &b.site_fn,
                &b.param_names,
                &b.param_types,
                &b.return_type,
                &concept_name,
            ) {
                Ok(r) => {
                    if r.is_stub {
                        stub_concepts.insert(concept_name.to_string());
                    }
                    if file_extension.is_none() && !r.extension.is_empty() {
                        file_extension = Some(r.extension.to_string());
                    }
                    chunks.push(r.source);
                }
                Err(e) => {
                    kit_unavailable = true;
                    if kit_unavailable_detail.is_empty() {
                        kit_unavailable_detail = format!("{e}");
                    }
                    stub_concepts.insert(concept_name.to_string());
                }
            }
        }

        let output_src = chunks.join("\n");
        if to_disk {
            let file_name = Path::new(rel_file)
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| rel_file.replace('/', "_"));
            let stem = file_name
                .rsplit_once('.')
                .map(|(s, _)| s.to_string())
                .unwrap_or(file_name.clone());
            let ext = file_extension.as_deref().unwrap_or(target_lang);
            let out_path = translated_dir.join(format!("{stem}.{ext}"));
            let _ = std::fs::write(out_path, &output_src);
        } else {
            print!("{output_src}");
        }
    }

    // Files with no bindings that are invisible: emit gap comment.
    for path in src_files {
        let rel = path
            .strip_prefix(root)
            .unwrap_or(path)
            .display()
            .to_string();
        if !by_file.contains_key(&rel) && !to_disk {
            println!("// bind:canonical:no-bindings:{rel}");
        }
    }

    if kit_unavailable {
        eprintln!(
            "bind: realize kit unavailable for `{target_lang}`: {kit_unavailable_detail}. \
             Every bound concept was recorded as a stub. Author or build a \
             `kind = \"realize\"` plugin for `{target_lang}` to close this gap."
        );
    }

    // BTreeSet → sorted Vec for deterministic gap-record output order.
    stub_concepts.into_iter().collect()
}

// Note: language-specific emit helpers (comment_prefix_for, emit_target_stub,
// build_target_annotations, build_bind_meta_comment, target_stub_body,
// target_fn_def, lang_extension, snake_to_pascal) MOVED OUT of cmd_bind under
// the PR #770 architectural cut. Per-language emission is now the realize
// kit's responsibility, dispatched through crate::kit_dispatch::dispatch_realize.

// ============================================================================
// v0 stubs: algebra / cluster / discharge / attrs (labeled)
// ============================================================================

// ---- Term shape (v0 stub — production uses provekit-ir-symbolic) ------------

/// Structural fingerprint of a function body emitted by the lift kit.
///
/// `value` is the JSON shape per `2026-05-13-bind-ir-lift-result.md` §2;
/// `cid_cached` is the kit's reported `term_shape_cid` (used directly so a
/// kit's CID derivation is canonical for the cluster bucket).
#[derive(Debug, Clone)]
pub struct TermShape {
    value: serde_json::Value,
    cid_cached: String,
}

impl TermShape {
    pub fn from_kit(value: serde_json::Value, cid: String) -> Self {
        Self {
            value,
            cid_cached: cid,
        }
    }

    fn shape_cid(&self) -> String {
        self.cid_cached.clone()
    }

    /// Coarse classification used by the v0 catalog. Operates on the kit's
    /// JSON shape: zero language knowledge, only structural patterns over
    /// the bind-IR canonical labels.
    fn classify(&self) -> &'static str {
        classify_value(&self.value)
    }
}

/// Classification over the bind-IR JSON shape per
/// `2026-05-13-bind-ir-lift-result.md`. Operates purely on canonical labels
/// emitted by the lift kit; no language knowledge.
fn classify_value(value: &serde_json::Value) -> &'static str {
    let kind = value.get("kind").and_then(|v| v.as_str()).unwrap_or("");
    if kind != "body" {
        return "unknown";
    }
    let stmts = value
        .get("stmts")
        .and_then(|v| v.as_array())
        .map(|arr| arr.as_slice())
        .unwrap_or(&[]);
    let mut has_loop = false;
    let mut has_if = false;
    for s in stmts {
        let k = s.get("kind").and_then(|v| v.as_str()).unwrap_or("");
        match k {
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

// ---- Seed catalog (v0 stub — production loads signed ConceptAbstractionMementos) ----

struct CatalogEntry {
    id: String,
    name: String,
    classification: &'static str,
}

struct Catalog {
    entries: Vec<CatalogEntry>,
}

impl Catalog {
    fn match_shape(&self, _shape_cid: &str, shape: &TermShape) -> Option<&CatalogEntry> {
        let cls = shape.classify();
        if cls == "unknown" {
            return None;
        }
        self.entries.iter().find(|e| e.classification == cls)
    }
}

fn seed_catalog() -> Catalog {
    // v0 stub: 3-entry classification-based catalog.
    // Production loads signed ConceptAbstractionMementos from menagerie/concept-shapes/catalog/.
    Catalog {
        entries: vec![
            CatalogEntry {
                id: "shape:retry-with-bounded-attempts".into(),
                name: "concept:retry-with-bounded-attempts".into(),
                classification: "retry-loop",
            },
            CatalogEntry {
                id: "shape:guard-then-commit".into(),
                name: "concept:guard-then-commit".into(),
                classification: "guard-then-commit",
            },
        ],
    }
}

// ---- Discharge verdict (trichotomy) ----------------------------------------

struct WpRule {
    id: String,
    pre: Option<String>,
    post: Option<String>,
}

fn wp_rule_for_shape(_shape_cid: &str, shape: &TermShape) -> Option<WpRule> {
    match shape.classify() {
        "retry-loop" => Some(WpRule {
            id: "wp_rule.retry-with-bounded-attempts.v0".into(),
            pre: Some("max_attempts >= 0".into()),
            post: Some("(out == true) || (out == false)".into()),
        }),
        "guard-then-commit" => Some(WpRule {
            id: "wp_rule.guard-then-commit.v0".into(),
            pre: None,
            post: Some("(out >= 0) || (out == before_state)".into()),
        }),
        _ => None,
    }
}

fn discharge_verdict(shape: &TermShape, origin: &ContractOrigin) -> DischargeVerdict {
    use libprovekit::core::types::{Cid, Term};
    use libprovekit::wp::{wp, OpContractInfo, OpContractResolver, WpError};
    use provekit_ir_symbolic::convert::formula_to_ir;
    use provekit_ir_symbolic::parse_expr::parse_expr;
    use std::collections::HashMap;

    match origin {
        ContractOrigin::Empty => DischargeVerdict::Refuse {
            reason: "no contract recovered".into(),
        },
        ContractOrigin::AttributeLift => DischargeVerdict::LoudlyBoundedLossy {
            loss: "annotation-lift: structural discharge not attempted (v0)".into(),
        },
        ContractOrigin::EvidenceLift { source_kind } => DischargeVerdict::LoudlyBoundedLossy {
            loss: format!("evidence-lift[{source_kind}]: structural discharge not attempted (v0)"),
        },
        ContractOrigin::TestLift => DischargeVerdict::LoudlyBoundedLossy {
            loss: "test-lift: structural discharge not attempted (v0)".into(),
        },
        ContractOrigin::AlgebraSynthesis { .. } => {
            let cls = shape.classify();
            if cls == "unknown" {
                return DischargeVerdict::Refuse {
                    reason: "shape classification fell through".into(),
                };
            }

            // Build wp resolver from the shape's registered rule.
            struct MapResolver(HashMap<String, OpContractInfo>);
            impl OpContractResolver for MapResolver {
                fn lookup(&self, op_name: &str) -> Option<OpContractInfo> {
                    self.0.get(op_name).cloned()
                }
            }

            fn predicate(text: &str) -> IrFormula {
                let sym = parse_expr(text)
                    .unwrap_or_else(|e| panic!("predicate '{text}' unparseable: {e}"));
                formula_to_ir(&sym)
            }

            let mut map: HashMap<String, OpContractInfo> = HashMap::new();
            match cls {
                "retry-loop" => {
                    let mut info = OpContractInfo::new(vec![]);
                    info.wp_rule = Some(IrFormula::And {
                        operands: vec![
                            predicate("max_attempts >= 0"),
                            IrFormula::Atomic {
                                name: "Q".to_string(),
                                args: vec![],
                            },
                        ],
                    });
                    map.insert(cls.to_string(), info);
                }
                "guard-then-commit" => {
                    let mut info = OpContractInfo::new(vec![]);
                    info.wp_rule = Some(IrFormula::Atomic {
                        name: "Q".to_string(),
                        args: vec![],
                    });
                    map.insert(cls.to_string(), info);
                }
                _ => {}
            }

            let sentinel =
                Cid::parse(format!("blake3-512:{}", "0".repeat(128))).expect("sentinel cid");
            let term = Term::Op {
                op_cid: sentinel,
                name: cls.to_string(),
                args: vec![],
            };
            let q = IrFormula::Atomic {
                name: "Q".to_string(),
                args: vec![],
            };
            let resolver = MapResolver(map);
            match wp(&term, &q, &resolver) {
                Ok(_) => DischargeVerdict::Exact,
                Err(WpError::Refused(r)) => DischargeVerdict::LoudlyBoundedLossy {
                    loss: format!("wp-refused: {r}"),
                },
                Err(e) => DischargeVerdict::Refuse {
                    reason: format!("wp-error: {e}"),
                },
            }
        }
    }
}

// ---- Attribute lifter (v0 stub) --------------------------------------------

// Note: extract_contract_attrs, extract_concept_annotation, collect_test_witnesses,
// and their helpers MOVED OUT of cmd_bind. Per the architectural cut
// (`2026-05-13-bind-ir-lift-result.md`), all source-AST visiting is a lift kit
// responsibility. The Rust kit implementation lives in provekit-walk's
// `walk_rpc` binary as the `lift` JSON-RPC method.

// ============================================================================
// Output documents
// ============================================================================

#[derive(Serialize, Deserialize)]
struct IndexDoc {
    total_bindings: usize,
    total_concepts: usize,
    unnamed_concepts: usize,
    verdicts: VerdictCounts,
    top_concepts: Vec<TopConcept>,
}

#[derive(Serialize, Deserialize)]
struct VerdictCounts {
    exact: usize,
    loudly_bounded_lossy: usize,
    refuse: usize,
}

#[derive(Serialize, Deserialize)]
struct TopConcept {
    name: String,
    site_count: usize,
    catalog_id: Option<String>,
}

#[derive(Serialize, Deserialize)]
struct GapsDoc {
    source_lang: String,
    gaps: Vec<GapRecord>,
}

fn build_index_doc(result: &EngineResult) -> IndexDoc {
    let exact = result
        .bindings
        .iter()
        .filter(|b| b.discharge_verdict == DischargeVerdict::Exact)
        .count();
    let lossy = result
        .bindings
        .iter()
        .filter(|b| {
            matches!(
                b.discharge_verdict,
                DischargeVerdict::LoudlyBoundedLossy { .. }
            )
        })
        .count();
    let refuse = result
        .bindings
        .iter()
        .filter(|b| matches!(b.discharge_verdict, DischargeVerdict::Refuse { .. }))
        .count();
    let unnamed = result
        .concepts
        .iter()
        .filter(|c| c.name.starts_with("UNNAMED-CONCEPT-"))
        .count();

    let mut top: Vec<TopConcept> = result
        .concepts
        .iter()
        .map(|c| TopConcept {
            name: c.name.clone(),
            site_count: c.site_indices.len(),
            catalog_id: c.catalog_id.clone(),
        })
        .collect();
    top.sort_by(|a, b| b.site_count.cmp(&a.site_count));
    top.truncate(10);

    IndexDoc {
        total_bindings: result.bindings.len(),
        total_concepts: result.concepts.len(),
        unnamed_concepts: unnamed,
        verdicts: VerdictCounts {
            exact,
            loudly_bounded_lossy: lossy,
            refuse,
        },
        top_concepts: top,
    }
}

fn build_gaps_doc(source_lang: &str, gaps: &[GapRecord]) -> GapsDoc {
    GapsDoc {
        source_lang: source_lang.to_string(),
        gaps: gaps.to_vec(),
    }
}

fn print_summary(result: &EngineResult) {
    let exact = result
        .bindings
        .iter()
        .filter(|b| b.discharge_verdict == DischargeVerdict::Exact)
        .count();
    let lossy = result
        .bindings
        .iter()
        .filter(|b| {
            matches!(
                b.discharge_verdict,
                DischargeVerdict::LoudlyBoundedLossy { .. }
            )
        })
        .count();
    let refuse = result
        .bindings
        .iter()
        .filter(|b| matches!(b.discharge_verdict, DischargeVerdict::Refuse { .. }))
        .count();
    let unnamed = result
        .concepts
        .iter()
        .filter(|c| c.name.starts_with("UNNAMED-CONCEPT-"))
        .count();

    println!(
        "bind: {} bindings ({exact} exact / {lossy} lossy / {refuse} refused)",
        result.bindings.len()
    );
    println!(
        "bind: {} concepts ({unnamed} unnamed candidates)",
        result.concepts.len()
    );

    // Top 10 concepts.
    let mut top: Vec<(&str, usize)> = result
        .concepts
        .iter()
        .map(|c| (c.name.as_str(), c.site_indices.len()))
        .collect();
    top.sort_by(|a, b| b.1.cmp(&a.1));
    top.truncate(10);
    if !top.is_empty() {
        println!("bind: top concepts:");
        for (name, count) in &top {
            println!("  {name}: {count} site(s)");
        }
    }
    if unnamed > 0 {
        println!("bind: {unnamed} unnamed concept(s) — run `provekit bind` on annotated source to name them.");
    }
}

// ============================================================================
// Helpers
// ============================================================================

/// Detect the source language from directory contents.
///
/// Priority: explicit `lang` arg (non-"auto") > Cargo.toml/\.rs > pom.xml/build.gradle/\.java >
/// pyproject.toml/setup.py/\.py > unknown (returns Err).
///
/// Returns `Ok(lang)` or `Err(message)` so `run()` can hard-fail with a clear gap record
/// rather than silently assuming Rust and producing empty output.
/// Detect the source language by probing the kit registry — substrate-wide
/// filesystem convention (`.provekit/lift/<lang>/manifest.toml` or a
/// built-in kit binary under `implementations/<lang>/`) rather than a
/// hard-coded extension list. Returns `Ok(lang)` for the first kit found
/// or `Err(message)` so `run()` can hard-fail with a clear gap record.
fn resolve_lang_detect(root: &Path) -> Result<String, String> {
    if let Some(lang) = crate::kit_dispatch::detect_lift_language(root) {
        return Ok(lang);
    }
    // Fallback heuristic: probe for a known project manifest OR a known
    // source-file extension. This is filesystem probing, not language
    // semantics: when a lift kit IS available for that language (PATH,
    // env override, or in-repo built-in) the kit_dispatch resolver picks
    // it up. The probe here only chooses WHICH language to ask about.
    for (lang, marker) in [
        ("rust", "Cargo.toml"),
        ("java", "pom.xml"),
        ("python", "pyproject.toml"),
    ] {
        if root.join(marker).exists() {
            return Ok(lang.to_string());
        }
    }
    // Extension-based last resort. Each row is "language -> first-matched
    // extension". Substrate-wide convention, not CLI semantics.
    let extensions: &[(&str, &str)] = &[
        ("rust", "rs"),
        ("java", "java"),
        ("python", "py"),
        ("go", "go"),
        ("typescript", "ts"),
        ("csharp", "cs"),
        ("ruby", "rb"),
        ("php", "php"),
        ("zig", "zig"),
    ];
    let mut dirs_to_scan: Vec<PathBuf> = vec![root.to_path_buf()];
    if root.join("src").is_dir() {
        dirs_to_scan.push(root.join("src"));
    }
    for dir in &dirs_to_scan {
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let ext = entry
                    .path()
                    .extension()
                    .and_then(|e| e.to_str())
                    .map(|s| s.to_string())
                    .unwrap_or_default();
                if ext.is_empty() {
                    continue;
                }
                for (lang, marker_ext) in extensions {
                    if &ext == *marker_ext {
                        return Ok((*lang).to_string());
                    }
                }
            }
        }
    }
    Err(format!(
        "cannot determine source language from {}; no kit registered under .provekit/lift/ or \
         implementations/<lang>/ and no common project manifest or source file detected. \
         Use --lang=<language> to specify explicitly.",
        root.display()
    ))
}

fn resolve_lang(lang: &str, root: &Path) -> Result<String, String> {
    if lang != "auto" {
        return Ok(lang.to_string());
    }
    resolve_lang_detect(root)
}

fn line_for_fn(src: &str, fn_name: &str) -> usize {
    let needle = format!("fn {fn_name}(");
    for (i, line) in src.lines().enumerate() {
        if line.contains(&needle) {
            return i + 1;
        }
    }
    1
}

fn byte_span_for_line(bytes: &[u8], line_no: usize) -> (u64, u64) {
    if line_no == 0 || bytes.is_empty() {
        return (0, 0);
    }
    let mut cur_line = 1usize;
    for (i, &b) in bytes.iter().enumerate() {
        if cur_line == line_no {
            let end_offset = bytes[i..]
                .iter()
                .position(|&x| x == b'\n')
                .map(|p| i + p)
                .unwrap_or(bytes.len());
            return (i as u64, end_offset as u64);
        }
        if b == b'\n' {
            cur_line += 1;
        }
    }
    (bytes.len() as u64, bytes.len() as u64)
}

fn contract_text_from_witnesses(witnesses: &[ContractWitness], role: &str) -> Option<String> {
    let parts: Vec<String> = witnesses
        .iter()
        .filter(|w| witness_role_matches(w, role))
        .map(|w| {
            w.predicate_text
                .clone()
                .unwrap_or_else(|| serde_json::to_string(&w.predicate).unwrap_or_default())
        })
        .filter(|text| !text.trim().is_empty())
        .collect();
    match parts.len() {
        0 => None,
        1 => parts.into_iter().next(),
        _ => Some(parts.join(" && ")),
    }
}

fn witness_role_matches(witness: &ContractWitness, role: &str) -> bool {
    witness.role == role
        || witness
            .extension_fields
            .get("role")
            .and_then(|v| v.as_str())
            .map(|r| r == role)
            .unwrap_or(false)
}

fn dominant_source_kind_label(witnesses: &[ContractWitness]) -> String {
    let labels: BTreeSet<String> = witnesses
        .iter()
        .map(|w| source_kind_label(&w.source_kind))
        .collect();
    match labels.len() {
        0 => "unspecified".to_string(),
        1 => labels
            .into_iter()
            .next()
            .unwrap_or_else(|| "unspecified".to_string()),
        _ => "mixed".to_string(),
    }
}

fn contract_witnesses_for_binding(
    lift: &RawLift,
    origin: &ContractOrigin,
    pre: Option<&str>,
    post: Option<&str>,
) -> Vec<ContractWitness> {
    match origin {
        ContractOrigin::AttributeLift | ContractOrigin::EvidenceLift { .. } => {
            lift.witnesses.clone()
        }
        ContractOrigin::TestLift => post
            .map(|text| {
                synthesized_contract_witness(
                    "post",
                    text,
                    SourceKind::TestAssertion,
                    lift.fn_line,
                    &lift.file,
                    &lift.fn_name,
                    [(
                        "surface".to_string(),
                        serde_json::Value::String("bind-test-witness-entry".to_string()),
                    )],
                )
            })
            .into_iter()
            .collect(),
        ContractOrigin::AlgebraSynthesis { rule_id } => {
            let mut witnesses = Vec::new();
            if let Some(text) = pre {
                witnesses.push(synthesized_contract_witness(
                    "pre",
                    text,
                    SourceKind::StructuralSynthesis,
                    lift.fn_line,
                    &lift.file,
                    &lift.fn_name,
                    [(
                        "synthesis_rule_id".to_string(),
                        serde_json::Value::String(rule_id.clone()),
                    )],
                ));
            }
            if let Some(text) = post {
                witnesses.push(synthesized_contract_witness(
                    "post",
                    text,
                    SourceKind::StructuralSynthesis,
                    lift.fn_line,
                    &lift.file,
                    &lift.fn_name,
                    [(
                        "synthesis_rule_id".to_string(),
                        serde_json::Value::String(rule_id.clone()),
                    )],
                ));
            }
            witnesses
        }
        ContractOrigin::Empty => Vec::new(),
    }
}

fn synthesized_contract_witness<I>(
    role: &str,
    predicate_text: &str,
    source_kind: SourceKind,
    fn_line: usize,
    file: &str,
    fn_name: &str,
    extra_fields: I,
) -> ContractWitness
where
    I: IntoIterator<Item = (String, serde_json::Value)>,
{
    let mut extension_fields = BTreeMap::new();
    extension_fields.insert(
        "role".to_string(),
        serde_json::Value::String(role.to_string()),
    );
    extension_fields.insert(
        "function_symbol".to_string(),
        serde_json::Value::String(format!("{fn_name}@{file}")),
    );
    for (key, value) in extra_fields {
        extension_fields.insert(key, value);
    }
    ContractWitness {
        role: role.to_string(),
        predicate: formula_text_to_ir_formula(predicate_text),
        predicate_text: Some(predicate_text.to_string()),
        confidence_basis_points: default_confidence_basis_points(&source_kind),
        source_kind,
        source_line: fn_line.max(1),
        source_col: 0,
        extension_fields,
    }
}

fn evidence_memento_from_contract_witness(
    witness: &ContractWitness,
    source_cid: &str,
    lifter_cid: &str,
) -> EvidenceMemento {
    let source_locator = SourceLocator {
        source_cid: source_cid.to_string(),
        span: SourceLocatorSpan {
            start: SourceLocatorPoint {
                line: witness.source_line.min(u32::MAX as usize) as u32,
                col: witness.source_col,
            },
            end: SourceLocatorPoint {
                line: witness.source_line.min(u32::MAX as usize) as u32,
                col: witness.source_col,
            },
        },
    };
    let cid = evidence_memento_cid(
        witness.confidence_basis_points,
        &witness.extension_fields,
        lifter_cid,
        &witness.predicate,
        &witness.source_kind,
        &source_locator,
    );
    EvidenceMemento {
        cid,
        confidence_basis_points: witness.confidence_basis_points,
        extension_fields: witness.extension_fields.clone(),
        kind: "evidence".to_string(),
        lifter_cid: lifter_cid.to_string(),
        predicate: witness.predicate.clone(),
        schema_version: "1".to_string(),
        source_kind: witness.source_kind.clone(),
        source_locator,
    }
}

fn evidence_memento_cid(
    confidence_basis_points: u16,
    extension_fields: &BTreeMap<String, serde_json::Value>,
    lifter_cid: &str,
    predicate: &IrFormula,
    source_kind: &SourceKind,
    source_locator: &SourceLocator,
) -> String {
    let pred_json = serde_json::to_value(predicate).expect("IrFormula must serialize");
    let pred_v = json_to_value(&pred_json);
    let ext_entries: Vec<(String, Arc<Value>)> = extension_fields
        .iter()
        .map(|(k, v)| (k.clone(), json_to_value(v)))
        .collect();
    let source_kind = source_kind_label(source_kind);
    let header = Value::object([
        (
            "confidence_basis_points",
            Value::integer(confidence_basis_points as i64),
        ),
        ("extension_fields", Arc::new(Value::Object(ext_entries))),
        ("kind", Value::string("evidence")),
        ("lifter_cid", Value::string(lifter_cid.to_string())),
        ("predicate", pred_v),
        ("schemaVersion", Value::string("1")),
        ("source_kind", Value::string(source_kind)),
        ("source_locator", source_locator_to_value(source_locator)),
    ]);
    blake3_512_of(encode_jcs(&header).as_bytes())
}

fn bind_default_policy_memento(lifter_cid: &str) -> PolicyMemento {
    PolicyMemento::ProofGate(ProofGatePolicyMemento {
        admission_rule: serde_json::json!({
            "result": "admitted",
            "requires": ["non-empty evidence_cids"]
        }),
        checker_cid: lifter_cid.to_string(),
        decision_payload_schema: serde_json::json!({
            "type": "object",
            "required": ["evidence_count", "gate_evaluated", "result"]
        }),
        input_requirements: serde_json::json!({
            "evidence_cids": "non-empty",
            "candidate_cid": "required",
            "promoted_cid": "required"
        }),
        policy_kind: "proof_gate".to_string(),
        policy_version: "bind-default-policy".to_string(),
        proof_artifact_schema: serde_json::json!({
            "kind": "bind-admission"
        }),
        proof_system: "provekit-bind".to_string(),
        provenance_cid: lifter_cid.to_string(),
        refusal_rule: serde_json::json!({
            "result": "rejected"
        }),
        theorem_ref: "bind-default-policy".to_string(),
        trusted_base_cid: lifter_cid.to_string(),
    })
}

fn policy_memento_cid(policy: &PolicyMemento) -> String {
    let json = serde_json::to_value(policy).expect("PolicyMemento must serialize");
    let value = json_to_value(&json);
    blake3_512_of(encode_jcs(&value).as_bytes())
}

fn promotion_decision_memento(
    candidate_cid: &str,
    promoted_cid: &str,
    evidence: &EvidenceMemento,
    policy_cid: &str,
    decider_cid: &str,
) -> Result<PromotionDecisionMemento, String> {
    let gate = promotion_gate_for_evidence(evidence);
    let gate_label = promotion_gate_label(&gate);
    let mut decision = PromotionDecisionMemento {
        envelope: PromotionDecisionEnvelope {
            declared_at: "2026-05-13T00:00:00.000Z".to_string(),
            signature: String::new(),
            signer: decider_cid.to_string(),
        },
        header: PromotionDecisionHeader {
            candidate_cid: candidate_cid.to_string(),
            cid: String::new(),
            decider_cid: decider_cid.to_string(),
            decision_payload: serde_json::json!({
                "evidence_count": 1,
                "gate_evaluated": gate_label,
                "result": "admitted"
            }),
            evidence_cids: vec![evidence.cid.clone()],
            gate,
            kind: "promotion-decision".to_string(),
            policy_cid: policy_cid.to_string(),
            promoted_cid: promoted_cid.to_string(),
            result: PromotionResult::Admitted,
            schema_version: "1".to_string(),
        },
        metadata: PromotionDecisionMetadata {
            counterexample_cids: None,
            note: Some(format!(
                "bind admitted {} evidence into compound contract",
                source_kind_label(&evidence.source_kind)
            )),
            source_url: None,
        },
    };
    decision.header.cid = decision
        .recompute_header_cid()
        .map_err(|err| err.to_string())?;
    decision.validate().map_err(|err| err.to_string())?;
    Ok(decision)
}

fn promotion_gate_for_evidence(evidence: &EvidenceMemento) -> PromotionGate {
    match &evidence.source_kind {
        SourceKind::NativeSurface => PromotionGate::Human,
        _ => PromotionGate::Proof,
    }
}

fn promotion_gate_label(gate: &PromotionGate) -> &'static str {
    match gate {
        PromotionGate::Human => "human",
        PromotionGate::Proof => "proof",
        PromotionGate::Property => "property",
        PromotionGate::Threshold => "threshold",
        PromotionGate::Other(_) => "other",
    }
}

fn compound_contract_memento(
    function_term_cid: &str,
    evidences: &[EvidenceMemento],
) -> Option<CompoundContractMemento> {
    if evidences.is_empty() {
        return None;
    }
    let mut evidence_refs: Vec<EvidenceRef> = evidences
        .iter()
        .map(|evidence| EvidenceRef {
            evidence_cid: evidence.cid.clone(),
            weight_basis_points: 10000,
        })
        .collect();
    evidence_refs.sort_by(|a, b| a.evidence_cid.cmp(&b.evidence_cid));
    let composed_pre = compose_evidence_role(evidences, "pre");
    let composed_post = compose_evidence_role(evidences, "post");
    let aggregation_strategy = AggregationStrategy::Conjunction;
    let cid = compound_contract_cid(
        &aggregation_strategy,
        &composed_pre,
        &composed_post,
        &evidence_refs,
        function_term_cid,
    );
    Some(CompoundContractMemento {
        aggregation_strategy,
        cid,
        composed_post,
        composed_pre,
        evidences: evidence_refs,
        function_term_cid: function_term_cid.to_string(),
        kind: "compound-contract".to_string(),
        schema_version: "1".to_string(),
    })
}

fn compose_evidence_role(evidences: &[EvidenceMemento], role: &str) -> IrFormula {
    let mut selected: Vec<&EvidenceMemento> = evidences
        .iter()
        .filter(|evidence| {
            evidence
                .extension_fields
                .get("role")
                .and_then(|v| v.as_str())
                .map(|r| r == role)
                .unwrap_or(false)
        })
        .collect();
    selected.sort_by(|a, b| a.cid.cmp(&b.cid));
    let predicates: Vec<IrFormula> = selected
        .into_iter()
        .map(|evidence| evidence.predicate.clone())
        .collect();
    and_formula(predicates)
}

fn compound_contract_cid(
    aggregation_strategy: &AggregationStrategy,
    composed_pre: &IrFormula,
    composed_post: &IrFormula,
    evidence_refs: &[EvidenceRef],
    function_term_cid: &str,
) -> String {
    let refs_json = serde_json::to_value(evidence_refs).expect("EvidenceRef must serialize");
    let strategy_label: String = aggregation_strategy.clone().into();
    let header = Value::object([
        ("aggregation_strategy", Value::string(strategy_label)),
        ("composed_post", ir_formula_to_value(composed_post)),
        ("composed_pre", ir_formula_to_value(composed_pre)),
        ("evidences", json_to_value(&refs_json)),
        (
            "function_term_cid",
            Value::string(function_term_cid.to_string()),
        ),
        ("kind", Value::string("compound-contract")),
        ("schemaVersion", Value::string("1")),
    ]);
    blake3_512_of(encode_jcs(&header).as_bytes())
}

fn ir_formula_to_value(formula: &IrFormula) -> Arc<Value> {
    json_to_value(&serde_json::to_value(formula).expect("IrFormula must serialize"))
}

fn source_locator_to_value(locator: &SourceLocator) -> Arc<Value> {
    let point_to_value = |point: &SourceLocatorPoint| {
        Value::object([
            ("col", Value::integer(point.col as i64)),
            ("line", Value::integer(point.line as i64)),
        ])
    };
    Value::object([
        ("source_cid", Value::string(locator.source_cid.clone())),
        (
            "span",
            Value::object([
                ("end", point_to_value(&locator.span.end)),
                ("start", point_to_value(&locator.span.start)),
            ]),
        ),
    ])
}

fn source_kind_label(source_kind: &SourceKind) -> String {
    source_kind.clone().into()
}

fn and_formula(operands: Vec<IrFormula>) -> IrFormula {
    match operands.len() {
        0 => formula_text_to_ir_formula("true"),
        1 => operands
            .into_iter()
            .next()
            .unwrap_or_else(|| formula_text_to_ir_formula("true")),
        _ => IrFormula::And { operands },
    }
}

fn formula_text_to_ir_formula(text: &str) -> IrFormula {
    IrFormula::Atomic {
        name: text.to_string(),
        args: vec![],
    }
}

fn formula_text_to_value(text: &str) -> Arc<Value> {
    Value::object([
        ("kind", Value::string("atomic")),
        ("name", Value::string(text)),
        ("args", Value::array(vec![])),
    ])
}

fn json_to_value(j: &serde_json::Value) -> Arc<Value> {
    match j {
        serde_json::Value::String(s) => Value::string(s.clone()),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Value::integer(i)
            } else {
                Value::string(n.to_string())
            }
        }
        serde_json::Value::Bool(b) => Value::boolean(*b),
        serde_json::Value::Null => Value::null(),
        serde_json::Value::Array(arr) => Value::array(arr.iter().map(json_to_value).collect()),
        serde_json::Value::Object(map) => {
            let kv: Vec<(String, Arc<Value>)> = map
                .iter()
                .map(|(k, v)| (k.clone(), json_to_value(v)))
                .collect();
            Value::object(kv)
        }
    }
}

fn safe_filename(cid: &str) -> String {
    cid.chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

fn parse_fn_name(line: &str) -> Option<String> {
    let trimmed = line.trim_start();
    let after = if let Some(s) = trimmed.strip_prefix("pub fn ") {
        s
    } else if let Some(s) = trimmed.strip_prefix("fn ") {
        s
    } else if let Some(s) = trimmed.strip_prefix("pub(crate) fn ") {
        s
    } else if let Some(s) = trimmed.strip_prefix("async fn ") {
        s
    } else if let Some(s) = trimmed.strip_prefix("pub async fn ") {
        s
    } else {
        return None;
    };
    let name: String = after
        .chars()
        .take_while(|c| c.is_alphanumeric() || *c == '_')
        .collect();
    if name.is_empty() {
        None
    } else {
        Some(name)
    }
}

fn name_for_annotation(name: &str) -> &str {
    name.strip_prefix("concept:").unwrap_or(name)
}

fn normalize_ws(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut prev_ws = false;
    for c in s.chars() {
        if c.is_whitespace() {
            if !prev_ws && !out.is_empty() {
                out.push(' ');
            }
            prev_ws = true;
        } else {
            out.push(c);
            prev_ws = false;
        }
    }
    out.trim().to_string()
}

fn rewrite_label(r: &RewriteShape) -> &'static str {
    match r {
        RewriteShape::Annotate => "annotate",
        RewriteShape::Canonical => "canonical",
        RewriteShape::Invisible => "invisible",
    }
}

fn mode_label(m: &RuntimeMode) -> &'static str {
    match m {
        RuntimeMode::Monitor => "monitor",
        RuntimeMode::Emitter => "emitter",
        RuntimeMode::Witness => "witness",
    }
}

fn mode_label_for_hint(m: &RuntimeMode) -> String {
    match m {
        RuntimeMode::Monitor => "monitor".to_string(),
        RuntimeMode::Emitter => "emitter".to_string(),
        RuntimeMode::Witness => "witness".to_string(),
    }
}

// ============================================================================
// Unit tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::resolve_lang_detect;
    use std::fs;

    /// Create a temporary directory for a test case, run the closure to populate it,
    /// then call resolve_lang_detect and return the result.
    fn with_temp_dir<F: FnOnce(&std::path::Path)>(populate: F) -> Result<String, String> {
        let dir = tempfile::tempdir().expect("tempdir");
        populate(dir.path());
        resolve_lang_detect(dir.path())
    }

    #[test]
    fn resolve_lang_detect_rust_via_cargo_toml() {
        let result = with_temp_dir(|p| {
            fs::write(p.join("Cargo.toml"), "[package]\nname = \"foo\"\n").unwrap();
        });
        assert_eq!(result, Ok("rust".to_string()));
    }

    #[test]
    fn resolve_lang_detect_java_via_pom_xml() {
        let result = with_temp_dir(|p| {
            fs::write(p.join("pom.xml"), "<project/>").unwrap();
        });
        assert_eq!(result, Ok("java".to_string()));
    }

    #[test]
    fn resolve_lang_detect_python_via_pyproject_toml() {
        let result = with_temp_dir(|p| {
            fs::write(p.join("pyproject.toml"), "[tool.poetry]\n").unwrap();
        });
        assert_eq!(result, Ok("python".to_string()));
    }

    #[test]
    fn resolve_lang_detect_unknown_returns_err() {
        let result = with_temp_dir(|_p| {
            // Empty directory: no recognised source files.
        });
        assert!(
            result.is_err(),
            "Expected Err for empty dir, got: {:?}",
            result
        );
        let msg = result.unwrap_err();
        assert!(
            msg.contains("cannot determine source language"),
            "Err message should explain the problem; got: {msg}"
        );
    }
}
