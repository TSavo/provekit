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

use crate::types::{CallSite, MementoPool, ResolvedProperty};

pub fn run(cs: &CallSite, pool: &MementoPool) -> Result<ResolvedProperty, String> {
    let env = pool
        .mementos
        .get(&cs.bridge_target_cid)
        .ok_or_else(|| format!("bridge target CID {} not in pool", cs.bridge_target_cid))?;
    let ev = env
        .get("evidence")
        .filter(|v| v.is_object())
        .ok_or("target memento has no evidence object")?;
    if ev.get("kind").and_then(|k| k.as_str()) != Some("contract") {
        return Err("target memento is not a contract memento".into());
    }
    let body = ev
        .get("body")
        .filter(|v| v.is_object())
        .ok_or("contract evidence has no body object")?;

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

    let ir_formula = body.get("pre").cloned();
    Ok(ResolvedProperty {
        cid: cs.bridge_target_cid.clone(),
        ir_formula,
        ir_kit_version: String::new(),
    })
}
