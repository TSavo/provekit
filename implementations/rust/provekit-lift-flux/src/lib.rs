// SPDX-License-Identifier: Apache-2.0
//
// provekit-lift-flux
//
// STRATEGIC POSITIONING
//
// ProvekIt consumes flux's existing annotations; we sit beneath, not
// against. Flux (https://flux-rs.github.io/flux/) is a refinement-type
// checker for Rust. Annotations look like:
//
//   #[flux::sig(fn(x: i32{x > 0}) -> i32{r: r >= 0})]
//   fn double(x: i32) -> i32 { x + x }
//
// V0 SHAPE
//
// We walk every `#[flux::sig(...)]` (and `#[flux_rs::sig(...)]`) on a
// function. The attribute body is NOT a valid `syn::Expr` (refinement
// braces and `r:` qualifier are flux's own syntax), so we walk the
// TokenStream by hand. For each `<name>: <ty>{<refinement>}` argument,
// we emit a pre-condition. For a `-> <ty>{<binder>: <refinement>}`
// return refinement, we emit a post-condition that uses the binder.
// Refinements without an explicit binder are interpreted with the
// argument-name (or "out" for return) as the implicit subject.
//
// V0 WHITELIST
//
//   - Refinement body must be a binop comparison whose operands are
//     vars (idents), integer literals, or single-arg calls.
//   - Argument types are integer-shaped (i8/i16/i32/i64/u8.../usize).
//   - Tuple/list/set refinements skip with a warning.
//
// All other flux attribute kinds (`#[flux::refined_by(...)]`,
// `#[flux::trusted]`, etc.) skip with a warning.

use std::rc::Rc;

use provekit_ir_symbolic::{
    and_, atomic_, eq, gt, gte, lt, lte, make_var, ne, num, str_const, ContractDecl, Formula, Int,
    Sort, Term,
};

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
    walk_items(&file.items, source_path, &mut out);
    out
}

fn walk_items(items: &[syn::Item], source_path: &str, out: &mut AdapterOutput) {
    for item in items {
        match item {
            syn::Item::Fn(f) => visit_fn(f, source_path, out),
            syn::Item::Mod(m) => {
                if let Some((_, items)) = &m.content {
                    walk_items(items, source_path, out);
                }
            }
            syn::Item::Impl(i) => {
                for it in &i.items {
                    if let syn::ImplItem::Fn(f) = it {
                        visit_impl_fn(f, source_path, out);
                    }
                }
            }
            _ => {}
        }
    }
}

fn visit_fn(f: &syn::ItemFn, source_path: &str, out: &mut AdapterOutput) {
    let attrs = &f.attrs;
    let name = f.sig.ident.to_string();
    if !any_flux_attr(attrs) {
        return;
    }
    out.seen += 1;
    process(name, attrs, source_path, out);
}

fn visit_impl_fn(f: &syn::ImplItemFn, source_path: &str, out: &mut AdapterOutput) {
    let attrs = &f.attrs;
    let name = f.sig.ident.to_string();
    if !any_flux_attr(attrs) {
        return;
    }
    out.seen += 1;
    process(name, attrs, source_path, out);
}

fn any_flux_attr(attrs: &[syn::Attribute]) -> bool {
    attrs.iter().any(|a| {
        let p = path_to_string(a.path());
        p.starts_with("flux::") || p.starts_with("flux_rs::")
    })
}

