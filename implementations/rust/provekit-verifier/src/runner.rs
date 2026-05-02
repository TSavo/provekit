// SPDX-License-Identifier: Apache-2.0
//
// Verifier runner. Composes the seven stages and fans out per
// callsite via rayon. Stage 6 (solve) is now driven by the
// `solvers::run_plan` multi-solver layer (see
// `protocol/specs/2026-04-30-multi-solver-protocol.md`); the
// legacy single-Z3 path is preserved when no `.provekit/config.toml`
// is found.
//
// Stage 4 handshake is unchanged: Tier 1 (publisher-post hash ==
// consumer-pre hash, zero solver work) -> Tier 2 (signed implication
// memento in `cache_dir`) -> Tier 3 (the configured solver plan). On
// Tier 3 unsat we mint+cache a fresh implication memento PER SOLVER
// so the lattice records each independent witness.

use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;
use std::time::Duration;

use crate::formula_rewrite;

use rayon::prelude::*;
use serde_json::Value as Json;

use crate::handshake::{
    formula_hash, implication_property_hash, locate_producer_post, try_tier1, try_tier2,
};
use crate::solvers::{
    plan::SolverInvocation, registry, run_plan, SolverHandle, SolverPlan, SolversConfig,
};
use crate::types::{CallSite, MementoPool, ObligationVerdict, Report};
use crate::{
    enumerate_callsites, instantiate, load_all_proofs, report as report_stage, resolve_target,
    smt_emitter,
};

#[derive(Debug, Clone, Default)]
pub struct RunnerConfig {
    pub project_root: PathBuf,
    /// Legacy: path to z3 binary. Used as a fallback when no
    /// `.provekit/config.toml` `[solvers]` table is found. Existing
    /// examples and tests pass this directly.
    pub z3_path: String,
    /// Per-project implication-memento cache directory.
    pub cache_dir: Option<PathBuf>,
    /// Ed25519 seed used to sign minted implication mementos.
    pub mint_seed: Option<[u8; 32]>,
    /// Producer id stamped into minted implication mementos.
    pub mint_producer_id: Option<String>,
    /// Optional pre-loaded SolversConfig. If set, bypasses
    /// `.provekit/config.toml` discovery (used by tests and the
    /// multi-solver demo).
    pub solvers_config: Option<SolversConfig>,
}

/// Per-solver telemetry, surfaced in the report alongside the legacy
/// per-tier counters.
#[derive(Debug, Default, Clone)]
pub struct SolverStats {
    /// How many call sites this solver discharged (returned unsat).
    pub discharged: usize,
    /// How many call sites this solver returned sat (counterexample).
    pub unsatisfied: usize,
    /// How many call sites this solver returned unknown / parse-error.
    pub undecidable: usize,
    /// Subset of `undecidable`: returned because of timeout.
    pub timeouts: usize,
    /// Cumulative wall-clock spent in this solver across the run.
    pub wall_clock: Duration,
    /// Solver version string (as configured).
    pub version: String,
}

#[derive(Debug, Default, Clone)]
pub struct TierStats {
    pub discharged_by_hash: usize,
    pub discharged_by_cache: usize,
    pub vacuous_discharge: usize,
    pub solved_and_minted: usize,
    pub residue: usize,
    pub violations: usize,
    pub disagreements: usize,
    /// Cumulative number of solver invocations across all call sites.
    /// Replaces the old `z3_invocations` (kept as alias for back-compat).
    pub solver_invocations: usize,
    /// Per-solver breakdown.
    pub per_solver: BTreeMap<String, SolverStats>,
}

impl TierStats {
    /// Back-compat alias for the old `z3_invocations` counter.
    pub fn z3_invocations(&self) -> usize {
        self.solver_invocations
    }
}

pub struct Runner {
    cfg: RunnerConfig,
    plan: SolverPlan,
    registry: HashMap<String, SolverHandle>,
}

impl Runner {
    pub fn new(cfg: RunnerConfig) -> Self {
        // Resolve solver config. Precedence:
        //   1. cfg.solvers_config (test/demo override)
        //   2. .provekit/config.toml under project_root
        //   3. fallback: single Z3 at cfg.z3_path
        let (plan, registry) = build_plan_and_registry(&cfg);
        Self {
            cfg,
            plan,
            registry,
        }
    }

    pub fn run(&self) -> Report {
        let (report, _stats) = self.run_with_tiers();
        report
    }

