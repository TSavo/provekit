// SPDX-License-Identifier: Apache-2.0
//
// ORP v0.2 compile-mode realizer for ProofIR terms targeting x86-64 SysV.

use std::path::Path;

use provekit_ir_compiler::{
    Capabilities, CompileError, CompiledFormula, FreeVar, IrCompiler, OpacityManifest,
    PROTOCOL_VERSION,
};
use serde_json::Value as Json;

mod generated;

pub use generated::{CORE_OPERATION_SUBSET, LANGUAGE_MORPHISM_TABLE};

pub const DIALECT: &str = "x86-64:sysv";
pub const COMPILER_NAME: &str = "x86-64-sysv-core";
pub const COMPILER_VERSION: &str = env!("CARGO_PKG_VERSION");

const ARG_REGS_32: [&str; 6] = ["edi", "esi", "edx", "ecx", "r8d", "r9d"];
const ARG_REGS_64: [&str; 6] = ["rdi", "rsi", "rdx", "rcx", "r8", "r9"];

pub trait TermCompiler {
    fn compile_term_json(&self, ir: &Json) -> Result<String, CompileError>;
}

#[derive(Debug, Clone, Default)]
pub struct X8664Compiler;

impl X8664Compiler {
    pub fn new() -> Self {
        Self
    }
}

impl TermCompiler for X8664Compiler {
    fn compile_term_json(&self, ir: &Json) -> Result<String, CompileError> {
        let term = root_term(ir)?;
        let function = function_name(ir);
        let mut vars = Vec::new();
        collect_vars(term, &mut vars);
        if vars.len() > ARG_REGS_32.len() {
            return Err(CompileError::UnsupportedPredicate(format!(
                "x86-64 core compiler supports at most {} integer arguments",
                ARG_REGS_32.len()
            )));
        }

        let mut lowerer = Lowerer::new(function, vars);
        lowerer.compile_function(term)
    }
}

