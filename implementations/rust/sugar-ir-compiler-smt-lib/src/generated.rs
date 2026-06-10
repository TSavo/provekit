// SPDX-License-Identifier: Apache-2.0
// SMT-LIB v2.6 compiler.
//
// HISTORICAL NOTE on the "GENERATED" label: this file's logic has been
// HAND-MAINTAINED for a long time (see git log -- every SMT-encoding fix is a
// manual edit; there is NO active generator that writes this file). The CDDL
// generator `tools/generate-from-cddl.py` emits the IR *type* definitions
// (`sugar-ir-types`) and a JSON Document emitter -- NOT this SMT-LIB
// compiler. The label is vestigial.
//
// CLOBBER-PROOFING: to be safe against a hypothetical future regeneration, the
// literal-constant encoding (string -> uninterpreted Int const, bool -> int,
// None/str/number cross-type distinctness per Python `==`) lives in the
// SEPARATE hand-maintained module `crate::literal_encoding`, which this file
// merely CALLS. Even a full rewrite of this file cannot silently revert that
// soundness-critical encoding without also touching `literal_encoding.rs`.

#![allow(unused_imports, unused_mut, unreachable_patterns)]

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use sugar_canonicalizer::{blake3_512_of, encode_jcs, Value as CValue};
use sugar_ir_compiler::{CompiledFormula, FreeVar, OpacityEntry, OpacityManifest};
use sugar_ir_types::*;

use crate::{COMPILER_NAME, COMPILER_VERSION, DIALECT};

pub fn emit_term(term: &Term) -> String {
    match term {
        // Quote Var names the same way ctor names are quoted: a synthetic
        // name like `#field:code` or `#pat:<hash>` (introduced by the
        // struct-literal / match lift) is not a legal simple SMT symbol --
        // unquoted, z3 reads the leading `#f`/`#p` as a malformed
        // bit-vector literal. `smt_quote` wraps it in `|...|`; it is a
        // no-op for ordinary names, so plain identifiers are unchanged.
        Term::Var { name, .. } => smt_quote(name),
        Term::Const { value, sort, .. } => {
            let sort_name = match sort {
                Sort::Primitive { name } => name.as_str(),
                Sort::Function { .. } | Sort::Dependent { .. } | Sort::Region { .. } => {
                    panic!("smt-lib: Const cannot carry a Function/Dependent/Region sort in pure SMT-LIB v2.6");
                }
            };
            emit_const_value(value, sort_name)
        }
        Term::Ctor { name, args, .. } => {
            if name == "str.len" && args.len() == 1 {
                return format!("(str.len {})", emit_string_term(&args[0]));
            }
            if args.is_empty() {
                return smt_quote(name);
            };
            let args_str = args.iter();
            let args_str = args_str.map(emit_term);
            let args_str: Vec<String> = args_str.collect();
            format!("({} {})", smt_quote(name), args_str.join(" "))
        }
        Term::Lambda {
            param_name,
            param_sort,
            body,
            ..
        } => {
            let sort_str = emit_sort(param_sort);
            let body_str = emit_term(body);
            // Quote the binder name so a unique-renamed param like `e#0`
            // (the `#N` suffix the lifter's LiftCtx appends) is a legal
            // SMT symbol `|e#0|` -- and matches the quoted Var reference to
            // it in the body. Unquoted, z3 reads `#0` as a malformed
            // bit-vector literal.
            format!(
                "(lambda (({} {})) {})",
                smt_quote(param_name),
                sort_str,
                body_str
            )
        }
        Term::Let { bindings, body, .. } => {
            let mut binding_strs = bindings.iter();
            let binding_strs = binding_strs
                .map(|b| format!("({} {})", smt_quote(&b.name), emit_term(&b.bound_term)));
            let binding_strs: Vec<String> = binding_strs.collect();
            let body_str = emit_term(body);
            format!("(let ({}) {})", binding_strs.join(" "), body_str)
        }
        Term::Ctor { name, args } => {
            if name == "str.len" && args.len() == 1 {
                return format!("(str.len {})", emit_string_term(&args[0]));
            }
            if args.is_empty() {
                return smt_quote(name);
            };
            let args_str = args.iter();
            let args_str = args_str.map(emit_term);
            let args_str: Vec<String> = args_str.collect();
            format!("({} {})", smt_quote(name), args_str.join(" "))
        }
        Term::Lambda {
            param_name,
            param_sort,
            body,
        } => {
            let sort_str = emit_sort(param_sort);
            let body_str = emit_term(body);
            // Quote the binder name so a unique-renamed param like `e#0`
            // (the `#N` suffix the lifter's LiftCtx appends) is a legal
            // SMT symbol `|e#0|` -- and matches the quoted Var reference to
            // it in the body. Unquoted, z3 reads `#0` as a malformed
            // bit-vector literal.
            format!(
                "(lambda (({} {})) {})",
                smt_quote(param_name),
                sort_str,
                body_str
            )
        }
        Term::Let { bindings, body } => {
            let binding_strs = bindings.iter();
            let binding_strs = binding_strs
                .map(|b| format!("({} {})", smt_quote(&b.name), emit_term(&b.bound_term)));
            let binding_strs: Vec<String> = binding_strs.collect();
            let body_str = emit_term(body);
            format!("(let ({}) {})", binding_strs.join(" "), body_str)
        }
    }
}

/// Emit a sort as SMT-LIB surface syntax. Returns (smt_string, reason_code)
/// where reason_code is Some if the sort was opaque.
fn emit_sort_with_reason(sort: &Sort) -> (String, Option<String>) {
    match sort {
        Sort::Primitive { name } if is_supported_smt_primitive_sort(name) => (name.clone(), None),
        Sort::Primitive { name } => (
            "Int".to_string(),
            Some(format!("opaque_primitive_sort:{name}")),
        ),
        Sort::Function { .. } => (
            "Int".to_string(),
            Some("predicate_quantification".to_string()),
        ),
        Sort::Dependent { .. } => ("Int".to_string(), Some("dependent_type".to_string())),
        Sort::Region { .. } => (
            "Int".to_string(),
            Some("other:RegionSort pre-resolved in composition".to_string()),
        ),
    }
}

fn is_supported_smt_primitive_sort(name: &str) -> bool {
    matches!(name, "Int" | "Bool" | "Real" | "String")
}

pub fn emit_sort(sort: &Sort) -> String {
    emit_sort_with_reason(sort).0
}

