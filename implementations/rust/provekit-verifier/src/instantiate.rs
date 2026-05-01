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
    let arg = arg_term
        .as_ref()
        .ok_or("no argument term to substitute")?;
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
    let sort = f
        .get("sort")
        .ok_or("forall has no sort")?
        .clone();
    let body = f
        .get("body")
        .ok_or("forall has no body")?;
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

/// Public adapter so other stages (notably the handshake's
/// implication-form obligation builder) can reuse the same
/// alpha-renaming helper.
pub fn substitute_formula_pub(f: &Json, name: &str, replacement: &Json) -> Json {
    substitute_formula(f, name, replacement)
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
