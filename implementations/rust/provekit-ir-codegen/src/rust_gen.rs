//! Rust code AST and emitter — generate valid Rust source from structured nodes.

use crate::cddl_parser::{Field, IrSchema, TypeDef, TypeKind, Variant};

#[derive(Debug)]
pub enum ModuleKind {
    Types,
}

pub fn emit_module(ir: &IrSchema, _kind: ModuleKind) -> String {
    let mut out = String::new();
    out.push_str("// SPDX-License-Identifier: Apache-2.0\n");
    out.push_str("//\n");
    out.push_str("// GENERATED FILE — DO NOT EDIT\n");
    out.push_str("// Source: protocol/provekit-ir.cddl\n");
    out.push_str("// Generator: provekit-ir-codegen\n");
    out.push('\n');
    out.push_str("use serde::{Deserialize, Serialize};\n");
    out.push('\n');

    for def in ir.types.values() {
        emit_type_def(&mut out, def);
        out.push('\n');
    }

    // Convenience aliases so generated compilers can use the canonical names.
    out.push_str("pub type Term = IrTerm;\n");
    out.push_str("pub type Formula = IrFormula;\n");

    out
}

fn emit_type_def(out: &mut String, def: &TypeDef) {
    match &def.kind {
        TypeKind::Struct { fields } => emit_struct(out, &def.name, fields),
        TypeKind::Enum { variants } => emit_enum(out, &def.name, variants),
        TypeKind::StringEnum { values } => emit_string_enum(out, &def.name, values),
        TypeKind::Alias { target } => {
            out.push_str(&format!("pub type {} = {};\n", def.name, target));
        }
    }
}

fn emit_struct(out: &mut String, name: &str, fields: &[Field]) {
    out.push_str("#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]\n");
    out.push_str(&format!("pub struct {} {{\n", name));
    for field in fields {
        if let Some(rename) = &field.serde_rename {
            out.push_str(&format!("    #[serde(rename = \"{}\")]\n", rename));
        }
        if field.optional {
            out.push_str(&format!(
                "    #[serde(skip_serializing_if = \"Option::is_none\")]\n"
            ));
            out.push_str(&format!("    pub {}: Option<{}>,\n", field.name, field.ty));
        } else {
            out.push_str(&format!("    pub {}: {},\n", field.name, field.ty));
        }
    }
    out.push_str("}\n");
}

fn emit_enum(out: &mut String, name: &str, variants: &[Variant]) {
    let all_unit = variants.iter().all(|v| v.is_unit && v.fields.is_empty());

    out.push_str("#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]\n");
    if !all_unit {
        out.push_str("#[serde(tag = \"kind\")]\n");
    }
    out.push_str(&format!("pub enum {} {{\n", name));
    for v in variants {
        out.push_str(&format!("    #[serde(rename = \"{}\")]\n", v.serde_rename));
        if v.fields.is_empty() {
            out.push_str(&format!("    {},\n", v.name));
        } else {
            out.push_str(&format!("    {} {{\n", v.name));
            for field in &v.fields {
                if let Some(rename) = &field.serde_rename {
                    out.push_str(&format!("        #[serde(rename = \"{}\")]\n", rename));
                }
                let ty = if is_recursive_type(&field.ty, name) {
                    format!("Box<{}>", field.ty)
                } else {
                    field.ty.clone()
                };
                if field.optional {
                    out.push_str(&format!(
                        "        #[serde(skip_serializing_if = \"Option::is_none\")]\n"
                    ));
                    out.push_str(&format!("        {}: Option<{}>,\n", field.name, ty));
                } else {
                    out.push_str(&format!("        {}: {},\n", field.name, ty));
                }
            }
            out.push_str("    },\n");
        }
    }
    out.push_str("}\n");
}

fn is_recursive_type(ty: &str, enum_name: &str) -> bool {
    ty == enum_name || ty == format!("Option<{enum_name}>") || ty == format!("Box<{enum_name}>")
}

fn emit_string_enum(out: &mut String, name: &str, values: &[String]) {
    out.push_str(&format!("pub type {} = String;\n", name));
    out.push_str(&format!("// Known values for {}:\n", name));
    for v in values {
        out.push_str(&format!("//   \"{}\"\n", v));
    }
}

// ============================================================================
// Rust source-code building helpers (used by compilers.rs)
// ============================================================================

/// A block of Rust source code statements.
#[derive(Debug, Default)]
pub struct Block {
    pub stmts: Vec<Stmt>,
}

