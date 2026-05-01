// SPDX-License-Identifier: Apache-2.0
//
// Bundled SMT-LIB v2.6 IR compiler. Extracted from the inline
// provekit-verifier::smt_emitter so the same code serves both the
// in-process fast path (verifier deps directly on this crate) and the
// standalone subprocess binary `provekit-ir-smt-lib`.
//
// Spec: protocol/specs/2026-04-30-ir-compiler-protocol.md.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::Value as Json;

use provekit_ir_compiler::{
    Capabilities, CompileError, CompiledFormula, FreeVar, IrCompiler, PROTOCOL_VERSION,
};

pub const DIALECT: &str = "smt-lib-v2.6";
pub const COMPILER_NAME: &str = "smt-lib-reference";
pub const COMPILER_VERSION: &str = env!("CARGO_PKG_VERSION");

/// SMT-LIB v2.6 compiler. Stateless; one instance suffices for any
/// number of compile calls.
pub struct SmtLibCompiler;

impl SmtLibCompiler {
    pub fn new() -> Self {
        Self
    }
}

impl Default for SmtLibCompiler {
    fn default() -> Self {
        Self::new()
    }
}

impl IrCompiler for SmtLibCompiler {
    fn compile(&self, ir: &Json, dialect: &str) -> Result<CompiledFormula, CompileError> {
        if dialect != DIALECT {
            return Err(CompileError::UnsupportedDialect(dialect.to_string()));
        }
        compile_to_parts(ir)
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities {
            name: COMPILER_NAME.to_string(),
            version: COMPILER_VERSION.to_string(),
            protocol_version: PROTOCOL_VERSION.to_string(),
            dialects: vec![DIALECT.to_string()],
            supported_sorts: vec![
                "Int".to_string(),
                "Bool".to_string(),
                "Real".to_string(),
                "String".to_string(),
            ],
            supported_predicates: vec![
                "=".to_string(),
                "distinct".to_string(),
                "<".to_string(),
                "<=".to_string(),
                ">".to_string(),
                ">=".to_string(),
                "and".to_string(),
                "or".to_string(),
                "not".to_string(),
                "implies".to_string(),
                "forall".to_string(),
                "exists".to_string(),
                "\u{2260}".to_string(), // ≠
                "\u{2264}".to_string(), // ≤
                "\u{2265}".to_string(), // ≥
            ],
        }
    }
}

/// Legacy single-string entry point. Equal to `preamble + body` from
/// `compile_to_parts`. Kept so the verifier crate can re-export it
/// under the historical `provekit_verifier::smt_emitter::emit` path
/// without changing the runner.
pub fn emit(ir_formula: &Json) -> Result<String, String> {
    compile_to_parts(ir_formula)
        .map(|c| {
            let mut s = c.preamble;
            s.push_str(&c.body);
            s
        })
        .map_err(|e| e.to_string())
}

/// Compile to (preamble, body, free_vars). Pure; no I/O.
pub fn compile_to_parts(ir_formula: &Json) -> Result<CompiledFormula, CompileError> {
    let body_expr = emit_formula(ir_formula).map_err(CompileError::MalformedIr)?;

    let mut free_vars: BTreeMap<String, String> = BTreeMap::new();
    let bound: BTreeSet<String> = BTreeSet::new();
    collect_free_vars(ir_formula, &mut free_vars, &bound, None);

    // Collect undeclared atomic predicates (kit-defined like roundTrips, len, etc.)
    let (undeclared_preds, var_sorts_from_preds) = collect_undeclared_atomics(ir_formula);
    // Merge: prefer explicit var sorts from collect_free_vars, fill gaps from predicate usage
for (var_name, sort) in &var_sorts_from_preds {
        free_vars.entry(var_name.clone()).or_insert(sort.clone());
    }
    eprintln!("DEBUG ir_formula sample: {}", serde_json::to_string(ir_formula).unwrap_or_default().chars().take(500).collect::<String>());
    eprintln!("DEBUG var_sorts_from_preds = {:?}", var_sorts_from_preds);
    eprintln!("DEBUG full_preamble:\n{}", { 
        let mut p = String::new();
        p.push_str("(set-logic ALL)\n");
        for (name, srt) in &free_vars {
            p.push_str(&format!("(declare-const {name} {srt})\n"));
        }
        for (name, arg_sorts, ret_sort) in &undeclared_preds {
            let args: String = arg_sorts
                .iter()
                .map(|s| format!("({s})"))
                .collect::<Vec<_>>()
                .join(" ");
            p.push_str(&format!("(declare-fun {name} ({args}) {ret_sort})\n"));
        }
        p
    });

    let mut preamble = String::new();
    preamble.push_str("(set-logic ALL)\n");
    for (name, srt) in &free_vars {
        preamble.push_str(&format!("(declare-const {name} {srt})\n"));
    }
    // Declare kit-defined predicates as uninterpreted functions
    for (name, arg_sorts, ret_sort) in undeclared_preds.into_iter() {
        let args: String = arg_sorts
            .iter()
            .map(|s| format!("({s})"))
            .collect::<Vec<_>>()
            .join(" ");
        preamble.push_str(&format!("(declare-fun {name} ({args}) {ret_sort})\n"));
    }

    let mut body = String::new();
    body.push_str(&format!("(assert (not {body_expr}))\n"));
    body.push_str("(check-sat)\n");

    let free_vars_vec = free_vars
        .into_iter()
        .map(|(name, sort)| FreeVar { name, sort })
        .collect();

    Ok(CompiledFormula {
        preamble,
        body,
        free_vars: free_vars_vec,
    })
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
    ctx_quant_sort: Option<&str>,
) {
    if !f.is_object() {
        return;
    }
    let kind = f.get("kind").and_then(|v| v.as_str()).unwrap_or_default();
    match kind {
        "atomic" => {
            if let Some(args) = f.get("args").and_then(|v| v.as_array()) {
                for a in args {
                    collect_free_vars_term(a, out, bound, ctx_quant_sort);
                }
            }
        }
        "and" | "or" | "not" | "implies" => {
            if let Some(ops) = f.get("operands").and_then(|v| v.as_array()) {
                for op in ops {
                    collect_free_vars(op, out, bound, ctx_quant_sort);
                }
            }
        }
        "forall" | "exists" => {
            // Quantifier's sort is authoritative for bound vars, but we DON'T
            // add them to free_vars - they're bound by the quantifier itself.
            let quant_sort = f
                .get("sort")
                .and_then(|s| s.get("name"))
                .and_then(|n| n.as_str());
            if let Some(n) = f.get("name").and_then(|v| v.as_str()) {
                let mut nb = bound.clone();
                nb.insert(n.to_string());
                if let Some(b) = f.get("body") {
                    collect_free_vars(b, out, &nb, quant_sort);
                }
            }
        }
        _ => {}
    }
}

