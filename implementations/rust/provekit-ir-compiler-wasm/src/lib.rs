// SPDX-License-Identifier: Apache-2.0
//
// provekit-ir-compiler-wasm: ORP v0.2 compile-mode realizer for the
// ProofIR term stratum. It lowers the core C11 operation-CID term subset
// to WebAssembly WAT text. The full operation set is mint-more-realizations,
// not new machinery: each added operation is another homomorphic table entry
// plus its proof or receipt.

use std::collections::BTreeSet;
use std::path::Path;

use provekit_ir_compiler::{
    Capabilities, CompileError, CompiledFormula, IrCompiler, OpacityManifest, PROTOCOL_VERSION,
};
use serde_json::Value as Json;

mod generated;

pub use generated::ALGEBRA_TO_WASM_TABLE;

pub const DIALECT: &str = "wasm-wat";
pub const COMPILER_NAME: &str = "wasm-wat-reference";
pub const COMPILER_VERSION: &str = env!("CARGO_PKG_VERSION");

const SUPPORTED_PREDICATES: &[&str] = &[
    "seq",
    "if",
    "while",
    "return",
    "call",
    "break",
    "continue",
    "skip",
    "eq",
    "lt",
    "le",
    "add",
    "sub",
    "mul",
    "neg",
    "and",
    "or",
    "not",
    "deref",
    "assign",
    "bop_eq",
    "bop_ne",
    "bop_lt",
    "bop_le",
    "bop_gt",
    "bop_ge",
    "bop_add",
    "bop_sub",
    "bop_mul",
    "bop_div",
    "bop_mod",
    "bop_shl",
    "bop_shr",
    "bop_bitand",
    "bop_bitor",
    "bop_bitxor",
    "bop_logand",
    "bop_logor",
    "bop_comma",
    "uop_neg",
    "uop_lognot",
    "uop_deref",
    "uop_bitnot",
    "uop_plus",
];

pub struct WasmCompiler;

#[derive(Default)]
struct VarSets {
    reads: BTreeSet<String>,
    assigned_locals: BTreeSet<String>,
}

#[derive(Clone)]
struct LoopLabels {
    break_label: String,
    continue_label: String,
}

struct EmitContext {
    lines: Vec<String>,
    loop_stack: Vec<LoopLabels>,
    next_loop: usize,
}

impl EmitContext {
    fn new() -> Self {
        Self {
            lines: Vec::new(),
            loop_stack: Vec::new(),
            next_loop: 0,
        }
    }

    fn line(&mut self, indent: usize, text: impl AsRef<str>) {
        let mut line = String::with_capacity(indent * 2 + text.as_ref().len());
        line.push_str(&"  ".repeat(indent));
        line.push_str(text.as_ref());
        self.lines.push(line);
    }

    fn loop_labels(&self, op: &str) -> Result<&LoopLabels, CompileError> {
        self.loop_stack
            .last()
            .ok_or_else(|| malformed(format!("{op} outside while")))
    }
}

#[derive(Debug, Clone)]
pub struct WasmModule {
    pub wat: String,
}

impl WasmCompiler {
    pub fn new() -> Self {
        Self
    }

    pub fn compile_term(&self, input: &Json) -> Result<String, CompileError> {
        compile_wat(input)
    }
}

impl Default for WasmCompiler {
    fn default() -> Self {
        Self::new()
    }
}

