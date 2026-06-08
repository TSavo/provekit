// SPDX-License-Identifier: Apache-2.0
//
// Receipt 1: test-assertion consistency pass.
//
// A test that asserts several facts about the SAME term (e.g. a bare
// variable `x`) lifts, after same-name coalescing in provekit-lift, to a
// single contract whose `inv` is the CONJUNCTION of those facts. When the
// conjuncts are mutually satisfiable the test's assertions are mutually
// CONSISTENT; when they contradict (`assert x is None` AND
// `assert x is not None` -> `=(x,None) ∧ ≠(x,None)`) the conjunction is
// UNSATISFIABLE.
//
// `enumerate_callsites` only produces obligations for `inv` ctor terms that
// match a known bridge sourceSymbol. An `inv` over a bare free var and a
// `None` constructor has no bridge ctor, so it produces ZERO call sites and
// the contradiction dies silently. This pass is where that conjoined `inv`
// is actually checked.
//
// SOLVER POLARITY. The shared SMT path (`smt_emitter::emit`) renders the
// NEGATED-VALIDITY form (`assert (not goal); check-sat`), so the z3 kit maps
// `unsat -> Discharged`. This pass needs the OPPOSITE: the RAW satisfiability
// of the invariant itself (`assert <inv>; check-sat`, via `emit_asserted`).
// So we INVERT the solver verdict:
//   raw z3 `sat`   (solver reports Unsatisfied) -> PROVEN-consistent
//   raw z3 `unsat` (solver reports Discharged)  -> REFUSED-contradictory
//   anything else  (Undecidable / unknown)      -> Undecidable, reported LOUD
//
// CLAIM. A PROVEN row here claims EXACTLY "test assertions mutually
// consistent about callsite X" -- NOT that the production code is correct
// and NOT that any postcondition is satisfied. Code-correctness is a
// separate obligation (production-bridge / self-post discharge).
//
// LITERAL-VALUE MODEL (Python `==` semantics; see
// `sugar_ir_compiler_smt_lib::literal_encoding`). The consistency verdict
// for literal-bearing assertions reflects Python equality EXACTLY in these
// dimensions:
//   - Distinct string literals are unequal:        `"a" != "b"`.
//   - A string literal is not any number:          `"5" != 5`.
//   - A string literal is not None:                `"x" != None`.
//   - None is not any number and not any string:   `None != 5`, `None != "x"`.
//   - bool IS int (bool encodes to its int value): `True == 1`, `False == 0`,
//     so `r == True; r == 1` stays CONSISTENT (NOT over-refused).
// RESIDUAL (not modeled): `float == int` cross-type equality. Python
// `5.0 == 5` is true, but a non-integer float literal is NOT folded into the
// integer distinctness set (asserting `5.0 != 5` would be Python-false and
// `(distinct strlit 5.0)` ill-sorted), so a `r == 5.0; r == 5`-style pairing
// is left unconstrained rather than risk a false refusal. Retirement: a
// float<->int sort-morphism / Real-theory encoding.
//
// DOCUMENTED LIMITATION. Contradictions are caught only when the facts share
// the SAME lifted term (same bare var / same syntactic callsite). Two tests
// asserting opposite things about the same INPUT at DIFFERENT source
// locations lift to DISTINCT free vars and do NOT contradict here; catching
// those requires the argument-carrying (uninterpreted-function / EUF) lifter
// change, which is queued as the next capability and deliberately not built
// here.

use std::collections::HashMap;

use rayon::prelude::*;
use serde_json::Value as Json;
use tracing::{info, warn};

use crate::solvers::{run_plan, SolverHandle, SolverPlan};
use crate::types::{memento_body, memento_kind, MementoPool, ObligationVerdict};
use sugar_ir_compiler_smt_lib::emit_asserted;

/// Outcome of a single contract's consistency check.
#[derive(Debug, Clone)]
pub struct ConsistencyResult {
    pub contract_cid: String,
    pub property_name: String,
    /// `Discharged` => PROVEN-consistent; `Unsatisfied` => REFUSED-contradictory;
    /// `Undecidable` => encoding STOP (must be surfaced, never silently passed).
    pub verdict: ObligationVerdict,
    pub reason: String,
    /// True when the verdict came from an EXECUTION WITNESS discharged by
    /// recompute (k(I)=t), NOT from a symbolic solver. Kept distinct so the
    /// report never reads witnessed-by-execution as proven-by-solver.
    pub witnessed: bool,
}

const CONSISTENT_REASON: &str = "test assertions mutually consistent about callsite";
const CONTRADICTORY_REASON: &str = "test assertions contradictory about callsite";