/// Derive a deterministic, language-blind SMT-LIB sort name for an opaque
/// sort. Uses the blake3 CID of the serialized sort as the disambiguator so
/// two distinct opaque sorts always get distinct names, and the same sort
/// always gets the same name within a compilation unit.
///
/// Output format: `S_<first-32-hex-chars-of-CID>`. Prefix ensures the name
/// starts with a letter (SMT-LIB simple symbol rule). The 32-char CID prefix
/// gives 128 bits of collision resistance -- more than sufficient for any
/// realistic formula. The symbol is safe for SMT-LIB simple-symbol syntax
/// (only [A-Za-z0-9_]).
fn opaque_sort_smt_name(sort: &Sort) -> String {
    let serialized = serde_json::to_value(sort).unwrap_or(serde_json::Value::Null);
    let cid = position_cid_of(&serialized);
    // Sanitize: keep only alphanumeric and underscore, then prefix with S_.
    let safe: String = cid
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '_')
        .take(32)
        .collect();
    format!("S_{}", safe)
}

/// Walk a formula collecting the SMT sort names for all opaque-sorted
/// quantifiers (Forall/Exists/Choice) so that `(declare-sort <S> 0)` can be
/// emitted into the preamble before the body. Each distinct opaque sort name
/// is stored as a key (value is unused).
fn collect_opaque_quantifier_sorts_formula(formula: &Formula, out: &mut BTreeMap<String, ()>) {
    match formula {
        Formula::Atomic { .. } => {}
        Formula::And { operands }
        | Formula::Or { operands }
        | Formula::Not { operands }
        | Formula::Implies { operands } => {
            for o in operands {
                collect_opaque_quantifier_sorts_formula(o, out);
            }
        }
        Formula::Forall { sort, body, .. }
        | Formula::Exists { sort, body, .. }
        | Formula::Choice { sort, body, .. } => {
            let (_, reason) = emit_sort_with_reason(sort);
            if reason.is_some() {
                out.insert(opaque_sort_smt_name(sort), ());
            }
            collect_opaque_quantifier_sorts_formula(body, out);
        }
        Formula::Substitute { .. } | Formula::Apply { .. } => {}
        Formula::DivergenceBetween { source, target } => {
            collect_opaque_quantifier_sorts_formula(source, out);
            collect_opaque_quantifier_sorts_formula(target, out);
        }
    }
}

pub fn emit_formula(formula: &Formula) -> String {
    match formula {
        Formula::Atomic { name, args } => {
            if let Some(rendered) = emit_string_theory_atomic(name, args) {
                return rendered;
            }
            let smt_name = smt_atomic_name(name);
            if args.is_empty() {
                return smt_name.to_string();
            };
            let args_str = args.iter();
            let args_str = args_str.map(emit_term);
            let args_str: Vec<String> = args_str.collect();
            format!("({} {})", smt_name, args_str.join(" "))
        }
        Formula::And { operands } => {
            let ops_str = operands.iter();
            let ops_str = ops_str.map(emit_formula);
            let ops_str: Vec<String> = ops_str.collect();
            format!("({} {})", "and", ops_str.join(" "))
        }
        Formula::Or { operands } => {
            let ops_str = operands.iter();
            let ops_str = ops_str.map(emit_formula);
            let ops_str: Vec<String> = ops_str.collect();
            format!("({} {})", "or", ops_str.join(" "))
        }
        Formula::Not { operands } => format!("(not {})", emit_formula(&operands[0])),
        Formula::Implies { operands } => format!(
            "(=> {} {})",
            emit_formula(&operands[0]),
            emit_formula(&operands[1])
        ),
        Formula::Forall { name, sort, body } => {
            let (sort_str, reason) = emit_sort_with_reason(sort);
            let body_str = emit_formula(body);
            // Opaque sort: use the CID-derived uninterpreted sort name declared
            // in the preamble. Collapsing to `true` is unsound: `forall x:S.
            // false` would then appear as `true` and pass falsely.
            let effective_sort = if reason.is_some() {
                opaque_sort_smt_name(sort)
            } else {
                sort_str
            };
            format!("(forall (({} {})) {})", name, effective_sort, body_str)
        }
        Formula::Exists { name, sort, body } => {
            let (sort_str, reason) = emit_sort_with_reason(sort);
            let body_str = emit_formula(body);
            let effective_sort = if reason.is_some() {
                opaque_sort_smt_name(sort)
            } else {
                sort_str
            };
            format!("(exists (({} {})) {})", name, effective_sort, body_str)
        }
        Formula::Choice {
            var_name,
            sort,
            body,
        } => {
            let (sort_str, reason) = emit_sort_with_reason(sort);
            let body_str = emit_formula(body);
            let effective_sort = if reason.is_some() {
                opaque_sort_smt_name(sort)
            } else {
                sort_str
            };
            let var_y = format!("{}_y", var_name);
            let body_y = body_str.replace(var_name, &var_y);
            let unique = format!(
                "(and {} (forall (({} {})) (=> {} (= {} {}))))",
                body_str, var_y, effective_sort, body_y, var_y, var_name
            );
            format!("(exists (({} {})) {})", var_name, effective_sort, unique)
        }
        // wp-rule schema nodes (spec 2026-05-13-wp-as-formula.md §2.3):
        // `substitute` / `apply` appear only inside an unreduced `wp_rule`
        // term and are eliminated by `libsugar::wp` before any solver
        // or compiler backend sees the formula. Reaching this arm is a bug.
        // TODO(wp-as-formula PR1+): teach sugar-ir-codegen to emit this arm.
        Formula::Substitute { .. } | Formula::Apply { .. } => {
            unreachable!(
                "wp-rule schema node reached the SMT-LIB formula emitter; \
                 must be reduced via libsugar::wp first"
            )
        }
        Formula::DivergenceBetween { .. } => {
            unreachable!(
                "platform divergence formula reached the SMT-LIB formula emitter; \
                 stage 4 must lower it before backend compilation"
            )
        }
    }
}

