// SPDX-License-Identifier: Apache-2.0
//
// provekit-lift-quickcheck
//
// STRATEGIC POSITIONING
//
// ProvekIt consumes quickcheck's existing annotations; we sit beneath,
// not against. quickcheck (https://crates.io/crates/quickcheck) marks
// property functions with `#[quickcheck]`. The function's parameters
// are the universally-quantified domain, and the function's body is the
// predicate. The classic shape:
//
//   #[quickcheck]
//   fn prop_commutes(a: i64, b: i64) -> bool {
//       a + b == b + a
//   }
//
// V0 SHAPE
//
//   - Function attributed with `#[quickcheck]` (or `#[quickcheck::quickcheck]`).
//   - Returns `bool` (not `TestResult`, not `()`).
//   - Body is a single tail expression matching the v0 binop whitelist:
//     `<var|lit|single-arg-call> <binop> <var|lit|single-arg-call>` with
//     binop in {>, >=, <, <=, ==, !=}.
//
// Each lifted property maps to a ContractDecl whose `inv` (universal
// invariant over the parameters) is the lifted formula. Multi-statement
// bodies and `TestResult`-returning props skip with a warning.

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
            _ => {}
        }
    }
}

fn visit_fn(f: &syn::ItemFn, source_path: &str, out: &mut AdapterOutput) {
    if !has_quickcheck_attr(&f.attrs) {
        return;
    }
    out.seen += 1;
    let name = f.sig.ident.to_string();
    if !returns_bool(&f.sig.output) {
        out.warnings.push(LiftWarning {
            source_path: source_path.into(),
            item_name: name,
            reason: "v0 quickcheck adapter only lifts -> bool returns (TestResult and other return shapes skipped)".into(),
        });
        return;
    }

    let mut params: Vec<(String, Sort)> = Vec::new();
    for arg in &f.sig.inputs {
        if let syn::FnArg::Typed(pt) = arg {
            if let syn::Pat::Ident(pi) = &*pt.pat {
                params.push((pi.ident.to_string(), sort_for_type(&pt.ty)));
            }
        }
    }

    let tail = match extract_tail_expr(&f.block) {
        Ok(e) => e,
        Err(reason) => {
            out.warnings.push(LiftWarning {
                source_path: source_path.into(),
                item_name: name,
                reason,
            });
            return;
        }
    };

    let body_formula = match translate_bool_expr(tail) {
        Ok(f) => f,
        Err(reason) => {
            out.warnings.push(LiftWarning {
                source_path: source_path.into(),
                item_name: name,
                reason,
            });
            return;
        }
    };

    let wrapped = wrap_forall(&params, 0, body_formula);
    out.decls.push(ContractDecl {
        name,
        pre: None,
        post: None,
        inv: Some(wrapped),
        out_binding: "out".into(),
    });
    out.lifted += 1;
}

fn has_quickcheck_attr(attrs: &[syn::Attribute]) -> bool {
    attrs.iter().any(|a| {
        let p = path_to_string(a.path());
        p == "quickcheck" || p == "quickcheck::quickcheck"
    })
}

fn returns_bool(ret: &syn::ReturnType) -> bool {
    match ret {
        syn::ReturnType::Default => false,
        syn::ReturnType::Type(_, ty) => {
            use quote::ToTokens;
            let mut ts = proc_macro2::TokenStream::new();
            ty.to_tokens(&mut ts);
            ts.to_string().trim() == "bool"
        }
    }
}

/// The body must be a single tail expression. Multi-statement bodies
/// skip with a warning to keep the lattice clean.
fn extract_tail_expr(block: &syn::Block) -> Result<&syn::Expr, String> {
    if block.stmts.len() != 1 {
        return Err(format!(
            "v0 quickcheck adapter requires single-expr body; got {} stmts",
            block.stmts.len()
        ));
    }
    match &block.stmts[0] {
        syn::Stmt::Expr(e, None) => Ok(e),
        _ => Err("v0 quickcheck adapter requires a tail expression body (no trailing semicolon)".into()),
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
        Formula::Choice { var_name, sort, body } => {
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
        Term::Lambda { param_name, param_sort, body } => {
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
            let new_body = if shadowed { body.clone() } else { subst_term(body, from, to) };
            Rc::new(Term::Let { bindings: new_bindings, body: new_body })
        }
    }
}

fn translate_bool_expr(expr: &syn::Expr) -> Result<Rc<Formula>, String> {
    match expr {
        syn::Expr::Binary(b) => {
            // Handle binary && / || by recursing.
            match b.op {
                syn::BinOp::And(_) => {
                    let l = translate_bool_expr(&b.left)?;
                    let r = translate_bool_expr(&b.right)?;
                    Ok(and_(vec![l, r]))
                }
                syn::BinOp::Or(_) => Err("v0 lifts only conjunctive bodies (no `||`)".into()),
                _ => {
                    let l = translate_term(&b.left)?;
                    let r = translate_term(&b.right)?;
                    match b.op {
                        syn::BinOp::Gt(_) => Ok(gt(l, r)),
                        syn::BinOp::Ge(_) => Ok(gte(l, r)),
                        syn::BinOp::Lt(_) => Ok(lt(l, r)),
                        syn::BinOp::Le(_) => Ok(lte(l, r)),
                        syn::BinOp::Eq(_) => Ok(eq(l, r)),
                        syn::BinOp::Ne(_) => Ok(ne(l, r)),
                        other => Err(format!("unsupported binop in quickcheck body: {other:?}")),
                    }
                }
            }
        }
        syn::Expr::Paren(p) => translate_bool_expr(&p.expr),
        _ => Err("quickcheck body must be a comparison expression".into()),
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
            // Allow simple sums on each side: a + b. v0 doesn't lift
            // arithmetic terms; skip with warning via Err.
            Err("unary expression not liftable".into())
        }
        _ => Err("expression shape not in v0 lift whitelist (no arithmetic, method calls, indexing, field access)".into()),
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
    fn lifts_simple_eq_property() {
        let src = r#"
            #[quickcheck]
            fn prop_zero_is_zero(a: i64) -> bool {
                a == a
            }
        "#;
        let f = parse(src);
        let out = lift_file(&f, "test.rs");
        assert_eq!(out.lifted, 1, "warnings: {:?}", out.warnings);
        assert!(out.decls[0].inv.is_some());
    }

    #[test]
    fn skips_arith_in_body() {
        // a + b == b + a is rejected because + is not in the v0 whitelist.
        let src = r#"
            #[quickcheck]
            fn prop_commutes(a: i64, b: i64) -> bool {
                a + b == b + a
            }
        "#;
        let f = parse(src);
        let out = lift_file(&f, "test.rs");
        assert_eq!(out.lifted, 0);
        assert_eq!(out.warnings.len(), 1);
    }

    #[test]
    fn skips_test_result_return() {
        let src = r#"
            #[quickcheck]
            fn prop_tr(a: i64) -> TestResult {
                TestResult::passed()
            }
        "#;
        let f = parse(src);
        let out = lift_file(&f, "test.rs");
        assert_eq!(out.lifted, 0);
        assert!(out.warnings[0].reason.contains("bool"));
    }

    #[test]
    fn lifts_namespaced_attr() {
        let src = r#"
            #[quickcheck::quickcheck]
            fn prop_nonneg(a: i64) -> bool {
                a >= -1
            }
        "#;
        let f = parse(src);
        let out = lift_file(&f, "test.rs");
        assert_eq!(out.lifted, 1, "warnings: {:?}", out.warnings);
    }
}
