// SPDX-License-Identifier: Apache-2.0
//
// provekit-lift-kani
//
// Walks the syn AST of a Rust source file looking for functions with
// Kani attribute macros (https://model-checking.github.io/kani/) and
// translates each predicate to canonical IR. Each annotated function
// becomes one `ContractDecl`.
//
// STRATEGIC POSITIONING
//
//   ProvekIt does not compete with Kani. Kani's bounded model checker
//   is the verifier; ProvekIt sits BENEATH it. Developers keep their
//   `#[kani::requires]` / `#[kani::ensures]` annotations exactly as
//   they are. This adapter reads what is already there and promotes
//   each predicate to a content-addressed signed contract memento.
//
// RECOGNIZED ATTRIBUTES (v0)
//
//   #[kani::requires(<expr>)]      -> lifted as `pre`
//   #[kani::ensures(<expr>)]       -> lifted as `post`
//
// SKIPPED WITH WARNING (v0)
//
//   #[kani::should_panic]          -> negation marker; v1 will lift to
//                                     an `inv` flagged with a kit-defined
//                                     `should_panic` ctor. v0 logs+skip.
//   #[kani::proof]                 -> entry-point marker, not a contract.
//   #[kani::unwind(N)]             -> bound, not a contract.
//
// LIFTABLE PREDICATE SHAPE (v0 whitelist; same as contracts/proptest):
//
//   <var|lit|single-arg-call> <binop> <var|lit|single-arg-call>
//   where binop is one of >, >=, <, <=, ==, !=.
//
// Anything outside the whitelist (method calls, field access, indexing,
// multi-arg calls, complex nesting, logical connectives) is SKIPPED
// with a warning. Under-coverage is honest; polluting the lattice with
// unverifiable atoms is not.
//
// KANI'S `result` BINDING
//
//   In Kani, `#[kani::ensures(result > 0)]` uses the identifier
//   `result` for the return value. ProvekIt's canonical out-binding
//   default is `"out"`. We rewrite every reference to `result` inside
//   ensures to `out()` so the lifted IR is uniform with other adapters
//   and the verifier renders predicates against the contract's
//   `out_binding`.

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

fn visit_fn(f: &syn::ItemFn, source_path: &str, out: &mut AdapterOutput) {
    let attrs = &f.attrs;
    let name = f.sig.ident.to_string();
    if !any_kani_attr(attrs) {
        return;
    }
    out.seen += 1;
    process(name, attrs, &f.sig, source_path, out);
}

fn visit_impl_fn(f: &syn::ImplItemFn, source_path: &str, out: &mut AdapterOutput) {
    let attrs = &f.attrs;
    let name = f.sig.ident.to_string();
    if !any_kani_attr(attrs) {
        return;
    }
    out.seen += 1;
    process(name, attrs, &f.sig, source_path, out);
}

fn any_kani_attr(attrs: &[syn::Attribute]) -> bool {
    attrs.iter().any(|a| {
        let p = path_to_string(a.path());
        p.starts_with("kani::") || p == "kani"
    })
}

#[derive(Copy, Clone, Debug)]
enum Slot {
    Pre,
    Post,
}

#[derive(Copy, Clone, Debug)]
enum AttrKind {
    Predicate(Slot),
    SkippedShouldPanic,
    SkippedProof,
    SkippedUnwind,
    SkippedOther,
}