fn emit_string_theory_atomic(name: &str, args: &[Term]) -> Option<String> {
    match name {
        "contains" if args.len() == 2 => Some(format!(
            "(str.contains {} {})",
            emit_string_term(&args[0]),
            emit_string_term(&args[1])
        )),
        "prefix-of" if args.len() == 2 => Some(format!(
            "(str.prefixof {} {})",
            emit_string_term(&args[0]),
            emit_string_term(&args[1])
        )),
        "suffix-of" if args.len() == 2 => Some(format!(
            "(str.suffixof {} {})",
            emit_string_term(&args[0]),
            emit_string_term(&args[1])
        )),
        "str.is_ascii" if args.len() == 1 => Some(format!(
            "(str.in_re {} (re.* (re.range \"\\u{{0}}\" \"\\u{{7f}}\")))",
            emit_string_term(&args[0])
        )),
        "str.is_ascii_alphabetic" if args.len() == 1 => Some(format!(
            "(str.in_re {} (re.union (re.range \"A\" \"Z\") (re.range \"a\" \"z\")))",
            emit_string_term(&args[0])
        )),
        "str.is_ascii_alphanumeric" if args.len() == 1 => Some(format!(
            "(str.in_re {} (re.union (re.range \"0\" \"9\") (re.union (re.range \"A\" \"Z\") (re.range \"a\" \"z\"))))",
            emit_string_term(&args[0])
        )),
        "str.is_ascii_digit" if args.len() == 1 => Some(format!(
            "(str.in_re {} (re.range \"0\" \"9\"))",
            emit_string_term(&args[0])
        )),
        "str.is_ascii_octdigit" if args.len() == 1 => Some(format!(
            "(str.in_re {} (re.range \"0\" \"7\"))",
            emit_string_term(&args[0])
        )),
        "str.is_ascii_lowercase" if args.len() == 1 => Some(format!(
            "(str.in_re {} (re.range \"a\" \"z\"))",
            emit_string_term(&args[0])
        )),
        "str.is_ascii_uppercase" if args.len() == 1 => Some(format!(
            "(str.in_re {} (re.range \"A\" \"Z\"))",
            emit_string_term(&args[0])
        )),
        "str.is_ascii_hexdigit" if args.len() == 1 => Some(format!(
            "(str.in_re {} (re.union (re.range \"0\" \"9\") (re.union (re.range \"A\" \"F\") (re.range \"a\" \"f\"))))",
            emit_string_term(&args[0])
        )),
        "str.is_ascii_punctuation" if args.len() == 1 => Some(format!(
            "(str.in_re {} (re.union (re.range \"!\" \"/\") (re.union (re.range \":\" \"@\") (re.union (re.range \"[\" \"`\") (re.range \"{{\" \"~\")))))",
            emit_string_term(&args[0])
        )),
        "str.is_ascii_graphic" if args.len() == 1 => Some(format!(
            "(str.in_re {} (re.range \"!\" \"~\"))",
            emit_string_term(&args[0])
        )),
        "str.is_ascii_whitespace" if args.len() == 1 => Some(format!(
            "(str.in_re {} (re.union (re.union (re.union (re.union (re.range \" \" \" \") (re.range \"\\u{{9}}\" \"\\u{{9}}\")) (re.range \"\\u{{a}}\" \"\\u{{a}}\")) (re.range \"\\u{{c}}\" \"\\u{{c}}\")) (re.range \"\\u{{d}}\" \"\\u{{d}}\")))",
            emit_string_term(&args[0])
        )),
        "str.is_ascii_control" if args.len() == 1 => Some(format!(
            "(str.in_re {} (re.union (re.range \"\\u{{0}}\" \"\\u{{1f}}\") (re.range \"\\u{{7f}}\" \"\\u{{7f}}\")))",
            emit_string_term(&args[0])
        )),
        _ => None,
    }
}

fn is_string_theory_atomic_predicate(name: &str) -> bool {
    matches!(
        name,
        "contains"
            | "prefix-of"
            | "suffix-of"
            | "str.is_ascii"
            | "str.is_ascii_alphabetic"
            | "str.is_ascii_alphanumeric"
            | "str.is_ascii_digit"
            | "str.is_ascii_octdigit"
            | "str.is_ascii_lowercase"
            | "str.is_ascii_uppercase"
            | "str.is_ascii_hexdigit"
            | "str.is_ascii_punctuation"
            | "str.is_ascii_graphic"
            | "str.is_ascii_whitespace"
            | "str.is_ascii_control"
    )
}

fn emit_string_term(term: &Term) -> String {
    match term {
        Term::Const { value, sort } => {
            if matches!(sort, Sort::Primitive { name } if name == "String") {
                if let serde_json::Value::String(s) = value {
                    return smt_string_literal(s);
                }
            }
            emit_term(term)
        }
        Term::Var { name } => smt_quote(name),
        Term::Ctor { name, args } if name == "str.++" && args.len() == 2 => {
            format!(
                "(str.++ {} {})",
                emit_string_term(&args[0]),
                emit_string_term(&args[1])
            )
        }
        Term::Let { body, .. } => emit_string_term(body),
        _ => emit_term(term),
    }
}

fn smt_string_literal(s: &str) -> String {
    let mut out = String::from("\"");
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\"\""),
            '\u{0}'..='\u{1f}' | '\u{7f}' => {
                out.push_str(&format!("\\u{{{:x}}}", ch as u32));
            }
            _ => out.push(ch),
        }
    }
    out.push('"');
    out
}

// String/bool literal encoding + cross-type distinctness live in the
// hand-maintained `literal_encoding` module (NOT here) so a regeneration of
// this file cannot silently revert them. See that module's header for the
// full Python-`==`-semantics rationale.
use crate::literal_encoding::{emit_const_value as encode_const, LiteralConstants};

// isinstance disjointness axioms live in the hand-maintained
// `isinstance_encoding` module (NOT here) so a regeneration of this file
// cannot silently revert the soundness-critical type-disjointness encoding.
use crate::isinstance_encoding::IsinstanceClauses;

fn emit_const_value(value: &serde_json::Value, sort_name: &str) -> String {
    // A `Real`-sorted const is a real literal carried as a CANONICAL DECIMAL
    // STRING (e.g. "0.00000015") so its CID is deterministic. Emit it verbatim as
    // an SMT-LIB Real literal. SMT-LIB has no negative real *literal*, so a
    // leading "-" renders as the unary-minus application `(- X)`.
    if sort_name == "Real" {
        if let Some(s) = value.as_str() {
            return match s.strip_prefix('-') {
                Some(mag) => format!("(- {mag})"),
                None => s.to_string(),
            };
        }
    }
    // Every other sort: the Int-universe literal encoding (int/bool -> int value,
    // string/None -> hash-named uninterpreted Int const). Unchanged, so every
    // pre-existing (Real-free) formula is byte-for-byte identical.
    encode_const(value)
}

