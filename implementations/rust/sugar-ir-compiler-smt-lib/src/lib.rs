// SPDX-License-Identifier: Apache-2.0
//
// Bundled SMT-LIB v2.6 IR compiler. Extracted from the inline
// sugar-verifier::smt_emitter so the same code serves both the
// in-process fast path (verifier deps directly on this crate) and the
// standalone subprocess binary `sugar-ir-smt-lib`.
//
// Spec: protocol/specs/2026-04-30-ir-compiler-protocol.md.

use serde_json::Value as Json;

use sugar_ir_compiler::{
    Capabilities, CompileError, CompiledFormula, IrCompiler, PROTOCOL_VERSION,
};

mod generated;
mod isinstance_encoding;
mod literal_encoding;

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

fn is_term_kind(kind: &str) -> bool {
    matches!(kind, "var" | "const" | "ctor" | "lambda" | "let")
}

/// Walk an IrTerm tree; reject any `Var` with an empty `name`. Spec
/// requires variable names to be non-empty identifiers; an empty name
/// would lower to an empty SMT-LIB symbol which the solver rejects
/// (or worse, silently aliases to another symbol). Validate at the
/// boundary so callers see an honest error instead of a malformed
/// emit.
fn validate_term(term: &sugar_ir_types::IrTerm) -> Result<(), String> {
    match term {
        sugar_ir_types::IrTerm::Var { name } => {
            if name.is_empty() {
                return Err("var name must not be empty".to_string());
            }
            Ok(())
        }
        sugar_ir_types::IrTerm::Const { .. } => Ok(()),
        sugar_ir_types::IrTerm::Ctor { args, .. } => args.iter().try_for_each(validate_term),
        sugar_ir_types::IrTerm::Lambda { body, .. } => validate_term(body),
        sugar_ir_types::IrTerm::Let { body, .. } => validate_term(body),
    }
}

/// H1 [B7]: mixed-sort conjunction detection.
///
/// A conjoined formula can equate the SAME `#euf#` ctor term to a String
/// literal in one row (String-theory regime: the ctor's return sort is SMT
/// `String`) and to an Int/Bool literal in another row (legacy opaque-Int
/// regime: return sort `Int`). One `(declare-fun ...)` cannot carry both
/// return sorts, so the emitted script would be ill-sorted -> z3 parse
/// error -> an OPAQUE Undecidable. Detect the conflict at emit time and
/// return a NAMED error instead, which the verifier surfaces as a loud
/// Undecidable with the reason intact.
///
/// Regime attribution mirrors `literal_encoding::routes_to_string_theory`
/// exactly (same gate the emitter uses to pick the encoding), so detection
/// can never disagree with emission.
fn check_mixed_sort_conjunction(formula: &sugar_ir_types::IrFormula) -> Result<(), String> {
    use std::collections::BTreeMap;
    let mut regimes: BTreeMap<String, &'static str> = BTreeMap::new();
    walk_mixed_sort(formula, &mut regimes)
}

fn mark_regime(
    name: &str,
    regime: &'static str,
    regimes: &mut std::collections::BTreeMap<String, &'static str>,
) -> Result<(), String> {
    if let Some(prev) = regimes.get(name) {
        if *prev != regime {
            return Err(format!(
                "mixed-sort conjunction on {name}: String vs Int \
                 (same ctor equated to a String literal in one row and an \
                 Int/Bool literal in another; one declare-fun cannot carry \
                 both return sorts)"
            ));
        }
    } else {
        regimes.insert(name.to_string(), regime);
    }
    Ok(())
}

fn walk_mixed_sort(
    formula: &sugar_ir_types::IrFormula,
    regimes: &mut std::collections::BTreeMap<String, &'static str>,
) -> Result<(), String> {
    use sugar_ir_types::{IrFormula, IrTerm};
    match formula {
        IrFormula::Atomic { name, args } => {
            let is_real_ctor = |t: &IrTerm| {
                matches!(t, IrTerm::Ctor { name, args } if !(name == "None" && args.is_empty()))
            };
            if literal_encoding::routes_to_string_theory(name, args) {
                // String regime: every non-None ctor in this atom gets SMT
                // `String` return sort from the string-theory emitter.
                for a in args {
                    if let IrTerm::Ctor { name: cn, .. } = a {
                        if is_real_ctor(a) {
                            mark_regime(cn, "String", regimes)?;
                        }
                    }
                }
            } else if name == "=" && args.len() == 2 {
                // Legacy regime: a ctor equated to an Int/Bool literal (or a
                // String literal NOT carrying the String sort -- the opaque
                // strlit_ Int encoding) is declared with Int return sort.
                let is_legacy_const = |t: &IrTerm| {
                    matches!(
                        t,
                        IrTerm::Const {
                            value: serde_json::Value::Number(_) | serde_json::Value::Bool(_),
                            ..
                        }
                    ) || matches!(
                        t,
                        IrTerm::Const {
                            value: serde_json::Value::String(_),
                            sort,
                        } if !matches!(sort, sugar_ir_types::Sort::Primitive { name } if name == "String")
                    )
                };
                for (i, j) in [(0usize, 1usize), (1, 0)] {
                    if is_real_ctor(&args[i]) && is_legacy_const(&args[j]) {
                        if let IrTerm::Ctor { name: cn, .. } = &args[i] {
                            mark_regime(cn, "Int", regimes)?;
                        }
                    }
                }
            }
            Ok(())
        }
        IrFormula::And { operands }
        | IrFormula::Or { operands }
        | IrFormula::Not { operands }
        | IrFormula::Implies { operands } => {
            operands.iter().try_for_each(|o| walk_mixed_sort(o, regimes))
        }
        IrFormula::Forall { body, .. }
        | IrFormula::Exists { body, .. }
        | IrFormula::Choice { body, .. } => walk_mixed_sort(body, regimes),
        IrFormula::Substitute { .. }
        | IrFormula::Apply { .. }
        | IrFormula::DivergenceBetween { .. } => Ok(()),
    }
}

