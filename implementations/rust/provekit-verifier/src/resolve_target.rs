// SPDX-License-Identifier: Apache-2.0
//
// Stage 3: resolve_target. Look up the CallSite's bridge.targetCid in
// the pool; return the target contract memento's `pre` formula as the
// discharge target. Mirrors .../verifier/resolve_target.cpp.
//
// Forward-pin gate (BridgeDeclaration.ConsequentBundlePinned, NORMATIVE):
// after locating the consequent contract member, refuse to consume it
// unless its containing `.proof` bundle CID matches the bridge's
// `targetProofCid`. See protocol/specs/2026-04-30-ir-formal-grammar.md
// § "Bridge target pinning: the shim-poisoning vector".

use serde_json::{json, Value as Json};

use crate::types::{memento_body, memento_kind, CallSite, MementoPool, ResolvedProperty};

pub fn run(cs: &CallSite, pool: &MementoPool) -> Result<ResolvedProperty, String> {
    let env = pool
        .mementos
        .get(&cs.bridge_target_cid)
        .ok_or_else(|| format!("bridge target CID {} not in pool", cs.bridge_target_cid))?;
    if memento_kind(env) != Some("contract") {
        return Err("target memento is not a contract memento".into());
    }
    // Shape-agnostic body: v1.2 layered -> `header`, v1.1 flat -> `evidence.body`.
    let body = memento_body(env)
        .filter(|v| v.is_object())
        .ok_or("contract memento has no body/header object")?;

    // Forward pin: BridgeDeclaration.ConsequentBundlePinned.
    //
    //     ∀b: BridgeDeclaration, P: ProofBundle →
    //       AcceptedAsConsequentFor(P, b) ⇒ Cid(P) = b.targetProofCid
    //
    // If the bridge pins a target proof CID, the contract member we just
    // resolved MUST come from that bundle. Bundles whose contract
    // members happen to share `targetContractCid` MUST NOT be
    // substituted for the pinned bundle. See protocol/specs/2026-04-30-
    // ir-formal-grammar.md § "Bridge target pinning: the shim-poisoning
    // vector".
    match cs.bridge_target_proof_cid.as_deref() {
        Some(expected_bundle) => {
            let bundle_members = pool.bundle_members.get(expected_bundle).ok_or_else(|| {
                format!(
                    "BridgeTargetProofCidMismatch: pinned bundle {} not in pool",
                    expected_bundle
                )
            })?;
            if !bundle_members.contains(&cs.bridge_target_cid) {
                return Err(format!(
                    "BridgeTargetProofCidMismatch: contract {} is not a member of pinned bundle {}",
                    cs.bridge_target_cid, expected_bundle
                ));
            }
        }
        None => {
            // Back-compat: legacy bridges that pre-date `targetProofCid`
            // are loadable but cannot have ConsequentBundlePinned
            // enforced. New bridges MUST set the field; flag the gap so
            // operators can see what isn't being checked.
            eprintln!(
                "warning: bridge {} has no targetProofCid; \
                 ConsequentBundlePinned not enforced (back-compat path)",
                cs.bridge_ir_name
            );
        }
    }

    // A body-derived contract emits its precondition as a BARE predicate over
    // a named formal (e.g. `encoding >= 0`). The discharge stages
    // (`instantiate`, `build_implication_obligation`) require it quantified, so
    // the actual passed at the callsite can be substituted for the formal.
    // Synthesize `forall (formal). pre` here from the contract's own
    // `formals`/`formalSorts`. Single-formal: the callsite model tracks a
    // single arg term, so we bind the first formal. An already-quantified pre
    // (hand-built bundles) passes through untouched.
    let ir_formula = body.get("pre").cloned().map(|pre| wrap_pre_forall(pre, body));
    // A target carrying a `formals` array is a body-derived op-contract
    // (body-bearing). The caller must NOT vacuous-pass such a target if its
    // obligation was not reduced + discharged; it must refuse. Surface the
    // marker here, where the body is already in hand.
    //
    // PRESENCE, not non-emptiness, is the marker: a zero-arg body-derived
    // contract carries `formals: []` (a body, no parameters) and is still
    // body-bearing. Gating on `!is_empty()` would let `formals: []` + a
    // non-equation post + no pre slip back into the vacuous-discharge branch.
    // A genuinely non-body-bearing target (e.g. a LIA refinement contract)
    // carries no `formals` key at all, so it stays on the legitimate path.
    let target_is_body_bearing = body.get("formals").and_then(|v| v.as_array()).is_some();
    Ok(ResolvedProperty {
        cid: cs.bridge_target_cid.clone(),
        ir_formula,
        ir_kit_version: String::new(),
        target_is_body_bearing,
    })
}

/// Wrap a bare precondition formula in `forall (firstFormal: sort). pre` so the
/// discharge stages can substitute the callsite's actual for the formal. If the
/// pre is already a `forall`, or the contract carries no `formals`, it is
/// returned unchanged (no double-wrap; nothing to quantify).
fn wrap_pre_forall(pre: Json, body: &Json) -> Json {
    if pre.get("kind").and_then(|v| v.as_str()) == Some("forall") {
        return pre;
    }
    let Some(name) = body
        .get("formals")
        .and_then(|v| v.as_array())
        .and_then(|a| a.first())
        .and_then(|v| v.as_str())
    else {
        return pre;
    };
    // The formal's sort is already canonical (the lifter emits the canonical
    // primitive set; integers are `Int`). Use it directly; default to `Int`
    // when a contract omits formalSorts.
    let sort = body
        .get("formalSorts")
        .and_then(|v| v.as_array())
        .and_then(|a| a.first())
        .cloned()
        .unwrap_or_else(|| json!({"kind": "primitive", "name": "Int"}));
    json!({"kind": "forall", "name": name, "sort": sort, "body": pre})
}
