// SPDX-License-Identifier: Apache-2.0
//
// provekit-lift-proptest
//
// Walks the syn AST of a Rust source file looking for `proptest! { ... }`
// macro invocations. For each `#[test] fn name(params) { body }` inside,
// translates the assertion expression body to canonical IR.
//
// LIFTABLE SHAPES (v0 whitelist):
//
//   prop_assert!(<lhs> <binop> <rhs>)
//      -> atomic predicate (one of >, >=, <, <=, ==, !=)
//
//   prop_assert_eq!(<lhs>, <rhs>)
//      -> atomic eq
//
//   prop_assert_ne!(<lhs>, <rhs>)
//      -> atomic ne
//
// Each side of an atomic must be a Var (ident), a Literal (integer or
// string), or a single-arg call (treated as Ctor; e.g. `compute_cid(x)`).
// Anything else (method calls, field access, indexing, multi-arg
// non-ctor calls, complex nesting) is SKIPPED with a warning. This
// keeps the lattice clean: under-coverage is honest, polluting with
// unverifiable atoms is not.
//
// Each lifted test maps to a ContractDecl whose `post` (or `inv` when
// the test is purely universal over its parameters) is the lifted
// formula. When the proptest body wraps a `forall` over its parameters,
// we wrap with `forall(sort, |v| body)`. Sort inference for v0:
//
//   `i32` / `i64` / `u32` / `u64` / `usize` / `Vec<u8>`-shaped strategies
//      -> Int    (Vec<u8> we model as Int because the kit IR has no Bytes
//                 sort; document gap)
//   `String`           -> String
//   `bool`             -> Bool
//   anything else      -> Int (default, with warning)

use std::rc::Rc;

use provekit_ir_symbolic::{
    and_, atomic_, eq, gt, gte, lt, lte, make_var, ne, num, str_const, ContractDecl, Formula,
    Int, Sort, Term,
};

/// One warning emitted by the adapter. Returned alongside the lifted
/// decls so callers can surface it without a global logger.
#[derive(Debug, Clone)]
pub struct LiftWarning {
    pub source_path: String,
    pub item_name: String,
    pub reason: String,
}

/// Adapter result: lifted ContractDecls plus per-skip warnings.
#[derive(Debug, Default)]
pub struct AdapterOutput {
    pub decls: Vec<ContractDecl>,
    pub warnings: Vec<LiftWarning>,
    pub seen: usize,
    pub lifted: usize,
}

/// Walk a parsed syn::File for `proptest! { ... }` macro invocations
/// and lift each contained `#[test] fn` whose body matches the
/// whitelist.
pub fn lift_file(file: &syn::File, source_path: &str) -> AdapterOutput {
    let mut out = AdapterOutput::default();
    walk_items(&file.items, source_path, &mut out);
    out
}

fn walk_items(items: &[syn::Item], source_path: &str, out: &mut AdapterOutput) {
    for item in items {
        match item {
            syn::Item::Macro(m) => visit_macro(&m.mac, source_path, out),
            syn::Item::Mod(m) => {
                if let Some((_, items)) = &m.content {
                    walk_items(items, source_path, out);
                }
            }
            syn::Item::Fn(f) => {
                walk_block(&f.block, source_path, out);
            }
            _ => {}
        }
    }
}

fn walk_block(block: &syn::Block, source_path: &str, out: &mut AdapterOutput) {
    for stmt in &block.stmts {
        if let syn::Stmt::Macro(m) = stmt {
            visit_macro(&m.mac, source_path, out);
        }
        if let syn::Stmt::Item(item) = stmt {
            walk_items(std::slice::from_ref(item), source_path, out);
        }
    }
}