    pub fn run_with_tiers(&self) -> (Report, TierStats) {
        let mut report = Report::default();
        let mut pool = load_all_proofs::run(&self.cfg.project_root);
        let callsites = enumerate_callsites::run(&pool);

        let n_hash = AtomicUsize::new(0);
        let n_cache = AtomicUsize::new(0);
        let n_vacuous = AtomicUsize::new(0);
        let n_solved = AtomicUsize::new(0);
        let n_residue = AtomicUsize::new(0);
        let n_disagree = AtomicUsize::new(0);
        let n_invoc = AtomicUsize::new(0);

        // Per-solver telemetry sink. Mutex-guarded; rayon workers append
        // their per-callsite SolverInvocations here.
        let invs_sink: Mutex<Vec<SolverInvocation>> = Mutex::new(vec![]);

        let cfg = &self.cfg;
        let plan = &self.plan;
        let registry = &self.registry;

        let minted_sink = Mutex::new(Vec::new());
        let per_results: Vec<(CallSite, ObligationVerdict, String)> = callsites
            .par_iter()
            .map(|cs| {
                work_one(
                    cs,
                    &pool,
                    plan,
                    registry,
                    cfg,
                    &n_hash,
                    &n_cache,
                    &n_vacuous,
                    &n_solved,
                    &n_residue,
                    &n_disagree,
                    &n_invoc,
                    &invs_sink,
                    &minted_sink,
                )
            })
            .collect();

        // Insert freshly minted implication mementos into the pool
        // so subsequent stages can use them immediately.
        if let Ok(minted) = minted_sink.lock() {
            for (cid, envelope) in minted.iter() {
                pool.insert(cid.clone(), envelope.clone());
            }
        }

        // Aggregate report rows.
        let mut violations = 0usize;
        for (cs, verdict, reason) in per_results {
            if verdict != ObligationVerdict::Discharged {
                violations += 1;
            }
            report_stage::add_callsite(&cs, verdict, &reason, &mut report);
        }
        report_stage::add_load_errors(&pool.load_errors, &mut report);

        // Aggregate per-solver stats from telemetry sink.
        let invs = invs_sink.into_inner().unwrap_or_default();
        let mut per_solver: BTreeMap<String, SolverStats> = BTreeMap::new();
        for inv in &invs {
            let r = &inv.result;
            let entry = per_solver.entry(r.solver_name.clone()).or_default();
            entry.version = r.solver_version.clone();
            entry.wall_clock += r.wall_clock;
            match r.verdict {
                ObligationVerdict::Discharged => entry.discharged += 1,
                ObligationVerdict::Unsatisfied => entry.unsatisfied += 1,
                ObligationVerdict::Undecidable => entry.undecidable += 1,
                ObligationVerdict::Disagreement => entry.undecidable += 1,
            }
            if r.timed_out {
                entry.timeouts += 1;
            }
        }

        let stats = TierStats {
            discharged_by_hash: n_hash.load(Ordering::Relaxed),
            discharged_by_cache: n_cache.load(Ordering::Relaxed),
            vacuous_discharge: n_vacuous.load(Ordering::Relaxed),
            solved_and_minted: n_solved.load(Ordering::Relaxed),
            residue: n_residue.load(Ordering::Relaxed),
            violations,
            disagreements: n_disagree.load(Ordering::Relaxed),
            solver_invocations: n_invoc.load(Ordering::Relaxed),
            per_solver,
        };
        (report, stats)
    }

    pub fn run_load_and_enumerate(&self) -> (MementoPool, Vec<CallSite>) {
        let pool = load_all_proofs::run(&self.cfg.project_root);
        let cs = enumerate_callsites::run(&pool);
        (pool, cs)
    }

    pub fn plan(&self) -> &SolverPlan {
        &self.plan
    }
}

fn build_plan_and_registry(
    cfg: &RunnerConfig,
) -> (SolverPlan, HashMap<String, SolverHandle>) {
    if let Some(sc) = &cfg.solvers_config {
        return (SolverPlan::from_config(sc), registry::build(sc));
    }
    if let Ok(Some(sc)) = SolversConfig::load(&cfg.project_root) {
        return (SolverPlan::from_config(&sc), registry::build(&sc));
    }
    // Fallback: legacy single-Z3 plan.
    let z3 = if cfg.z3_path.is_empty() {
        "z3".to_string()
    } else {
        cfg.z3_path.clone()
    };
    (
        SolverPlan::Single("z3".into()),
        registry::build_default_z3(&z3),
    )
}