pub fn compile_wat(input: &Json) -> Result<String, CompileError> {
    let term = term_payload(input)?;
    let function_name = function_name(input)?;

    let mut vars = VarSets::default();
    collect_vars(term, &mut vars)?;
    let params = vars.reads;
    let locals = vars
        .assigned_locals
        .difference(&params)
        .cloned()
        .collect::<BTreeSet<_>>();
    let needs_memory = needs_memory(term)?;

    let mut body = EmitContext::new();
    if is_statement_term(term) {
        emit_stmt(term, &mut body, 2)?;
        if !stmt_always_returns(term) {
            body.line(2, "i32.const 0");
        }
    } else {
        emit_expr(term, &mut body, 2)?;
    }

    let mut lines = Vec::new();
    lines.push("(module".to_string());
    if needs_memory {
        lines.push("  (memory (export \"memory\") 1)".to_string());
    }
    lines.push(func_header(&function_name, &params));
    for local in &locals {
        lines.push(format!("    (local ${local} i32)"));
    }
    lines.extend(body.lines);
    lines.push("  )".to_string());
    lines.push(")".to_string());
    lines.push(String::new());

    Ok(lines.join("\n"))
}

impl IrCompiler for WasmCompiler {
    fn compile(&self, ir: &Json, dialect: &str) -> Result<CompiledFormula, CompileError> {
        if dialect != DIALECT {
            return Err(CompileError::UnsupportedDialect(dialect.to_string()));
        }

        Ok(CompiledFormula {
            preamble: String::new(),
            body: compile_wat(ir)?,
            free_vars: Vec::new(),
            opacity_manifest: OpacityManifest {
                protocol_version: "ir-compiler-protocol/2".to_string(),
                compiler: DIALECT.to_string(),
                compiler_version: COMPILER_VERSION.to_string(),
                opacities: Vec::new(),
            },
        })
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities {
            name: COMPILER_NAME.to_string(),
            version: COMPILER_VERSION.to_string(),
            protocol_version: PROTOCOL_VERSION.to_string(),
            dialects: vec![DIALECT.to_string()],
            supported_sorts: vec!["Int".to_string(), "Bool".to_string(), "Addr".to_string()],
            supported_predicates: SUPPORTED_PREDICATES
                .iter()
                .map(|name| (*name).to_string())
                .collect(),
        }
    }
}

fn term_payload(input: &Json) -> Result<&Json, CompileError> {
    match input.get("kind").and_then(Json::as_str) {
        Some("c11-algebra-term") => input
            .get("term")
            .ok_or_else(|| malformed("c11-algebra-term missing term")),
        _ => Ok(input),
    }
}

fn function_name(input: &Json) -> Result<String, CompileError> {
    let raw = input
        .get("function")
        .or_else(|| input.get("name"))
        .and_then(Json::as_str)
        .map(str::to_string)
        .or_else(|| {
            input
                .get("source")
                .and_then(Json::as_str)
                .and_then(|source| Path::new(source).file_stem())
                .and_then(|stem| stem.to_str())
                .map(str::to_string)
        })
        .unwrap_or_else(|| "main".to_string());
    sanitize_identifier(&raw)
}

fn func_header(function_name: &str, params: &BTreeSet<String>) -> String {
    let mut header = format!("  (func ${function_name} (export \"{function_name}\")");
    for param in params {
        header.push_str(&format!(" (param ${param} i32)"));
    }
    header.push_str(" (result i32)");
    header
}

fn collect_vars(term: &Json, vars: &mut VarSets) -> Result<(), CompileError> {
    match term.get("kind").and_then(Json::as_str) {
        Some("var") => {
            let name = name_field(term)?;
            vars.reads.insert(sanitize_identifier(name)?);
            Ok(())
        }
        Some("const") | Some("unit") => Ok(()),
        Some("op") => {
            let name = name_field(term)?;
            let args = args_field(term)?;
            if name == "call" {
                for arg in args.iter().skip(1) {
                    collect_vars(arg, vars)?;
                }
                return Ok(());
            }
            if name == "assign" {
                expect_arity(name, args, 2)?;
                if let Some(target) = local_assign_target(&args[0])? {
                    vars.assigned_locals.insert(target);
                    collect_vars(&args[1], vars)?;
                    return Ok(());
                }
            }
            for arg in args {
                collect_vars(arg, vars)?;
            }
            Ok(())
        }
        Some(other) => Err(malformed(format!("unknown term kind: {other}"))),
        None => Err(malformed("term missing kind")),
    }
}