fn visit_macro(mac: &syn::Macro, source_path: &str, out: &mut AdapterOutput) {
    let path = path_to_string(&mac.path);
    if path != "proptest" {
        return;
    }
    let tokens = mac.tokens.clone();
    // The proptest! macro body is a sequence of items. We re-parse it
    // as a syn::File-like list of items (#![attr]* fn* ...).
    let parsed: Result<ProptestBody, _> = syn::parse2(tokens);
    let Ok(body) = parsed else {
        // The macro body wasn't pure items (it had a #![cfg] inner attr,
        // or other forms). v0 best-effort: try item-level walk on the
        // same tokens by retrying with a leading skip of inner attrs.
        let stripped = strip_inner_attrs(mac.tokens.clone());
        match syn::parse2::<ProptestBody>(stripped) {
            Ok(b) => visit_proptest_body(&b, source_path, out),
            Err(e) => {
                out.warnings.push(LiftWarning {
                    source_path: source_path.into(),
                    item_name: "<proptest>".into(),
                    reason: format!("could not parse proptest! body: {e}"),
                });
            }
        }
        return;
    };
    visit_proptest_body(&body, source_path, out);
}

struct ProptestBody {
    items: Vec<syn::Item>,
}

impl syn::parse::Parse for ProptestBody {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let mut items = Vec::new();
        // Skip any leading inner attributes (#![cfg], #![proptest_config]).
        while input.peek(syn::Token![#]) && input.peek2(syn::Token![!]) {
            let _: syn::Attribute = input.call(parse_inner_attr)?;
        }
        while !input.is_empty() {
            let item: syn::Item = input.parse()?;
            items.push(item);
        }
        Ok(ProptestBody { items })
    }
}

fn parse_inner_attr(input: syn::parse::ParseStream) -> syn::Result<syn::Attribute> {
    syn::Attribute::parse_inner(input).and_then(|v| {
        v.into_iter()
            .next()
            .ok_or_else(|| input.error("expected inner attribute"))
    })
}

fn strip_inner_attrs(tokens: proc_macro2::TokenStream) -> proc_macro2::TokenStream {
    // Drop leading `#! [...]` triplets. Best-effort fallback.
    let mut iter = tokens.into_iter().peekable();
    loop {
        let snapshot: Vec<proc_macro2::TokenTree> = iter.clone().collect();
        let mut probe = snapshot.iter().peekable();
        let p1 = probe.next();
        let p2 = probe.next();
        let p3 = probe.next();
        let is_inner = matches!(p1, Some(proc_macro2::TokenTree::Punct(p)) if p.as_char() == '#')
            && matches!(p2, Some(proc_macro2::TokenTree::Punct(p)) if p.as_char() == '!')
            && matches!(p3, Some(proc_macro2::TokenTree::Group(g)) if g.delimiter() == proc_macro2::Delimiter::Bracket);
        if !is_inner {
            break;
        }
        iter.next();
        iter.next();
        iter.next();
    }
    iter.collect()
}

fn visit_proptest_body(body: &ProptestBody, source_path: &str, out: &mut AdapterOutput) {
    for item in &body.items {
        if let syn::Item::Fn(f) = item {
            // Only accept items annotated #[test].
            if !has_test_attr(&f.attrs) {
                continue;
            }
            out.seen += 1;
            let name = f.sig.ident.to_string();
            match lift_test_fn(f) {
                Ok(decl) => {
                    out.decls.push(decl);
                    out.lifted += 1;
                }
                Err(reason) => out.warnings.push(LiftWarning {
                    source_path: source_path.into(),
                    item_name: name,
                    reason,
                }),
            }
        }
    }
}

fn has_test_attr(attrs: &[syn::Attribute]) -> bool {
    attrs.iter().any(|a| {
        let p = path_to_string(a.path());
        p == "test"
    })
}

