// SPDX-License-Identifier: Apache-2.0
//
// Stage 2: enumerate_callsites. For every contract memento in the
// pool, walk its pre/post/inv looking for ctor terms whose `name`
// matches a known bridge sourceSymbol. Each hit is a CallSite.
//
// Mirrors implementations/cpp/.../verifier/enumerate_callsites.cpp.

use serde_json::Value as Json;
use tracing::{debug, info};

use crate::types::{memento_body, memento_kind, CallSite, MementoPool};

pub fn run(pool: &MementoPool) -> Vec<CallSite> {
    let _span = tracing::info_span!("enumerate_callsites").entered();
    info!(
        mementos = pool.mementos.len(),
        bridges = pool.bridges_by_symbol.len(),
        "enumerate_callsites: scanning contracts for callsites"
    );
    let mut out = Vec::new();
    for (cid, env) in &pool.mementos {
        // Shape-agnostic (matches resolve_target): v1.2-layered contracts
        // carry their kind on `header.kind` and pre/post/inv on `header`;
        // v1.1-flat carry them on `evidence.kind` / `evidence.body`. The
        // production harvest path (`mint_contract`) emits v1.2; reading
        // only `evidence.body` here meant harvested calls never enumerated.
        if memento_kind(env) != Some("contract") {
            continue;
        }
        let body = match memento_body(env) {
            Some(v) if v.is_object() => v,
            _ => continue,
        };
        let mut property_name = body
            .get("contractName")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();
        if property_name.is_empty() {
            // Stable fallback: short prefix of CID.
            property_name = format!("{}...", cid.chars().take(12).collect::<String>());
        }
        for slot in ["pre", "post", "inv"] {
            if let Some(f) = body.get(slot) {
                if f.is_object() {
                    walk_formula(f, &property_name, cid, pool, &mut out);
                }
            }
        }
    }
    info!(callsites = out.len(), "enumerate_callsites: complete");
    if out.is_empty() {
        debug!("enumerate_callsites: no callsites found (check that bridges exist in pool)");
    } else {
        for cs in &out {
            debug!(
                bridge = %cs.bridge_ir_name,
                property = %cs.property_name,
                target_cid = %cs.bridge_target_cid,
                "enumerate_callsites: callsite"
            );
        }
    }
    out
}

fn walk_formula(
    f: &Json,
    property_name: &str,
    property_cid: &str,
    pool: &MementoPool,
    out: &mut Vec<CallSite>,
) {
    let kind = f.get("kind").and_then(|v| v.as_str()).unwrap_or_default();
    match kind {
        "atomic" => {
            if let Some(args) = f.get("args").and_then(|v| v.as_array()) {
                for a in args {
                    // Pass the enclosing atomic down: when a bridged call
                    // ctor is found as a direct argument of this atomic,
                    // the body-discharge path needs the whole predicate
                    // (e.g. `=(double(3), 6)`) to derive the postcondition.
                    // A formula's terms have no dominating control-flow guard
                    // until a `cf_ite` is descended into, so the path condition
                    // starts empty here.
                    walk_term(a, property_name, property_cid, pool, Some(f), &[], out);
                }
            }
        }
        "and" | "or" | "not" | "implies" => {
            if let Some(ops) = f.get("operands").and_then(|v| v.as_array()) {
                for op in ops {
                    walk_formula(op, property_name, property_cid, pool, out);
                }
            }
        }
        "forall" | "exists" => {
            if let Some(b) = f.get("body") {
                if b.is_object() {
                    walk_formula(b, property_name, property_cid, pool, out);
                }
            }
        }
        _ => {}
    }
}

