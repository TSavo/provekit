// SPDX-License-Identifier: Apache-2.0
//
// provekit-ir-compiler-jvm-bytecode: ORP v0.2 compile-mode realizer for
// the ProofIR term stratum. It lowers the core C11 operation-CID term
// subset to deterministic Jasmin text for JVM bytecode.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use provekit_ir_compiler::{
    Capabilities, CompileError, CompiledFormula, IrCompiler, OpacityManifest, PROTOCOL_VERSION,
};
use serde_json::Value as Json;

mod generated;

pub use generated::{ALGEBRA_TO_JVM_TABLE, CORE_OPERATION_SUBSET};

pub const DIALECT: &str = "jvm-jasmin";
pub const COMPILER_NAME: &str = "jvm-bytecode-jasmin-core";
pub const COMPILER_VERSION: &str = env!("CARGO_PKG_VERSION");

const MEMORY_CELLS: i32 = 1024;

pub trait TermCompiler {
    fn compile_term_json(&self, ir: &Json) -> Result<String, CompileError>;
}

#[derive(Debug, Clone, Default)]
pub struct JvmBytecodeCompiler;

#[derive(Default)]
struct VarSets {
    reads: BTreeSet<String>,
    assigned_locals: BTreeSet<String>,
}

#[derive(Clone)]
struct LoopLabels {
    continue_label: String,
    break_label: String,
}

#[derive(Default)]
struct StackState {
    depth: i32,
    max_depth: i32,
}

impl StackState {
    fn apply(&mut self, delta: i32) {
        self.depth += delta;
        debug_assert!(self.depth >= 0);
        if self.depth > self.max_depth {
            self.max_depth = self.depth;
        }
    }

    fn set_depth(&mut self, depth: i32) {
        debug_assert!(depth >= 0);
        self.depth = depth;
        if self.depth > self.max_depth {
            self.max_depth = self.depth;
        }
    }

    fn depth(&self) -> i32 {
        self.depth
    }

    fn limit(&self) -> i32 {
        self.max_depth.max(1)
    }
}

struct Lowerer {
    class_name: String,
    method_name: String,
    locals: BTreeMap<String, u16>,
    uses_memory: bool,
    lines: Vec<String>,
    label_counter: usize,
    loop_stack: Vec<LoopLabels>,
    stack: StackState,
}

impl JvmBytecodeCompiler {
    pub fn new() -> Self {
        Self
    }
}

impl TermCompiler for JvmBytecodeCompiler {
    fn compile_term_json(&self, ir: &Json) -> Result<String, CompileError> {
        compile_jasmin(ir)
    }
}

impl IrCompiler for JvmBytecodeCompiler {
    fn compile(&self, ir: &Json, dialect: &str) -> Result<CompiledFormula, CompileError> {
        if dialect != DIALECT {
            return Err(CompileError::UnsupportedDialect(dialect.to_string()));
        }
        Ok(CompiledFormula {
            preamble: String::new(),
            body: self.compile_term_json(ir)?,
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
            supported_predicates: CORE_OPERATION_SUBSET
                .iter()
                .map(|op| (*op).to_string())
                .collect(),
        }
    }
}

pub fn compile_jasmin(ir: &Json) -> Result<String, CompileError> {
    let term = root_term(ir)?;
    let method_name = function_name(ir);
    let class_name = class_name(ir, &method_name);

    let mut vars = VarSets::default();
    collect_vars(term, &mut vars)?;
    let mut locals = BTreeMap::new();
    for name in vars.reads {
        let slot = u16::try_from(locals.len())
            .map_err(|_| unsupported("too many JVM local slots in core subset"))?;
        locals.insert(name, slot);
    }
    for name in vars.assigned_locals {
        if !locals.contains_key(&name) {
            let slot = u16::try_from(locals.len())
                .map_err(|_| unsupported("too many JVM local slots in core subset"))?;
            locals.insert(name, slot);
        }
    }

    let uses_memory = needs_memory(term)?;
    let mut lowerer = Lowerer::new(class_name, method_name, locals, uses_memory);
    lowerer.compile_function(term)
}

impl Lowerer {
    fn new(
        class_name: String,
        method_name: String,
        locals: BTreeMap<String, u16>,
        uses_memory: bool,
    ) -> Self {
        Self {
            class_name,
            method_name,
            locals,
            uses_memory,
            lines: Vec::new(),
            label_counter: 0,
            loop_stack: Vec::new(),
            stack: StackState::default(),
        }
    }

