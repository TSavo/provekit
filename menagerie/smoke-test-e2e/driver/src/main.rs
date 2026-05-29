// SPDX-License-Identifier: Apache-2.0
//
// End-to-end smoke-test driver for the paper-20 vision.
//
// Implements the eight verbs end to end against the fixture under
// `menagerie/smoke-test-e2e/src`:
//
//     Lift  -> Cluster -> Name -> Scope -> Cluster
//           -> Identify -> Realize -> Witness.
//
// Output:
//   - menagerie/smoke-test-e2e/artifacts/   signed mementos, JSON
//   - menagerie/smoke-test-e2e/rewritten/   rewritten source files
//                                            with substrate-attributed
//                                            contracts and concept
//                                            annotations
//   - menagerie/smoke-test-e2e/report.md    the smoke-test report
//
// Conventions:
//   - All hashes are BLAKE3-512 self-identifying strings via
//     provekit_canonicalizer::blake3_512_of.
//   - All mementos that have a corresponding mint API in
//     provekit_claim_envelope use it (signed, layered shape).
//   - ConceptSiteMemento is emitted with `schemaVersion: "1"` using the
//     canonical provekit_ir_types::ConceptSiteMemento type (PR #692 /
//     5919e46f). Provenance CIDs and discharge_receipt_cid are synthetic
//     deterministic hashes; see report §8 "Known transport losses".
//   - ConceptAbstractionMemento uses a locally-defined stub schema labelled
//     "schemaVersion": "stub-0" (the ConceptAbstractionMemento spec is a
//     separate landing from the ConceptSiteMemento spec).

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use provekit_canonicalizer::{blake3_512_of, encode_jcs, Value};
use provekit_claim_envelope::{mint_contract, Authoring, MintContractArgs};
use provekit_ir_types::{
    CodeSite, CodeSiteSpan, ConceptSiteMemento, ConceptSiteProvenance, Discharge, IrFormula,
    LossRecord,
};
use provekit_proof_envelope::Ed25519Seed;

mod algebra;
mod attrs;
mod cluster;
mod naming_roundtrip;
mod realize;
mod report;
mod synthesize;
mod test_lift;

use algebra::{FormulaShape, TermShape};

fn main() {
    let fixture_dir = locate_fixture_dir();
    eprintln!("[smoke] fixture dir: {}", fixture_dir.display());

    let pass_1 = run_pass(
        &fixture_dir,
        /*pass_id=*/ 1,
        /*read_concept_comments=*/ false,
    );
    eprintln!(
        "[smoke] pass 1 complete: {} sites, {} concepts ({} unnamed)",
        pass_1.bindings.len(),
        pass_1.concepts.len(),
        pass_1.unnamed_count()
    );

    // Write pass-1 rewritten files.
    let rewritten_dir = fixture_dir.join("rewritten");
    let _ = fs::create_dir_all(&rewritten_dir);
    realize::write_rewritten(&fixture_dir, &rewritten_dir, &pass_1);
    eprintln!(
        "[smoke] pass 1 rewritten files at {}",
        rewritten_dir.display()
    );

    // Simulated human action: pick the first UNNAMED concept and replace
    // its name with `retry-with-jitter` in the rewritten source. This
    // mutation goes through the same `// concept: <name>` annotation
    // the driver itself emits; the next pass reads it back.
    let renamed_pair = naming_roundtrip::apply_human_naming(
        &rewritten_dir,
        &pass_1,
        // The unnamed cluster that surfaces is a saturating clamp
        // pattern. The human renames it once; every site of the same
        // shape inherits the name on the next pass.
        "saturating-clamp",
    );
    if let Some((shape_cid, new_name)) = &renamed_pair {
        eprintln!(
            "[smoke] human rename: shape {} now named '{}'",
            shape_cid, new_name
        );
    } else {
        eprintln!("[smoke] no unnamed concept found to rename in pass 1");
    }

    // Pass 2: lift the REWRITTEN code (now carrying the human-supplied
    // name on the same shape-CID). The driver respects the comment as
    // substrate input and inherits the name onto every binding of that
    // shape-CID.
    let pass_2 = run_pass(
        &rewritten_dir,
        /*pass_id=*/ 2,
        /*read_concept_comments=*/ true,
    );
    eprintln!(
        "[smoke] pass 2 complete: {} sites, {} concepts ({} unnamed)",
        pass_2.bindings.len(),
        pass_2.concepts.len(),
        pass_2.unnamed_count()
    );

    // Write the final report. The report is the substrate speaking.
    let report_path = fixture_dir.join("report.md");
    let report_md = report::render_report(&fixture_dir, &pass_1, &pass_2, renamed_pair.as_ref());
    fs::write(&report_path, report_md).expect("write report.md");
    eprintln!("[smoke] report written: {}", report_path.display());
}