fn needs_memory(term: &Json) -> Result<bool, CompileError> {
    match term.get("kind").and_then(Json::as_str) {
        Some("op") => {
            let name = name_field(term)?;
            let args = args_field(term)?;
            if is_deref_op(name) {
                return Ok(true);
            }
            if name == "assign" {
                expect_arity(name, args, 2)?;
                if local_assign_target(&args[0])?.is_none() {
                    return Ok(true);
                }
            }
            args.iter().try_fold(false, |seen, arg| {
                Ok::<_, CompileError>(seen || needs_memory(arg)?)
            })
        }
        Some("var" | "const" | "unit") => Ok(false),
        Some(other) => Err(malformed(format!("unknown term kind: {other}"))),
        None => Err(malformed("term missing kind")),
    }
}

fn emit_stmt(term: &Json, ctx: &mut EmitContext, indent: usize) -> Result<(), CompileError> {
    match term.get("kind").and_then(Json::as_str) {
        Some("unit") => Ok(()),
        Some("op") => {
            let name = name_field(term)?;
            let args = args_field(term)?;
            match name {
                "seq" => {
                    for arg in args {
                        emit_stmt(arg, ctx, indent)?;
                    }
                    Ok(())
                }
                "if" => {
                    expect_arity(name, args, 3)?;
                    emit_expr(&args[0], ctx, indent)?;
                    ctx.line(indent, "if");
                    emit_stmt(&args[1], ctx, indent + 1)?;
                    ctx.line(indent, "else");
                    emit_stmt(&args[2], ctx, indent + 1)?;
                    ctx.line(indent, "end");
                    Ok(())
                }
                "while" => {
                    expect_arity(name, args, 2)?;
                    let loop_id = ctx.next_loop;
                    ctx.next_loop += 1;
                    let labels = LoopLabels {
                        break_label: format!("break{loop_id}"),
                        continue_label: format!("continue{loop_id}"),
                    };
                    ctx.line(indent, format!("block ${}", labels.break_label));
                    ctx.line(indent + 1, format!("loop ${}", labels.continue_label));
                    ctx.loop_stack.push(labels);
                    emit_expr(&args[0], ctx, indent + 2)?;
                    let active = ctx
                        .loop_stack
                        .last()
                        .ok_or_else(|| malformed("missing active loop"))?
                        .clone();
                    ctx.line(indent + 2, "i32.eqz");
                    ctx.line(indent + 2, format!("br_if ${}", active.break_label));
                    emit_stmt(&args[1], ctx, indent + 2)?;
                    ctx.line(indent + 2, format!("br ${}", active.continue_label));
                    ctx.loop_stack.pop();
                    ctx.line(indent + 1, "end");
                    ctx.line(indent, "end");
                    Ok(())
                }
                "return" => {
                    expect_arity(name, args, 1)?;
                    emit_expr(&args[0], ctx, indent)?;
                    ctx.line(indent, "return");
                    Ok(())
                }
                "break" => emit_branch("break", args, ctx, indent),
                "continue" => emit_branch("continue", args, ctx, indent),
                "skip" => Ok(()),
                "assign" => emit_assign(args, ctx, indent),
                _ if is_expr_op(name) => {
                    emit_expr(term, ctx, indent)?;
                    ctx.line(indent, "drop");
                    Ok(())
                }
                _ => Err(unsupported(name)),
            }
        }
        Some("var" | "const") => {
            emit_expr(term, ctx, indent)?;
            ctx.line(indent, "drop");
            Ok(())
        }
        Some(other) => Err(malformed(format!("unknown statement kind: {other}"))),
        None => Err(malformed("statement missing kind")),
    }
}

