// SPDX-License-Identifier: Apache-2.0
// GENERATED SMT-LIB v2.6 compiler

use std::collections::{BTreeMap, BTreeSet};
use provekit_ir_compiler::{CompiledFormula, FreeVar, OpacityEntry, OpacityManifest};
use provekit_ir_types::*;
use std::sync::Arc;
use provekit_canonicalizer::{encode_jcs, blake3_512_of, Value as CValue};
use serde_json;

use crate::{COMPILER_NAME, COMPILER_VERSION, DIALECT};

pub fn emit_term(term: &Term) -> String {
    match term {
        Term::Var { name, .. } => name.clone(),
        Term::Const { value, sort, .. } => {
            let sort_name = match sort {
                Sort::Primitive { name } => name.as_str(),
                Sort::Function { .. } | Sort::Dependent { .. } | Sort::Float { .. } | Sort::Region { .. } => {
                    panic!("smt-lib: Const cannot carry a Function/Dependent/Float/Region sort in pure SMT-LIB v2.6");
                }
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
            return format!("(lambda (({} {})) {})", param_name, sort_str, body_str);
        },
        Term::Let { bindings, body, .. } => {
            let mut binding_strs = bindings.iter();
            let binding_strs = binding_strs.map(|b| format!("({} {})", b.name, emit_term(&b.bound_term)));
            let binding_strs: Vec<String> = binding_strs.collect();
            let body_str = emit_term(body);
            return format!("(let ({}) {})", binding_strs.join(" "), body_str);
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
            return format!("(lambda (({} {})) {})", param_name, sort_str, body_str);
        },
        Term::Let { bindings, body } => {
            let binding_strs = bindings.iter();
            let binding_strs = binding_strs.map(|b| format!("({} {})", b.name, emit_term(&b.bound_term)));
            let binding_strs: Vec<String> = binding_strs.collect();
            let body_str = emit_term(body);
            return format!("(let ({}) {})", binding_strs.join(" "), body_str);
        },
    }
}

/// Emit a sort as SMT-LIB surface syntax. Returns (smt_string, reason_code)
/// where reason_code is Some if the sort was opaque.
fn emit_sort_with_reason(sort: &Sort) -> (String, Option<String>) {
    match sort {
        Sort::Primitive { name } => (name.clone(), None),
        Sort::Function { .. } => {
            ("Int".to_string(), Some("predicate_quantification".to_string()))
        }
        Sort::Dependent { .. } => {
            ("Int".to_string(), Some("dependent_type".to_string()))
        }
        Sort::Float { .. } => {
            ("Int".to_string(), Some("other:FloatSort unsupported in pure SMT-LIB v2.6".to_string()))
        }
        Sort::Region { .. } => {
            ("Int".to_string(), Some("other:RegionSort pre-resolved in composition".to_string()))
        }
    }
}

pub fn emit_sort(sort: &Sort) -> String {
    emit_sort_with_reason(sort).0
}

pub fn emit_formula(formula: &Formula) -> String {
    match formula {
        Formula::Atomic { name, args } => {
            let smt_name = smt_atomic_name(name);
            if args.is_empty() { return smt_name.to_string(); };
            let args_str = args.iter();
            let args_str = args_str.map(|a| emit_term(a));
            let args_str: Vec<String> = args_str.collect();
            return format!("({} {})", smt_name, args_str.join(" "));
        },
        Formula::And { operands } => {
            let ops_str = operands.iter();
            let ops_str = ops_str.map(|o| emit_formula(o));
            let ops_str: Vec<String> = ops_str.collect();
            return format!("({} {})", "and", ops_str.join(" "));
        },
        Formula::Or { operands } => {
            let ops_str = operands.iter();
            let ops_str = ops_str.map(|o| emit_formula(o));
            let ops_str: Vec<String> = ops_str.collect();
            return format!("({} {})", "or", ops_str.join(" "));
        },
        Formula::Not { operands } => format!("(not {})", emit_formula(&operands[0])),
        Formula::Implies { operands } => format!("(=> {} {})", emit_formula(&operands[0]), emit_formula(&operands[1])),
        Formula::Forall { name, sort, body } => {
            let (sort_str, reason) = emit_sort_with_reason(sort);
            let body_str = emit_formula(body);
            if let Some(_r) = reason {
                // Quantifier over opaque sort: assert true as placeholder
                return "(true)".to_string();
            }
            return format!("(forall (({} {})) {})", name, sort_str, body_str);
        },
        Formula::Exists { name, sort, body } => {
            let (sort_str, reason) = emit_sort_with_reason(sort);
            let body_str = emit_formula(body);
            if let Some(_r) = reason {
                return "(true)".to_string();
            }
            return format!("(exists (({} {})) {})", name, sort_str, body_str);
        },
        Formula::Choice { var_name, sort, body } => {
            let (sort_str, reason) = emit_sort_with_reason(sort);
            let body_str = emit_formula(body);
            if let Some(_r) = reason {
                return "(true)".to_string();
            }
            let var_y = format!("{}_y", var_name);
            let body_y = body_str.replace(var_name, &var_y);
            let unique = format!("(and {} (forall (({} {})) (=> {} (= {} {}))))", body_str, var_y, sort_str, body_y, var_y, var_name);
            return format!("(exists (({} {})) {})", var_name, sort_str, unique);
        },
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

fn smt_atomic_name(name: &str) -> &str {
    match name {
        "eq" => "=",
        "neq" => "distinct",
        "gt" => ">",
        "gte" => ">=",
        "lt" => "<",
        "lte" => "<=",
        "\u{2260}" => "distinct",
        "\u{2264}" => "<=",
        "\u{2265}" => ">=",
        other => other,
    }
}

/// Compute the positionCid for an IR subterm.
fn position_cid_of(value: &serde_json::Value) -> String {
    let cv = to_cvalue(value);
    let jcs = encode_jcs(&cv);
    blake3_512_of(jcs.as_bytes())
}

fn to_cvalue(v: &serde_json::Value) -> Arc<CValue> {
    match v {
        serde_json::Value::Null => CValue::null(),
        serde_json::Value::Bool(b) => CValue::boolean(*b),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() { CValue::integer(i) }
            else if let Some(f) = n.as_f64() { CValue::string(format!("{}", f)) }
            else { CValue::null() }
        }
        serde_json::Value::String(s) => CValue::string(s.clone()),
        serde_json::Value::Array(arr) => {
            CValue::array(arr.iter().map(|v| to_cvalue(v)).collect())
        }
        serde_json::Value::Object(obj) => {
            CValue::object(obj.iter().map(|(k, v)| (k.clone(), to_cvalue(v))))
        }
    }
}

/// Walk a formula collecting opacity entries for sorts the SMT-LIB
/// compiler cannot handle. Returns (formula_string, opacities).
fn emit_formula_with_opacities(formula: &Formula, opacities: &mut Vec<OpacityEntry>) -> String {
    match formula {
        Formula::Atomic { args, .. } => {
            for a in args {
                collect_opacities_term(a, opacities);
            }
            emit_formula(formula)
        },
        Formula::And { operands } => {
            let ops: Vec<String> = operands.iter()
                .map(|o| emit_formula_with_opacities(o, opacities))
                .collect();
            format!("({} {})", "and", ops.join(" "))
        },
        Formula::Or { operands } => {
            let ops: Vec<String> = operands.iter()
                .map(|o| emit_formula_with_opacities(o, opacities))
                .collect();
            format!("({} {})", "or", ops.join(" "))
        },
        Formula::Not { operands } => {
            format!("(not {})", emit_formula_with_opacities(&operands[0], opacities))
        },
        Formula::Implies { operands } => {
            format!("(=> {} {})",
                emit_formula_with_opacities(&operands[0], opacities),
                emit_formula_with_opacities(&operands[1], opacities))
        },
        Formula::Forall { name, sort, body } => {
            let (_, reason) = emit_sort_with_reason(sort);
            if let Some(reason_code) = reason {
                let serialized = serde_json::to_value(sort).unwrap_or(serde_json::Value::Null);
                let cid = position_cid_of(&serialized);
                opacities.push(OpacityEntry {
                    position_cid: cid,
                    reason_code,
                });
                "(true)".to_string()
            } else {
                let sort_str = emit_sort(sort);
                let body_str = emit_formula_with_opacities(body, opacities);
                format!("(forall (({} {})) {})", name, sort_str, body_str)
            }
        },
        Formula::Exists { name, sort, body } => {
            let (_, reason) = emit_sort_with_reason(sort);
            if let Some(reason_code) = reason {
                let serialized = serde_json::to_value(sort).unwrap_or(serde_json::Value::Null);
                let cid = position_cid_of(&serialized);
                opacities.push(OpacityEntry {
                    position_cid: cid,
                    reason_code,
                });
                "(true)".to_string()
            } else {
                let sort_str = emit_sort(sort);
                let body_str = emit_formula_with_opacities(body, opacities);
                format!("(exists (({} {})) {})", name, sort_str, body_str)
            }
        },
        Formula::Choice { var_name, sort, body } => {
            let (_, reason) = emit_sort_with_reason(sort);
            if let Some(reason_code) = reason {
                let serialized = serde_json::to_value(sort).unwrap_or(serde_json::Value::Null);
                let cid = position_cid_of(&serialized);
                opacities.push(OpacityEntry {
                    position_cid: cid,
                    reason_code,
                });
                "(true)".to_string()
            } else {
                let sort_str = emit_sort(sort);
                let body_str = emit_formula_with_opacities(body, opacities);
                let var_y = format!("{}_y", var_name);
                let body_y = body_str.replace(var_name, &var_y);
                let unique = format!("(and {} (forall (({} {})) (=> {} (= {} {}))))", body_str, var_y, sort_str, body_y, var_y, var_name);
                format!("(exists (({} {})) {})", var_name, sort_str, unique)
            }
        },
    }
}

fn collect_opacities_term(term: &Term, opacities: &mut Vec<OpacityEntry>) {
    match term {
        Term::Var { .. } | Term::Const { .. } => {},
        Term::Ctor { args, .. } => {
            for a in args {
                collect_opacities_term(a, opacities);
            }
        },
        Term::Lambda { param_sort, body, .. } => {
            let (_, reason) = emit_sort_with_reason(param_sort);
            if let Some(reason_code) = reason {
                let serialized = serde_json::to_value(param_sort).unwrap_or(serde_json::Value::Null);
                let cid = position_cid_of(&serialized);
                opacities.push(OpacityEntry {
                    position_cid: cid,
                    reason_code,
                });
            }
            collect_opacities_term(body, opacities);
        },
        Term::Let { bindings, body, .. } => {
            for b in bindings {
                collect_opacities_term(&b.bound_term, opacities);
            }
            collect_opacities_term(body, opacities);
        },
    }
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

pub fn compile_formula(formula: &Formula) -> CompiledFormula {
    let mut free_vars = BTreeMap::new();
    let bound = BTreeSet::new();
    collect_free_vars_formula(formula, &mut free_vars, &bound);

    let mut opacities: Vec<OpacityEntry> = Vec::new();
    let body_formula = emit_formula_with_opacities(formula, &mut opacities);

    // Sort opacities by positionCid ascending, then reasonCode ascending.
    opacities.sort_by(|a, b| {
        a.position_cid.cmp(&b.position_cid)
            .then_with(|| a.reason_code.cmp(&b.reason_code))
    });
    opacities.dedup();

    let opacity_manifest = OpacityManifest {
        protocol_version: "ir-compiler-protocol/2".to_string(),
        compiler: DIALECT.to_string(),
        compiler_version: COMPILER_VERSION.to_string(),
        opacities,
    };

    // Check whether the formula references Outlives — if so, inject the
    // kernel axioms (per protocol/specs/2026-05-05-outlives-kernel-axioms.md §2).
    let has_outlives = has_outlives_predicate(formula);
    let mut preamble = String::new();
    preamble.push_str("(set-logic ALL)\n");
    if has_outlives {
        // Declare the Region sort and Outlives predicate.
        preamble.push_str("(declare-sort Region 0)\n");
        preamble.push_str("(declare-fun Outlives (Region Region) Bool)\n");
        // Kernel axiom 1: reflexivity — Outlives(r, r) always holds.
        preamble.push_str("(assert (forall ((r Region)) (Outlives r r)))\n");
        // Kernel axiom 2: transitivity — Outlives(r1, r2) ∧ Outlives(r2, r3) → Outlives(r1, r3).
        preamble.push_str("(assert (forall ((r1 Region) (r2 Region) (r3 Region)) (=> (and (Outlives r1 r2) (Outlives r2 r3)) (Outlives r1 r3))))\n");
        // Kernel axiom 3: 'static top element — Outlives('static, r) for every region r.
        // 'static outlives every region per spec §2.3 (corrected in commit 655ab84).
        preamble.push_str("(declare-fun static_region () Region)\n");
        preamble.push_str("(assert (forall ((r Region)) (Outlives static_region r)))\n");
    }
    for (name, sort) in free_vars.iter() {
        preamble.push_str(&format!("(declare-const {} {})\n", name, sort));
    }
    let body = format!("(assert (not {}))\n(check-sat)\n", body_formula);
    let free_vars_vec = free_vars.into_iter().map(|(name, sort)| FreeVar { name, sort });
    let free_vars_vec = free_vars_vec.collect();
    CompiledFormula { preamble, body, free_vars: free_vars_vec, opacity_manifest }
}

/// Recursively check whether a formula tree references the `Outlives`
/// atomic predicate.
fn has_outlives_predicate(formula: &Formula) -> bool {
    match formula {
        Formula::Atomic { name, .. } => name == "Outlives",
        Formula::And { operands } | Formula::Or { operands } | Formula::Implies { operands } => {
            operands.iter().any(|o| has_outlives_predicate(o))
        }
        Formula::Not { operands } => operands.iter().any(|o| has_outlives_predicate(o)),
        Formula::Forall { body, .. } | Formula::Exists { body, .. } => {
            has_outlives_predicate(body)
        }
        Formula::Choice { body, .. } => has_outlives_predicate(body),
    }
}