    fn compile_function(&mut self, term: &Json) -> Result<String, CompileError> {
        if is_statement_term(term) {
            self.emit_stmt(term)?;
            if stmt_falls_through(term) {
                self.emit_int_const(0);
                self.inst("ireturn", -1);
            }
        } else {
            self.emit_expr(term)?;
            self.inst("ireturn", -1);
        }

        let body = std::mem::take(&mut self.lines);
        let stack_limit = self.stack.limit();
        let locals_limit = self.locals.len();
        let descriptor = method_descriptor(self.locals.len());

        let mut out = Vec::new();
        out.push(format!(".class public {}", self.class_name));
        out.push(".super java/lang/Object".to_string());
        out.push(String::new());
        if self.uses_memory {
            out.push(".field private static memory [I".to_string());
            out.push(String::new());
            out.extend(self.clinit_lines());
            out.push(String::new());
        }
        out.push(format!(
            ".method public static {}{}",
            self.method_name, descriptor
        ));
        out.push(format!("  .limit stack {stack_limit}"));
        out.push(format!("  .limit locals {locals_limit}"));
        out.extend(body);
        out.push(".end method".to_string());
        out.push(String::new());
        Ok(out.join("\n"))
    }

    fn clinit_lines(&self) -> Vec<String> {
        vec![
            ".method static <clinit>()V".to_string(),
            "  .limit stack 1".to_string(),
            "  .limit locals 0".to_string(),
            format!("  sipush {MEMORY_CELLS}"),
            "  newarray int".to_string(),
            format!("  putstatic {}/memory [I", self.class_name),
            "  return".to_string(),
            ".end method".to_string(),
        ]
    }

    fn emit_stmt(&mut self, term: &Json) -> Result<(), CompileError> {
        match term_kind(term)? {
            "unit" => Ok(()),
            "op" => match op_name(term)? {
                "seq" => {
                    for arg in op_args(term)? {
                        self.emit_stmt(arg)?;
                        if !stmt_falls_through(arg) {
                            break;
                        }
                    }
                    Ok(())
                }
                "if" => self.emit_if_stmt(term),
                "while" => self.emit_while(term),
                "return" => {
                    let args = op_args(term)?;
                    expect_arity("return", args, 1)?;
                    self.emit_expr(&args[0])?;
                    self.inst("ireturn", -1);
                    Ok(())
                }
                "call" => {
                    self.emit_expr(term)?;
                    self.inst("pop", -1);
                    Ok(())
                }
                "break" => self.emit_loop_branch("break", op_args(term)?),
                "continue" => self.emit_loop_branch("continue", op_args(term)?),
                "skip" => {
                    expect_skip_args(op_args(term)?)?;
                    Ok(())
                }
                "assign" => self.emit_assign_stmt(term),
                name if is_expr_op(name) => {
                    self.emit_expr(term)?;
                    self.inst("pop", -1);
                    Ok(())
                }
                name => Err(unsupported_operation(name)),
            },
            "var" | "const" => {
                self.emit_expr(term)?;
                self.inst("pop", -1);
                Ok(())
            }
            other => Err(malformed(format!("unknown statement kind: {other}"))),
        }
    }