/// Does this contract have an `inv` that produces no enumerable bridge
/// callsite? We approximate "no bridge callsite" structurally: the pass only
/// fires for contracts that carry an `inv` and NO `pre`/`post` (the lifted
/// shape of a coalesced test-assertion fact set). Bridge-bearing contracts
/// carry pre/post and are handled by the call-site path.
///
/// SETUP-BINDING EXCLUSION. The Pattern-5 (call-binding) lifter emits, per
/// call site, a `::facts` contract carrying the SETUP BINDING (e.g.
/// `y = make_value(x)` -> `=(y, make_value(x))`) alongside the asserted-
/// property `::assertion` contract. A `::facts` binding is SAT by
/// construction (it is just a definition, not a claim); reporting it as
/// "test assertions mutually consistent" is vacuous and mislabeled. Only
/// asserted-property contracts belong in the consistency report:
///   - whole-test Pattern-3 contracts (named by the test, no `::facts` suffix)
///   - `::assertion` contracts (Pattern-5 conjoined asserted properties)
///   - loop/parametrize assertion contracts (no `::facts` suffix)
/// So `::facts` and `::facts::N` setup-binding contracts are excluded by name.
fn is_consistency_candidate(body: &Json) -> bool {
    let has_inv = body.get("inv").map(|v| v.is_object()).unwrap_or(false);
    let has_pre = body.get("pre").map(|v| v.is_object()).unwrap_or(false);
    let has_post = body.get("post").map(|v| v.is_object()).unwrap_or(false);
    if !(has_inv && !has_pre && !has_post) {
        return false;
    }
    let name = body
        .get("name")
        .and_then(|v| v.as_str())
        .or_else(|| body.get("contractName").and_then(|v| v.as_str()))
        .unwrap_or("");
    !is_setup_binding_name(name)
}

/// A `::facts` / `::facts::N` contract is a setup binding, not an asserted
/// property. Matches the trailing segment exactly so it does not catch the
/// asserted-property `::assertion` name or any other suffix. (The
/// `::facts-implies-assertion` form is an implication DECL, not a contract,
/// so it never reaches this pass; the guard is nonetheless precise.)
fn is_setup_binding_name(name: &str) -> bool {
    // Strip an optional trailing `::N` duplicate-disambiguation suffix, then
    // require the remaining segment to end in exactly `::facts`.
    let stem = match name.rsplit_once("::") {
        Some((head, tail)) if tail.chars().all(|c| c.is_ascii_digit()) && !tail.is_empty() => head,
        _ => name,
    };
    stem.ends_with("::facts")
}

/// Invert a raw-satisfiability solver verdict into a consistency verdict.
/// See the SOLVER POLARITY note at the top of the module.
fn consistency_verdict(raw: ObligationVerdict) -> (ObligationVerdict, &'static str) {
    match raw {
        // raw `sat`  -> solver said Unsatisfied -> the inv IS satisfiable -> consistent
        ObligationVerdict::Unsatisfied => (ObligationVerdict::Discharged, CONSISTENT_REASON),
        // raw `unsat` -> solver said Discharged -> the inv is contradictory -> refuse
        ObligationVerdict::Discharged => (ObligationVerdict::Unsatisfied, CONTRADICTORY_REASON),
        // An honest refusal (no sound discharger) passes through as a refusal --
        // it carries its own named reason from the solver layer, never overwritten
        // with the generic encoding-STOP message.
        ObligationVerdict::Refused => (ObligationVerdict::Refused, "refused: no sound discharger"),
        // unknown / error -> encoding STOP, surfaced loud
        other => (other, "consistency check undecidable (encoding STOP)"),
    }
}

/// Settle a contract carrying a `custom` execution-witness EvidenceTerm by
/// RECOMPUTE: write the EvidenceTerm to a temp `.proof` and spawn the kit's
/// discharge command (`$PROVEKIT_WITNESS_DISCHARGE <witness.proof> <project>`),
/// which re-runs the pinned test. The verifier stays language-blind; the kit
/// owns recompute. Returns None when there is no custom witness (caller falls
/// through to symbolic solving). FAIL-CLOSED: missing config / spawn error /
/// unparseable output is Undecidable, never Discharged.
fn try_witness_discharge(
    body: &Json,
    contract_cid: String,
    property_name: String,
) -> Option<ConsistencyResult> {
    let evidence = body.get("evidence")?;
    if evidence.get("proofType").and_then(|v| v.as_str()) != Some("custom") {
        return None;
    }
    let undecidable = |reason: String| ConsistencyResult {
        contract_cid: contract_cid.clone(),
        property_name: property_name.clone(),
        verdict: ObligationVerdict::Undecidable,
        reason,
        witnessed: false,
    };
    // Route to the TOOL-specific discharge command (federation): the certificate
    // names its `tool`; the kit's manifest declared the matching command, which
    // `cmd_prove` exported as PROVEKIT_WITNESS_DISCHARGE_<TOOL>. Fall back to the
    // generic PROVEKIT_WITNESS_DISCHARGE (manual override). Fail-closed if neither.
    let tool = evidence
        .get("certificate")
        .and_then(|c| c.get("tool"))
        .and_then(|t| t.as_str())
        .unwrap_or("");
    let tool_key = format!(
        "PROVEKIT_WITNESS_DISCHARGE_{}",
        tool.to_uppercase()
            .replace(|c: char| !c.is_ascii_alphanumeric(), "_")
    );
    let cmd = match std::env::var(&tool_key)
        .ok()
        .filter(|c| !c.trim().is_empty())
        .or_else(|| {
            std::env::var("PROVEKIT_WITNESS_DISCHARGE")
                .ok()
                .filter(|c| !c.trim().is_empty())
        }) {
        Some(c) => c,
        None => {
            return Some(undecidable(format!(
                "custom witness (tool={tool:?}) present but no discharge command \
                 configured (declare `discharge_command` + `witness_tool` in the \
                 kit's lift manifest) (fail-closed)"
            )))
        }
    };
    let project = match std::env::var("PROVEKIT_WITNESS_PROJECT_DIR") {
        Ok(p) if !p.trim().is_empty() => p,
        _ => {
            return Some(undecidable(
                "custom witness present but PROVEKIT_WITNESS_PROJECT_DIR unset (fail-closed)"
                    .into(),
            ))
        }
    };
    let tmp = std::env::temp_dir().join(format!(
        "{}.witness.proof",
        contract_cid.replace([':', '/'], "_")
    ));
    if let Err(e) = std::fs::write(&tmp, evidence.to_string()) {
        return Some(undecidable(format!("witness temp write failed: {e}")));
    }
    let mut parts = cmd.split_whitespace();
    let prog = match parts.next() {
        Some(p) => p,
        None => return Some(undecidable("empty PROVEKIT_WITNESS_DISCHARGE".into())),
    };
    // Spawn with cwd = the project root so a manifest's RELATIVE discharge
    // command (e.g. `PYTHONPATH=../../implementations/...`) resolves the same
    // way the lift command does from its working_dir.
    let output = std::process::Command::new(prog)
        .args(parts)
        .arg(&tmp)
        .arg(&project)
        .current_dir(&project)
        .output();
    let out = match output {
        Ok(o) => o,
        Err(e) => return Some(undecidable(format!("witness discharge spawn failed: {e}"))),
    };
    let stdout = String::from_utf8_lossy(&out.stdout);
    let parsed: Json = match serde_json::from_str(stdout.lines().last().unwrap_or("")) {
        Ok(j) => j,
        Err(e) => {
            return Some(undecidable(format!(
                "witness discharge output unparseable: {e}"
            )))
        }
    };
    let verdict_str = parsed
        .get("verdict")
        .and_then(|v| v.as_str())
        .unwrap_or("REFUSED");
    let reason = parsed
        .get("reason")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    Some(if verdict_str == "DISCHARGED" {
        ConsistencyResult {
            contract_cid,
            property_name,
            verdict: ObligationVerdict::Discharged,
            reason: format!("witnessed by recompute (kit): {reason}"),
            witnessed: true,
        }
    } else {
        ConsistencyResult {
            contract_cid,
            property_name,
            verdict: ObligationVerdict::Unsatisfied,
            reason: format!("witness REFUSED by recompute: {reason}"),
            witnessed: false,
        }
    })
}

