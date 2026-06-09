// SPDX-License-Identifier: Apache-2.0
//
// sugar-lift-rust-tests
//
// Rust parity for sugar-lift-py-tests' assertion-consistency path:
// recognize scalar assertions inside #[test] functions and emit inv-only
// ContractDecls. The verifier's existing consistency pass checks those closed
// invariants with raw SAT: SAT => consistent/discharged; UNSAT => refused.

use std::rc::Rc;

use quote::ToTokens;
use sugar_ir_symbolic::{
    and_, eq, make_var, num, real_const, str_const, ConstValue, ContractDecl, Formula, Term,
};
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
    let mut entries = Vec::new();
    let mut skipped = Vec::new();
    collect_assertion_entries(&f.block.stmts, &test_name, &mut entries, &mut skipped);

    if !skipped.is_empty() {
        out.warnings.push(LiftWarning {
            source_path: source_path.to_string(),
            item_name: test_name.clone(),
            reason: format!(
                "rust test assertions: unsupported assertion surface; released to layer 0: {}",
                skipped.join("; ")
            ),
        });
    }

    if entries.is_empty() {
        out.warnings.push(LiftWarning {
            source_path: source_path.to_string(),
            item_name: test_name,
            reason: "rust test assertions: no liftable scalar assertions".to_string(),
        });
        return;
    }

    for (name, atoms) in group_assertions(entries, &test_name) {
        out.decls.push(ContractDecl {
            name,
            pre: None,
            post: None,
            inv: Some(and_(atoms)),
            out_binding: "out".to_string(),
            evidence: None,
            panic_loci: Vec::new(),
            concept_hint: None,
        });
    }
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

struct AssertionEntry {
    name: Option<String>,
    atom: Rc<Formula>,
}

fn group_assertions(
    entries: Vec<AssertionEntry>,
    fallback_name: &str,
) -> Vec<(String, Vec<Rc<Formula>>)> {
    let mut groups: Vec<(String, Vec<Rc<Formula>>)> = Vec::new();
    for entry in entries {
        let name = entry.name.unwrap_or_else(|| fallback_name.to_string());
        if let Some((_, atoms)) = groups
            .iter_mut()
            .find(|(group_name, _)| group_name == &name)
        {
            atoms.push(entry.atom);
        } else {
            groups.push((name, vec![entry.atom]));
        }
    }
    groups
}

fn collect_assertion_entries(
    stmts: &[Stmt],
    local_scope: &str,
    entries: &mut Vec<AssertionEntry>,
    skipped: &mut Vec<String>,
) {
    for stmt in stmts {
        match stmt {
            Stmt::Macro(m) => collect_macro(
                &m.mac.path,
                m.mac.tokens.clone(),
                local_scope,
                entries,
                skipped,
            ),
            Stmt::Expr(Expr::Macro(m), _) => {
                collect_macro(
                    &m.mac.path,
                    m.mac.tokens.clone(),
                    local_scope,
                    entries,
                    skipped,
                )
            }
            _ => {}
        }
    }
}

fn collect_macro(
    path: &syn::Path,
    tokens: proc_macro2::TokenStream,
    local_scope: &str,
    entries: &mut Vec<AssertionEntry>,
    skipped: &mut Vec<String>,
) {
    match assertion_from_macro(path, tokens, local_scope) {
        Ok(Some(entry)) => entries.push(entry),
        Ok(None) => {}
        Err(reason) => skipped.push(reason),
    }
}

