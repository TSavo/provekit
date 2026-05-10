// SPDX-License-Identifier: Apache-2.0
//
// ORP v0.2 compile-mode realizer for ProofIR C11 term algebra to C11 source.

use std::path::Path;

use provekit_ir_compiler::{
    Capabilities, CompileError, CompiledFormula, FreeVar, IrCompiler, OpacityManifest,
    PROTOCOL_VERSION,
};
use serde_json::Value as Json;

mod generated;

pub use generated::{ALGEBRA_TO_C_TABLE, CORE_OPERATION_SUBSET};

pub const DIALECT: &str = "c:c11";
pub const COMPILER_NAME: &str = "c11-source-core";
pub const COMPILER_VERSION: &str = env!("CARGO_PKG_VERSION");

pub trait TermCompiler {
    fn compile_term_json(&self, ir: &Json) -> Result<String, CompileError>;
}

#[derive(Debug, Clone, Default)]
pub struct CCompiler;

#[derive(Debug, Default)]
struct VarSets {
    params: Vec<String>,
    assigned: Vec<String>,
}

struct Emitter {
    lines: Vec<String>,
    loop_depth: usize,
}

impl CCompiler {
    pub fn new() -> Self {
        Self
    }
}

impl TermCompiler for CCompiler {
    fn compile_term_json(&self, ir: &Json) -> Result<String, CompileError> {
        compile_c(ir)
    }
}

impl IrCompiler for CCompiler {
    fn compile(&self, ir: &Json, dialect: &str) -> Result<CompiledFormula, CompileError> {
        if dialect != DIALECT {
            return Err(CompileError::UnsupportedDialect(dialect.to_string()));
        }

        Ok(CompiledFormula {
            preamble: String::new(),
            body: compile_c(ir)?,
            free_vars: Vec::<FreeVar>::new(),
            opacity_manifest: OpacityManifest {
                protocol_version: "ir-compiler-protocol/2".to_string(),
                compiler: DIALECT.to_string(),
                compiler_version: COMPILER_VERSION.to_string(),
                opacities: vec![],
            },
        })
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
                "Ptr".to_string(),
                "LValue".to_string(),
                "Stmt".to_string(),
                "Unit".to_string(),
            ],
            supported_predicates: CORE_OPERATION_SUBSET
                .iter()
                .map(|op| (*op).to_string())
                .collect(),
        }
    }
}

pub fn compile_c(input: &Json) -> Result<String, CompileError> {
    let term = term_payload(input)?;
    let function_name = function_name(input);

    let mut vars = VarSets::default();
    collect_vars(term, &mut vars)?;

    let params = vars.params;
    let locals = vars
        .assigned
        .into_iter()
        .filter(|name| !params.iter().any(|param| param == name))
        .collect::<Vec<_>>();

    let mut lines = Vec::new();
    lines.push("#include <stdint.h>".to_string());
    lines.push(String::new());
    lines.push(format!("int {}({}) {{", function_name, param_list(&params)));

    for local in &locals {
        lines.push(format!("    int {local} = 0;"));
    }

    let mut emitter = Emitter::new();
    if is_statement_term(term) {
        emitter.emit_stmt(term, 1)?;
        lines.extend(emitter.lines);
        if !stmt_always_returns(term) {
            lines.push("    return (0);".to_string());
        }
    } else {
        let expr = emit_expr(term)?;
        lines.push(format!("    return ({});", expr));
    }

    lines.push("}".to_string());
    lines.push(String::new());
    Ok(lines.join("\n"))
}

impl Emitter {
    fn new() -> Self {
        Self {
            lines: Vec::new(),
            loop_depth: 0,
        }
    }

