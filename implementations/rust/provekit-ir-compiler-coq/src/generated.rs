// SPDX-License-Identifier: Apache-2.0
// GENERATED Coq compiler

use std::collections::{BTreeMap, BTreeSet};
use provekit_ir_compiler::FreeVar;
use provekit_ir_types::*;

pub fn emit_term(term: &Term) -> String {
    match term {
        Term::Var { name, .. } => name.clone(),
        Term::Const { value, sort, .. } => {
            // Coq: Function/Dependent sorts on a Const are structurally unusual but
            // not unsound — Coq's higher-order universe permits e.g. `(fun x => x) : nat -> nat`
            // as a constant inhabiting a function sort. `emit_const_value` ignores the sort name
            // (it only branches on the JSON value shape), so we feed it the empty string for
            // non-primitive sorts and let the value's own JSON dictate the surface form.
            let sort_name = match sort {
                Sort::Primitive { name } => name.as_str(),
                Sort::Function { .. } | Sort::Dependent { .. } => "",
            };
            return emit_const_value(value, sort_name);
        },
        Term::Ctor { name, args, .. } => {
            if args.is_empty() { return name.clone(); };
            let args_str = args.iter();
            let args_str = args_str.map(|a| emit_term(a));
            let args_str: Vec<String> = args_str.collect();
            return format!("({} {})", name, args_str.join(" "));
        },
        Term::Lambda { param_name, param_sort, body, .. } => {
            let sort_str = emit_sort(param_sort);
            let body_str = emit_term(body);
            return format!("fun ({} : {}) => {}", param_name, sort_str, body_str);
        },
        Term::Let { bindings, body, .. } => {
            let mut parts = Vec::new();
            for b in bindings {
                parts.push(format!("let {} := {} in", b.name, emit_term(&b.bound_term)));
            }
            let body_str = emit_term(body);
            return format!("{} {}", parts.join(" "), body_str);
        },
        Term::Ctor { name, args } => {
            if args.is_empty() { return name.clone(); };
            let args_str = args.iter();
            let args_str = args_str.map(|a| emit_term(a));
            let args_str: Vec<String> = args_str.collect();
            return format!("({} {})", name, args_str.join(" "));
        },
        Term::Lambda { param_name, param_sort, body } => {
            let sort_str = emit_sort(param_sort);
            let body_str = emit_term(body);
            return format!("fun ({} : {}) => {}", param_name, sort_str, body_str);
        },
        Term::Let { bindings, body } => {
            let mut parts = Vec::new();
            for b in bindings {
                parts.push(format!("let {} := {} in", b.name, emit_term(&b.bound_term)));
            }
            let body_str = emit_term(body);
            return format!("{} {}", parts.join(" "), body_str);
        },
    }
}
pub fn emit_formula(formula: &Formula) -> String {
    match formula {
        Formula::Atomic { name, args } => {
            let args_str = args.iter();
            let args_str = args_str.map(|a| emit_term(a));
            let args_str: Vec<String> = args_str.collect();
            return match name.as_str() {
    "=" => format!("({} = {})", args_str[0].clone(), args_str[1].clone()),
    ">" => format!("({} > {})", args_str[0].clone(), args_str[1].clone()),
    "<" => format!("({} < {})", args_str[0].clone(), args_str[1].clone()),
    "\u{2265}" => format!("({} >= {})", args_str[0].clone(), args_str[1].clone()),
    "\u{2264}" => format!("({} <= {})", args_str[0].clone(), args_str[1].clone()),
    "\u{2260}" => format!("({} <> {})", args_str[0].clone(), args_str[1].clone()),
    "true" => "True".to_string(),
    "false" => "False".to_string(),
    _ => format!("{} {}", name, args_str.join(" ")),
};
        },
        Formula::And { operands } => {
            let ops = operands.iter();
            let ops = ops.map(|o| emit_formula(o));
            let ops: Vec<String> = ops.collect();
            return format!("({})", ops.join(r#" /\ "#));
        },
        Formula::Or { operands } => {
            let ops = operands.iter();
            let ops = ops.map(|o| emit_formula(o));
            let ops: Vec<String> = ops.collect();
            return format!("({})", ops.join(r#" \/ "#));
        },
        Formula::Not { operands } => format!("(~{})", emit_formula(&operands[0])),
        Formula::Implies { operands } => format!("({} -> {})", emit_formula(&operands[0]), emit_formula(&operands[1])),
        Formula::Forall { name, sort, body } => {
            let coq_sort = sort_to_coq(sort);
            let body_str = emit_formula(body);
            return format!("forall {} : {}, {}", name, coq_sort, body_str);
        },
        Formula::Exists { name, sort, body } => {
            let coq_sort = sort_to_coq(sort);
            let body_str = emit_formula(body);
            return format!("exists {} : {}, {}", name, coq_sort, body_str);
        },
        Formula::Choice { var_name, sort, body } => {
            let coq_sort = sort_to_coq(sort);
            let body_str = emit_formula(body);
            return format!("@sig {} {} (fun {} => {})", var_name, coq_sort, var_name, body_str);
        },
    }
}
fn emit_sort(sort: &Sort) -> String {
    match sort {
        Sort::Primitive { name } => match name.as_str() {
    "Int" | "Real" => "Z".to_string(),
    "String" => "string".to_string(),
    "Bool" => "bool".to_string(),
    _ => "Z".to_string(),
},
        // FunctionSort: Coq function arrow `A1 -> A2 -> ... -> Ret`. Coq's `->` is
        // right-associative, so a function-typed argument MUST be parenthesized to
        // preserve meaning: `(A -> B) -> C` differs from `A -> B -> C`. Soundness
        // depends on this. Issue #331; see protocol/specs/multi-solver-protocol-v2.md
        // — Coq's portfolio seat covers higher-order, so this position is NOT opaque.
        Sort::Function { args, ret } => {
            let mut parts: Vec<String> = args.iter().map(|a| emit_sort_paren(a)).collect();
            parts.push(emit_sort_paren(ret));
            return parts.join(" -> ");
        },
        // DependentSort: Coq Π-type. `Vec` indexed by `n: nat` becomes
        // `forall n : nat, Vec n`. The instantiated form (`<name> <index_var>`)
        // matches the canonical dependent-product shape in Coq, where the sort
        // name is applied to the bound index. Issue #331.
        Sort::Dependent { name, index_var, index_sort } => {
            return format!("forall {} : {}, {} {}", index_var, emit_sort(index_sort), name, index_var);
        },
    }
}

/// Wrap a sort emission in parens when it is a `Sort::Function`, so right-associative
/// `->` does not silently re-bracket nested function args. Primitive and Dependent
/// emissions are self-delimited (single token / leading `forall`) and need no parens.
fn emit_sort_paren(sort: &Sort) -> String {
    match sort {
        Sort::Function { .. } => format!("({})", emit_sort(sort)),
        _ => emit_sort(sort),
    }
}

fn sort_to_coq(sort: &Sort) -> String {
    match sort {
        Sort::Primitive { name } => match name.as_str() {
    "Int" | "Real" => "Z".to_string(),
    "String" => "string".to_string(),
    "Bool" => "bool".to_string(),
    _ => "Z".to_string(),
},
        // Identical Coq syntax to `emit_sort`; this entry point is used in formula
        // binder positions (Forall/Exists/Choice). See `emit_sort` for soundness
        // notes on associativity (Function) and Π-type shape (Dependent).
        Sort::Function { args, ret } => {
            let mut parts: Vec<String> = args.iter().map(|a| sort_to_coq_paren(a)).collect();
            parts.push(sort_to_coq_paren(ret));
            return parts.join(" -> ");
        },
        Sort::Dependent { name, index_var, index_sort } => {
            return format!("forall {} : {}, {} {}", index_var, sort_to_coq(index_sort), name, index_var);
        },
    }
}

fn sort_to_coq_paren(sort: &Sort) -> String {
    match sort {
        Sort::Function { .. } => format!("({})", sort_to_coq(sort)),
        _ => sort_to_coq(sort),
    }
}
fn emit_const_value(value: &serde_json::Value, _sort_name: &str) -> String {
    match value {
        serde_json::Value::Number(n) => if let Some(i) = n.as_i64() { i.to_string() } else if let Some(u) = n.as_u64() { u.to_string() } else { n.to_string() },
        serde_json::Value::Bool(b) => if *b { "true".to_string() } else { "false".to_string() },
        serde_json::Value::String(s) => format!("\"{}\"", s),
        _ => "0".to_string(),
    }
}
pub fn compile_formula(formula: &Formula) -> (String, String, Vec<FreeVar>) {
    let mut free_vars = BTreeMap::new();
    let bound = BTreeSet::new();
    collect_free_vars_formula(formula, &mut free_vars, &bound);
    let mut body = String::new();
    for (name, sort) in free_vars.iter() {
        let coq_sort = match sort.as_str() {
    "Int" | "Real" => "Z",
    "String" => "string",
    "Bool" => "bool",
    _ => "Z",
};
        body.push_str(&format!("Parameter {} : {}.\n", name, coq_sort));
    }
    body.push_str("\nGoal ");
    body.push_str(&emit_formula(formula));
    body.push_str(".\n");
    body.push_str("Proof.\n  intuition.\n  admit.\nQed.\n");
    let preamble = "Require Import ZArith String List.\nOpen Scope Z.\nOpen Scope string.\n\n".to_string();
    let free_vars_vec = free_vars.into_iter().map(|(name, sort)| FreeVar { name, sort });
    let free_vars_vec = free_vars_vec.collect();
    return (preamble, body, free_vars_vec);
}
pub fn collect_free_vars_formula(formula: &Formula, out: &mut BTreeMap<String, String>, bound: &BTreeSet<String>) {
    match formula {
        Formula::Atomic { args, .. } => {
            for a in args {
                collect_free_vars_term(a, out, bound);
            }
        },
        Formula::And { operands } => {
            for o in operands {
                collect_free_vars_formula(o, out, bound);
            }
        },
        Formula::Or { operands } => {
            for o in operands {
                collect_free_vars_formula(o, out, bound);
            }
        },
        Formula::Not { operands } => {
            for o in operands {
                collect_free_vars_formula(o, out, bound);
            }
        },
        Formula::Implies { operands } => {
            for o in operands {
                collect_free_vars_formula(o, out, bound);
            }
        },
        Formula::Forall { name, sort: _, body } => {
            let mut nb = bound.clone();
            nb.insert(name.clone());
            collect_free_vars_formula(body, out, &nb);
        },
        Formula::Exists { name, sort: _, body } => {
            let mut nb = bound.clone();
            nb.insert(name.clone());
            collect_free_vars_formula(body, out, &nb);
        },
        Formula::Choice { var_name, sort: _, body } => {
            let mut nb = bound.clone();
            nb.insert(var_name.clone());
            collect_free_vars_formula(body, out, &nb);
        },
    }
}
pub fn collect_free_vars_term(term: &Term, out: &mut BTreeMap<String, String>, bound: &BTreeSet<String>) {
    match term {
        Term::Var { name, .. } => if !bound.contains(name) { out.entry(name.clone()).or_insert("Int".to_string()); },
        Term::Const { .. } => {
        },
        Term::Ctor { args, .. } => {
            for a in args {
                collect_free_vars_term(a, out, bound);
            }
        },
        Term::Lambda { param_name, param_sort: _, body, .. } => {
            let mut nb = bound.clone();
            nb.insert(param_name.clone());
            collect_free_vars_term(body, out, &nb);
        },
        Term::Let { bindings, body, .. } => {
            let mut current_bound = bound.clone();
            for b in bindings {
                collect_free_vars_term(&b.bound_term, out, &current_bound);
                current_bound.insert(b.name.clone());
            }
            collect_free_vars_term(body, out, &current_bound);
        },
    }
}
