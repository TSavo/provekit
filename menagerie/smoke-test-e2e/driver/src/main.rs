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
//     Concept-shape mementos and concept-site mementos use a
//     locally-defined stub schema labelled "schemaVersion": "stub-0"
//     because the canonical ConceptSiteMemento spec PR has not yet
//     landed (see report §10 / open-questions).

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;

use provekit_canonicalizer::{blake3_512_of, encode_jcs, Value};
use provekit_claim_envelope::{mint_contract, Authoring, MintContractArgs};
use provekit_proof_envelope::Ed25519Seed;

mod algebra;
mod attrs;
mod cluster;
mod naming_roundtrip;
mod realize;
mod report;
mod synthesize;
mod test_lift;

use algebra::{TermShape, FormulaShape};

fn main() {
    let fixture_dir = locate_fixture_dir();
    eprintln!("[smoke] fixture dir: {}", fixture_dir.display());

    let pass_1 = run_pass(&fixture_dir, /*pass_id=*/ 1, /*read_concept_comments=*/ false);
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
    eprintln!("[smoke] pass 1 rewritten files at {}", rewritten_dir.display());

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
    let pass_2 = run_pass(&rewritten_dir, /*pass_id=*/ 2, /*read_concept_comments=*/ true);
    eprintln!(
        "[smoke] pass 2 complete: {} sites, {} concepts ({} unnamed)",
        pass_2.bindings.len(),
        pass_2.concepts.len(),
        pass_2.unnamed_count()
    );

    // Write the final report. The report is the substrate speaking.
    let report_path = fixture_dir.join("report.md");
    let report_md = report::render_report(
        &fixture_dir,
        &pass_1,
        &pass_2,
        renamed_pair.as_ref(),
    );
    fs::write(&report_path, report_md).expect("write report.md");
    eprintln!("[smoke] report written: {}", report_path.display());
}

/// Walk up from CARGO_MANIFEST_DIR and locate the fixture root.
fn locate_fixture_dir() -> PathBuf {
    // CARGO_MANIFEST_DIR for the driver crate is .../smoke-test-e2e/driver.
    let driver_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    driver_dir.parent().expect("driver has a parent").to_path_buf()
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
        self.concepts.iter().filter(|c| c.name.starts_with("UNNAMED-CONCEPT-")).count()
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
    /// Site-level memento CID for the concept:site binding (stub schema).
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
            ContractOrigin::AlgebraSynthesis { rule_id } => format!("algebra-synthesis[{}]", rule_id),
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
            DischargeVerdict::LoudlyBoundedLossy { loss } => format!("loudly-bounded-lossy({})", loss),
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

fn run_pass(
    source_root: &Path,
    pass_id: u32,
    read_concept_comments: bool,
) -> PassResult {
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
        let rel = path.strip_prefix(source_root).unwrap_or(path).display().to_string();
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
                (format!("shape:{}", shape_cid), String::new(), NameSource::Auto, None)
            };

        if !key_to_concept_idx.contains_key(&bucket_key) {
            let final_name = if source == NameSource::Auto {
                unnamed_counter += 1;
                format!("UNNAMED-CONCEPT-{}", unnamed_counter)
            } else {
                name_pick
            };

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
        let concept_name = concepts[concept_idx].name.clone();

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
        } else if let Some(test_post) = test_lift::lift_assertion_for_fn(&test_files, &lift.fn_name) {
            (
                ContractOrigin::TestLift,
                None,
                Some(test_post),
            )
        } else if let Some(rule) = synthesize::wp_rule_for_shape(&shape_cid, &lift.term_shape) {
            (
                ContractOrigin::AlgebraSynthesis { rule_id: rule.id.clone() },
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
        // The live libprovekit::wp evaluator is not yet stable enough for
        // this driver to drive end-to-end without per-rule wiring (the
        // wp-as-formula PR series is still landing). Per the
        // "loudly-bounded-incomplete" rule the verdict is computed by
        // a small structural-discharge oracle here and EXPLICITLY
        // labelled in the report as "structural-oracle" instead of
        // "live-wp" wherever the live evaluator was not used.
        let verdict = synthesize::discharge_for_shape(&lift.term_shape, &origin);

        // Site memento CID (stub schema; the canonical ConceptSiteMemento
        // PR has not landed yet, but the schema below matches the
        // emerging shape: kind / siteLocation / conceptShapeCid /
        // contractCid / dischargeVerdict).
        let site_v = Value::object([
            ("kind", Value::string("concept-site-stub-0")),
            ("schemaVersion", Value::string("stub-0")),
            ("siteFile", Value::string(lift.file.clone())),
            ("siteFn", Value::string(lift.fn_name.clone())),
            ("siteLine", Value::integer(lift.fn_line as i64)),
            ("conceptName", Value::string(concept_name.clone())),
            ("conceptShapeCid", Value::string(shape_cid.clone())),
            (
                "contractCid",
                Value::string(contract_cid.clone().unwrap_or_default()),
            ),
            (
                "contractOrigin",
                Value::string(origin.label()),
            ),
            ("dischargeVerdict", Value::string(verdict.label())),
        ]);
        let site_memento_cid = blake3_512_of(encode_jcs(&site_v).as_bytes());
        let _ = fs::write(
            artifacts_dir.join(format!(
                "pass{}_site_{}_{}.json",
                pass_id,
                sanitize(&lift.file),
                lift.fn_name
            )),
            encode_jcs(&site_v),
        );

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
    let Ok(entries) = fs::read_dir(dir) else { return out };
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
/// The kit's serializer expects a Formula AST; constructing one from a
/// free-form string would require a parser that mirrors
/// `provekit-ir-symbolic::parse`. To stay within the substrate and
/// avoid forking the kit parser, we wrap the string into a
/// single-atom Formula node:
///
///     {"kind":"atomic","name":"<text>","args":[]}
///
/// This is semantically lossy but is hashable and signable through the
/// existing mint API, which is sufficient for the smoke test's
/// transport demonstration. The lossiness is loudly labelled in
/// report §8 as the "smoke-test formula encoding" gap.
fn formula_text_to_value(text: &str) -> Arc<Value> {
    Value::object([
        ("kind", Value::string("atomic")),
        ("name", Value::string(text.to_string())),
        ("args", Value::array(vec![])),
    ])
}

// Allow the kit's Rc-based Formula type to be referenced; we don't
// construct it directly here but the import keeps the dependency
// edge visible.
#[allow(dead_code)]
fn _ensure_kit_link(_: Rc<provekit_ir_symbolic::Formula>) {}
