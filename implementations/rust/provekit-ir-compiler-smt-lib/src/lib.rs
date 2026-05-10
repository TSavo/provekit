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
        .map_err(|e| CompileError::MalformedIr(e.to_string().into()))?;
    validate_formula(&formula).map_err(|e| CompileError::MalformedIr(e.into()))?;
    Ok(generated::compile_formula(&formula))
}

pub fn compile_asserted_to_parts(ir_formula: &Json) -> Result<CompiledFormula, CompileError> {
    let formula: provekit_ir_types::Formula = serde_json::from_value(ir_formula.clone())
        .map_err(|e| CompileError::MalformedIr(e.to_string().into()))?;
    validate_formula(&formula).map_err(|e| CompileError::MalformedIr(e.into()))?;
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

    #[test]
    fn functionsort_quantifier_emits_opacity_entry() {
        // forall (f: Function) . true — FunctionSort in quantifier
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
        assert!(result.body.contains("(true)"));
    }

    #[test]
    fn dependent_sort_quantifier_emits_opacity_entry() {
        // exists (n: Dependent) . true — DependentSort in quantifier
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
        assert!(result.body.contains("(true)"));
    }

    #[test]
    fn primitive_sort_quantifier_no_opacity() {
        // forall (x: Int) . x >= 0 — Int is supported
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
        // Two quantifiers over opaque sorts — entries should be sorted.
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
