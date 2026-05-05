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
pub mod opacity;

pub use opacity::{
    manifest_for_formula, OpacityEntry, OpacityManifest, OPACITY_PROTOCOL_VERSION,
    REASON_DEPENDENT_TYPE, REASON_PREDICATE_QUANTIFICATION,
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
    let kind = ir_formula.get("kind").and_then(|v| v.as_str()).unwrap_or("");
    if is_term_kind(kind) {
        let term: provekit_ir_types::Term = serde_json::from_value(ir_formula.clone())
            .map_err(|e| format!("{e}"))?;
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
    compile_to_parts_with_manifest(ir_formula).map(|(c, _)| c)
}

/// Compile + emit the OpacityManifest declaring positions SMT-LIB
/// cannot soundly translate (FunctionSort, DependentSort).
///
/// The `CompiledFormula` portion is byte-identical to what
/// `compile_to_parts` returns; the manifest is the additional v2
/// emission required by `2026-05-02-ir-compiler-protocol-v2.md`.
///
/// We intentionally do NOT plumb the manifest through the
/// `IrCompiler` trait (which targets `provekit-ir-compiler/1`); v2
/// transport is a separate protocol identifier and the manifest is
/// surfaced via this dedicated entry point until the trait grows a
/// v2 surface.
pub fn compile_to_parts_with_manifest(
    ir_formula: &Json,
) -> Result<(CompiledFormula, OpacityManifest), CompileError> {
    let formula: provekit_ir_types::Formula = serde_json::from_value(ir_formula.clone())
        .map_err(|e| CompileError::MalformedIr(e.to_string().into()))?;
    validate_formula(&formula).map_err(|e| CompileError::MalformedIr(e.into()))?;
    let manifest = manifest_for_formula(&formula);
    Ok((generated::compile_formula(&formula), manifest))
}