#[allow(clippy::too_many_arguments)]
fn work_one(
    cs: &CallSite,
    pool: &MementoPool,
    plan: &SolverPlan,
    registry: &HashMap<String, SolverHandle>,
    cfg: &RunnerConfig,
    n_hash: &AtomicUsize,
    n_cache: &AtomicUsize,
    n_vacuous: &AtomicUsize,
    n_solved: &AtomicUsize,
    n_residue: &AtomicUsize,
    n_disagree: &AtomicUsize,
    n_invoc: &AtomicUsize,
    invs_sink: &Mutex<Vec<SolverInvocation>>,
    minted_sink: &Mutex<Vec<(String, Json)>>,
) -> (CallSite, ObligationVerdict, String) {
    let resolved = match resolve_target::run(cs, pool) {
        Ok(r) => r,
        Err(e) => {
            n_residue.fetch_add(1, Ordering::Relaxed);
            return (
                cs.clone(),
                ObligationVerdict::Undecidable,
                format!("resolve-target: {e}"),
            );
        }
    };

    if resolved.ir_formula.is_none() {
        n_vacuous.fetch_add(1, Ordering::Relaxed);
        return (
            cs.clone(),
            ObligationVerdict::Discharged,
            "vacuous: no precondition on target (publisher post-only)".into(),
        );
    }

    let consumer_pre = resolved.ir_formula.as_ref();
    let consumer_pre_hash = consumer_pre.map(formula_hash);
    let producer_post = locate_producer_post(
        &cs.arg_term,
        &pool.mementos,
        &pool.bridges_by_symbol,
    );

    // Tier 0: Memento IS verification. Look up the formula CID in the pool.
    // The hash IS the boundary: we verify by hash lookup, not by solving.
    if let Some(pre_formula) = consumer_pre {
        if let Some(memento) = pool.verify(pre_formula) {
            n_hash.fetch_add(1, Ordering::Relaxed);
            let memento_cid = memento.get("cid").and_then(|v| v.as_str()).unwrap_or("unknown");
            return (
                cs.clone(),
                ObligationVerdict::Discharged,
                format!("tier0: memento-is-verification (cid={})", short(memento_cid)),
            );
        }
        
        // Tier 0b: Sub-formula composition. If parts of the formula are
        // already verified, note them for partial discharge.
        let verified_subs = pool.find_verified_subformulas(pre_formula);
        if !verified_subs.is_empty() {
            // TODO: In v1, use verified_subs to build a reduced obligation
            // for the solver. For now, we just note it in telemetry.
            let sub_cids: Vec<String> = verified_subs.into_iter().map(|(cid, _)| short(&cid)).collect();
            eprintln!("info: formula has {} verified sub-formulas: {}", sub_cids.len(), sub_cids.join(", "));
        }
    }

    if let (Some(pre_hash), Some((_post_formula, post_hash))) =
        (consumer_pre_hash.as_ref(), producer_post.as_ref())
    {
        // Tier 0c: Implication composition. Is postA → preB already
        // proven in the memento pool? Direct or transitive?
        match pool.can_implies(post_hash, pre_hash) {
            crate::types::ImplicationResult::ProvenDirect { memento_cid } => {
                n_hash.fetch_add(1, Ordering::Relaxed);
                return (
                    cs.clone(),
                    ObligationVerdict::Discharged,
                    format!("tier0c: implication proven direct (memento {})", short(&memento_cid)),
                );
            }
            crate::types::ImplicationResult::ProvenTransitive { path } => {
                n_hash.fetch_add(1, Ordering::Relaxed);
                let path_str = path.iter().map(|s| short(s)).collect::<Vec<_>>().join(" → ");
                return (
                    cs.clone(),
                    ObligationVerdict::Discharged,
                    format!("tier0c: implication proven transitive ({path_str})"),
                );
            }
            crate::types::ImplicationResult::ProvenReflexive => {
                n_hash.fetch_add(1, Ordering::Relaxed);
                return (
                    cs.clone(),
                    ObligationVerdict::Discharged,
                    "tier0c: implication reflexive (post == pre)".into(),
                );
            }
            crate::types::ImplicationResult::Unknown => {}
        }

        if try_tier1(post_hash, pre_hash) {
            n_hash.fetch_add(1, Ordering::Relaxed);
            return (
                cs.clone(),
                ObligationVerdict::Discharged,
                format!("tier1: hash equality (post == pre, hash={})", short(pre_hash)),
            );
        }
        if let Some(cache_dir) = &cfg.cache_dir {
            if let Some(impl_cid) = try_tier2(cache_dir, post_hash, pre_hash) {
                n_cache.fetch_add(1, Ordering::Relaxed);
                return (
                    cs.clone(),
                    ObligationVerdict::Discharged,
                    format!("tier2: cache hit (implication memento {})", short(&impl_cid)),
                );
            }
        }
    }

    // Tier 3: build SMT-LIB and run the configured plan.
    let smt: String;
    let formula_for_dispatch: Option<Json>;
    let used_implication_form: bool;

    if let (Some((post_formula, _)), Some(pre_formula)) =
        (producer_post.as_ref(), consumer_pre)
    {
        used_implication_form = true;
        let implication = match build_implication_obligation(post_formula, pre_formula) {
            Ok(f) => f,
            Err(e) => {
                n_residue.fetch_add(1, Ordering::Relaxed);
                return (
                    cs.clone(),
                    ObligationVerdict::Undecidable,
                    format!("build-implication: {e}"),
                );
            }
        };

        // Tier 3a: Apply proof tactics before invoking solver.
        // Contrapositive, sub-formula weakening, etc.
        match formula_rewrite::apply_tactics(&implication, pool) {
            formula_rewrite::TacticResult::Discharged { reason } => {
                n_solved.fetch_add(1, Ordering::Relaxed);
                return (
                    cs.clone(),
                    ObligationVerdict::Discharged,
                    format!("tier3a: tactic discharged ({reason})"),
                );
            }
            formula_rewrite::TacticResult::Reduced { new_formula, reason: _ } => {
                // Use the reduced formula for SMT emission
                smt = match smt_emitter::emit(&new_formula) {
                    Ok(s) => s,
                    Err(e) => {
                        n_residue.fetch_add(1, Ordering::Relaxed);
                        return (
                            cs.clone(),
                            ObligationVerdict::Undecidable,
                            format!("smt-emit: {e}"),
                        );
                    }
                };
                formula_for_dispatch = Some(new_formula);
                // Continue to solver with reduced formula
            }
            formula_rewrite::TacticResult::NoChange => {
                smt = match smt_emitter::emit(&implication) {
                    Ok(s) => s,
                    Err(e) => {
                        n_residue.fetch_add(1, Ordering::Relaxed);
                        return (
                            cs.clone(),
                            ObligationVerdict::Undecidable,
                            format!("smt-emit: {e}"),
                        );
                    }
                };
                formula_for_dispatch = Some(implication);
            }
        }
    } else {
        used_implication_form = false;
        let ob = match instantiate::run(&resolved, &cs.arg_term) {
            Ok(o) => o,
            Err(e) => {
                n_residue.fetch_add(1, Ordering::Relaxed);
                return (
                    cs.clone(),
                    ObligationVerdict::Undecidable,
                    format!("instantiate: {e}"),
                );
            }
        };
        smt = match smt_emitter::emit(&ob.ir_formula) {
            Ok(s) => s,
            Err(e) => {
                n_residue.fetch_add(1, Ordering::Relaxed);
                return (
                    cs.clone(),
                    ObligationVerdict::Undecidable,
                    format!("smt-emit: {e}"),
                );
            }
        };
        formula_for_dispatch = Some(ob.ir_formula);
    }

    let (verdict, reason, invs) =
        run_plan(plan, registry, &smt, formula_for_dispatch.as_ref());

    n_invoc.fetch_add(invs.len(), Ordering::Relaxed);

    if verdict == ObligationVerdict::Disagreement {
        n_disagree.fetch_add(1, Ordering::Relaxed);
        n_residue.fetch_add(1, Ordering::Relaxed);
    }

    // Mint per-solver mementos for every solver that returned unsat
    // when the implication form was used.
    if used_implication_form {
        if let (Some(post_hash), Some(pre_hash), Some(cache_dir), Some(seed), Some(producer)) = (
            producer_post.as_ref().map(|(_, h)| h.clone()),
            consumer_pre_hash.clone(),
            cfg.cache_dir.as_ref(),
            cfg.mint_seed.as_ref(),
            cfg.mint_producer_id.as_ref(),
        ) {
            for inv in &invs {
                if inv.result.verdict == ObligationVerdict::Discharged {
                    let prover_tag = format!(
                        "{}@{}",
                        inv.result.solver_name, inv.result.solver_version
                    );
                    match mint_and_cache(
                        cache_dir,
                        seed,
                        producer,
                        &post_hash,
                        &pre_hash,
                        producer_post.as_ref().map(|(p, _)| p.clone()),
                        consumer_pre.cloned(),
                        &smt,
                        &prover_tag,
                        inv.result.wall_clock.as_millis() as i64,
                    ) {
                        Ok((cid, envelope)) => {
                            // Queue for insertion into pool after parallel
                            // work completes (pool is not Sync).
                            if let Ok(mut g) = minted_sink.lock() {
                                g.push((cid, envelope));
                            }
                        }
                        Err(e) => {
                            eprintln!("warning: mint_and_cache: {e}");
                        }
                    }
                }
            }
        }
    }

    if verdict == ObligationVerdict::Discharged && used_implication_form {
        n_solved.fetch_add(1, Ordering::Relaxed);
    }
    if verdict != ObligationVerdict::Discharged && verdict != ObligationVerdict::Disagreement {
        n_residue.fetch_add(1, Ordering::Relaxed);
    }

    // Push telemetry into the sink.
    if let Ok(mut g) = invs_sink.lock() {
        g.extend(invs);
    }

    (cs.clone(), verdict, reason)
}

