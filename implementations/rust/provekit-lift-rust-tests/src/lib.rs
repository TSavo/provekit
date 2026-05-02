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
// LIFTABLE SHAPES (v0.5 whitelist):
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
//   - integer / string / byte-string / bool literal
//   - bare or multi-segment path (treated as nullary Ctor)
//   - free-function call `f(a, b, ...)` with N >= 0 args
//     -> Ctor { name: "f", args: [lift(a), lift(b), ...] }
//   - method call `recv.f(a, b)` (UFCS-flatten ONE level)
//     -> Ctor { name: "f", args: [lift(recv), lift(a), lift(b)] }
//   - reference `&expr` -> lift(expr)
//   - cast `expr as T`  -> lift(expr)
//   - parenthesised expr-> lift(inner)
//   - array literal `[a, b]` -> Ctor { name: "array", args: [...] }
//   - tuple literal `(a, b)` -> Ctor { name: "tuple", args: [...] }
//   - vec! macro `vec![a, b]` -> Ctor { name: "vec", args: [...] }
//   - binary op `a OP b` (operand position only)
//                         -> Ctor { name: "<op>", args: [lift(a), lift(b)] }
//   - unary `-int_lit`  (negative integer literal)
//
// Anything else SKIPS with a logged warning. Honest under-coverage
// beats polluting the lattice with unverifiable atoms.
//
// NOTE: method-call + free-call extension was a deliberate widening of
// the original v0 whitelist. The original v0 admitted only single-arg
// or zero-arg ctor calls and rejected method calls outright. The
// canonicalizer's load-bearing claim shape is
//     assert_eq!(encode_jcs(&Value::object([("k", v), ...])), "<canonical>")
// which the original v0 grammar could not reach, leaving the byte-faithful
// JCS invariants operationally enforced but un-lifted. v0.5 admits the
// idiomatic "compose a constructor, call a function, compare" shape.
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
                evidence: None,
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