fn assertion_from_macro(
    path: &syn::Path,
    tokens: proc_macro2::TokenStream,
    local_scope: &str,
) -> Result<Option<AssertionEntry>, String> {
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
            Ok(Some(assertion_entry_from_eq(lhs, rhs, local_scope)))
        }
        "assert" => {
            let args = parse_macro_args(tokens).map_err(|e| format!("assert!: {e}"))?;
            let Some(first) = args.exprs.first() else {
                return Err("assert!: expected a condition".to_string());
            };
            let entry = translate_bool_assertion(first, local_scope)
                .map_err(|e| format!("assert!: {e}"))?;
            Ok(Some(entry))
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

fn translate_bool_assertion(expr: &Expr, local_scope: &str) -> Result<AssertionEntry, String> {
    match expr {
        Expr::Binary(binary) if matches!(binary.op, BinOp::Eq(_)) => {
            let lhs = translate_term(&binary.left)?;
            let rhs = translate_term(&binary.right)?;
            Ok(assertion_entry_from_eq(lhs, rhs, local_scope))
        }
        Expr::Paren(paren) => translate_bool_assertion(&paren.expr, local_scope),
        Expr::Group(group) => translate_bool_assertion(&group.expr, local_scope),
        other => Err(format!(
            "only scalar equality is liftable, got `{}`",
            token_key(other)
        )),
    }
}

fn assertion_entry_from_eq(lhs: Rc<Term>, rhs: Rc<Term>, local_scope: &str) -> AssertionEntry {
    let name = if is_concrete_value(lhs.as_ref()) {
        callsite_assertion_name(rhs.as_ref(), local_scope)
    } else if is_concrete_value(rhs.as_ref()) {
        callsite_assertion_name(lhs.as_ref(), local_scope)
    } else {
        None
    };
    AssertionEntry {
        name,
        atom: eq(lhs, rhs),
    }
}

fn is_concrete_value(term: &Term) -> bool {
    matches!(term, Term::Const { .. })
}

fn callsite_assertion_name(term: &Term, local_scope: &str) -> Option<String> {
    let Term::Ctor { name, .. } = term else {
        return None;
    };
    let callee = callsite_callee_name(name)?;
    Some(format!(
        "{callee}#euf#{}::assertion",
        canonical_callsite_sig(term, local_scope)
    ))
}

fn canonical_callsite_sig(term: &Term, local_scope: &str) -> String {
    let Term::Ctor { name, args } = term else {
        return term_key(term);
    };
    let Some(callee) = callsite_callee_name(name) else {
        return term_key(term);
    };
    let head = call_result_head(callee, args.len());
    let inner = args
        .iter()
        .map(|arg| {
            if callee.starts_with("method:") {
                canonical_method_arg_sig(arg, local_scope)
            } else {
                canonical_term_sig(arg)
            }
        })
        .collect::<Vec<_>>()
        .join(",");
    format!("c:{head}({inner})")
}

fn callsite_callee_name(name: &str) -> Option<&str> {
    name.strip_prefix("call:")
        .or_else(|| name.starts_with("method:").then_some(name))
}

fn call_result_head(callee: &str, arity: usize) -> String {
    let safe = callee
        .chars()
        .map(|ch| {
            if ch.is_ascii() && ch.is_ascii_alphanumeric() {
                ch
            } else {
                '_'
            }
        })
        .collect::<String>();
    format!("callresult_{safe}_a{arity}")
}

fn canonical_term_sig(term: &Term) -> String {
    match term {
        Term::Var { name } => format!("v:{name}"),
        Term::Const { value, .. } => match value {
            ConstValue::Int(value) => format!("i:{value}"),
            ConstValue::Real(value) => format!("r:{value}"),
            ConstValue::String(value) => format!("s:{value:?}"),
            ConstValue::Bool(value) => format!("b:{value}"),
        },
        Term::Ctor { name, args } => {
            let inner = args
                .iter()
                .map(|arg| canonical_term_sig(arg))
                .collect::<Vec<_>>()
                .join(",");
            format!("c:{name}({inner})")
        }
        _ => term_key(term),
    }
}

fn canonical_method_arg_sig(term: &Term, local_scope: &str) -> String {
    match term {
        Term::Var { name } if is_unqualified_local_name(name) => {
            format!("v:{local_scope}::{name}")
        }
        Term::Var { name } => format!("v:{name}"),
        Term::Const { value, .. } => match value {
            ConstValue::Int(value) => format!("i:{value}"),
            ConstValue::Real(value) => format!("r:{value}"),
            ConstValue::String(value) => format!("s:{value:?}"),
            ConstValue::Bool(value) => format!("b:{value}"),
        },
        Term::Ctor { name, args } => {
            let inner = args
                .iter()
                .map(|arg| canonical_method_arg_sig(arg, local_scope))
                .collect::<Vec<_>>()
                .join(",");
            format!("c:{name}({inner})")
        }
        _ => term_key(term),
    }
}

fn is_unqualified_local_name(name: &str) -> bool {
    !name.contains("::")
}

fn term_key(term: &Term) -> String {
    format!("{term:?}")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
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
            let method = match &call.turbofish {
                Some(args) => format!("{}{}", call.method, angle_args_key(args)),
                None => call.method.to_string(),
            };
            Ok(Rc::new(Term::Ctor {
                name: format!("method:{method}"),
                args,
            }))
        }
        Expr::Await(await_expr) => Ok(Rc::new(Term::Ctor {
            name: "await".to_string(),
            args: vec![translate_term(&await_expr.base)?],
        })),
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
        Lit::Float(f) => canonical_float_literal(f).map(real_const),
        Lit::Str(s) => Ok(str_const(s.value())),
        other => Err(format!(
            "only integer/string/finite decimal float scalar constants are liftable, got `{}`",
            token_key(other)
        )),
    }
}