/// Walk up from CARGO_MANIFEST_DIR and locate the fixture root.
fn locate_fixture_dir() -> PathBuf {
    // CARGO_MANIFEST_DIR for the driver crate is .../smoke-test-e2e/driver.
    let driver_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    driver_dir
        .parent()
        .expect("driver has a parent")
        .to_path_buf()
}

// ===========================================================================
// PassResult: the artifact a single Lift+Cluster+Name+...+Realize pass emits.
// ===========================================================================

#[derive(Debug, Clone)]
pub struct PassResult {
    pub pass_id: u32,
    /// Every lifted contract decl, keyed by (file, fn).
    pub bindings: Vec<BindingRecord>,
    /// Concept registry built during this pass.
    pub concepts: Vec<ConceptRecord>,
    /// Witness obligations attached to a concept CID and propagated.
    pub witnesses: Vec<WitnessRecord>,
}

impl PassResult {
    fn unnamed_count(&self) -> usize {
        self.concepts
            .iter()
            .filter(|c| c.name.starts_with("UNNAMED-CONCEPT-"))
            .count()
    }
}

#[derive(Debug, Clone)]
pub struct BindingRecord {
    pub site_file: String,
    pub site_fn: String,
    pub site_line: usize,
    /// The lifted FunctionContract memento (signed envelope CID), if any.
    pub contract_cid: Option<String>,
    pub contract_content_cid: Option<String>,
    /// Source of the contract: which substrate input produced it.
    pub contract_origin: ContractOrigin,
    /// Canonical term-shape CID this site collapses to (the input to
    /// clustering). The shape ignores variable names, literal values,
    /// and trivial syntactic variation; it is the algebra's address
    /// for this site.
    pub shape_cid: String,
    /// Index into PassResult.concepts.
    pub concept_idx: usize,
    /// Site-level memento CID for the concept:site binding (canonical ConceptSiteMemento schema v1).
    /// Empty string when no contract was recovered (site is skip-emitted; see report §8).
    pub site_memento_cid: String,
    /// wp-discharge verdict: "exact" / "loudly-bounded-lossy" / "refuse".
    pub discharge_verdict: DischargeVerdict,
    /// Pretty-printed contract (for the report and the rewritten files).
    pub pretty_pre: Option<String>,
    pub pretty_post: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContractOrigin {
    /// Lifted from `#[requires]` / `#[ensures]` attribute on the source.
    AttributeLift,
    /// Lifted from an `assert!` inside a unit test that exercises this fn.
    TestLift,
    /// Synthesized by applying a wp_rule registered for the cluster.
    AlgebraSynthesis { rule_id: String },
    /// No contract recovered (the concept itself is contract-free).
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
}

#[derive(Debug, Clone)]
pub struct ConceptRecord {
    /// `concept:<name>` if named, else `UNNAMED-CONCEPT-N`. Stable across
    /// passes only via `shape_cid`; the name may change.
    pub name: String,
    /// Canonical address: BLAKE3-512(JCS(canonical-term-shape)) of the
    /// FIRST shape observed for this concept. The cluster's compression
    /// can absorb additional shape-CIDs as aliases (see below).
    pub shape_cid: String,
    /// Additional shape-CIDs that the catalog or human-annotation
    /// collapsed into this concept. Each alias is a structurally distinct
    /// canonical shape that nonetheless realizes the same concept; the
    /// list is the algebra-compression receipt.
    pub shape_cid_aliases: Vec<String>,
    /// True if this name came from a human-supplied annotation in the
    /// rewritten source.
    pub name_source: NameSource,
    /// Site indices (into PassResult.bindings) that realize this concept.
    pub site_indices: Vec<usize>,
    /// Concept-abstraction memento CID (stub schema).
    pub abstraction_cid: String,
    /// If the cluster matched a known catalog shape, the catalog id.
    pub catalog_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NameSource {
    /// Auto-generated UNNAMED-CONCEPT-N.
    Auto,
    /// Hard-coded in the driver's seed catalog for shape kinds it ships with
    /// (e.g. `concept:option-default`, `concept:retry-with-bounded-attempts`).
    Catalog,
    /// Read from a `// concept: <name>` annotation in the source on the
    /// most recent pass. THIS is the closing edge of the naming round-trip.
    HumanAnnotation,
}

#[derive(Debug, Clone)]
pub struct WitnessRecord {
    /// The concept-CID this witness is attached to.
    pub concept_shape_cid: String,
    /// Source of the witness: the file:line of the test that produced it.
    pub source_location: String,
    /// Pretty-printed witness formula.
    pub pretty_formula: String,
    /// Indices of bindings now inheriting this obligation.
    pub propagated_to: Vec<usize>,
}

// ===========================================================================
// run_pass: implements the eight verbs end-to-end on a source tree.
// ===========================================================================

fn run_pass(source_root: &Path, pass_id: u32, read_concept_comments: bool) -> PassResult {
    // Collect *.rs sources under src/ (and tests/ for the test lift).
    let src_dir = source_root.join("src");
    let tests_dir = source_root.join("tests");

    let src_files = list_rs_files(&src_dir);
    let test_files = list_rs_files(&tests_dir);

    let artifacts_dir = source_root.join("artifacts");
    let _ = fs::create_dir_all(&artifacts_dir);

    let signer_seed: Ed25519Seed = [0x42; 32];

    // ---- Verb 1: LIFT -----------------------------------------------------
    //
    // For each file under src/, parse with syn::parse_file. For each fn:
    //   - collect any `#[requires(..)]`/`#[ensures(..)]` attributes
    //     (or their cfg_attr-gated form),
    //   - collect any `// concept: <name>` line above the fn (only
    //     consumed when read_concept_comments == true),
    //   - build a canonical TermShape for the fn body via
    //     algebra::TermShape::from_fn,
    //   - hash the shape for clustering.
    let mut raw_lifts: Vec<RawLift> = Vec::new();
    for path in &src_files {
        let src = fs::read_to_string(path).expect("read fixture src");
        let file = match syn::parse_file(&src) {
            Ok(f) => f,
            Err(e) => {
                eprintln!("[smoke] parse error in {}: {}", path.display(), e);
                continue;
            }
        };
        let rel = path
            .strip_prefix(source_root)
            .unwrap_or(path)
            .display()
            .to_string();
        for item in &file.items {
            if let syn::Item::Fn(item_fn) = item {
                let fn_name = item_fn.sig.ident.to_string();
                let line = approximate_line_for_fn(&src, &fn_name);
                let attr_contract = attrs::extract_contract_attrs(&item_fn.attrs);
                let concept_comment = if read_concept_comments {
                    attrs::extract_concept_annotation(&src, &fn_name)
                } else {
                    None
                };
                let term_shape = TermShape::from_fn(item_fn);
                let formula_shape = FormulaShape::from_fn_body(item_fn);
                raw_lifts.push(RawLift {
                    file: rel.clone(),
                    fn_name,
                    fn_line: line,
                    attr_pre: attr_contract.pre,
                    attr_post: attr_contract.post,
                    concept_annotation: concept_comment,
                    term_shape,
                    formula_shape,
                });
            }
        }
    }

    // ---- Verb 2 + 5: CLUSTER ---------------------------------------------
    //
    // The clustering key is (human-annotation | catalog-classification | shape-cid).
    //
    //   - If a `// concept: <name>` annotation is present, ALL sites
    //     with that same name share a concept (the human's choice
    //     authoritatively groups them).
    //   - Else if the term-shape's classifier matches a seed-catalog
    //     entry, the catalog name groups every shape that classifies
    //     to that entry. Different shape-CIDs that classify to the
    //     same catalog entry ARE deliberately merged here. This is
    //     the substrate's compression event: the algebra recognizes
    //     two surface-different variants as the same concept.
    //   - Else (shape unknown to the catalog and to humans), the
    //     shape-CID itself is the bucket key.
    //
    // Each concept's site list collects every binding that resolved
    // to that bucket regardless of which alias shape-CID produced it.
    // Aliases are tracked so the report can show how compression
    // collapsed multiple shape-CIDs.
    let mut concepts: Vec<ConceptRecord> = Vec::new();
    let mut key_to_concept_idx: BTreeMap<String, usize> = BTreeMap::new();
    let mut unnamed_counter = 0u32;

    let catalog = cluster::seed_catalog();

    for lift in &raw_lifts {
        let shape_cid = lift.term_shape.shape_cid();
        let matched_catalog = catalog.match_shape(&shape_cid, &lift.term_shape);
        let (bucket_key, name_pick, source, catalog_id_pick) =
            if let Some(human) = lift.concept_annotation.as_ref() {
                (
                    format!("human:{}", human),
                    format!("concept:{}", human),
                    NameSource::HumanAnnotation,
                    None,
                )
            } else if let Some(c) = matched_catalog {
                (
                    format!("catalog:{}", c.id),
                    c.name.clone(),
                    NameSource::Catalog,
                    Some(c.id.clone()),
                )
            } else {
                (
                    format!("shape:{}", shape_cid),
                    String::new(),
                    NameSource::Auto,
                    None,
                )
            };

        if !key_to_concept_idx.contains_key(&bucket_key) {
            let final_name = if source == NameSource::Auto {
                unnamed_counter += 1;
                format!("UNNAMED-CONCEPT-{}", unnamed_counter)
            } else {
                name_pick
            };
            if source == NameSource::Auto {
                let target_concept = format!("concept:{final_name}");
                if let Ok(gap) = cluster::unknown_shape_gap_record(
                    &shape_cid,
                    &target_concept,
                    "rust",
                ) {
                    if let Ok(bytes) = serde_json::to_vec_pretty(&gap) {
                        let _ = fs::write(
                            artifacts_dir.join(format!(
                                "pass{}_cluster_gap_{}.json",
                                pass_id,
                                sanitize(&final_name)
                            )),
                            bytes,
                        );
                    }
                }
            }

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
                shape_cid_aliases: Vec::new(),
                name_source: source,
                site_indices: Vec::new(),
                abstraction_cid,
                catalog_id: catalog_id_pick,
            });
            key_to_concept_idx.insert(bucket_key, idx);
        } else {
            // Record the alias shape-CID under the existing concept.
            let idx = *key_to_concept_idx.get(&bucket_key).unwrap();
            let primary = concepts[idx].shape_cid.clone();
            if shape_cid != primary && !concepts[idx].shape_cid_aliases.contains(&shape_cid) {
                concepts[idx].shape_cid_aliases.push(shape_cid.clone());
            }
        }
    }