    fn emit_stmt(&mut self, term: &Json, indent: usize) -> Result<(), CompileError> {
        match term_kind(term)? {
            "unit" => {
                self.line(indent, ";");
                Ok(())
            }
            "var" | "const" => {
                let expr = emit_expr(term)?;
                self.line(indent, format!("({expr});"));
                Ok(())
            }
            "op" => match op_name(term)? {
                "seq" => {
                    let args = op_args(term)?;
                    expect_arity("seq", args, 2)?;
                    self.emit_stmt(&args[0], indent)?;
                    self.emit_stmt(&args[1], indent)
                }
                "if" => {
                    let args = op_args(term)?;
                    expect_arity("if", args, 3)?;
                    let cond = emit_condition(&args[0])?;
                    self.line(indent, format!("if {cond} {{"));
                    self.emit_stmt(&args[1], indent + 1)?;
                    self.line(indent, "} else {");
                    self.emit_stmt(&args[2], indent + 1)?;
                    self.line(indent, "}");
                    Ok(())
                }
                "while" => {
                    let args = op_args(term)?;
                    expect_arity("while", args, 2)?;
                    let cond = emit_condition(&args[0])?;
                    self.line(indent, format!("while {cond} {{"));
                    self.loop_depth += 1;
                    let body_result = self.emit_stmt(&args[1], indent + 1);
                    self.loop_depth -= 1;
                    body_result?;
                    self.line(indent, "}");
                    Ok(())
                }
                "return" => {
                    let args = op_args(term)?;
                    expect_arity("return", args, 1)?;
                    let expr = emit_expr(&args[0])?;
                    self.line(indent, format!("return ({});", expr));
                    Ok(())
                }
                "call" => {
                    let expr = emit_call(term)?;
                    self.line(indent, format!("{expr};"));
                    Ok(())
                }
                "break" => {
                    expect_unit_arg("break", op_args(term)?)?;
                    if self.loop_depth == 0 {
                        return Err(malformed("break outside loop"));
                    }
                    self.line(indent, "break;");
                    Ok(())
                }
                "continue" => {
                    expect_unit_arg("continue", op_args(term)?)?;
                    if self.loop_depth == 0 {
                        return Err(malformed("continue outside loop"));
                    }
                    self.line(indent, "continue;");
                    Ok(())
                }
                "skip" => {
                    expect_unit_arg("skip", op_args(term)?)?;
                    self.line(indent, ";");
                    Ok(())
                }
                "assign" => {
                    let args = op_args(term)?;
                    expect_arity("assign", args, 2)?;
                    let target = emit_lvalue(&args[0])?;
                    let value = emit_expr(&args[1])?;
                    self.line(indent, format!("{target} = ({value});"));
                    Ok(())
                }
                name if is_expr_op(name) => {
                    let expr = emit_expr(term)?;
                    self.line(indent, format!("({expr});"));
                    Ok(())
                }
                name => Err(unsupported(name)),
            },
            other => Err(malformed(format!("unknown statement kind: {other}"))),
        }
    }

    fn line(&mut self, indent: usize, text: impl AsRef<str>) {
        self.lines
            .push(format!("{}{}", "    ".repeat(indent), text.as_ref()));
    }
}

fn emit_expr(term: &Json) -> Result<String, CompileError> {
    match term_kind(term)? {
        "var" => c_identifier(var_name(term)?).map(str::to_string),
        "const" => const_literal(term),
        "op" => {
            let name = op_name(term)?;
            let args = op_args(term)?;
            match name {
                "eq" => emit_binary(args, "=="),
                "lt" => emit_binary(args, "<"),
                "le" => emit_binary(args, "<="),
                "add" => emit_binary(args, "+"),
                "sub" => emit_binary(args, "-"),
                "mul" => emit_binary(args, "*"),
                "and" => emit_binary(args, "&&"),
                "or" => emit_binary(args, "||"),
                "neg" => {
                    expect_arity(name, args, 1)?;
                    Ok(format!("(-{})", operand(&emit_expr(&args[0])?)))
                }
                "not" => {
                    expect_arity(name, args, 1)?;
                    Ok(format!("(!{})", operand(&emit_expr(&args[0])?)))
                }
                "deref" => {
                    expect_arity(name, args, 1)?;
                    Ok(format!("(*{})", operand(&emit_expr(&args[0])?)))
                }
                "assign" => {
                    expect_arity(name, args, 2)?;
                    let target = emit_lvalue(&args[0])?;
                    let value = emit_expr(&args[1])?;
                    Ok(format!("({target} = ({value}))"))
                }
                "call" => emit_call(term),
                "if" => {
                    expect_arity(name, args, 3)?;
                    if is_statement_term(&args[1]) || is_statement_term(&args[2]) {
                        return Err(malformed("if expression branches must be expressions"));
                    }
                    let cond = emit_expr(&args[0])?;
                    let then_branch = emit_expr(&args[1])?;
                    let else_branch = emit_expr(&args[2])?;
                    Ok(format!("(({cond}) ? ({then_branch}) : ({else_branch}))"))
                }
                name => Err(unsupported(name)),
            }
        }
        "unit" => Err(malformed("unit has no expression value")),
        other => Err(malformed(format!("unknown expression kind: {other}"))),
    }
}