    fn emit_expr(&mut self, term: &Json) -> Result<(), CompileError> {
        match term_kind(term)? {
            "var" => {
                let slot = self.local_slot(var_name(term)?)?;
                self.inst(&format!("iload {slot}"), 1);
                Ok(())
            }
            "const" => {
                self.emit_int_const(const_i32(term)?);
                Ok(())
            }
            "op" => match op_name(term)? {
                "eq" => self.emit_compare(term, "if_icmpeq"),
                "lt" => self.emit_compare(term, "if_icmplt"),
                "le" => self.emit_compare(term, "if_icmple"),
                "add" => self.emit_binary_arith(term, "iadd"),
                "sub" => self.emit_binary_arith(term, "isub"),
                "mul" => self.emit_binary_arith(term, "imul"),
                "neg" => {
                    let args = op_args(term)?;
                    expect_arity("neg", args, 1)?;
                    self.emit_expr(&args[0])?;
                    self.inst("ineg", 0);
                    Ok(())
                }
                "and" => self.emit_and(term),
                "or" => self.emit_or(term),
                "not" => self.emit_not(term),
                "deref" => self.emit_deref(term),
                "call" => self.emit_call(term),
                "if" => self.emit_if_expr(term),
                "assign" => self.emit_assign_expr(term),
                name => Err(unsupported_operation(name)),
            },
            "unit" => Err(malformed("unit has no JVM int expression value")),
            other => Err(malformed(format!("unknown expression kind: {other}"))),
        }
    }

    fn emit_if_stmt(&mut self, term: &Json) -> Result<(), CompileError> {
        let args = op_args(term)?;
        expect_arity("if", args, 3)?;
        let base_depth = self.stack.depth();

        self.emit_expr(&args[0])?;
        let else_label = self.fresh_label("else");
        let end_label = self.fresh_label("end");
        self.inst(&format!("ifeq {else_label}"), -1);
        self.stack.set_depth(base_depth);
        self.emit_stmt(&args[1])?;
        if stmt_falls_through(&args[1]) {
            self.inst(&format!("goto {end_label}"), 0);
        }
        self.stack.set_depth(base_depth);
        self.label(&else_label);
        self.emit_stmt(&args[2])?;
        self.stack.set_depth(base_depth);
        self.label(&end_label);
        Ok(())
    }

    fn emit_if_expr(&mut self, term: &Json) -> Result<(), CompileError> {
        let args = op_args(term)?;
        expect_arity("if", args, 3)?;
        if is_statement_term(&args[1]) || is_statement_term(&args[2]) {
            return Err(malformed("if expression branches must be expression terms"));
        }
        let base_depth = self.stack.depth();

        self.emit_expr(&args[0])?;
        let else_label = self.fresh_label("else");
        let end_label = self.fresh_label("end");
        self.inst(&format!("ifeq {else_label}"), -1);
        self.stack.set_depth(base_depth);
        self.emit_expr(&args[1])?;
        self.inst(&format!("goto {end_label}"), 0);
        self.stack.set_depth(base_depth);
        self.label(&else_label);
        self.emit_expr(&args[2])?;
        self.stack.set_depth(base_depth + 1);
        self.label(&end_label);
        Ok(())
    }

    fn emit_while(&mut self, term: &Json) -> Result<(), CompileError> {
        let args = op_args(term)?;
        if args.len() < 2 {
            return Err(malformed("while expects at least 2 arguments"));
        }
        if let Some(invariant) = term.get("invariant").or_else(|| args.get(2)) {
            if !is_unit(invariant) {
                return Err(unsupported("unsupported while invariant shape"));
            }
        }

        let top_label = self.fresh_label("loop");
        let done_label = self.fresh_label("done");
        let base_depth = self.stack.depth();
        self.label(&top_label);
        self.stack.set_depth(base_depth);
        self.emit_expr(&args[0])?;
        self.inst(&format!("ifeq {done_label}"), -1);
        self.stack.set_depth(base_depth);
        self.loop_stack.push(LoopLabels {
            continue_label: top_label.clone(),
            break_label: done_label.clone(),
        });
        self.emit_stmt(&args[1])?;
        self.loop_stack.pop();
        if stmt_falls_through(&args[1]) {
            self.inst(&format!("goto {top_label}"), 0);
        }
        self.stack.set_depth(base_depth);
        self.label(&done_label);
        Ok(())
    }