    // Build a per-shape-CID -> concept_idx lookup used during binding scoping.
    let mut shape_to_concept_idx: BTreeMap<String, usize> = BTreeMap::new();
    for (ci, c) in concepts.iter().enumerate() {
        shape_to_concept_idx.insert(c.shape_cid.clone(), ci);
        for alias in &c.shape_cid_aliases {
            shape_to_concept_idx.insert(alias.clone(), ci);
        }
    }

    // ---- Verb 4: SCOPE ---------------------------------------------------
    //
    // Each binding is scoped to its file:fn location. We attach the
    // binding into its concept's site_indices and produce a per-site
    // memento CID.

    let mut bindings: Vec<BindingRecord> = Vec::new();
    for lift in &raw_lifts {
        let shape_cid = lift.term_shape.shape_cid();
        let concept_idx = *shape_to_concept_idx.get(&shape_cid).expect("clustered");
        // ----- Choose contract origin in priority order -----
        //
        //   1. attribute lift if present,
        //   2. test lift if a matching assertion exists in tests/,
        //   3. algebra synthesis if the cluster has a registered wp_rule,
        //   4. Empty.
        let (origin, pre, post) = if lift.attr_pre.is_some() || lift.attr_post.is_some() {
            (
                ContractOrigin::AttributeLift,
                lift.attr_pre.clone(),
                lift.attr_post.clone(),
            )
        } else if let Some(test_post) = test_lift::lift_assertion_for_fn(&test_files, &lift.fn_name)
        {
            (ContractOrigin::TestLift, None, Some(test_post))
        } else if let Some(rule) = synthesize::wp_rule_for_shape(&shape_cid, &lift.term_shape) {
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

        // Mint a real signed contract memento via provekit-claim-envelope
        // when we have a non-empty contract.
        let (contract_cid, contract_content_cid) = if pre.is_some() || post.is_some() {
            let pre_v = pre.as_deref().map(formula_text_to_value);
            let post_v = post.as_deref().map(formula_text_to_value);
            let mint_args = MintContractArgs {
                contract_name: format!("smoke::{}::{}", lift.file, lift.fn_name),
                pre: pre_v,
                post: post_v,
                inv: None,
                out_binding: "out".to_string(),
                produced_by: "smoke-test-e2e-driver@0.1.0".into(),
                produced_at: "2026-05-12T00:00:00.000Z".into(),
                input_cids: vec![],
                authoring: Authoring::Lift {
                    lifter: "smoke-test-e2e-driver".into(),
                    evidence: origin.label(),
                    source_cid: None,
                },
                signer_seed,
                formals: Vec::new(),
                formal_sorts: Vec::new(),
                emit_empty_formals: false,
            };
            match mint_contract(&mint_args) {
                Ok(env) => {
                    // Persist the memento.
                    let target = artifacts_dir.join(format!(
                        "pass{}_contract_{}_{}.proof.json",
                        pass_id,
                        sanitize(&lift.file),
                        lift.fn_name
                    ));
                    let _ = fs::write(&target, &env.canonical_bytes);
                    (Some(env.cid), Some(env.contract_cid))
                }
                Err(e) => {
                    eprintln!("[smoke] mint_contract failed for {}: {}", lift.fn_name, e);
                    (None, None)
                }
            }
        } else {
            (None, None)
        };

        // Discharge verdict.
        // The live libprovekit::wp evaluator runs for algebra-synthesis sites
        // (wp-as-formula PR series fully merged on main; Stub 2 closed). For
        // annotation-lift and test-lift sites the verdict is computed by a
        // structural oracle and labelled "loudly-bounded-lossy" because the
        // formula encoding is single-atom (Stub 1 open). For empty-contract
        // sites the verdict is "refuse(no contract recovered)".
        let verdict = synthesize::discharge_for_shape(&lift.term_shape, &origin);

        // Canonical ConceptSiteMemento (schema v1, PR #692 / 5919e46f).
        //
        // Sites with no recovered contract are skipped: the spec requires
        // local_contract_cid to be present (§1.1), and an empty-contract
        // mint would produce a meaningless FunctionContractMemento. These
        // are recorded as "skip-emitted" in report §8.
        //
        // Provenance and discharge_receipt_cid are synthetic deterministic
        // hashes (no real MorphismDischargeReceipt or binary CIDs exist yet;
        // PR-B and PR-C are pending). See report §8 for loss accounting.
        let site_memento_cid = if contract_content_cid.is_some() {
            // Synthetic deterministic provenance CIDs.
            let lifter_cid = blake3_512_of(b"smoke-test-e2e-driver/lifter-v1");
            let clusterer_cid = blake3_512_of(b"smoke-test-e2e-driver/clusterer-v1");
            let discharger_cid = blake3_512_of(b"smoke-test-e2e-driver/discharger-v1");

            // source_cid: hash the actual file bytes of the source file.
            // The file path is relative to source_root; the src_dir is
            // source_root/src/. Re-read the content here to hash it.
            let source_file_path = source_root.join(&lift.file);
            let source_bytes = fs::read(&source_file_path).unwrap_or_default();
            let source_cid = blake3_512_of(&source_bytes);

            // span: approximate byte offsets from line number.
            // We know fn starts at lift.fn_line (1-based); scan to find
            // the byte offset of that line's start and end.
            let (span_start, span_end) = byte_span_for_line(&source_bytes, lift.fn_line);

            // function_term_cid and local_contract_cid: per spec §4 step 1/3,
            // these are the FunctionContractMemento content CID (not the
            // attestation/envelope CID). That is contract_content_cid.
            let local_cid = contract_content_cid.clone().expect("checked above");
            let fn_term_cid = local_cid.clone();

            // Build discharge block.
            let (d_verdict, d_loss, d_receipt, d_refusal) = match &verdict {
                DischargeVerdict::Exact => {
                    let receipt = blake3_512_of(
                        format!("smoke-test-discharge-receipt:{}", local_cid).as_bytes(),
                    );
                    (
                        "exact".to_string(),
                        LossRecord(std::collections::BTreeMap::new()),
                        Some(receipt),
                        None,
                    )
                }
                DischargeVerdict::LoudlyBoundedLossy { loss } => {
                    // Loss dimension: structural_divergence, characterized by
                    // an atomic formula naming the smoke-test single-atom encoding.
                    let receipt = blake3_512_of(
                        format!("smoke-test-discharge-receipt:{}", local_cid).as_bytes(),
                    );
                    let mut loss_map = std::collections::BTreeMap::new();
                    loss_map.insert(
                        "structural_divergence".to_string(),
                        IrFormula::Atomic {
                            name: format!("smoke-test-single-atom-encoding:{}", loss),
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
                    LossRecord(std::collections::BTreeMap::new()),
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

            // Build the header WITHOUT cid for JCS hashing.
            // Locked alphabetical key order per spec §3.1:
            //   code_site, concept_cid, discharge, kind, local_contract_cid,
            //   provenance, [realization_mode_hint,] schemaVersion, witnesses
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

            // discharge as Value — build in locked alphabetical order.
            // Note: Value::string/integer etc. return Arc<Value>.
            let mut discharge_kv: Vec<(&str, Arc<Value>)> = Vec::new();
            discharge_kv.push(("method", Value::string(discharge.method.clone())));
            if let Some(ref rr) = discharge.refusal_reason {
                discharge_kv.push(("refusal_reason", Value::string(rr.clone())));
            }
            discharge_kv.push(("verdict", Value::string(discharge.verdict.clone())));
            if let Some(ref drc) = discharge.discharge_receipt_cid {
                discharge_kv.push(("discharge_receipt_cid", Value::string(drc.clone())));
            }
            // loss_record: serialize via serde_json then convert to Arc<Value>
            let loss_json =
                serde_json::to_string(&discharge.loss_record).expect("LossRecord serialization");
            let loss_v = json_to_value(&serde_json::from_str(&loss_json).expect("parse loss_json"));
            discharge_kv.push(("loss_record", loss_v));
            let discharge_v = Value::object(discharge_kv);

            let provenance_v = Value::object([
                ("clusterer_cid", Value::string(clusterer_cid.clone())),
                ("discharger_cid", Value::string(discharger_cid.clone())),
                ("lifter_cid", Value::string(lifter_cid.clone())),
            ]);

            let abstraction_cid = concepts[concept_idx].abstraction_cid.clone();

            let mut header_kv: Vec<(&str, Arc<Value>)> = Vec::new();
            header_kv.push(("code_site", code_site_v));
            header_kv.push(("concept_cid", Value::string(abstraction_cid.clone())));
            header_kv.push(("discharge", discharge_v));
            header_kv.push(("kind", Value::string("concept-site".to_string())));
            header_kv.push(("local_contract_cid", Value::string(local_cid.clone())));
            header_kv.push(("provenance", provenance_v));
            // realization_mode_hint: omitted (discharger is silent in smoke test)
            header_kv.push(("schemaVersion", Value::string("1".to_string())));
            header_kv.push(("witnesses", Value::array(vec![])));

            let header_v = Value::object(header_kv);
            let computed_cid = blake3_512_of(encode_jcs(&header_v).as_bytes());

            // Build the full ConceptSiteMemento struct for artifact serialization.
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
                realization_mode_hint: None,
                schema_version: "1".to_string(),
                witnesses: vec![],
            };

            // §5.3 derived-CID round-trip check (real, not self-comparison).
            // Re-derive the CID from the assembled struct fields independently
            // to confirm the pre-stamp header_v hash equals the struct's own cid.
            let rederived_cid = {
                let cs_v = Value::object([
                    (
                        "function_term_cid",
                        Value::string(memento.code_site.function_term_cid.clone()),
                    ),
                    (
                        "source_cid",
                        Value::string(memento.code_site.source_cid.clone()),
                    ),
                    (
                        "span",
                        Value::object([
                            ("end", Value::integer(memento.code_site.span.end as i64)),
                            ("start", Value::integer(memento.code_site.span.start as i64)),
                        ]),
                    ),
                ]);
                let mut rd_discharge_kv: Vec<(&str, Arc<Value>)> = Vec::new();
                rd_discharge_kv.push(("method", Value::string(memento.discharge.method.clone())));
                if let Some(ref rr) = memento.discharge.refusal_reason {
                    rd_discharge_kv.push(("refusal_reason", Value::string(rr.clone())));
                }
                rd_discharge_kv.push(("verdict", Value::string(memento.discharge.verdict.clone())));
                if let Some(ref drc) = memento.discharge.discharge_receipt_cid {
                    rd_discharge_kv.push(("discharge_receipt_cid", Value::string(drc.clone())));
                }
                let rd_loss_json = serde_json::to_string(&memento.discharge.loss_record)
                    .expect("rederive LossRecord");
                let rd_loss_v =
                    json_to_value(&serde_json::from_str(&rd_loss_json).expect("rederive parse"));
                rd_discharge_kv.push(("loss_record", rd_loss_v));
                let rd_discharge_v = Value::object(rd_discharge_kv);
                let rd_prov_v = Value::object([
                    (
                        "clusterer_cid",
                        Value::string(memento.provenance.clusterer_cid.clone()),
                    ),
                    (
                        "discharger_cid",
                        Value::string(memento.provenance.discharger_cid.clone()),
                    ),
                    (
                        "lifter_cid",
                        Value::string(memento.provenance.lifter_cid.clone()),
                    ),
                ]);
                let mut rd_kv: Vec<(&str, Arc<Value>)> = Vec::new();
                rd_kv.push(("code_site", cs_v));
                rd_kv.push(("concept_cid", Value::string(memento.concept_cid.clone())));
                rd_kv.push(("discharge", rd_discharge_v));
                rd_kv.push(("kind", Value::string(memento.kind.clone())));
                rd_kv.push((
                    "local_contract_cid",
                    Value::string(memento.local_contract_cid.clone()),
                ));
                rd_kv.push(("provenance", rd_prov_v));
                rd_kv.push((
                    "schemaVersion",
                    Value::string(memento.schema_version.clone()),
                ));
                rd_kv.push(("witnesses", Value::array(vec![])));
                let rd_header_v = Value::object(rd_kv);
                blake3_512_of(encode_jcs(&rd_header_v).as_bytes())
            };

            // Inline §5.1-§5.3 validator.
            validate_concept_site_memento(
                &computed_cid,
                "concept-site",
                "1",
                &memento.discharge.verdict,
                &memento.discharge.loss_record,
                &memento.discharge.discharge_receipt_cid,
                &memento.discharge.refusal_reason,
                &rederived_cid, // §5.3: re-derived from struct fields, not a self-comparison
                &lift.fn_name,
            );

            let memento_json =
                serde_json::to_string_pretty(&memento).expect("ConceptSiteMemento serialization");
            let _ = fs::write(
                artifacts_dir.join(format!(
                    "pass{}_site_{}_{}.json",
                    pass_id,
                    sanitize(&lift.file),
                    lift.fn_name
                )),
                memento_json,
            );
            computed_cid
        } else {
            // No contract recovered: skip-emit. Record empty string as sentinel.
            // See report §8 for loss accounting.
            String::new()
        };

        let idx = bindings.len();
        concepts[concept_idx].site_indices.push(idx);
        bindings.push(BindingRecord {
            site_file: lift.file.clone(),
            site_fn: lift.fn_name.clone(),
            site_line: lift.fn_line,
            contract_cid,
            contract_content_cid,
            contract_origin: origin,
            shape_cid,
            concept_idx,
            site_memento_cid,
            discharge_verdict: verdict,
            pretty_pre: pre,
            pretty_post: post,
        });
    }

    // ---- Verb 8: WITNESS + PROPAGATE -------------------------------------
    //
    // For each `tests/properties.rs` assertion `assert!(<fn>(..) op ..)`,
    // attach an IrFormula witness to the *concept* (not just the site).
    // Then every other binding to that concept inherits the witness.
    let mut witnesses: Vec<WitnessRecord> = Vec::new();
    for (test_loc, fn_name, formula_text) in test_lift::collect_witnesses(&test_files) {
        // Find the concept_idx whose member set contains this fn.
        let mut concept_idx_opt = None;
        for (ci, c) in concepts.iter().enumerate() {
            for si in &c.site_indices {
                if bindings[*si].site_fn == fn_name {
                    concept_idx_opt = Some(ci);
                    break;
                }
            }
            if concept_idx_opt.is_some() {
                break;
            }
        }
        let Some(ci) = concept_idx_opt else { continue };
        let concept_shape = concepts[ci].shape_cid.clone();
        let propagated_to: Vec<usize> = concepts[ci].site_indices.clone();
        witnesses.push(WitnessRecord {
            concept_shape_cid: concept_shape,
            source_location: test_loc,
            pretty_formula: formula_text,
            propagated_to,
        });
    }

    PassResult {
        pass_id,
        bindings,
        concepts,
        witnesses,
    }
}

// --------------------------------------------------------------------------
// RawLift: intermediate per-fn record produced by the LIFT verb.
// --------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct RawLift {
    file: String,
    fn_name: String,
    fn_line: usize,
    attr_pre: Option<String>,
    attr_post: Option<String>,
    /// `// concept: <name>` pulled off the function-leading comment block.
    concept_annotation: Option<String>,
    term_shape: TermShape,
    #[allow(dead_code)]
    formula_shape: FormulaShape,
}

// --------------------------------------------------------------------------
// Helpers.
// --------------------------------------------------------------------------

fn list_rs_files(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    if !dir.exists() {
        return out;
    }
    let Ok(entries) = fs::read_dir(dir) else {
        return out;
    };
    for entry in entries.flatten() {
        let p = entry.path();
        if p.extension().and_then(|s| s.to_str()) == Some("rs") {
            out.push(p);
        }
    }
    out.sort();
    out
}

fn approximate_line_for_fn(src: &str, fn_name: &str) -> usize {
    let needle = format!("fn {}(", fn_name);
    for (i, line) in src.lines().enumerate() {
        if line.contains(&needle) {
            return i + 1;
        }
    }
    1
}

fn sanitize(s: &str) -> String {
    s.replace(['/', '\\', ':', '.', ' '], "_")
}

/// Convert a pretty-printed formula string into a kit Value tree.
///
/// Uses `provekit_ir_symbolic::parse_expr` to parse the predicate string
/// into a structured `Formula` AST, then serializes it via
/// `provekit_ir_symbolic::serialize::formula_to_value`. The result is
/// structurally identical to what the kit's authoring API (`gt`, `and_`,
/// etc.) would produce, enabling proper parse/serialize round-trips and
/// structural discharge.
///
/// Operator mapping:
///   `<`, `<=`, `>`, `>=`, `==`, `!=` map to the kit's atomic predicate
///   names (including Unicode ≤/≥/≠ per `lte`/`gte`/`ne`).
///
/// Falls back to the single-atom encoding for strings that cannot be
/// parsed (e.g. free-form prose), so existing behaviour is preserved for
/// non-expression contract text. The fallback is loudly labelled as lossy
/// via an eprintln warning.
fn formula_text_to_value(text: &str) -> Arc<Value> {
    match provekit_ir_symbolic::parse_expr::parse_expr(text) {
        Ok(formula) => provekit_ir_symbolic::serialize::formula_to_value(&formula),
        Err(e) => {
            eprintln!(
                "[smoke] formula_text_to_value: parse_expr({text:?}) failed ({e}); \
                 falling back to single-atom encoding (loudly-bounded-lossy)"
            );
            // Fallback: single-atom shim (same as the prior stub behaviour).
            Value::object([
                ("kind", Value::string("atomic")),
                ("name", Value::string(text.to_string())),
                ("args", Value::array(vec![])),
            ])
        }
    }
}
/// Given a file's bytes and a 1-based line number, return (start, end) byte
/// offsets for that line (exclusive end). Used to populate CodeSiteSpan.
/// Falls back to (0, 0) if the line number is out of range.
fn byte_span_for_line(bytes: &[u8], line_1based: usize) -> (u64, u64) {
    if line_1based == 0 || bytes.is_empty() {
        return (0, 0);
    }
    let mut line = 1usize;
    let mut start = 0usize;
    for (i, &b) in bytes.iter().enumerate() {
        if line == line_1based {
            // Scan forward to end of line.
            let end = bytes[i..]
                .iter()
                .position(|&c| c == b'\n')
                .map(|p| i + p + 1)
                .unwrap_or(bytes.len());
            return (start as u64, end as u64);
        }
        if b == b'\n' {
            line += 1;
            start = i + 1;
        }
    }
    (start as u64, bytes.len() as u64)
}

/// Convert a `serde_json::Value` into a `provekit_canonicalizer::Value` (Arc-wrapped).
/// Used to bridge the LossRecord serialization path.
fn json_to_value(v: &serde_json::Value) -> Arc<Value> {
    match v {
        serde_json::Value::Null => Value::null(),
        serde_json::Value::Bool(b) => Value::boolean(*b),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Value::integer(i)
            } else {
                Value::string(n.to_string())
            }
        }
        serde_json::Value::String(s) => Value::string(s.clone()),
        serde_json::Value::Array(arr) => Value::array(arr.iter().map(json_to_value).collect()),
        serde_json::Value::Object(map) => {
            let kv: Vec<(&str, Arc<Value>)> = map
                .iter()
                .map(|(k, v)| (k.as_str(), json_to_value(v)))
                .collect();
            Value::object(kv)
        }
    }
}

/// Inline §5.1-§5.3 validator for ConceptSiteMemento.
///
/// Panics if any invariant from spec §5.1 (CDDL shape), §5.2
/// (verdict-consistency), or §5.3 (derived CID) is violated. This
/// makes the smoke test a live conformance check against the spec.
#[allow(clippy::too_many_arguments)]
fn validate_concept_site_memento(
    computed_cid: &str,
    kind: &str,
    schema_version: &str,
    verdict: &str,
    loss_record: &LossRecord,
    discharge_receipt_cid: &Option<String>,
    refusal_reason: &Option<String>,
    header_cid: &str,
    site_label: &str,
) {
    // §5.1 CDDL shape check.
    let cid_re = regex_cid();
    assert_eq!(
        kind, "concept-site",
        "[§5.1] kind mismatch at {}",
        site_label
    );
    assert_eq!(
        schema_version, "1",
        "[§5.1] schemaVersion mismatch at {}",
        site_label
    );
    assert!(
        cid_re(computed_cid),
        "[§5.1] computed_cid is not a valid blake3-512 CID at {}",
        site_label
    );
    assert!(
        ["exact", "loudly-bounded-lossy", "refuse"].contains(&verdict),
        "[§5.1] invalid verdict '{}' at {}",
        verdict,
        site_label
    );

    // §5.2 verdict-consistency.
    match verdict {
        "exact" => {
            assert!(
                loss_record.0.is_empty(),
                "[§5.2] exact verdict requires empty loss_record at {}",
                site_label
            );
            assert!(
                discharge_receipt_cid.is_some(),
                "[§5.2] exact verdict requires discharge_receipt_cid at {}",
                site_label
            );
            assert!(
                refusal_reason.is_none(),
                "[§5.2] exact verdict must omit refusal_reason at {}",
                site_label
            );
        }
        "loudly-bounded-lossy" => {
            assert!(
                !loss_record.0.is_empty(),
                "[§5.2] loudly-bounded-lossy requires non-empty loss_record at {}",
                site_label
            );
            assert!(
                discharge_receipt_cid.is_some(),
                "[§5.2] loudly-bounded-lossy requires discharge_receipt_cid at {}",
                site_label
            );
            assert!(
                refusal_reason.is_none(),
                "[§5.2] loudly-bounded-lossy must omit refusal_reason at {}",
                site_label
            );
        }
        "refuse" => {
            assert!(
                discharge_receipt_cid.is_none(),
                "[§5.2] refuse verdict must omit discharge_receipt_cid at {}",
                site_label
            );
            assert!(
                refusal_reason
                    .as_deref()
                    .map(|s| !s.is_empty())
                    .unwrap_or(false),
                "[§5.2] refuse verdict requires non-empty refusal_reason at {}",
                site_label
            );
        }
        _ => unreachable!("already checked above"),
    }

    // §5.3 derived CID check.
    assert_eq!(
        computed_cid, header_cid,
        "[§5.3] CID mismatch: computed {} != header {} at {}",
        computed_cid, header_cid, site_label
    );
}

/// Returns a simple regex matcher for "blake3-512:" + 128 hex chars.
fn regex_cid() -> impl Fn(&str) -> bool {
    |s: &str| {
        s.starts_with("blake3-512:") && {
            let hex = &s["blake3-512:".len()..];
            hex.len() == 128 && hex.chars().all(|c| c.is_ascii_hexdigit())
        }
    }
}