fn emit_binary(args: &[Json], op: &str) -> Result<String, CompileError> {
    expect_arity(op, args, 2)?;
    let lhs = emit_expr(&args[0])?;
    let rhs = emit_expr(&args[1])?;
    Ok(format!("({} {} {})", operand(&lhs), op, operand(&rhs)))
}

fn operand(expr: &str) -> String {
    if expr.starts_with('(') && expr.ends_with(')') {
        expr.to_string()
    } else {
        format!("({expr})")
    }
}

fn emit_condition(term: &Json) -> Result<String, CompileError> {
    let expr = emit_expr(term)?;
    if expr.starts_with('(') && expr.ends_with(')') {
        Ok(expr)
    } else {
        Ok(format!("({expr})"))
    }
}

fn emit_lvalue(term: &Json) -> Result<String, CompileError> {
    match term_kind(term)? {
        "var" => c_identifier(var_name(term)?).map(str::to_string),
        "op" if op_name(term)? == "deref" => {
            let args = op_args(term)?;
            expect_arity("deref", args, 1)?;
            Ok(format!("*({})", emit_expr(&args[0])?))
        }
        _ => Err(unsupported("assign target")),
    }
}

fn emit_call(term: &Json) -> Result<String, CompileError> {
    let args = op_args(term)?;
    if args.is_empty() {
        return Err(malformed("call expects callee plus arguments"));
    }
    let callee = callee_name(&args[0])?;
    let call_args = call_arguments(args)?;
    let rendered_args = call_args
        .iter()
        .map(|arg| emit_expr(arg))
        .collect::<Result<Vec<_>, _>>()?
        .join(", ");
    Ok(format!("{callee}({rendered_args})"))
}

fn collect_vars(term: &Json, vars: &mut VarSets) -> Result<(), CompileError> {
    match term_kind(term)? {
        "var" => push_unique(&mut vars.params, c_identifier(var_name(term)?)?),
        "const" | "unit" => {}
        "op" => {
            let name = op_name(term)?;
            let args = op_args(term)?;
            if name == "call" {
                for arg in call_arguments(args)? {
                    collect_vars(arg, vars)?;
                }
                return Ok(());
            }
            if name == "assign" {
                expect_arity(name, args, 2)?;
                if term_kind(&args[0])? == "var" {
                    push_unique(&mut vars.assigned, c_identifier(var_name(&args[0])?)?);
                    collect_vars(&args[1], vars)?;
                    return Ok(());
                }
            }
            for arg in args {
                collect_vars(arg, vars)?;
            }
        }
        "ctor" => {
            if let Some(args) = term.get("args").and_then(Json::as_array) {
                for arg in args {
                    collect_vars(arg, vars)?;
                }
            }
        }
        other => return Err(malformed(format!("unknown term kind: {other}"))),
    }
    Ok(())
}

fn term_payload(input: &Json) -> Result<&Json, CompileError> {
    match input.get("kind").and_then(Json::as_str) {
        Some("c11-algebra-term") => input
            .get("term")
            .ok_or_else(|| malformed("c11 algebra term envelope missing term")),
        _ => Ok(input),
    }
}