/// Convert a `cf_ite` GUARD TERM into the atomic-predicate FORMULA it asserts,
/// when (and only when) its head is a recognized boolean predicate. The lifter
/// folds a control-flow condition `x.is_some()` into the guard term
/// `is_some(x)` (`formula_to_term` keeps non-builtin predicate heads unchanged;
/// see lift.rs `cf_head`). Recovering the atomic lets the verifier discharge a
/// panic partial's `pre = is_some(recv)` UNDER the guard `is_some(recv)`.
///
/// SOUNDNESS: only the closed set of boolean-method predicates the lifter emits
/// as guards is recognized. A comparison guard (`cf_lt`/`cf_le`/...), a method
/// guard, or any other head returns `None`: it becomes no usable fact, so the
/// pre stays unprovable and the site is honestly undecidable. Recognizing a
/// head we cannot soundly assert would risk a false "cannot panic"; refusing to
/// recognize one can only UNDER-prove. Fail-safe by construction.
fn guard_term_to_atomic(t: &Json, negated: bool) -> Option<Json> {
    if t.get("kind").and_then(|v| v.as_str()) != Some("ctor") {
        return None;
    }
    let head = t.get("name").and_then(|v| v.as_str())?;
    let args = t.get("args").cloned().unwrap_or_else(|| serde_json::json!([]));
    // The boolean unary predicates the lifter recognizes as if/match guards.
    // Their POSITIVE form is the panic-freedom obligation a partial demands
    // (option_unwrap pre = is_some, result_unwrap pre = is_ok, ...). A negated
    // guard (the else-branch) flips to the complementary predicate, which never
    // establishes the partial's pre -- so an else-branch unwrap stays unproven.
    let atomic_name = match (head, negated) {
        ("is_some", false) | ("is_none", true) => "is_some",
        ("is_none", false) | ("is_some", true) => "is_none",
        ("is_ok", false) | ("is_err", true) => "is_ok",
        ("is_err", false) | ("is_ok", true) => "is_err",
        ("is_empty", false) => "is_empty",
        ("is_empty", true) => return None, // !is_empty has no partial-pre use
        _ => return None,
    };
    Some(serde_json::json!({ "kind": "atomic", "name": atomic_name, "args": args }))
}

