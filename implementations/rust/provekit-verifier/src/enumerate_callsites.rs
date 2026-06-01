// SPDX-License-Identifier: Apache-2.0
//
// Stage 2: enumerate_callsites. For every contract memento in the
// pool, walk its pre/post/inv looking for ctor terms whose `name`
// matches a known bridge sourceSymbol. Each hit is a CallSite.
//
// Mirrors implementations/cpp/.../verifier/enumerate_callsites.cpp.

use serde_json::Value as Json;
use tracing::{debug, info, warn};

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
        let callsite_bundle_cid = bundle_containing_member(pool, cid);
        // PANIC-LOCUS PRESERVATION (#1745): the per-occurrence source loci the
        // lifter stamped on THIS contract, each `{argTerm, file, line, col,
        // callee}`. A panic-leaf call (`x.unwrap()`) lifts to the abstract ctor
        // `method:unwrap` with no source span; the bridge index is per-symbol
        // (last-writer-wins), so two functions both calling `.unwrap()` would
        // otherwise collapse onto one call-site line. The locus lives on the
        // contract the occurrence belongs to, so we read it HERE, scoped to this
        // contract, and match an occurrence to its locus by the lifted argument
        // term (see `panic_line_for`). Absent/empty -> panic sites in this
        // contract carry no line (honestly undecidable), never a collapsed one.
        let panic_loci: &[Json] = body
            .get("panicLoci")
            .and_then(|v| v.as_array())
            .map(|a| a.as_slice())
            .unwrap_or(&[]);
        for slot in ["pre", "post", "inv"] {
            if let Some(f) = body.get(slot) {
                if f.is_object() {
                    walk_formula(
                        f,
                        &property_name,
                        cid,
                        pool,
                        callsite_bundle_cid.as_deref(),
                        panic_loci,
                        &mut out,
                    );
                }
            }
        }
        for locus in panic_loci {
            if let Some(cs) = callsite_from_panic_locus(
                locus,
                &property_name,
                cid,
                pool,
                callsite_bundle_cid.as_deref(),
            ) {
                if !has_same_panic_callsite(&out, &cs) {
                    out.push(cs);
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

fn callsite_from_panic_locus(
    locus: &Json,
    property_name: &str,
    property_cid: &str,
    pool: &MementoPool,
    callsite_bundle_cid: Option<&str>,
) -> Option<CallSite> {
    if !locus.is_object() {
        return None;
    }
    let callee = locus
        .get("callee")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())?;
    let file = locus
        .get("file")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    let line = locus
        .get("line")
        .or_else(|| locus.get("start_line"))
        .and_then(|v| v.as_u64())
        .map(|n| n as usize);
    let arg_term = locus.get("argTerm").cloned();

    let scoped_bridge = match (callsite_bundle_cid, file.as_deref(), line) {
        (Some(bundle), Some(file), Some(line)) => pool.bridges_by_callsite.get(&(
            bundle.to_string(),
            file.to_string(),
            line,
            callee.to_string(),
        )),
        _ => None,
    };
    let bridge_env = scoped_bridge.or_else(|| pool.bridges_by_symbol.get(callee));
    let bridge_body = bridge_env
        .and_then(memento_body)
        .cloned()
        .unwrap_or_else(|| serde_json::json!({}));
    if bridge_env.is_none() {
        warn!(
            callee = %callee,
            file = ?file,
            line = ?line,
            "enumerate_callsites: panicLoci entry has no bridge; surfacing an undecidable panic callsite"
        );
    }

    let bridge_self_bundle_cid = if scoped_bridge.is_some() {
        callsite_bundle_cid.map(str::to_string)
    } else {
        pool.bridge_self_bundle_by_symbol.get(callee).cloned()
    };

    Some(CallSite {
        bridge_ir_name: callee.to_string(),
        bridge_target_cid: bridge_body
            .get("targetContractCid")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string(),
        bridge_source_layer: bridge_body
            .get("sourceLayer")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string(),
        bridge_target_layer: bridge_body
            .get("targetLayer")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string(),
        bridge_target_proof_cid: bridge_body
            .get("targetProofCid")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .map(str::to_string),
        bridge_self_bundle_cid,
        property_name: property_name.to_string(),
        property_cid: property_cid.to_string(),
        callsite_bundle_cid: callsite_bundle_cid.map(str::to_string),
        arg_term,
        containing_atomic: None,
        guard_facts: Vec::new(),
        file,
        line,
        callee: Some(callee.to_string()),
        panic_site: true,
    })
}

fn has_same_panic_callsite(existing: &[CallSite], candidate: &CallSite) -> bool {
    existing.iter().any(|cs| {
        cs.panic_site
            && cs.property_cid == candidate.property_cid
            && cs.bridge_ir_name == candidate.bridge_ir_name
            && cs.file == candidate.file
            && cs.line == candidate.line
            && cs.arg_term == candidate.arg_term
    })
}

fn bundle_containing_member(pool: &MementoPool, member_cid: &str) -> Option<String> {
    pool.bundle_members
        .iter()
        .find_map(|(bundle_cid, members)| {
            members
                .contains(member_cid)
                .then(|| bundle_cid.to_string())
        })
}

fn walk_formula(
    f: &Json,
    property_name: &str,
    property_cid: &str,
    pool: &MementoPool,
    callsite_bundle_cid: Option<&str>,
    // PANIC-LOCUS PRESERVATION (#1745): the loci stamped on the contract being
    // walked, threaded down so a panic-site occurrence can resolve its OWN line.
    panic_loci: &[Json],
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
                    walk_term(
                        a,
                        property_name,
                        property_cid,
                        pool,
                        Some(f),
                        &[],
                        callsite_bundle_cid,
                        panic_loci,
                        out,
                    );
                }
            }
        }
        "and" | "or" | "not" | "implies" => {
            if let Some(ops) = f.get("operands").and_then(|v| v.as_array()) {
                for op in ops {
                    walk_formula(
                        op,
                        property_name,
                        property_cid,
                        pool,
                        callsite_bundle_cid,
                        panic_loci,
                        out,
                    );
                }
            }
        }
        "forall" | "exists" => {
            if let Some(b) = f.get("body") {
                if b.is_object() {
                    walk_formula(
                        b,
                        property_name,
                        property_cid,
                        pool,
                        callsite_bundle_cid,
                        panic_loci,
                        out,
                    );
                }
            }
        }
        _ => {}
    }
}