    fn emit_loop_branch(&mut self, op: &str, args: &[Json]) -> Result<(), CompileError> {
        if args.len() > 1 {
            return Err(malformed(format!("{op} expects zero or one argument")));
        }
        let labels = self
            .loop_stack
            .last()
            .ok_or_else(|| malformed(format!("{op} outside while")))?
            .clone();
        let target = if op == "break" {
            labels.break_label
        } else {
            labels.continue_label
        };
        let base_depth = self.stack.depth();
        if let Some(condition) = args.first() {
            self.emit_expr(condition)?;
            self.inst(&format!("ifne {target}"), -1);
            self.stack.set_depth(base_depth);
        } else {
            self.inst(&format!("goto {target}"), 0);
            self.stack.set_depth(base_depth);
        }
        Ok(())
    }

    fn emit_binary_arith(&mut self, term: &Json, inst: &str) -> Result<(), CompileError> {
        let args = op_args(term)?;
        expect_arity(op_name(term)?, args, 2)?;
        self.emit_expr(&args[0])?;
        self.emit_expr(&args[1])?;
        self.inst(inst, -1);
        Ok(())
    }

    fn emit_compare(&mut self, term: &Json, branch: &str) -> Result<(), CompileError> {
        let args = op_args(term)?;
        expect_arity(op_name(term)?, args, 2)?;
        let true_label = self.fresh_label("true");
        let end_label = self.fresh_label("end");
        let base_depth = self.stack.depth();
        self.emit_expr(&args[0])?;
        self.emit_expr(&args[1])?;
        self.inst(&format!("{branch} {true_label}"), -2);
        self.stack.set_depth(base_depth);
        self.emit_int_const(0);
        self.inst(&format!("goto {end_label}"), 0);
        self.stack.set_depth(base_depth);
        self.label(&true_label);
        self.emit_int_const(1);
        self.stack.set_depth(base_depth + 1);
        self.label(&end_label);
        Ok(())
    }

    fn emit_and(&mut self, term: &Json) -> Result<(), CompileError> {
        let args = op_args(term)?;
        expect_arity("and", args, 2)?;
        let false_label = self.fresh_label("false");
        let end_label = self.fresh_label("end");
        let base_depth = self.stack.depth();
        self.emit_expr(&args[0])?;
        self.inst(&format!("ifeq {false_label}"), -1);
        self.stack.set_depth(base_depth);
        self.emit_expr(&args[1])?;
        self.inst(&format!("ifeq {false_label}"), -1);
        self.stack.set_depth(base_depth);
        self.emit_int_const(1);
        self.inst(&format!("goto {end_label}"), 0);
        self.stack.set_depth(base_depth);
        self.label(&false_label);
        self.emit_int_const(0);
        self.stack.set_depth(base_depth + 1);
        self.label(&end_label);
        Ok(())
    }

    fn emit_or(&mut self, term: &Json) -> Result<(), CompileError> {
        let args = op_args(term)?;
        expect_arity("or", args, 2)?;
        let true_label = self.fresh_label("true");
        let end_label = self.fresh_label("end");
        let base_depth = self.stack.depth();
        self.emit_expr(&args[0])?;
        self.inst(&format!("ifne {true_label}"), -1);
        self.stack.set_depth(base_depth);
        self.emit_expr(&args[1])?;
        self.inst(&format!("ifne {true_label}"), -1);
        self.stack.set_depth(base_depth);
        self.emit_int_const(0);
        self.inst(&format!("goto {end_label}"), 0);
        self.stack.set_depth(base_depth);
        self.label(&true_label);
        self.emit_int_const(1);
        self.stack.set_depth(base_depth + 1);
        self.label(&end_label);
        Ok(())
    }