fn canonical_float_literal(lit: &syn::LitFloat) -> Result<String, String> {
    let digits = lit.base10_digits().replace('_', "");
    if digits.contains('e') || digits.contains('E') {
        return Err(format!(
            "float literal with exponent is a refinement gap `{}`",
            lit.to_token_stream()
        ));
    }
    if digits.is_empty() {
        return Err("empty float literal".to_string());
    }
    Ok(digits)
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
        .map(|segment| {
            let mut name = segment.ident.to_string();
            name.push_str(&path_arguments_key(&segment.arguments));
            name
        })
        .collect::<Vec<_>>()
        .join("::")
}

fn path_arguments_key(arguments: &syn::PathArguments) -> String {
    match arguments {
        syn::PathArguments::None => String::new(),
        syn::PathArguments::AngleBracketed(args) => angle_args_key(args),
        syn::PathArguments::Parenthesized(args) => token_key(args),
    }
}

fn angle_args_key(args: &syn::AngleBracketedGenericArguments) -> String {
    let inner = args
        .args
        .iter()
        .map(generic_arg_key)
        .collect::<Vec<_>>()
        .join(",");
    format!("::<{inner}>")
}

fn generic_arg_key(arg: &syn::GenericArgument) -> String {
    match arg {
        syn::GenericArgument::Type(ty) => type_key(ty),
        syn::GenericArgument::Const(expr) => format!("const:{}", token_key(expr)),
        syn::GenericArgument::Lifetime(lifetime) => format!("'{}", lifetime.ident),
        syn::GenericArgument::AssocType(assoc) => {
            format!("{}={}", assoc.ident, type_key(&assoc.ty))
        }
        syn::GenericArgument::AssocConst(assoc) => {
            format!("{}=const:{}", assoc.ident, token_key(&assoc.value))
        }
        syn::GenericArgument::Constraint(constraint) => token_key(constraint),
        _ => token_key(arg),
    }
}

fn type_key(ty: &syn::Type) -> String {
    match ty {
        syn::Type::Path(path) => path_to_name(&path.path),
        syn::Type::Reference(reference) => {
            let mut out = String::from("&");
            if let Some(lifetime) = &reference.lifetime {
                out.push('\'');
                out.push_str(&lifetime.ident.to_string());
                out.push(' ');
            }
            if reference.mutability.is_some() {
                out.push_str("mut ");
            }
            out.push_str(&type_key(&reference.elem));
            out
        }
        syn::Type::Tuple(tuple) => {
            let inner = tuple
                .elems
                .iter()
                .map(type_key)
                .collect::<Vec<_>>()
                .join(",");
            format!("({inner})")
        }
        syn::Type::Array(array) => {
            format!("[{};{}]", type_key(&array.elem), token_key(&array.len))
        }
        syn::Type::Slice(slice) => format!("[{}]", type_key(&slice.elem)),
        _ => token_key(ty),
    }
}

fn token_key<T: ToTokens>(node: T) -> String {
    node.to_token_stream()
        .to_string()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}
