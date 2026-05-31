// SPDX-License-Identifier: Apache-2.0
//
// Bundled SMT-LIB v2.6 IR compiler. Extracted from the inline
// provekit-verifier::smt_emitter so the same code serves both the
// in-process fast path (verifier deps directly on this crate) and the
// standalone subprocess binary `provekit-ir-smt-lib`.
//
// Spec: protocol/specs/2026-04-30-ir-compiler-protocol.md.

use serde_json::Value as Json;

use provekit_ir_compiler::{
    Capabilities, CompileError, CompiledFormula, IrCompiler, PROTOCOL_VERSION,
};

mod generated;

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
fn validate_term(term: &provekit_ir_types::IrTerm) -> Result<(), String> {
    match term {
        provekit_ir_types::IrTerm::Var { name } => {
            if name.is_empty() {
                return Err("var name must not be empty".to_string());
            }
            Ok(())
        }
        provekit_ir_types::IrTerm::Const { .. } => Ok(()),
        provekit_ir_types::IrTerm::Ctor { args, .. } => args.iter().try_for_each(validate_term),
        provekit_ir_types::IrTerm::Lambda { body, .. } => validate_term(body),
        provekit_ir_types::IrTerm::Let { body, .. } => validate_term(body),
    }
}

fn validate_formula(formula: &provekit_ir_types::IrFormula) -> Result<(), String> {
    match formula {
        provekit_ir_types::IrFormula::Atomic { args, .. } => {
            args.iter().try_for_each(validate_term)
        }
        provekit_ir_types::IrFormula::And { operands }
        | provekit_ir_types::IrFormula::Or { operands }
        | provekit_ir_types::IrFormula::Not { operands }
        | provekit_ir_types::IrFormula::Implies { operands } => {
            operands.iter().try_for_each(validate_formula)
        }
        provekit_ir_types::IrFormula::Forall { body, .. }
        | provekit_ir_types::IrFormula::Exists { body, .. }
        | provekit_ir_types::IrFormula::Choice { body, .. } => validate_formula(body),
        // wp-rule schema nodes (spec 2026-05-13-wp-as-formula.md §2.3):
        // `substitute` / `apply` appear only inside an unreduced `wp_rule`
        // term; `libprovekit::wp` eliminates them before any formula reaches
        // the SMT-LIB backend. Reaching this arm means a `wp_rule` schema was
        // handed to the solver without instantiation.
        provekit_ir_types::IrFormula::Substitute { .. }
        | provekit_ir_types::IrFormula::Apply { .. } => Err(
            "wp-rule schema node (substitute/apply) reached the SMT-LIB validator; \
             it must be reduced via libprovekit::wp before solving"
                .to_string(),
        ),
        provekit_ir_types::IrFormula::DivergenceBetween { .. } => Err(
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
        let term: provekit_ir_types::Term =
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
    let formula: provekit_ir_types::Formula = serde_json::from_value(ir_formula.clone())
        .map_err(|e| CompileError::MalformedIr(e.to_string()))?;
    validate_formula(&formula).map_err(CompileError::MalformedIr)?;
    Ok(generated::compile_formula(&formula))
}

pub fn compile_asserted_to_parts(ir_formula: &Json) -> Result<CompiledFormula, CompileError> {
    let formula: provekit_ir_types::Formula = serde_json::from_value(ir_formula.clone())
        .map_err(|e| CompileError::MalformedIr(e.to_string()))?;
    validate_formula(&formula).map_err(CompileError::MalformedIr)?;
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

        let reflexive = compile_to_parts(&eq(ctor("Ok", vec![var("x")]), ctor("Ok", vec![var("x")])))
            .expect("compile reflexive");
        let r_out = run_z3(&z3, &format!("{}{}", reflexive.preamble, reflexive.body));
        assert!(
            r_out.contains("unsat"),
            "reflexive `Ok(x) == Ok(x)` must be unsat (discharged): {r_out}"
        );

        let distinct = compile_to_parts(&eq(ctor("Ok", vec![var("x")]), ctor("Err", vec![var("x")])))
            .expect("compile distinct");
        let d_out = run_z3(&z3, &format!("{}{}", distinct.preamble, distinct.body));
        assert!(
            d_out.contains("sat") && !d_out.contains("unsat"),
            "distinct `Ok(x) == Err(x)` must be sat (NOT discharged): {d_out}"
        );
    }

    fn which_z3() -> Option<String> {
        for cand in ["z3", "/opt/homebrew/bin/z3", "/usr/local/bin/z3", "/usr/bin/z3"] {
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
        // forall (f: Function) . true: FunctionSort in quantifier
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
        assert!(result.body.contains("(assert (not true))"));
        assert!(!result.body.contains("(true)"));
    }

    #[test]
    fn dependent_sort_quantifier_emits_opacity_entry() {
        // exists (n: Dependent) . true: DependentSort in quantifier
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
        assert!(result.body.contains("(assert (not true))"));
        assert!(!result.body.contains("(true)"));
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
        assert!(result.body.contains("(assert (not true))"));
        assert!(!result.body.contains("(true)"));
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
}