impl IrCompiler for X8664Compiler {
    fn compile(&self, ir: &Json, dialect: &str) -> Result<CompiledFormula, CompileError> {
        if dialect != DIALECT {
            return Err(CompileError::UnsupportedDialect(dialect.to_string()));
        }
        let body = self.compile_term_json(ir)?;
        Ok(CompiledFormula {
            preamble: String::new(),
            body,
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

pub fn emit(ir: &Json) -> Result<String, CompileError> {
    X8664Compiler::new().compile_term_json(ir)
}

#[derive(Debug, Clone)]
struct LoopLabels {
    continue_label: String,
    break_label: String,
}

struct Lowerer {
    function: String,
    vars: Vec<String>,
    out: String,
    label_counter: usize,
    stack_slots: usize,
    loop_stack: Vec<LoopLabels>,
}

impl Lowerer {
    fn new(function: String, vars: Vec<String>) -> Self {
        Self {
            function,
            vars,
            out: String::new(),
            label_counter: 0,
            stack_slots: 0,
            loop_stack: Vec::new(),
        }
    }

    fn compile_function(&mut self, term: &Json) -> Result<String, CompileError> {
        self.out.push_str(".intel_syntax noprefix\n");
        self.out.push_str(".text\n");
        self.out.push_str(&format!(".globl {}\n", self.function));
        self.out
            .push_str(&format!(".type {}, @function\n", self.function));
        self.out.push_str(&format!("{}:\n", self.function));
        self.compile_stmt(term)?;
        if stmt_falls_through(term) {
            self.inst("xor", "eax, eax");
            self.inst0("ret");
        }
        self.out
            .push_str(&format!(".size {}, .-{}\n", self.function, self.function));
        self.out
            .push_str(".section .note.GNU-stack,\"\",@progbits\n");
        Ok(std::mem::take(&mut self.out))
    }

    fn compile_stmt(&mut self, term: &Json) -> Result<(), CompileError> {
        match term_kind(term)? {
            "unit" => Ok(()),
            "op" => match op_name(term)? {
                "seq" => {
                    let args = op_args(term)?;
                    expect_arity("seq", args, 2)?;
                    self.compile_stmt(&args[0])?;
                    if stmt_falls_through(&args[0]) {
                        self.compile_stmt(&args[1])?;
                    }
                    Ok(())
                }
                "if" => {
                    let args = op_args(term)?;
                    expect_arity("if", args, 3)?;
                    let else_label = self.fresh_label("else");
                    let end_label = self.fresh_label("end");
                    self.compile_expr(&args[0])?;
                    self.inst("cmp", "eax, 0");
                    self.inst("je", &else_label);
                    self.compile_stmt(&args[1])?;
                    if stmt_falls_through(&args[1]) {
                        self.inst("jmp", &end_label);
                    }
                    self.label(&else_label);
                    self.compile_stmt(&args[2])?;
                    self.label(&end_label);
                    Ok(())
                }
                "while" => {
                    let args = op_args(term)?;
                    if args.len() < 2 {
                        return Err(malformed("while expects at least 2 args"));
                    }
                    if let Some(invariant) = term.get("invariant").or_else(|| args.get(2)) {
                        if !is_unit(invariant) {
                            return Err(CompileError::UnsupportedPredicate(
                                "unsupported while invariant shape".to_string(),
                            ));
                        }
                    }

                    let head_label = self.fresh_label("loop");
                    let end_label = self.fresh_label("end");
                    self.label(&head_label);
                    self.compile_expr(&args[0])?;
                    self.inst("cmp", "eax, 0");
                    self.inst("je", &end_label);
                    self.loop_stack.push(LoopLabels {
                        continue_label: head_label.clone(),
                        break_label: end_label.clone(),
                    });
                    self.compile_stmt(&args[1])?;
                    self.loop_stack.pop();
                    if stmt_falls_through(&args[1]) {
                        self.inst("jmp", &head_label);
                    }
                    self.label(&end_label);
                    Ok(())
                }
                "return" => {
                    let args = op_args(term)?;
                    expect_arity("return", args, 1)?;
                    self.compile_expr(&args[0])?;
                    self.inst0("ret");
                    Ok(())
                }
                "call" => {
                    self.compile_call(term)?;
                    Ok(())
                }
                "break" => {
                    let break_label = self
                        .loop_stack
                        .last()
                        .ok_or_else(|| malformed("break outside loop"))?
                        .break_label
                        .clone();
                    self.inst("jmp", &break_label);
                    Ok(())
                }
                "continue" => {
                    let continue_label = self
                        .loop_stack
                        .last()
                        .ok_or_else(|| malformed("continue outside loop"))?
                        .continue_label
                        .clone();
                    self.inst("jmp", &continue_label);
                    Ok(())
                }
                "skip" => Ok(()),
                "assign" => self.compile_assign(term),
                name => Err(unsupported_operation(name)),
            },
            _ => {
                self.compile_expr(term)?;
                Ok(())
            }
        }
    }

    fn compile_expr(&mut self, term: &Json) -> Result<(), CompileError> {
        match term_kind(term)? {
            "var" => {
                let name = var_name(term)?;
                let index = self.var_index(name)?;
                self.inst("mov", &format!("eax, {}", ARG_REGS_32[index]));
                Ok(())
            }
            "const" => {
                let value = int_value(term)?;
                self.inst("mov", &format!("eax, {value}"));
                Ok(())
            }
            "op" => match op_name(term)? {
                "add" => self.compile_binary_arith(term, "add"),
                "sub" => self.compile_binary_arith(term, "sub"),
                "mul" => self.compile_binary_arith(term, "imul"),
                "neg" => {
                    let args = op_args(term)?;
                    expect_arity("neg", args, 1)?;
                    self.compile_expr(&args[0])?;
                    self.inst0("neg     eax");
                    Ok(())
                }
                "eq" => self.compile_compare(term, "sete"),
                "lt" => self.compile_compare(term, "setl"),
                "le" => self.compile_compare(term, "setle"),
                "and" => self.compile_logical_binary(term, "and"),
                "or" => self.compile_logical_binary(term, "or"),
                "not" => {
                    let args = op_args(term)?;
                    expect_arity("not", args, 1)?;
                    self.compile_expr(&args[0])?;
                    self.inst("cmp", "eax, 0");
                    self.inst("sete", "al");
                    self.inst("movzx", "eax, al");
                    Ok(())
                }
                "deref" => {
                    let args = op_args(term)?;
                    expect_arity("deref", args, 1)?;
                    self.compile_address(&args[0])?;
                    self.inst("mov", "eax, DWORD PTR [rax]");
                    Ok(())
                }
                "call" => self.compile_call(term),
                "assign" => self.compile_assign(term),
                name => Err(unsupported_operation(name)),
            },
            other => Err(malformed(format!("unsupported expression kind {other}"))),
        }
    }

    fn compile_binary_arith(&mut self, term: &Json, mnemonic: &str) -> Result<(), CompileError> {
        let args = op_args(term)?;
        expect_arity(op_name(term)?, args, 2)?;
        self.compile_expr(&args[0])?;
        self.push_rax();
        self.compile_expr(&args[1])?;
        self.inst("mov", "ecx, eax");
        self.pop_to("rax");
        self.inst(mnemonic, "eax, ecx");
        Ok(())
    }

    fn compile_compare(&mut self, term: &Json, setcc: &str) -> Result<(), CompileError> {
        let args = op_args(term)?;
        expect_arity(op_name(term)?, args, 2)?;
        self.compile_expr(&args[0])?;
        if term_kind(&args[1])? == "const" {
            let value = int_value(&args[1])?;
            self.inst("cmp", &format!("eax, {value}"));
        } else {
            self.push_rax();
            self.compile_expr(&args[1])?;
            self.inst("mov", "ecx, eax");
            self.pop_to("rax");
            self.inst("cmp", "eax, ecx");
        }
        self.inst(setcc, "al");
        self.inst("movzx", "eax, al");
        Ok(())
    }

    fn compile_logical_binary(&mut self, term: &Json, mnemonic: &str) -> Result<(), CompileError> {
        let args = op_args(term)?;
        expect_arity(op_name(term)?, args, 2)?;
        self.compile_expr(&args[0])?;
        self.inst("cmp", "eax, 0");
        self.inst("setne", "al");
        self.inst("movzx", "eax, al");
        self.push_rax();
        self.compile_expr(&args[1])?;
        self.inst("cmp", "eax, 0");
        self.inst("setne", "al");
        self.inst("movzx", "eax, al");
        self.inst("mov", "ecx, eax");
        self.pop_to("rax");
        self.inst(mnemonic, "eax, ecx");
        Ok(())
    }

    fn compile_assign(&mut self, term: &Json) -> Result<(), CompileError> {
        let args = op_args(term)?;
        expect_arity("assign", args, 2)?;
        let target = &args[0];
        let value = &args[1];
        if term_kind(target)? == "var" {
            self.compile_expr(value)?;
            let index = self.var_index(var_name(target)?)?;
            self.inst("mov", &format!("{}, eax", ARG_REGS_32[index]));
            return Ok(());
        }

        if term_kind(target)? == "op" && op_name(target)? == "deref" {
            let target_args = op_args(target)?;
            expect_arity("deref", target_args, 1)?;
            self.compile_expr(value)?;
            self.push_rax();
            self.compile_address(&target_args[0])?;
            self.inst("mov", "rdx, rax");
            self.pop_to("rax");
            self.inst("mov", "DWORD PTR [rdx], eax");
            return Ok(());
        }

        Err(CompileError::UnsupportedPredicate(
            "unsupported assign target".to_string(),
        ))
    }

    fn compile_address(&mut self, term: &Json) -> Result<(), CompileError> {
        match term_kind(term)? {
            "var" => {
                let index = self.var_index(var_name(term)?)?;
                self.inst("mov", &format!("rax, {}", ARG_REGS_64[index]));
                Ok(())
            }
            "const" => {
                let value = int_value(term)?;
                self.inst("mov", &format!("rax, {value}"));
                Ok(())
            }
            "op" if op_name(term)? == "deref" => {
                let args = op_args(term)?;
                expect_arity("deref", args, 1)?;
                self.compile_address(&args[0])
            }
            "op" => {
                self.compile_expr(term)?;
                self.inst0("movsxd  rax, eax");
                Ok(())
            }
            other => Err(malformed(format!("unsupported address kind {other}"))),
        }
    }

    fn compile_call(&mut self, term: &Json) -> Result<(), CompileError> {
        let args = op_args(term)?;
        if args.is_empty() {
            return Err(malformed("call expects a callee"));
        }
        let callee = callee_name(&args[0])?;
        let call_args = call_arguments(args)?;
        if call_args.len() > ARG_REGS_64.len() {
            return Err(CompileError::UnsupportedPredicate(format!(
                "x86-64 SysV core call supports at most {} integer arguments",
                ARG_REGS_64.len()
            )));
        }

        for arg in &call_args {
            self.compile_expr(arg)?;
            self.push_rax();
        }
        for (index, _) in call_args.iter().enumerate().rev() {
            self.pop_to(ARG_REGS_64[index]);
        }

        let needs_alignment_pad = self.stack_slots.is_multiple_of(2);
        if needs_alignment_pad {
            self.inst("sub", "rsp, 8");
            self.stack_slots += 1;
        }
        self.inst("call", &callee);
        if needs_alignment_pad {
            self.inst("add", "rsp, 8");
            self.stack_slots -= 1;
        }
        Ok(())
    }

    fn var_index(&self, name: &str) -> Result<usize, CompileError> {
        self.vars
            .iter()
            .position(|var| var == name)
            .ok_or_else(|| malformed(format!("unknown variable {name}")))
    }

    fn fresh_label(&mut self, kind: &str) -> String {
        let label = format!(".L_{kind}_{}", self.label_counter);
        self.label_counter += 1;
        label
    }

    fn label(&mut self, name: &str) {
        self.out.push_str(name);
        self.out.push_str(":\n");
    }

    fn inst(&mut self, mnemonic: &str, operands: &str) {
        self.out
            .push_str(&format!("    {mnemonic:<7} {operands}\n"));
    }

    fn inst0(&mut self, text: &str) {
        self.out.push_str("    ");
        self.out.push_str(text);
        self.out.push('\n');
    }

    fn push_rax(&mut self) {
        self.inst0("push    rax");
        self.stack_slots += 1;
    }

    fn pop_to(&mut self, register: &str) {
        self.inst0(&format!("pop     {register}"));
        self.stack_slots -= 1;
    }
}

fn root_term(ir: &Json) -> Result<&Json, CompileError> {
    match ir.get("kind").and_then(Json::as_str) {
        Some("c11-algebra-term") => ir
            .get("term")
            .ok_or_else(|| malformed("c11 algebra term envelope missing term")),
        _ => Ok(ir),
    }
}

fn function_name(ir: &Json) -> String {
    for key in ["function", "function_name", "fn_name"] {
        if let Some(name) = ir.get(key).and_then(Json::as_str) {
            return sanitize_symbol(name);
        }
    }
    if let Some(source) = ir.get("source").and_then(Json::as_str) {
        if let Some(stem) = Path::new(source).file_stem().and_then(|stem| stem.to_str()) {
            return sanitize_symbol(stem);
        }
    }
    "proofir_term".to_string()
}

fn sanitize_symbol(name: &str) -> String {
    let mut out = String::new();
    for (index, ch) in name.chars().enumerate() {
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

fn collect_vars(term: &Json, vars: &mut Vec<String>) {
    match term.get("kind").and_then(Json::as_str) {
        Some("var") => {
            if let Some(name) = term.get("name").and_then(Json::as_str) {
                push_unique(vars, name);
            }
        }
        Some("op") if term.get("name").and_then(Json::as_str) == Some("call") => {
            if let Some(args) = term.get("args").and_then(Json::as_array) {
                if let Ok(call_args) = call_arguments(args) {
                    for arg in call_args {
                        collect_vars(arg, vars);
                    }
                }
            }
        }
        Some("op") | Some("ctor") => {
            if let Some(args) = term.get("args").and_then(Json::as_array) {
                for arg in args {
                    collect_vars(arg, vars);
                }
            }
        }
        _ => {}
    }
}

fn push_unique(vars: &mut Vec<String>, name: &str) {
    if !vars.iter().any(|existing| existing == name) {
        vars.push(name.to_string());
    }
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
        [] => Err(malformed("call expects a callee")),
    }
}

fn callee_name(term: &Json) -> Result<String, CompileError> {
    match term_kind(term)? {
        "var" => Ok(sanitize_symbol(var_name(term)?)),
        "const" => term
            .get("value")
            .and_then(Json::as_str)
            .map(sanitize_symbol)
            .ok_or_else(|| malformed("call callee const must be a string")),
        "ctor" => term
            .get("name")
            .and_then(Json::as_str)
            .map(sanitize_symbol)
            .ok_or_else(|| malformed("call callee ctor missing name")),
        other => Err(malformed(format!("unsupported callee kind {other}"))),
    }
}

fn stmt_falls_through(term: &Json) -> bool {
    match term.get("kind").and_then(Json::as_str) {
        Some("op") => match term.get("name").and_then(Json::as_str).unwrap_or_default() {
            "return" | "break" | "continue" => false,
            "seq" => term
                .get("args")
                .and_then(Json::as_array)
                .is_none_or(|args| args.last().is_none_or(stmt_falls_through)),
            "if" => term
                .get("args")
                .and_then(Json::as_array)
                .is_none_or(|args| {
                    args.get(1).is_none_or(stmt_falls_through)
                        || args.get(2).is_none_or(stmt_falls_through)
                }),
            _ => true,
        },
        _ => true,
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
        .ok_or_else(|| malformed("op missing args"))
}

fn var_name(term: &Json) -> Result<&str, CompileError> {
    term.get("name")
        .and_then(Json::as_str)
        .ok_or_else(|| malformed("var missing name"))
}

fn int_value(term: &Json) -> Result<i64, CompileError> {
    term.get("value")
        .and_then(Json::as_i64)
        .ok_or_else(|| CompileError::UnsupportedSort("only integer constants are supported".into()))
}

fn is_unit(term: &Json) -> bool {
    term.get("kind").and_then(Json::as_str) == Some("unit")
        || (term.get("kind").and_then(Json::as_str) == Some("op")
            && term.get("name").and_then(Json::as_str) == Some("skip"))
}

fn expect_arity(name: &str, args: &[Json], arity: usize) -> Result<(), CompileError> {
    if args.len() == arity {
        Ok(())
    } else {
        Err(malformed(format!(
            "{name} expects {arity} args, got {}",
            args.len()
        )))
    }
}

fn malformed(message: impl Into<String>) -> CompileError {
    CompileError::MalformedIr(message.into())
}

fn unsupported_operation(name: &str) -> CompileError {
    CompileError::UnsupportedPredicate(format!("unsupported operation {name}"))
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

        let asm = X8664Compiler::new()
            .compile_term_json(&ir)
            .expect("compile");

        assert!(asm.contains(".globl foo\n"));
    }

    #[test]
    fn capabilities_include_core_ops() {
        let caps = X8664Compiler::new().capabilities();
        for op in CORE_OPERATION_SUBSET {
            assert!(caps.supported_predicates.iter().any(|item| item == op));
        }
    }
}