fn validate_formula(formula: &sugar_ir_types::IrFormula) -> Result<(), String> {
    match formula {
        sugar_ir_types::IrFormula::Atomic { args, .. } => args.iter().try_for_each(validate_term),
        sugar_ir_types::IrFormula::And { operands }
        | sugar_ir_types::IrFormula::Or { operands }
        | sugar_ir_types::IrFormula::Not { operands }
        | sugar_ir_types::IrFormula::Implies { operands } => {
            operands.iter().try_for_each(validate_formula)
        }
        sugar_ir_types::IrFormula::Forall { body, .. }
        | sugar_ir_types::IrFormula::Exists { body, .. }
        | sugar_ir_types::IrFormula::Choice { body, .. } => validate_formula(body),
        // wp-rule schema nodes (spec 2026-05-13-wp-as-formula.md §2.3):
        // `substitute` / `apply` appear only inside an unreduced `wp_rule`
        // term; `libsugar::wp` eliminates them before any formula reaches
        // the SMT-LIB backend. Reaching this arm means a `wp_rule` schema was
        // handed to the solver without instantiation.
        sugar_ir_types::IrFormula::Substitute { .. } | sugar_ir_types::IrFormula::Apply { .. } => {
            Err(
                "wp-rule schema node (substitute/apply) reached the SMT-LIB validator; \
             it must be reduced via libsugar::wp before solving"
                    .to_string(),
            )
        }
        sugar_ir_types::IrFormula::DivergenceBetween { .. } => Err(
            "platform divergence formula reached the SMT-LIB validator; \
             stage 4 must lower it before solving"
                .to_string(),
        ),
    }
}

/// Legacy single-string entry point. Equal to `preamble + body` from
/// `compile_to_parts`. Also accepts bare terms (lambda, let, etc.) for
/// backward compatibility with the historical verifier emitter.
pub fn emit(ir_formula: &Json) -> Result<String, String> {
    let kind = ir_formula
        .get("kind")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if is_term_kind(kind) {
        let term: sugar_ir_types::Term =
            serde_json::from_value(ir_formula.clone()).map_err(|e| format!("{e}"))?;
        validate_term(&term)?;
        Ok(generated::emit_term(&term))
    } else {
        compile_to_parts(ir_formula)
            .map(|c| {
                let mut s = c.preamble;
                s.push_str(&c.body);
                s
            })
            .map_err(|e| e.to_string())
    }
}

/// Compile to (preamble, body, free_vars). Pure; no I/O.
pub fn compile_to_parts(ir_formula: &Json) -> Result<CompiledFormula, CompileError> {
    let formula: sugar_ir_types::Formula = serde_json::from_value(ir_formula.clone())
        .map_err(|e| CompileError::MalformedIr(e.to_string()))?;
    validate_formula(&formula).map_err(CompileError::MalformedIr)?;
    check_mixed_sort_conjunction(&formula).map_err(CompileError::UnsupportedSort)?;
    Ok(generated::compile_formula(&formula))
}

pub fn compile_asserted_to_parts(ir_formula: &Json) -> Result<CompiledFormula, CompileError> {
    let formula: sugar_ir_types::Formula = serde_json::from_value(ir_formula.clone())
        .map_err(|e| CompileError::MalformedIr(e.to_string()))?;
    validate_formula(&formula).map_err(CompileError::MalformedIr)?;
    check_mixed_sort_conjunction(&formula).map_err(CompileError::UnsupportedSort)?;
    Ok(generated::compile_asserted_formula(&formula))
}