/// Lift a single proptest #[test] function. Strategy: pull the
/// universally-quantified parameters from the signature (proptest uses
/// `name in strategy` in the param list, parsed as a normal sig). For
/// each `prop_assert*` macro invocation in the body, translate the
/// expression to a Formula. Combine multiple asserts via `and`. Wrap
/// the whole body in a `forall` per parameter.
fn lift_test_fn(f: &syn::ItemFn) -> Result<ContractDecl, String> {
    let name = f.sig.ident.to_string();

    // Collect parameters as (name, sort).
    let mut params: Vec<(String, Sort)> = Vec::new();
    for arg in &f.sig.inputs {
        if let syn::FnArg::Typed(pt) = arg {
            if let syn::Pat::Ident(pi) = &*pt.pat {
                let sort = sort_for_type(&pt.ty);
                params.push((pi.ident.to_string(), sort));
            }
        }
    }

    // Walk the function body for prop_assert* macro invocations.
    let mut atoms: Vec<Rc<Formula>> = Vec::new();
    for stmt in &f.block.stmts {
        if let syn::Stmt::Macro(sm) = stmt {
            if let Some(formula) = lift_assert_macro(&sm.mac)? {
                atoms.push(formula);
            }
        }
    }

    if atoms.is_empty() {
        return Err("no liftable prop_assert*! invocations in body".into());
    }

    let body_formula = if atoms.len() == 1 {
        atoms.into_iter().next().unwrap()
    } else {
        and_(atoms)
    };

    // Wrap with forall per parameter (right-fold so order matches
    // signature: outer = first param).
    let wrapped = if params.is_empty() {
        body_formula
    } else {
        wrap_forall(&params, 0, body_formula)
    };

    Ok(ContractDecl {
        name,
        pre: None,
        post: None,
        inv: Some(wrapped),
        out_binding: "out".into(),
    })
}

fn wrap_forall(params: &[(String, Sort)], i: usize, body: Rc<Formula>) -> Rc<Formula> {
    if i >= params.len() {
        return body;
    }
    let (pname, sort) = &params[i];
    let pname = pname.clone();
    let sort = sort.clone();
    let i_next = i + 1;
    let params = params.to_vec();
    // Build the inner body first, then construct the Quantifier
    // directly so the bound-var name is the original parameter
    // identifier (not the kit's `_x{counter}` placeholder, which
    // varies across runs and breaks content-addressed dedup).
    let inner = wrap_forall(&params, i_next, body);
    let inner_renamed = subst_var_name(&inner, &pname, &pname); // identity guard
    let _ = inner_renamed;
    Rc::new(Formula::Quantifier {
        kind: "forall".into(),
        name: pname,
        sort,
        body: inner,
    })
}


fn subst_var_name(f: &Rc<Formula>, from: &str, to: &str) -> Rc<Formula> {
    if from.is_empty() || from == to {
        return f.clone();
    }
    match &**f {
        Formula::Atomic { name, args } => {
            let new_args: Vec<Rc<Term>> = args.iter().map(|a| subst_term(a, from, to)).collect();
            atomic_(name.clone(), new_args)
        }
        Formula::Connective { kind, operands } => {
            let new_ops: Vec<Rc<Formula>> =
                operands.iter().map(|o| subst_var_name(o, from, to)).collect();
            Rc::new(Formula::Connective {
                kind: kind.clone(),
                operands: new_ops,
            })
        }
        Formula::Quantifier {
            kind,
            name,
            sort,
            body,
        } => {
            // Don't substitute if shadowed.
            if name == from {
                f.clone()
            } else {
                Rc::new(Formula::Quantifier {
                    kind: kind.clone(),
                    name: name.clone(),
                    sort: sort.clone(),
                    body: subst_var_name(body, from, to),
                })
            }
        }
    }
}

fn subst_term(t: &Rc<Term>, from: &str, to: &str) -> Rc<Term> {
    match &**t {
        Term::Var { name } if name == from => make_var(to),
        Term::Var { .. } => t.clone(),
        Term::Const { .. } => t.clone(),
        Term::Ctor { name, args } => {
            let new_args: Vec<Rc<Term>> = args.iter().map(|a| subst_term(a, from, to)).collect();
            Rc::new(Term::Ctor {
                name: name.clone(),
                args: new_args,
            })
        }
    }
}