/// Convert an OPAQUE `cf_guarded` guard term into the atomic-predicate FORMULA
/// the verifier threads into the path condition. NAME-BLIND by construction:
/// it copies the guard ctor's `name` and `args` verbatim into an `atomic` with
/// NO recognition table and NO complement logic. The Rust kit (the lifter, see
/// `provekit-walk` `wrap_branch_guard`) has ALREADY resolved which predicate
/// governs a branch -- the then-branch carries the positive predicate atom
/// (`is_some(x)`), the else-branch carries the kit-computed complement
/// (`is_none(x)`). This verifier carries whatever atom the kit emitted; it does
/// not know Option/Result/collection complementarity, and recognizes no Rust
/// predicate name. The language-blindness invariant lives or dies here.
fn guarded_term_to_atomic(guard: &Json) -> Option<Json> {
    if guard.get("kind").and_then(|v| v.as_str()) != Some("ctor") {
        return None;
    }
    let head = guard.get("name").and_then(|v| v.as_str())?;
    let args = guard
        .get("args")
        .cloned()
        .unwrap_or_else(|| serde_json::json!([]));
    // Opaque copy: head is carried through unchanged. The verifier asserts no
    // semantics over it; downstream discharge is purely syntactic (the threaded
    // atom must match the target contract's instantiated `pre` byte-for-byte).
    Some(serde_json::json!({ "kind": "atomic", "name": head, "args": args }))
}

