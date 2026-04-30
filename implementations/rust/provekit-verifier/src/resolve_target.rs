// SPDX-License-Identifier: Apache-2.0
//
// Stage 3: resolve_target. Look up the CallSite's bridge.targetCid in
// the pool; return the target contract memento's `pre` formula as the
// discharge target. Mirrors .../verifier/resolve_target.cpp.

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
    let ir_formula = body.get("pre").cloned();
    Ok(ResolvedProperty {
        cid: cs.bridge_target_cid.clone(),
        ir_formula,
        ir_kit_version: String::new(),
    })
}