/// Run the consistency pass over every candidate contract in the pool.
/// True iff this contract carries a `custom` execution-witness EvidenceTerm, so
/// it is settled BY RECOMPUTE (`try_witness_discharge`) rather than symbolic SAT.
fn is_witness_member(body: &Json) -> bool {
    body.get("evidence")
        .and_then(|e| e.get("proofType"))
        .and_then(|v| v.as_str())
        == Some("custom")
}

/// Run the raw-satisfiability consistency check on a single `inv` and label it.
/// Shared by the per-contract path and the cross-proof conjoined path.
fn check_inv_consistency(
    cid: String,
    property_name: &str,
    inv: Json,
    plan: &SolverPlan,
    registry: &HashMap<String, SolverHandle>,
) -> ConsistencyResult {
    let smt = match emit_asserted(&inv) {
        Ok(s) => s,
        Err(e) => {
            return ConsistencyResult {
                contract_cid: cid,
                property_name: property_name.to_string(),
                verdict: ObligationVerdict::Undecidable,
                reason: format!("consistency smt-emit (encoding STOP): {e}"),
                witnessed: false,
            };
        }
    };
    let (raw, raw_reason, _invs) = run_plan(plan, registry, &smt, Some(&inv));
    let (verdict, label) = consistency_verdict(raw);
    let reason = format!("{label} `{property_name}` [{raw_reason}]");
    if verdict == ObligationVerdict::Undecidable {
        warn!(
            contract = %property_name,
            cid = %cid,
            raw = ?raw,
            "consistency: UNDECIDABLE/ill-sorted -- encoding STOP, NOT a pass"
        );
    }
    ConsistencyResult {
        contract_cid: cid,
        property_name: property_name.to_string(),
        verdict,
        reason,
        witnessed: false,
    }
}