/// PANIC-LOCUS PRESERVATION (#1745): resolve a panic-site occurrence's OWN
/// source `(file, line, col)` from the contract's `panicLoci`, matching by the
/// lifted argument term.
///
/// `arg_term` is the bridged ctor's first argument as it appears in the contract
/// formula (the unwrap RECEIVER, e.g. `to_string(v)`). The lifter recorded each
/// panic leaf keyed by that SAME lifted term (via the same `lift_expr_to_term`),
/// so a byte-equal `argTerm` uniquely identifies the occurrence WITHIN this
/// contract. This is a content match, not a positional one: two `.unwrap()`
/// calls in one function on different receivers each find their own line
/// regardless of walk order.
///
/// Returns `None` when the term matches no locus (the occurrence then carries no
/// line and stays honestly undecidable -- fail-SAFE, never the collapsed
/// per-symbol line). For the degenerate case of two byte-identical occurrences
/// in one contract (same receiver, same lifted term => genuinely identical
/// obligation, same verdict), the first locus is returned; the lines are a
/// cosmetic tie, not a soundness question.
fn panic_line_for<'a>(arg_term: Option<&Json>, panic_loci: &'a [Json]) -> Option<&'a Json> {
    let arg = arg_term?;
    panic_loci
        .iter()
        .find(|locus| locus.get("argTerm") == Some(arg))
}