/// Translate a `prop_assert*!` macro to a Formula, or Ok(None) if it's
/// not one we recognize. Errors when we recognize the shape but cannot
/// translate the inner expression.
fn lift_assert_macro(mac: &syn::Macro) -> Result<Option<Rc<Formula>>, String> {
    let path = path_to_string(&mac.path);
    match path.as_str() {
        "prop_assert" => {
            let expr: syn::Expr = syn::parse2(mac.tokens.clone())
                .map_err(|e| format!("prop_assert: parse expr: {e}"))?;
            translate_bool_expr(&expr).map(Some)
        }
        "prop_assert_eq" => {
            let pair: TwoExprs = syn::parse2(mac.tokens.clone())
                .map_err(|e| format!("prop_assert_eq: parse: {e}"))?;
            let l = translate_term(&pair.a)?;
            let r = translate_term(&pair.b)?;
            Ok(Some(eq(l, r)))
        }
        "prop_assert_ne" => {
            let pair: TwoExprs = syn::parse2(mac.tokens.clone())
                .map_err(|e| format!("prop_assert_ne: parse: {e}"))?;
            let l = translate_term(&pair.a)?;
            let r = translate_term(&pair.b)?;
            Ok(Some(ne(l, r)))
        }
        _ => Ok(None),
    }
}

struct TwoExprs {
    a: syn::Expr,
    b: syn::Expr,
}

impl syn::parse::Parse for TwoExprs {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let a: syn::Expr = input.parse()?;
        let _: syn::Token![,] = input.parse()?;
        let b: syn::Expr = input.parse()?;
        // Ignore any trailing format-args after a second comma.
        if input.peek(syn::Token![,]) {
            let _: syn::Token![,] = input.parse()?;
            // consume remaining tokens
            let _: proc_macro2::TokenStream = input.parse()?;
        }
        Ok(TwoExprs { a, b })
    }
}

fn translate_bool_expr(expr: &syn::Expr) -> Result<Rc<Formula>, String> {
    match expr {
        syn::Expr::Binary(b) => {
            let l = translate_term(&b.left)?;
            let r = translate_term(&b.right)?;
            match b.op {
                syn::BinOp::Gt(_) => Ok(gt(l, r)),
                syn::BinOp::Ge(_) => Ok(gte(l, r)),
                syn::BinOp::Lt(_) => Ok(lt(l, r)),
                syn::BinOp::Le(_) => Ok(lte(l, r)),
                syn::BinOp::Eq(_) => Ok(eq(l, r)),
                syn::BinOp::Ne(_) => Ok(ne(l, r)),
                _ => Err(format!("unsupported binop in prop_assert: {:?}", b.op)),
            }
        }
        syn::Expr::Paren(p) => translate_bool_expr(&p.expr),
        _ => Err("prop_assert body must be a comparison expression".into()),
    }
}

/// Term translation. Whitelist:
///   - identifier (Var)
///   - integer literal (num)
///   - string literal (str_const)
///   - call with single argument (Ctor with one arg)
fn translate_term(expr: &syn::Expr) -> Result<Rc<Term>, String> {
    match expr {
        syn::Expr::Path(p) => {
            if let Some(id) = p.path.get_ident() {
                Ok(make_var(id.to_string()))
            } else {
                Err(format!("path is not a simple identifier: {}", path_to_string(&p.path)))
            }
        }
        syn::Expr::Lit(l) => match &l.lit {
            syn::Lit::Int(li) => {
                let n: i64 = li
                    .base10_parse()
                    .map_err(|e| format!("integer literal: {e}"))?;
                Ok(num(n))
            }
            syn::Lit::Str(ls) => Ok(str_const(ls.value())),
            _ => Err("only integer and string literals are liftable in v0".into()),
        },
        syn::Expr::Paren(p) => translate_term(&p.expr),
        syn::Expr::Call(c) => {
            // Single-argument call: lift to Ctor with one argument.
            let callee = match &*c.func {
                syn::Expr::Path(p) => path_to_string(&p.path),
                _ => return Err("call target is not a simple path".into()),
            };
            if c.args.len() != 1 {
                return Err(format!(
                    "call `{callee}` with {} args is not liftable in v0 (single-arg only)",
                    c.args.len()
                ));
            }
            let inner = translate_term(c.args.first().unwrap())?;
            Ok(Rc::new(Term::Ctor {
                name: callee,
                args: vec![inner],
            }))
        }
        syn::Expr::Unary(u) => {
            // Allow unary -literal for integers.
            if matches!(u.op, syn::UnOp::Neg(_)) {
                if let syn::Expr::Lit(l) = &*u.expr {
                    if let syn::Lit::Int(li) = &l.lit {
                        let n: i64 = li
                            .base10_parse()
                            .map_err(|e| format!("integer literal: {e}"))?;
                        return Ok(num(-n));
                    }
                }
            }
            Err("unary expression not liftable".into())
        }
        _ => Err("expression shape not in v0 lift whitelist (no method calls, indexing, field access, multi-arg calls, complex nesting)".into()),
    }
}

