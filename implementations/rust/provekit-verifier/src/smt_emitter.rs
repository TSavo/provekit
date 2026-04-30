// SPDX-License-Identifier: Apache-2.0
//
// Stage 5: smt_emitter. Render an obligation's IR to an SMT-LIB
// script. Mirrors .../verifier/smt_emitter.cpp.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::Value as Json;

pub fn emit(ir_formula: &Json) -> Result<String, String> {
    let body = emit_formula(ir_formula)?;

    let mut free_vars: BTreeMap<String, String> = BTreeMap::new();
    let bound: BTreeSet<String> = BTreeSet::new();
    collect_free_vars(ir_formula, &mut free_vars, &bound);

    let mut out = String::new();
    out.push_str("(set-logic ALL)\n");
    for (name, srt) in &free_vars {
        out.push_str(&format!("(declare-const {name} {srt})\n"));
    }
    out.push_str(&format!("(assert (not {body}))\n"));
    out.push_str("(check-sat)\n");
    Ok(out)
}

fn emit_term(t: &Json) -> Result<String, String> {
    if !t.is_object() {
        return Err("non-object IR term".into());
    }
    let kind = t.get("kind").and_then(|v| v.as_str()).unwrap_or_default();
    match kind {
        "var" => {
            let n = t.get("name").and_then(|v| v.as_str()).unwrap_or_default();
            if n.is_empty() {
                return Err("var: empty name".into());
            }
            Ok(n.to_string())
        }
        "const" => {
            let v = t.get("value").ok_or("const: missing value")?;
            if let Some(i) = v.as_i64() {
                Ok(i.to_string())
            } else if let Some(u) = v.as_u64() {
                Ok(u.to_string())
            } else if let Some(b) = v.as_bool() {
                Ok(if b { "true".into() } else { "false".into() })
            } else if let Some(s) = v.as_str() {
                Ok(format!("\"{s}\""))
            } else if let Some(f) = v.as_f64() {
                if f == (f as i64 as f64) {
                    Ok((f as i64).to_string())
                } else {
                    Ok(f.to_string())
                }
            } else {
                Err("const: unsupported value type".into())
            }
        }
        "ctor" => {
            let name = t.get("name").and_then(|v| v.as_str()).unwrap_or_default();
            match t.get("args").and_then(|v| v.as_array()) {
                None => Ok(name.to_string()),
                Some(args) if args.is_empty() => Ok(name.to_string()),
                Some(args) => {
                    let mut s = String::from("(");
                    s.push_str(name);
                    for a in args {
                        s.push(' ');
                        s.push_str(&emit_term(a)?);
                    }
                    s.push(')');
                    Ok(s)
                }
            }
        }
        other => Err(format!("emit_term: unknown kind '{other}'")),
    }
}

fn smt_atomic_name(n: &str) -> &str {
    match n {
        "\u{2260}" => "distinct", // ≠
        "\u{2264}" => "<=",       // ≤
        "\u{2265}" => ">=",       // ≥
        other => other,
    }
}

fn smt_sort(s: &Json) -> String {
    if !s.is_object() {
        return "Int".into();
    }
    let n = s.get("name").and_then(|v| v.as_str()).unwrap_or_default();
    match n {
        "Bool" | "Real" | "String" | "Int" => n.to_string(),
        "" => "Int".into(),
        other => other.to_string(),
    }
}

fn emit_formula(f: &Json) -> Result<String, String> {
    if !f.is_object() {
        return Err("non-object IR formula".into());
    }
    let kind = f.get("kind").and_then(|v| v.as_str()).unwrap_or_default();
    match kind {
        "atomic" => {
            let nm = f.get("name").and_then(|v| v.as_str()).unwrap_or_default();
            let smt_n = smt_atomic_name(nm);
            let args = f
                .get("args")
                .and_then(|v| v.as_array())
                .ok_or("atomic: no args")?;
            let mut s = String::from("(");
            s.push_str(smt_n);
            for a in args {
                s.push(' ');
                s.push_str(&emit_term(a)?);
            }
            s.push(')');
            Ok(s)
        }
        "and" | "or" | "not" | "implies" => {
            let ops = f
                .get("operands")
                .and_then(|v| v.as_array())
                .ok_or_else(|| format!("{kind}: missing operands"))?;
            let smt_op = if kind == "implies" { "=>" } else { kind };
            let mut s = String::from("(");
            s.push_str(smt_op);
            for op in ops {
                s.push(' ');
                s.push_str(&emit_formula(op)?);
            }
            s.push(')');
            Ok(s)
        }
        "forall" | "exists" => {
            let vn = f.get("name").and_then(|v| v.as_str()).unwrap_or_default();
            let srt = f
                .get("sort")
                .map(smt_sort)
                .unwrap_or_else(|| "Int".into());
            let body = f
                .get("body")
                .ok_or_else(|| format!("{kind}: missing body"))?;
            let body_s = emit_formula(body)?;
            Ok(format!("({kind} (({vn} {srt})) {body_s})"))
        }
        other => Err(format!("emit_formula: unknown kind '{other}'")),
    }
}

fn collect_free_vars(
    f: &Json,
    out: &mut BTreeMap<String, String>,
    bound: &BTreeSet<String>,
) {
    if !f.is_object() {
        return;
    }
    let kind = f.get("kind").and_then(|v| v.as_str()).unwrap_or_default();
    match kind {
        "atomic" => {
            if let Some(args) = f.get("args").and_then(|v| v.as_array()) {
                for a in args {
                    collect_free_vars_term(a, out, bound);
                }
            }
        }
        "and" | "or" | "not" | "implies" => {
            if let Some(ops) = f.get("operands").and_then(|v| v.as_array()) {
                for op in ops {
                    collect_free_vars(op, out, bound);
                }
            }
        }
        "forall" | "exists" => {
            let mut nb = bound.clone();
            if let Some(n) = f.get("name").and_then(|v| v.as_str()) {
                nb.insert(n.to_string());
            }
            if let Some(b) = f.get("body") {
                collect_free_vars(b, out, &nb);
            }
        }
        _ => {}
    }
}

fn collect_free_vars_term(
    t: &Json,
    out: &mut BTreeMap<String, String>,
    bound: &BTreeSet<String>,
) {
    if !t.is_object() {
        return;
    }
    let kind = t.get("kind").and_then(|v| v.as_str()).unwrap_or_default();
    if kind == "var" {
        if let Some(n) = t.get("name").and_then(|v| v.as_str()) {
            if !bound.contains(n) {
                // VarTerm carries no sort under the new IR; default to
                // Int (mirrors the C++ peer's choice for v1).
                out.insert(n.to_string(), "Int".into());
            }
        }
    } else if kind == "ctor" {
        if let Some(args) = t.get("args").and_then(|v| v.as_array()) {
            for a in args {
                collect_free_vars_term(a, out, bound);
            }
        }
    }
}