fn emit_expr(term: &Json, ctx: &mut EmitContext, indent: usize) -> Result<(), CompileError> {
    match term.get("kind").and_then(Json::as_str) {
        Some("var") => {
            let name = sanitize_identifier(name_field(term)?)?;
            ctx.line(indent, format!("local.get ${name}"));
            Ok(())
        }
        Some("const") => {
            let value = const_i32(term)?;
            ctx.line(indent, format!("i32.const {value}"));
            Ok(())
        }
        Some("op") => {
            let name = name_field(term)?;
            let args = args_field(term)?;
            match name {
                "bop_logand" => emit_logical_and(args, ctx, indent),
                "bop_logor" => emit_logical_or(args, ctx, indent),
                name if binary_wasm_opcode(name).is_some() => emit_binary(
                    args,
                    ctx,
                    indent,
                    binary_wasm_opcode(name).expect("matched"),
                ),
                "bop_comma" => {
                    expect_arity(name, args, 2)?;
                    emit_expr(&args[0], ctx, indent)?;
                    ctx.line(indent, "drop");
                    emit_expr(&args[1], ctx, indent)
                }
                "neg" | "uop_neg" => {
                    expect_arity(name, args, 1)?;
                    ctx.line(indent, "i32.const 0");
                    emit_expr(&args[0], ctx, indent)?;
                    ctx.line(indent, "i32.sub");
                    Ok(())
                }
                "not" | "uop_lognot" => {
                    expect_arity(name, args, 1)?;
                    emit_expr(&args[0], ctx, indent)?;
                    ctx.line(indent, "i32.eqz");
                    Ok(())
                }
                "uop_bitnot" => {
                    expect_arity(name, args, 1)?;
                    emit_expr(&args[0], ctx, indent)?;
                    ctx.line(indent, "i32.const -1");
                    ctx.line(indent, "i32.xor");
                    Ok(())
                }
                "uop_plus" => {
                    expect_arity(name, args, 1)?;
                    emit_expr(&args[0], ctx, indent)
                }
                name if is_deref_op(name) => {
                    expect_arity(name, args, 1)?;
                    emit_expr(&args[0], ctx, indent)?;
                    ctx.line(indent, "i32.load");
                    Ok(())
                }
                "call" => {
                    let (callee, call_args) = call_parts(term)?;
                    for arg in call_args {
                        emit_expr(arg, ctx, indent)?;
                    }
                    ctx.line(indent, format!("call ${callee}"));
                    Ok(())
                }
                "if" => {
                    expect_arity(name, args, 3)?;
                    if is_statement_term(&args[1]) || is_statement_term(&args[2]) {
                        return Err(malformed("if expression branches must be expression terms"));
                    }
                    emit_expr(&args[0], ctx, indent)?;
                    ctx.line(indent, "if (result i32)");
                    emit_expr(&args[1], ctx, indent + 1)?;
                    ctx.line(indent, "else");
                    emit_expr(&args[2], ctx, indent + 1)?;
                    ctx.line(indent, "end");
                    Ok(())
                }
                _ => Err(unsupported(name)),
            }
        }
        Some("unit") => Err(malformed("unit has no i32 expression value")),
        Some(other) => Err(malformed(format!("unknown expression kind: {other}"))),
        None => Err(malformed("expression missing kind")),
    }
}

fn emit_binary(
    args: &[Json],
    ctx: &mut EmitContext,
    indent: usize,
    op: &str,
) -> Result<(), CompileError> {
    expect_arity(op, args, 2)?;
    emit_expr(&args[0], ctx, indent)?;
    emit_expr(&args[1], ctx, indent)?;
    ctx.line(indent, op);
    Ok(())
}

fn emit_logical_and(
    args: &[Json],
    ctx: &mut EmitContext,
    indent: usize,
) -> Result<(), CompileError> {
    expect_arity("bop_logand", args, 2)?;
    emit_expr(&args[0], ctx, indent)?;
    ctx.line(indent, "i32.eqz");
    ctx.line(indent, "if (result i32)");
    ctx.line(indent + 1, "i32.const 0");
    ctx.line(indent, "else");
    emit_expr(&args[1], ctx, indent + 1)?;
    emit_to_bool(ctx, indent + 1);
    ctx.line(indent, "end");
    Ok(())
}