fn process(name: String, attrs: &[syn::Attribute], source_path: &str, out: &mut AdapterOutput) {
    // We only translate `flux::sig` / `flux_rs::sig`. Other flux attrs
    // skip with a warning.
    let mut sig_tokens: Option<proc_macro2::TokenStream> = None;
    for a in attrs {
        let p = path_to_string(a.path());
        if p == "flux::sig" || p == "flux_rs::sig" {
            // The attribute is `#[flux::sig(...)]`; we want the `...`.
            sig_tokens = match &a.meta {
                syn::Meta::List(l) => Some(l.tokens.clone()),
                _ => None,
            };
        } else if p.starts_with("flux::") || p.starts_with("flux_rs::") {
            out.warnings.push(LiftWarning {
                source_path: source_path.into(),
                item_name: name.clone(),
                reason: format!("v0 flux adapter only translates #[flux::sig]; skipped #[{p}]"),
            });
        }
    }
    let Some(tokens) = sig_tokens else {
        out.warnings.push(LiftWarning {
            source_path: source_path.into(),
            item_name: name,
            reason: "no #[flux::sig] attribute on item".into(),
        });
        return;
    };

    let parsed = match parse_flux_sig(tokens) {
        Ok(p) => p,
        Err(reason) => {
            out.warnings.push(LiftWarning {
                source_path: source_path.into(),
                item_name: name,
                reason,
            });
            return;
        }
    };

    let mut params: Vec<(String, Sort)> = Vec::new();
    let mut pre_atoms: Vec<Rc<Formula>> = Vec::new();
    let mut had_failure = false;
    for arg in &parsed.args {
        params.push((arg.name.clone(), arg.sort.clone()));
        if let Some(ref_src) = &arg.refinement {
            // The refinement is an expression over `arg.name` (the
            // implicit subject). Parse it as an Expr and translate.
            match parse_and_translate(ref_src) {
                Ok(f) => pre_atoms.push(f),
                Err(reason) => {
                    out.warnings.push(LiftWarning {
                        source_path: source_path.into(),
                        item_name: name.clone(),
                        reason: format!("arg `{}`: {reason}", arg.name),
                    });
                    had_failure = true;
                }
            }
        }
    }

    let mut post_atoms: Vec<Rc<Formula>> = Vec::new();
    if let Some(ret) = &parsed.ret {
        // Return refinement source uses either an explicit binder
        // (e.g. `r: r >= 0`) or an implicit `out` subject. We rewrite
        // the binder to "out" so it lines up with the envelope's
        // outBinding.
        let (binder, body) = split_binder(&ret.refinement_src);
        let translated = parse_and_translate(&body);
        match translated {
            Ok(formula) => {
                let formula = match binder {
                    Some(b) if b != "out" => subst_var_name(&formula, &b, "out"),
                    _ => formula,
                };
                post_atoms.push(formula);
            }
            Err(reason) => {
                out.warnings.push(LiftWarning {
                    source_path: source_path.into(),
                    item_name: name.clone(),
                    reason: format!("return refinement: {reason}"),
                });
                had_failure = true;
            }
        }
    }

    if pre_atoms.is_empty() && post_atoms.is_empty() {
        if !had_failure {
            out.warnings.push(LiftWarning {
                source_path: source_path.into(),
                item_name: name,
                reason: "no liftable refinements in #[flux::sig]".into(),
            });
        }
        return;
    }

    let pre = combine(pre_atoms).map(|f| wrap_forall(&params, 0, f));
    let post = combine(post_atoms).map(|f| wrap_forall(&params, 0, f));

    out.decls.push(ContractDecl {
        name,
        pre,
        post,
        inv: None,
        out_binding: "out".into(),
        evidence: None,
        panic_loci: Vec::new(),
        concept_hint: None,
    });
    out.lifted += 1;
}

/// Hand-parsed flux signature.
#[derive(Debug)]
struct FluxSig {
    args: Vec<FluxArg>,
    ret: Option<FluxRet>,
}

#[derive(Debug)]
struct FluxArg {
    name: String,
    sort: Sort,
    refinement: Option<String>,
}

#[derive(Debug)]
struct FluxRet {
    refinement_src: String,
}

/// Parse `fn(<arg>, <arg>) -> <ret>` where each arg is
/// `<name>: <ty>` or `<name>: <ty>{<refinement>}` and ret is
/// `<ty>` or `<ty>{<binder>: <refinement>}` or `<ty>{<refinement>}`.
///
/// We walk the TokenStream looking for `fn (...) -> {...}` shape.
fn parse_flux_sig(tokens: proc_macro2::TokenStream) -> Result<FluxSig, String> {
    let trees: Vec<proc_macro2::TokenTree> = tokens.into_iter().collect();
    // Expect `fn`, then a Group(parens) for args, then `->`, then ret.
    let mut iter = trees.into_iter().peekable();
    let first = iter.next().ok_or_else(|| "empty flux sig".to_string())?;
    match &first {
        proc_macro2::TokenTree::Ident(id) if id.to_string() == "fn" => {}
        _ => return Err("expected `fn` keyword at start of flux sig".into()),
    }
    let args_group = match iter.next() {
        Some(proc_macro2::TokenTree::Group(g))
            if g.delimiter() == proc_macro2::Delimiter::Parenthesis =>
        {
            g
        }
        _ => return Err("expected `(...)` arg list after `fn`".into()),
    };

    let args = parse_flux_args(args_group.stream())?;

    // Optional `-> <ret>`.
    let mut ret: Option<FluxRet> = None;
    let next1 = iter.next();
    let next2 = iter.next();
    match (next1, next2) {
        (Some(proc_macro2::TokenTree::Punct(p1)), Some(proc_macro2::TokenTree::Punct(p2)))
            if p1.as_char() == '-' && p2.as_char() == '>' =>
        {
            // Collect the rest as ret tokens; find `{...}` in there.
            let rest: Vec<proc_macro2::TokenTree> = iter.collect();
            ret = parse_flux_ret(rest)?;
        }
        (None, None) => {}
        _ => {
            // Either a stray punct or an unknown shape; ignore.
        }
    }

    Ok(FluxSig { args, ret })
}