// smt_quote renders a name as an SMT-LIB symbol, quoting with |...| when it is
// not a valid simple symbol (e.g. lifted ctor names like `go:call`, which
// contain ':' -- an unquoted ':' is a syntax error z3 rejects). Applied
// consistently at ctor applications and their declare-fun, so the symbol
// matches. NOTE: mirror this in tools/generate-from-cddl.py on regeneration.
fn smt_quote(name: &str) -> String {
    let simple = !name.is_empty()
        && !name.chars().next().is_some_and(|c| c.is_ascii_digit())
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "~!@$%^&*_-+=<>.?/".contains(c));
    if simple {
        name.to_string()
    } else {
        format!("|{}|", name)
    }
}

fn smt_atomic_name(name: &str) -> &str {
    match name {
        "eq" => "=",
        "ne" | "neq" => "distinct",
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
            if let Some(i) = n.as_i64() {
                CValue::integer(i)
            } else if let Some(f) = n.as_f64() {
                CValue::string(format!("{}", f))
            } else {
                CValue::null()
            }
        }
        serde_json::Value::String(s) => CValue::string(s.clone()),
        serde_json::Value::Array(arr) => CValue::array(arr.iter().map(to_cvalue).collect()),
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
        }
        Formula::And { operands } => {
            let ops: Vec<String> = operands
                .iter()
                .map(|o| emit_formula_with_opacities(o, opacities))
                .collect();
            format!("({} {})", "and", ops.join(" "))
        }
        Formula::Or { operands } => {
            let ops: Vec<String> = operands
                .iter()
                .map(|o| emit_formula_with_opacities(o, opacities))
                .collect();
            format!("({} {})", "or", ops.join(" "))
        }
        Formula::Not { operands } => {
            format!(
                "(not {})",
                emit_formula_with_opacities(&operands[0], opacities)
            )
        }
        Formula::Implies { operands } => {
            format!(
                "(=> {} {})",
                emit_formula_with_opacities(&operands[0], opacities),
                emit_formula_with_opacities(&operands[1], opacities)
            )
        }
        Formula::Forall { name, sort, body } => {
            let (_, reason) = emit_sort_with_reason(sort);
            let effective_sort = if let Some(reason_code) = reason {
                // Record opacity provenance. Still emit a sound quantifier
                // using the CID-derived uninterpreted sort name declared in
                // the preamble. Collapsing to `true` is unsound.
                let serialized = serde_json::to_value(sort).unwrap_or(serde_json::Value::Null);
                let cid = position_cid_of(&serialized);
                opacities.push(OpacityEntry {
                    position_cid: cid,
                    reason_code,
                });
                opaque_sort_smt_name(sort)
            } else {
                emit_sort(sort)
            };
            let body_str = emit_formula_with_opacities(body, opacities);
            format!("(forall (({} {})) {})", name, effective_sort, body_str)
        }
        Formula::Exists { name, sort, body } => {
            let (_, reason) = emit_sort_with_reason(sort);
            let effective_sort = if let Some(reason_code) = reason {
                let serialized = serde_json::to_value(sort).unwrap_or(serde_json::Value::Null);
                let cid = position_cid_of(&serialized);
                opacities.push(OpacityEntry {
                    position_cid: cid,
                    reason_code,
                });
                opaque_sort_smt_name(sort)
            } else {
                emit_sort(sort)
            };
            let body_str = emit_formula_with_opacities(body, opacities);
            format!("(exists (({} {})) {})", name, effective_sort, body_str)
        }
        Formula::Choice {
            var_name,
            sort,
            body,
        } => {
            let (_, reason) = emit_sort_with_reason(sort);
            let effective_sort = if let Some(reason_code) = reason {
                let serialized = serde_json::to_value(sort).unwrap_or(serde_json::Value::Null);
                let cid = position_cid_of(&serialized);
                opacities.push(OpacityEntry {
                    position_cid: cid,
                    reason_code,
                });
                opaque_sort_smt_name(sort)
            } else {
                emit_sort(sort)
            };
            let body_str = emit_formula_with_opacities(body, opacities);
            let var_y = format!("{}_y", var_name);
            let body_y = body_str.replace(var_name, &var_y);
            let unique = format!(
                "(and {} (forall (({} {})) (=> {} (= {} {}))))",
                body_str, var_y, effective_sort, body_y, var_y, var_name
            );
            format!("(exists (({} {})) {})", var_name, effective_sort, unique)
        }
        // wp-rule schema nodes (spec 2026-05-13-wp-as-formula.md §2.3):
        // see the note in `emit_formula`.
        // TODO(wp-as-formula PR1+): teach sugar-ir-codegen to emit this arm.
        Formula::Substitute { .. } | Formula::Apply { .. } => {
            unreachable!(
                "wp-rule schema node reached the SMT-LIB opacity emitter; \
                 must be reduced via libsugar::wp first"
            )
        }
        Formula::DivergenceBetween { .. } => {
            unreachable!(
                "platform divergence formula reached the SMT-LIB opacity emitter; \
                 stage 4 must lower it before backend compilation"
            )
        }
    }
}

fn collect_opacities_term(term: &Term, opacities: &mut Vec<OpacityEntry>) {
    match term {
        Term::Var { .. } | Term::Const { .. } => {}
        Term::Ctor { args, .. } => {
            for a in args {
                collect_opacities_term(a, opacities);
            }
        }
        Term::Lambda {
            param_sort, body, ..
        } => {
            let (_, reason) = emit_sort_with_reason(param_sort);
            if let Some(reason_code) = reason {
                let serialized =
                    serde_json::to_value(param_sort).unwrap_or(serde_json::Value::Null);
                let cid = position_cid_of(&serialized);
                opacities.push(OpacityEntry {
                    position_cid: cid,
                    reason_code,
                });
            }
            collect_opacities_term(body, opacities);
        }
        Term::Let { bindings, body, .. } => {
            for b in bindings {
                collect_opacities_term(&b.bound_term, opacities);
            }
            collect_opacities_term(body, opacities);
        }
    }
}

