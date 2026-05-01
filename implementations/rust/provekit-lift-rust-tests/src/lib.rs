// SPDX-License-Identifier: Apache-2.0
//
// provekit-lift-rust-tests
//
// Walks the syn AST of a Rust source file looking for `#[test]` and
// `#[tokio::test]` functions. For each, scans the function body for
// assertion-macro invocations and lifts EACH assertion to its own
// content-addressed ContractDecl.
//
// THE FRAMING:
//
//   A unit test is a point-specific predicate: "at this input, this
//   output." A property test is a universal predicate: "forall input,
//   this property." Both are content-addressable behavior witnesses.
//   ProvekIt lifts both. Every passing test in your codebase becomes a
//   content-addressed signed contract memento. Test authors don't need
//   to write contracts; they already wrote the contracts. We just
//   promote them.
//
// LIFTABLE SHAPES (v0 whitelist):
//
//   assert_eq!(<lhs>, <rhs>)         -> atomic eq
//   assert_ne!(<lhs>, <rhs>)         -> atomic ne
//   assert!(<lhs> <binop> <rhs>)     -> atomic comparison
//   assert_matches!(<lhs>, <pat>)    -> atomic eq against ctor-of-lit
//                                       (only the trivial `Ok(42)` /
//                                       `Err(_)` shape; deeper patterns
//                                       skip with a warning)
//
// Each side of a lifted predicate must be one of:
//   - identifier (Var)
//   - integer literal (num)
//   - string literal (str_const)
//   - single-arg call (Ctor with one arg)
//
// Anything else SKIPS with a logged warning. Honest under-coverage
// beats polluting the lattice with unverifiable atoms.
//
// Naming convention: contract name = "<test_function_name>::<index>",
// where index counts assertion-macro statements zero-indexed in source
// order. The semantics: each assertion is independently a witness; if
// the file later splits an assert into two, the new one mints a fresh
// CID without invalidating the others.
//
// Each lifted ContractDecl has:
//   - name           = "<test_fn>::<i>"
//   - inv            = the lifted atomic Formula (closed; no foralls)
//   - pre/post       = None
//   - out_binding    = "out" (unused; provided for ContractDecl shape parity)

use std::collections::BTreeSet;
use std::rc::Rc;

use provekit_ir_symbolic::{
    eq, gt, gte, lt, lte, make_var, ne, num, str_const, ContractDecl, Formula, Term,
};

pub mod layer2;
pub use layer2::{lift_file_layer2, Layer2Output};

// ---------------------------------------------------------------------------
// Public re-exports of internals so the layer2 module can reuse Layer 0's
// assertion-macro recognizer / leaf translator. These are intentionally
// kept module-private to outside callers (the names end in `_pub` and are
// not re-exported from the crate root) but are visible to the layer2
// child module via the `pub(crate)` visibility.
// ---------------------------------------------------------------------------

pub(crate) fn is_assertion_macro_pub(mac: &syn::Macro) -> bool {
    is_assertion_macro(mac)
}

pub(crate) fn lift_assertion_macro_pub(mac: &syn::Macro) -> Result<Rc<Formula>, String> {
    lift_assertion_macro(mac)
}

pub(crate) fn path_to_string_pub(p: &syn::Path) -> String {
    path_to_string(p)
}

pub(crate) fn translate_term_pub(expr: &syn::Expr) -> Result<Rc<Term>, String> {
    translate_term(expr)
}

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
    /// Total assertion macro candidates the adapter saw (lifted + skipped).
    pub seen: usize,
    /// Assertion macros successfully lifted to a ContractDecl.
    pub lifted: usize,
}

/// Walk a parsed `syn::File` for `#[test]` / `#[tokio::test]` functions
/// and lift each contained assertion-macro invocation to its own
/// ContractDecl.
pub fn lift_file(file: &syn::File, source_path: &str) -> AdapterOutput {
    lift_file_with_skip(file, source_path, &BTreeSet::new())
}

/// Same as `lift_file` but skip any test fn whose name appears in
/// `skip`. Used by the dispatcher to avoid double-lifting tests Layer 2
/// has already claimed (bounded loop, helper-inlined, characterization).
pub fn lift_file_with_skip(
    file: &syn::File,
    source_path: &str,
    skip: &BTreeSet<String>,
) -> AdapterOutput {
    let mut out = AdapterOutput::default();
    walk_items(&file.items, source_path, skip, &mut out);
    out
}