fn parse_flux_args(ts: proc_macro2::TokenStream) -> Result<Vec<FluxArg>, String> {
    // Split at top-level commas.
    let mut groups: Vec<Vec<proc_macro2::TokenTree>> = vec![Vec::new()];
    for tt in ts {
        match tt {
            proc_macro2::TokenTree::Punct(ref p) if p.as_char() == ',' => {
                groups.push(Vec::new());
            }
            other => groups.last_mut().unwrap().push(other),
        }
    }
    let mut args: Vec<FluxArg> = Vec::new();
    for g in groups {
        if g.is_empty() {
            continue;
        }
        // Expect `<ident> : <ty>` possibly with `{<ref>}` trailing.
        let mut it = g.into_iter().peekable();
        let name_tt = it.next().ok_or_else(|| "empty arg".to_string())?;
        let name = match name_tt {
            proc_macro2::TokenTree::Ident(id) => id.to_string(),
            _ => return Err("arg must start with an identifier".into()),
        };
        match it.next() {
            Some(proc_macro2::TokenTree::Punct(p)) if p.as_char() == ':' => {}
            _ => return Err(format!("expected `:` after arg `{name}`")),
        }
        // Type token (single ident, e.g. `i32`). We don't model paths.
        let ty_tt = it
            .next()
            .ok_or_else(|| format!("missing type for `{name}`"))?;
        let ty = match ty_tt {
            proc_macro2::TokenTree::Ident(id) => id.to_string(),
            _ => return Err(format!("arg `{name}`: type must be a simple ident in v0")),
        };
        let sort = sort_for_type_name(&ty);
        // Optional `{ ... }` refinement group.
        let refinement = match it.next() {
            Some(proc_macro2::TokenTree::Group(g))
                if g.delimiter() == proc_macro2::Delimiter::Brace =>
            {
                Some(g.stream().to_string())
            }
            _ => None,
        };
        args.push(FluxArg {
            name,
            sort,
            refinement,
        });
    }
    Ok(args)
}

fn parse_flux_ret(rest: Vec<proc_macro2::TokenTree>) -> Result<Option<FluxRet>, String> {
    // We accept `<ty>` (no refinement; nothing to lift) or `<ty>{<ref>}`.
    let mut it = rest.into_iter();
    let _ty = match it.next() {
        Some(proc_macro2::TokenTree::Ident(id)) => id.to_string(),
        Some(_) => return Err("return type must start with a simple ident in v0".into()),
        None => return Ok(None),
    };
    if let Some(proc_macro2::TokenTree::Group(g)) = it.next() {
        if g.delimiter() == proc_macro2::Delimiter::Brace {
            return Ok(Some(FluxRet {
                refinement_src: g.stream().to_string(),
            }));
        }
    }
    Ok(None)
}

/// Split `<binder>: <body>` into (Some(binder), body). If no binder is
/// present, returns (None, src).
fn split_binder(src: &str) -> (Option<String>, String) {
    // Look for the first `:` not inside parens. We require LHS to be a
    // simple ident.
    let s = src.trim();
    if let Some(idx) = s.find(':') {
        let (lhs, rhs) = s.split_at(idx);
        let lhs = lhs.trim();
        if !lhs.is_empty() && lhs.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
            // Skip the `:` character itself.
            let rhs = rhs.trim_start_matches(':').trim();
            return (Some(lhs.to_string()), rhs.to_string());
        }
    }
    (None, s.to_string())
}

fn parse_and_translate(src: &str) -> Result<Rc<Formula>, String> {
    let expr: syn::Expr = syn::parse_str(src).map_err(|e| format!("parse refinement: {e}"))?;
    translate_bool_expr(&expr)
}