fn function_name(input: &Json) -> String {
    for key in ["function", "function_name", "fn_name", "name"] {
        if let Some(name) = input.get(key).and_then(Json::as_str) {
            return sanitize_symbol(name);
        }
    }
    if let Some(source) = input.get("source").and_then(Json::as_str) {
        if let Some(stem) = Path::new(source).file_stem().and_then(|stem| stem.to_str()) {
            return sanitize_symbol(stem);
        }
    }
    "proofir_term".to_string()
}

fn sanitize_symbol(raw: &str) -> String {
    let mut out = String::new();
    for (index, ch) in raw.chars().enumerate() {
        let valid = ch.is_ascii_alphanumeric() || ch == '_';
        if valid && !(index == 0 && ch.is_ascii_digit()) {
            out.push(ch);
        } else if valid {
            out.push('_');
            out.push(ch);
        } else {
            out.push('_');
        }
    }
    if out.is_empty() {
        "proofir_term".to_string()
    } else {
        out
    }
}

fn param_list(params: &[String]) -> String {
    if params.is_empty() {
        return "void".to_string();
    }
    params
        .iter()
        .map(|name| format!("int {name}"))
        .collect::<Vec<_>>()
        .join(", ")
}

fn call_arguments(args: &[Json]) -> Result<Vec<&Json>, CompileError> {
    match args {
        [_callee] => Ok(Vec::new()),
        [_callee, maybe_list] if term_kind(maybe_list)? == "ctor" => Ok(maybe_list
            .get("args")
            .and_then(Json::as_array)
            .map(|items| items.iter().collect())
            .unwrap_or_default()),
        [_callee, maybe_unit] if is_unit(maybe_unit) => Ok(Vec::new()),
        [_callee, rest @ ..] => Ok(rest.iter().collect()),
        [] => Err(malformed("call expects callee plus arguments")),
    }
}

fn callee_name(term: &Json) -> Result<String, CompileError> {
    match term_kind(term)? {
        "var" => c_identifier(var_name(term)?).map(str::to_string),
        "const" => term
            .get("value")
            .and_then(Json::as_str)
            .ok_or_else(|| malformed("call callee const must be a string"))
            .and_then(|name| c_identifier(name).map(str::to_string)),
        "ctor" => term
            .get("name")
            .and_then(Json::as_str)
            .ok_or_else(|| malformed("call callee ctor missing name"))
            .and_then(|name| c_identifier(name).map(str::to_string)),
        other => Err(malformed(format!("unsupported callee kind {other}"))),
    }
}

fn const_literal(term: &Json) -> Result<String, CompileError> {
    let value = term
        .get("value")
        .ok_or_else(|| malformed("const missing value"))?;
    if let Some(i) = value.as_i64() {
        return Ok(i.to_string());
    }
    if let Some(u) = value.as_u64() {
        return Ok(u.to_string());
    }
    if let Some(b) = value.as_bool() {
        return Ok(i32::from(b).to_string());
    }
    Err(CompileError::UnsupportedSort(
        "C11 core compiler supports only integer and bool constants".to_string(),
    ))
}

fn is_statement_term(term: &Json) -> bool {
    term.get("kind")
        .and_then(Json::as_str)
        .is_some_and(|kind| match kind {
            "unit" => true,
            "op" => match term.get("name").and_then(Json::as_str) {
                Some("if") => term
                    .get("args")
                    .and_then(Json::as_array)
                    .is_some_and(|args| {
                        args.len() == 3
                            && (is_statement_term(&args[1]) || is_statement_term(&args[2]))
                    }),
                Some(name) => is_statement_op(name),
                None => false,
            },
            _ => false,
        })
}

fn is_statement_op(name: &str) -> bool {
    matches!(
        name,
        "seq" | "while" | "return" | "break" | "continue" | "skip" | "assign"
    )
}

fn is_expr_op(name: &str) -> bool {
    matches!(
        name,
        "eq" | "lt"
            | "le"
            | "add"
            | "sub"
            | "mul"
            | "neg"
            | "and"
            | "or"
            | "not"
            | "deref"
            | "call"
            | "if"
    )
}