fn walk_items(
    items: &[syn::Item],
    source_path: &str,
    skip: &BTreeSet<String>,
    out: &mut AdapterOutput,
) {
    for item in items {
        match item {
            syn::Item::Fn(f) => {
                if has_test_attr(&f.attrs) && !skip.contains(&f.sig.ident.to_string()) {
                    visit_test_fn(f, source_path, out);
                }
                // Also recurse into the body in case nested items hold test fns.
                walk_block_for_items(&f.block, source_path, skip, out);
            }
            syn::Item::Mod(m) => {
                if let Some((_, items)) = &m.content {
                    walk_items(items, source_path, skip, out);
                }
            }
            _ => {}
        }
    }
}

fn walk_block_for_items(
    block: &syn::Block,
    source_path: &str,
    skip: &BTreeSet<String>,
    out: &mut AdapterOutput,
) {
    for stmt in &block.stmts {
        if let syn::Stmt::Item(item) = stmt {
            walk_items(std::slice::from_ref(item), source_path, skip, out);
        }
    }
}

/// Recognize `#[test]` and `#[tokio::test]` (also tolerates other
/// `*::test` attribute paths used by async-runtime crates: `async_std`,
/// `actix_rt`, `smol`, etc.).
fn has_test_attr(attrs: &[syn::Attribute]) -> bool {
    attrs.iter().any(|a| {
        let p = path_to_string(a.path());
        p == "test" || p.ends_with("::test")
    })
}

fn visit_test_fn(f: &syn::ItemFn, source_path: &str, out: &mut AdapterOutput) {
    let test_name = f.sig.ident.to_string();
    // Walk every statement in the body. Each macro statement that is
    // an assertion macro increments the assertion index, regardless
    // of whether it lifts cleanly.
    let mut idx: usize = 0;
    for stmt in &f.block.stmts {
        let mac_opt = match stmt {
            syn::Stmt::Macro(sm) => Some(&sm.mac),
            syn::Stmt::Expr(syn::Expr::Macro(em), _) => Some(&em.mac),
            _ => None,
        };
        let Some(mac) = mac_opt else { continue };
        if !is_assertion_macro(mac) {
            continue;
        }
        out.seen += 1;
        let memento_name = format!("{test_name}::{idx}");
        idx += 1;
        match lift_assertion_macro(mac) {
            Ok(formula) => {
                out.decls.push(ContractDecl {
                    name: memento_name,
                    pre: None,
                    post: None,
                    inv: Some(formula),
                    out_binding: "out".into(),
                });
                out.lifted += 1;
            }
            Err(reason) => {
                out.warnings.push(LiftWarning {
                    source_path: source_path.into(),
                    item_name: memento_name,
                    reason,
                });
            }
        }
    }
}

fn is_assertion_macro(mac: &syn::Macro) -> bool {
    let p = path_to_string(&mac.path);
    matches!(
        p.as_str(),
        "assert_eq" | "assert_ne" | "assert" | "assert_matches"
    )
}

fn lift_assertion_macro(mac: &syn::Macro) -> Result<Rc<Formula>, String> {
    let path = path_to_string(&mac.path);
    match path.as_str() {
        "assert_eq" => {
            let pair: TwoExprs = syn::parse2(mac.tokens.clone())
                .map_err(|e| format!("assert_eq: parse: {e}"))?;
            let l = translate_term(&pair.a)?;
            let r = translate_term(&pair.b)?;
            Ok(eq(l, r))
        }
        "assert_ne" => {
            let pair: TwoExprs = syn::parse2(mac.tokens.clone())
                .map_err(|e| format!("assert_ne: parse: {e}"))?;
            let l = translate_term(&pair.a)?;
            let r = translate_term(&pair.b)?;
            Ok(ne(l, r))
        }
        "assert" => {
            // Only the `assert!(<expr>)` form. The expression must be a
            // top-level binary comparison.
            let one: OneExpr = syn::parse2(mac.tokens.clone())
                .map_err(|e| format!("assert: parse: {e}"))?;
            translate_bool_expr(&one.a)
        }
        "assert_matches" => {
            // `assert_matches!(<scrutinee>, <pattern>[, ...])`. We only
            // accept patterns that are a single ctor of a literal,
            // `None`, or a bare ident. Anything richer skips.
            let pair: ScrutineePattern = syn::parse2(mac.tokens.clone())
                .map_err(|e| format!("assert_matches: parse: {e}"))?;
            let scrutinee = translate_term(&pair.scrutinee)?;
            let rhs = translate_pattern_to_term(&pair.pattern)?;
            Ok(eq(scrutinee, rhs))
        }
        other => Err(format!("not an assertion macro: {other}")),
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
        if input.peek(syn::Token![,]) {
            let _: syn::Token![,] = input.parse()?;
            let _: proc_macro2::TokenStream = input.parse()?;
        }
        Ok(TwoExprs { a, b })
    }
}