#[derive(Debug)]
pub enum Stmt {
    Let {
        name: String,
        expr: Expr,
    },
    LetTyped {
        name: String,
        ty: String,
        expr: Expr,
    },
    LetMut {
        name: String,
        expr: Expr,
    },
    Expr(Expr),
    Return(Expr),
    PushStr {
        target: String,
        value: String,
    },
    For {
        var: String,
        iter: Expr,
        body: Block,
    },
    Match {
        expr: Expr,
        arms: Vec<MatchArm>,
    },
}

#[derive(Debug)]
pub struct MatchArm {
    pub pattern: String,
    pub guard: Option<String>,
    pub body: Block,
}

#[derive(Debug)]
pub enum Expr {
    Raw(String),
    LiteralString(String),
    Var(String),
    Call {
        func: String,
        args: Vec<Expr>,
    },
    MethodCall {
        receiver: Box<Expr>,
        method: String,
        args: Vec<Expr>,
    },
    FieldAccess {
        base: Box<Expr>,
        field: String,
    },
    Match {
        expr: Box<Expr>,
        arms: Vec<MatchArm>,
    },
    Closure {
        params: Vec<String>,
        body: Box<Expr>,
    },
    Block(Block),
    If {
        cond: Box<Expr>,
        then_branch: Block,
        else_branch: Option<Block>,
    },
    Format {
        template: String,
        args: Vec<Expr>,
    },
    Vec(Vec<Expr>),
    Ref(Box<Expr>),
}

pub fn emit_block(out: &mut String, block: &Block, indent: usize) {
    for stmt in &block.stmts {
        emit_stmt(out, stmt, indent);
    }
}

fn emit_stmt(out: &mut String, stmt: &Stmt, indent: usize) {
    let pad = "    ".repeat(indent);
    match stmt {
        Stmt::Let { name, expr } => {
            out.push_str(&format!("{}let {} = ", pad, name));
            emit_expr_inline(out, expr);
            out.push_str(";\n");
        }
        Stmt::LetTyped { name, ty, expr } => {
            out.push_str(&format!("{}let {}: {} = ", pad, name, ty));
            emit_expr_inline(out, expr);
            out.push_str(";\n");
        }
        Stmt::LetMut { name, expr } => {
            out.push_str(&format!("{}let mut {} = ", pad, name));
            emit_expr_inline(out, expr);
            out.push_str(";\n");
        }
        Stmt::Expr(expr) => {
            out.push_str(&pad);
            emit_expr_inline(out, expr);
            out.push_str(";\n");
        }
        Stmt::Return(expr) => {
            out.push_str(&format!("{}return ", pad));
            emit_expr_inline(out, expr);
            out.push_str(";\n");
        }
        Stmt::PushStr { target, value } => {
            out.push_str(&format!(
                "{}{}.push_str({});\n",
                pad,
                target,
                escape_rust_string(value)
            ));
        }
        Stmt::For { var, iter, body } => {
            out.push_str(&format!("{}for {} in ", pad, var));
            emit_expr_inline(out, iter);
            out.push_str(" {\n");
            emit_block(out, body, indent + 1);
            out.push_str(&format!("{}}}\n", pad));
        }
        Stmt::Match { expr, arms } => {
            out.push_str(&format!("{}match ", pad));
            emit_expr_inline(out, expr);
            out.push_str(" {\n");
            for arm in arms {
                out.push_str(&format!("{}    {} => ", pad, arm.pattern));
                if arm.body.stmts.len() == 1 {
                    if let Stmt::Expr(e) | Stmt::Return(e) = &arm.body.stmts[0] {
                        emit_expr_inline(out, e);
                        out.push_str(",\n");
                    } else {
                        out.push_str("{\n");
                        emit_block(out, &arm.body, indent + 2);
                        out.push_str(&format!("{}    }},\n", pad));
                    }
                } else {
                    out.push_str("{\n");
                    emit_block(out, &arm.body, indent + 2);
                    out.push_str(&format!("{}    }},\n", pad));
                }
            }
            out.push_str(&format!("{}}}\n", pad));
        }
    }
}

fn emit_expr_inline(out: &mut String, expr: &Expr) {
    emit_expr_inline_indent(out, expr, 0);
}