fn classify_attr(a: &syn::Attribute) -> Option<AttrKind> {
    let p = path_to_string(a.path());
    match p.as_str() {
        "kani::requires" => Some(AttrKind::Predicate(Slot::Pre)),
        "kani::ensures" => Some(AttrKind::Predicate(Slot::Post)),
        "kani::should_panic" => Some(AttrKind::SkippedShouldPanic),
        "kani::proof" => Some(AttrKind::SkippedProof),
        "kani::unwind" => Some(AttrKind::SkippedUnwind),
        _ => {
            if p.starts_with("kani::") {
                Some(AttrKind::SkippedOther)
            } else {
                None
            }
        }
    }
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
    let mut had_failure = false;

    for a in attrs {
        let Some(kind) = classify_attr(a) else {
            continue;
        };
        match kind {
            AttrKind::Predicate(slot) => {
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
                let in_post = matches!(slot, Slot::Post);
                match translate_bool_expr(&expr, in_post) {
                    Ok(f) => match slot {
                        Slot::Pre => pre_atoms.push(f),
                        Slot::Post => post_atoms.push(f),
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
            AttrKind::SkippedShouldPanic => {
                out.warnings.push(LiftWarning {
                    source_path: source_path.into(),
                    item_name: name.clone(),
                    reason: "skipped #[kani::should_panic]: v0 does not lift negation markers; \
                             v1 will model as an `inv` with a kit-defined `should_panic` ctor"
                        .into(),
                });
            }
            AttrKind::SkippedProof => {
                out.warnings.push(LiftWarning {
                    source_path: source_path.into(),
                    item_name: name.clone(),
                    reason: "skipped #[kani::proof]: entry-point marker, not a contract".into(),
                });
            }
            AttrKind::SkippedUnwind => {
                out.warnings.push(LiftWarning {
                    source_path: source_path.into(),
                    item_name: name.clone(),
                    reason: "skipped #[kani::unwind]: loop bound, not a contract".into(),
                });
            }
            AttrKind::SkippedOther => {
                out.warnings.push(LiftWarning {
                    source_path: source_path.into(),
                    item_name: name.clone(),
                    reason: format!(
                        "skipped #[{}]: not in v0 lift whitelist",
                        path_to_string(a.path())
                    ),
                });
            }
        }
    }

    if pre_atoms.is_empty() && post_atoms.is_empty() {
        if !had_failure {
            // No predicate-bearing attrs at all (only proof/unwind/etc.).
            // The skip warnings already explain what we saw; no extra
            // log.
        }
        return;
    }

    let pre = combine(pre_atoms);
    let post = combine(post_atoms);

    let pre = pre.map(|f| wrap_forall(&params, 0, f));
    let post = post.map(|f| wrap_forall(&params, 0, f));

    out.decls.push(ContractDecl {
        name,
        pre,
        post,
        inv: None,
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

#[allow(dead_code)]
fn subst_var_name(f: &Rc<Formula>, from: &str, to: &str) -> Rc<Formula> {
    if from.is_empty() || from == to {
        return f.clone();
    }
    match &**f {
        Formula::Atomic { name, args } => {
            let new_args: Vec<Rc<Term>> = args.iter().map(|a| subst_term(a, from, to)).collect();
            atomic_(name.clone(), new_args)
        }
        Formula::Connective { kind, operands } => Rc::new(Formula::Connective {
            kind: kind.clone(),
            operands: operands.iter().map(|o| subst_var_name(o, from, to)).collect(),
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
    }
}

#[allow(dead_code)]
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

fn translate_bool_expr(expr: &syn::Expr, in_post: bool) -> Result<Rc<Formula>, String> {
    match expr {
        syn::Expr::Binary(b) => {
            let l = translate_term(&b.left, in_post)?;
            let r = translate_term(&b.right, in_post)?;
            match b.op {
                syn::BinOp::Gt(_) => Ok(gt(l, r)),
                syn::BinOp::Ge(_) => Ok(gte(l, r)),
                syn::BinOp::Lt(_) => Ok(lt(l, r)),
                syn::BinOp::Le(_) => Ok(lte(l, r)),
                syn::BinOp::Eq(_) => Ok(eq(l, r)),
                syn::BinOp::Ne(_) => Ok(ne(l, r)),
                _ => Err(format!("unsupported binop: {:?}", b.op)),
            }
        }
        syn::Expr::Paren(p) => translate_bool_expr(&p.expr, in_post),
        _ => Err("kani contract expression must be a comparison".into()),
    }
}

fn translate_term(expr: &syn::Expr, in_post: bool) -> Result<Rc<Term>, String> {
    match expr {
        syn::Expr::Path(p) => {
            if let Some(id) = p.path.get_ident() {
                let name = id.to_string();
                // Kani's ensures convention: `result` is the return
                // value. In our IR we model the return as the
                // contract's out_binding (default "out").
                if in_post && name == "result" {
                    Ok(make_var("out"))
                } else {
                    Ok(make_var(name))
                }
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
        syn::Expr::Paren(p) => translate_term(&p.expr, in_post),
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
            let inner = translate_term(c.args.first().unwrap(), in_post)?;
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
    fn lifts_kani_requires_and_ensures() {
        let src = r#"
            #[kani::requires(x > 0)]
            #[kani::ensures(result >= 0)]
            fn sqrt(x: i64) -> i64 { x }
        "#;
        let f = parse(src);
        let out = lift_file(&f, "test.rs");
        assert_eq!(out.lifted, 1, "warnings: {:?}", out.warnings);
        let d = &out.decls[0];
        assert_eq!(d.name, "sqrt");
        assert!(d.pre.is_some());
        assert!(d.post.is_some());
    }

    #[test]
    fn rewrites_result_to_out_in_ensures() {
        let src = r#"
            #[kani::ensures(result == 0)]
            fn zero() -> i64 { 0 }
        "#;
        let f = parse(src);
        let out = lift_file(&f, "test.rs");
        assert_eq!(out.lifted, 1);
        // The post should reference `out`, not `result`.
        let post = out.decls[0].post.as_ref().unwrap();
        let s = format!("{post:?}");
        assert!(s.contains("\"out\""), "expected `out` binding in post: {s}");
        assert!(!s.contains("\"result\""), "result should be rewritten: {s}");
    }

    #[test]
    fn skips_should_panic_with_warning() {
        let src = r#"
            #[kani::proof]
            #[kani::should_panic]
            fn divide_by_zero() { let _ = 1 / 0; }
        "#;
        let f = parse(src);
        let out = lift_file(&f, "test.rs");
        assert_eq!(out.lifted, 0);
        assert!(out
            .warnings
            .iter()
            .any(|w| w.reason.contains("should_panic")));
        assert!(out.warnings.iter().any(|w| w.reason.contains("kani::proof")));
    }

    #[test]
    fn skips_method_call_with_warning() {
        let src = r#"
            #[kani::requires(s.len() > 0)]
            fn f(s: String) { }
        "#;
        let f = parse(src);
        let out = lift_file(&f, "test.rs");
        assert_eq!(out.lifted, 0);
        assert!(!out.warnings.is_empty());
    }
}