    fn emit_not(&mut self, term: &Json) -> Result<(), CompileError> {
        let args = op_args(term)?;
        expect_arity("not", args, 1)?;
        let true_label = self.fresh_label("true");
        let end_label = self.fresh_label("end");
        let base_depth = self.stack.depth();
        self.emit_expr(&args[0])?;
        self.inst(&format!("ifeq {true_label}"), -1);
        self.stack.set_depth(base_depth);
        self.emit_int_const(0);
        self.inst(&format!("goto {end_label}"), 0);
        self.stack.set_depth(base_depth);
        self.label(&true_label);
        self.emit_int_const(1);
        self.stack.set_depth(base_depth + 1);
        self.label(&end_label);
        Ok(())
    }

    fn emit_deref(&mut self, term: &Json) -> Result<(), CompileError> {
        let args = op_args(term)?;
        expect_arity("deref", args, 1)?;
        self.inst(&format!("getstatic {}/memory [I", self.class_name), 1);
        self.emit_expr(&args[0])?;
        self.inst("iaload", -1);
        Ok(())
    }

    fn emit_call(&mut self, term: &Json) -> Result<(), CompileError> {
        let (callee, call_args) = call_parts(term)?;
        for arg in &call_args {
            self.emit_expr(arg)?;
        }
        let descriptor = method_descriptor(call_args.len());
        let delta = 1_i32
            - i32::try_from(call_args.len())
                .map_err(|_| unsupported("too many JVM call arguments"))?;
        self.inst(
            &format!("invokestatic {}/{callee}{descriptor}", self.class_name),
            delta,
        );
        Ok(())
    }

    fn emit_assign_stmt(&mut self, term: &Json) -> Result<(), CompileError> {
        let args = op_args(term)?;
        expect_arity("assign", args, 2)?;
        if term_kind(&args[0])? == "var" {
            self.emit_expr(&args[1])?;
            let slot = self.local_slot(var_name(&args[0])?)?;
            self.inst(&format!("istore {slot}"), -1);
            return Ok(());
        }

        let address = store_address_term(&args[0])?;
        self.inst(&format!("getstatic {}/memory [I", self.class_name), 1);
        self.emit_expr(address)?;
        self.emit_expr(&args[1])?;
        self.inst("iastore", -3);
        Ok(())
    }

    fn emit_assign_expr(&mut self, term: &Json) -> Result<(), CompileError> {
        let args = op_args(term)?;
        expect_arity("assign", args, 2)?;
        if term_kind(&args[0])? == "var" {
            self.emit_expr(&args[1])?;
            self.inst("dup", 1);
            let slot = self.local_slot(var_name(&args[0])?)?;
            self.inst(&format!("istore {slot}"), -1);
            return Ok(());
        }

        let address = store_address_term(&args[0])?;
        self.inst(&format!("getstatic {}/memory [I", self.class_name), 1);
        self.emit_expr(address)?;
        self.emit_expr(&args[1])?;
        self.inst("dup_x2", 1);
        self.inst("iastore", -3);
        Ok(())
    }

    fn emit_int_const(&mut self, value: i32) {
        match value {
            -1 => self.inst("iconst_m1", 1),
            0..=5 => self.inst(&format!("iconst_{value}"), 1),
            -128..=127 => self.inst(&format!("bipush {value}"), 1),
            -32768..=32767 => self.inst(&format!("sipush {value}"), 1),
            _ => self.inst(&format!("ldc {value}"), 1),
        }
    }

    fn local_slot(&self, name: &str) -> Result<u16, CompileError> {
        self.locals
            .get(name)
            .copied()
            .ok_or_else(|| malformed(format!("unknown local variable {name}")))
    }