#[allow(clippy::too_many_arguments)]
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
    // The bundle containing the contract being walked. For panic sites, this is
    // the caller bundle that also contains co-located producer bridges.
    callsite_bundle_cid: Option<&str>,
    // PANIC-LOCUS PRESERVATION (#1745): the panic loci of the contract being
    // walked. A panic site reads its own line from here, keyed by arg_term.
    panic_loci: &[Json],
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
        let bridge_callsite = bbody.get("callsite");
        let callsite_callee = bbody
            .get("sourceSymbol")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string());
        let panic_site = bridge_callsite
            .and_then(|v| v.get("panicSite"))
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        // The bridged ctor's first argument is the obligation's receiver
        // (e.g. `to_string(v)` in `to_string(v).unwrap()`); it is both the
        // CallSite's `arg_term` AND the key that pairs a panic occurrence with
        // its source locus in THIS contract.
        let arg_term = t
            .get("args")
            .and_then(|v| v.as_array())
            .and_then(|arr| arr.first().cloned());
        // PANIC-LOCUS PRESERVATION (#1745): a panic site reads its line/col/file
        // from the contract's own `panicLoci`, keyed by `arg_term` -- NOT from
        // the per-symbol bridge `callsite` (last-writer-wins, collapses two
        // distinct `.unwrap()` lines to one). The bridge `callsite` still
        // classifies `panicSite`, but its line is occurrence-collapsed and must
        // NOT be the source of truth for a panic obligation's locus. A non-panic
        // bridged call keeps reading its line from the bridge as before (those
        // are 1:1 with their bridge and are not collapsed across occurrences).
        let occ_locus = if panic_site {
            panic_line_for(arg_term.as_ref(), panic_loci)
        } else {
            None
        };
        let (callsite_file, callsite_line) = if let Some(locus) = occ_locus {
            let f = locus
                .get("file")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string());
            let l = locus
                .get("line")
                .or_else(|| locus.get("start_line"))
                .and_then(|v| v.as_u64())
                .map(|n| n as usize);
            (f, l)
        } else if panic_site {
            // No matching locus for a panic site: carry NO line. The collapsed
            // bridge line must never stand in for the real occurrence -- that is
            // exactly the silent mis-attribution this fix removes. The site then
            // stays honestly undecidable downstream.
            if panic_loci.is_empty() {
                debug!(
                    name = %name,
                    "enumerate_callsites: panic site with no contract panicLoci -- \
                     carrying no line (occurrence locus unavailable; honestly undecidable)"
                );
            } else {
                warn!(
                    name = %name,
                    "enumerate_callsites: panic site arg_term matched no contract locus -- \
                     carrying no line rather than the collapsed per-symbol bridge line \
                     (panic-locus miss; site stays undecidable)"
                );
            }
            (None, None)
        } else {
            // Non-panic bridged call: keep the bridge-carried locus (1:1, not
            // subject to the per-symbol panic collapse).
            let f = bridge_callsite
                .and_then(|v| v.get("file"))
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string());
            let l = bridge_callsite
                .and_then(|v| v.get("start_line").or_else(|| v.get("line")))
                .and_then(|v| v.as_u64())
                .map(|n| n as usize);
            (f, l)
        };
        if panic_site {
            debug!(
                name = %name,
                line = ?callsite_line,
                file = ?callsite_file,
                matched_locus = occ_locus.is_some(),
                "enumerate_callsites: panic-site locus resolved from contract panicLoci by arg_term"
            );
        }
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
            bridge_self_bundle_cid: pool.bridge_self_bundle_by_symbol.get(&name).cloned(),
            property_name: property_name.to_string(),
            property_cid: property_cid.to_string(),
            callsite_bundle_cid: callsite_bundle_cid.map(str::to_string),
            arg_term: arg_term.clone(),
            containing_atomic: containing_atomic.cloned(),
            // Snapshot the dominating guard context for this call site. The
            // runner discharges a panic partial's `pre` under these facts.
            guard_facts: path_cond.to_vec(),
            file: callsite_file,
            line: callsite_line,
            callee: callsite_callee,
            panic_site,
        };
        debug!(
            name = %name,
            panic_site,
            callsite_present = bridge_callsite.is_some(),
            arg_term_kind = ?cs.arg_term.as_ref().and_then(|a| a.get("kind")).and_then(|k| k.as_str()),
            "enumerate_callsites: enumerated bridge call site"
        );
        // NO-SILENT-FAILURE (Phase 0): a `method:`-seam bridge is the protocol's
        // method-call ctor (language-blind seam from the lift grammar). It MUST
        // carry call-site provenance; a missing `callsite` field means the mint
        // dropped it, which silently reads back `panic_site=false` and sends a
        // real panic leaf to undecidable. Surface it loudly and count it instead
        // of letting K silently rot. (Function-level bridges have no `method:`
        // seam, so they do not trip this.)
        if name.starts_with("method:") && bridge_callsite.is_none() {
            warn!(
                bridge = %name,
                "enumerate_callsites: method-seam bridge has NO callsite provenance -- mint dropped it; \
                 this panic site will read panic_site=false and stay undecidable (callsite-provenance drop)"
            );
        }
        out.push(cs);
    }
    // Descend into the call's arguments. A nested call is no longer a
    // direct argument of `containing_atomic`, so stop threading it: only a
    // call DIRECTLY under an atomic carries that atomic as its `Q` source.
    //
    // PANIC-FREEDOM path condition. The guard that dominates a branch is no
    // longer recovered HERE by recognizing the `cf_ite` condition's head -- the
    // verifier knows no Rust predicate names. Instead the Rust kit emits the
    // resolved guard ON the dominated branch as a `cf_guarded(guard, value)`
    // wrapper (then-branch -> positive predicate, else-branch -> kit-computed
    // complement; see `provekit-walk` `wrap_branch_guard`). This verifier:
    //   * `cf_ite(cond, then, else)`: descends all three branches with the
    //     path condition UNCHANGED. arg0 (the condition) introduces no fact;
    //     any dominating fact rides on the `cf_guarded` wrapper the kit placed
    //     around `then`/`else`.
    //   * `cf_guarded(guard, value)`: copies the OPAQUE guard atom into the
    //     path condition (name-blind, no complement table) and descends `value`
    //     under it. A branch the kit did not wrap (unrecognized guard) carries
    //     no `cf_guarded`, so a partial inside it stays honestly undecidable.
    //   * any other ctor: descends args with the path condition unchanged.
    if name == "cf_guarded" {
        if let Some(args) = t.get("args").and_then(|v| v.as_array()) {
            let guard = args.first();
            let value = args.get(1);
            let mut branch_pc = path_cond.to_vec();
            if let Some(g) = guard.and_then(guarded_term_to_atomic) {
                branch_pc.push(g);
            }
            if let Some(v) = value {
                walk_term(
                    v,
                    property_name,
                    property_cid,
                    pool,
                    None,
                    &branch_pc,
                    callsite_bundle_cid,
                    panic_loci,
                    out,
                );
            }
            // The guard term itself is a predicate over the receiver, not a
            // call value; do not descend it as a callsite source.
        }
    } else if name == "cf_ite" {
        if let Some(args) = t.get("args").and_then(|v| v.as_array()) {
            // arg0: the condition term, evaluated in the enclosing context. It
            // introduces no path fact (the dominating fact rides cf_guarded).
            if let Some(c) = args.first() {
                walk_term(
                    c,
                    property_name,
                    property_cid,
                    pool,
                    None,
                    path_cond,
                    callsite_bundle_cid,
                    panic_loci,
                    out,
                );
            }
            // arg1 (then) / arg2 (else): descend unchanged. A guarded branch is
            // a `cf_guarded` wrapper handled above; an unguarded branch carries
            // the inherited path condition only.
            for branch in [args.get(1), args.get(2)].into_iter().flatten() {
                walk_term(
                    branch,
                    property_name,
                    property_cid,
                    pool,
                    None,
                    path_cond,
                    callsite_bundle_cid,
                    panic_loci,
                    out,
                );
            }
        }
    } else if let Some(args) = t.get("args").and_then(|v| v.as_array()) {
        // NESTED-CALL threading: when the current ctor has NO bridge (is not
        // itself a callsite), the enclosing atomic predicate is still the
        // correct `Q` source for any bridged call nested inside it. Thread
        // `containing_atomic` through so the inner callsite can use the outer
        // predicate for the reduce-in-place discharge path.
        //
        // When the current ctor IS bridged (it captured the atomic as its own
        // callsite above), the inner args are sub-obligations of that callee,
        // not of the outer predicate. Stop threading (pass `None`) to avoid
        // conflating the outer predicate with a sub-obligation the inner call
        // is not directly participating in.
        let inner_atomic = if pool.bridges_by_symbol.contains_key(&name) {
            None
        } else {
            containing_atomic
        };
        for a in args {
            walk_term(
                a,
                property_name,
                property_cid,
                pool,
                inner_atomic,
                path_cond,
                callsite_bundle_cid,
                panic_loci,
                out,
            );
        }
    }
}