fn walk_term(
    t: &Json,
    property_name: &str,
    property_cid: &str,
    pool: &MementoPool,
    containing_atomic: Option<&Json>,
    // PANIC-FREEDOM guard context: the atomic-predicate facts that dominate
    // this position in the lifted caller body, accumulated as `cf_ite`
    // branches are descended. Empty at the top of a formula.
    path_cond: &[Json],
    out: &mut Vec<CallSite>,
) {
    if !t.is_object() {
        return;
    }
    if t.get("kind").and_then(|v| v.as_str()) != Some("ctor") {
        return;
    }
    let name = t
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();
    if let Some(benv) = pool.bridges_by_symbol.get(&name) {
        // Shape-agnostic: v1.2-layered bridges carry the fields on
        // `header`; v1.1-flat on `evidence.body`.
        let bbody = memento_body(benv)
            .cloned()
            .unwrap_or_else(|| serde_json::json!({}));
        // Forward pin: REQUIRED by the current BridgeDeclaration grammar
        // (see protocol/specs/2026-04-30-ir-formal-grammar.md §
        // "Bridge target pinning: the shim-poisoning vector"). Older
        // bridges without this field are tolerated at load time but
        // can't have ConsequentBundlePinned enforced; resolve_target
        // emits a soft warning in that case.
        let bridge_target_proof_cid = bbody
            .get("targetProofCid")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string());
        let cs = CallSite {
            bridge_ir_name: name.clone(),
            bridge_target_cid: bbody
                .get("targetContractCid")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string(),
            bridge_source_layer: bbody
                .get("sourceLayer")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string(),
            bridge_target_layer: bbody
                .get("targetLayer")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string(),
            bridge_target_proof_cid,
            bridge_self_bundle_cid: pool
                .bridge_self_bundle_by_symbol
                .get(&name)
                .cloned(),
            property_name: property_name.to_string(),
            property_cid: property_cid.to_string(),
            arg_term: t
                .get("args")
                .and_then(|v| v.as_array())
                .and_then(|arr| arr.first().cloned()),
            containing_atomic: containing_atomic.cloned(),
            // Snapshot the dominating guard context for this call site. The
            // runner discharges a panic partial's `pre` under these facts.
            guard_facts: path_cond.to_vec(),
        };
        out.push(cs);
    }
    // Descend into the call's arguments. A nested call is no longer a
    // direct argument of `containing_atomic`, so stop threading it: only a
    // call DIRECTLY under an atomic carries that atomic as its `Q` source.
    //
    // `cf_ite(cond, then, else)` is the lifted control-flow value (see
    // walk_rpc `lift_tail_if_to_ite_term` / `lift_match_to_ite_term`): a panic
    // call nested in `then` is DOMINATED by `cond`, one in `else` by `!cond`.
    // Push the recovered guard fact onto the path condition for the dominated
    // branch so the partial's pre can discharge under it. arg0 (the guard term)
    // carries no guard itself. Any other ctor passes the current path condition
    // through unchanged (no new control-flow fact is introduced).
    if name == "cf_ite" {
        if let Some(args) = t.get("args").and_then(|v| v.as_array()) {
            let cond = args.first();
            // arg0: the guard term, evaluated in the enclosing context.
            if let Some(c) = cond {
                walk_term(c, property_name, property_cid, pool, None, path_cond, out);
            }
            // arg1: the then-branch, dominated by the positive guard.
            if let Some(then_t) = args.get(1) {
                let mut then_pc = path_cond.to_vec();
                if let Some(g) = cond.and_then(|c| guard_term_to_atomic(c, false)) {
                    then_pc.push(g);
                }
                walk_term(then_t, property_name, property_cid, pool, None, &then_pc, out);
            }
            // arg2: the else-branch, dominated by the NEGATED guard.
            if let Some(else_t) = args.get(2) {
                let mut else_pc = path_cond.to_vec();
                if let Some(g) = cond.and_then(|c| guard_term_to_atomic(c, true)) {
                    else_pc.push(g);
                }
                walk_term(else_t, property_name, property_cid, pool, None, &else_pc, out);
            }
        }
    } else if let Some(args) = t.get("args").and_then(|v| v.as_array()) {
        for a in args {
            walk_term(a, property_name, property_cid, pool, None, path_cond, out);
        }
    }
}

#[cfg(test)]
mod guard_propagation_tests {
    //! PANIC-FREEDOM guard-context discharge (the soundness-critical step).
    //! A panic partial (`option_unwrap`, pre = `is_some(recv)`) discharges
    //! panic-safe ONLY when the call site is dominated by the matching guard.
    //! These tests pin the three cases at the enumeration boundary, where the
    //! dominating `cf_ite` condition is threaded into `CallSite::guard_facts`:
    //!   - guarded then-branch  -> guard_facts = [is_some(recv)]  (=> panic-safe)
    //!   - bare (no guard)      -> guard_facts = []               (=> undecidable)
    //!   - else-branch          -> guard_facts = [is_none(recv)]  (=> undecidable)
    //! The else-branch case is the trap: a positive guard must dominate ONLY the
    //! then-branch; the else-branch's negated guard never establishes the pre.

    use super::*;
    use serde_json::json;

    // The receiver term the panic obligation is about (`x` in `x.unwrap()`).
    fn recv() -> Json {
        json!({ "kind": "var", "name": "x" })
    }

    // A panic-partial call term: `option_unwrap(recv)`. Its ctor name matches
    // the bridge sourceSymbol, so it enumerates as a CallSite.
    fn unwrap_call() -> Json {
        json!({ "kind": "ctor", "name": "option_unwrap", "args": [recv()] })
    }

    // `is_some(recv)` as the lifter folds an `if x.is_some()` condition into a
    // `cf_ite` guard term (non-builtin head passes through unchanged).
    fn is_some_guard() -> Json {
        json!({ "kind": "ctor", "name": "is_some", "args": [recv()] })
    }