fn combine(mut atoms: Vec<Rc<Formula>>) -> Option<Rc<Formula>> {
    if atoms.is_empty() {
        None
    } else if atoms.len() == 1 {
        Some(atoms.pop().unwrap())
    } else {
        Some(and_(atoms))
    }
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
    let inner = wrap_forall(&params, i_next, body);
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
        Formula::Atomic { name, args } => atomic_(
            name.clone(),
            args.iter()
                .map(|a| subst_term(a, from, to))
                .collect::<Vec<_>>(),
        ),
        Formula::Connective { kind, operands } => Rc::new(Formula::Connective {
            kind: kind.clone(),
            operands: operands
                .iter()
                .map(|o| subst_var_name(o, from, to))
                .collect(),
        }),
        Formula::Quantifier {
            kind,
            name,
            sort,
            body,
        } => {
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
        Formula::Choice {
            var_name,
            sort,
            body,
        } => {
            if var_name == from {
                f.clone() // shadowed
            } else {
                Rc::new(Formula::Choice {
                    var_name: var_name.clone(),
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
        Term::Ctor { name, args } => Rc::new(Term::Ctor {
            name: name.clone(),
            args: args.iter().map(|a| subst_term(a, from, to)).collect(),
        }),
        Term::Lambda {
            param_name,
            param_sort,
            body,
        } => {
            if param_name == from {
                t.clone() // shadowed
            } else {
                Rc::new(Term::Lambda {
                    param_name: param_name.clone(),
                    param_sort: param_sort.clone(),
                    body: subst_term(body, from, to),
                })
            }
        }
        Term::Let { bindings, body } => {
            let mut new_bindings = Vec::new();
            let mut shadowed = false;
            for b in bindings {
                if !shadowed {
                    new_bindings.push(provekit_ir_symbolic::LetBinding {
                        name: b.name.clone(),
                        bound_term: subst_term(&b.bound_term, from, to),
                    });
                    if b.name == from {
                        shadowed = true;
                    }
                } else {
                    new_bindings.push(provekit_ir_symbolic::LetBinding {
                        name: b.name.clone(),
                        bound_term: b.bound_term.clone(),
                    });
                }
            }
            let new_body = if shadowed {
                body.clone()
            } else {
                subst_term(body, from, to)
            };
            Rc::new(Term::Let {
                bindings: new_bindings,
                body: new_body,
            })
        }
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
                _ => Err(format!("unsupported binop in flux refinement: {:?}", b.op)),
            }
        }
        syn::Expr::Paren(p) => translate_bool_expr(&p.expr),
        _ => Err("flux refinement must be a comparison".into()),
    }
}

fn translate_term(expr: &syn::Expr) -> Result<Rc<Term>, String> {
    match expr {
        syn::Expr::Path(p) => {
            if let Some(id) = p.path.get_ident() {
                Ok(make_var(id.to_string()))
            } else {
                Err("path is not a simple identifier".into())
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
        _ => Err("expression shape not in v0 lift whitelist".into()),
    }
}

fn sort_for_type_name(s: &str) -> Sort {
    if s == "String" || s == "str" {
        Sort::string()
    } else if s == "bool" {
        Sort::bool()
    } else if s == "f32" || s == "f64" {
        Sort::real()
    } else {
        Int()
    }
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
    fn lifts_arg_refinement() {
        let src = r#"
            #[flux::sig(fn(x: i32{x > 0}) -> i32)]
            fn pos(x: i32) -> i32 { x }
        "#;
        let f = parse(src);
        let out = lift_file(&f, "test.rs");
        assert_eq!(out.lifted, 1, "warnings: {:?}", out.warnings);
        let d = &out.decls[0];
        assert!(d.pre.is_some());
        assert!(d.post.is_none());
    }

    #[test]
    fn lifts_return_refinement_with_binder() {
        let src = r#"
            #[flux::sig(fn(x: i32) -> i32{r: r >= 0})]
            fn nonneg(x: i32) -> i32 { 0 }
        "#;
        let f = parse(src);
        let out = lift_file(&f, "test.rs");
        assert_eq!(out.lifted, 1, "warnings: {:?}", out.warnings);
        assert!(out.decls[0].post.is_some());
    }

    #[test]
    fn lifts_combined_pre_and_post() {
        let src = r#"
            #[flux::sig(fn(x: i32{x > 0}) -> i32{r: r >= 0})]
            fn sqrt(x: i32) -> i32 { x }
        "#;
        let f = parse(src);
        let out = lift_file(&f, "test.rs");
        assert_eq!(out.lifted, 1, "warnings: {:?}", out.warnings);
        let d = &out.decls[0];
        assert!(d.pre.is_some() && d.post.is_some());
    }

    #[test]
    fn skips_non_sig_flux_attrs() {
        let src = r#"
            #[flux::trusted]
            fn t() {}
        "#;
        let f = parse(src);
        let out = lift_file(&f, "test.rs");
        assert_eq!(out.lifted, 0);
        assert!(out.warnings.iter().any(|w| w.reason.contains("trusted")));
    }
}