pub fn emit_asserted(ir_formula: &Json) -> Result<String, String> {
    compile_asserted_to_parts(ir_formula)
        .map(|c| {
            let mut s = c.preamble;
            s.push_str(&c.body);
            s
        })
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn eq(a: serde_json::Value, b: serde_json::Value) -> serde_json::Value {
        serde_json::json!({"kind": "atomic", "name": "=", "args": [a, b]})
    }
    fn ctor(name: &str, args: Vec<serde_json::Value>) -> serde_json::Value {
        serde_json::json!({"kind": "ctor", "name": name, "args": args})
    }
    fn var(name: &str) -> serde_json::Value {
        serde_json::json!({"kind": "var", "name": name})
    }

    #[test]
    fn mixed_sort_conjunction_is_named_error_not_ill_sorted_script() {
        // H1 [B7]: the same `call:f` ctor equated to a String literal in one
        // row (String-theory regime -> String return sort) and an Int literal
        // in another row (legacy regime -> Int return sort). One declare-fun
        // cannot carry both; emitting would produce an ill-sorted script ->
        // z3 parse error -> OPAQUE undecidable. The compiler must instead
        // return a NAMED error at emit time.
        let subject = ctor("call:f", vec![serde_json::json!(
            {"kind":"const","value":1,"sort":{"kind":"primitive","name":"Int"}})]);
        let string_row = eq(
            subject.clone(),
            serde_json::json!({"kind":"const","value":"abc","sort":{"kind":"primitive","name":"String"}}),
        );
        let int_row = eq(
            subject,
            serde_json::json!({"kind":"const","value":7,"sort":{"kind":"primitive","name":"Int"}}),
        );
        let ir = serde_json::json!({"kind":"and","operands":[string_row, int_row]});

        for result in [compile_to_parts(&ir), compile_asserted_to_parts(&ir)] {
            let err = result.expect_err("mixed-sort conjunction must be refused, not emitted");
            let msg = err.to_string();
            assert!(
                msg.contains("mixed-sort conjunction on call:f"),
                "error must name the conflict and the ctor: {msg}"
            );
            assert!(
                msg.contains("String vs Int"),
                "error must name both regimes: {msg}"
            );
        }
    }

    #[test]
    fn same_regime_conjunction_still_compiles() {
        // Discrimination twin for B7: two rows equating the same ctor to TWO
        // DIFFERENT Int literals are contradictory but NOT mixed-sort -- the
        // conjunction must still compile (the solver, not the emitter, rules
        // on satisfiability). The detector must not over-trigger.
        let mk = |v: i64| {
            eq(
                ctor("call:f", vec![serde_json::json!(
                    {"kind":"const","value":1,"sort":{"kind":"primitive","name":"Int"}})]),
                serde_json::json!({"kind":"const","value":v,"sort":{"kind":"primitive","name":"Int"}}),
            )
        };
        let ir = serde_json::json!({"kind":"and","operands":[mk(7), mk(8)]});
        compile_to_parts(&ir).expect("same-regime conjunction must compile");

        // And the all-String twin: same ctor equated to two String literals.
        let mks = |s: &str| {
            eq(
                ctor("call:g", vec![serde_json::json!(
                    {"kind":"const","value":"x","sort":{"kind":"primitive","name":"String"}})]),
                serde_json::json!({"kind":"const","value":s,"sort":{"kind":"primitive","name":"String"}}),
            )
        };
        let ir2 = serde_json::json!({"kind":"and","operands":[mks("a"), mks("b")]});
        compile_to_parts(&ir2).expect("all-String conjunction must compile");
    }

    #[test]
    fn negated_path_declares_non_builtin_ctors_as_uninterpreted_fns() {
        // The reflexive-discharge encoding: a non-arithmetic ctor head
        // (`Ok`) must be DECLARED as an uninterpreted function on the
        // negated (validity) path, not left undeclared. Before this, the
        // whitelist had to refuse such terms because the negated path could
        // not render them.
        let ir = eq(ctor("Ok", vec![var("x")]), ctor("Ok", vec![var("x")]));
        let parts = compile_to_parts(&ir).expect("compile");
        assert!(
            parts.preamble.contains("(declare-fun Ok ("),
            "Ok must be declared as an uninterpreted fn on the negated path: {}",
            parts.preamble
        );
        // The body asserts the NEGATION (prove validity via unsat).
        assert!(
            parts.body.contains("(assert (not"),
            "negated path must assert (not ...): {}",
            parts.body
        );
    }

    fn atomic(name: &str, args: Vec<serde_json::Value>) -> serde_json::Value {
        serde_json::json!({"kind": "atomic", "name": name, "args": args})
    }
    fn implies(a: serde_json::Value, b: serde_json::Value) -> serde_json::Value {
        serde_json::json!({"kind": "implies", "operands": [a, b]})
    }

    #[test]
    fn non_builtin_atomic_predicate_in_boolean_position_is_declared() {
        // A user predicate (`is_some`) sitting as a boolean atom in an
        // implication -- the panic-freedom guard-discharge shape -- must be
        // declared `(declare-fun is_some (Int) Bool)`. Before the
        // predicate-decl pass it was left undeclared and z3 rejected the
        // script with `unknown constant is_some`. This is the COMPLEMENT of the
        // builtin set, so it is generic and language-blind (no `is_some`
        // special-casing).
        let ir = implies(
            atomic("is_some", vec![var("opt")]),
            atomic("is_some", vec![var("opt")]),
        );
        let parts = compile_to_parts(&ir).expect("compile");
        assert!(
            parts.preamble.contains("(declare-fun is_some (Int) Bool)"),
            "is_some must be declared as a Bool-returning fn: {}",
            parts.preamble
        );
    }

    #[test]
    fn float_refinement_predicate_uses_real_call_result_sort_and_has_unsat_teeth() {
        let z3 = which_z3().expect("z3 required for float refinement predicate check");
        for (predicate, call_name) in [
            ("float.f64.is_nan", "method:div_duration_f64"),
            ("float.f32.is_infinite", "method:div_duration_f32"),
            ("float.f64.is_normal", "method:normal_value_f64"),
            ("float.f64.is_sign_positive", "method:positive_value_f64"),
            ("float.f64.is_sign_negative", "method:negative_value_f64"),
        ] {
            let call = ctor(call_name, vec![]);
            let atom = atomic(predicate, vec![call.clone()]);
            let inv = serde_json::json!({
                "kind": "and",
                "operands": [
                    atom.clone(),
                    {"kind": "not", "operands": [atom]},
                ]
            });
            let parts = compile_asserted_to_parts(&inv).expect("compile");
            assert!(
                parts
                    .preamble
                    .contains(&format!("(declare-fun |{call_name}| () Real)")),
                "float call result must be declared Real: {}",
                parts.preamble
            );
            assert!(
                parts
                    .preamble
                    .contains(&format!("(declare-fun {predicate} (Real) Bool)")),
                "float refinement predicate must accept Real: {}",
                parts.preamble
            );

            let script = format!("{}{}", parts.preamble, parts.body);
            let out = run_z3(&z3, &script);
            assert_eq!(
                out.trim(),
                "unsat",
                "P(call) and not P(call) must be UNSAT, got: {out}\nscript:\n{script}"
            );
        }
    }

    #[test]
    fn builtin_atomic_predicates_are_not_declared() {
        // DISCRIMINATION: builtin/theory predicates (`=`, `<`, ...) must NOT be
        // declared (they are SMT-LIB primitives). Declaring them would be a
        // redefinition error.
        let zero = serde_json::json!({"kind":"const","value":0,
            "sort":{"kind":"primitive","name":"Int"}});
        let ir = implies(
            atomic(">", vec![var("n"), zero]),
            atomic("=", vec![var("n"), var("n")]),
        );
        let parts = compile_to_parts(&ir).expect("compile");
        assert!(
            !parts.preamble.contains("(declare-fun = ")
                && !parts.preamble.contains("(declare-fun > "),
            "builtin predicates must not be declared: {}",
            parts.preamble
        );
    }

    #[test]
    fn guarded_pre_implication_discharges_bare_pre_does_not() {
        // STRUCTURAL end-to-end (panic-freedom soundness). The guard-discharge
        // obligation `(=> (is_some opt) (is_some opt))` is valid -> negation
        // unsat -> discharged (panic-safe). The bare unguarded pre `is_some(opt)`
        // over a free `opt` is NOT valid -> negation sat -> undecidable (the
        // refuse-floor negative control). With the predicate declared, z3 RUNS
        // (no `unknown constant` error) and discriminates the two correctly.
        let Some(z3) = which_z3() else {
            eprintln!("z3 not found; skipping panic-freedom end-to-end check");
            return;
        };

        let guarded = compile_to_parts(&implies(
            atomic("is_some", vec![var("opt")]),
            atomic("is_some", vec![var("opt")]),
        ))
        .expect("compile guarded");
        let g_out = run_z3(&z3, &format!("{}{}", guarded.preamble, guarded.body));
        assert!(
            g_out.contains("unsat"),
            "guarded `(=> is_some(opt) is_some(opt))` must be unsat (discharged): {g_out}"
        );

        let bare = compile_to_parts(&atomic("is_some", vec![var("opt")])).expect("compile bare");
        let b_out = run_z3(&z3, &format!("{}{}", bare.preamble, bare.body));
        assert!(
            b_out.contains("sat") && !b_out.contains("unsat"),
            "bare unguarded `is_some(opt)` must be sat (NOT discharged): {b_out}"
        );
    }

    #[test]
    fn reflexive_equality_is_unsat_under_z3_and_distinct_is_sat() {
        // End-to-end soundness check via z3 IF available. `(= (Ok x) (Ok
        // x))` is valid -> its negation is unsat -> discharged by
        // congruence over an uninterpreted `Ok`. `(= (Ok x) (Err x))` is
        // NOT valid -> its negation is sat -> the encoding does NOT
        // launder a mismatched post. This is the soundness guard for the
        // encoder: reflexivity, not blanket-pass.
        let z3 = which_z3();
        let Some(z3) = z3 else {
            eprintln!("z3 not found; skipping end-to-end congruence check");
            return;
        };

        let reflexive =
            compile_to_parts(&eq(ctor("Ok", vec![var("x")]), ctor("Ok", vec![var("x")])))
                .expect("compile reflexive");
        let r_out = run_z3(&z3, &format!("{}{}", reflexive.preamble, reflexive.body));
        assert!(
            r_out.contains("unsat"),
            "reflexive `Ok(x) == Ok(x)` must be unsat (discharged): {r_out}"
        );

        let distinct =
            compile_to_parts(&eq(ctor("Ok", vec![var("x")]), ctor("Err", vec![var("x")])))
                .expect("compile distinct");
        let d_out = run_z3(&z3, &format!("{}{}", distinct.preamble, distinct.body));
        assert!(
            d_out.contains("sat") && !d_out.contains("unsat"),
            "distinct `Ok(x) == Err(x)` must be sat (NOT discharged): {d_out}"
        );
    }

    fn which_z3() -> Option<String> {
        for cand in [
            "z3",
            "/opt/homebrew/bin/z3",
            "/usr/local/bin/z3",
            "/usr/bin/z3",
        ] {
            if std::process::Command::new(cand)
                .arg("--version")
                .output()
                .map(|o| o.status.success())
                .unwrap_or(false)
            {
                return Some(cand.to_string());
            }
        }
        None
    }

    // ── String-literal encoding tests ─────────────────────────────────────
    // POSITIVE: `=(r,"{"a":1}")` is satisfiable (single consistent assertion).
    // RED before fix: z3 returns a parse error; test asserts no parse error + real verdict.
    // GREEN after fix: real sat/unsat, no `(error ...` output.

    fn string_const(s: &str) -> serde_json::Value {
        serde_json::json!({"kind":"const","value":s,"sort":{"kind":"primitive","name":"String"}})
    }

    #[test]
    fn string_literal_equality_single_no_parse_error() {
        // POSITIVE: `assert r == '{"a":1}'` — a single string-equality assertion.
        // Must compile without error and produce a real sat/unsat, not a parse error.
        let z3 = which_z3().expect("z3 must be present for string-literal soundness check");
        let inv = eq(var("r"), string_const(r#"{"a":1}"#));
        let parts = compile_asserted_to_parts(&inv).expect("compile must succeed");
        let script = format!("{}{}", parts.preamble, parts.body);
        let out = run_z3(&z3, &script);
        assert!(
            !out.contains("(error"),
            "string-literal equality must produce no z3 parse error; got: {out}\nscript:\n{script}"
        );
        assert!(
            out.contains("sat") || out.contains("unsat"),
            "string-literal equality must produce a real sat/unsat verdict; got: {out}"
        );
    }

    #[test]
    fn two_distinct_string_literals_same_var_is_unsat() {
        // DISCRIMINATION: `=(r,"a") ∧ =(r,"b")` with two distinct string literals.
        // A single var cannot equal two different string constants -> UNSAT.
        // RED before fix: parse error -> undecidable (false pass of consistency).
        // GREEN after fix: real unsat verdict.
        let z3 = which_z3().expect("z3 must be present for string-literal soundness check");
        let inv = serde_json::json!({
            "kind": "and",
            "operands": [
                eq(var("r"), string_const("a")),
                eq(var("r"), string_const("b")),
            ]
        });
        let parts = compile_asserted_to_parts(&inv).expect("compile must succeed");
        let script = format!("{}{}", parts.preamble, parts.body);
        let out = run_z3(&z3, &script);
        assert!(
            !out.contains("(error"),
            "two-literal contradiction must produce no parse error; got: {out}\nscript:\n{script}"
        );
        assert_eq!(
            out.trim(),
            "unsat",
            "=(r,'a') ∧ =(r,'b') with distinct literals must be UNSAT (refused); \
             z3 said: {out}\nscript:\n{script}"
        );
    }

    #[test]
    fn string_literal_with_brace_backslash_unicode_no_parse_error() {
        // Weird-char cases: brace, backslash, control char (\x01), unicode (≥).
        // All must compile without parse error and give a real verdict.
        let z3 = which_z3().expect("z3 must be present for string-literal soundness check");
        let weird_cases = vec![
            r#"{"a":"x"}"#,    // braces
            r#"path\to\file"#, // backslashes
            "\x01",            // control char
            "≥ ≤ ≠",           // unicode operators
        ];
        for s in weird_cases {
            let inv = eq(var("r"), string_const(s));
            let parts = compile_asserted_to_parts(&inv).expect("compile must succeed");
            let script = format!("{}{}", parts.preamble, parts.body);
            let out = run_z3(&z3, &script);
            assert!(
                !out.contains("(error"),
                "weird-char literal {:?} must produce no z3 parse error; got: {out}\nscript:\n{script}",
                s
            );
            assert!(
                out.contains("sat") || out.contains("unsat"),
                "weird-char literal {:?} must produce real sat/unsat; got: {out}",
                s
            );
        }
    }

    fn string_theory_atom(name: &str, args: Vec<serde_json::Value>) -> serde_json::Value {
        serde_json::json!({"kind": "atomic", "name": name, "args": args})
    }

    fn str_len(s: &str) -> serde_json::Value {
        serde_json::json!({"kind": "ctor", "name": "str.len", "args": [string_const(s)]})
    }

    #[test]
    fn string_contains_prefix_suffix_route_to_z3_string_theory() {
        let z3 = which_z3().expect("z3 required for string theory check");
        let inv = serde_json::json!({
            "kind": "and",
            "operands": [
                string_theory_atom("contains", vec![string_const("abcde"), string_const("bcd")]),
                string_theory_atom("prefix-of", vec![string_const("ab"), string_const("abcde")]),
                string_theory_atom("suffix-of", vec![string_const("de"), string_const("abcde")]),
            ]
        });
        let parts = compile_asserted_to_parts(&inv).expect("compile");
        let script = format!("{}{}", parts.preamble, parts.body);
        assert!(
            script.contains(r#"(str.contains "abcde" "bcd")"#),
            "contains must lower to z3 string theory, got:\n{script}"
        );
        assert!(
            script.contains(r#"(str.prefixof "ab" "abcde")"#),
            "prefix-of must lower to z3 string theory, got:\n{script}"
        );
        assert!(
            script.contains(r#"(str.suffixof "de" "abcde")"#),
            "suffix-of must lower to z3 string theory, got:\n{script}"
        );
        assert!(
            !script.contains("strlit_"),
            "string-theory atoms must not use opaque equality literals:\n{script}"
        );
        let out = run_z3(&z3, &script);
        assert_eq!(
            out.trim(),
            "sat",
            "true string predicates must be SAT: {out}"
        );
    }

    #[test]
    fn string_theory_bad_twin_is_unsat() {
        let z3 = which_z3().expect("z3 required for string theory check");
        let inv = serde_json::json!({
            "kind": "and",
            "operands": [
                string_theory_atom("contains", vec![string_const("abcde"), string_const("bcd")]),
                {"kind": "not", "operands": [
                    string_theory_atom("contains", vec![string_const("abcde"), string_const("bcd")])
                ]},
            ]
        });
        let parts = compile_asserted_to_parts(&inv).expect("compile");
        let script = format!("{}{}", parts.preamble, parts.body);
        let out = run_z3(&z3, &script);
        assert_eq!(
            out.trim(),
            "unsat",
            "contradictory string predicate twin must be UNSAT, got: {out}\nscript:\n{script}"
        );
    }

    #[test]
    fn string_len_and_ascii_class_predicates_route_to_z3_string_theory() {
        let z3 = which_z3().expect("z3 required for string theory check");
        let inv = serde_json::json!({
            "kind": "and",
            "operands": [
                eq(str_len("～～～～～"), int_const(15)),
                string_theory_atom("str.is_ascii", vec![string_const("banana\0\u{7f}")]),
                string_theory_atom("str.is_ascii_alphabetic", vec![string_const("A")]),
                string_theory_atom("str.is_ascii_alphanumeric", vec![string_const("A")]),
                string_theory_atom("str.is_ascii_digit", vec![string_const("9")]),
                string_theory_atom("str.is_ascii_octdigit", vec![string_const("7")]),
                string_theory_atom("str.is_ascii_lowercase", vec![string_const("z")]),
                string_theory_atom("str.is_ascii_uppercase", vec![string_const("Z")]),
                string_theory_atom("str.is_ascii_hexdigit", vec![string_const("f")]),
                string_theory_atom("str.is_ascii_punctuation", vec![string_const("!")]),
                string_theory_atom("str.is_ascii_graphic", vec![string_const("~")]),
                string_theory_atom("str.is_ascii_whitespace", vec![string_const(" ")]),
                string_theory_atom("str.is_ascii_control", vec![string_const("\u{7f}")]),
                {"kind": "not", "operands": [
                    string_theory_atom("str.is_ascii_alphabetic", vec![string_const("0")])
                ]},
            ]
        });
        let parts = compile_asserted_to_parts(&inv).expect("compile");
        let script = format!("{}{}", parts.preamble, parts.body);
        assert!(
            script.contains("(str.len \"～～～～～\")"),
            "str.len must lower to z3 string length, got:\n{script}"
        );
        assert!(
            script.contains("str.in_re"),
            "ascii predicates must lower to z3 regex string checks, got:\n{script}"
        );
        assert!(
            script.contains("(re.range \"0\" \"9\")"),
            "digit regex missing: {script}"
        );
        assert!(
            script.contains("(re.range \"0\" \"7\")"),
            "octdigit regex missing: {script}"
        );
        assert!(
            script.contains("(re.range \"a\" \"z\")"),
            "lowercase regex missing: {script}"
        );
        assert!(
            script.contains("(re.range \"A\" \"Z\")"),
            "uppercase regex missing: {script}"
        );
        assert!(
            script.contains("(re.range \"!\" \"/\")"),
            "punctuation regex missing: {script}"
        );
        assert!(
            script.contains("(re.range \"!\" \"~\")"),
            "graphic regex missing: {script}"
        );
        assert!(
            script.contains("(re.range \"\\u{0}\" \"\\u{1f}\")"),
            "control regex missing: {script}"
        );
        assert!(
            script.contains("(re.union"),
            "union-based ascii class regex missing: {script}"
        );
        let out = run_z3(&z3, &script);
        assert_eq!(
            out.trim(),
            "sat",
            "true len/ascii predicate set must be SAT, got: {out}\nscript:\n{script}"
        );
    }

    // ── G1: str.chars-in-set (universe membership over a walked charset) ──
    // The Java kit's universe pass emits `str.chars-in-set(subject, set)`
    // where `set` is the encode table walked from the vendor's static-final
    // AST. Lowering: (str.in_re subject (re.* (re.union (str.to_re "c") ...))).

    /// The G1 conjoin subject: a `callresult_*` ctor with a string-literal
    /// arg, exactly the shape the Java kit's `buildUniverseContract` /
    /// `buildStringContract` emit over the same `#euf#` contract name.
    fn callresult(name: &str, str_arg: &str) -> serde_json::Value {
        ctor(name, vec![string_const(str_arg)])
    }

    #[test]
    fn chars_in_set_with_in_set_literal_is_sat() {
        // POSITIVE: universe row + the vendor's sworn equality over the SAME
        // callresult subject is consistent. "Zm9v" over a base64-table-shaped
        // set — the GOOD-suite conjoin.
        let z3 = which_z3().expect("z3 required for chars-in-set check");
        let call = callresult("c:callresult_encodeBase64String_a1", "foo");
        let inv = serde_json::json!({
            "kind": "and",
            "operands": [
                string_theory_atom(
                    "str.chars-in-set",
                    vec![call.clone(), string_const("ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/=")],
                ),
                eq(call, string_const("Zm9v")),
            ]
        });
        let parts = compile_asserted_to_parts(&inv).expect("compile");
        let script = format!("{}{}", parts.preamble, parts.body);
        assert!(
            script.contains("(re.* (re.union (str.to_re \"+\")"),
            "chars-in-set must lower to re.* over str.to_re union (sorted+deduped):\n{script}"
        );
        assert!(
            script.contains("(Int) String)"),
            "the callresult subject must be declared with String return sort:\n{script}"
        );
        assert!(
            script.contains("\"Zm9v\""),
            "the string-routed equality must render a real String literal:\n{script}"
        );
        let out = run_z3(&z3, &script);
        assert_eq!(
            out.trim(),
            "sat",
            "universe row + in-set sworn literal must be SAT, got: {out}\nscript:\n{script}"
        );
    }

    #[test]
    fn chars_in_set_with_out_of_set_char_is_unsat() {
        // DISCRIMINATION: the BAD-twin shape. Universe = the URL_SAFE table
        // (no '+', no '/'); the consumer's claimed equality over the same
        // callresult contains '+' and '/'. The conjunction must be UNSAT —
        // z3 string theory refutes an input the vendor never tested.
        let z3 = which_z3().expect("z3 required for chars-in-set check");
        let call = callresult("c:callresult_encodeBase64URLSafeString_a1", "bar");
        let inv = serde_json::json!({
            "kind": "and",
            "operands": [
                string_theory_atom(
                    "str.chars-in-set",
                    vec![call.clone(), string_const("ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_")],
                ),
                eq(call, string_const("YmFy+/x=")),
            ]
        });
        let parts = compile_asserted_to_parts(&inv).expect("compile");
        let script = format!("{}{}", parts.preamble, parts.body);
        let out = run_z3(&z3, &script);
        assert_eq!(
            out.trim(),
            "unsat",
            "universe row + out-of-set literal must be UNSAT, got: {out}\nscript:\n{script}"
        );
    }

    #[test]
    fn string_routed_equality_does_not_capture_legacy_var_regime() {
        // GUARD: the legacy Python opaque-Int regime is byte-identical. A
        // free-VAR equality (`r == "a"`) must NOT route to string theory —
        // cross-type consistency (`r == "a" ∧ r == 1` UNSAT via distinctness)
        // depends on it. Same for `None == "x"`.
        let var_eq = compile_asserted_to_parts(&eq(var("r"), string_const("a"))).expect("compile");
        let script = format!("{}{}", var_eq.preamble, var_eq.body);
        assert!(
            script.contains("strlit_") && script.contains("(declare-const r Int)"),
            "var equality must stay in the opaque-Int regime:\n{script}"
        );
        let none_eq = compile_asserted_to_parts(&eq(
            serde_json::json!({"kind":"ctor","name":"None","args":[]}),
            string_const("x"),
        ))
        .expect("compile");
        let script = format!("{}{}", none_eq.preamble, none_eq.body);
        assert!(
            script.contains("strlit_"),
            "None equality must stay in the opaque-Int regime:\n{script}"
        );
    }

    #[test]
    fn chars_in_set_round_trips_through_emit_asserted() {
        // STRUCTURAL: the predicate survives the asserted path end-to-end —
        // no opaque strlit_ laundering of the set, no undeclared-predicate
        // fallback, and the empty string is in the Kleene-star universe (SAT
        // alone). Quote char in the set must be escaped SMT-style ("" not \").
        let inv = string_theory_atom("str.chars-in-set", vec![var("r"), string_const("a\"b")]);
        let parts = compile_asserted_to_parts(&inv).expect("compile");
        let script = format!("{}{}", parts.preamble, parts.body);
        assert!(
            script.contains("(str.in_re r (re.* (re.union (str.to_re \"\"\"\") (re.union (str.to_re \"a\") (str.to_re \"b\")))))"),
            "round-trip rendering wrong:\n{script}"
        );
        assert!(
            !script.contains("strlit_"),
            "the set must lower to string theory, not opaque literals:\n{script}"
        );
        assert!(
            !script.contains("(declare-fun str.chars-in-set")
                && !script.contains("(declare-fun |str.chars-in-set|"),
            "chars-in-set must be a theory lowering, not an uninterpreted predicate:\n{script}"
        );
        if let Some(z3) = which_z3() {
            let out = run_z3(&z3, &script);
            assert_eq!(out.trim(), "sat", "lone universe row must be SAT: {out}");
        }
    }

    // ── Cross-type literal distinctness (Python `==` semantics) ───────────
    // Helpers for int / bool / None literal terms.
    fn int_const(n: i64) -> serde_json::Value {
        serde_json::json!({"kind":"const","value":n,"sort":{"kind":"primitive","name":"Int"}})
    }
    fn bool_const(b: bool) -> serde_json::Value {
        serde_json::json!({"kind":"const","value":b,"sort":{"kind":"primitive","name":"Bool"}})
    }
    fn none_ctor() -> serde_json::Value {
        serde_json::json!({"kind":"ctor","name":"None","args":[]})
    }
    fn and2(a: serde_json::Value, b: serde_json::Value) -> serde_json::Value {
        serde_json::json!({"kind":"and","operands":[a,b]})
    }

    #[test]
    fn str_literal_distinct_from_int_literal_is_unsat() {
        // Python: `"5" != 5`. `r == "5" ∧ r == 5` is contradictory -> UNSAT.
        // RED before fix: both collapse into Int universe with no distinctness
        // axiom -> z3 picks strlit == 5 -> SAT (false consistent).
        let z3 = which_z3().expect("z3 required");
        let inv = and2(eq(var("r"), string_const("5")), eq(var("r"), int_const(5)));
        let parts = compile_asserted_to_parts(&inv).expect("compile");
        let script = format!("{}{}", parts.preamble, parts.body);
        let out = run_z3(&z3, &script);
        assert_eq!(
            out.trim(),
            "unsat",
            "`r==\"5\" ∧ r==5` must be UNSAT (Python str≠int); got: {out}\nscript:\n{script}"
        );
    }

    #[test]
    fn none_distinct_from_int_literal_is_unsat() {
        // Python: `None != 5`. `r is None ∧ r == 5` is contradictory -> UNSAT.
        let z3 = which_z3().expect("z3 required");
        let inv = and2(eq(var("r"), none_ctor()), eq(var("r"), int_const(5)));
        let parts = compile_asserted_to_parts(&inv).expect("compile");
        let script = format!("{}{}", parts.preamble, parts.body);
        let out = run_z3(&z3, &script);
        assert_eq!(
            out.trim(),
            "unsat",
            "`r is None ∧ r==5` must be UNSAT (Python None≠int); got: {out}\nscript:\n{script}"
        );
    }

    #[test]
    fn none_distinct_from_str_literal_is_unsat() {
        // Python: `None != "x"`. `r is None ∧ r == "x"` is contradictory -> UNSAT.
        let z3 = which_z3().expect("z3 required");
        let inv = and2(eq(var("r"), none_ctor()), eq(var("r"), string_const("x")));
        let parts = compile_asserted_to_parts(&inv).expect("compile");
        let script = format!("{}{}", parts.preamble, parts.body);
        let out = run_z3(&z3, &script);
        assert_eq!(
            out.trim(),
            "unsat",
            "`r is None ∧ r==\"x\"` must be UNSAT (Python None≠str); got: {out}\nscript:\n{script}"
        );
    }

    #[test]
    fn none_distinct_from_bool_false_is_unsat() {
        // Python: `None != False` (and False==0). `r is None ∧ r == False`
        // is contradictory -> UNSAT. This is the discriminating test for the
        // "bool must join the concrete-int distinctness target set" wiring:
        // False encodes as 0, and None must be distinct from 0.
        let z3 = which_z3().expect("z3 required");
        let inv = and2(eq(var("r"), none_ctor()), eq(var("r"), bool_const(false)));
        let parts = compile_asserted_to_parts(&inv).expect("compile");
        let script = format!("{}{}", parts.preamble, parts.body);
        let out = run_z3(&z3, &script);
        assert_eq!(
            out.trim(),
            "unsat",
            "`r is None ∧ r==False` must be UNSAT (Python None≠False); got: {out}\nscript:\n{script}"
        );
    }

    #[test]
    fn bool_true_consistent_with_int_one_is_sat() {
        // Python: `True == 1`. `r == True ∧ r == 1` is CONSISTENT -> SAT.
        // This is the OVER-DISTINCTNESS GUARD: bool literals must encode to
        // their int values (True->1) and must NOT be asserted distinct from
        // int. A false-refusal here would mean over-distinctness. Permanent.
        let z3 = which_z3().expect("z3 required");
        let inv = and2(eq(var("r"), bool_const(true)), eq(var("r"), int_const(1)));
        let parts = compile_asserted_to_parts(&inv).expect("compile");
        let script = format!("{}{}", parts.preamble, parts.body);
        let out = run_z3(&z3, &script);
        assert!(
            !out.contains("(error"),
            "`r==True ∧ r==1` must not parse-error; got: {out}\nscript:\n{script}"
        );
        assert_eq!(
            out.trim(),
            "sat",
            "`r==True ∧ r==1` must be SAT (Python True==1); got: {out}\nscript:\n{script}"
        );
    }

    #[test]
    fn bool_false_consistent_with_int_zero_is_sat() {
        // Python: `False == 0`. `r == False ∧ r == 0` is CONSISTENT -> SAT.
        let z3 = which_z3().expect("z3 required");
        let inv = and2(eq(var("r"), bool_const(false)), eq(var("r"), int_const(0)));
        let parts = compile_asserted_to_parts(&inv).expect("compile");
        let script = format!("{}{}", parts.preamble, parts.body);
        let out = run_z3(&z3, &script);
        assert_eq!(
            out.trim(),
            "sat",
            "`r==False ∧ r==0` must be SAT (Python False==0); got: {out}\nscript:\n{script}"
        );
    }

    fn run_z3(z3: &str, script: &str) -> String {
        use std::io::Write;
        let mut child = std::process::Command::new(z3)
            .args(["-smt2", "-in"])
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .spawn()
            .expect("spawn z3");
        child
            .stdin
            .as_mut()
            .unwrap()
            .write_all(script.as_bytes())
            .unwrap();
        let out = child.wait_with_output().unwrap();
        String::from_utf8_lossy(&out.stdout).to_string()
    }

    #[test]
    fn functionsort_quantifier_emits_opacity_entry() {
        // forall (f: Function) . true: FunctionSort in quantifier.
        // After fix: the quantifier is emitted soundly over a CID-derived
        // uninterpreted sort instead of collapsing to `true`.
        let ir = serde_json::json!({
            "kind": "forall",
            "name": "f",
            "sort": { "kind": "function", "args": [], "return": { "kind": "primitive", "name": "Bool" } },
            "body": { "kind": "atomic", "name": "true", "args": [] }
        });
        let result = compile_to_parts(&ir).expect("compile succeeds");
        assert_eq!(result.opacity_manifest.opacities.len(), 1);
        assert_eq!(
            result.opacity_manifest.opacities[0].reason_code,
            "predicate_quantification"
        );
        // Sound encoding: the body now contains a real quantifier, not `true`.
        assert!(
            result.body.contains("(assert (not (forall ((f S_"),
            "must emit sound quantifier, got: {}",
            result.body
        );
        assert!(
            !result.body.contains("(assert (not true))"),
            "must not collapse quantifier to true: {}",
            result.body
        );
        // The opaque sort is declared in the preamble, not emitted raw.
        assert!(
            result.preamble.contains("(declare-sort S_"),
            "opaque sort must be declared in preamble: {}",
            result.preamble
        );
    }

    #[test]
    fn dependent_sort_quantifier_emits_opacity_entry() {
        // exists (n: Dependent) . true: DependentSort in quantifier.
        // After fix: emitted soundly over a CID-derived uninterpreted sort.
        let ir = serde_json::json!({
            "kind": "exists",
            "name": "n",
            "sort": { "kind": "dependent", "name": "Vec<n>", "indexVar": "n", "indexSort": { "kind": "primitive", "name": "Int" } },
            "body": { "kind": "atomic", "name": "true", "args": [] }
        });
        let result = compile_to_parts(&ir).expect("compile succeeds");
        assert_eq!(result.opacity_manifest.opacities.len(), 1);
        assert_eq!(
            result.opacity_manifest.opacities[0].reason_code,
            "dependent_type"
        );
        // Sound encoding: real quantifier, not collapsed to `true`.
        assert!(
            result.body.contains("(assert (not (exists ((n S_"),
            "must emit sound existential quantifier, got: {}",
            result.body
        );
        assert!(
            !result.body.contains("(assert (not true))"),
            "must not collapse quantifier to true: {}",
            result.body
        );
    }

    #[test]
    fn primitive_sort_quantifier_no_opacity() {
        // forall (x: Int) . x >= 0: Int is supported
        let ir = serde_json::json!({
            "kind": "forall",
            "name": "x",
            "sort": { "kind": "primitive", "name": "Int" },
            "body": { "kind": "atomic", "name": ">=", "args": [
                { "kind": "var", "name": "x" },
                { "kind": "const", "value": 0, "sort": { "kind": "primitive", "name": "Int" } }
            ]}
        });
        let result = compile_to_parts(&ir).expect("compile succeeds");
        assert!(result.opacity_manifest.opacities.is_empty());
        assert!(result.body.contains("(forall ((x Int))"));
    }

    #[test]
    fn opaque_primitive_sort_quantifier_emits_opacity_entry() {
        // Rust source sorts such as `Ref<Connection>` are valid IR
        // primitive-sort labels for identity, but not SMT-LIB builtin sorts.
        // The SMT backend must not emit them raw.
        // After fix: emitted soundly over a CID-derived uninterpreted sort.
        let ir = serde_json::json!({
            "kind": "forall",
            "name": "conn",
            "sort": { "kind": "primitive", "name": "Ref<Connection>" },
            "body": { "kind": "atomic", "name": "true", "args": [] }
        });
        let result = compile_to_parts(&ir).expect("compile succeeds");
        assert_eq!(result.opacity_manifest.opacities.len(), 1);
        assert_eq!(
            result.opacity_manifest.opacities[0].reason_code,
            "opaque_primitive_sort:Ref<Connection>"
        );
        assert!(
            !result.body.contains("Ref<Connection>"),
            "opaque Rust source sort must not be emitted as raw SMT-LIB: {}",
            result.body
        );
        // Sound encoding: real quantifier over CID-derived sort, not `true`.
        assert!(
            result.body.contains("(assert (not (forall ((conn S_"),
            "must emit sound quantifier, got: {}",
            result.body
        );
        assert!(
            !result.body.contains("(assert (not true))"),
            "must not collapse quantifier to true: {}",
            result.body
        );
        // The opaque sort is declared in the preamble via (declare-sort S_... 0).
        assert!(
            result.preamble.contains("(declare-sort S_"),
            "opaque sort must be declared in preamble: {}",
            result.preamble
        );
    }

    #[test]
    fn opacity_manifest_has_correct_envelope() {
        let ir = serde_json::json!({
            "kind": "forall",
            "name": "f",
            "sort": { "kind": "function", "args": [], "return": { "kind": "primitive", "name": "Bool" } },
            "body": { "kind": "atomic", "name": "true", "args": [] }
        });
        let result = compile_to_parts(&ir).expect("compile succeeds");
        let manifest = &result.opacity_manifest;
        assert_eq!(manifest.protocol_version, "ir-compiler-protocol/2");
        assert_eq!(manifest.compiler, "smt-lib-v2.6");
        assert!(!manifest.compiler_version.is_empty());
    }

    #[test]
    fn opacity_entries_sorted_by_position_cid() {
        // Two quantifiers over opaque sorts: entries should be sorted.
        let ir = serde_json::json!({
            "kind": "and",
            "operands": [
                { "kind": "forall", "name": "f",
                  "sort": { "kind": "function", "args": [], "return": { "kind": "primitive", "name": "Bool" } },
                  "body": { "kind": "atomic", "name": "true", "args": [] } },
                { "kind": "exists", "name": "n",
                  "sort": { "kind": "dependent", "name": "Vec<n>", "indexVar": "n", "indexSort": { "kind": "primitive", "name": "Int" } },
                  "body": { "kind": "atomic", "name": "true", "args": [] } }
            ]
        });
        let result = compile_to_parts(&ir).expect("compile succeeds");
        assert_eq!(result.opacity_manifest.opacities.len(), 2);
        let cids: Vec<&str> = result
            .opacity_manifest
            .opacities
            .iter()
            .map(|e| e.position_cid.as_str())
            .collect();
        assert!(
            cids[0] <= cids[1],
            "opacities must be sorted by positionCid"
        );
    }

    // ACCEPTANCE DETECTOR (issue #1717 soundness fix):
    //
    // Positive:       forall x:opaque. true  -> DISCHARGED (negation is unsat)
    // Discrimination: forall x:opaque. false -> NOT DISCHARGED (negation is sat)
    //
    // Before the fix both collapsed to `true`, making the negation
    // `(assert (not true))` which z3 returns `unsat` for -- a false pass.
    // After the fix the negated `false` case emits:
    //   `(assert (not (forall ((x S_...)) false)))` == `(assert (exists ((x S_...)) true))`
    // Over a nonempty uninterpreted sort, z3 returns `sat` -> not discharged.
    // The `true` body case emits:
    //   `(assert (not (forall ((x S_...)) true)))` == `(assert (exists ((x S_...)) false))`
    // which z3 returns `unsat` -> discharged. Both are correct.
    //
    // This test HARD-FAILS if z3 is not present (no skip): a skipped solver
    // test is a false green (per product invariant falsePass=0).

    #[test]
    fn opaque_sort_forall_true_is_discharged_under_z3() {
        // POSITIVE case: `forall x:opaque. true` must be discharged (negation unsat).
        let z3 = which_z3().expect(
            "z3 must be available for opaque-sort soundness check; \
             install z3 and re-run (a missing z3 is a false green)",
        );
        let ir = serde_json::json!({
            "kind": "forall",
            "name": "x",
            "sort": { "kind": "primitive", "name": "OpaqueT" },
            "body": { "kind": "atomic", "name": "true", "args": [] }
        });
        let result = compile_to_parts(&ir).expect("compile succeeds");
        // Sanity: check the sound quantifier is present in the body.
        assert!(
            result.body.contains("(forall ((x S_"),
            "must emit real quantifier over opaque sort, got: {}",
            result.body
        );
        let script = format!("{}{}", result.preamble, result.body);
        let out = run_z3(&z3, &script);
        assert_eq!(
            out.trim(),
            "unsat",
            "forall x:opaque. true must be discharged (negation unsat); z3 said: {}",
            out
        );
    }

    #[test]
    fn opaque_sort_forall_false_is_not_discharged_under_z3() {
        // DISCRIMINATION case: `forall x:opaque. false` must NOT be discharged
        // (negation sat). Before fix this falsely returned `unsat` (false pass).
        let z3 = which_z3().expect(
            "z3 must be available for opaque-sort soundness check; \
             install z3 and re-run (a missing z3 is a false green)",
        );
        let ir = serde_json::json!({
            "kind": "forall",
            "name": "x",
            "sort": { "kind": "primitive", "name": "OpaqueT" },
            "body": { "kind": "atomic", "name": "false", "args": [] }
        });
        let result = compile_to_parts(&ir).expect("compile succeeds");
        // Sanity: the body must contain the real quantifier, not collapsed true.
        assert!(
            result.body.contains("(forall ((x S_"),
            "must emit real quantifier over opaque sort, got: {}",
            result.body
        );
        assert!(
            !result.body.contains("(assert (not true))"),
            "quantifier must not have been collapsed to true: {}",
            result.body
        );
        let script = format!("{}{}", result.preamble, result.body);
        let out = run_z3(&z3, &script);
        assert_eq!(
            out.trim(),
            "sat",
            "forall x:opaque. false must NOT be discharged (negation must be sat); \
             z3 said: {} -- this is the false-pass soundness hole from issue #1717",
            out
        );
    }
}