#[cfg(test)]
mod guard_propagation_tests {
    //! PANIC-FREEDOM guard-context threading at the enumeration boundary, tested
    //! WITHOUT any Rust predicate name. The verifier is language-blind: it does
    //! not know `is_some`'s complement is `is_none`, nor that `option_unwrap`'s
    //! pre is `is_some`. The Rust kit (`provekit-walk` `wrap_branch_guard`) has
    //! ALREADY resolved which predicate governs each branch and emitted it as a
    //! `cf_guarded(guard, value)` wrapper. The verifier's only job is to copy
    //! whatever OPAQUE atom rides that wrapper into `CallSite::guard_facts`.
    //!
    //! So these tests use opaque names (`pred_a`, `pred_b`, `panic_call`) with
    //! no semantic table behind them:
    //!   - wrapped call    -> guard_facts = [pred_a(x)]   (the kit's atom, verbatim)
    //!   - unwrapped call  -> guard_facts = []            (undecidable -- no fact)
    //!   - cf_ite descent  -> the condition introduces NO fact; only the
    //!                        cf_guarded wrapper the kit placed carries one.
    //! The NAMED then->positive / else->complement discrimination is a Rust-kit
    //! property and is pinned in `provekit-walk`'s lift tests, not here.

    use super::*;
    use serde_json::json;

    // The receiver term the obligation is about (`x` in `x.panic_call()`).
    fn recv() -> Json {
        json!({ "kind": "var", "name": "x" })
    }

    // A panic-partial call term whose ctor name matches the bridge sourceSymbol,
    // so it enumerates as a CallSite. The name is OPAQUE to the verifier.
    fn panic_call() -> Json {
        json!({ "kind": "ctor", "name": "panic_call", "args": [recv()] })
    }

    // An OPAQUE guard predicate atom term -- whatever the kit resolved for this
    // branch. The verifier carries it through with no recognition.
    fn pred(name: &str) -> Json {
        json!({ "kind": "ctor", "name": name, "args": [recv()] })
    }

    // Wrap a value in the kit's `cf_guarded(guard, value)` carrier.
    fn cf_guarded(guard: Json, value: Json) -> Json {
        json!({ "kind": "ctor", "name": "cf_guarded", "args": [guard, value] })
    }

