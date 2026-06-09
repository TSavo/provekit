// SPDX-License-Identifier: Apache-2.0
//
// Stage 4: instantiate. Substitute the call's arg term for the
// resolved forall's bound variable. Flat quantifier shape: the
// resolved formula is expected to be `{kind:"forall", name, sort, body}`;
// we substitute `arg_term` for `name` in `body`.
//
// Mirrors .../verifier/instantiate.cpp.

use serde_json::{json, Value as Json};

use crate::types::{Obligation, ResolvedProperty};

pub fn run(resolved: &ResolvedProperty, arg_term: &Option<Json>) -> Result<Obligation, String> {
    let arg = arg_term.as_ref().ok_or("no argument term to substitute")?;
    let f = resolved
        .ir_formula
        .as_ref()
        .ok_or("resolved property has no ir_formula (no pre slot)")?;
    if f.get("kind").and_then(|v| v.as_str()) != Some("forall") {
        return Err("precondition formula is not a forall".into());
    }
    let var_name = f
        .get("name")
        .and_then(|v| v.as_str())
        .ok_or("forall has empty bound-variable name")?;
    let sort = f.get("sort").ok_or("forall has no sort")?.clone();
    let body = f.get("body").ok_or("forall has no body")?;
    let substituted_body = substitute_formula(body, var_name, arg);
    let forall_with_sort = json!({
        "kind": "forall",
        "name": var_name,
        "sort": sort,
        "body": substituted_body
    });
    Ok(Obligation {
        property_cid: resolved.cid.clone(),
        ir_kit_version: resolved.ir_kit_version.clone(),
        ir_formula: forall_with_sort,
    })
}

/// Specialize a target precondition to the concrete callsite actuals.
///
/// This is the value-level seam obligation used when the caller directly calls
/// a precondition-bearing target: substitute every target formal with the
/// corresponding bridged ctor argument and return the bare specialized
/// predicate. The legacy `run` API above intentionally preserves its quantified
/// wrapper for older single-formal paths; this helper is for actual callsite
/// discharge.
pub fn run_specialized(
    resolved: &ResolvedProperty,
    arg_terms: &[Json],
) -> Result<Obligation, String> {
    let f = resolved
        .ir_formula
        .as_ref()
        .ok_or("resolved property has no ir_formula (no pre slot)")?;
    if f.get("kind").and_then(|v| v.as_str()) != Some("forall") {
        return Err("precondition formula is not a forall".into());
    }
    let fallback_name = f
        .get("name")
        .and_then(|v| v.as_str())
        .ok_or("forall has empty bound-variable name")?
        .to_string();
    let body = f.get("body").ok_or("forall has no body")?;
    let formal_names = if resolved.formal_names.is_empty() {
        vec![fallback_name]
    } else {
        resolved.formal_names.clone()
    };
    if arg_terms.len() < formal_names.len() {
        return Err(format!(
            "not enough actual terms to specialize precondition: need {}, got {}",
            formal_names.len(),
            arg_terms.len()
        ));
    }
    let mut substituted = body.clone();
    for (name, actual) in formal_names.iter().zip(arg_terms.iter()) {
        substituted = substitute_formula(&substituted, name, actual);
    }
    Ok(Obligation {
        property_cid: resolved.cid.clone(),
        ir_kit_version: resolved.ir_kit_version.clone(),
        ir_formula: substituted,
    })
}

/// Public adapter so other stages (notably the handshake's
/// implication-form obligation builder) can reuse the same
/// alpha-renaming helper.
pub fn substitute_formula_pub(f: &Json, name: &str, replacement: &Json) -> Json {
    substitute_formula(f, name, replacement)
}

/// Strip ONE redundant outer `forall` from an already-instantiated
/// precondition, returning its body.
///
/// `instantiate::run` substitutes the call's actual argument into the
/// resolved pre's forall body and then RE-WRAPS the result in a forall that
/// re-binds the same formal name. For the panic-freedom GUARD-DISCHARGE
/// obligation this is variable capture: the guard fact (`is_some(opt)`) has a
/// FREE `opt`, but the re-wrapped consequent re-binds `opt`, so the implication
/// becomes `is_some(opt_free) => forall opt_bound. is_some(opt_bound)` =
/// `P(a) => forall x. P(x)`, which a solver correctly refutes. The correct
/// call-site obligation is the pre SPECIALIZED to the actual argument
/// (`pre[formal := arg]`), which is exactly the forall's body. The outer
/// binder is redundant: vacuous when arg != formal, capturing when arg ==
/// formal. Dropping it yields the bare specialized pre, so a matching guard
/// gives the valid `(=> P P)`.
///
/// ONLY the panic-guard branch calls this; the normal refinement obligation
/// keeps the quantified form so its obligation CID / hash-tier lookups are
/// unchanged. If the formula is not a `forall`, it is returned unchanged.
pub fn strip_outer_forall(f: &Json) -> Json {
    if f.get("kind").and_then(|v| v.as_str()) == Some("forall") {
        if let Some(body) = f.get("body") {
            return body.clone();
        }
    }
    f.clone()
}