    fn fresh_label(&mut self, kind: &str) -> String {
        let label = format!("L_{kind}_{}", self.label_counter);
        self.label_counter += 1;
        label
    }

    fn label(&mut self, name: &str) {
        self.lines.push(format!("{name}:"));
    }

    fn inst(&mut self, text: &str, delta: i32) {
        self.lines.push(format!("  {text}"));
        self.stack.apply(delta);
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
    for key in ["function", "function_name", "fn_name", "name"] {
        if let Some(name) = ir.get(key).and_then(Json::as_str) {
            return sanitize_jvm_identifier(name, "proofir_term");
        }
    }
    if let Some(source) = ir.get("source").and_then(Json::as_str) {
        if let Some(stem) = Path::new(source).file_stem().and_then(|stem| stem.to_str()) {
            return sanitize_jvm_identifier(stem, "proofir_term");
        }
    }
    "proofir_term".to_string()
}

fn class_name(ir: &Json, method_name: &str) -> String {
    for key in ["class", "class_name"] {
        if let Some(name) = ir.get(key).and_then(Json::as_str) {
            return upper_class_name(&sanitize_jvm_identifier(name, "ProofIrTerm"));
        }
    }
    upper_class_name(method_name)
}

fn upper_class_name(name: &str) -> String {
    let mut out = String::new();
    let mut make_upper = true;
    for ch in name.chars() {
        if ch == '_' {
            make_upper = true;
            continue;
        }
        if make_upper {
            out.push(ch.to_ascii_uppercase());
            make_upper = false;
        } else {
            out.push(ch);
        }
    }
    if out.is_empty() || out.as_bytes()[0].is_ascii_digit() {
        "ProofIrTerm".to_string()
    } else {
        out
    }
}

fn sanitize_jvm_identifier(raw: &str, fallback: &str) -> String {
    let mut out = String::new();
    for (index, ch) in raw.chars().enumerate() {
        let valid = ch.is_ascii_alphanumeric() || ch == '_' || ch == '$';
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
        fallback.to_string()
    } else {
        out
    }
}

fn method_descriptor(param_count: usize) -> String {
    let mut descriptor = String::from("(");
    for _ in 0..param_count {
        descriptor.push('I');
    }
    descriptor.push_str(")I");
    descriptor
}

fn collect_vars(term: &Json, vars: &mut VarSets) -> Result<(), CompileError> {
    match term_kind(term)? {
        "var" => {
            vars.reads.insert(var_name(term)?.to_string());
            Ok(())
        }
        "const" | "unit" => Ok(()),
        "ctor" => {
            if let Some(args) = term.get("args").and_then(Json::as_array) {
                for arg in args {
                    collect_vars(arg, vars)?;
                }
            }
            Ok(())
        }
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
                expect_arity("assign", args, 2)?;
                if term_kind(&args[0])? == "var" {
                    vars.assigned_locals.insert(var_name(&args[0])?.to_string());
                    collect_vars(&args[1], vars)?;
                    return Ok(());
                }
            }
            for arg in args {
                collect_vars(arg, vars)?;
            }
            Ok(())
        }
        other => Err(malformed(format!("unknown term kind: {other}"))),
    }
}

fn needs_memory(term: &Json) -> Result<bool, CompileError> {
    match term_kind(term)? {
        "op" => {
            let name = op_name(term)?;
            let args = op_args(term)?;
            if name == "deref" {
                return Ok(true);
            }
            if name == "assign" {
                expect_arity("assign", args, 2)?;
                if term_kind(&args[0])? != "var" {
                    return Ok(true);
                }
            }
            for arg in args {
                if needs_memory(arg)? {
                    return Ok(true);
                }
            }
            Ok(false)
        }
        "ctor" => {
            if let Some(args) = term.get("args").and_then(Json::as_array) {
                for arg in args {
                    if needs_memory(arg)? {
                        return Ok(true);
                    }
                }
            }
            Ok(false)
        }
        "var" | "const" | "unit" => Ok(false),
        other => Err(malformed(format!("unknown term kind: {other}"))),
    }
}

