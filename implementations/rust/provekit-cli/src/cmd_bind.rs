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
//   .provekit/bindings/sites/<cid>.json   — one ConceptSiteMemento per match
//   .provekit/bindings/index.json         — summary map
//   .provekit/bindings/gaps.json          — coverage gaps / deferred capabilities
//   <src-file> (or stdout)                — annotated/canonical/streamed source
//
// Trichotomy applies per-binding (Supra omnia, rectum):
//   exact                  — wp evaluator fired, formula reduced
//   loudly-bounded-lossy   — annotation/test-lift (structural shim)
//   refuse                 — no contract recovered or wp-error

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use chrono;
use clap::Parser;
use serde::{Deserialize, Serialize};

use provekit_canonicalizer::{blake3_512_of, encode_jcs, Value};
use provekit_claim_envelope::{mint_contract, Authoring, MintContractArgs};
use provekit_ir_types::{
    CodeSite, CodeSiteSpan, ConceptSiteMemento, ConceptSiteProvenance, Discharge, IrFormula,
    LossRecord,
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
    let sealed_at = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string();
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

    let root = args.root.canonicalize().unwrap_or_else(|_| args.root.clone());
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
            let gap_doc = build_gaps_doc("unknown", &[GapRecord {
                kind: "source-language-not-supported".into(),
                detail: msg,
            }]);
            let _ = std::fs::write(
                output_dir.join("gaps.json"),
                serde_json::to_string_pretty(&gap_doc).unwrap_or_default(),
            );
            return EXIT_USER_ERROR;
        }
    };

    if source_lang != "rust" {
        eprintln!(
            "bind: v0 supports Rust source only; detected or specified lang '{source_lang}'. \
             Multi-lang dispatch is recorded in gaps.json."
        );
        // Fall through: still create gaps.json so callers know what's missing.
    }

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

    // Collect source files.
    let src_dir = root.join("src");
    let scan_root = if src_dir.is_dir() { &src_dir } else { &root };
    let src_files = collect_rs_files(scan_root);
    let test_files = collect_rs_files(&root.join("tests"));

    if src_files.is_empty() {
        eprintln!("bind: no Rust source files found under {}", scan_root.display());
        // Emit a real gap record so callers know WHY nothing was produced.
        // When source_lang is non-Rust, record the source-language-not-supported gap
        // so composed loss in round-trip tests contains real (not synthetic) evidence.
        let mut early_gaps: Vec<GapRecord> = Vec::new();
        if source_lang != "rust" {
            early_gaps.push(GapRecord {
                kind: "source-language-not-supported".into(),
                detail: format!(
                    "no full lifter for {source_lang} in v0; bind engine is Rust-only. \
                     Round-trip through {source_lang} is loudly-bounded-lossy at this boundary. \
                     Full {source_lang} lifter deferred to v1."
                ),
            });
        }
        let _ = std::fs::create_dir_all(&output_dir);
        let gaps = build_gaps_doc(&source_lang, &early_gaps);
        let _ = std::fs::write(
            output_dir.join("gaps.json"),
            serde_json::to_string_pretty(&gaps).unwrap_or_default(),
        );
        return EXIT_OK;
    }

    // Run the engine.
    let result = match run_bind_engine(
        &root,
        &src_files,
        &test_files,
        &source_lang,
        args.threshold,
        args.quiet,
    ) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("bind: engine error: {e}");
            return EXIT_USER_ERROR;
        }
    };

    // Persist artifacts.
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
    let gaps = build_gaps_doc(&source_lang, &result.gaps);
    let _ = std::fs::write(
        output_dir.join("gaps.json"),
        serde_json::to_string_pretty(&gaps).unwrap_or_default(),
    );

    // Rewrite output.
    match &args.rewrite {
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
            apply_annotate_rewrite(&root, &src_files, &result, &args.mode, /*to_disk=*/ true);
        }
        RewriteShape::Canonical => {
            apply_canonical_rewrite(
                &root,
                &src_files,
                &result,
                &args.mode,
                &target_lang,
                /*to_disk=*/ true,
                &output_dir,
            );
        }
        RewriteShape::Invisible => {
            // Invisible: stream to stdout. Apply annotate-shape for same-language,
            // canonical for cross-language.
            if target_lang == source_lang {
                apply_annotate_rewrite(&root, &src_files, &result, &args.mode, /*to_disk=*/ false);
            } else {
                apply_canonical_rewrite(
                    &root,
                    &src_files,
                    &result,
                    &args.mode,
                    &target_lang,
                    /*to_disk=*/ false,
                    &output_dir,
                );
            }
        }
    }

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
    pub site_mementos: Vec<ConceptSiteMemento>,
    pub gaps: Vec<GapRecord>,
}