pub fn collect_free_vars_formula(
    formula: &Formula,
    out: &mut BTreeMap<String, String>,
    bound: &BTreeSet<String>,
) {
    match formula {
        Formula::Atomic { name, args } => {
            if is_string_theory_atomic_predicate(name) {
                for a in args {
                    collect_free_vars_string_term(a, out, bound);
                }
                return;
            }
            if is_float_refinement_atomic_predicate(name) {
                for a in args {
                    collect_free_vars_term_ctx(a, out, bound, true);
                }
                return;
            }
            // A var in an atom that carries a `Real` const is a real-arithmetic
            // operand (e.g. `(< (- a b) 0.00000015)`): declare it `Real`, not
            // `Int`. Atoms with no Real const collect exactly as before, so all
            // pre-existing (Real-free) formulas are byte-for-byte identical.
            let real_ctx = args.iter().any(term_has_real_const);
            for a in args {
                collect_free_vars_term_ctx(a, out, bound, real_ctx);
            }
        }
        Formula::And { operands } => {
            for o in operands {
                collect_free_vars_formula(o, out, bound);
            }
        }
        Formula::Or { operands } => {
            for o in operands {
                collect_free_vars_formula(o, out, bound);
            }
        }
        Formula::Not { operands } => {
            for o in operands {
                collect_free_vars_formula(o, out, bound);
            }
        }
        Formula::Implies { operands } => {
            for o in operands {
                collect_free_vars_formula(o, out, bound);
            }
        }
        Formula::Forall {
            name,
            sort: _,
            body,
        } => {
            let mut nb = bound.clone();
            nb.insert(name.clone());
            collect_free_vars_formula(body, out, &nb);
        }
        Formula::Exists {
            name,
            sort: _,
            body,
        } => {
            let mut nb = bound.clone();
            nb.insert(name.clone());
            collect_free_vars_formula(body, out, &nb);
        }
        Formula::Choice {
            var_name,
            sort: _,
            body,
        } => {
            let mut nb = bound.clone();
            nb.insert(var_name.clone());
            collect_free_vars_formula(body, out, &nb);
        }
        // wp-rule schema nodes (spec 2026-05-13-wp-as-formula.md §2.3):
        // see the note in `emit_formula`.
        // TODO(wp-as-formula PR1+): teach sugar-ir-codegen to emit this arm.
        Formula::Substitute { .. } | Formula::Apply { .. } => {
            unreachable!(
                "wp-rule schema node reached the SMT-LIB free-var collector; \
                 must be reduced via libsugar::wp first"
            )
        }
        Formula::DivergenceBetween { source, target } => {
            collect_free_vars_formula(source, out, bound);
            collect_free_vars_formula(target, out, bound);
        }
    }
}

/// True iff the term contains a `Real`-sorted constant anywhere. Used to mark an
/// enclosing atom as real-arithmetic so its variable operands declare as `Real`.
fn term_has_real_const(term: &Term) -> bool {
    match term {
        Term::Const { sort, .. } => {
            matches!(sort, Sort::Primitive { name } if name == "Real")
        }
        Term::Ctor { args, .. } => args.iter().any(term_has_real_const),
        Term::Lambda { body, .. } => term_has_real_const(body),
        Term::Let { bindings, body, .. } => {
            bindings.iter().any(|b| term_has_real_const(&b.bound_term)) || term_has_real_const(body)
        }
        Term::Var { .. } => false,
    }
}

fn collect_free_vars_term_ctx(
    term: &Term,
    out: &mut BTreeMap<String, String>,
    bound: &BTreeSet<String>,
    real_ctx: bool,
) {
    match term {
        Term::Var { name, .. } => {
            if !bound.contains(name) {
                if real_ctx {
                    // Real dominates Int: a var used as a real operand anywhere is
                    // declared Real regardless of collection order.
                    out.insert(name.clone(), "Real".to_string());
                } else {
                    out.entry(name.clone()).or_insert_with(|| "Int".to_string());
                }
            }
        }
        Term::Const { .. } => {}
        Term::Ctor { args, .. } => {
            if let Term::Ctor { name, args } = term {
                if name == "str.len" && args.len() == 1 {
                    collect_free_vars_string_term(&args[0], out, bound);
                    return;
                }
            }
            for a in args {
                collect_free_vars_term_ctx(a, out, bound, real_ctx);
            }
        }
        Term::Lambda {
            param_name,
            param_sort: _,
            body,
            ..
        } => {
            let mut nb = bound.clone();
            nb.insert(param_name.clone());
            collect_free_vars_term_ctx(body, out, &nb, real_ctx);
        }
        Term::Let { bindings, body, .. } => {
            let mut current_bound = bound.clone();
            for b in bindings {
                collect_free_vars_term_ctx(&b.bound_term, out, &current_bound, real_ctx);
                current_bound.insert(b.name.clone());
            }
            collect_free_vars_term_ctx(body, out, &current_bound, real_ctx);
        }
    }
}