    // Build a pool with a single `panic_call` bridge and one contract whose
    // post is `result == <body>` (the self-post the call term lives in).
    fn pool_with_post(body_term: Json) -> MementoPool {
        let mut pool = MementoPool::default();
        let bridge = json!({
            "envelope": true,
            "header": {
                "kind": "bridge",
                "sourceSymbol": "panic_call",
                "targetContractCid": "blake3-512:target",
                "sourceLayer": "rust",
                "targetLayer": "rust-tests",
            }
        });
        pool.bridges_by_symbol
            .insert("panic_call".to_string(), bridge);
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

    fn enumerated_call(sites: &[CallSite]) -> &CallSite {
        sites
            .iter()
            .find(|cs| cs.bridge_ir_name == "panic_call")
            .expect("the panic call must enumerate")
    }

    #[test]
    fn cf_guarded_threads_the_opaque_atom_verbatim() {
        // The kit wrapped the call: cf_guarded(pred_a(x), panic_call(x)). The
        // verifier copies `pred_a(x)` into guard_facts as an atomic, byte-blind.
        let body = cf_guarded(pred("pred_a"), panic_call());
        let sites = run(&pool_with_post(body));
        assert_eq!(
            enumerated_call(&sites).guard_facts,
            vec![json!({ "kind": "atomic", "name": "pred_a", "args": [recv()] })],
            "a cf_guarded-wrapped call must carry the kit's opaque guard atom verbatim"
        );
    }

    #[test]
    fn unwrapped_call_has_no_guard() {
        // No cf_guarded wrapper -> no dominating fact -> undecidable.
        let sites = run(&pool_with_post(panic_call()));
        assert!(
            enumerated_call(&sites).guard_facts.is_empty(),
            "an unwrapped call must carry NO guard -> stays undecidable, never panic-safe: {:?}",
            enumerated_call(&sites).guard_facts
        );
    }

    #[test]
    fn cf_ite_condition_introduces_no_fact_only_the_wrapper_does() {
        // cf_ite(cond, cf_guarded(pred_a, panic_call), cf_guarded(pred_b, 0)).
        // The verifier reads the guard ONLY off the cf_guarded wrapper the kit
        // placed on the then-branch -- it does NOT derive anything from `cond`.
        // (cond uses an opaque head; the verifier must not recognize it.)
        let body = json!({
            "kind": "ctor",
            "name": "cf_ite",
            "args": [
                pred("some_condition"),
                cf_guarded(pred("pred_a"), panic_call()),
                cf_guarded(pred("pred_b"), json!({ "kind": "lit", "value": 0 })),
            ],
        });
        let sites = run(&pool_with_post(body));
        assert_eq!(
            enumerated_call(&sites).guard_facts,
            vec![json!({ "kind": "atomic", "name": "pred_a", "args": [recv()] })],
            "the call must carry ONLY the kit's then-branch wrapper atom, nothing from cond"
        );
    }

    #[test]
    fn cf_ite_unwrapped_branch_carries_no_fact() {
        // An else-branch the kit did NOT wrap (e.g. its complement was not a
        // partial-pre-establishing predicate): the call there stays unguarded.
        let body = json!({
            "kind": "ctor",
            "name": "cf_ite",
            "args": [
                pred("some_condition"),
                { "kind": "lit", "value": 0 },
                panic_call(), // bare, no cf_guarded wrapper
            ],
        });
        let sites = run(&pool_with_post(body));
        assert!(
            enumerated_call(&sites).guard_facts.is_empty(),
            "an unwrapped cf_ite branch must carry no fact -> undecidable"
        );
    }

    #[test]
    fn cf_guarded_with_non_ctor_guard_adds_no_fact() {
        // Robustness: a malformed guard (not a ctor) yields no atom; the call
        // descends with the inherited (empty) path condition.
        let body = json!({
            "kind": "ctor",
            "name": "cf_guarded",
            "args": [ { "kind": "var", "name": "not_a_predicate" }, panic_call() ],
        });
        let sites = run(&pool_with_post(body));
        assert!(
            enumerated_call(&sites).guard_facts.is_empty(),
            "a non-ctor guard must add no fact"
        );
    }
}