pub fn verify_consistency(
    pool: &MementoPool,
    plan: &SolverPlan,
    registry: &HashMap<String, SolverHandle>,
) -> Vec<ConsistencyResult> {
    let candidates: Vec<(&String, &Json)> = pool
        .mementos
        .iter()
        .filter(|(_, env)| memento_kind(env) == Some("contract"))
        .filter_map(|(cid, env)| memento_body(env).map(|b| (cid, b)))
        .filter(|(_, body)| is_consistency_candidate(body))
        .collect();

    // CROSS-PROOF CONJOIN: group same-named contracts and conjoin their `inv`s
    // before the SAT check -- the cross-proof twin of mint's same-name coalesce
    // (cmd_mint.rs `ir_coalesced` / CoalesceEntry::InvOnly). When a consumer
    // asserts `np.add(2,3)==6` and an IMPORTED numpy proof asserts
    // `np.add(2,3)==5`, both land on `numpy.add#euf#...::assertion`; conjoining
    // gives `and(==5, ==6)` -> raw unsat -> CONTRADICTORY -> refused. Identical
    // assertions dedupe by CID (one member) and stay PROVEN. The contract NAME is
    // the content-keyed callsite, so same name == same callsite == sound to
    // conjoin -- the same invariant mint relies on.
    let mut by_name: std::collections::BTreeMap<String, Vec<(&String, &Json)>> =
        std::collections::BTreeMap::new();
    for (cid, body) in &candidates {
        let name = body
            .get("name")
            .and_then(|v| v.as_str())
            .or_else(|| body.get("contractName").and_then(|v| v.as_str()))
            .unwrap_or("<unnamed>")
            .to_string();
        by_name.entry(name).or_default().push((*cid, *body));
    }
    let groups: Vec<(String, Vec<(&String, &Json)>)> = by_name.into_iter().collect();

    let results: Vec<ConsistencyResult> = groups
        .par_iter()
        .flat_map(|(property_name, members)| {
            let mut out: Vec<ConsistencyResult> = Vec::new();

            // WITNESS members are settled BY RECOMPUTE, PER MEMBER (the kit's
            // discharge command; the verifier stays language-blind). They are
            // NEVER folded into the symbolic conjunction AND never short-circuit
            // the group: a witness member must not mask a contradictory inv group.
            let mut inv_cids: Vec<&String> = Vec::new();
            let mut inv_bodies: Vec<&Json> = Vec::new();
            for (m_cid, body) in members {
                if is_witness_member(body) {
                    if let Some(res) =
                        try_witness_discharge(body, (*m_cid).clone(), property_name.clone())
                    {
                        out.push(res);
                        continue;
                    }
                }
                inv_bodies.push(body);
                inv_cids.push(m_cid);
            }
            if inv_bodies.is_empty() {
                return out;
            }

            // CROSS-PROOF CONJOIN only for CALLSITE-KEYED names (`#euf#`). That key
            // is `(callee, args)`, so same name == same call == sound to conjoin a
            // consumer's assertion with an imported vendor contract -> `and(==5,==6)`
            // -> unsat -> refused. A bare test/location name does NOT guarantee the
            // same subject, so those stay PER-CONTRACT (conjoining them could falsely
            // refuse two unrelated tests that happen to share a function name).
            let callsite_keyed = property_name.contains("#euf#");
            if callsite_keyed && inv_bodies.len() > 1 {
                let invs: Vec<Json> = inv_bodies
                    .iter()
                    .map(|b| b.get("inv").cloned().unwrap_or(Json::Null))
                    .collect();
                let inv = serde_json::json!({ "kind": "and", "operands": invs });
                out.push(check_inv_consistency(
                    inv_cids[0].clone(),
                    property_name,
                    inv,
                    plan,
                    registry,
                ));
            } else {
                for (cid, body) in inv_cids.iter().zip(inv_bodies.iter()) {
                    let inv = body.get("inv").cloned().unwrap_or(Json::Null);
                    out.push(check_inv_consistency(
                        (*cid).clone(),
                        property_name,
                        inv,
                        plan,
                        registry,
                    ));
                }
            }
            out
        })
        .collect();

    info!(
        candidates = candidates.len(),
        consistent = results
            .iter()
            .filter(|r| r.verdict == ObligationVerdict::Discharged)
            .count(),
        contradictory = results
            .iter()
            .filter(|r| r.verdict == ObligationVerdict::Unsatisfied)
            .count(),
        undecidable = results
            .iter()
            .filter(|r| r.verdict == ObligationVerdict::Undecidable)
            .count(),
        witnessed = results.iter().filter(|r| r.witnessed).count(),
        "verifier: test-assertion consistency pass complete"
    );

    results
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::solvers::registry;
    use serde_json::json;

    fn pool_with_contract(name: &str, inv: Json) -> MementoPool {
        let mut pool = MementoPool::default();
        let cid = format!("blake3-512:{name}");
        // v1.2 layered shape: accessors branch on presence of `envelope`.
        let env = json!({
            "envelope": {
                "header": {
                    "kind": "contract",
                    "contractName": name,
                    "inv": inv,
                }
            }
        });
        pool.insert(cid.clone(), env);
        pool
    }

    fn z3_plan_and_registry() -> (SolverPlan, HashMap<String, SolverHandle>) {
        let registry = registry::build_default_z3("z3");
        (SolverPlan::Single("z3".into()), registry)
    }

    fn ne(a: Json, b: Json) -> Json {
        json!({"kind":"atomic","name":"≠","args":[a,b]})
    }
    fn eqf(a: Json, b: Json) -> Json {
        json!({"kind":"atomic","name":"=","args":[a,b]})
    }
    fn var(n: &str) -> Json {
        json!({"kind":"var","name":n})
    }
    fn none() -> Json {
        json!({"kind":"ctor","name":"None","args":[]})
    }
    fn int(n: i64) -> Json {
        json!({"kind":"const","sort":{"kind":"primitive","name":"Int"},"value":n})
    }
    fn gt(a: Json, b: Json) -> Json {
        json!({"kind":"atomic","name":">","args":[a,b]})
    }
    fn insert_contract(pool: &mut MementoPool, cid: &str, name: &str, inv: Json) {
        let env = json!({
            "envelope": { "header": { "kind": "contract", "contractName": name, "inv": inv } }
        });
        pool.insert(cid.to_string(), env);
    }

    /// CROSS-PROOF CONJOIN: two contracts sharing a callsite name -- a consumer's
    /// assertion and an IMPORTED vendor contract about the same call -- are
    /// CONJOINED before the SAT check, not kept-one-dropped-one. This is what
    /// makes a numpy USER who asserts `np.add(2,3)==6` get REFUSED against an
    /// inherited numpy `==5`. Discrimination guards the false-refusal boundary:
    /// a CONSISTENT conjunction stays PROVEN, and a lone contract is untouched.
    #[test]
    fn cross_proof_same_named_contracts_are_conjoined() {
        let (plan, reg) = z3_plan_and_registry();
        let name = "numpy.add#euf#callresult_numpy_add_a2(2,3)::assertion";

        // consumer ==6 + imported numpy ==5 (distinct CIDs) -> and(==5,==6) -> REFUSED
        let mut pool = MementoPool::default();
        insert_contract(
            &mut pool,
            "blake3-512:consumer6",
            name,
            eqf(var("r"), int(6)),
        );
        insert_contract(&mut pool, "blake3-512:numpy5", name, eqf(var("r"), int(5)));
        let res = verify_consistency(&pool, &plan, &reg);
        assert_eq!(
            res.len(),
            1,
            "same-named contracts collapse to one obligation: {res:?}"
        );
        assert_eq!(
            res[0].verdict,
            ObligationVerdict::Unsatisfied,
            "cross-proof contradiction must be refused: {res:?}"
        );

        // consumer ==5 + numpy r>0 (distinct CIDs, CONSISTENT) -> and -> PROVEN
        let mut pool = MementoPool::default();
        insert_contract(&mut pool, "blake3-512:a", name, eqf(var("r"), int(5)));
        insert_contract(&mut pool, "blake3-512:b", name, gt(var("r"), int(0)));
        let res = verify_consistency(&pool, &plan, &reg);
        assert_eq!(res.len(), 1);
        assert_eq!(
            res[0].verdict,
            ObligationVerdict::Discharged,
            "consistent conjunction must stay proven (no false refusal): {res:?}"
        );

        // a LONE contract is untouched -> PROVEN
        let mut pool = MementoPool::default();
        insert_contract(&mut pool, "blake3-512:solo", name, eqf(var("r"), int(5)));
        let res = verify_consistency(&pool, &plan, &reg);
        assert_eq!(res.len(), 1);
        assert_eq!(res[0].verdict, ObligationVerdict::Discharged);
    }

    /// A WITNESS member in a same-callsite group must NOT short-circuit the group
    /// and mask a contradictory inv conjunction. Witnesses settle per-member; the
    /// `and(==5,==6)` must still surface as Unsatisfied. (Review: CodeRabbit
    /// Critical / Codex P1 on the first-witnessed-member return.)
    #[test]
    fn witness_member_does_not_mask_a_contradictory_group() {
        let (plan, reg) = z3_plan_and_registry();
        let name = "numpy.add#euf#c:callresult_numpy_add_a2(i:2,i:3)::assertion";
        let mut pool = MementoPool::default();
        // a custom-witness member sharing the callsite name (no discharge command
        // configured -> Undecidable, fail-closed; the point is it must not swallow
        // the group's contradiction).
        let witness = json!({"envelope":{"header":{
            "kind":"contract","contractName":name,"inv": eqf(var("r"), int(5)),
            "evidence":{"proofType":"custom","certificate":
                {"tool":"pytest","version":"x","formulaHash":"x","proofData":"{}"}}}}});
        pool.insert("blake3-512:witnessmember".to_string(), witness);
        insert_contract(&mut pool, "blake3-512:c5", name, eqf(var("r"), int(5)));
        insert_contract(&mut pool, "blake3-512:c6", name, eqf(var("r"), int(6)));
        let res = verify_consistency(&pool, &plan, &reg);
        assert!(
            res.iter()
                .any(|r| r.verdict == ObligationVerdict::Unsatisfied),
            "the contradiction must surface despite a witness member: {res:?}"
        );
    }

    /// Same callee NAME but DIFFERENT (non-callsite-keyed) test names must NOT be
    /// conjoined: two unrelated tests that share a function name stay independent,
    /// no false refusal. Only `#euf#` callsite keys conjoin across proofs.
    #[test]
    fn bare_test_names_are_not_conjoined() {
        let (plan, reg) = z3_plan_and_registry();
        let mut pool = MementoPool::default();
        // Two same-named, contradictory-looking contracts under a BARE test name.
        // They are about independent subjects; conjoining would falsely refuse.
        insert_contract(
            &mut pool,
            "blake3-512:t1",
            "test_add",
            eqf(var("r"), int(5)),
        );
        insert_contract(
            &mut pool,
            "blake3-512:t2",
            "test_add",
            eqf(var("r"), int(6)),
        );
        let res = verify_consistency(&pool, &plan, &reg);
        // per-contract: each is internally satisfiable -> both Discharged, none refused.
        assert_eq!(
            res.len(),
            2,
            "bare names must NOT collapse into one obligation: {res:?}"
        );
        assert!(
            res.iter()
                .all(|r| r.verdict == ObligationVerdict::Discharged),
            "independent same-test-name contracts must not be conjoined: {res:?}"
        );
    }

    /// The witness-discharge arm: a contract carrying a `custom` EvidenceTerm is
    /// settled by spawning the tool's discharge command (here a stub), routed by
    /// the certificate's `tool`. Discharge -> Witnessed; refuse -> Unsatisfied;
    /// no command -> Undecidable (fail-closed). Hermetic: no python.
    #[test]
    fn witness_arm_routes_by_tool_and_fails_closed() {
        let dir = std::env::temp_dir();
        let script = dir.join("provekit_test_witness_discharge.sh");
        let write_stub = |verdict: &str| {
            std::fs::write(
                &script,
                format!("#!/bin/sh\necho '{{\"verdict\":\"{verdict}\",\"reason\":\"stub\"}}'\n"),
            )
            .unwrap();
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
            }
        };
        let body = json!({
            "kind": "contract",
            "contractName": "test_x",
            "inv": {"kind":"atomic","name":"witnessed","args":[]},
            "evidence": {"kind":"evidence","proofType":"custom",
                         "certificate":{"tool":"stubtool","proofData":"{}"}},
        });
        let call =
            || try_witness_discharge(&body, "blake3-512:cid".into(), "test_x".into()).unwrap();

        std::env::set_var("PROVEKIT_WITNESS_PROJECT_DIR", &dir);
        // routed to PROVEKIT_WITNESS_DISCHARGE_STUBTOOL (tool="stubtool")
        write_stub("DISCHARGED");
        std::env::set_var("PROVEKIT_WITNESS_DISCHARGE_STUBTOOL", &script);
        let r = call();
        assert_eq!(r.verdict, ObligationVerdict::Discharged);
        assert!(r.witnessed, "discharge must be flagged witnessed");

        write_stub("REFUSED");
        let r = call();
        assert_eq!(r.verdict, ObligationVerdict::Unsatisfied);
        assert!(!r.witnessed);

        // fail-closed: no command for this tool, no generic fallback
        std::env::remove_var("PROVEKIT_WITNESS_DISCHARGE_STUBTOOL");
        std::env::remove_var("PROVEKIT_WITNESS_DISCHARGE");
        let r = call();
        assert_eq!(r.verdict, ObligationVerdict::Undecidable);
        assert!(!r.witnessed);

        std::env::remove_var("PROVEKIT_WITNESS_PROJECT_DIR");
        let _ = std::fs::remove_file(&script);
    }

    /// A contract WITHOUT a custom witness is untouched by the arm (falls through
    /// to the normal SAT path).
    #[test]
    fn non_witness_contract_ignores_the_arm() {
        let body = json!({"kind":"contract","contractName":"t","inv": ne(var("x"), none())});
        assert!(try_witness_discharge(&body, "c".into(), "t".into()).is_none());
    }

    #[test]
    fn consistent_assertions_prove_consistent() {
        // assert x is not None  (single satisfiable fact) -> ≠(x, None) -> SAT
        let inv = ne(var("x"), none());
        let pool = pool_with_contract("test_consistent", inv);
        let (plan, registry) = z3_plan_and_registry();
        let results = verify_consistency(&pool, &plan, &registry);
        assert_eq!(results.len(), 1, "exactly one candidate");
        assert_eq!(
            results[0].verdict,
            ObligationVerdict::Discharged,
            "consistent inv must be PROVEN-consistent; reason: {}",
            results[0].reason
        );
        assert!(
            results[0].reason.contains("mutually consistent"),
            "claim must be labeled consistency, got: {}",
            results[0].reason
        );
    }

    #[test]
    fn contradictory_assertions_are_refused() {
        // assert x is None AND assert x is not None
        //   -> and(=(x,None), ≠(x,None)) -> UNSAT
        let inv = json!({"kind":"and","operands":[
            eqf(var("x"), none()),
            ne(var("x"), none()),
        ]});
        let pool = pool_with_contract("test_contradictory", inv);
        let (plan, registry) = z3_plan_and_registry();
        let results = verify_consistency(&pool, &plan, &registry);
        assert_eq!(results.len(), 1, "exactly one candidate");
        assert_eq!(
            results[0].verdict,
            ObligationVerdict::Unsatisfied,
            "contradictory inv must be REFUSED; reason: {}",
            results[0].reason
        );
        assert!(
            results[0].reason.contains("contradictory"),
            "claim must be labeled contradiction, got: {}",
            results[0].reason
        );
    }

    #[test]
    fn pre_post_bearing_contract_is_not_a_consistency_candidate() {
        // A bridge-bearing contract (carries pre/post) must NOT be picked up
        // by this pass; it is the call-site path's job.
        let mut pool = MementoPool::default();
        let env = json!({
            "envelope": {
                "header": {
                    "kind": "contract",
                    "contractName": "bridge_contract",
                    "pre": ne(var("x"), none()),
                    "inv": ne(var("x"), none()),
                }
            }
        });
        pool.insert("blake3-512:bridge".into(), env);
        let (plan, registry) = z3_plan_and_registry();
        let results = verify_consistency(&pool, &plan, &registry);
        assert!(
            results.is_empty(),
            "pre-bearing contract must not be a consistency candidate"
        );
    }

    #[test]
    fn facts_setup_binding_contract_is_not_a_consistency_candidate() {
        // A `::facts` contract carries the call-site SETUP BINDING
        // (e.g. `y = make_value(x)` -> `=(y, make_value(x))`), not an
        // asserted property. It is SAT by construction and reporting it
        // as "test assertions mutually consistent" is vacuous and
        // mislabeled. It must NOT appear in the consistency report.
        let facts_inv = eqf(var("y"), none());
        let pool = pool_with_contract("make_value@t.py:6:8::facts", facts_inv);
        let (plan, registry) = z3_plan_and_registry();
        let results = verify_consistency(&pool, &plan, &registry);
        assert!(
            results.is_empty(),
            "::facts setup-binding contract must not be a consistency candidate; got: {:?}",
            results.iter().map(|r| &r.property_name).collect::<Vec<_>>()
        );
    }

    #[test]
    fn facts_indexed_setup_binding_contract_is_not_a_consistency_candidate() {
        // The duplicate-disambiguated `::facts::N` setup-binding form is
        // likewise excluded.
        let facts_inv = eqf(var("y"), none());
        let pool = pool_with_contract("make_value@t.py:6:8::facts::1", facts_inv);
        let (plan, registry) = z3_plan_and_registry();
        let results = verify_consistency(&pool, &plan, &registry);
        assert!(
            results.is_empty(),
            "::facts::N setup-binding contract must not be a consistency candidate; got: {:?}",
            results.iter().map(|r| &r.property_name).collect::<Vec<_>>()
        );
    }

    #[test]
    fn assertion_contract_remains_a_consistency_candidate() {
        // The `::assertion` contract carries the asserted property and MUST
        // still be checked. Guards against an over-broad `::facts` filter
        // (substring match would wrongly catch `::facts-implies-assertion`,
        // but that is an implication decl, not a contract; the asserted
        // property contract ends in `::assertion`).
        let inv = ne(var("y"), none());
        let pool = pool_with_contract("make_value@t.py:6:8::assertion", inv);
        let (plan, registry) = z3_plan_and_registry();
        let results = verify_consistency(&pool, &plan, &registry);
        assert_eq!(
            results.len(),
            1,
            "::assertion contract must remain a consistency candidate"
        );
        assert_eq!(results[0].verdict, ObligationVerdict::Discharged);
    }

    #[test]
    fn bare_var_pattern3_contract_remains_a_consistency_candidate() {
        // A whole-test Pattern-3 contract is named by the test (no `::facts`
        // suffix) and must remain a candidate.
        let inv = ne(var("x"), none());
        let pool = pool_with_contract("test_x_consistent", inv);
        let (plan, registry) = z3_plan_and_registry();
        let results = verify_consistency(&pool, &plan, &registry);
        assert_eq!(
            results.len(),
            1,
            "bare-var Pattern-3 contract must remain a consistency candidate"
        );
    }

    // ── String-equality consistency tests ─────────────────────────────────
    // These are the census contracts that were UNDECIDABLE before the fix.
    // Shape: `assert r == '{"a":1}'` lifts to `=(r, string_const)` in `inv`.

    fn string_const(s: &str) -> Json {
        json!({"kind":"const","value":s,"sort":{"kind":"primitive","name":"String"}})
    }

    #[test]
    fn single_string_equality_asserted_is_consistent() {
        // POSITIVE: `assert r == '{"a":1}'` — a single string-equality assertion
        // is satisfiable (consistent). Before the fix: UNDECIDABLE (parse error).
        // After fix: PROVEN-consistent (raw sat from z3).
        let inv = eqf(var("r"), string_const(r#"{"a":1}"#));
        let pool = pool_with_contract("encode_jcs::assertion", inv);
        let (plan, registry) = z3_plan_and_registry();
        let results = verify_consistency(&pool, &plan, &registry);
        assert_eq!(results.len(), 1, "exactly one candidate");
        assert_eq!(
            results[0].verdict,
            ObligationVerdict::Discharged,
            "single string-equality inv must be PROVEN-consistent (not UNDECIDABLE); \
             reason: {}",
            results[0].reason
        );
        assert!(
            !results[0].reason.contains("UNDECIDABLE")
                && !results[0].reason.contains("encoding STOP"),
            "single string-equality must not be UNDECIDABLE; got: {}",
            results[0].reason
        );
    }

    #[test]
    fn two_distinct_string_literals_same_var_consistency_refused() {
        // DISCRIMINATION: `assert r == "a"; assert r == "b"` with distinct literals.
        // Conjoined inv: `=(r,"a") ∧ =(r,"b")` — same var, two different string
        // constants — is UNSAT (refused as contradictory).
        // Before fix: UNDECIDABLE (parse error / ill-sorted).
        // After fix: REFUSED-contradictory (raw unsat from z3).
        let inv = json!({"kind":"and","operands":[
            eqf(var("r"), string_const("a")),
            eqf(var("r"), string_const("b")),
        ]});
        let pool = pool_with_contract("encode_jcs_two_literals::assertion", inv);
        let (plan, registry) = z3_plan_and_registry();
        let results = verify_consistency(&pool, &plan, &registry);
        assert_eq!(results.len(), 1, "exactly one candidate");
        assert_eq!(
            results[0].verdict,
            ObligationVerdict::Unsatisfied,
            "two-distinct-literal inv must be REFUSED (not UNDECIDABLE); reason: {}",
            results[0].reason
        );
        assert!(
            results[0].reason.contains("contradictory"),
            "must be labeled contradictory, got: {}",
            results[0].reason
        );
    }

    #[test]
    fn weird_char_string_literal_consistency_proven() {
        // STRUCTURAL: brace/backslash/unicode in the literal — must parse cleanly.
        // Before fix: UNDECIDABLE (z3 parse error on the raw literal text).
        // After fix: real sat/unsat verdict.
        let inv = eqf(var("r"), string_const(r#"{"a":"x"}"#));
        let pool = pool_with_contract("encode_jcs_brace::assertion", inv);
        let (plan, registry) = z3_plan_and_registry();
        let results = verify_consistency(&pool, &plan, &registry);
        assert_eq!(results.len(), 1, "exactly one candidate");
        assert_ne!(
            results[0].verdict,
            ObligationVerdict::Undecidable,
            "brace-containing string-literal inv must NOT be UNDECIDABLE; got: {}",
            results[0].reason
        );
    }

    // ── Cross-type literal distinctness (Python `==` semantics) ───────────
    // Permanent regression suite. The PROVEN/REFUSED verdict must match
    // Python's `==`: str/None disjoint from numbers and each other; bool IS
    // int (True==1, False==0). The `bool_true ... consistent` test is the
    // guard against over-distinctness and never leaves the suite.

    fn int_const(n: i64) -> Json {
        json!({"kind":"const","value":n,"sort":{"kind":"primitive","name":"Int"}})
    }
    fn bool_const(b: bool) -> Json {
        json!({"kind":"const","value":b,"sort":{"kind":"primitive","name":"Bool"}})
    }

    #[test]
    fn str_literal_vs_int_literal_is_refused() {
        // `assert r == "5"; assert r == 5` -> `=(r,"5") ∧ =(r,5)`.
        // Python `"5" != 5` -> contradictory -> REFUSED. (Was a falsePass:
        // both collapsed into Int with no distinctness -> sat -> "consistent".)
        let inv = json!({"kind":"and","operands":[
            eqf(var("r"), string_const("5")),
            eqf(var("r"), int_const(5)),
        ]});
        let pool = pool_with_contract("cross_str_int::assertion", inv);
        let (plan, registry) = z3_plan_and_registry();
        let results = verify_consistency(&pool, &plan, &registry);
        assert_eq!(results.len(), 1);
        assert_eq!(
            results[0].verdict,
            ObligationVerdict::Unsatisfied,
            "`r==\"5\" ∧ r==5` must be REFUSED (Python str≠int); reason: {}",
            results[0].reason
        );
    }

    #[test]
    fn none_vs_int_literal_is_refused() {
        // `assert r is None; assert r == 5`. Python `None != 5` -> REFUSED.
        let inv = json!({"kind":"and","operands":[
            eqf(var("r"), none()),
            eqf(var("r"), int_const(5)),
        ]});
        let pool = pool_with_contract("cross_none_int::assertion", inv);
        let (plan, registry) = z3_plan_and_registry();
        let results = verify_consistency(&pool, &plan, &registry);
        assert_eq!(results.len(), 1);
        assert_eq!(
            results[0].verdict,
            ObligationVerdict::Unsatisfied,
            "`r is None ∧ r==5` must be REFUSED (Python None≠int); reason: {}",
            results[0].reason
        );
    }

    #[test]
    fn none_vs_bool_false_is_refused() {
        // `assert r is None; assert r == False`. Python `None != False`
        // (False==0, None != 0) -> REFUSED. Discriminating test for the
        // "bool joins the concrete-int distinctness target set" wiring.
        let inv = json!({"kind":"and","operands":[
            eqf(var("r"), none()),
            eqf(var("r"), bool_const(false)),
        ]});
        let pool = pool_with_contract("cross_none_false::assertion", inv);
        let (plan, registry) = z3_plan_and_registry();
        let results = verify_consistency(&pool, &plan, &registry);
        assert_eq!(results.len(), 1);
        assert_eq!(
            results[0].verdict,
            ObligationVerdict::Unsatisfied,
            "`r is None ∧ r==False` must be REFUSED (Python None≠False); reason: {}",
            results[0].reason
        );
    }

    #[test]
    fn bool_true_consistent_with_int_one_is_proven() {
        // OVER-DISTINCTNESS GUARD (permanent). `assert r == True; assert r == 1`.
        // Python `True == 1` -> CONSISTENT -> PROVEN. A REFUSED here would mean
        // bool was wrongly asserted distinct from int. This test never leaves
        // the suite.
        let inv = json!({"kind":"and","operands":[
            eqf(var("r"), bool_const(true)),
            eqf(var("r"), int_const(1)),
        ]});
        let pool = pool_with_contract("cross_true_one::assertion", inv);
        let (plan, registry) = z3_plan_and_registry();
        let results = verify_consistency(&pool, &plan, &registry);
        assert_eq!(results.len(), 1);
        assert_eq!(
            results[0].verdict,
            ObligationVerdict::Discharged,
            "`r==True ∧ r==1` must be PROVEN-consistent (Python True==1); reason: {}",
            results[0].reason
        );
    }

    #[test]
    fn same_type_string_contradiction_still_refused() {
        // Regression guard: same-type two-literal contradiction unchanged.
        let inv = json!({"kind":"and","operands":[
            eqf(var("r"), string_const("a")),
            eqf(var("r"), string_const("b")),
        ]});
        let pool = pool_with_contract("same_str::assertion", inv);
        let (plan, registry) = z3_plan_and_registry();
        let results = verify_consistency(&pool, &plan, &registry);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].verdict, ObligationVerdict::Unsatisfied);
    }
}
