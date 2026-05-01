// SPDX-License-Identifier: Apache-2.0
//
// provekit-lift-creusot
//
// STRATEGIC POSITIONING
//
// ProvekIt consumes creusot's existing annotations; we sit beneath, not
// against. Creusot (https://github.com/creusot-rs/creusot) is a Rust
// verifier targeting Why3. It exposes:
//   #[requires(cond)] / #[ensures(cond)] / #[invariant(cond)]
//   #[predicate] / #[law] / #[trusted] / #[variant(expr)]
//
// V0 PRIORITY
//
// We recognize ONLY the namespaced forms `creusot::requires` /
// `creusot::ensures` / `creusot::invariant` (and the
// `creusot_contracts::` alias used by current upstream). The unprefixed
// `#[requires]` shape is intentionally left to provekit-lift-contracts.
//
// In #[ensures], creusot uses `result` for the return value (same as
// prusti). We rewrite `result` to `out` to line up with the contract
// envelope's outBinding.
//
// Skipped with logged warnings:
//   - `#[predicate]`, `#[law]`, `#[trusted]`
//   - `#[variant(...)]` (termination measure; v0 doesn't model)
//   - Anything outside the v0 binop whitelist.
//
// V0 WHITELIST (same as proptest/contracts):
//   <var|lit|single-arg-call> <binop> <var|lit|single-arg-call>
// where binop is one of >, >=, <, <=, ==, !=.

use std::rc::Rc;

use provekit_ir_symbolic::{
    and_, atomic_, eq, gt, gte, lt, lte, make_var, ne, num, str_const, ContractDecl, Formula,
    Int, Sort, Term,
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

#[derive(Copy, Clone, Debug)]
enum Slot {
    Pre,
    Post,
    Inv,
}

fn classify_attr(a: &syn::Attribute) -> Option<Slot> {
    let p = path_to_string(a.path());
    match p.as_str() {
        "creusot::requires" | "creusot_contracts::requires" => Some(Slot::Pre),
        "creusot::ensures" | "creusot_contracts::ensures" => Some(Slot::Post),
        "creusot::invariant" | "creusot_contracts::invariant" => Some(Slot::Inv),
        _ => None,
    }
}

fn classify_skip_marker(a: &syn::Attribute) -> Option<&'static str> {
    let p = path_to_string(a.path());
    match p.as_str() {
        "creusot::predicate" | "creusot_contracts::predicate" => Some("predicate"),
        "creusot::law" | "creusot_contracts::law" => Some("law"),
        "creusot::trusted" | "creusot_contracts::trusted" => Some("trusted"),
        "creusot::variant" | "creusot_contracts::variant" => Some("variant"),
        _ => None,
    }
}

fn visit_fn(f: &syn::ItemFn, source_path: &str, out: &mut AdapterOutput) {
    let attrs = &f.attrs;
    let name = f.sig.ident.to_string();
    let has_ctr = attrs.iter().any(|a| classify_attr(a).is_some());
    let has_skip = attrs.iter().any(|a| classify_skip_marker(a).is_some());
    if !has_ctr && !has_skip {
        return;
    }
    if has_skip && !has_ctr {
        for a in attrs {
            if let Some(marker) = classify_skip_marker(a) {
                out.seen += 1;
                out.warnings.push(LiftWarning {
                    source_path: source_path.into(),
                    item_name: name.clone(),
                    reason: format!("v0 creusot adapter skips #[creusot::{marker}] (revisit in v1.x)"),
                });
            }
        }
        return;
    }
    out.seen += 1;
    process(name, attrs, &f.sig, source_path, out);
}

fn visit_impl_fn(f: &syn::ImplItemFn, source_path: &str, out: &mut AdapterOutput) {
    let attrs = &f.attrs;
    let name = f.sig.ident.to_string();
    let has_ctr = attrs.iter().any(|a| classify_attr(a).is_some());
    let has_skip = attrs.iter().any(|a| classify_skip_marker(a).is_some());
    if !has_ctr && !has_skip {
        return;
    }
    if has_skip && !has_ctr {
        for a in attrs {
            if let Some(marker) = classify_skip_marker(a) {
                out.seen += 1;
                out.warnings.push(LiftWarning {
                    source_path: source_path.into(),
                    item_name: name.clone(),
                    reason: format!("v0 creusot adapter skips #[creusot::{marker}] (revisit in v1.x)"),
                });
            }
        }
        return;
    }
    out.seen += 1;
    process(name, attrs, &f.sig, source_path, out);
}