fn short(s: &str) -> String {
    let cleaned = s.trim_start_matches("blake3-512:");
    let take: String = cleaned.chars().take(12).collect();
    format!("blake3-512:{take}...")
}

fn build_implication_obligation(
    post_formula: &Json,
    pre_formula: &Json,
) -> Result<Json, String> {
    let post_obj = post_formula
        .as_object()
        .ok_or("post is not an object")?;
    let pre_obj = pre_formula.as_object().ok_or("pre is not an object")?;
    if post_obj.get("kind").and_then(|v| v.as_str()) != Some("forall") {
        return Err("post is not a forall".into());
    }
    if pre_obj.get("kind").and_then(|v| v.as_str()) != Some("forall") {
        return Err("pre is not a forall".into());
    }
    let post_name = post_obj
        .get("name")
        .and_then(|v| v.as_str())
        .ok_or("post forall name missing")?;
    let pre_name = pre_obj
        .get("name")
        .and_then(|v| v.as_str())
        .ok_or("pre forall name missing")?;
    let sort = post_obj
        .get("sort")
        .cloned()
        .unwrap_or_else(|| {
            pre_obj
                .get("sort")
                .cloned()
                .unwrap_or_else(|| serde_json::json!({"kind":"primitive","name":"Int"}))
        });
    let post_body = post_obj.get("body").cloned().ok_or("post body missing")?;
    let pre_body = pre_obj.get("body").cloned().ok_or("pre body missing")?;

    let shared = "_h0";
    let replacement = serde_json::json!({"kind": "var", "name": shared});
    let post_body_renamed =
        crate::instantiate::substitute_formula_pub(&post_body, post_name, &replacement);
    let pre_body_renamed =
        crate::instantiate::substitute_formula_pub(&pre_body, pre_name, &replacement);

    Ok(serde_json::json!({
        "kind": "forall",
        "name": shared,
        "sort": sort,
        "body": {
            "kind": "implies",
            "operands": [post_body_renamed, pre_body_renamed]
        }
    }))
}