struct OneExpr {
    a: syn::Expr,
}

impl syn::parse::Parse for OneExpr {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let a: syn::Expr = input.parse()?;
        if input.peek(syn::Token![,]) {
            let _: syn::Token![,] = input.parse()?;
            let _: proc_macro2::TokenStream = input.parse()?;
        }
        Ok(OneExpr { a })
    }
}

struct ScrutineePattern {
    scrutinee: syn::Expr,
    pattern: syn::Pat,
}

impl syn::parse::Parse for ScrutineePattern {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let scrutinee: syn::Expr = input.parse()?;
        let _: syn::Token![,] = input.parse()?;
        let pattern: syn::Pat = syn::Pat::parse_single(input)?;
        if input.peek(syn::Token![,]) {
            let _: syn::Token![,] = input.parse()?;
            let _: proc_macro2::TokenStream = input.parse()?;
        }
        Ok(ScrutineePattern { scrutinee, pattern })
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
                _ => Err(format!("unsupported binop in assert!: {:?}", b.op)),
            }
        }
        syn::Expr::Paren(p) => translate_bool_expr(&p.expr),
        _ => Err("assert! body must be a comparison expression".into()),
    }
}

/// Term translation. Whitelist:
///   - identifier (Var)
///   - integer literal (num)
///   - string literal (str_const)
///   - single-arg call (Ctor with one arg)
///   - call with two args is allowed only when the callee is a
///     constructor-shaped path (TitleCase first segment), so
///     `Ok(42)` lifts as `Ctor("Ok", [num(42)])`.
fn translate_term(expr: &syn::Expr) -> Result<Rc<Term>, String> {
    match expr {
        syn::Expr::Path(p) => {
            // Permit `None` -> `Ctor("None", [])` since it's a common assert RHS.
            if p.path.is_ident("None") {
                return Ok(Rc::new(Term::Ctor {
                    name: "None".to_string(),
                    args: vec![],
                }));
            }
            if let Some(id) = p.path.get_ident() {
                Ok(make_var(id.to_string()))
            } else {
                // Multi-segment path treated as ctor (e.g., `MyEnum::Variant`).
                let name = path_to_string(&p.path);
                Ok(Rc::new(Term::Ctor { name, args: vec![] }))
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
            syn::Lit::Bool(lb) => {
                let name = if lb.value { "True" } else { "False" };
                Ok(Rc::new(Term::Ctor {
                    name: name.into(),
                    args: vec![],
                }))
            }
            _ => Err("only integer, string, and bool literals are liftable in v0".into()),
        },
        syn::Expr::Paren(p) => translate_term(&p.expr),
        syn::Expr::Call(c) => {
            let callee = match &*c.func {
                syn::Expr::Path(p) => path_to_string(&p.path),
                _ => return Err("call target is not a simple path".into()),
            };
            if c.args.is_empty() {
                return Ok(Rc::new(Term::Ctor {
                    name: callee,
                    args: vec![],
                }));
            }
            if c.args.len() > 1 {
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
        _ => Err("expression shape not in v0 lift whitelist (no method calls, indexing, field access, multi-arg calls, complex nesting)".into()),
    }
}

/// Translate a single pattern to a Term. Only literal-leaf shapes pass.
fn translate_pattern_to_term(pat: &syn::Pat) -> Result<Rc<Term>, String> {
    match pat {
        syn::Pat::Lit(pl) => {
            let expr = syn::Expr::Lit(pl.clone());
            translate_term(&expr)
        }
        syn::Pat::Path(pp) => {
            if pp.path.is_ident("None") {
                return Ok(Rc::new(Term::Ctor {
                    name: "None".into(),
                    args: vec![],
                }));
            }
            let name = path_to_string(&pp.path);
            Ok(Rc::new(Term::Ctor { name, args: vec![] }))
        }
        syn::Pat::TupleStruct(pts) => {
            let name = path_to_string(&pts.path);
            if pts.elems.len() != 1 {
                return Err(format!(
                    "tuple-struct pattern `{name}` with {} elems is not liftable in v0",
                    pts.elems.len()
                ));
            }
            let inner = pts.elems.first().unwrap();
            let inner_term = translate_pattern_to_term(inner)?;
            Ok(Rc::new(Term::Ctor {
                name,
                args: vec![inner_term],
            }))
        }
        syn::Pat::Wild(_) => Err("wildcard `_` patterns are not liftable in v0".into()),
        _ => Err("pattern shape not in v0 lift whitelist".into()),
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
    fn lifts_simple_assert_eq() {
        let src = r#"
            #[test]
            fn parse_int_42() {
                assert_eq!(parse_int("42"), 42);
            }
        "#;
        let f = parse(src);
        let out = lift_file(&f, "t.rs");
        assert_eq!(out.lifted, 1, "warnings: {:?}", out.warnings);
        assert_eq!(out.decls[0].name, "parse_int_42::0");
        assert!(out.decls[0].inv.is_some());
    }

    #[test]
    fn each_assert_gets_its_own_decl() {
        let src = r#"
            #[test]
            fn three_facts() {
                assert_eq!(f(1), 1);
                assert_eq!(f(2), 2);
                assert_ne!(f(3), 0);
            }
        "#;
        let f = parse(src);
        let out = lift_file(&f, "t.rs");
        assert_eq!(out.lifted, 3, "warnings: {:?}", out.warnings);
        let names: Vec<_> = out.decls.iter().map(|d| d.name.as_str()).collect();
        assert_eq!(
            names,
            vec!["three_facts::0", "three_facts::1", "three_facts::2"]
        );
    }

    #[test]
    fn lifts_assert_with_binop() {
        let src = r#"
            #[test]
            fn nonneg_one() {
                assert!(some_value > 0);
            }
        "#;
        let f = parse(src);
        let out = lift_file(&f, "t.rs");
        assert_eq!(out.lifted, 1, "warnings: {:?}", out.warnings);
    }

    #[test]
    fn lifts_tokio_test() {
        let src = r#"
            #[tokio::test]
            async fn async_two() {
                assert_eq!(g(2), 4);
            }
        "#;
        let f = parse(src);
        let out = lift_file(&f, "t.rs");
        assert_eq!(out.lifted, 1, "warnings: {:?}", out.warnings);
    }

    #[test]
    fn skips_method_call_with_warning() {
        let src = r#"
            #[test]
            fn uses_method() {
                assert_eq!("foo".len(), 3);
            }
        "#;
        let f = parse(src);
        let out = lift_file(&f, "t.rs");
        assert_eq!(out.lifted, 0);
        assert_eq!(out.warnings.len(), 1);
    }

    #[test]
    fn lifts_assert_matches_simple_ctor() {
        let src = r#"
            #[test]
            fn matches_ok() {
                assert_matches!(parse_int("42"), Ok(42));
            }
        "#;
        let f = parse(src);
        let out = lift_file(&f, "t.rs");
        assert_eq!(out.lifted, 1, "warnings: {:?}", out.warnings);
    }

    #[test]
    fn skips_assert_matches_wildcard() {
        let src = r#"
            #[test]
            fn matches_any() {
                assert_matches!(some_call(x), Ok(_));
            }
        "#;
        let f = parse(src);
        let out = lift_file(&f, "t.rs");
        assert_eq!(out.lifted, 0);
        assert_eq!(out.warnings.len(), 1);
    }

    #[test]
    fn ignores_non_test_function() {
        let src = r#"
            fn helper() {
                assert_eq!(1, 1);
            }
        "#;
        let f = parse(src);
        let out = lift_file(&f, "t.rs");
        assert_eq!(out.seen, 0);
        assert_eq!(out.lifted, 0);
    }
}