/// Term translation. Whitelist (v0.5):
///   - identifier (Var) / multi-segment path (nullary Ctor)
///   - integer / string / byte-string / bool literal
///   - free-function call `f(a, b, ...)`        -> Ctor("f", [args...])
///   - method call `recv.f(a, b)`               -> Ctor("f", [recv, args...]) (UFCS, one level)
///   - reference `&expr`                        -> lift(expr) (transparent)
///   - cast `expr as T`                         -> lift(expr) (transparent)
///   - parenthesised `(expr)`                   -> lift(expr)
///   - array literal `[a, b, c]`                -> Ctor("array", [args...])
///   - tuple literal `(a, b)`                   -> Ctor("tuple", [args...])
///   - `vec![a, b, c]` macro                    -> Ctor("vec", [args...])
///   - binary op `a OP b` in operand position   -> Ctor("<OP>", [a, b])
///   - unary `-int_lit`                         -> num(-n)
///
/// Method-call flattening is exactly one level (`a.b().c()` lifts the
/// outer `c` as `Ctor("c", [<inner>])` only if the inner `a.b()` is also
/// recognized; deeper chains skip naturally because the recursion bottoms
/// out at a non-whitelisted shape). We do NOT attempt `a.b.c.d.e()`-style
/// chains as a flat ctor; structural fidelity beats reach.
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
            syn::Lit::ByteStr(bs) => {
                // Byte-string literal `b"..."`. The IR has no native bytes
                // sort; lift as Ctor("byte_str", [str_const(<utf8-or-escaped>)])
                // so the predicate stays semantically distinct from a normal
                // string literal even when the bytes are valid UTF-8.
                let bytes = bs.value();
                let s = match std::str::from_utf8(&bytes) {
                    Ok(s) => s.to_string(),
                    Err(_) => bytes
                        .iter()
                        .map(|b| format!("\\x{:02x}", b))
                        .collect::<String>(),
                };
                Ok(Rc::new(Term::Ctor {
                    name: "byte_str".into(),
                    args: vec![str_const(s)],
                }))
            }
            syn::Lit::Bool(lb) => {
                let name = if lb.value { "True" } else { "False" };
                Ok(Rc::new(Term::Ctor {
                    name: name.into(),
                    args: vec![],
                }))
            }
            _ => Err(
                "only integer, string, byte-string, and bool literals are liftable in v0.5".into(),
            ),
        },
        syn::Expr::Paren(p) => translate_term(&p.expr),
        // `&expr` and `&mut expr` are transparent for lift purposes.
        // `encode_jcs(&v)` and `encode_jcs(v)` lift to the same Ctor.
        syn::Expr::Reference(r) => translate_term(&r.expr),
        // `expr as T` is transparent: the target type is invisible to the
        // IR (no sort coercion), so we strip the cast and lift the inner
        // expression. This handles
        //     Value::object([] as [(String, _); 0])
        // shapes from the canonicalizer test corpus.
        syn::Expr::Cast(c) => translate_term(&c.expr),
        syn::Expr::Call(c) => {
            let callee = match &*c.func {
                syn::Expr::Path(p) => path_to_string(&p.path),
                _ => return Err("call target is not a simple path".into()),
            };
            let mut args: Vec<Rc<Term>> = Vec::with_capacity(c.args.len());
            for (i, a) in c.args.iter().enumerate() {
                let lifted = translate_term(a)
                    .map_err(|e| format!("call `{callee}` arg {i}: {e}"))?;
                args.push(lifted);
            }
            Ok(Rc::new(Term::Ctor { name: callee, args }))
        }
        syn::Expr::MethodCall(mc) => {
            // `recv.method(a, b)` UFCS-flattens to Ctor("method", [recv, a, b]).
            // The receiver is lifted recursively; if it isn't whitelisted, the
            // whole expression fails cleanly.
            let recv = translate_term(&mc.receiver).map_err(|e| {
                format!("method call `.{}`: receiver: {e}", mc.method)
            })?;
            let mut args: Vec<Rc<Term>> = Vec::with_capacity(1 + mc.args.len());
            args.push(recv);
            for (i, a) in mc.args.iter().enumerate() {
                let lifted = translate_term(a).map_err(|e| {
                    format!("method call `.{}` arg {i}: {e}", mc.method)
                })?;
                args.push(lifted);
            }
            Ok(Rc::new(Term::Ctor {
                name: mc.method.to_string(),
                args,
            }))
        }
        syn::Expr::Array(a) => {
            let mut args: Vec<Rc<Term>> = Vec::with_capacity(a.elems.len());
            for (i, e) in a.elems.iter().enumerate() {
                let lifted = translate_term(e)
                    .map_err(|err| format!("array elem {i}: {err}"))?;
                args.push(lifted);
            }
            Ok(Rc::new(Term::Ctor {
                name: "array".into(),
                args,
            }))
        }
        syn::Expr::Tuple(t) => {
            let mut args: Vec<Rc<Term>> = Vec::with_capacity(t.elems.len());
            for (i, e) in t.elems.iter().enumerate() {
                let lifted = translate_term(e)
                    .map_err(|err| format!("tuple elem {i}: {err}"))?;
                args.push(lifted);
            }
            Ok(Rc::new(Term::Ctor {
                name: "tuple".into(),
                args,
            }))
        }
        syn::Expr::Macro(em) => {
            // The only macro we lift in operand position is `vec![...]`.
            // `format!`, `println!`, `concat!`, `assert!`, etc. all skip
            // with a specific reason.
            let mac_name = path_to_string(&em.mac.path);
            if mac_name == "vec" {
                let elems: VecMacroArgs = syn::parse2(em.mac.tokens.clone())
                    .map_err(|e| format!("vec! macro: parse: {e}"))?;
                let mut args: Vec<Rc<Term>> = Vec::with_capacity(elems.exprs.len());
                for (i, e) in elems.exprs.iter().enumerate() {
                    let lifted = translate_term(e)
                        .map_err(|err| format!("vec! elem {i}: {err}"))?;
                    args.push(lifted);
                }
                Ok(Rc::new(Term::Ctor {
                    name: "vec".into(),
                    args,
                }))
            } else {
                Err(format!(
                    "macro `{mac_name}!(...)` in operand position not lifted in v0.5 \
                     (only `vec![...]` is recognized)"
                ))
            }
        }
        syn::Expr::Binary(b) => {
            // Binary op in operand position (NOT inside `assert!`, which has
            // its own predicate-shaped path through `translate_bool_expr`).
            // Lift structurally as Ctor("<op>", [lhs, rhs]) so e.g.
            //     assert_eq!(h.len(), PREFIX.len() + 128)
            // becomes
            //     eq(Ctor("len", [h]), Ctor("+", [Ctor("len", [PREFIX]), num(128)]))
            // The verifier may not have a semantic interpretation of `+` as
            // an atom name, but the structural lift is faithful and leaves
            // the lattice clean.
            let l = translate_term(&b.left)?;
            let r = translate_term(&b.right)?;
            let op = match b.op {
                syn::BinOp::Add(_) => "+",
                syn::BinOp::Sub(_) => "-",
                syn::BinOp::Mul(_) => "*",
                syn::BinOp::Div(_) => "/",
                syn::BinOp::Rem(_) => "%",
                syn::BinOp::BitAnd(_) => "&",
                syn::BinOp::BitOr(_) => "|",
                syn::BinOp::BitXor(_) => "^",
                syn::BinOp::Shl(_) => "<<",
                syn::BinOp::Shr(_) => ">>",
                _ => {
                    return Err(format!(
                        "binary op {:?} in operand position not lifted in v0.5",
                        b.op
                    ));
                }
            };
            Ok(Rc::new(Term::Ctor {
                name: op.into(),
                args: vec![l, r],
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
        _ => Err("expression shape not in v0.5 lift whitelist (supported: paths, literals, calls, method calls, refs, casts, arrays, tuples, vec!, binary ops, parens)".into()),
    }
}

/// Parser for the body of a `vec![a, b, c]` macro: a comma-separated
/// list of expressions. Trailing comma is permitted. The `vec![v; n]`
/// repeat shape is rejected (it's structurally different from a finite
/// list; we keep the lift faithful).
struct VecMacroArgs {
    exprs: Vec<syn::Expr>,
}

impl syn::parse::Parse for VecMacroArgs {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let mut exprs = Vec::new();
        if input.is_empty() {
            return Ok(VecMacroArgs { exprs });
        }
        let first: syn::Expr = input.parse()?;
        // Detect `vec![v; n]` repeat-shape.
        if input.peek(syn::Token![;]) {
            return Err(input.error(
                "vec![v; n] repeat-form not lifted in v0.5 (only finite list form)",
            ));
        }
        exprs.push(first);
        while input.peek(syn::Token![,]) {
            let _: syn::Token![,] = input.parse()?;
            if input.is_empty() {
                break;
            }
            let e: syn::Expr = input.parse()?;
            exprs.push(e);
        }
        Ok(VecMacroArgs { exprs })
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
    fn lifts_method_call_as_ufcs_ctor() {
        // v0.5 widening: method calls are now lifted by UFCS-flattening
        // one level. `recv.f(args)` becomes Ctor("f", [recv, args...]).
        let src = r#"
            #[test]
            fn uses_method() {
                assert_eq!("foo".len(), 3);
            }
        "#;
        let f = parse(src);
        let out = lift_file(&f, "t.rs");
        assert_eq!(out.lifted, 1, "warnings: {:?}", out.warnings);
        // Inspect the IR: the LHS should be Ctor("len", [str_const("foo")]).
        let inv = out.decls[0].inv.as_ref().expect("inv");
        let lhs = match &**inv {
            Formula::Atomic { name, args } if name == "=" && args.len() == 2 => {
                args[0].clone()
            }
            other => panic!("expected atomic =, got {other:?}"),
        };
        match &*lhs {
            Term::Ctor { name, args } => {
                assert_eq!(name, "len");
                assert_eq!(args.len(), 1, "expected receiver as single arg");
                match &**args.first().unwrap() {
                    Term::Const {
                        value: provekit_ir_symbolic::ConstValue::String(s),
                        ..
                    } => assert_eq!(s, "foo"),
                    other => panic!("expected string receiver, got {other:?}"),
                }
            }
            other => panic!("expected Ctor LHS, got {other:?}"),
        }
    }

    #[test]
    fn lifts_multi_arg_call_with_array_and_tuple() {
        // The canonicalizer's load-bearing shape:
        //   assert_eq!(encode_jcs(&Value::object([("a", "x")])), "{\"a\":\"x\"}")
        // lifts to a nested Ctor of {encode_jcs, Value::object, array, tuple}.
        let src = r#"
            #[test]
            fn jcs_object_one_pair() {
                assert_eq!(encode_jcs(&Value::object([("a", "x")])), "{\"a\":\"x\"}");
            }
        "#;
        let f = parse(src);
        let out = lift_file(&f, "t.rs");
        assert_eq!(out.lifted, 1, "warnings: {:?}", out.warnings);
        // Walk the structure.
        let inv = out.decls[0].inv.as_ref().expect("inv");
        let lhs = match &**inv {
            Formula::Atomic { name, args } if name == "=" && args.len() == 2 => {
                args[0].clone()
            }
            other => panic!("expected atomic =, got {other:?}"),
        };
        // outer: encode_jcs(<one arg>)
        let inner = match &*lhs {
            Term::Ctor { name, args } if name == "encode_jcs" && args.len() == 1 => {
                args[0].clone()
            }
            other => panic!("expected Ctor encode_jcs/1, got {other:?}"),
        };
        // next: Value::object(<one arg = array>)
        let arr = match &*inner {
            Term::Ctor { name, args } if name == "Value::object" && args.len() == 1 => {
                args[0].clone()
            }
            other => panic!("expected Ctor Value::object/1, got {other:?}"),
        };
        // next: array(<one elem = tuple>)
        let tup = match &*arr {
            Term::Ctor { name, args } if name == "array" && args.len() == 1 => args[0].clone(),
            other => panic!("expected Ctor array/1, got {other:?}"),
        };
        match &*tup {
            Term::Ctor { name, args } if name == "tuple" && args.len() == 2 => {}
            other => panic!("expected Ctor tuple/2, got {other:?}"),
        }
    }

    #[test]
    fn lifts_vec_macro_as_ctor() {
        let src = r#"
            #[test]
            fn jcs_array_vec() {
                assert_eq!(encode_jcs(&Value::array(vec![])), "[]");
            }
        "#;
        let f = parse(src);
        let out = lift_file(&f, "t.rs");
        assert_eq!(out.lifted, 1, "warnings: {:?}", out.warnings);
    }

    #[test]
    fn lifts_cast_transparently() {
        // `[] as [(String, _); 0]` is the canonicalizer's idiom for an
        // empty typed array literal. The cast must be transparent.
        let src = r#"
            #[test]
            fn jcs_object_empty() {
                assert_eq!(
                    encode_jcs(&Value::object([] as [(String, i32); 0])),
                    "{}"
                );
            }
        "#;
        let f = parse(src);
        let out = lift_file(&f, "t.rs");
        assert_eq!(out.lifted, 1, "warnings: {:?}", out.warnings);
    }

    #[test]
    fn lifts_byte_string_literal() {
        let src = r#"
            #[test]
            fn distinct_inputs_distinct_hashes() {
                assert_ne!(blake3_512_of(b"hello"), blake3_512_of(b"world"));
            }
        "#;
        let f = parse(src);
        let out = lift_file(&f, "t.rs");
        assert_eq!(out.lifted, 1, "warnings: {:?}", out.warnings);
    }

    #[test]
    fn lifts_binary_op_in_operand_position() {
        // `h.len() == BLAKE3_512_PREFIX.len() + 128` shape.
        let src = r#"
            #[test]
            fn cid_regex_compliance() {
                assert_eq!(h.len(), PREFIX.len() + 128);
            }
        "#;
        let f = parse(src);
        let out = lift_file(&f, "t.rs");
        assert_eq!(out.lifted, 1, "warnings: {:?}", out.warnings);
    }

    #[test]
    fn skips_format_macro_in_operand_position() {
        // `format!(...)` is `Expr::Macro` — explicitly NOT lifted in v0.5.
        // This is the negative-coverage test that documents what stays out.
        let src = r#"
            #[test]
            fn uses_format_macro() {
                assert_eq!(encoded, format!("\"{}\"", sym));
            }
        "#;
        let f = parse(src);
        let out = lift_file(&f, "t.rs");
        assert_eq!(out.lifted, 0);
        assert_eq!(out.warnings.len(), 1);
        assert!(
            out.warnings[0].reason.contains("macro")
                && out.warnings[0].reason.contains("format"),
            "expected reason to name the macro: {:?}",
            out.warnings[0].reason
        );
    }

    #[test]
    fn skips_block_expression_in_operand_position() {
        let src = r#"
            #[test]
            fn uses_block_expr() {
                assert_eq!({ let x = 1; x + 2 }, 3);
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