#[allow(clippy::too_many_arguments)]
/// Mint an implication memento and cache it to disk.
/// Returns (cid, envelope_json) so the caller can insert into the pool.
#[allow(clippy::too_many_arguments)]
fn mint_and_cache(
    cache_dir: &std::path::Path,
    seed: &[u8; 32],
    producer_id: &str,
    post_hash: &str,
    pre_hash: &str,
    post_formula: Option<Json>,
    pre_formula: Option<Json>,
    smt_lib_input: &str,
    prover_tag: &str,
    prover_run_ms: i64,
) -> Result<(String, Json), Box<dyn std::error::Error>> {
    use provekit_canonicalizer::{blake3_512_of, encode_jcs, Value};
    use provekit_proof_envelope::{
        build_proof_envelope, ed25519_pubkey_string, ed25519_sign_string, ProofEnvelopeInput,
    };

    std::fs::create_dir_all(cache_dir)?;
    let now = "2026-04-30T00:00:00.000Z";
    let pubkey = ed25519_pubkey_string(seed);
    let _ = (post_formula, pre_formula);

    let mut body_kvs: Vec<(String, std::sync::Arc<Value>)> = vec![
        ("antecedentHash".into(), Value::string(post_hash.to_string())),
        ("consequentHash".into(), Value::string(pre_hash.to_string())),
        ("antecedentCid".into(), Value::string("blake3-512:0000".to_string())),
        ("consequentCid".into(), Value::string("blake3-512:0000".to_string())),
        ("antecedentSlot".into(), Value::string("post".to_string())),
        ("consequentSlot".into(), Value::string("pre".to_string())),
        ("prover".into(), Value::string(prover_tag.to_string())),
        ("proverRunMs".into(), Value::integer(prover_run_ms)),
        ("producerPubkey".into(), Value::string(pubkey.clone())),
    ];
    if !smt_lib_input.is_empty() {
        body_kvs.push((
            "smtLibInput".into(),
            Value::string(smt_lib_input.to_string()),
        ));
    }
    body_kvs.push(("proofWitness".into(), Value::string("(unsat)".to_string())));

    let body = std::sync::Arc::new(Value::Object(body_kvs));

    let evidence = Value::object([
        ("kind", Value::string("implication")),
        (
            "schema",
            Value::string(
                "blake3-512:00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000c08",
            ),
        ),
        ("body", body),
    ]);

    let bh = Value::object([
        ("antecedentHash", Value::string(post_hash.to_string())),
        ("consequentHash", Value::string(pre_hash.to_string())),
    ]);
    let binding_hash = blake3_512_of(encode_jcs(&bh).as_bytes());
    let property_hash = implication_property_hash(post_hash, pre_hash);

    let mut input_cids = vec![
        "blake3-512:0000".to_string(),
        "blake3-512:0000".to_string(),
    ];
    input_cids.sort();
    let cids_arr: Vec<std::sync::Arc<Value>> =
        input_cids.into_iter().map(Value::string).collect();

    let unsigned_v = Value::object([
        ("schemaVersion", Value::string("1")),
        ("bindingHash", Value::string(binding_hash)),
        ("propertyHash", Value::string(property_hash.clone())),
        ("verdict", Value::string("holds")),
        ("producedBy", Value::string(producer_id.to_string())),
        ("producedAt", Value::string(now.to_string())),
        ("inputCids", Value::array(cids_arr)),
        ("evidence", evidence),
    ]);
    let unsigned_canonical = encode_jcs(&unsigned_v);
    let cid = blake3_512_of(unsigned_canonical.as_bytes());
    let producer_sig = ed25519_sign_string(seed, unsigned_canonical.as_bytes());

    let mut entries = match unsigned_v.as_ref() {
        Value::Object(kvs) => kvs.clone(),
        _ => unreachable!("envelope is an object"),
    };
    entries.push(("cid".into(), Value::string(cid.clone())));
    entries.push((
        "producerSignature".into(),
        Value::string(producer_sig),
    ));
    let signed_v = std::sync::Arc::new(Value::Object(entries));
    let final_canonical = encode_jcs(&signed_v).into_bytes();

    let mut members: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    members.insert(cid.clone(), final_canonical.clone());
    let signer_cid = blake3_512_of(pubkey.as_bytes());
    // Encode prover_tag into the filename to disambiguate per-solver
    // mementos for the same (antecedent, consequent) pair.
    let safe_prover: String = prover_tag
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    let proof_input = ProofEnvelopeInput {
        name: format!(
            "@cache/implication-{}-{}",
            short(&property_hash),
            safe_prover
        ),
        version: "1.0.0".into(),
        binary_cid: None,
        metadata: None,
        members,
        signer_cid,
        signer_seed: *seed,
        declared_at: now.into(),
    };
    let built = build_proof_envelope(&proof_input);

    let fname = format!("{}-{}.proof", property_hash, safe_prover);
    let path = cache_dir.join(fname);
    std::fs::write(path, built.bytes)?;
    
    // Convert the canonicalizer Value back to serde_json for pool insertion
    let envelope_json: Json = serde_json::from_slice(&final_canonical)?;
    
    Ok((cid, envelope_json))
}
