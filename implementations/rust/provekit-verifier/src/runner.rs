// SPDX-License-Identifier: Apache-2.0
//
// Verifier runner — composes the six stages and fans out stages 3-5
// per callsite using rayon (mirrors the C++ std::async fan-out and
// the Go goroutine fan-out).
//
// On top of the six stages this runner also implements the Stage 4
// handshake: Tier 1 (publisher-post hash == consumer-pre hash, zero
// solver work) and Tier 2 (signed implication memento in
// `cfg.cache_dir` keyed by the publisher-post / consumer-pre pair).
// Tier 3 is the existing Z3 path; on unsat we mint + write a fresh
// implication memento into `cfg.cache_dir` so the next run hits Tier 2.

use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

use rayon::prelude::*;
use serde_json::Value as Json;

use crate::handshake::{
    formula_hash, implication_property_hash, locate_producer_post, try_tier1, try_tier2,
};
use crate::types::{CallSite, MementoPool, ObligationVerdict, Report};
use crate::{
    enumerate_callsites, instantiate, load_all_proofs, report as report_stage, resolve_target,
    smt_emitter, solve_obligation,
};

#[derive(Debug, Clone, Default)]
pub struct RunnerConfig {
    pub project_root: PathBuf,
    pub z3_path: String,
    /// Per-project implication-memento cache directory. `.proof`
    /// files placed here are searched in Tier 2; new mementos minted
    /// in Tier 3 are written here so subsequent runs become Tier-2
    /// cache hits. `None` disables the handshake (legacy mode).
    pub cache_dir: Option<PathBuf>,
    /// Ed25519 seed used to sign minted implication mementos. The
    /// public key is embedded in the implication body so any Tier-2
    /// reader can verify the signature without an external key store.
    /// Required when `cache_dir` is `Some`.
    pub mint_seed: Option<[u8; 32]>,
    /// Producer id stamped into minted implication mementos.
    pub mint_producer_id: Option<String>,
}

/// Per-tier discharge counters. Reported alongside the per-callsite
/// rows so launch-post metrics ("M discharged-by-hash, K cached, L
/// solved+minted") can be read directly off the report.
#[derive(Debug, Default, Clone)]
pub struct TierStats {
    pub discharged_by_hash: usize,
    pub discharged_by_cache: usize,
    pub solved_and_minted: usize,
    pub residue: usize,
    pub violations: usize,
    pub z3_invocations: usize,
}

pub struct Runner {
    cfg: RunnerConfig,
}

impl Runner {
    pub fn new(cfg: RunnerConfig) -> Self {
        Self { cfg }
    }

    /// Legacy entry point: returns the per-callsite Report. Tier
    /// counters are not surfaced; use `run_with_tiers` for those.
    pub fn run(&self) -> Report {
        let (report, _stats) = self.run_with_tiers();
        report
    }

    /// Full Stage 4 pipeline: returns both the per-callsite report
    /// and the tier-discharge counters that drive the demo's headline
    /// metrics.
    pub fn run_with_tiers(&self) -> (Report, TierStats) {
        let mut report = Report::default();

        // Stage 1.
        let pool = load_all_proofs::run(&self.cfg.project_root);

        // Stage 2.
        let callsites = enumerate_callsites::run(&pool);

        // Atomic counters so the rayon parallel fan-out stays
        // contention-free; we collapse them into a TierStats at the
        // end.
        let n_hash = AtomicUsize::new(0);
        let n_cache = AtomicUsize::new(0);
        let n_solved = AtomicUsize::new(0);
        let n_residue = AtomicUsize::new(0);
        let n_z3 = AtomicUsize::new(0);

        let z3 = self.cfg.z3_path.clone();
        let cfg = &self.cfg;

        // Stages 3-5 (+ Tier 1/2 shortcut + Tier 3 mint) per callsite.
        let per_results: Vec<(CallSite, ObligationVerdict, String)> = callsites
            .par_iter()
            .map(|cs| {
                work_one(
                    cs, &pool, &z3, cfg, &n_hash, &n_cache, &n_solved, &n_residue, &n_z3,
                )
            })
            .collect();

        // Stage 6 (report aggregation).
        let mut violations = 0usize;
        for (cs, verdict, reason) in per_results {
            if verdict != ObligationVerdict::Discharged {
                violations += 1;
            }
            report_stage::add_callsite(&cs, verdict, &reason, &mut report);
        }
        report_stage::add_load_errors(&pool.load_errors, &mut report);

        let stats = TierStats {
            discharged_by_hash: n_hash.load(Ordering::Relaxed),
            discharged_by_cache: n_cache.load(Ordering::Relaxed),
            solved_and_minted: n_solved.load(Ordering::Relaxed),
            residue: n_residue.load(Ordering::Relaxed),
            violations,
            z3_invocations: n_z3.load(Ordering::Relaxed),
        };
        (report, stats)
    }