fn emit_logical_or(
    args: &[Json],
    ctx: &mut EmitContext,
    indent: usize,
) -> Result<(), CompileError> {
    expect_arity("bop_logor", args, 2)?;
    emit_expr(&args[0], ctx, indent)?;
    ctx.line(indent, "if (result i32)");
    ctx.line(indent + 1, "i32.const 1");
    ctx.line(indent, "else");
    emit_expr(&args[1], ctx, indent + 1)?;
    emit_to_bool(ctx, indent + 1);
    ctx.line(indent, "end");
    Ok(())
}

fn emit_to_bool(ctx: &mut EmitContext, indent: usize) {
    ctx.line(indent, "i32.eqz");
    ctx.line(indent, "i32.eqz");
}

fn emit_assign(args: &[Json], ctx: &mut EmitContext, indent: usize) -> Result<(), CompileError> {
    expect_arity("assign", args, 2)?;
    if let Some(target) = local_assign_target(&args[0])? {
        emit_expr(&args[1], ctx, indent)?;
        ctx.line(indent, format!("local.set ${target}"));
        return Ok(());
    }

    let address = store_address_term(&args[0])?;
    emit_expr(address, ctx, indent)?;
    emit_expr(&args[1], ctx, indent)?;
    ctx.line(indent, "i32.store");
    Ok(())
}

fn emit_branch(
    op: &str,
    args: &[Json],
    ctx: &mut EmitContext,
    indent: usize,
) -> Result<(), CompileError> {
    if args.len() > 1 {
        return Err(malformed(format!("{op} expects zero or one argument")));
    }
    let labels = ctx.loop_labels(op)?.clone();
    let target = if op == "break" {
        labels.break_label
    } else {
        labels.continue_label
    };
    if let Some(condition) = args.first() {
        emit_expr(condition, ctx, indent)?;
        ctx.line(indent, format!("br_if ${target}"));
    } else {
        ctx.line(indent, format!("br ${target}"));
    }
    Ok(())
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
    binary_wasm_opcode(name).is_some()
        || matches!(
            name,
            "bop_comma"
                | "bop_logand"
                | "bop_logor"
                | "neg"
                | "uop_neg"
                | "not"
                | "uop_lognot"
                | "uop_bitnot"
                | "uop_plus"
                | "deref"
                | "uop_deref"
                | "call"
                | "if"
        )
}

fn is_deref_op(name: &str) -> bool {
    matches!(name, "deref" | "uop_deref")
}

fn binary_wasm_opcode(name: &str) -> Option<&'static str> {
    match name {
        "eq" | "bop_eq" => Some("i32.eq"),
        "bop_ne" => Some("i32.ne"),
        "lt" | "bop_lt" => Some("i32.lt_s"),
        "le" | "bop_le" => Some("i32.le_s"),
        "bop_gt" => Some("i32.gt_s"),
        "bop_ge" => Some("i32.ge_s"),
        "add" | "bop_add" => Some("i32.add"),
        "sub" | "bop_sub" => Some("i32.sub"),
        "mul" | "bop_mul" => Some("i32.mul"),
        "bop_div" => Some("i32.div_s"),
        "bop_mod" => Some("i32.rem_s"),
        "bop_shl" => Some("i32.shl"),
        "bop_shr" => Some("i32.shr_s"),
        "and" | "bop_bitand" => Some("i32.and"),
        "or" | "bop_bitor" => Some("i32.or"),
        "bop_bitxor" => Some("i32.xor"),
        _ => None,
    }
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