fn substitute_formula(f: &Json, name: &str, replacement: &Json) -> Json {
    let mut out = f.clone();
    let kind = f.get("kind").and_then(|v| v.as_str()).unwrap_or_default();
    if let Json::Object(map) = &mut out {
        match kind {
            "atomic" => {
                if let Some(Json::Array(args)) = map.get("args").cloned() {
                    let new_args: Vec<Json> = args
                        .iter()
                        .map(|a| substitute_term(a, name, replacement))
                        .collect();
                    map.insert("args".into(), Json::Array(new_args));
                }
            }
            "and" | "or" | "not" | "implies" => {
                if let Some(Json::Array(ops)) = map.get("operands").cloned() {
                    let new_ops: Vec<Json> = ops
                        .iter()
                        .map(|op| substitute_formula(op, name, replacement))
                        .collect();
                    map.insert("operands".into(), Json::Array(new_ops));
                }
            }
            "forall" | "exists" => {
                let bound = map.get("name").and_then(|v| v.as_str()).unwrap_or_default();
                if bound == name {
                    // Shadowed; do not descend.
                    return out;
                }
                if let Some(body) = map.get("body").cloned() {
                    map.insert("body".into(), substitute_formula(&body, name, replacement));
                }
            }
            _ => {}
        }
    }
    out
}

fn substitute_term(t: &Json, name: &str, replacement: &Json) -> Json {
    if !t.is_object() {
        return t.clone();
    }
    let kind = t.get("kind").and_then(|v| v.as_str()).unwrap_or_default();
    if kind == "var" && t.get("name").and_then(|v| v.as_str()) == Some(name) {
        return replacement.clone();
    }
    if kind == "ctor" {
        let mut out = t.clone();
        if let Json::Object(map) = &mut out {
            if let Some(Json::Array(args)) = map.get("args").cloned() {
                let new_args: Vec<Json> = args
                    .iter()
                    .map(|a| substitute_term(a, name, replacement))
                    .collect();
                map.insert("args".into(), Json::Array(new_args));
            }
        }
        return out;
    }
    t.clone()
}

#[cfg(test)]
mod strip_outer_forall_tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn strips_one_outer_forall_returning_body() {
        // POSITIVE: a `forall opt. is_some(opt)` specializes to its body
        // `is_some(opt)` (the panic-pre over the free callsite arg).
        let f = json!({"kind": "forall", "name": "opt",
            "sort": {"kind": "primitive", "name": "Option<T>"},
            "body": {"kind": "atomic", "name": "is_some",
                "args": [{"kind": "var", "name": "opt"}]}});
        let stripped = strip_outer_forall(&f);
        assert_eq!(
            stripped,
            json!({"kind": "atomic", "name": "is_some",
                "args": [{"kind": "var", "name": "opt"}]})
        );
    }

    #[test]
    fn non_forall_is_returned_unchanged() {
        // DISCRIMINATION: a bare atomic (already specialized) is untouched, so
        // the strip is a safe no-op on non-quantified obligations.
        let f = json!({"kind": "atomic", "name": "is_some",
            "args": [{"kind": "var", "name": "opt"}]});
        assert_eq!(strip_outer_forall(&f), f);
        // An implication is likewise unchanged (only the OUTER forall is peeled).
        let imp = json!({"kind": "implies", "operands": [
            {"kind": "atomic", "name": "is_some", "args": []},
            {"kind": "atomic", "name": "is_some", "args": []}]});
        assert_eq!(strip_outer_forall(&imp), imp);
    }

    #[test]
    fn strips_only_the_outermost_forall() {
        // STRUCTURAL: a nested forall in the body is preserved -- only ONE
        // outer binder is removed (matching `instantiate::run`'s single
        // re-wrap).
        let inner = json!({"kind": "forall", "name": "y",
            "sort": {"kind": "primitive", "name": "Int"},
            "body": {"kind": "atomic", "name": "p",
                "args": [{"kind": "var", "name": "y"}]}});
        let f = json!({"kind": "forall", "name": "x",
            "sort": {"kind": "primitive", "name": "Int"},
            "body": inner.clone()});
        assert_eq!(strip_outer_forall(&f), inner);
    }
}