fn collect_free_vars_term(
    t: &Json,
    out: &mut BTreeMap<String, String>,
    bound: &BTreeSet<String>,
    ctx_quant_sort: Option<&str>,
) {
    if !t.is_object() {
        return;
    }
    let kind = t.get("kind").and_then(|v| v.as_str()).unwrap_or_default();
    if kind == "var" {
        if let Some(n) = t.get("name").and_then(|v| v.as_str()) {
            if !bound.contains(n) {
                // Try to get sort from the term itself - IR may have sort field
                let sort = t
                    .get("sort")
                    .and_then(|s| s.get("name"))
                    .and_then(|n| n.as_str())
                    .or(ctx_quant_sort)  // Fall back to enclosing quantifier's sort
                    .unwrap_or("Int");
                out.insert(n.to_string(), sort.to_string());
            }
        }
    } else if kind == "ctor" {
        if let Some(args) = t.get("args").and_then(|v| v.as_array()) {
            for a in args {
                collect_free_vars_term(a, out, bound, ctx_quant_sort);
            }
        }
    }
}

/// Standard SMT predicates that don't need declaration.
fn is_standard_predicate(name: &str) -> bool {
    matches!(
        name,
        "=" | "distinct" | "<" | "<=" | ">" | ">="
            | "and" | "or" | "not" | "implies"
            | "forall" | "exists"
            | "\u{2260}" // ≠
            | "\u{2264}" // ≤
            | "\u{2265}" // ≥
    )
}

/// Collect atomic predicates that need to be declared as uninterpreted functions.
/// Also collects var sorts from atomic predicate argument usage.
fn collect_undeclared_atomics(formula: &Json) -> (Vec<(String, Vec<String>, String)>, BTreeMap<String, String>) {
    let mut preds: BTreeSet<(String, Vec<String>, String)> = BTreeSet::new();
    let mut var_sorts: BTreeMap<String, String> = BTreeMap::new();
    collect_atomics_with_context(formula, &mut preds, None);
    // Merge: prefer explicit var sorts, fill gaps from predicate usage
    let pred_sorts: BTreeMap<String, String> = preds.iter()
        .filter_map(|(name, args, _)| {
            if args.len() == 1 {
                Some((name.clone(), args[0].clone()))
            } else {
                None
            }
        })
        .collect();
    for (pred_name, pred_sort) in pred_sorts {
        var_sorts.entry(pred_name).or_insert(pred_sort);
    }
    (preds.into_iter().collect(), var_sorts)
}

fn collect_atomics_with_context(
    f: &Json,
    out: &mut BTreeSet<(String, Vec<String>, String)>,
    ctx_sort: Option<&str>,
) {
    if !f.is_object() {
        return;
    }
    let kind = f.get("kind").and_then(|v| v.as_str()).unwrap_or_default();
    match kind {
        "atomic" => {
            let name = f.get("name").and_then(|v| v.as_str()).unwrap_or_default();
            if !is_standard_predicate(name) {
                // Determine arg sorts: try each arg's sort field, or use context sort
                let arg_sorts: Vec<String> = f
                    .get("args")
                    .and_then(|v| v.as_array())
                    .map(|args| {
                        args.iter()
                            .filter_map(|a| {
                                // Try to get sort from the term itself
                                a.get("sort")
                                    .and_then(|s| s.get("name"))
                                    .and_then(|n| n.as_str())
                                    .map(|s| s.to_string())
                            })
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                // If no args had explicit sort, use context sort for each bound var
                let final_sorts = if arg_sorts.is_empty() {
                    if let Some(s) = ctx_sort {
                        vec![s.to_string()]
                    } else {
                        vec!["String".to_string()]
                    }
                } else {
                    arg_sorts
                };
                out.insert((name.to_string(), final_sorts, "Bool".to_string()));
            }
        }
        "and" | "or" | "not" | "implies" => {
            if let Some(ops) = f.get("operands").and_then(|v| v.as_array()) {
                for op in ops {
                    collect_atomics_with_context(op, out, ctx_sort);
                }
            }
        }
        "forall" | "exists" => {
            let quant_sort = f
                .get("sort")
                .and_then(|s| s.get("name"))
                .and_then(|n| n.as_str());
            if let Some(b) = f.get("body") {
                collect_atomics_with_context(b, out, quant_sort);
            }
        }
        _ => {}
    }
}
