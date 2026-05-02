// SPDX-License-Identifier: Apache-2.0
//
// Byte-for-byte regression check. The trait surface returns a
// CompiledFormula with separate preamble + body + free_vars. The
// historical inline emitter returned one String. The contract says
// `preamble + body == legacy emit() output`, byte-identical.
//
// This test re-implements the historical emitter inline and compares
// every byte against the new path. Drift in the extraction would
// surface here even if the broader contains-style assertions in
// tests/emitter.rs continue to pass.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::{json, Value as Json};

use provekit_ir_compiler::IrCompiler;
use provekit_ir_compiler_smt_lib::{compile_to_parts, emit, SmtLibCompiler, DIALECT};

// -------------------- legacy inline emitter, frozen --------------------

fn legacy_emit(ir_formula: &Json) -> Result<String, String> {
    let body = legacy_emit_formula(ir_formula)?;
    let mut free_vars: BTreeMap<String, String> = BTreeMap::new();
    let bound: BTreeSet<String> = BTreeSet::new();
    legacy_collect_free_vars(ir_formula, &mut free_vars, &bound);
    let mut out = String::new();
    out.push_str("(set-logic ALL)\n");
    for (name, srt) in &free_vars {
        out.push_str(&format!("(declare-const {name} {srt})\n"));
    }
    out.push_str(&format!("(assert (not {body}))\n"));
    out.push_str("(check-sat)\n");
    Ok(out)
}

fn legacy_emit_term(t: &Json) -> Result<String, String> {
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
                        s.push_str(&legacy_emit_term(a)?);
                    }
                    s.push(')');
                    Ok(s)
                }
            }
        }
        other => Err(format!("emit_term: unknown kind '{other}'")),
    }
}

fn legacy_smt_atomic_name(n: &str) -> &str {
    match n {
        "\u{2260}" => "distinct",
        "\u{2264}" => "<=",
        "\u{2265}" => ">=",
        other => other,
    }
}

fn legacy_smt_sort(s: &Json) -> String {
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

fn legacy_emit_formula(f: &Json) -> Result<String, String> {
    if !f.is_object() {
        return Err("non-object IR formula".into());
    }
    let kind = f.get("kind").and_then(|v| v.as_str()).unwrap_or_default();
    match kind {
        "atomic" => {
            let nm = f.get("name").and_then(|v| v.as_str()).unwrap_or_default();
            let smt_n = legacy_smt_atomic_name(nm);
            let args = f
                .get("args")
                .and_then(|v| v.as_array())
                .ok_or("atomic: no args")?;
            let mut s = String::from("(");
            s.push_str(smt_n);
            for a in args {
                s.push(' ');
                s.push_str(&legacy_emit_term(a)?);
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
                s.push_str(&legacy_emit_formula(op)?);
            }
            s.push(')');
            Ok(s)
        }
        "forall" | "exists" => {
            let vn = f.get("name").and_then(|v| v.as_str()).unwrap_or_default();
            let srt = f
                .get("sort")
                .map(legacy_smt_sort)
                .unwrap_or_else(|| "Int".into());
            let body = f
                .get("body")
                .ok_or_else(|| format!("{kind}: missing body"))?;
            let body_s = legacy_emit_formula(body)?;
            Ok(format!("({kind} (({vn} {srt})) {body_s})"))
        }
        other => Err(format!("emit_formula: unknown kind '{other}'")),
    }
}

fn legacy_collect_free_vars(
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
                    legacy_collect_free_vars_term(a, out, bound);
                }
            }
        }
        "and" | "or" | "not" | "implies" => {
            if let Some(ops) = f.get("operands").and_then(|v| v.as_array()) {
                for op in ops {
                    legacy_collect_free_vars(op, out, bound);
                }
            }
        }
        "forall" | "exists" => {
            let mut nb = bound.clone();
            if let Some(n) = f.get("name").and_then(|v| v.as_str()) {
                nb.insert(n.to_string());
            }
            if let Some(b) = f.get("body") {
                legacy_collect_free_vars(b, out, &nb);
            }
        }
        _ => {}
    }
}

fn legacy_collect_free_vars_term(
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
                out.insert(n.to_string(), "Int".into());
            }
        }
    } else if kind == "ctor" {
        if let Some(args) = t.get("args").and_then(|v| v.as_array()) {
            for a in args {
                legacy_collect_free_vars_term(a, out, bound);
            }
        }
    }
}

// -------------------- the actual regression check --------------------