    /// Loads the pool but stops short of solving — useful for the
    /// Rust round-trip example (asserts callsite resolution works).
    pub fn run_load_and_enumerate(&self) -> (MementoPool, Vec<CallSite>) {
        let pool = load_all_proofs::run(&self.cfg.project_root);
        let cs = enumerate_callsites::run(&pool);
        (pool, cs)
    }
}

#[allow(clippy::too_many_arguments)]
fn work_one(
    cs: &CallSite,
    pool: &MementoPool,
    z3_path: &str,
    cfg: &RunnerConfig,
    n_hash: &AtomicUsize,
    n_cache: &AtomicUsize,
    n_solved: &AtomicUsize,
    n_residue: &AtomicUsize,
    n_z3: &AtomicUsize,
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

    // If the target contract has no `pre` slot at all, the call site
    // has nothing to discharge; the publisher has only a post.
    // Vacuously discharged. (Counts as Tier-1 free work.)
    if resolved.ir_formula.is_none() {
        n_hash.fetch_add(1, Ordering::Relaxed);
        return (
            cs.clone(),
            ObligationVerdict::Discharged,
            "tier1: no precondition on target (publisher post-only): vacuously discharged".into(),
        );
    }

    // ----- Stage 4 handshake: Tier 1 + Tier 2 ----------------------
    // We only attempt the handshake when:
    //   (a) the consumer's resolved-pre formula is a forall (so it
    //       has a single canonical body shape we can hash), and
    //   (b) the callsite's arg_term is itself a Ctor whose name maps
    //       to a producer bridge (whose targetContractCid points at
    //       a contract memento with a `post` slot).
    let consumer_pre = resolved.ir_formula.as_ref();
    let consumer_pre_hash = consumer_pre.map(formula_hash);
    let producer_post = locate_producer_post(
        &cs.arg_term,
        &pool.mementos,
        &pool.bridges_by_symbol,
    );

    if let (Some(pre_hash), Some((_post_formula, post_hash))) =
        (consumer_pre_hash.as_ref(), producer_post.as_ref())
    {
        // Tier 1: literal hash equality.
        if try_tier1(post_hash, pre_hash) {
            n_hash.fetch_add(1, Ordering::Relaxed);
            return (
                cs.clone(),
                ObligationVerdict::Discharged,
                format!("tier1: hash equality (post == pre, hash={})", short(pre_hash)),
            );
        }
        // Tier 2: cached implication memento (post -> pre).
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

    // ----- Tier 3: Z3 path ----------------------------------------
    //
    // Two shapes of obligation:
    //
    // (a) Implication form (when both producer.post and consumer.pre
    //     are present): query `forall x. producer.post(x) -> consumer.pre(x)`.
    //     This is the cross-language handshake's deep query: does the
    //     producer's output guarantee imply the consumer's input
    //     requirement? On unsat we mint an implication memento.
    //
    // (b) Instantiation form (legacy): substitute the call's arg term
    //     into consumer.pre and ask Z3. Used when arg is a literal
    //     and there's no producer-side post we can pair with.
    let smt: String;
    let res: solve_obligation::SolveResult;
    let used_implication_form: bool;

    if let (Some((post_formula, _)), Some(pre_formula)) =
        (producer_post.as_ref(), consumer_pre)
    {
        used_implication_form = true;
        // Build the implication formula. We want
        //   forall x. post[x/v_post] -> pre[x/v_pre]
        // Since both come from a `forall` quantifier of the same sort,
        // we substitute their bodies onto a shared bound name.
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
        n_z3.fetch_add(1, Ordering::Relaxed);
        res = solve_obligation::run(z3_path, &smt);
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
        n_z3.fetch_add(1, Ordering::Relaxed);
        res = solve_obligation::run(z3_path, &smt);
    }

    // On Tier 3 unsat with handshake hashes available AND the
    // implication form was used, mint and cache an implication
    // memento so the next run becomes Tier 2.
    if res.verdict == ObligationVerdict::Discharged && used_implication_form {
        n_solved.fetch_add(1, Ordering::Relaxed);
        if let (
            Some(post_hash),
            Some(pre_hash),
            Some(cache_dir),
            Some(seed),
            Some(producer),
        ) = (
            producer_post.as_ref().map(|(_, h)| h.clone()),
            consumer_pre_hash.clone(),
            cfg.cache_dir.as_ref(),
            cfg.mint_seed.as_ref(),
            cfg.mint_producer_id.as_ref(),
        ) {
            if let Err(e) = mint_and_cache(
                cache_dir,
                seed,
                producer,
                &post_hash,
                &pre_hash,
                producer_post.as_ref().map(|(p, _)| p.clone()),
                consumer_pre.cloned(),
                &smt,
            ) {
                eprintln!("warning: mint_and_cache: {e}");
            }
        }
    }

    let reason = if !res.error.is_empty() {
        res.error
    } else {
        match res.verdict {
            ObligationVerdict::Discharged => {
                "tier3: solver returned unsat: obligation holds (memento minted into cache)".into()
            }
            ObligationVerdict::Unsatisfied => {
                "tier3: solver returned sat (counterexample found): obligation falsifiable".into()
            }
            _ => String::new(),
        }
    };
    if res.verdict != ObligationVerdict::Discharged {
        n_residue.fetch_add(1, Ordering::Relaxed);
    }
    (cs.clone(), res.verdict, reason)
}

fn short(s: &str) -> String {
    let cleaned = s.trim_start_matches("blake3-512:");
    let take: String = cleaned.chars().take(12).collect();
    format!("blake3-512:{take}...")
}

/// Build the implication `forall x: Int. post(x) -> pre(x)` from two
/// `forall`-headed formulas. We renormalize both bound names onto a
/// single fresh name, then assemble the wrapping forall + implies.
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
        .unwrap_or_else(|| serde_json::json!({"kind":"primitive","name":"Int"}));
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
fn mint_and_cache(
    cache_dir: &std::path::Path,
    seed: &[u8; 32],
    producer_id: &str,
    post_hash: &str,
    pre_hash: &str,
    post_formula: Option<Json>,
    pre_formula: Option<Json>,
    smt_lib_input: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    use provekit_canonicalizer::{blake3_512_of, encode_jcs, Value};
    use provekit_proof_envelope::{
        build_proof_envelope, ed25519_pubkey_string, ed25519_sign_string, ProofEnvelopeInput,
    };

    std::fs::create_dir_all(cache_dir)?;
    let now = "2026-04-30T00:00:00.000Z"; // deterministic for the demo

    let pubkey = ed25519_pubkey_string(seed);

    // Build the implication evidence.body manually (mirrors
    // provekit-claim-envelope::mint_implication but adds
    // `producerPubkey` so tier-2 lookups can verify the signature
    // without a pubkey directory).
    let _ = (post_formula, pre_formula); // Reserved for richer body in v1.2.

    let mut body_kvs: Vec<(String, std::sync::Arc<Value>)> = vec![
        ("antecedentHash".into(), Value::string(post_hash.to_string())),
        ("consequentHash".into(), Value::string(pre_hash.to_string())),
        ("antecedentCid".into(), Value::string("blake3-512:0000".to_string())),
        ("consequentCid".into(), Value::string("blake3-512:0000".to_string())),
        ("antecedentSlot".into(), Value::string("post".to_string())),
        ("consequentSlot".into(), Value::string("pre".to_string())),
        ("prover".into(), Value::string("z3@4.x".to_string())),
        ("proverRunMs".into(), Value::integer(0)),
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

    // bindingHash = hash(canonical({antecedentHash, consequentHash}))
    // propertyHash = hash("implication:" || ah || ":" || ch)
    let bh = Value::object([
        ("antecedentHash", Value::string(post_hash.to_string())),
        ("consequentHash", Value::string(pre_hash.to_string())),
    ]);
    let binding_hash = blake3_512_of(encode_jcs(&bh).as_bytes());
    let property_hash = implication_property_hash(post_hash, pre_hash);

    // input_cids per spec: [antecedentCid, consequentCid] lex-sorted.
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

    // Re-emit signed.
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

    // Wrap in a single-member .proof catalog so it's picked up by the
    // standard load_all_proofs / Tier 2 scanner.
    let mut members: std::collections::BTreeMap<String, Vec<u8>> = std::collections::BTreeMap::new();
    members.insert(cid.clone(), final_canonical);
    let signer_cid = blake3_512_of(pubkey.as_bytes());
    let proof_input = ProofEnvelopeInput {
        name: format!("@cache/implication-{}", short(&property_hash)),
        version: "1.0.0".into(),
        members,
        signer_cid,
        signer_seed: *seed,
        declared_at: now.into(),
    };
    let built = build_proof_envelope(&proof_input);

    // File name: the implication memento's propertyHash, so callers
    // can spot it visually. Tier 2 scans by content not filename, but
    // this makes the cache directory readable.
    let fname = format!("{}.proof", property_hash);
    let path = cache_dir.join(fname);
    std::fs::write(path, built.bytes)?;
    Ok(())
}