fn sort_for_type(ty: &syn::Type) -> Sort {
    let s = type_to_string(ty);
    let s = s.trim();
    if s == "String" || s == "&str" || s == "str" {
        Sort::string()
    } else if s == "bool" {
        Sort::bool()
    } else if s.starts_with("i") || s.starts_with("u") || s == "usize" || s == "isize" {
        Int()
    } else {
        // Default to Int with no warning at this point; callers may
        // log gap separately.
        Int()
    }
}

fn type_to_string(ty: &syn::Type) -> String {
    use quote::ToTokens;
    let mut s = String::new();
    ty.to_tokens(&mut proc_macro2::TokenStream::new());
    let mut ts = proc_macro2::TokenStream::new();
    ty.to_tokens(&mut ts);
    s.push_str(&ts.to_string());
    s
}

fn path_to_string(p: &syn::Path) -> String {
    let mut s = String::new();
    for (i, seg) in p.segments.iter().enumerate() {
        if i > 0 {
            s.push_str("::");
        }
        s.push_str(&seg.ident.to_string());
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(src: &str) -> syn::File {
        syn::parse_file(src).unwrap()
    }

    #[test]
    fn lifts_simple_prop_assert_eq() {
        let src = r#"
            proptest! {
                #[test]
                fn answer_is_42(x: i64) {
                    prop_assert_eq!(x, 42);
                }
            }
        "#;
        let f = parse(src);
        let out = lift_file(&f, "test.rs");
        assert_eq!(out.lifted, 1, "warnings: {:?}", out.warnings);
        assert_eq!(out.decls.len(), 1);
        assert_eq!(out.decls[0].name, "answer_is_42");
        assert!(out.decls[0].inv.is_some());
    }

    #[test]
    fn lifts_prop_assert_with_binop() {
        let src = r#"
            proptest! {
                #[test]
                fn nonneg(x: i64) {
                    prop_assert!(x >= 0);
                }
            }
        "#;
        let f = parse(src);
        let out = lift_file(&f, "test.rs");
        assert_eq!(out.lifted, 1, "warnings: {:?}", out.warnings);
    }

    #[test]
    fn skips_method_call_with_warning() {
        let src = r#"
            proptest! {
                #[test]
                fn uses_method(s: String) {
                    prop_assert_eq!(s.len(), 0);
                }
            }
        "#;
        let f = parse(src);
        let out = lift_file(&f, "test.rs");
        assert_eq!(out.lifted, 0);
        assert_eq!(out.warnings.len(), 1);
        assert!(out.warnings[0].reason.contains("not in v0 lift whitelist") || out.warnings[0].reason.contains("liftable"));
    }

    #[test]
    fn lifts_single_arg_call_as_ctor() {
        let src = r#"
            proptest! {
                #[test]
                fn cid_len(bytes: i64) {
                    prop_assert_eq!(compute_cid(bytes), 139);
                }
            }
        "#;
        let f = parse(src);
        let out = lift_file(&f, "test.rs");
        assert_eq!(out.lifted, 1, "warnings: {:?}", out.warnings);
    }
}
