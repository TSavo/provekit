// SPDX-License-Identifier: Apache-2.0
//
// provekit-lift-rust-tests
//
// Rust parity for provekit-lift-py-tests' assertion-consistency path:
// recognize scalar assertions inside #[test] functions and emit inv-only
// ContractDecls. The verifier's existing consistency pass checks those closed
// invariants with raw SAT: SAT => consistent/discharged; UNSAT => refused.

use std::rc::Rc;

use quote::ToTokens;
use sugar_ir_symbolic::{and_, eq, make_var, num, ContractDecl, Formula, Term};
use syn::parse::{Parse, ParseStream};
use syn::punctuated::Punctuated;
use syn::{BinOp, Expr, ExprLit, Item, Lit, Stmt, Token, UnOp};

#[derive(Debug, Clone)]
pub struct LiftWarning {
    pub source_path: String,
    pub item_name: String,
    pub reason: String,
}

#[derive(Debug, Default)]
pub struct AdapterOutput {
    pub decls: Vec<ContractDecl>,
    pub warnings: Vec<LiftWarning>,
    pub seen: usize,
    pub lifted: usize,
}

pub fn lift_file(file: &syn::File, source_path: &str) -> AdapterOutput {
    let mut out = AdapterOutput::default();
    let mut modules = Vec::new();
    walk_items(&file.items, source_path, &mut modules, &mut out);
    out
}

fn walk_items(
    items: &[Item],
    source_path: &str,
    modules: &mut Vec<String>,
    out: &mut AdapterOutput,
) {
    for item in items {
        match item {
            Item::Fn(f) => visit_test_fn(f, source_path, modules, out),
            Item::Mod(m) => {
                if let Some((_, items)) = &m.content {
                    modules.push(m.ident.to_string());
                    walk_items(items, source_path, modules, out);
                    modules.pop();
                }
            }
            _ => {}
        }
    }
}

fn visit_test_fn(f: &syn::ItemFn, source_path: &str, modules: &[String], out: &mut AdapterOutput) {
    if !has_test_attr(&f.attrs) {
        return;
    }
    out.seen += 1;

    let test_name = scoped_test_name(source_path, modules, &f.sig.ident.to_string());
    let mut atoms = Vec::new();
    let mut skipped = Vec::new();
    collect_assertion_atoms(&f.block.stmts, &mut atoms, &mut skipped);

    if !skipped.is_empty() {
        out.warnings.push(LiftWarning {
            source_path: source_path.to_string(),
            item_name: test_name,
            reason: format!(
                "rust test assertions: unsupported assertion surface; released to layer 0: {}",
                skipped.join("; ")
            ),
        });
        return;
    }

    if atoms.is_empty() {
        out.warnings.push(LiftWarning {
            source_path: source_path.to_string(),
            item_name: test_name,
            reason: "rust test assertions: no liftable scalar assertions".to_string(),
        });
        return;
    }

    out.decls.push(ContractDecl {
        name: test_name,
        pre: None,
        post: None,
        inv: Some(and_(atoms)),
        out_binding: "out".to_string(),
        evidence: None,
        panic_loci: Vec::new(),
        concept_hint: None,
    });
    out.lifted += 1;
}

fn has_test_attr(attrs: &[syn::Attribute]) -> bool {
    attrs.iter().any(|attr| {
        attr.path()
            .segments
            .last()
            .is_some_and(|segment| segment.ident == "test")
    })
}

fn scoped_test_name(source_path: &str, modules: &[String], fn_name: &str) -> String {
    if modules.is_empty() {
        format!("{source_path}::{fn_name}")
    } else {
        format!("{source_path}::{}::{fn_name}", modules.join("::"))
    }
}

fn collect_assertion_atoms(
    stmts: &[Stmt],
    atoms: &mut Vec<Rc<Formula>>,
    skipped: &mut Vec<String>,
) {
    for stmt in stmts {
        match stmt {
            Stmt::Macro(m) => collect_macro(&m.mac.path, m.mac.tokens.clone(), atoms, skipped),
            Stmt::Expr(Expr::Macro(m), _) => {
                collect_macro(&m.mac.path, m.mac.tokens.clone(), atoms, skipped)
            }
            _ => {}
        }
    }
}

fn collect_macro(
    path: &syn::Path,
    tokens: proc_macro2::TokenStream,
    atoms: &mut Vec<Rc<Formula>>,
    skipped: &mut Vec<String>,
) {
    match assertion_from_macro(path, tokens) {
        Ok(Some(atom)) => atoms.push(atom),
        Ok(None) => {}
        Err(reason) => skipped.push(reason),
    }
}