fn fixtures() -> Vec<Json> {
    vec![
        // simple atomic
        json!({"kind": "atomic", "name": ">", "args": [
            {"kind": "var", "name": "x"},
            {"kind": "const", "value": 0,
             "sort": {"kind": "primitive", "name": "Int"}}
        ]}),
        // unicode atom mapped to <=, distinct, >=
        json!({"kind": "atomic", "name": "\u{2260}", "args": [
            {"kind": "var", "name": "alpha"},
            {"kind": "var", "name": "beta"}
        ]}),
        json!({"kind": "atomic", "name": "\u{2264}", "args": [
            {"kind": "var", "name": "p"},
            {"kind": "var", "name": "q"}
        ]}),
        json!({"kind": "atomic", "name": "\u{2265}", "args": [
            {"kind": "var", "name": "p"},
            {"kind": "var", "name": "q"}
        ]}),
        // and / or / not / implies
        json!({"kind": "and", "operands": [
            {"kind": "atomic", "name": ">", "args": [
                {"kind": "var", "name": "x"},
                {"kind": "const", "value": 0,
                 "sort": {"kind": "primitive", "name": "Int"}}
            ]},
            {"kind": "atomic", "name": "<", "args": [
                {"kind": "var", "name": "x"},
                {"kind": "const", "value": 10,
                 "sort": {"kind": "primitive", "name": "Int"}}
            ]}
        ]}),
        json!({"kind": "implies", "operands": [
            {"kind": "atomic", "name": ">", "args": [
                {"kind": "var", "name": "x"},
                {"kind": "const", "value": 0,
                 "sort": {"kind": "primitive", "name": "Int"}}
            ]},
            {"kind": "atomic", "name": ">", "args": [
                {"kind": "var", "name": "x"},
                {"kind": "const", "value": -1,
                 "sort": {"kind": "primitive", "name": "Int"}}
            ]}
        ]}),
        // forall over Int with bound var
        json!({"kind": "forall", "name": "n",
            "sort": {"kind": "primitive", "name": "Int"},
            "body": {"kind": "atomic", "name": ">", "args": [
                {"kind": "var", "name": "n"},
                {"kind": "const", "value": 0,
                 "sort": {"kind": "primitive", "name": "Int"}}
            ]}}),
        // forall Real
        json!({"kind": "forall", "name": "x",
            "sort": {"kind": "primitive", "name": "Real"},
            "body": {"kind": "atomic", "name": ">", "args": [
                {"kind": "var", "name": "x"},
                {"kind": "const", "value": 0,
                 "sort": {"kind": "primitive", "name": "Real"}}
            ]}}),
        // ctor in atomic
        json!({"kind": "atomic", "name": "=", "args": [
            {"kind": "ctor", "name": "sumDebits",
                "args": [{"kind": "var", "name": "txn"}]},
            {"kind": "ctor", "name": "sumCredits",
                "args": [{"kind": "var", "name": "txn"}]}
        ]}),
        // boolean const + string const
        json!({"kind": "atomic", "name": "=", "args": [
            {"kind": "var", "name": "flag"},
            {"kind": "const", "value": true, "sort": {"kind": "primitive", "name": "Int"}}
        ]}),
        // multiple free vars exercising sort order in declare-const block
        json!({"kind": "and", "operands": [
            {"kind": "atomic", "name": ">", "args": [
                {"kind": "var", "name": "z"},
                {"kind": "const", "value": 0,
                 "sort": {"kind": "primitive", "name": "Int"}}
            ]},
            {"kind": "atomic", "name": ">", "args": [
                {"kind": "var", "name": "a"},
                {"kind": "const", "value": 0,
                 "sort": {"kind": "primitive", "name": "Int"}}
            ]}
        ]}),
    ]
}

#[test]
fn extracted_emitter_matches_legacy_byte_for_byte() {
    for (i, ir) in fixtures().iter().enumerate() {
        let new = emit(ir).expect("new emit");
        let old = legacy_emit(ir).expect("legacy emit");
        assert_eq!(
            new, old,
            "fixture #{i} drifted between extracted and legacy emitter\n--- new ---\n{new}\n--- old ---\n{old}"
        );
    }
}

#[test]
fn trait_compile_preamble_plus_body_equals_emit_string() {
    for ir in fixtures() {
        let parts = compile_to_parts(&ir).expect("compile_to_parts");
        let combined = format!("{}{}", parts.preamble, parts.body);
        let single = emit(&ir).expect("emit");
        assert_eq!(combined, single);
    }
}

#[test]
fn trait_dispatch_through_smtlib_impl_matches_emit() {
    let c = SmtLibCompiler::new();
    for ir in fixtures() {
        let parts = c.compile(&ir, DIALECT).expect("compile");
        let combined = format!("{}{}", parts.preamble, parts.body);
        let single = emit(&ir).expect("emit");
        assert_eq!(combined, single);
    }
}

#[test]
fn trait_compile_rejects_wrong_dialect() {
    let c = SmtLibCompiler::new();
    let ir = json!({"kind": "atomic", "name": "=", "args": [
        {"kind": "var", "name": "x"}, {"kind": "var", "name": "x"}
    ]});
    let r = c.compile(&ir, "tptp-fof");
    assert!(matches!(
        r,
        Err(provekit_ir_compiler::CompileError::UnsupportedDialect(_))
    ));
}

#[test]
fn capabilities_lists_all_documented_predicates() {
    let c = SmtLibCompiler::new();
    let caps = c.capabilities();
    for needed in [
        "=", "distinct", "<", "<=", ">", ">=", "and", "or", "not", "implies",
        "forall", "exists",
    ] {
        assert!(
            caps.supported_predicates.iter().any(|p| p == needed),
            "missing predicate {needed} in capabilities"
        );
    }
    for needed in ["Int", "Bool", "Real", "String"] {
        assert!(
            caps.supported_sorts.iter().any(|s| s == needed),
            "missing sort {needed} in capabilities"
        );
    }
}

#[test]
fn compiled_formula_free_vars_match_preamble_declares() {
    let ir = json!({"kind": "and", "operands": [
        {"kind": "atomic", "name": ">", "args": [
            {"kind": "var", "name": "z"},
            {"kind": "const", "value": 0,
             "sort": {"kind": "primitive", "name": "Int"}}
        ]},
        {"kind": "atomic", "name": ">", "args": [
            {"kind": "var", "name": "a"},
            {"kind": "const", "value": 0,
             "sort": {"kind": "primitive", "name": "Int"}}
        ]}
    ]});
    let parts = compile_to_parts(&ir).expect("compile");
    assert_eq!(parts.free_vars.len(), 2);
    let names: Vec<&str> = parts.free_vars.iter().map(|v| v.name.as_str()).collect();
    assert_eq!(names, vec!["a", "z"], "free vars must be sorted by name");
    for v in &parts.free_vars {
        let needle = format!("(declare-const {} {})", v.name, v.sort);
        assert!(
            parts.preamble.contains(&needle),
            "preamble missing declare for {needle}"
        );
    }
}