fn stmt_always_returns(term: &Json) -> bool {
    match term.get("kind").and_then(Json::as_str) {
        Some("op") => match term.get("name").and_then(Json::as_str) {
            Some("return") => true,
            Some("seq") => term
                .get("args")
                .and_then(Json::as_array)
                .is_some_and(|args| args.iter().any(stmt_always_returns)),
            Some("if") => term
                .get("args")
                .and_then(Json::as_array)
                .is_some_and(|args| {
                    args.len() == 3
                        && stmt_always_returns(&args[1])
                        && stmt_always_returns(&args[2])
                }),
            _ => false,
        },
        _ => false,
    }
}

fn term_kind(term: &Json) -> Result<&str, CompileError> {
    term.get("kind")
        .and_then(Json::as_str)
        .ok_or_else(|| malformed("term missing kind"))
}

fn op_name(term: &Json) -> Result<&str, CompileError> {
    term.get("name")
        .and_then(Json::as_str)
        .ok_or_else(|| malformed("op missing name"))
}

fn op_args(term: &Json) -> Result<&[Json], CompileError> {
    term.get("args")
        .and_then(Json::as_array)
        .map(Vec::as_slice)
        .ok_or_else(|| malformed("op missing args array"))
}

fn var_name(term: &Json) -> Result<&str, CompileError> {
    term.get("name")
        .and_then(Json::as_str)
        .ok_or_else(|| malformed("var missing name"))
}

fn expect_arity(op: &str, args: &[Json], expected: usize) -> Result<(), CompileError> {
    if args.len() == expected {
        Ok(())
    } else {
        Err(malformed(format!(
            "{op} expects {expected} arguments, got {}",
            args.len()
        )))
    }
}

fn expect_unit_arg(op: &str, args: &[Json]) -> Result<(), CompileError> {
    match args {
        [] => Ok(()),
        [arg] if is_unit(arg) => Ok(()),
        _ => Err(malformed(format!("{op} expects one unit argument"))),
    }
}

fn is_unit(term: &Json) -> bool {
    term.get("kind").and_then(Json::as_str) == Some("unit")
        || (term.get("kind").and_then(Json::as_str) == Some("op")
            && term.get("name").and_then(Json::as_str) == Some("skip"))
}

fn c_identifier(raw: &str) -> Result<&str, CompileError> {
    let mut chars = raw.chars();
    let Some(first) = chars.next() else {
        return Err(malformed("empty identifier"));
    };
    if !(first.is_ascii_alphabetic() || first == '_') {
        return Err(malformed(format!("unsupported C identifier: {raw}")));
    }
    if !chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_') {
        return Err(malformed(format!("unsupported C identifier: {raw}")));
    }
    Ok(raw)
}

fn push_unique(vec: &mut Vec<String>, name: &str) {
    if !vec.iter().any(|existing| existing == name) {
        vec.push(name.to_string());
    }
}

fn unsupported(name: &str) -> CompileError {
    CompileError::UnsupportedPredicate(name.to_string())
}

fn malformed(message: impl Into<String>) -> CompileError {
    CompileError::MalformedIr(message.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn default_function_name_uses_source_stem() {
        let ir = json!({
            "kind": "c11-algebra-term",
            "source": "path/to/foo.c",
            "term": {"kind": "op", "name": "skip", "args": [{"kind": "unit"}]}
        });

        let c_source = compile_c(&ir).expect("compile");

        assert!(c_source.contains("int foo(void) {"));
    }

    #[test]
    fn invalid_variable_names_are_refused() {
        let ir = json!({
            "kind": "var",
            "name": "x-y"
        });

        let err = compile_c(&ir).expect_err("invalid identifier");

        assert!(err.to_string().contains("unsupported C identifier"));
    }

    #[test]
    fn capabilities_include_core_ops() {
        let caps = CCompiler::new().capabilities();
        for op in CORE_OPERATION_SUBSET {
            assert!(caps.supported_predicates.iter().any(|item| item == op));
        }
    }
}