fn assertion_from_macro(
    path: &syn::Path,
    tokens: proc_macro2::TokenStream,
) -> Result<Option<Rc<Formula>>, String> {
    let Some(name) = path
        .segments
        .last()
        .map(|segment| segment.ident.to_string())
    else {
        return Ok(None);
    };
    match name.as_str() {
        "assert_eq" => {
            let args = parse_macro_args(tokens).map_err(|e| format!("assert_eq!: {e}"))?;
            if args.exprs.len() < 2 {
                return Err("assert_eq!: expected at least 2 arguments".to_string());
            }
            let lhs = translate_term(&args.exprs[0]).map_err(|e| format!("assert_eq!: {e}"))?;
            let rhs = translate_term(&args.exprs[1]).map_err(|e| format!("assert_eq!: {e}"))?;
            Ok(Some(eq(lhs, rhs)))
        }
        "assert" => {
            let args = parse_macro_args(tokens).map_err(|e| format!("assert!: {e}"))?;
            let Some(first) = args.exprs.first() else {
                return Err("assert!: expected a condition".to_string());
            };
            let atom = translate_bool_assertion(first).map_err(|e| format!("assert!: {e}"))?;
            Ok(Some(atom))
        }
        other if other.starts_with("assert") || other.starts_with("debug_assert") => {
            Err(format!("{other}!: unsupported assertion macro"))
        }
        _ => Ok(None),
    }
}

struct MacroArgs {
    exprs: Vec<Expr>,
}

impl Parse for MacroArgs {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let exprs = Punctuated::<Expr, Token![,]>::parse_terminated(input)?
            .into_iter()
            .collect();
        Ok(Self { exprs })
    }
}

fn parse_macro_args(tokens: proc_macro2::TokenStream) -> syn::Result<MacroArgs> {
    syn::parse2(tokens)
}

fn translate_bool_assertion(expr: &Expr) -> Result<Rc<Formula>, String> {
    match expr {
        Expr::Binary(binary) if matches!(binary.op, BinOp::Eq(_)) => {
            let lhs = translate_term(&binary.left)?;
            let rhs = translate_term(&binary.right)?;
            Ok(eq(lhs, rhs))
        }
        Expr::Paren(paren) => translate_bool_assertion(&paren.expr),
        Expr::Group(group) => translate_bool_assertion(&group.expr),
        other => Err(format!(
            "only scalar equality is liftable, got `{}`",
            token_key(other)
        )),
    }
}

fn translate_term(expr: &Expr) -> Result<Rc<Term>, String> {
    match expr {
        Expr::Lit(lit) => translate_lit(lit),
        Expr::Unary(unary) if matches!(unary.op, UnOp::Neg(_)) => {
            let value = const_int(&unary.expr)
                .ok_or_else(|| format!("unsupported negative literal `{}`", token_key(expr)))?;
            Ok(num(-value))
        }
        Expr::Path(path) => Ok(make_var(path_to_name(&path.path))),
        Expr::Call(call) => {
            let mut args = Vec::new();
            for arg in &call.args {
                args.push(translate_term(arg)?);
            }
            Ok(Rc::new(Term::Ctor {
                name: format!("call:{}", expr_head_key(&call.func)),
                args,
            }))
        }
        Expr::MethodCall(call) => {
            let mut args = vec![translate_term(&call.receiver)?];
            for arg in &call.args {
                args.push(translate_term(arg)?);
            }
            Ok(Rc::new(Term::Ctor {
                name: format!("method:{}", call.method),
                args,
            }))
        }
        Expr::Field(field) => Ok(Rc::new(Term::Ctor {
            name: format!("field:{}", token_key(&field.member)),
            args: vec![translate_term(&field.base)?],
        })),
        Expr::Binary(binary) => {
            let Some(op) = term_binop_name(&binary.op) else {
                return Err(format!("unsupported term operator `{}`", token_key(expr)));
            };
            Ok(Rc::new(Term::Ctor {
                name: op.to_string(),
                args: vec![
                    translate_term(&binary.left)?,
                    translate_term(&binary.right)?,
                ],
            }))
        }
        Expr::Paren(paren) => translate_term(&paren.expr),
        Expr::Group(group) => translate_term(&group.expr),
        other => Err(format!("unsupported term `{}`", token_key(other))),
    }
}

fn translate_lit(lit: &ExprLit) -> Result<Rc<Term>, String> {
    match &lit.lit {
        Lit::Int(i) => i
            .base10_parse::<i64>()
            .map(num)
            .map_err(|e| format!("int literal: {e}")),
        other => Err(format!(
            "only integer scalar constants are liftable, got `{}`",
            token_key(other)
        )),
    }
}

fn const_int(expr: &Expr) -> Option<i64> {
    match expr {
        Expr::Lit(ExprLit {
            lit: Lit::Int(i), ..
        }) => i.base10_parse().ok(),
        Expr::Paren(paren) => const_int(&paren.expr),
        Expr::Group(group) => const_int(&group.expr),
        _ => None,
    }
}

fn term_binop_name(op: &BinOp) -> Option<&'static str> {
    match op {
        BinOp::Add(_) => Some("+"),
        BinOp::Sub(_) => Some("-"),
        BinOp::Mul(_) => Some("*"),
        _ => None,
    }
}

fn expr_head_key(expr: &Expr) -> String {
    match expr {
        Expr::Path(path) => path_to_name(&path.path),
        Expr::Paren(paren) => expr_head_key(&paren.expr),
        Expr::Group(group) => expr_head_key(&group.expr),
        other => token_key(other),
    }
}

fn path_to_name(path: &syn::Path) -> String {
    path.segments
        .iter()
        .map(|segment| segment.ident.to_string())
        .collect::<Vec<_>>()
        .join("::")
}

fn token_key<T: ToTokens>(node: T) -> String {
    node.to_token_stream()
        .to_string()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}
