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

/// Legacy single-string entry point. Equal to `preamble + body` from
/// `compile_to_parts`. Also accepts bare terms (lambda, let, etc.) for
/// backward compatibility with the historical verifier emitter.
pub fn emit(ir_formula: &Json) -> Result<String, String> {
    let kind = ir_formula.get("kind").and_then(|v| v.as_str()).unwrap_or("");
    if is_term_kind(kind) {
        let term: provekit_ir_types::Term = serde_json::from_value(ir_formula.clone())
            .map_err(|e| format!("{e}"))?;
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
    Ok(generated::compile_formula(&formula))
}