fn collect_free_vars_string_term(
    term: &Term,
    out: &mut BTreeMap<String, String>,
    bound: &BTreeSet<String>,
) {
    match term {
        Term::Var { name, .. } => {
            if !bound.contains(name) {
                out.insert(name.clone(), "String".to_string());
            }
        }
        Term::Const { .. } => {}
        Term::Ctor { args, .. } => {
            for a in args {
                collect_free_vars_string_term(a, out, bound);
            }
        }
        Term::Lambda {
            param_name, body, ..
        } => {
            let mut nb = bound.clone();
            nb.insert(param_name.clone());
            collect_free_vars_string_term(body, out, &nb);
        }
        Term::Let { bindings, body, .. } => {
            let mut current_bound = bound.clone();
            for b in bindings {
                collect_free_vars_string_term(&b.bound_term, out, &current_bound);
                current_bound.insert(b.name.clone());
            }
            collect_free_vars_string_term(body, out, &current_bound);
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CtorSignature {
    args: Vec<String>,
    ret: String,
}

fn sort_name(sort: &Sort) -> String {
    emit_sort(sort)
}

fn known_term_sort(term: &Term) -> Option<String> {
    match term {
        Term::Const { sort, value } => {
            // String AND bool literals are encoded into the Int universe (see
            // `literal_encoding`): strings as hash-named uninterpreted Int
            // consts, bools as concrete ints (True->1, False->0). Return "Int"
            // for both so ctor-decl / predicate-decl passes emit consistent
            // Int-sort declarations rather than ill-sorted String/Bool ones
            // that z3 rejects against an Int free var.
            match sort {
                Sort::Primitive { name }
                    if name == "String" && matches!(value, serde_json::Value::String(_)) =>
                {
                    return Some("Int".to_string());
                }
                Sort::Primitive { name }
                    if name == "Bool" && matches!(value, serde_json::Value::Bool(_)) =>
                {
                    return Some("Int".to_string());
                }
                _ => {}
            }
            Some(sort_name(sort))
        }
        Term::Var { .. } => Some("Int".to_string()),
        Term::Ctor { name, .. } if name == "str.len" => Some("Int".to_string()),
        Term::Ctor { .. } => None,
        Term::Lambda { .. } => None,
        Term::Let { body, .. } => known_term_sort(body),
    }
}

fn expected_atomic_arg_sort(name: &str, args: &[Term]) -> Option<String> {
    if is_float_refinement_atomic_predicate(name) {
        return Some("Real".to_string());
    }
    let smt_name = smt_atomic_name(name);
    if matches!(smt_name, "=" | "distinct" | "<" | "<=" | ">" | ">=") {
        return args
            .iter()
            .find_map(known_term_sort)
            .or_else(|| Some("Int".to_string()));
    }
    None
}

fn collect_ctor_decls_formula(formula: &Formula, out: &mut BTreeMap<String, CtorSignature>) {
    match formula {
        Formula::Atomic { name, args } => {
            let expected = expected_atomic_arg_sort(name, args);
            for arg in args {
                collect_ctor_decls_term(arg, expected.as_deref(), out);
            }
        }
        Formula::And { operands } | Formula::Or { operands } | Formula::Implies { operands } => {
            for operand in operands {
                collect_ctor_decls_formula(operand, out);
            }
        }
        Formula::Not { operands } => {
            for operand in operands {
                collect_ctor_decls_formula(operand, out);
            }
        }
        Formula::Forall { body, .. }
        | Formula::Exists { body, .. }
        | Formula::Choice { body, .. } => {
            collect_ctor_decls_formula(body, out);
        }
        // wp-rule schema nodes (spec 2026-05-13-wp-as-formula.md §2.3):
        // see the note in `emit_formula`.
        // TODO(wp-as-formula PR1+): teach sugar-ir-codegen to emit this arm.
        Formula::Substitute { .. } | Formula::Apply { .. } => {
            unreachable!(
                "wp-rule schema node reached the SMT-LIB ctor-decl collector; \
                 must be reduced via libsugar::wp first"
            )
        }
        Formula::DivergenceBetween { source, target } => {
            collect_ctor_decls_formula(source, out);
            collect_ctor_decls_formula(target, out);
        }
    }
}

/// True iff `name` is an SMT-LIB theory operator that may appear in TERM
/// position and MUST stay interpreted -- never declared as an uninterpreted
/// function. This is the term-position analogue of
/// `is_builtin_atomic_predicate` (which covers boolean-position builtins).
///
/// The set is `+ - *`: the exact arithmetic operators the verifier's solver
/// dispatcher (`sugar-verifier/src/solvers/dispatch.rs`) classifies as
/// linear-arithmetic. A Java/Go/... lifter lowers `x * 2` to `ctor("*", ...)`,
/// so without this guard the honesty-layer ctor-declaration pass emitted
/// `(declare-fun * (Int Int) Int)`, shadowing the theory and turning a proven
/// `(= (* 3 2) 6)` obligation `sat` (false counterexample).
///
/// Integer `/` and `%` are DELIBERATELY excluded: SMT-LIB Int division/modulo
/// semantics (Euclidean) differ from source truncation, so leaving them
/// uninterpreted is the sound choice (the cardinal-sin guard). They keep
/// getting declared uninterpreted, exactly as before.
fn is_builtin_term_operator(name: &str) -> bool {
    matches!(name, "+" | "-" | "*" | "str.len" | "str.++")
}

fn collect_ctor_decls_term(
    term: &Term,
    expected_ret: Option<&str>,
    out: &mut BTreeMap<String, CtorSignature>,
) {
    match term {
        Term::Ctor { name, args } => {
            let arg_sorts: Vec<String> = args
                .iter()
                .map(|arg| known_term_sort(arg).unwrap_or_else(|| "Int".to_string()))
                .collect();
            // Arithmetic theory operators stay interpreted: declaring them as
            // uninterpreted functions would shadow the SMT theory and let the
            // solver pick a counterexample interpretation. Still recurse into
            // the arguments so any genuine non-builtin ctor nested underneath
            // (e.g. `Ok`, `method:foo`) is declared.
            if !is_builtin_term_operator(name) {
                out.entry(name.clone()).or_insert_with(|| CtorSignature {
                    args: arg_sorts.clone(),
                    ret: expected_ret.unwrap_or("Int").to_string(),
                });
            }
            for (arg, arg_sort) in args.iter().zip(arg_sorts.iter()) {
                collect_ctor_decls_term(arg, Some(arg_sort), out);
            }
        }
        Term::Lambda { body, .. } => collect_ctor_decls_term(body, expected_ret, out),
        Term::Let { bindings, body } => {
            for binding in bindings {
                collect_ctor_decls_term(&binding.bound_term, None, out);
            }
            collect_ctor_decls_term(body, expected_ret, out);
        }
        Term::Var { .. } | Term::Const { .. } => {}
    }
}

/// True iff `name` (after `smt_atomic_name` normalization) is an SMT-LIB
/// builtin/theory predicate that needs no declaration. Everything else is a
/// user-defined (uninterpreted) predicate symbol -- `is_some`, `is_ok`,
/// `is_empty`, ... -- that MUST be declared as a Bool-returning function
/// before it can appear in boolean position. This recognizes no particular
/// predicate name as special: it is the COMPLEMENT of the builtin set, so it
/// is generic and language-blind.
fn is_builtin_atomic_predicate(name: &str) -> bool {
    if is_string_theory_atomic_predicate(name) {
        return true;
    }
    matches!(
        smt_atomic_name(name),
        // Equality / relational theory predicates.
        "=" | "distinct" | "<" | "<=" | ">" | ">="
        // Boolean literals (nullary, emitted verbatim).
        | "true" | "false"
        // The lifetime-kernel predicate is declared explicitly in the preamble.
        | "Outlives"
    )
}

fn is_float_refinement_atomic_predicate(name: &str) -> bool {
    matches!(
        name,
        "float.f32.is_nan" | "float.f64.is_nan" | "float.f32.is_infinite" | "float.f64.is_infinite"
    )
}

/// Collect every NON-BUILTIN atomic predicate that appears in boolean
/// position, mapped to its declared signature (`(argSorts) Bool`).
///
/// This is the predicate analogue of `collect_ctor_decls_formula`: a ctor
/// (`Ok`, `method:unwrap`) sitting in TERM position is declared as a value
/// function by that pass, but a PREDICATE (`is_some`) sitting in BOOLEAN
/// position -- e.g. as the antecedent/consequent of an implication in a
/// guard-discharge obligation `(=> (is_some opt) (is_some opt))` -- was never
/// declared, so the solver rejected it with `unknown constant is_some`. Here
/// we declare it `(declare-fun is_some (<argSorts>) Bool)`. Arg sorts reuse
/// the same `known_term_sort` heuristic the ctor pass uses (var/ctor -> Int),
/// matching the `(declare-const opt Int)` the free-var pass already emits, so
/// applications type-check. A nullary atomic (the boolean literals, or a
/// 0-ary user predicate constant) is left to the existing handling.
fn collect_predicate_decls_formula(formula: &Formula, out: &mut BTreeMap<String, CtorSignature>) {
    match formula {
        Formula::Atomic { name, args } => {
            if !args.is_empty() && !is_builtin_atomic_predicate(name) {
                let expected = expected_atomic_arg_sort(name, args);
                let arg_sorts: Vec<String> = args
                    .iter()
                    .map(|arg| {
                        known_term_sort(arg)
                            .or_else(|| expected.clone())
                            .unwrap_or_else(|| "Int".to_string())
                    })
                    .collect();
                out.entry(smt_atomic_name(name).to_string())
                    .or_insert_with(|| CtorSignature {
                        args: arg_sorts,
                        ret: "Bool".to_string(),
                    });
            }
        }
        Formula::And { operands } | Formula::Or { operands } | Formula::Implies { operands } => {
            for operand in operands {
                collect_predicate_decls_formula(operand, out);
            }
        }
        Formula::Not { operands } => {
            for operand in operands {
                collect_predicate_decls_formula(operand, out);
            }
        }
        Formula::Forall { body, .. }
        | Formula::Exists { body, .. }
        | Formula::Choice { body, .. } => {
            collect_predicate_decls_formula(body, out);
        }
        Formula::Substitute { .. } | Formula::Apply { .. } => {
            unreachable!(
                "wp-rule schema node reached the SMT-LIB predicate-decl collector; \
                 must be reduced via libsugar::wp first"
            )
        }
        Formula::DivergenceBetween { source, target } => {
            collect_predicate_decls_formula(source, out);
            collect_predicate_decls_formula(target, out);
        }
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
        a.position_cid
            .cmp(&b.position_cid)
            .then_with(|| a.reason_code.cmp(&b.reason_code))
    });
    opacities.dedup();

    let opacity_manifest = OpacityManifest {
        protocol_version: "ir-compiler-protocol/2".to_string(),
        compiler: DIALECT.to_string(),
        compiler_version: COMPILER_VERSION.to_string(),
        opacities,
    };

    // Check whether the formula references Outlives. If so, inject the
    // kernel axioms (per protocol/specs/2026-05-05-outlives-kernel-axioms.md §2).
    let has_outlives = has_outlives_predicate(formula);
    let mut preamble = String::new();
    preamble.push_str("(set-logic ALL)\n");
    if has_outlives {
        // Declare the Region sort and Outlives predicate.
        preamble.push_str("(declare-sort Region 0)\n");
        preamble.push_str("(declare-fun Outlives (Region Region) Bool)\n");
        // Kernel axiom 1: reflexivity. Outlives(r, r) always holds.
        preamble.push_str("(assert (forall ((r Region)) (Outlives r r)))\n");
        // Kernel axiom 2: transitivity. Outlives(r1, r2) and Outlives(r2, r3) imply Outlives(r1, r3).
        preamble.push_str("(assert (forall ((r1 Region) (r2 Region) (r3 Region)) (=> (and (Outlives r1 r2) (Outlives r2 r3)) (Outlives r1 r3))))\n");
        // Kernel axiom 3: 'static top element. Outlives('static, r) for every region r.
        // 'static outlives every region per spec §2.3 (corrected in commit 655ab84).
        preamble.push_str("(declare-fun static_region () Region)\n");
        preamble.push_str("(assert (forall ((r Region)) (Outlives static_region r)))\n");
    }
    // Declare every opaque-sorted quantifier sort as an uninterpreted sort.
    // These are sorts the SMT-LIB backend cannot encode natively (non-builtin
    // primitive sorts, function sorts, dependent sorts, ...). Rather than
    // collapsing the quantifier to `true` (which is unsound: `forall x:S.
    // false` would falsely pass), we model each opaque sort as a fresh
    // uninterpreted sort via `(declare-sort <S> 0)`. Z3 then reasons over it
    // under an open-world assumption, which is sound: it can only produce
    // false-negatives (undecidable), never false-positives (false-pass).
    let mut opaque_sort_decls: BTreeMap<String, ()> = BTreeMap::new();
    collect_opaque_quantifier_sorts_formula(formula, &mut opaque_sort_decls);
    for sort_name in opaque_sort_decls.keys() {
        preamble.push_str(&format!("(declare-sort {} 0)\n", sort_name));
    }
    for (name, sort) in free_vars.iter() {
        preamble.push_str(&format!("(declare-const {} {})\n", smt_quote(name), sort));
    }
    // Declare every non-builtin ctor head as an UNINTERPRETED FUNCTION
    // symbol (`Ok`, `Err`, `Some`, `field`, `method:foo`, `tuple`,
    // `json!`-keyed macro terms, ...). This is the reflexive-discharge
    // encoding: an obligation `result == <body term>` whose body term is a
    // self-derived enum/struct/call/macro shape lowers to `f(args) ==
    // f(args)`, which is provable by reflexivity/congruence under ANY
    // interpretation of `f` (the solver never needs to know what `f`
    // means). It is SOUND: if the two sides genuinely differ (a lifter bug
    // emits `result == Ok(x)` for a body returning `Err(x)`), the encoding
    // yields `Ok(x) == Err(x)`, which z3 refutes (the negation is sat), so
    // the obligation stays honestly undecidable. The encoding is
    // self-protecting; it is reflexivity, not blanket-pass.
    //
    // The same declarations were already emitted on the asserted path
    // (`compile_asserted_formula`); they were missing here on the negated
    // path, which is why the lift-time whitelist had to refuse every
    // non-arithmetic post term. With declarations present the whitelist is
    // obsolete: the negated path renders any ctor head as a declared
    // uninterpreted symbol instead of an undeclared-function error.
    let mut ctor_decls = BTreeMap::new();
    collect_ctor_decls_formula(formula, &mut ctor_decls);
    for (name, signature) in ctor_decls.iter() {
        preamble.push_str(&format!(
            "(declare-fun {} ({}) {})\n",
            smt_quote(name),
            signature.args.join(" "),
            signature.ret
        ));
    }
    // Declare every non-builtin atomic PREDICATE in boolean position (e.g.
    // `is_some` in a guard-discharge obligation `(=> (is_some opt) (is_some
    // opt))`). Skip any name already declared as a value ctor above, so a
    // symbol used in both term and boolean position is declared exactly once.
    let mut predicate_decls = BTreeMap::new();
    collect_predicate_decls_formula(formula, &mut predicate_decls);
    for (name, signature) in predicate_decls.iter() {
        if ctor_decls.contains_key(name) {
            continue;
        }
        preamble.push_str(&format!(
            "(declare-fun {} ({}) {})\n",
            smt_quote(name),
            signature.args.join(" "),
            signature.ret
        ));
    }
    // Declare string-literal constants and emit the cross-type distinctness
    // axiom (str/None distinct from each other and from concrete int/bool
    // values; bool encoded as int; floats residual). See `literal_encoding`.
    // The axiom is a Python-TRUE fact, so it is sound on the negated path too:
    // it only removes spurious models, never adds one.
    preamble.push_str(&LiteralConstants::from_formula_for_legacy_literals(formula).preamble());
    // Emit isinstance disjointness clauses for genuinely-disjoint builtin type
    // pairs that appear with the same subject in the formula. These are
    // Python-TRUE facts (ground, quantifier-free). See `isinstance_encoding`.
    preamble.push_str(&IsinstanceClauses::from_formula(formula).preamble());
    let body = format!("(assert (not {}))\n(check-sat)\n", body_formula);
    let free_vars_vec = free_vars
        .into_iter()
        .map(|(name, sort)| FreeVar { name, sort });
    let free_vars_vec = free_vars_vec.collect();
    CompiledFormula {
        preamble,
        body,
        free_vars: free_vars_vec,
        opacity_manifest,
    }
}

pub fn compile_asserted_formula(formula: &Formula) -> CompiledFormula {
    let mut free_vars = BTreeMap::new();
    let bound = BTreeSet::new();
    collect_free_vars_formula(formula, &mut free_vars, &bound);

    let mut opacities: Vec<OpacityEntry> = Vec::new();
    let body_formula = emit_formula_with_opacities(formula, &mut opacities);

    opacities.sort_by(|a, b| {
        a.position_cid
            .cmp(&b.position_cid)
            .then_with(|| a.reason_code.cmp(&b.reason_code))
    });
    opacities.dedup();

    let opacity_manifest = OpacityManifest {
        protocol_version: "ir-compiler-protocol/2".to_string(),
        compiler: DIALECT.to_string(),
        compiler_version: COMPILER_VERSION.to_string(),
        opacities,
    };

    let mut ctor_decls = BTreeMap::new();
    collect_ctor_decls_formula(formula, &mut ctor_decls);

    let has_outlives = has_outlives_predicate(formula);
    let mut preamble = String::new();
    preamble.push_str("(set-logic ALL)\n");
    if has_outlives {
        preamble.push_str("(declare-sort Region 0)\n");
        preamble.push_str("(declare-fun Outlives (Region Region) Bool)\n");
        preamble.push_str("(assert (forall ((r Region)) (Outlives r r)))\n");
        preamble.push_str("(assert (forall ((r1 Region) (r2 Region) (r3 Region)) (=> (and (Outlives r1 r2) (Outlives r2 r3)) (Outlives r1 r3))))\n");
        preamble.push_str("(declare-fun static_region () Region)\n");
        preamble.push_str("(assert (forall ((r Region)) (Outlives static_region r)))\n");
    }
    // Declare opaque-sorted quantifier sorts as uninterpreted sorts (see the
    // matching block in `compile_formula` for full rationale).
    let mut opaque_sort_decls: BTreeMap<String, ()> = BTreeMap::new();
    collect_opaque_quantifier_sorts_formula(formula, &mut opaque_sort_decls);
    for sort_name in opaque_sort_decls.keys() {
        preamble.push_str(&format!("(declare-sort {} 0)\n", sort_name));
    }
    for (name, sort) in free_vars.iter() {
        preamble.push_str(&format!("(declare-const {} {})\n", smt_quote(name), sort));
    }
    for (name, signature) in ctor_decls.iter() {
        preamble.push_str(&format!(
            "(declare-fun {} ({}) {})\n",
            smt_quote(name),
            signature.args.join(" "),
            signature.ret
        ));
    }
    // Declare non-builtin atomic predicates in boolean position (see the
    // matching block in `compile_formula`). Same de-dup against ctor decls.
    let mut predicate_decls = BTreeMap::new();
    collect_predicate_decls_formula(formula, &mut predicate_decls);
    for (name, signature) in predicate_decls.iter() {
        if ctor_decls.contains_key(name) {
            continue;
        }
        preamble.push_str(&format!(
            "(declare-fun {} ({}) {})\n",
            smt_quote(name),
            signature.args.join(" "),
            signature.ret
        ));
    }
    // Declare string-literal constants and emit the cross-type distinctness
    // axiom (str/None distinct from each other and from concrete int/bool
    // values; bool encoded as int; floats residual). See `literal_encoding`.
    preamble.push_str(&LiteralConstants::from_formula_for_legacy_literals(formula).preamble());
    // Emit isinstance disjointness clauses (see `isinstance_encoding`).
    preamble.push_str(&IsinstanceClauses::from_formula(formula).preamble());

    let body = format!("(assert {})\n(check-sat)\n", body_formula);
    let free_vars_vec = free_vars
        .into_iter()
        .map(|(name, sort)| FreeVar { name, sort })
        .collect();
    CompiledFormula {
        preamble,
        body,
        free_vars: free_vars_vec,
        opacity_manifest,
    }
}

/// Recursively check whether a formula tree references the `Outlives`
/// atomic predicate.
fn has_outlives_predicate(formula: &Formula) -> bool {
    match formula {
        Formula::Atomic { name, .. } => name == "Outlives",
        Formula::And { operands } | Formula::Or { operands } | Formula::Implies { operands } => {
            operands.iter().any(has_outlives_predicate)
        }
        Formula::Not { operands } => operands.iter().any(has_outlives_predicate),
        Formula::Forall { body, .. } | Formula::Exists { body, .. } => has_outlives_predicate(body),
        Formula::Choice { body, .. } => has_outlives_predicate(body),
        // wp-rule schema nodes (spec 2026-05-13-wp-as-formula.md §2.3):
        // see the note in `emit_formula`.
        // TODO(wp-as-formula PR1+): teach sugar-ir-codegen to emit this arm.
        Formula::Substitute { target, .. } => has_outlives_predicate(target),
        Formula::Apply { args, .. } => args.iter().any(has_outlives_predicate),
        Formula::DivergenceBetween { source, target } => {
            has_outlives_predicate(source) || has_outlives_predicate(target)
        }
    }
}