#[derive(Debug, Clone)]
pub struct BindingRecord {
    pub site_file: String,
    pub site_fn: String,
    pub site_line: usize,
    pub shape_cid: String,
    pub concept_idx: usize,
    pub contract_cid: Option<String>,
    pub contract_content_cid: Option<String>,
    pub origin: ContractOrigin,
    pub discharge_verdict: DischargeVerdict,
    pub pretty_pre: Option<String>,
    pub pretty_post: Option<String>,
    pub site_memento_cid: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContractOrigin {
    AttributeLift,
    TestLift,
    AlgebraSynthesis { rule_id: String },
    Empty,
}

impl ContractOrigin {
    pub fn label(&self) -> String {
        match self {
            ContractOrigin::AttributeLift => "annotation-lift".into(),
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

impl DischargeVerdict {
    pub fn label(&self) -> String {
        match self {
            DischargeVerdict::Exact => "exact".into(),
            DischargeVerdict::LoudlyBoundedLossy { loss } => {
                format!("loudly-bounded-lossy({})", loss)
            }
            DischargeVerdict::Refuse { reason } => format!("refuse({})", reason),
        }
    }
    pub fn verdict_str(&self) -> &'static str {
        match self {
            DischargeVerdict::Exact => "exact",
            DischargeVerdict::LoudlyBoundedLossy { .. } => "loudly-bounded-lossy",
            DischargeVerdict::Refuse { .. } => "refuse",
        }
    }
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
    concept_annotation: Option<String>,
    term_shape: TermShape,
}

// ---------------------------------------------------------------------------
// Main engine pass
// ---------------------------------------------------------------------------

fn run_bind_engine(
    root: &Path,
    src_files: &[PathBuf],
    test_files: &[PathBuf],
    source_lang: &str,
    threshold: usize,
    quiet: bool,
) -> Result<EngineResult, String> {
    // v0: Rust only. Record gap for anything else.
    let mut gaps: Vec<GapRecord> = Vec::new();
    if source_lang != "rust" {
        gaps.push(GapRecord {
            kind: "v0-lang-gap".into(),
            detail: format!(
                "multi-lang lift_plugin dispatch not yet wired; source_lang={source_lang} \
                 deferred to v1"
            ),
        });
        return Ok(EngineResult {
            bindings: vec![],
            concepts: vec![],
            site_mementos: vec![],
            gaps,
        });
    }

    let signer_seed: Ed25519Seed = [0x42; 32]; // v0: deterministic seed

    // ---- Verb 1: LIFT -------------------------------------------------------
    let mut raw_lifts: Vec<RawLift> = Vec::new();
    for path in src_files {
        let src = std::fs::read_to_string(path)
            .map_err(|e| format!("read {}: {e}", path.display()))?;
        let file = match syn::parse_file(&src) {
            Ok(f) => f,
            Err(e) => {
                if !quiet {
                    eprintln!("bind: parse error in {}: {e}", path.display());
                }
                continue;
            }
        };
        let rel = path
            .strip_prefix(root)
            .unwrap_or(path)
            .display()
            .to_string();
        for item in &file.items {
            if let syn::Item::Fn(item_fn) = item {
                let fn_name = item_fn.sig.ident.to_string();
                let line = line_for_fn(&src, &fn_name);
                let attr_contract = extract_contract_attrs(&item_fn.attrs);
                let concept_comment = extract_concept_annotation(&src, &fn_name);
                let term_shape = TermShape::from_fn(item_fn);
                raw_lifts.push(RawLift {
                    file: rel.clone(),
                    fn_name,
                    fn_line: line,
                    attr_pre: attr_contract.pre,
                    attr_post: attr_contract.post,
                    concept_annotation: concept_comment,
                    term_shape,
                });
            }
        }
    }

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

    // shape -> concept idx lookup
    let mut shape_to_concept: BTreeMap<String, usize> = BTreeMap::new();
    for (ci, c) in concepts.iter().enumerate() {
        shape_to_concept.insert(c.shape_cid.clone(), ci);
        for alias in &c.shape_cid_aliases {
            shape_to_concept.insert(alias.clone(), ci);
        }
    }

    // Collect test witnesses.
    let test_witnesses = collect_test_witnesses(test_files);
    // Map fn_name -> post text.
    let test_post_map: BTreeMap<String, String> = test_witnesses
        .into_iter()
        .map(|(_, fn_name, formula)| (fn_name, formula))
        .collect();

    // ---- Verb 4: SCOPE + Verb 6: IDENTIFY + Verb 7: REALIZE ----------------
    let mut bindings: Vec<BindingRecord> = Vec::new();
    let mut site_mementos: Vec<ConceptSiteMemento> = Vec::new();

    let lifter_cid = blake3_512_of(b"provekit-cli/bind-v0/lifter");
    let clusterer_cid = blake3_512_of(b"provekit-cli/bind-v0/clusterer");
    let discharger_cid = blake3_512_of(b"provekit-cli/bind-v0/discharger");

    for lift in &raw_lifts {
        let shape_cid = lift.term_shape.shape_cid();
        let concept_idx = *shape_to_concept.get(&shape_cid).expect("shape was clustered");

        // Contract origin priority: attribute > test > algebra-synthesis > empty.
        let (origin, pre, post) = if lift.attr_pre.is_some() || lift.attr_post.is_some() {
            (
                ContractOrigin::AttributeLift,
                lift.attr_pre.clone(),
                lift.attr_post.clone(),
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
        let (contract_cid, contract_content_cid) = if pre.is_some() || post.is_some() {
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
        let site_memento_cid = if let Some(local_cid) = &contract_content_cid {
            let source_bytes = std::fs::read(root.join(&lift.file)).unwrap_or_default();
            let source_cid = blake3_512_of(&source_bytes);
            let (span_start, span_end) = byte_span_for_line(&source_bytes, lift.fn_line);
            let fn_term_cid = local_cid.clone();

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
            let loss_json = serde_json::to_string(&discharge.loss_record)
                .expect("LossRecord serialization");
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
            header_kv.push(("local_contract_cid", Value::string(local_cid.clone())));
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
                local_contract_cid: local_cid.clone(),
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
            site_line: lift.fn_line,
            shape_cid,
            concept_idx,
            contract_cid,
            contract_content_cid,
            origin,
            discharge_verdict: verdict,
            pretty_pre: pre,
            pretty_post: post,
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

    // Record v0 capability gaps.
    gaps.push(GapRecord {
        kind: "v0-capability-gap".into(),
        detail: "multi-lang lift_plugin dispatch deferred to v1 (only Rust lifting in v0)".into(),
    });
    gaps.push(GapRecord {
        kind: "v0-capability-gap".into(),
        detail: "real ConceptAbstractionMemento catalog lookup deferred to v1 (v0 uses soft-match classification)".into(),
    });
    // F6: Record stub-body gap when bindings exist.
    //
    // In v0 the canonical-rewrite path has no full term graph; it delegates to
    // `realize_for_bind` which always emits idiomatic stub bodies (panic/raise/todo).
    // This gap record is the honest substrate disclosure: real lifted source bodies
    // are NOT present in these outputs.  A future PR that wires the term graph to the
    // canonical path should remove this gap kind and replace it with a "term-body-realized"
    // kind.
    if !bindings.is_empty() {
        gaps.push(GapRecord {
            kind: "bind-stub-body-emitted".into(),
            detail: format!(
                "canonical-rewrite emitted stub bodies for {n} binding(s): no real lifted term \
                 graph available in v0; bodies are idiomatic language stubs \
                 (panic/raise/todo/throw). Real bodies deferred to v1.",
                n = bindings.len()
            ),
        });
    }
    Ok(EngineResult {
        bindings,
        concepts,
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
        let rel = path.strip_prefix(root).unwrap_or(path).display().to_string();
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
                    && !out_lines.last().map(|s| s.trim().is_empty()).unwrap_or(false)
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
            let next_is_substrate_origin = lines.get(i + 1)
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
fn apply_canonical_rewrite(
    root: &Path,
    src_files: &[PathBuf],
    result: &EngineResult,
    mode: &RuntimeMode,
    target_lang: &str,
    to_disk: bool,
    output_dir: &Path,
) {
    // Group bindings by file.
    let mut by_file: BTreeMap<String, Vec<&BindingRecord>> = BTreeMap::new();
    for b in &result.bindings {
        by_file.entry(b.site_file.clone()).or_default().push(b);
    }

    let translated_dir = output_dir.join("translated").join(target_lang);
    if to_disk {
        let _ = std::fs::create_dir_all(&translated_dir);
    }

    for (rel_file, bindings) in &by_file {
        let in_path = root.join(rel_file);
        let Ok(orig) = std::fs::read_to_string(&in_path) else {
            continue;
        };

        // Parse functions from source; for each bound function, emit a
        // realized target-language snippet. Unbound functions are skipped
        // (coverage gap already recorded).
        let file = match syn::parse_file(&orig) {
            Ok(f) => f,
            Err(_) => continue,
        };

        let mut by_fn: BTreeMap<String, &BindingRecord> = BTreeMap::new();
        for b in bindings {
            by_fn.insert(b.site_fn.clone(), b);
        }

        // F2: use target-language comment prefix (not always `//`).
        let cmt = comment_prefix_for(target_lang);

        // F1: emit file-level header exactly once per output file.
        // Go needs `package main`, PHP needs `<?php`, others need nothing.
        let file_header = crate::cmd_transport::realize_file_header(target_lang);

        let mut chunks: Vec<String> = Vec::new();
        // File-level header (may be empty) comes first.
        if !file_header.is_empty() {
            chunks.push(file_header);
        }
        chunks.push(format!("{cmt} canonical rewrite: {rel_file} -> {target_lang}\n"));

        for item in &file.items {
            if let syn::Item::Fn(item_fn) = item {
                let fn_name = item_fn.sig.ident.to_string();
                let params: Vec<String> = item_fn
                    .sig
                    .inputs
                    .iter()
                    .filter_map(|arg| match arg {
                        syn::FnArg::Typed(pt) => {
                            if let syn::Pat::Ident(pi) = pt.pat.as_ref() {
                                Some(pi.ident.to_string())
                            } else {
                                None
                            }
                        }
                        _ => None,
                    })
                    .collect();

                if let Some(b) = by_fn.get(&fn_name) {
                    let concept_name = name_for_annotation(&result.concepts[b.concept_idx].name);
                    // Delegate to ORP realize_for_bind; fall back to emit_target_stub if
                    // the ORP realizer refuses (e.g. unsupported language).
                    let realized_chunk = match crate::cmd_transport::realize_for_bind(
                        target_lang,
                        &fn_name,
                        &params,
                        &orig,
                        &concept_name,
                    ) {
                        Ok(r) => {
                            // Prepend bind-specific metadata (substrate-origin, memento-cid,
                            // contract annotations, runtime-mode) before the ORP-generated snippet.
                            let meta = build_bind_meta_comment(target_lang, &concept_name, b, mode);
                            format!("{meta}\n{}", r.source)
                        }
                        Err(_) => {
                            // ORP refused (unsupported language or parse error); fall back
                            // to the annotation-level stub so output is never empty.
                            emit_target_stub(target_lang, &fn_name, &params, b, &concept_name, mode)
                        }
                    };
                    chunks.push(realized_chunk);
                } else {
                    // Coverage gap: no binding for this function.
                    chunks.push(format!(
                        "{cmt} bind:canonical:gap: fn {fn_name} has no concept binding for {target_lang}\n"
                    ));
                }
            }
        }

        // F1: emit file-level footer exactly once per output file (currently empty for all langs).
        let file_footer = crate::cmd_transport::realize_file_footer(target_lang);
        if !file_footer.is_empty() {
            chunks.push(file_footer);
        }

        let output_src = chunks.join("\n");

        if to_disk {
            // Write to .provekit/bindings/translated/<lang>/<file>
            let file_name = Path::new(rel_file)
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| rel_file.replace('/', "_"));
            let out_path = translated_dir.join(format!(
                "{}.{}",
                file_name.trim_end_matches(".rs"),
                lang_extension(target_lang)
            ));
            let _ = std::fs::write(out_path, &output_src);
        } else {
            // Invisible: stream to stdout.
            print!("{output_src}");
        }
    }

    // Files with no bindings that are invisible: emit gap comment.
    for path in src_files {
        let rel = path.strip_prefix(root).unwrap_or(path).display().to_string();
        if !by_file.contains_key(&rel) && !to_disk {
            let cmt = comment_prefix_for(target_lang);
            println!("{cmt} bind:canonical:no-bindings:{rel}");
        }
    }
}

/// Return the line-comment prefix for the target language.
///
/// Python and Ruby use `#` for comments; `//` is the floor-division operator
/// and would produce a syntax error. All other supported languages use `//`.
fn comment_prefix_for(target_lang: &str) -> &'static str {
    match target_lang {
        "python" | "ruby" => "#",
        _ => "//",
    }
}

/// Fallback stub emitter used when ORP `realize_for_bind` refuses (e.g. an
/// unsupported target language). Emits bind-level annotations without real
/// body realization. Canonical mode always tries `realize_for_bind` first;
/// this path is only taken on refusal.
fn emit_target_stub(
    target_lang: &str,
    fn_name: &str,
    params: &[String],
    binding: &BindingRecord,
    concept_name: &str,
    mode: &RuntimeMode,
) -> String {
    // Build annotation block for the target language.
    let ann = build_target_annotations(target_lang, concept_name, binding, mode);
    let stub_body = target_stub_body(target_lang);
    let fn_def = target_fn_def(target_lang, fn_name, params, &ann, &stub_body);
    fn_def
}

/// Build language-appropriate annotation prefix for a function.
fn build_target_annotations(
    target_lang: &str,
    concept_name: &str,
    binding: &BindingRecord,
    mode: &RuntimeMode,
) -> String {
    let mut lines: Vec<String> = Vec::new();

    // concept comment (universal)
    let comment_prefix = match target_lang {
        "python" | "ruby" => "#",
        _ => "//",
    };
    lines.push(format!("{comment_prefix} concept: {concept_name}"));
    lines.push(format!(
        "{comment_prefix} substrate-origin: {}",
        binding.origin.label()
    ));
    if !binding.site_memento_cid.is_empty() {
        lines.push(format!(
            "{comment_prefix} memento-cid: {}",
            binding.site_memento_cid
        ));
    }

    // Contract attributes in target-language syntax.
    // F3: Zig does NOT use Rust `#[cfg_attr(...)]` syntax; give it `// @provekit:` comments.
    if let Some(pre) = &binding.pretty_pre {
        match target_lang {
            "rust" => lines.push(format!("#[cfg_attr(any(), requires({pre}))]")),
            "zig" => lines.push(format!("// @requires: {pre}")),
            "java" | "csharp" => lines.push(format!("/* @requires: {pre} */")),
            "python" | "ruby" => lines.push(format!("# @requires: {pre}")),
            _ => lines.push(format!("// @requires: {pre}")),
        }
    }
    if let Some(post) = &binding.pretty_post {
        match target_lang {
            "rust" => lines.push(format!("#[cfg_attr(any(), ensures({post}))]")),
            "zig" => lines.push(format!("// @ensures: {post}")),
            "java" | "csharp" => lines.push(format!("/* @ensures: {post} */")),
            "python" | "ruby" => lines.push(format!("# @ensures: {post}")),
            _ => lines.push(format!("// @ensures: {post}")),
        }
    }

    // Mode attribute.
    match mode {
        RuntimeMode::Monitor => {
            match target_lang {
                "rust" => lines.push(format!(
                    "#[cfg_attr(any(), provekit_monitor(concept = \"{concept_name}\"))]"
                )),
                "zig" => lines.push(format!("// @provekit_monitor(concept = \"{concept_name}\")")),
                "java" => lines.push(format!("// @provekit_monitor(concept = \"{concept_name}\")")),
                "python" => lines.push(format!("# @provekit_monitor(concept = \"{concept_name}\")")),
                _ => lines.push(format!("// @provekit_monitor(concept = \"{concept_name}\")")),
            }
        }
        RuntimeMode::Emitter => {
            match target_lang {
                "rust" => lines.push(format!(
                    "#[cfg_attr(any(), provekit_emitter(concept = \"{concept_name}\"))]"
                )),
                "zig" => lines.push(format!("// @provekit_emitter(concept = \"{concept_name}\")")),
                _ => lines.push(format!("// @provekit_emitter(concept = \"{concept_name}\")")),
            }
        }
        RuntimeMode::Witness => {
            match target_lang {
                "rust" => lines.push(format!(
                    "#[cfg_attr(any(), provekit_witness(concept = \"{concept_name}\"))]"
                )),
                "zig" => lines.push(format!("// @provekit_witness(concept = \"{concept_name}\")")),
                _ => lines.push(format!("// @provekit_witness(concept = \"{concept_name}\")")),
            }
        }
    }

    lines.join("\n")
}

/// Build bind-specific metadata comment block to prepend before an ORP-realized
/// snippet. Carries substrate-origin, memento-cid, contract annotations from
/// the binding record, and the runtime-mode attribute. The ORP realizer may also
/// emit concept+contract lines in its own prefix; the bind-layer ones here ensure
/// provenance is always present even when ORP cannot re-lift contracts from source.
fn build_bind_meta_comment(
    target_lang: &str,
    concept_name: &str,
    binding: &BindingRecord,
    mode: &RuntimeMode,
) -> String {
    let comment_prefix = match target_lang {
        "python" | "ruby" => "#",
        _ => "//",
    };
    let mut lines: Vec<String> = Vec::new();
    lines.push(format!(
        "{comment_prefix} substrate-origin: {}",
        binding.origin.label()
    ));
    if !binding.site_memento_cid.is_empty() {
        lines.push(format!(
            "{comment_prefix} memento-cid: {}",
            binding.site_memento_cid
        ));
    }
    // Contract annotations from the binding record (pre/post as pretty-printed strings).
    // These come from the bind engine's own lift pass, which is authoritative for the
    // source language. ORP may also emit contract lines in its prefix; these ensure
    // they are always present.
    // F3: Zig does NOT use Rust `#[cfg_attr(...)]` syntax.
    if let Some(pre) = &binding.pretty_pre {
        match target_lang {
            "rust" => lines.push(format!("#[cfg_attr(any(), requires({pre}))]")),
            "zig" => lines.push(format!("// @requires: {pre}")),
            "java" | "csharp" => lines.push(format!("/* @requires: {pre} */")),
            "python" | "ruby" => lines.push(format!("# @requires: {pre}")),
            _ => lines.push(format!("// @requires: {pre}")),
        }
    }
    if let Some(post) = &binding.pretty_post {
        match target_lang {
            "rust" => lines.push(format!("#[cfg_attr(any(), ensures({post}))]")),
            "zig" => lines.push(format!("// @ensures: {post}")),
            "java" | "csharp" => lines.push(format!("/* @ensures: {post} */")),
            "python" | "ruby" => lines.push(format!("# @ensures: {post}")),
            _ => lines.push(format!("// @ensures: {post}")),
        }
    }
    match mode {
        RuntimeMode::Monitor => {
            match target_lang {
                "rust" => lines.push(format!(
                    "#[cfg_attr(any(), provekit_monitor(concept = \"{concept_name}\"))]"
                )),
                "java" => lines.push(format!("// @provekit_monitor(concept = \"{concept_name}\")")),
                "python" => lines.push(format!("# @provekit_monitor(concept = \"{concept_name}\")")),
                _ => lines.push(format!("{comment_prefix} @provekit_monitor(concept = \"{concept_name}\")")),
            }
        }
        RuntimeMode::Emitter => {
            match target_lang {
                "rust" => lines.push(format!(
                    "#[cfg_attr(any(), provekit_emitter(concept = \"{concept_name}\"))]"
                )),
                _ => lines.push(format!("{comment_prefix} @provekit_emitter(concept = \"{concept_name}\")")),
            }
        }
        RuntimeMode::Witness => {
            match target_lang {
                "rust" => lines.push(format!(
                    "#[cfg_attr(any(), provekit_witness(concept = \"{concept_name}\"))]"
                )),
                _ => lines.push(format!("{comment_prefix} @provekit_witness(concept = \"{concept_name}\")")),
            }
        }
    }
    lines.join("\n")
}

/// Emit a language-appropriate stub body (the actual term realization is
/// handled by ORP when the full Term graph is available).
fn target_stub_body(target_lang: &str) -> String {
    match target_lang {
        "python" => "    raise NotImplementedError(\"provekit-bind: canonical stub\")".into(),
        "java" => "        throw new UnsupportedOperationException(\"provekit-bind: canonical stub\");".into(),
        "go" => "    panic(\"provekit-bind: canonical stub\")".into(),
        "ruby" => "  raise NotImplementedError, \"provekit-bind: canonical stub\"".into(),
        _ => "    unimplemented!(\"provekit-bind: canonical stub\")".into(),
    }
}

/// Emit a full function definition in the target language, with annotations.
fn target_fn_def(
    target_lang: &str,
    fn_name: &str,
    params: &[String],
    annotations: &str,
    body: &str,
) -> String {
    let indent = match target_lang {
        "java" | "csharp" => "    ",
        _ => "",
    };
    match target_lang {
        "rust" | "zig" => {
            let param_list = params
                .iter()
                .map(|p| format!("{p}: i64"))
                .collect::<Vec<_>>()
                .join(", ");
            format!("{annotations}\n{indent}pub fn {fn_name}({param_list}) -> i64 {{\n{body}\n}}\n")
        }
        "python" => {
            let param_list = params.join(", ");
            format!("{annotations}\ndef {fn_name}({param_list}):\n{body}\n")
        }
        "java" => {
            let param_list = params
                .iter()
                .map(|p| format!("long {p}"))
                .collect::<Vec<_>>()
                .join(", ");
            // Each function gets a uniquely-named top-level class to avoid
            // "multiple top-level classes in one file" Java compiler errors.
            let class_name = snake_to_pascal(fn_name) + "Transported";
            format!(
                "final class {class_name} {{\n{annotations}\n{indent}public static long {fn_name}({param_list}) {{\n{body}\n{indent}}}\n}}\n"
            )
        }
        "go" => {
            let param_list = params
                .iter()
                .map(|p| format!("{p} int64"))
                .collect::<Vec<_>>()
                .join(", ");
            format!("{annotations}\nfunc {fn_name}({param_list}) int64 {{\n{body}\n}}\n")
        }
        "csharp" => {
            let param_list = params
                .iter()
                .map(|p| format!("long {p}"))
                .collect::<Vec<_>>()
                .join(", ");
            format!(
                "public static class Transported {{\n{annotations}\n{indent}public static long {fn_name}({param_list}) {{\n{body}\n{indent}}}\n}}\n"
            )
        }
        "typescript" => {
            let param_list = params
                .iter()
                .map(|p| format!("{p}: number"))
                .collect::<Vec<_>>()
                .join(", ");
            format!("{annotations}\nexport function {fn_name}({param_list}): number {{\n{body}\n}}\n")
        }
        "ruby" => {
            let param_list = params.join(", ");
            format!("{annotations}\ndef {fn_name}({param_list})\n{body}\nend\n")
        }
        "php" => {
            let param_list = params
                .iter()
                .map(|p| format!("${p}"))
                .collect::<Vec<_>>()
                .join(", ");
            format!(
                "<?php\n{annotations}\nfunction {fn_name}({param_list}) {{\n{body}\n}}\n"
            )
        }
        _ => {
            // Fallback: Rust-style.
            let param_list = params
                .iter()
                .map(|p| format!("{p}: i64"))
                .collect::<Vec<_>>()
                .join(", ");
            format!("{annotations}\npub fn {fn_name}({param_list}) -> i64 {{\n{body}\n}}\n")
        }
    }
}

fn lang_extension(lang: &str) -> &'static str {
    match lang {
        "python" => "py",
        "java" => "java",
        "go" => "go",
        "csharp" => "cs",
        "typescript" => "ts",
        "zig" => "zig",
        "ruby" => "rb",
        "php" => "php",
        _ => "rs",
    }
}

/// Convert a snake_case identifier to PascalCase for use as a Java class name.
/// E.g. "deposit" -> "Deposit", "retry_send" -> "RetrySend".
fn snake_to_pascal(s: &str) -> String {
    s.split('_')
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                None => String::new(),
                Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
            }
        })
        .collect()
}

// ============================================================================
// v0 stubs: algebra / cluster / discharge / attrs (labeled)
// ============================================================================

// ---- Term shape (v0 stub — production uses provekit-ir-symbolic) ------------

struct TermShape {
    root: ShapeNode,
}

#[allow(dead_code)]
enum ShapeNode {
    Body(Vec<ShapeNode>),
    If {
        cond: Box<ShapeNode>,
        then_branch: Box<ShapeNode>,
        else_branch: Option<Box<ShapeNode>>,
    },
    While {
        cond: Box<ShapeNode>,
        body: Box<ShapeNode>,
    },
    For {
        body: Box<ShapeNode>,
    },
    Exit,
    Assign,
    Let,
    Rel {
        op: String,
    },
    Bin {
        op: String,
    },
    Call,
    Block(Vec<ShapeNode>),
    Opaque,
}

impl TermShape {
    fn from_fn(item_fn: &syn::ItemFn) -> Self {
        let stmts = item_fn.block.stmts.iter().map(shape_of_stmt).collect();
        TermShape {
            root: ShapeNode::Body(stmts),
        }
    }

    fn shape_cid(&self) -> String {
        let v = node_to_value(&self.root);
        blake3_512_of(encode_jcs(&v).as_bytes())
    }

    fn classify(&self) -> &'static str {
        classify_node(&self.root)
    }
}

fn shape_of_stmt(stmt: &syn::Stmt) -> ShapeNode {
    match stmt {
        syn::Stmt::Expr(e, _) => shape_of_expr(e),
        syn::Stmt::Local(l) => {
            let _ = l;
            ShapeNode::Let
        }
        _ => ShapeNode::Opaque,
    }
}

fn shape_of_expr(expr: &syn::Expr) -> ShapeNode {
    match expr {
        syn::Expr::If(e) => ShapeNode::If {
            cond: Box::new(shape_of_expr(&e.cond)),
            then_branch: Box::new(ShapeNode::Block(
                e.then_branch.stmts.iter().map(shape_of_stmt).collect(),
            )),
            else_branch: e.else_branch.as_ref().map(|(_, else_expr)| {
                Box::new(shape_of_expr(else_expr))
            }),
        },
        syn::Expr::While(e) => ShapeNode::While {
            cond: Box::new(shape_of_expr(&e.cond)),
            body: Box::new(ShapeNode::Block(
                e.body.stmts.iter().map(shape_of_stmt).collect(),
            )),
        },
        syn::Expr::ForLoop(e) => ShapeNode::For {
            body: Box::new(ShapeNode::Block(
                e.body.stmts.iter().map(shape_of_stmt).collect(),
            )),
        },
        syn::Expr::Return(_) | syn::Expr::Break(_) | syn::Expr::Continue(_) => ShapeNode::Exit,
        syn::Expr::Assign(_) => ShapeNode::Assign,
        syn::Expr::Binary(e) => {
            let op = match &e.op {
                syn::BinOp::Add(_) | syn::BinOp::AddAssign(_) => "+",
                syn::BinOp::Sub(_) | syn::BinOp::SubAssign(_) => "-",
                syn::BinOp::Mul(_) | syn::BinOp::MulAssign(_) => "*",
                syn::BinOp::Div(_) | syn::BinOp::DivAssign(_) => "/",
                syn::BinOp::Rem(_) | syn::BinOp::RemAssign(_) => "%",
                syn::BinOp::Eq(_) => "==",
                syn::BinOp::Ne(_) => "!=",
                syn::BinOp::Lt(_) => "<",
                syn::BinOp::Le(_) => "<=",
                syn::BinOp::Gt(_) => ">",
                syn::BinOp::Ge(_) => ">=",
                _ => "opaque-op",
            };
            let is_rel = matches!(op, "==" | "!=" | "<" | "<=" | ">" | ">=");
            if is_rel {
                ShapeNode::Rel { op: op.to_string() }
            } else {
                ShapeNode::Bin { op: op.to_string() }
            }
        }
        syn::Expr::Call(_) | syn::Expr::MethodCall(_) => ShapeNode::Call,
        syn::Expr::Block(b) => ShapeNode::Block(
            b.block.stmts.iter().map(shape_of_stmt).collect(),
        ),
        _ => ShapeNode::Opaque,
    }
}

fn node_to_value(node: &ShapeNode) -> Arc<Value> {
    match node {
        ShapeNode::Body(stmts) => Value::object([
            ("kind", Value::string("body")),
            ("stmts", Value::array(stmts.iter().map(node_to_value).collect())),
        ]),
        ShapeNode::If { cond, then_branch, else_branch } => {
            let mut kv: Vec<(&str, Arc<Value>)> = Vec::new();
            kv.push(("kind", Value::string("if")));
            kv.push(("cond", node_to_value(cond)));
            kv.push(("then", node_to_value(then_branch)));
            if let Some(e) = else_branch {
                kv.push(("else", node_to_value(e)));
            }
            Value::object(kv)
        }
        ShapeNode::While { cond, body } => Value::object([
            ("kind", Value::string("while")),
            ("cond", node_to_value(cond)),
            ("body", node_to_value(body)),
        ]),
        ShapeNode::For { body } => Value::object([
            ("kind", Value::string("for")),
            ("body", node_to_value(body)),
        ]),
        ShapeNode::Exit => Value::object([("kind", Value::string("exit"))]),
        ShapeNode::Assign => Value::object([("kind", Value::string("assign"))]),
        ShapeNode::Let => Value::object([("kind", Value::string("let"))]),
        ShapeNode::Rel { op } => Value::object([
            ("kind", Value::string("rel")),
            ("op", Value::string(op.clone())),
        ]),
        ShapeNode::Bin { op } => Value::object([
            ("kind", Value::string("bin")),
            ("op", Value::string(op.clone())),
        ]),
        ShapeNode::Call => Value::object([("kind", Value::string("call"))]),
        ShapeNode::Block(stmts) => Value::object([
            ("kind", Value::string("block")),
            ("stmts", Value::array(stmts.iter().map(node_to_value).collect())),
        ]),
        ShapeNode::Opaque => Value::object([("kind", Value::string("opaque"))]),
    }
}

fn classify_node(node: &ShapeNode) -> &'static str {
    match node {
        ShapeNode::Body(stmts) => {
            let has_while = stmts.iter().any(|s| matches!(s, ShapeNode::While { .. }));
            let has_for = stmts.iter().any(|s| matches!(s, ShapeNode::For { .. }));
            let has_if = stmts.iter().any(|s| matches!(s, ShapeNode::If { .. }));
            if has_while || has_for {
                "retry-loop"
            } else if has_if {
                "guard-then-commit"
            } else {
                "unknown"
            }
        }
        _ => "unknown",
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

            let sentinel = Cid::parse(format!("blake3-512:{}", "0".repeat(128)))
                .expect("sentinel cid");
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

struct ExtractedContract {
    pre: Option<String>,
    post: Option<String>,
}

fn extract_contract_attrs(attrs: &[syn::Attribute]) -> ExtractedContract {
    let mut out = ExtractedContract { pre: None, post: None };
    for attr in attrs {
        if let Some(name) = attr.path().get_ident().map(|i| i.to_string()) {
            if let syn::Meta::List(l) = &attr.meta {
                let text = normalize_ws(&l.tokens.to_string());
                match name.as_str() {
                    "requires" => { if out.pre.is_none() { out.pre = Some(text); } }
                    "ensures" => { if out.post.is_none() { out.post = Some(text); } }
                    _ => {}
                }
            }
        }
        if attr.path().is_ident("cfg_attr") {
            if let syn::Meta::List(l) = &attr.meta {
                let tokens_str = l.tokens.to_string();
                // Parse the form: `any() , <kind>(<body>)` textually.
                // This avoids the proc_macro2 dependency.
                if let Some(rest) = tokens_str.strip_prefix("any ()") {
                    let rest = rest.trim().trim_start_matches(',').trim();
                    parse_kind_body(rest, &mut out);
                } else if let Some(rest) = tokens_str.strip_prefix("any()") {
                    let rest = rest.trim().trim_start_matches(',').trim();
                    parse_kind_body(rest, &mut out);
                }
            }
        }
    }
    out
}

fn parse_kind_body(s: &str, out: &mut ExtractedContract) {
    // s is like `requires(max_attempts >= 0)` or `ensures(out >= 0)`.
    for kind in ["requires", "ensures"] {
        if let Some(rest) = s.strip_prefix(kind) {
            let rest = rest.trim_start();
            if rest.starts_with('(') && rest.ends_with(')') {
                let body = normalize_ws(&rest[1..rest.len() - 1]);
                match kind {
                    "requires" => { if out.pre.is_none() { out.pre = Some(body); } }
                    "ensures" => { if out.post.is_none() { out.post = Some(body); } }
                    _ => {}
                }
            }
        }
    }
}

fn extract_concept_annotation(src: &str, fn_name: &str) -> Option<String> {
    let needle = format!("fn {fn_name}(");
    let lines: Vec<&str> = src.lines().collect();
    for (i, line) in lines.iter().enumerate() {
        if line.contains(&needle) {
            let mut j = i;
            while j > 0 {
                let prev = lines[j - 1].trim_start();
                if let Some(rest) = prev.strip_prefix("// concept:") {
                    let trimmed = rest.trim().to_string();
                    if trimmed.starts_with("UNNAMED-CONCEPT-") {
                        return None;
                    }
                    return Some(trimmed);
                }
                if prev.starts_with("#[")
                    || prev.starts_with("// substrate-origin:")
                    || prev.starts_with("// memento-cid:")
                    || prev.starts_with("// witness-inherited-from:")
                {
                    j -= 1;
                    continue;
                }
                break;
            }
            return None;
        }
    }
    None
}

// ---- Test-lift (v0 stub) ---------------------------------------------------

fn collect_test_witnesses(
    test_files: &[PathBuf],
) -> Vec<(String, String, String)> {
    let mut out = Vec::new();
    for path in test_files {
        let Ok(src) = std::fs::read_to_string(path) else { continue };
        let Ok(file) = syn::parse_file(&src) else { continue };
        let rel = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        for item in &file.items {
            if let syn::Item::Fn(item_fn) = item {
                let is_test = item_fn.attrs.iter().any(|a| a.path().is_ident("test"));
                if !is_test { continue; }
                let mut let_map: BTreeMap<String, String> = BTreeMap::new();
                for stmt in &item_fn.block.stmts {
                    if let syn::Stmt::Local(local) = stmt {
                        if let Some(name) = pat_to_ident(&local.pat) {
                            if let Some(init) = &local.init {
                                if let Some(callee) = call_target(&init.expr) {
                                    let_map.insert(name, callee);
                                }
                            }
                        }
                    }
                }
                for stmt in &item_fn.block.stmts {
                    if let syn::Stmt::Macro(m) = stmt {
                        if let Some(ident) = m.mac.path.get_ident() {
                            let s = ident.to_string();
                            if s == "assert" || s == "assert_eq" || s == "assert_ne" {
                                let body = m.mac.tokens.to_string();
                                let lhs = first_ident_before_relop(&body);
                                let target = lhs.as_deref()
                                    .and_then(|k| let_map.get(k).cloned())
                                    .or_else(|| guess_fn_under_test(&body));
                                if let Some(fn_name) = target {
                                    let normalized = rewrite_assert_to_post(
                                        body.split(',').next().unwrap_or(&body).trim(),
                                    );
                                    out.push((format!("{rel}:{s}"), fn_name, normalized));
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    out
}

fn pat_to_ident(pat: &syn::Pat) -> Option<String> {
    if let syn::Pat::Ident(p) = pat { Some(p.ident.to_string()) } else { None }
}

fn call_target(expr: &syn::Expr) -> Option<String> {
    match expr {
        syn::Expr::Call(c) => {
            if let syn::Expr::Path(p) = c.func.as_ref() {
                p.path.segments.last().map(|s| s.ident.to_string())
            } else { None }
        }
        syn::Expr::MethodCall(m) => Some(m.method.to_string()),
        _ => None,
    }
}

fn first_ident_before_relop(s: &str) -> Option<String> {
    let bytes = s.as_bytes();
    let mut i = 0;
    let mut last: Option<String> = None;
    let mut cur = String::new();
    while i < bytes.len() {
        let c = bytes[i] as char;
        if c.is_alphanumeric() || c == '_' { cur.push(c); }
        else {
            if !cur.is_empty() { last = Some(std::mem::take(&mut cur)); }
            let two = if i + 1 < bytes.len() {
                std::str::from_utf8(&bytes[i..i + 2]).unwrap_or("")
            } else { "" };
            if matches!(two, ">=" | "<=" | "==" | "!=") { return last; }
            if matches!(c, '>' | '<') { return last; }
        }
        i += 1;
    }
    None
}

fn guess_fn_under_test(body: &str) -> Option<String> {
    let mut cur = String::new();
    for c in body.chars() {
        if c.is_alphanumeric() || c == '_' { cur.push(c); }
        else if c == '(' && !cur.is_empty()
            && cur.chars().next().map(|x| x.is_alphabetic()).unwrap_or(false)
        {
            if !matches!(cur.as_str(), "assert" | "let" | "if" | "for" | "while" | "return" | "match") {
                return Some(cur);
            }
            cur.clear();
        } else { cur.clear(); }
    }
    None
}

fn rewrite_assert_to_post(s: &str) -> String {
    for op in [">=", "<=", "==", "!=", ">", "<"].iter() {
        if let Some(pos) = s.find(op) {
            let (_, rhs) = s.split_at(pos);
            return format!("out {rhs}");
        }
    }
    s.to_string()
}

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
fn resolve_lang_detect(root: &Path) -> Result<String, String> {
    // Check for Rust indicators.
    if root.join("Cargo.toml").exists() {
        return Ok("rust".to_string());
    }
    if root.join("src").is_dir() {
        // Check if src/ contains .rs files.
        if let Ok(entries) = std::fs::read_dir(root.join("src")) {
            for e in entries.flatten() {
                if e.path().extension().map(|x| x == "rs").unwrap_or(false) {
                    return Ok("rust".to_string());
                }
            }
        }
    }
    // Check root for *.rs files.
    if let Ok(entries) = std::fs::read_dir(root) {
        for e in entries.flatten() {
            if e.path().extension().map(|x| x == "rs").unwrap_or(false) {
                return Ok("rust".to_string());
            }
        }
    }

    // Check for Java indicators.
    if root.join("pom.xml").exists() || root.join("build.gradle").exists() {
        return Ok("java".to_string());
    }
    if let Ok(entries) = std::fs::read_dir(root) {
        for e in entries.flatten() {
            if e.path().extension().map(|x| x == "java").unwrap_or(false) {
                return Ok("java".to_string());
            }
        }
    }

    // Check for Python indicators.
    if root.join("pyproject.toml").exists() || root.join("setup.py").exists() {
        return Ok("python".to_string());
    }
    if let Ok(entries) = std::fs::read_dir(root) {
        for e in entries.flatten() {
            if e.path().extension().map(|x| x == "py").unwrap_or(false) {
                return Ok("python".to_string());
            }
        }
    }

    Err(format!(
        "cannot determine source language from {}; directory contains no recognised source files \
         (.rs / Cargo.toml, .java / pom.xml / build.gradle, .py / pyproject.toml / setup.py). \
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

fn collect_rs_files(dir: &Path) -> Vec<PathBuf> {
    if !dir.is_dir() {
        return vec![];
    }
    let walker = walkdir::WalkDir::new(dir).max_depth(8);
    walker
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().map(|x| x == "rs").unwrap_or(false))
        .map(|e| e.path().to_path_buf())
        .collect()
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
    let mut start = 0usize;
    for (i, &b) in bytes.iter().enumerate() {
        if cur_line == line_no {
            start = i;
            let end_offset = bytes[i..]
                .iter()
                .position(|&x| x == b'\n')
                .map(|p| i + p)
                .unwrap_or(bytes.len());
            return (start as u64, end_offset as u64);
        }
        if b == b'\n' {
            cur_line += 1;
        }
    }
    (bytes.len() as u64, bytes.len() as u64)
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
        serde_json::Value::Array(arr) => {
            Value::array(arr.iter().map(json_to_value).collect())
        }
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
        .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
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
    if name.is_empty() { None } else { Some(name) }
}

fn name_for_annotation(name: &str) -> &str {
    name.strip_prefix("concept:").unwrap_or(name)
}

fn normalize_ws(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut prev_ws = false;
    for c in s.chars() {
        if c.is_whitespace() {
            if !prev_ws && !out.is_empty() { out.push(' '); }
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
    fn with_temp_dir<F: FnOnce(&std::path::Path)>(
        populate: F,
    ) -> Result<String, String> {
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