fn call_parts(term: &Json) -> Result<(String, Vec<&Json>), CompileError> {
    let args = op_args(term)?;
    if args.is_empty() {
        return Err(malformed("call expects a callee"));
    }
    let callee = match term_kind(&args[0])? {
        "var" => sanitize_jvm_identifier(var_name(&args[0])?, "callee"),
        "const" => args[0]
            .get("value")
            .and_then(Json::as_str)
            .map(|name| sanitize_jvm_identifier(name, "callee"))
            .ok_or_else(|| malformed("call callee const must be a string"))?,
        "ctor" => args[0]
            .get("name")
            .and_then(Json::as_str)
            .map(|name| sanitize_jvm_identifier(name, "callee"))
            .ok_or_else(|| malformed("call callee ctor missing name"))?,
        other => return Err(malformed(format!("unsupported callee kind {other}"))),
    };
    Ok((callee, call_arguments(args)?))
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

fn store_address_term(target: &Json) -> Result<&Json, CompileError> {
    if term_kind(target)? == "op" && op_name(target)? == "deref" {
        let args = op_args(target)?;
        expect_arity("deref", args, 1)?;
        Ok(&args[0])
    } else {
        Ok(target)
    }
}

fn is_statement_term(term: &Json) -> bool {
    match term.get("kind").and_then(Json::as_str) {
        Some("unit") => true,
        Some("op") => match term.get("name").and_then(Json::as_str) {
            Some("if") => term
                .get("args")
                .and_then(Json::as_array)
                .is_some_and(|args| {
                    args.len() == 3 && (is_statement_term(&args[1]) || is_statement_term(&args[2]))
                }),
            Some(name) => is_statement_op(name),
            None => false,
        },
        _ => false,
    }
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

fn stmt_falls_through(term: &Json) -> bool {
    match term.get("kind").and_then(Json::as_str) {
        Some("op") => match term.get("name").and_then(Json::as_str).unwrap_or_default() {
            "return" => false,
            "break" | "continue" => term
                .get("args")
                .and_then(Json::as_array)
                .is_some_and(|args| !args.is_empty()),
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
        "JVM core subset supports only i32 integer and bool constants".to_string(),
    ))
}

fn is_unit(term: &Json) -> bool {
    term.get("kind").and_then(Json::as_str) == Some("unit")
        || (term.get("kind").and_then(Json::as_str) == Some("op")
            && term.get("name").and_then(Json::as_str) == Some("skip"))
}

fn expect_skip_args(args: &[Json]) -> Result<(), CompileError> {
    match args {
        [] => Ok(()),
        [arg] if is_unit(arg) => Ok(()),
        _ => Err(malformed(format!(
            "skip expects zero args, got {}",
            args.len()
        ))),
    }
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

fn unsupported(message: impl Into<String>) -> CompileError {
    CompileError::UnsupportedPredicate(message.into())
}

fn unsupported_operation(name: &str) -> CompileError {
    CompileError::UnsupportedPredicate(format!("unsupported operation {name}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn default_class_name_uses_source_stem() {
        let ir = json!({
            "kind": "c11-algebra-term",
            "source": "path/to/foo.c",
            "term": {"kind": "op", "name": "return", "args": [{"kind": "const", "value": 1}]}
        });

        let jasmin = compile_jasmin(&ir).expect("compile");

        assert!(jasmin.contains(".class public Foo\n"));
        assert!(jasmin.contains(".method public static foo()I\n"));
    }

    #[test]
    fn capabilities_include_core_ops() {
        let caps = JvmBytecodeCompiler::new().capabilities();
        for op in CORE_OPERATION_SUBSET {
            assert!(caps.supported_predicates.iter().any(|item| item == op));
        }
    }
}