    // Build a pool with a single `option_unwrap` bridge and one contract whose
    // post is `result == <body>` (the self-post the call term lives in).
    fn pool_with_post(body_term: Json) -> MementoPool {
        let mut pool = MementoPool::default();
        // v1.2-layered bridge: header.kind == "bridge", sourceSymbol on header.
        let bridge = json!({
            "envelope": true,
            "header": {
                "kind": "bridge",
                "sourceSymbol": "option_unwrap",
                "targetContractCid": "blake3-512:target",
                "sourceLayer": "rust",
                "targetLayer": "rust-tests",
            }
        });
        pool.bridges_by_symbol
            .insert("option_unwrap".to_string(), bridge);
        // v1.2-layered contract whose post nests the call term.
        let contract = json!({
            "envelope": true,
            "header": {
                "kind": "contract",
                "contractName": "caller_self_post",
                "post": {
                    "kind": "atomic",
                    "name": "=",
                    "args": [ { "kind": "var", "name": "result" }, body_term ],
                }
            }
        });
        pool.mementos
            .insert("blake3-512:caller".to_string(), contract);
        pool
    }

    #[test]
    fn guarded_then_branch_threads_positive_guard() {
        // `if x.is_some() { x.unwrap() } else { 0 }`
        // -> post: result == cf_ite(is_some(x), option_unwrap(x), 0)
        let body = json!({
            "kind": "ctor",
            "name": "cf_ite",
            "args": [ is_some_guard(), unwrap_call(), { "kind": "lit", "value": 0 } ],
        });
        let sites = run(&pool_with_post(body));
        let call = sites
            .iter()
            .find(|cs| cs.bridge_ir_name == "option_unwrap")
            .expect("the unwrap call must enumerate");
        assert_eq!(
            call.guard_facts,
            vec![json!({ "kind": "atomic", "name": "is_some", "args": [recv()] })],
            "a then-branch unwrap must carry the positive is_some guard -> panic-safe"
        );
    }

    #[test]
    fn bare_unwrap_has_no_guard() {
        // `x.unwrap()` with no dominating control flow.
        // -> post: result == option_unwrap(x)
        let sites = run(&pool_with_post(unwrap_call()));
        let call = sites
            .iter()
            .find(|cs| cs.bridge_ir_name == "option_unwrap")
            .expect("the unwrap call must enumerate");
        assert!(
            call.guard_facts.is_empty(),
            "an unguarded unwrap must carry NO guard -> stays undecidable (unproven), \
             never marked panic-safe: {:?}",
            call.guard_facts
        );
    }

    #[test]
    fn else_branch_unwrap_carries_negated_guard_not_positive() {
        // `if x.is_some() { 0 } else { x.unwrap() }`  -- the trap.
        // -> post: result == cf_ite(is_some(x), 0, option_unwrap(x))
        // The else-branch unwrap is NOT established by `is_some`; it must carry
        // the NEGATED guard `is_none(x)`, which never discharges `is_some(x)`.
        let body = json!({
            "kind": "ctor",
            "name": "cf_ite",
            "args": [ is_some_guard(), { "kind": "lit", "value": 0 }, unwrap_call() ],
        });
        let sites = run(&pool_with_post(body));
        let call = sites
            .iter()
            .find(|cs| cs.bridge_ir_name == "option_unwrap")
            .expect("the unwrap call must enumerate");
        assert_eq!(
            call.guard_facts,
            vec![json!({ "kind": "atomic", "name": "is_none", "args": [recv()] })],
            "an else-branch unwrap must carry the NEGATED guard (is_none), which does \
             NOT establish is_some -> stays undecidable, NOT panic-safe"
        );
        assert!(
            !call
                .guard_facts
                .iter()
                .any(|g| g.get("name").and_then(|v| v.as_str()) == Some("is_some")),
            "the else-branch must never carry the positive is_some guard (the trap)"
        );
    }
}