fn emit_expr_inline_indent(out: &mut String, expr: &Expr, indent: usize) {
    match expr {
        Expr::Raw(s) => out.push_str(s),
        Expr::LiteralString(s) => out.push_str(&escape_rust_string(s)),
        Expr::Var(s) => out.push_str(s),
        Expr::Call { func, args } => {
            out.push_str(func);
            out.push('(');
            for (i, arg) in args.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                emit_expr_inline_indent(out, arg, indent);
            }
            out.push(')');
        }
        Expr::MethodCall {
            receiver,
            method,
            args,
        } => {
            emit_expr_inline_indent(out, receiver, indent);
            out.push('.');
            out.push_str(method);
            out.push('(');
            for (i, arg) in args.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                emit_expr_inline_indent(out, arg, indent);
            }
            out.push(')');
        }
        Expr::FieldAccess { base, field } => {
            emit_expr_inline_indent(out, base, indent);
            out.push('.');
            out.push_str(field);
        }
        Expr::Match { expr, arms } => {
            out.push_str("match ");
            emit_expr_inline_indent(out, expr, indent);
            out.push_str(" {\n");
            let arm_pad = "    ".repeat(indent + 1);
            for arm in arms {
                out.push_str(&arm_pad);
                out.push_str(&arm.pattern);
                out.push_str(" => ");
                if arm.body.stmts.len() == 1 {
                    match &arm.body.stmts[0] {
                        Stmt::Expr(e) => {
                            emit_expr_inline_indent(out, e, indent + 1);
                            out.push_str(",\n");
                        }
                        Stmt::Return(e) => {
                            out.push_str("{\n");
                            let ret_pad = "    ".repeat(indent + 2);
                            out.push_str(&format!("{}return ", ret_pad));
                            emit_expr_inline_indent(out, e, indent + 2);
                            out.push_str(";\n");
                            out.push_str(&arm_pad);
                            out.push_str("},\n");
                        }
                        _ => {
                            out.push_str("{\n");
                            emit_block(out, &arm.body, indent + 2);
                            out.push_str(&arm_pad);
                            out.push_str("},\n");
                        }
                    }
                } else {
                    out.push_str("{\n");
                    emit_block(out, &arm.body, indent + 2);
                    out.push_str(&arm_pad);
                    out.push_str("},\n");
                }
            }
            out.push_str(&"    ".repeat(indent));
            out.push('}');
        }
        Expr::Closure { params, body } => {
            out.push('|');
            for (i, p) in params.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                out.push_str(p);
            }
            out.push_str("| ");
            emit_expr_inline_indent(out, body, indent);
        }
        Expr::Block(block) => {
            out.push_str("{\n");
            emit_block(out, block, indent + 1);
            out.push_str(&"    ".repeat(indent));
            out.push('}');
        }
        Expr::If {
            cond,
            then_branch,
            else_branch,
        } => {
            out.push_str("if ");
            emit_expr_inline_indent(out, cond, indent);
            out.push_str(" {\n");
            emit_block(out, then_branch, indent + 1);
            out.push_str(&"    ".repeat(indent));
            out.push('}');
            if let Some(else_b) = else_branch {
                out.push_str(" else {\n");
                emit_block(out, else_b, indent + 1);
                out.push_str(&"    ".repeat(indent));
                out.push('}');
            }
        }
        Expr::Format { template, args } => {
            out.push_str("format!(");
            out.push_str(&escape_rust_string(template));
            if !args.is_empty() {
                out.push_str(", ");
                for (i, arg) in args.iter().enumerate() {
                    if i > 0 {
                        out.push_str(", ");
                    }
                    emit_expr_inline_indent(out, arg, indent);
                }
            }
            out.push(')');
        }
        Expr::Vec(items) => {
            out.push_str("vec![");
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                emit_expr_inline_indent(out, item, indent);
            }
            out.push(']');
        }
        Expr::Ref(expr) => {
            out.push('&');
            emit_expr_inline_indent(out, expr, indent);
        }
    }
}

/// Escape a string so it can be used as a Rust string literal.
fn escape_rust_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for ch in s.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Emit a function signature and body.
pub fn emit_function(
    out: &mut String,
    vis: &str,
    name: &str,
    args: &[(String, String)],
    ret: Option<&str>,
    body: &Block,
) {
    if !vis.is_empty() {
        out.push_str(vis);
        out.push(' ');
    }
    out.push_str("fn ");
    out.push_str(name);
    out.push('(');
    for (i, (name, ty)) in args.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        out.push_str(name);
        out.push_str(": ");
        out.push_str(ty);
    }
    out.push(')');
    if let Some(r) = ret {
        out.push_str(" -> ");
        out.push_str(r);
    }
    out.push_str(" {\n");
    emit_block(out, body, 1);
    out.push_str("}\n");
}