fn call_parts(term: &Json) -> Result<(String, &[Json]), CompileError> {
    let args = args_field(term)?;
    if args.is_empty() {
        return Err(malformed("call expects callee plus zero or more arguments"));
    }
    let callee = match args[0].get("kind").and_then(Json::as_str) {
        Some("var") => name_field(&args[0])?,
        Some("const") => args[0]
            .get("value")
            .and_then(Json::as_str)
            .ok_or_else(|| malformed("call callee const must be a string"))?,
        _ => {
            return Err(malformed(
                "call callee must be a var or string const in this subset",
            ))
        }
    };
    Ok((sanitize_identifier(callee)?, &args[1..]))
}

fn store_address_term(target: &Json) -> Result<&Json, CompileError> {
    if target.get("kind").and_then(Json::as_str) == Some("op")
        && target
            .get("name")
            .and_then(Json::as_str)
            .is_some_and(is_deref_op)
    {
        let args = args_field(target)?;
        expect_arity("deref", args, 1)?;
        Ok(&args[0])
    } else {
        Ok(target)
    }
}

fn local_assign_target(target: &Json) -> Result<Option<String>, CompileError> {
    if target.get("kind").and_then(Json::as_str) == Some("var") {
        Ok(Some(sanitize_identifier(name_field(target)?)?))
    } else {
        Ok(None)
    }
}

fn const_i32(term: &Json) -> Result<i32, CompileError> {
    let value = term
        .get("value")
        .ok_or_else(|| malformed("const missing value"))?;
    if let Some(i) = value.as_i64() {
        return i32::try_from(i).map_err(|_| malformed(format!("const out of i32 range: {i}")));
    }
    if let Some(u) = value.as_u64() {
        return i32::try_from(u).map_err(|_| malformed(format!("const out of i32 range: {u}")));
    }
    if let Some(b) = value.as_bool() {
        return Ok(i32::from(b));
    }
    Err(CompileError::UnsupportedSort(
        "WASM MVP subset supports only i32 integer and bool constants".to_string(),
    ))
}

fn args_field(term: &Json) -> Result<&[Json], CompileError> {
    term.get("args")
        .and_then(Json::as_array)
        .map(Vec::as_slice)
        .ok_or_else(|| malformed("op missing args array"))
}

fn name_field(term: &Json) -> Result<&str, CompileError> {
    term.get("name")
        .and_then(Json::as_str)
        .ok_or_else(|| malformed("term missing name"))
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

fn sanitize_identifier(raw: &str) -> Result<String, CompileError> {
    if raw.is_empty() {
        return Err(malformed("empty identifier"));
    }
    if raw
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '.' | '$'))
    {
        Ok(raw.to_string())
    } else {
        Err(malformed(format!("unsupported identifier: {raw}")))
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
    fn top_level_expression_leaves_i32_result() {
        let input = json!({
            "function": "expr",
            "kind": "op",
            "name": "add",
            "args": [
                {"kind": "const", "value": 1, "sort": {"kind": "ctor", "name": "Int", "args": []}},
                {"kind": "const", "value": 2, "sort": {"kind": "ctor", "name": "Int", "args": []}}
            ]
        });

        let wat = compile_wat(&input).unwrap();

        assert!(wat.contains("i32.add"));
        assert!(!wat.contains("drop"));
    }

    #[test]
    fn local_assignment_uses_local_set() {
        let input = json!({
            "function": "locals",
            "kind": "op",
            "name": "seq",
            "args": [
                {
                    "kind": "op",
                    "name": "assign",
                    "args": [
                        {"kind": "var", "name": "x"},
                        {"kind": "const", "value": 1, "sort": {"kind": "ctor", "name": "Int", "args": []}}
                    ]
                },
                {"kind": "op", "name": "return", "args": [{"kind": "var", "name": "x"}]}
            ]
        });

        let wat = compile_wat(&input).unwrap();

        assert!(wat.contains("local.set $x"));
        assert!(wat.contains("(param $x i32)"));
    }
}
