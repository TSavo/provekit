// SPDX-License-Identifier: Apache-2.0
//
// Stage 2: enumerate_callsites. For every contract memento in the
// pool, walk its pre/post/inv looking for ctor terms whose `name`
// matches a known bridge sourceSymbol. Each hit is a CallSite.
//
// Mirrors implementations/cpp/.../verifier/enumerate_callsites.cpp.

use serde_json::Value as Json;

use crate::types::{memento_body, memento_kind, CallSite, MementoPool};

pub fn run(pool: &MementoPool) -> Vec<CallSite> {
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
                    walk_term(a, property_name, property_cid, pool, Some(f), out);
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

fn walk_term(
    t: &Json,
    property_name: &str,
    property_cid: &str,
    pool: &MementoPool,
    containing_atomic: Option<&Json>,
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
        };
        out.push(cs);
    }
    // Descend into the call's arguments. A nested call is no longer a
    // direct argument of `containing_atomic`, so stop threading it: only a
    // call DIRECTLY under an atomic carries that atomic as its `Q` source.
    if let Some(args) = t.get("args").and_then(|v| v.as_array()) {
        for a in args {
            walk_term(a, property_name, property_cid, pool, None, out);
        }
    }
}