fn process(
    name: String,
    attrs: &[syn::Attribute],
    sig: &syn::Signature,
    source_path: &str,
    out: &mut AdapterOutput,
) {
    let mut params: Vec<(String, Sort)> = Vec::new();
    for arg in &sig.inputs {
        if let syn::FnArg::Typed(pt) = arg {
            if let syn::Pat::Ident(pi) = &*pt.pat {
                params.push((pi.ident.to_string(), sort_for_type(&pt.ty)));
            }
        }
    }

    let mut pre_atoms: Vec<Rc<Formula>> = Vec::new();
    let mut post_atoms: Vec<Rc<Formula>> = Vec::new();
    let mut inv_atoms: Vec<Rc<Formula>> = Vec::new();
    let mut had_failure = false;

    for a in attrs {
        let Some(slot) = classify_attr(a) else {
            continue;
        };
        let expr = match a.parse_args::<syn::Expr>() {
            Ok(e) => e,
            Err(e) => {
                out.warnings.push(LiftWarning {
                    source_path: source_path.into(),
                    item_name: name.clone(),
                    reason: format!("parse attr arg: {e}"),
                });
                had_failure = true;
                continue;
            }
        };
        match translate_bool_expr(&expr) {
            Ok(f) => match slot {
                Slot::Pre => pre_atoms.push(f),
                Slot::Post => post_atoms.push(subst_var_name(&f, "result", "out")),
                Slot::Inv => inv_atoms.push(f),
            },
            Err(reason) => {
                out.warnings.push(LiftWarning {
                    source_path: source_path.into(),
                    item_name: name.clone(),
                    reason,
                });
                had_failure = true;
            }
        }
    }

    if pre_atoms.is_empty() && post_atoms.is_empty() && inv_atoms.is_empty() {
        if !had_failure {
            out.warnings.push(LiftWarning {
                source_path: source_path.into(),
                item_name: name,
                reason: "no liftable creusot contract attrs".into(),
            });
        }
        return;
    }

    let pre = combine(pre_atoms).map(|f| wrap_forall(&params, 0, f));
    let post = combine(post_atoms).map(|f| wrap_forall(&params, 0, f));
    let inv = combine(inv_atoms).map(|f| wrap_forall(&params, 0, f));

    out.decls.push(ContractDecl {
        name,
        pre,
        post,
        inv,
        out_binding: "out".into(),
    });
    out.lifted += 1;
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
            args.iter().map(|a| subst_term(a, from, to)).collect::<Vec<_>>(),
        ),
        Formula::Connective { kind, operands } => Rc::new(Formula::Connective {
            kind: kind.clone(),
            operands: operands.iter().map(|o| subst_var_name(o, from, to)).collect(),
        }),
        Formula::Quantifier { kind, name, sort, body } => {
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
        Term::Ctor { name, args } => Rc::new(Term::Ctor {
            name: name.clone(),
            args: args.iter().map(|a| subst_term(a, from, to)).collect(),
        }),
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
                _ => Err(format!("unsupported binop in creusot contract: {:?}", b.op)),
            }
        }
        syn::Expr::Paren(p) => translate_bool_expr(&p.expr),
        _ => Err("creusot contract expression must be a comparison".into()),
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
                let n: i64 = li.base10_parse().map_err(|e| format!("integer literal: {e}"))?;
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
            Ok(Rc::new(Term::Ctor { name: callee, args: vec![inner] }))
        }
        syn::Expr::Unary(u) => {
            if matches!(u.op, syn::UnOp::Neg(_)) {
                if let syn::Expr::Lit(l) = &*u.expr {
                    if let syn::Lit::Int(li) = &l.lit {
                        let n: i64 = li.base10_parse().map_err(|e| format!("integer literal: {e}"))?;
                        return Ok(num(-n));
                    }
                }
            }
            Err("unary expression not liftable".into())
        }
        _ => Err("expression shape not in v0 lift whitelist".into()),
    }
}

fn sort_for_type(ty: &syn::Type) -> Sort {
    use quote::ToTokens;
    let mut ts = proc_macro2::TokenStream::new();
    ty.to_tokens(&mut ts);
    let s = ts.to_string();
    let s = s.trim();
    if s == "String" || s == "& str" || s == "str" {
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
    fn lifts_namespaced_requires_and_ensures() {
        let src = r#"
            #[creusot::requires(x > 0)]
            #[creusot::ensures(result >= 0)]
            fn sqrt(x: i64) -> i64 { x }
        "#;
        let f = parse(src);
        let out = lift_file(&f, "test.rs");
        assert_eq!(out.lifted, 1, "warnings: {:?}", out.warnings);
        let d = &out.decls[0];
        assert!(d.pre.is_some());
        assert!(d.post.is_some());
    }

    #[test]
    fn ignores_bare_requires() {
        let src = r#"
            #[requires(x > 0)]
            fn f(x: i64) -> i64 { x }
        "#;
        let f = parse(src);
        let out = lift_file(&f, "test.rs");
        assert_eq!(out.lifted, 0);
        assert_eq!(out.seen, 0);
    }

    #[test]
    fn skips_law_with_warning() {
        let src = r#"
            #[creusot::law]
            fn lemma() {}
        "#;
        let f = parse(src);
        let out = lift_file(&f, "test.rs");
        assert_eq!(out.lifted, 0);
        assert!(out.warnings[0].reason.contains("law"));
    }

    #[test]
    fn lifts_creusot_contracts_alias() {
        let src = r#"
            #[creusot_contracts::requires(n >= 0)]
            fn f(n: i64) -> i64 { n }
        "#;
        let f = parse(src);
        let out = lift_file(&f, "test.rs");
        assert_eq!(out.lifted, 1, "warnings: {:?}", out.warnings);
    }
}
