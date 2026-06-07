// SPDX-License-Identifier: Apache-2.0
//
// provekit-lift-rust-tests
//
// Walks the syn AST of a Rust source file looking for `#[test]` and
// `#[tokio::test]` functions. For each assertion macro, identifies the
// producer callsite for the asserted value and lifts one content-addressed
// ContractDecl per callsite.
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
//   assert!(<bool-expr>)             -> atomic eq against True
//   assert!(!<bool-expr>)            -> atomic eq against False
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
// Naming convention: contract name = "<callee>@<file>:<line>:<col>",
// where the locus is the call expression that produced the asserted value.
// Let-bound observations substitute the latest visible `let name = <call>`
// binding into the lifted formula before emission. Assertions with no
// identifiable producer callsite skip with a LiftWarning.
//
// Each lifted ContractDecl has:
//   - name           = "<callee>@<file>:<line>:<col>"
//   - inv            = the lifted atomic Formula (closed; no foralls)
//   - pre/post       = None
//   - out_binding    = "out" (unused; provided for ContractDecl shape parity)

use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;
use std::sync::Arc;

use std::collections::HashMap;

use provekit_canonicalizer::{blake3_512_of, encode_jcs, Value as CValue};
use provekit_ir_symbolic::{
    and_, eq, gt, gte, lt, lte, make_var, ne, num, or_, serialize::formula_to_value, str_const,
    ConstValue, ContractDecl, Formula, Sort, Term,
};
use provekit_ir_types::{
    EvidenceMemento, IrFormula, SourceKind, SourceLocator, SourceLocatorPoint, SourceLocatorSpan,
};
use provekit_walk::emit::rust_function_term_json_cid;
use syn::spanned::Spanned;

/// The auto-promote sentinel lifter CID (128 hex zeros after the prefix).
/// Used until PR-F wires the real lifter CID (compound spec §4.4).
pub const AUTO_PROMOTE_LIFTER_CID: &str = concat!(
    "blake3-512:",
    "0000000000000000000000000000000000000000000000000000000000000000",
    "0000000000000000000000000000000000000000000000000000000000000000",
);

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

pub(crate) fn lift_assertion_macro_at_callsites_pub(
    mac: &syn::Macro,
    source_path: &str,
    bindings: &BTreeMap<String, BoundCall>,
) -> Result<Vec<LiftedCallsiteAssertion>, String> {
    lift_assertion_macro_at_callsites(mac, source_path, bindings)
}

pub(crate) fn callsite_contract_name_pub(
    callee: &str,
    source_path: &str,
    span: proc_macro2::Span,
) -> String {
    callsite_contract_name(callee, None, source_path, span)
}

#[derive(Debug, Clone)]
pub(crate) struct BoundCall {
    pub(crate) callee: String,
    pub(crate) span: proc_macro2::Span,
    pub(crate) term: Rc<Term>,
}

#[derive(Debug, Clone)]
pub(crate) struct LiftedCallsiteAssertion {
    pub(crate) name: String,
    pub(crate) formula: Rc<Formula>,
}

/// Extended version of `LiftedCallsiteAssertion` that also carries the
/// callsite span and raw callee name -- needed for `SourceLocatorSpan` and
/// `test_target_function_cid` in `EvidenceMemento`.
#[derive(Debug, Clone)]
struct LiftedCallsiteAssertionWithSpan {
    name: String,
    formula: Rc<Formula>,
    callsite_span: proc_macro2::Span,
    /// Raw callee name (e.g. `"deposit"` or `"Account::deposit"`). Used to
    /// look up the target function in the same lift pass for Option A CID
    /// computation (spec §10 `test_target_function_cid`).
    callee: String,
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
    /// One `EvidenceMemento` per lifted callsite assertion.
    /// Populated only when `lift_file_with_evidence` is called; empty
    /// when using the basic `lift_file` / `lift_file_with_skip` API.
    pub evidences: Vec<EvidenceMemento>,
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

/// Same as `lift_file` but also mints an `EvidenceMemento` per lifted
/// callsite assertion. The `source_bytes` are the raw bytes of the source
/// file identified by `source_path`; they are hashed to produce the
/// `source_cid` embedded in each evidence's `source_locator`.
///
/// Use this variant when the caller already holds the file bytes (e.g., the
/// `walk_emit` binary after reading the file to pass to `syn::parse_file`).
/// This keeps the lifter pure (no filesystem I/O).
pub fn lift_file_with_evidence(
    file: &syn::File,
    source_path: &str,
    source_bytes: &[u8],
) -> AdapterOutput {
    lift_file_with_evidence_and_skip(file, source_path, source_bytes, &BTreeSet::new())
}

/// Combined `lift_file_with_evidence` + `lift_file_with_skip`.
pub fn lift_file_with_evidence_and_skip(
    file: &syn::File,
    source_path: &str,
    source_bytes: &[u8],
    skip: &BTreeSet<String>,
) -> AdapterOutput {
    let source_cid = blake3_512_of(source_bytes);
    // Build a map from bare function name to &syn::ItemFn for all non-test top-level
    // functions in this file. Used by `visit_test_fn_with_evidence` to compute
    // `test_target_function_cid` (spec §10) via Option A: same-lift-pass CID
    // for free-fn callees whose definition is visible in this syn::File.
    let fn_map: HashMap<String, &syn::ItemFn> = collect_non_test_fns(&file.items);
    let mut out = AdapterOutput::default();
    walk_items_with_evidence(
        &file.items,
        source_path,
        &source_cid,
        skip,
        &fn_map,
        &mut out,
    );
    out
}

/// Collect all non-test `fn` items reachable from `items` (top-level and inside
/// `mod` blocks) by bare name. Only free functions are included; method calls
/// on `self` / receiver types cannot be resolved by name alone and fall through
/// to the pending path.
fn collect_non_test_fns<'a>(items: &'a [syn::Item]) -> HashMap<String, &'a syn::ItemFn> {
    let mut map = HashMap::new();
    for item in items {
        match item {
            syn::Item::Fn(f) if !has_test_attr(&f.attrs) => {
                map.insert(f.sig.ident.to_string(), f);
            }
            syn::Item::Mod(m) => {
                if let Some((_, inner)) = &m.content {
                    map.extend(collect_non_test_fns(inner));
                }
            }
            _ => {}
        }
    }
    map
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

/// Evidence-emitting variant of `walk_items`. Mirrors `walk_items` but passes
/// `source_cid` and `fn_map` down so `visit_test_fn_with_evidence` can populate
/// `evidences` including `test_target_function_cid` (spec §10).
fn walk_items_with_evidence<'a>(
    items: &'a [syn::Item],
    source_path: &str,
    source_cid: &str,
    skip: &BTreeSet<String>,
    fn_map: &HashMap<String, &'a syn::ItemFn>,
    out: &mut AdapterOutput,
) {
    for item in items {
        match item {
            syn::Item::Fn(f) => {
                if has_test_attr(&f.attrs) && !skip.contains(&f.sig.ident.to_string()) {
                    visit_test_fn_with_evidence(f, source_path, source_cid, fn_map, out);
                }
                walk_block_for_items_with_evidence(
                    &f.block,
                    source_path,
                    source_cid,
                    skip,
                    fn_map,
                    out,
                );
            }
            syn::Item::Mod(m) => {
                if let Some((_, items)) = &m.content {
                    walk_items_with_evidence(items, source_path, source_cid, skip, fn_map, out);
                }
            }
            _ => {}
        }
    }
}

fn walk_block_for_items_with_evidence<'a>(
    block: &'a syn::Block,
    source_path: &str,
    source_cid: &str,
    skip: &BTreeSet<String>,
    fn_map: &HashMap<String, &'a syn::ItemFn>,
    out: &mut AdapterOutput,
) {
    for stmt in &block.stmts {
        if let syn::Stmt::Item(item) = stmt {
            walk_items_with_evidence(
                std::slice::from_ref(item),
                source_path,
                source_cid,
                skip,
                fn_map,
                out,
            );
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
    let mut bindings: BTreeMap<String, BoundCall> = BTreeMap::new();
    for stmt in &f.block.stmts {
        update_call_bindings_from_stmt(stmt, &mut bindings);

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
        match lift_assertion_macro_at_callsites(mac, source_path, &bindings) {
            Ok(parts) => {
                for part in parts {
                    out.decls.push(ContractDecl {
                        name: part.name,
                        pre: None,
                        post: None,
                        inv: Some(part.formula),
                        out_binding: "out".into(),
                        evidence: None,
                        panic_loci: Vec::new(),
                        concept_hint: None,
                    });
                    out.lifted += 1;
                }
            }
            Err(reason) => {
                out.warnings.push(LiftWarning {
                    source_path: source_path.into(),
                    item_name: test_name.clone(),
                    reason,
                });
            }
        }
    }
}

/// Evidence-emitting variant of `visit_test_fn`. Emits both a `ContractDecl`
/// (via the same path as `visit_test_fn`) and an `EvidenceMemento` per
/// lifted callsite.
///
/// `fn_map` maps bare function names to their `syn::ItemFn` for non-test
/// functions in the same lift pass. Used to compute `test_target_function_cid`
/// (spec §10) via Option A when the callee is visible in this syn::File.
/// When the callee is not in the map (cross-crate or method call on a receiver
/// type), emits `"pending:<symbol>"` as a clearly-non-CID placeholder.
fn visit_test_fn_with_evidence(
    f: &syn::ItemFn,
    source_path: &str,
    source_cid: &str,
    fn_map: &HashMap<String, &syn::ItemFn>,
    out: &mut AdapterOutput,
) {
    let test_name = f.sig.ident.to_string();
    let mut bindings: BTreeMap<String, BoundCall> = BTreeMap::new();
    for stmt in &f.block.stmts {
        update_call_bindings_from_stmt(stmt, &mut bindings);

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
        match lift_assertion_macro_at_callsites_with_span(mac, source_path, &bindings) {
            Ok(parts) => {
                for part in parts {
                    // Emit ContractDecl (same as the basic path).
                    out.decls.push(ContractDecl {
                        name: part.name.clone(),
                        pre: None,
                        post: None,
                        inv: Some(part.formula.clone()),
                        out_binding: "out".into(),
                        evidence: None,
                        panic_loci: Vec::new(),
                        concept_hint: None,
                    });
                    out.lifted += 1;

                    // Also mint an EvidenceMemento.
                    let span_start = part.callsite_span.start();
                    let span_end = part.callsite_span.end();
                    let source_locator = SourceLocator {
                        source_cid: source_cid.to_string(),
                        span: SourceLocatorSpan {
                            // proc_macro2 col is 0-indexed; spec §1.1 mandates 0-indexed col.
                            start: SourceLocatorPoint {
                                line: span_start.line as u32,
                                col: span_start.column as u32,
                            },
                            end: SourceLocatorPoint {
                                line: span_end.line as u32,
                                col: span_end.column as u32,
                            },
                        },
                    };

                    // Build extension_fields.
                    let mut ext = BTreeMap::new();

                    // Compute test_target_function_cid (spec §10 REQUIRED for
                    // source_kind "test-assertion").
                    // Option A: if the bare callee name resolves to a fn in the
                    // same syn::File, compute the rust-algebra-term CID directly.
                    // Restriction: only free-fn callees (Expr::Call with Expr::Path)
                    // have a resolvable bare name; method calls on receivers fall
                    // through to Option B.
                    // Option B: emit "pending:<symbol>" -- a clearly-non-CID marker
                    // that is NOT a blake3-512: prefixed string, so downstream tools
                    // know this binding needs post-lift resolution.
                    let bare_callee = part.callee.split("::").last().unwrap_or(&part.callee);
                    let target_function_cid = match fn_map.get(bare_callee) {
                        Some(item_fn) => {
                            // Option A: same-file function -- compute CID via walk.
                            match rust_function_term_json_cid(item_fn, source_path) {
                                Ok(cid) => cid,
                                Err(_) => format!("pending:{}", part.callee),
                            }
                        }
                        None => {
                            // Option B: cross-crate or unresolvable -- pending marker.
                            format!("pending:{}", part.callee)
                        }
                    };
                    ext.insert(
                        "test_target_function_cid".to_string(),
                        serde_json::Value::String(target_function_cid),
                    );
                    ext.insert(
                        "test_function_name".to_string(),
                        serde_json::Value::String(test_name.clone()),
                    );
                    ext.insert(
                        "target_callsite_symbol".to_string(),
                        serde_json::Value::String(part.name.clone()),
                    );

                    // Mint the EvidenceMemento CID.
                    let ir_formula = formula_to_ir_formula(&part.formula);
                    let cid = evidence_memento_cid(
                        10000,
                        &ext,
                        AUTO_PROMOTE_LIFTER_CID,
                        &ir_formula,
                        &SourceKind::TestAssertion,
                        &source_locator,
                    );

                    out.evidences.push(EvidenceMemento {
                        cid,
                        confidence_basis_points: 10000,
                        extension_fields: ext,
                        kind: "evidence".to_string(),
                        lifter_cid: AUTO_PROMOTE_LIFTER_CID.to_string(),
                        predicate: ir_formula,
                        schema_version: "1".to_string(),
                        source_kind: SourceKind::TestAssertion,
                        source_locator,
                    });
                }
            }
            Err(reason) => {
                out.warnings.push(LiftWarning {
                    source_path: source_path.into(),
                    item_name: test_name.clone(),
                    reason,
                });
            }
        }
    }
}

fn update_call_bindings_from_stmt(stmt: &syn::Stmt, bindings: &mut BTreeMap<String, BoundCall>) {
    let syn::Stmt::Local(local) = stmt else {
        return;
    };

    let mut bound_names = Vec::new();
    collect_pat_idents(&local.pat, &mut bound_names);
    for name in &bound_names {
        bindings.remove(name);
    }

    if bound_names.len() != 1 {
        return;
    }
    let Some(init) = &local.init else {
        return;
    };
    if let Some(call) = bound_call_from_expr(&init.expr) {
        bindings.insert(bound_names.remove(0), call);
    }
}

fn collect_pat_idents(pat: &syn::Pat, out: &mut Vec<String>) {
    match pat {
        syn::Pat::Ident(p) => out.push(p.ident.to_string()),
        syn::Pat::Type(p) => collect_pat_idents(&p.pat, out),
        syn::Pat::Reference(p) => collect_pat_idents(&p.pat, out),
        syn::Pat::Paren(p) => collect_pat_idents(&p.pat, out),
        syn::Pat::Tuple(p) => {
            for elem in &p.elems {
                collect_pat_idents(elem, out);
            }
        }
        syn::Pat::TupleStruct(p) => {
            for elem in &p.elems {
                collect_pat_idents(elem, out);
            }
        }
        syn::Pat::Struct(p) => {
            for field in &p.fields {
                collect_pat_idents(&field.pat, out);
            }
        }
        syn::Pat::Slice(p) => {
            for elem in &p.elems {
                collect_pat_idents(elem, out);
            }
        }
        _ => {}
    }
}

fn bound_call_from_expr(expr: &syn::Expr) -> Option<BoundCall> {
    match expr {
        syn::Expr::Call(c) => {
            let syn::Expr::Path(p) = &*c.func else {
                return None;
            };
            Some(BoundCall {
                callee: path_final_segment(&p.path)?,
                span: c.span(),
                term: translate_term(expr).ok()?,
            })
        }
        syn::Expr::MethodCall(mc) => Some(BoundCall {
            callee: mc.method.to_string(),
            span: mc.span(),
            term: translate_term(expr).ok()?,
        }),
        syn::Expr::Paren(p) => bound_call_from_expr(&p.expr),
        syn::Expr::Reference(r) => bound_call_from_expr(&r.expr),
        syn::Expr::Cast(c) => bound_call_from_expr(&c.expr),
        _ => None,
    }
}

#[derive(Clone)]
struct AssertionCallsite {
    callee: String,
    span: proc_macro2::Span,
    /// EUF arg signature when the call's args are all literals (Some) -> the
    /// contract is named by location-independent call identity so it inherits.
    arg_sig: Option<String>,
}

fn lift_assertion_macro_at_callsites(
    mac: &syn::Macro,
    source_path: &str,
    bindings: &BTreeMap<String, BoundCall>,
) -> Result<Vec<LiftedCallsiteAssertion>, String> {
    let formula = lift_assertion_macro(mac)?;
    let exprs = assertion_observed_exprs(mac)?;
    let mut callsites = Vec::new();
    let mut substitutions: BTreeMap<String, Rc<Term>> = BTreeMap::new();
    for expr in &exprs {
        collect_expr_callsites(expr, bindings, &mut callsites, &mut substitutions);
    }
    if callsites.is_empty() {
        return Err("assertion has no identifiable callsite".into());
    }

    let formula = subst_vars_in_formula(&formula, &substitutions);
    let mut seen_names = BTreeSet::new();
    let mut out = Vec::new();
    for callsite in callsites {
        let name = callsite_contract_name(
            &callsite.callee,
            callsite.arg_sig.as_deref(),
            source_path,
            callsite.span,
        );
        if seen_names.insert(name.clone()) {
            out.push(LiftedCallsiteAssertion {
                name,
                formula: formula.clone(),
            });
        }
    }
    Ok(out)
}

fn assertion_observed_exprs(mac: &syn::Macro) -> Result<Vec<syn::Expr>, String> {
    let raw = path_to_string(&mac.path);
    let path = canonical_assertion_name(&raw).unwrap_or(raw.as_str());
    match path {
        "assert_eq" => {
            let pair: TwoExprs =
                syn::parse2(mac.tokens.clone()).map_err(|e| format!("assert_eq: parse: {e}"))?;
            Ok(vec![pair.a, pair.b])
        }
        "assert_ne" => {
            let pair: TwoExprs =
                syn::parse2(mac.tokens.clone()).map_err(|e| format!("assert_ne: parse: {e}"))?;
            Ok(vec![pair.a, pair.b])
        }
        "assert" => {
            let one: OneExpr =
                syn::parse2(mac.tokens.clone()).map_err(|e| format!("assert: parse: {e}"))?;
            Ok(vec![one.a])
        }
        "assert_matches" => {
            let pair: ScrutineePattern = syn::parse2(mac.tokens.clone())
                .map_err(|e| format!("assert_matches: parse: {e}"))?;
            Ok(vec![pair.scrutinee])
        }
        "assert_abs_diff_eq" => {
            let args: AbsDiffArgs = syn::parse2(mac.tokens.clone())
                .map_err(|e| format!("assert_abs_diff_eq: parse: {e}"))?;
            Ok(vec![args.a, args.b])
        }
        "assert_relative_eq" | "assert_ulps_eq" => {
            let args: RelativeArgs = syn::parse2(mac.tokens.clone())
                .map_err(|e| format!("{path}: parse: {e}"))?;
            Ok(vec![args.a, args.b])
        }
        other => Err(format!("not an assertion macro: {other}")),
    }
}

fn collect_expr_callsites(
    expr: &syn::Expr,
    bindings: &BTreeMap<String, BoundCall>,
    out: &mut Vec<AssertionCallsite>,
    substitutions: &mut BTreeMap<String, Rc<Term>>,
) {
    match expr {
        syn::Expr::Call(c) => {
            if let syn::Expr::Path(p) = &*c.func {
                if let Some(callee) = path_final_segment(&p.path) {
                    let arg_sig = euf_arg_sig(&callee, &c.args);
                    out.push(AssertionCallsite {
                        callee,
                        span: c.span(),
                        arg_sig,
                    });
                }
            }
        }
        syn::Expr::MethodCall(mc) => out.push(AssertionCallsite {
            callee: mc.method.to_string(),
            span: mc.span(),
            arg_sig: None,
        }),
        syn::Expr::Path(p) => {
            if let Some(id) = p.path.get_ident() {
                if let Some(binding) = bindings.get(&id.to_string()) {
                    out.push(AssertionCallsite {
                        callee: binding.callee.clone(),
                        span: binding.span,
                        arg_sig: None,
                    });
                    substitutions.insert(id.to_string(), binding.term.clone());
                }
            }
        }
        syn::Expr::Binary(b) => {
            collect_expr_callsites(&b.left, bindings, out, substitutions);
            collect_expr_callsites(&b.right, bindings, out, substitutions);
        }
        syn::Expr::Unary(u) => collect_expr_callsites(&u.expr, bindings, out, substitutions),
        syn::Expr::Paren(p) => collect_expr_callsites(&p.expr, bindings, out, substitutions),
        syn::Expr::Reference(r) => collect_expr_callsites(&r.expr, bindings, out, substitutions),
        syn::Expr::Cast(c) => collect_expr_callsites(&c.expr, bindings, out, substitutions),
        syn::Expr::Tuple(t) => {
            for elem in &t.elems {
                collect_expr_callsites(elem, bindings, out, substitutions);
            }
        }
        syn::Expr::Array(a) => {
            for elem in &a.elems {
                collect_expr_callsites(elem, bindings, out, substitutions);
            }
        }
        _ => {}
    }
}

/// The argument signature of a call whose args are all literals, in the
/// substrate-wide EUF form Python's lifter uses: `c:callresult_<callee>_a<n>(<args>)`,
/// e.g. `add(2, 3)` -> `c:callresult_add_a2(i:2,i:3)`. Returns None when any arg is
/// not a recognized literal (then the callsite stays location-keyed -- a symbolic
/// call is not a cross-crate-inheritable identity).
fn euf_arg_sig(callee: &str, args: &syn::punctuated::Punctuated<syn::Expr, syn::token::Comma>) -> Option<String> {
    let mut sigs = Vec::with_capacity(args.len());
    for a in args {
        sigs.push(literal_arg_sig(a)?);
    }
    let safe: String = callee
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();
    Some(format!("c:callresult_{}_a{}({})", safe, args.len(), sigs.join(",")))
}

/// One literal arg in canonical sig form: `i:<n>` (int), `i:-<n>` (neg int),
/// `b:<bool>`. None for anything else (a non-literal kills the whole EUF sig).
fn literal_arg_sig(e: &syn::Expr) -> Option<String> {
    match e {
        syn::Expr::Lit(syn::ExprLit { lit: syn::Lit::Int(i), .. }) => {
            Some(format!("i:{}", i.base10_digits()))
        }
        syn::Expr::Lit(syn::ExprLit { lit: syn::Lit::Bool(b), .. }) => {
            Some(format!("b:{}", b.value))
        }
        syn::Expr::Unary(u) if matches!(u.op, syn::UnOp::Neg(_)) => match &*u.expr {
            syn::Expr::Lit(syn::ExprLit { lit: syn::Lit::Int(i), .. }) => {
                Some(format!("i:-{}", i.base10_digits()))
            }
            _ => None,
        },
        syn::Expr::Group(g) => literal_arg_sig(&g.expr),
        syn::Expr::Paren(p) => literal_arg_sig(&p.expr),
        _ => None,
    }
}

/// The contract name for one observed callsite. When the call's args are all
/// literals (`arg_sig` is Some), use the location-INDEPENDENT, value-INDEPENDENT
/// EUF identity `<callee>#euf#<sig>::assertion` -- the form that lets a vendor's
/// `==5` and a consumer's `==6` about the SAME call conjoin to UNSAT across
/// crates (the numpy inheritance story). Otherwise fall back to the
/// location-keyed `<callee>@<file>:<line>:<col>` (a symbolic call is not an
/// inheritable identity).
fn callsite_contract_name(
    callee: &str,
    arg_sig: Option<&str>,
    source_path: &str,
    span: proc_macro2::Span,
) -> String {
    if let Some(sig) = arg_sig {
        return format!("{callee}#euf#{sig}::assertion");
    }
    let start = span.start();
    format!("{callee}@{source_path}:{}:{}", start.line, start.column)
}

/// Evidence-emitting variant of `lift_assertion_macro_at_callsites`.
/// Returns `LiftedCallsiteAssertionWithSpan` so callers can extract both
/// the callsite name AND the span for `SourceLocatorSpan`.
fn lift_assertion_macro_at_callsites_with_span(
    mac: &syn::Macro,
    source_path: &str,
    bindings: &BTreeMap<String, BoundCall>,
) -> Result<Vec<LiftedCallsiteAssertionWithSpan>, String> {
    let formula = lift_assertion_macro(mac)?;
    let exprs = assertion_observed_exprs(mac)?;
    let mut callsites = Vec::new();
    let mut substitutions: BTreeMap<String, Rc<Term>> = BTreeMap::new();
    for expr in &exprs {
        collect_expr_callsites(expr, bindings, &mut callsites, &mut substitutions);
    }
    if callsites.is_empty() {
        return Err("assertion has no identifiable callsite".into());
    }

    let formula = subst_vars_in_formula(&formula, &substitutions);
    let mut seen_names = BTreeSet::new();
    let mut out = Vec::new();
    for callsite in callsites {
        let name = callsite_contract_name(
            &callsite.callee,
            callsite.arg_sig.as_deref(),
            source_path,
            callsite.span,
        );
        if seen_names.insert(name.clone()) {
            out.push(LiftedCallsiteAssertionWithSpan {
                name,
                formula: formula.clone(),
                callsite_span: callsite.span,
                callee: callsite.callee.clone(),
            });
        }
    }
    Ok(out)
}

/// Convert a `provekit_ir_symbolic::Formula` (Rc-based internal repr) to
/// `provekit_ir_types::IrFormula` (serde-friendly substrate type) by
/// round-tripping through the `canonicalizer::Value` JSON encoding.
///
/// Panics on serialization failure -- if `formula_to_value` succeeded,
/// the JSON is always valid `IrFormula` JSON by construction.
fn formula_to_ir_formula(f: &Rc<Formula>) -> IrFormula {
    let cv = formula_to_value(f);
    let json_str = encode_jcs(&cv);
    serde_json::from_str::<IrFormula>(&json_str)
        .expect("formula_to_ir_formula: round-trip through JCS should always succeed")
}

/// Convert a `serde_json::Value` (from `extension_fields`) to a
/// `canonicalizer::Value` for inclusion in the JCS CID computation.
fn serde_json_to_cvalue(v: &serde_json::Value) -> Arc<CValue> {
    match v {
        serde_json::Value::Null => CValue::null(),
        serde_json::Value::Bool(b) => CValue::boolean(*b),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                CValue::integer(i)
            } else {
                // Floats are not used in extension_fields; fall back to
                // string representation (stable, deterministic).
                CValue::string(n.to_string())
            }
        }
        serde_json::Value::String(s) => CValue::string(s.clone()),
        serde_json::Value::Array(items) => {
            let cv: Vec<Arc<CValue>> = items.iter().map(serde_json_to_cvalue).collect();
            CValue::array(cv)
        }
        serde_json::Value::Object(map) => {
            // serde_json::Map preserves insertion order by default, but for
            // JCS the encoder sorts keys anyway, so any order is fine here.
            let entries: Vec<(String, Arc<CValue>)> = map
                .iter()
                .map(|(k, v)| (k.clone(), serde_json_to_cvalue(v)))
                .collect();
            Arc::new(CValue::Object(entries))
        }
    }
}

/// Compute the JCS-canonical CID for an `EvidenceMemento` by building the
/// full header tree as a `canonicalizer::Value` with the `cid` key elided,
/// then hashing with BLAKE3-512.
///
/// Locked JCS key order (alphabetical; mirrors spec §1.1):
///   confidence_basis_points, extension_fields, kind, lifter_cid,
///   predicate, schemaVersion, source_kind, source_locator
/// (Note: `cid` is elided; the hash IS the cid.)
fn evidence_memento_cid(
    confidence_basis_points: u16,
    extension_fields: &BTreeMap<String, serde_json::Value>,
    lifter_cid: &str,
    predicate: &IrFormula,
    source_kind: &SourceKind,
    source_locator: &SourceLocator,
) -> String {
    // predicate as canonical Value: serialize IrFormula -> serde_json::Value
    // -> serde_json string -> parse into CValue via encode_jcs round-trip.
    let pred_json = serde_json::to_value(predicate).expect("IrFormula must be serializable");
    let pred_cv = serde_json_to_cvalue(&pred_json);

    // extension_fields as sorted CValue object.
    let ext_entries: Vec<(String, Arc<CValue>)> = extension_fields
        .iter()
        .map(|(k, v)| (k.clone(), serde_json_to_cvalue(v)))
        .collect();
    let ext_cv = Arc::new(CValue::Object(ext_entries));

    // source_kind wire string.
    let kind_str: String = source_kind.clone().into();

    // source_locator as CValue: JCS key order: source_cid, span.
    // span: JCS key order: end, start.
    // point: JCS key order: col, line.
    let make_point = |p: &SourceLocatorPoint| {
        CValue::object([
            ("col", CValue::integer(p.col as i64)),
            ("line", CValue::integer(p.line as i64)),
        ])
    };
    let span_cv = CValue::object([
        ("end", make_point(&source_locator.span.end)),
        ("start", make_point(&source_locator.span.start)),
    ]);
    let locator_cv = CValue::object([
        (
            "source_cid",
            CValue::string(source_locator.source_cid.clone()),
        ),
        ("span", span_cv),
    ]);

    // Full header WITHOUT the cid key.
    let header = CValue::object([
        (
            "confidence_basis_points",
            CValue::integer(confidence_basis_points as i64),
        ),
        ("extension_fields", ext_cv),
        ("kind", CValue::string("evidence")),
        ("lifter_cid", CValue::string(lifter_cid.to_string())),
        ("predicate", pred_cv),
        ("schemaVersion", CValue::string("1")),
        ("source_kind", CValue::string(kind_str)),
        ("source_locator", locator_cv),
    ]);

    let canonical_bytes = encode_jcs(&header);
    blake3_512_of(canonical_bytes.as_bytes())
}

fn path_final_segment(path: &syn::Path) -> Option<String> {
    path.segments.last().map(|seg| seg.ident.to_string())
}

fn subst_vars_in_formula(
    f: &Rc<Formula>,
    substitutions: &BTreeMap<String, Rc<Term>>,
) -> Rc<Formula> {
    if substitutions.is_empty() {
        return f.clone();
    }
    match &**f {
        Formula::Atomic { name, args } => {
            let new_args: Vec<Rc<Term>> = args
                .iter()
                .map(|a| subst_vars_in_term(a, substitutions))
                .collect();
            Rc::new(Formula::Atomic {
                name: name.clone(),
                args: new_args,
            })
        }
        Formula::Connective { kind, operands } => {
            let new_ops: Vec<Rc<Formula>> = operands
                .iter()
                .map(|o| subst_vars_in_formula(o, substitutions))
                .collect();
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
            if substitutions.contains_key(name) {
                f.clone()
            } else {
                Rc::new(Formula::Quantifier {
                    kind: kind.clone(),
                    name: name.clone(),
                    sort: sort.clone(),
                    body: subst_vars_in_formula(body, substitutions),
                })
            }
        }
        Formula::Choice {
            var_name,
            sort,
            body,
        } => {
            if substitutions.contains_key(var_name) {
                f.clone()
            } else {
                Rc::new(Formula::Choice {
                    var_name: var_name.clone(),
                    sort: sort.clone(),
                    body: subst_vars_in_formula(body, substitutions),
                })
            }
        }
    }
}

fn subst_vars_in_term(t: &Rc<Term>, substitutions: &BTreeMap<String, Rc<Term>>) -> Rc<Term> {
    match &**t {
        Term::Var { name } => substitutions
            .get(name)
            .cloned()
            .unwrap_or_else(|| t.clone()),
        Term::Const { .. } => t.clone(),
        Term::Ctor { name, args } => {
            let new_args: Vec<Rc<Term>> = args
                .iter()
                .map(|a| subst_vars_in_term(a, substitutions))
                .collect();
            Rc::new(Term::Ctor {
                name: name.clone(),
                args: new_args,
            })
        }
        Term::Lambda {
            param_name,
            param_sort,
            body,
        } => {
            if substitutions.contains_key(param_name) {
                t.clone()
            } else {
                Rc::new(Term::Lambda {
                    param_name: param_name.clone(),
                    param_sort: param_sort.clone(),
                    body: subst_vars_in_term(body, substitutions),
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
                        bound_term: subst_vars_in_term(&b.bound_term, substitutions),
                    });
                    if substitutions.contains_key(&b.name) {
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
                subst_vars_in_term(body, substitutions)
            };
            Rc::new(Term::Let {
                bindings: new_bindings,
                body: new_body,
            })
        }
    }
}

/// Map an assertion-macro name to its canonical handler, LEARNING the kind from the
/// name's shape rather than a closed hand-list. The std assertions are matched
/// exactly; any other `assert*` macro is classified by the tolerance-family semantic
/// its name carries (`abs_diff`/`relative`/`ulps`), so a sibling or re-exported macro
/// (another crate's `assert_abs_diff_eq`, a `debug_assert_relative_eq`, ...) is
/// recognised WITHOUT a hardcoded entry -- the Rust mirror of deriving the vocabulary
/// from source, where the macro name is the readable signal. None for a non-assertion.
fn canonical_assertion_name(name: &str) -> Option<&'static str> {
    match name {
        "assert_eq" => Some("assert_eq"),
        "assert_ne" => Some("assert_ne"),
        "assert" => Some("assert"),
        "assert_matches" => Some("assert_matches"),
        _ if name.starts_with("assert") => {
            if name.contains("abs_diff") {
                Some("assert_abs_diff_eq")
            } else if name.contains("relative") {
                Some("assert_relative_eq")
            } else if name.contains("ulps") {
                Some("assert_ulps_eq")
            } else {
                None
            }
        }
        _ => None,
    }
}

fn is_assertion_macro(mac: &syn::Macro) -> bool {
    canonical_assertion_name(&path_to_string(&mac.path)).is_some()
}

/// `assert_abs_diff_eq!(a, b, epsilon = <e>)` from the `approx` crate. We take the
/// two values under test plus an explicit `epsilon = <numeric literal>`; any
/// further named args are ignored. Omitted/non-literal epsilon is refused upstream
/// (a type-default tolerance is not a fixed liftable bound).
struct AbsDiffArgs {
    a: syn::Expr,
    b: syn::Expr,
    epsilon: Option<syn::Expr>,
}

impl syn::parse::Parse for AbsDiffArgs {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let a: syn::Expr = input.parse()?;
        let _: syn::Token![,] = input.parse()?;
        let b: syn::Expr = input.parse()?;
        let mut epsilon = None;
        while input.peek(syn::Token![,]) {
            let _: syn::Token![,] = input.parse()?;
            if input.is_empty() {
                break;
            }
            if input.peek(syn::Ident) && input.peek2(syn::Token![=]) {
                let name: syn::Ident = input.parse()?;
                let _: syn::Token![=] = input.parse()?;
                let val: syn::Expr = input.parse()?;
                if name == "epsilon" {
                    epsilon = Some(val);
                }
            } else {
                let _: proc_macro2::TokenStream = input.parse()?;
                break;
            }
        }
        Ok(AbsDiffArgs { a, b, epsilon })
    }
}

/// A `Real`-sorted constant carried as a CANONICAL DECIMAL STRING -- the wire form
/// the four solver compilers lower as a real literal (the rust mirror of the Python
/// producer's `_ConstReal`). Discriminated from a string literal by its `Real` sort.
fn real_const(decimal: &str) -> Rc<Term> {
    Rc::new(Term::Const {
        value: ConstValue::String(decimal.to_string()),
        sort: Sort::real(),
    })
}

/// A positive numeric literal as a canonical decimal string; None if not a numeric
/// literal (refuse: the tolerance is not computable).
fn decimal_from_literal(expr: &syn::Expr) -> Option<String> {
    let lit = match expr {
        syn::Expr::Lit(l) => &l.lit,
        _ => return None,
    };
    match lit {
        syn::Lit::Float(f) => normalize_decimal(f.base10_digits()),
        syn::Lit::Int(i) => normalize_decimal(i.base10_digits()),
        _ => None,
    }
}

/// `(+eps, -eps)` canonical decimal strings from an `epsilon` literal.
fn abs_diff_bound_strings(eps: &syn::Expr) -> Option<(String, String)> {
    let pos = decimal_from_literal(eps)?;
    let neg = format!("-{pos}");
    Some((pos, neg))
}

/// Build a Term constructor (`abs`/`max`/`*` -- uninterpreted to the solvers, but a
/// FAITHFUL statement of the relation in the contract).
fn term_ctor(name: &str, args: Vec<Rc<Term>>) -> Rc<Term> {
    Rc::new(Term::Ctor {
        name: name.to_string(),
        args,
    })
}

/// `assert_relative_eq!(a, b, epsilon = <e>, max_relative = <m>)` from `approx`. We
/// require `max_relative = <numeric literal>`; `epsilon = <literal>` (optional) adds
/// the abs-fallback disjunct, matching approx's `|a-b| <= eps || |a-b| <= max(|a|,|b|)*max_rel`.
struct RelativeArgs {
    a: syn::Expr,
    b: syn::Expr,
    epsilon: Option<syn::Expr>,
    max_relative: Option<syn::Expr>,
}

impl syn::parse::Parse for RelativeArgs {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let a: syn::Expr = input.parse()?;
        let _: syn::Token![,] = input.parse()?;
        let b: syn::Expr = input.parse()?;
        let mut epsilon = None;
        let mut max_relative = None;
        while input.peek(syn::Token![,]) {
            let _: syn::Token![,] = input.parse()?;
            if input.is_empty() {
                break;
            }
            if input.peek(syn::Ident) && input.peek2(syn::Token![=]) {
                let name: syn::Ident = input.parse()?;
                let _: syn::Token![=] = input.parse()?;
                let val: syn::Expr = input.parse()?;
                if name == "epsilon" {
                    epsilon = Some(val);
                } else if name == "max_relative" {
                    max_relative = Some(val);
                }
            } else {
                let _: proc_macro2::TokenStream = input.parse()?;
                break;
            }
        }
        Ok(RelativeArgs {
            a,
            b,
            epsilon,
            max_relative,
        })
    }
}

/// Normalize a positive base-10 numeric literal (with an optional `e` exponent) to a
/// fixed-point decimal string, by shifting the decimal point with STRING arithmetic
/// -- never through an `f64`, which would lose the determinism the contract CID needs.
fn normalize_decimal(text: &str) -> Option<String> {
    let t = text.trim();
    let (mantissa, exp) = match t.split_once(['e', 'E']) {
        Some((m, e)) => (m, e.parse::<i32>().ok()?),
        None => (t, 0),
    };
    let (int_part, frac_part) = mantissa.split_once('.').unwrap_or((mantissa, ""));
    if !int_part.bytes().all(|c| c.is_ascii_digit())
        || !frac_part.bytes().all(|c| c.is_ascii_digit())
        || (int_part.is_empty() && frac_part.is_empty())
    {
        return None;
    }
    let digits = format!("{int_part}{frac_part}");
    let point = int_part.len() as i32 + exp; // decimal point sits after `point` digits
    let s = if point <= 0 {
        format!("0.{}{}", "0".repeat((-point) as usize), digits)
    } else if (point as usize) >= digits.len() {
        format!("{}{}.0", digits, "0".repeat(point as usize - digits.len()))
    } else {
        let (a, b) = digits.split_at(point as usize);
        format!("{a}.{b}")
    };
    Some(s)
}

fn lift_assertion_macro(mac: &syn::Macro) -> Result<Rc<Formula>, String> {
    let raw = path_to_string(&mac.path);
    let path = canonical_assertion_name(&raw).unwrap_or(raw.as_str());
    match path {
        "assert_eq" => {
            let pair: TwoExprs =
                syn::parse2(mac.tokens.clone()).map_err(|e| format!("assert_eq: parse: {e}"))?;
            let l = translate_term(&pair.a)?;
            let r = translate_term(&pair.b)?;
            Ok(eq(l, r))
        }
        "assert_ne" => {
            let pair: TwoExprs =
                syn::parse2(mac.tokens.clone()).map_err(|e| format!("assert_ne: parse: {e}"))?;
            let l = translate_term(&pair.a)?;
            let r = translate_term(&pair.b)?;
            Ok(ne(l, r))
        }
        "assert" => {
            // Only the `assert!(<expr>)` form. The expression must be a
            // top-level binary comparison.
            let one: OneExpr =
                syn::parse2(mac.tokens.clone()).map_err(|e| format!("assert: parse: {e}"))?;
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
        "assert_abs_diff_eq" => {
            // approx: `|a - b| <= epsilon`. Lifted two-sided -- `(a-b) <= eps ∧
            // (a-b) >= -eps` -- needing only `-` and `<=`/`>=`; no abs term. The
            // bound rides as a `Real` const (canonical decimal). Mirror of numpy's
            // assert_almost_equal.
            let args: AbsDiffArgs = syn::parse2(mac.tokens.clone())
                .map_err(|e| format!("assert_abs_diff_eq: parse: {e}"))?;
            let eps = args.epsilon.as_ref().ok_or_else(|| {
                "assert_abs_diff_eq without `epsilon = <literal>` uses a type-default \
                 tolerance; refused (not a fixed liftable bound)"
                    .to_string()
            })?;
            let (pos, neg) = abs_diff_bound_strings(eps).ok_or_else(|| {
                "assert_abs_diff_eq: `epsilon` is not a numeric literal; tolerance not computable"
                    .to_string()
            })?;
            let l = translate_term(&args.a)?;
            let r = translate_term(&args.b)?;
            let diff = Rc::new(Term::Ctor {
                name: "-".into(),
                args: vec![l, r],
            });
            Ok(and_(vec![
                lte(diff.clone(), real_const(&pos)),
                gte(diff, real_const(&neg)),
            ]))
        }
        "assert_relative_eq" => {
            // approx: `|a-b| <= eps  ||  |a-b| <= max_relative * max(|a|,|b|)`. Lifted
            // FAITHFULLY with `abs`/`max`/`*` ctors (uninterpreted to solvers, like
            // the deferred Python allclose -- the witness carries discharge; the
            // contract states the relation). `max_relative` literal required.
            let args: RelativeArgs = syn::parse2(mac.tokens.clone())
                .map_err(|e| format!("assert_relative_eq: parse: {e}"))?;
            let max_rel = args.max_relative.as_ref().ok_or_else(|| {
                "assert_relative_eq without `max_relative = <literal>` uses a type-default; \
                 refused (not a fixed liftable bound)"
                    .to_string()
            })?;
            let max_rel_dec = decimal_from_literal(max_rel).ok_or_else(|| {
                "assert_relative_eq: `max_relative` is not a numeric literal; not computable"
                    .to_string()
            })?;
            let l = translate_term(&args.a)?;
            let r = translate_term(&args.b)?;
            let abs_diff = term_ctor("abs", vec![term_ctor("-", vec![l.clone(), r.clone()])]);
            let relative = lte(
                abs_diff.clone(),
                term_ctor(
                    "*",
                    vec![
                        real_const(&max_rel_dec),
                        term_ctor("max", vec![term_ctor("abs", vec![l]), term_ctor("abs", vec![r])]),
                    ],
                ),
            );
            match &args.epsilon {
                Some(eps) => {
                    let eps_dec = decimal_from_literal(eps).ok_or_else(|| {
                        "assert_relative_eq: `epsilon` is not a numeric literal; not computable"
                            .to_string()
                    })?;
                    Ok(or_(vec![lte(abs_diff, real_const(&eps_dec)), relative]))
                }
                None => Ok(relative),
            }
        }
        "assert_ulps_eq" => Err(
            "assert_ulps_eq compares ULP distance; not algebraic, refused (no fixed real bound)"
                .to_string(),
        ),
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
        syn::Expr::Unary(u) if matches!(u.op, syn::UnOp::Not(_)) => {
            let term = translate_term(&u.expr)?;
            Ok(eq(term, bool_ctor(false)))
        }
        _ => {
            let term = translate_term(expr)?;
            Ok(eq(term, bool_ctor(true)))
        }
    }
}

fn bool_ctor(value: bool) -> Rc<Term> {
    Rc::new(Term::Ctor {
        name: if value { "True" } else { "False" }.into(),
        args: vec![],
    })
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
            syn::Lit::Float(lf) => {
                // A float literal lifts as a `Real` const (canonical decimal) -- the
                // numeric-real counterpart to an integer literal's `Int`. This is the
                // integral/real distinction the numeric hierarchy needs: i16/i32/i64
                // literals stay integral (`num`); float literals are Real.
                let dec = normalize_decimal(lf.base10_digits()).ok_or_else(|| {
                    format!("float literal not normalizable: {}", lf.base10_digits())
                })?;
                Ok(real_const(&dec))
            }
            syn::Lit::Bool(lb) => Ok(bool_ctor(lb.value)),
            _ => Err(
                "only integer, float, string, byte-string, and bool literals are liftable in v0.5"
                    .into(),
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
            return Err(
                input.error("vec![v; n] repeat-form not lifted in v0.5 (only finite list form)")
            );
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

    // --- approx::assert_abs_diff_eq! -> real tolerance bound (mirror of numpy's
    //     assert_almost_equal) ---------------------------------------------------

    #[test]
    fn abs_diff_eq_is_recognized_as_an_assertion_macro() {
        let mac: syn::Macro = syn::parse_quote!(assert_abs_diff_eq!(x, y, epsilon = 0.001));
        assert!(is_assertion_macro(&mac));
    }

    #[test]
    fn abs_diff_eq_lifts_to_two_sided_real_bound() {
        let mac: syn::Macro = syn::parse_quote!(assert_abs_diff_eq!(x, y, epsilon = 1e-6));
        let f = lift_assertion_macro(&mac).expect("abs_diff_eq must lift");
        let dump = format!("{f:?}");
        // a conjunction of two comparisons over the difference, bounded by a Real.
        assert!(
            matches!(&*f, Formula::Connective { kind, .. } if kind == "and"),
            "expected an `and` conjunction: {dump}"
        );
        assert!(dump.contains("Real"), "bound must be Real-sorted: {dump}");
        assert!(dump.contains("0.000001"), "1e-6 normalized to decimal: {dump}");
        assert!(dump.contains("-0.000001"), "two-sided lower bound: {dump}");
    }

    #[test]
    fn abs_diff_eq_without_epsilon_is_refused() {
        // a type-default tolerance is not a fixed liftable bound.
        let mac: syn::Macro = syn::parse_quote!(assert_abs_diff_eq!(x, y));
        assert!(lift_assertion_macro(&mac).is_err());
    }

    #[test]
    fn abs_diff_eq_non_literal_epsilon_is_refused() {
        // tolerance not computable at lift time -> refuse, never guess.
        let mac: syn::Macro = syn::parse_quote!(assert_abs_diff_eq!(x, y, epsilon = tol));
        assert!(lift_assertion_macro(&mac).is_err());
    }

    #[test]
    fn normalize_decimal_shifts_the_point_without_floats() {
        assert_eq!(normalize_decimal("1e-6").as_deref(), Some("0.000001"));
        assert_eq!(normalize_decimal("1.5e-3").as_deref(), Some("0.0015"));
        assert_eq!(normalize_decimal("0.001").as_deref(), Some("0.001"));
        assert_eq!(normalize_decimal("2").as_deref(), Some("2.0"));
        assert!(normalize_decimal("nan").is_none());
    }

    #[test]
    fn relative_eq_lifts_the_faithful_relative_bound() {
        let mac: syn::Macro = syn::parse_quote!(assert_relative_eq!(x, y, max_relative = 0.01));
        let f = lift_assertion_macro(&mac).expect("relative_eq must lift");
        let dump = format!("{f:?}");
        assert!(dump.contains("max"), "uses max(|a|,|b|): {dump}");
        assert!(dump.contains("abs"), "uses abs: {dump}");
        assert!(dump.contains("0.01"), "max_relative bound present: {dump}");
    }

    #[test]
    fn relative_eq_with_epsilon_adds_the_abs_fallback_disjunct() {
        let mac: syn::Macro =
            syn::parse_quote!(assert_relative_eq!(x, y, epsilon = 1e-6, max_relative = 0.01));
        let f = lift_assertion_macro(&mac).expect("lift");
        assert!(
            matches!(&*f, Formula::Connective { kind, .. } if kind == "or"),
            "epsilon adds an OR fallback: {f:?}"
        );
    }

    #[test]
    fn relative_eq_without_max_relative_is_refused() {
        let mac: syn::Macro = syn::parse_quote!(assert_relative_eq!(x, y));
        assert!(lift_assertion_macro(&mac).is_err());
    }

    #[test]
    fn ulps_eq_is_recognized_but_refused() {
        // ULP distance is not algebraic: claimed (so it is not silently skipped)
        // but loud-refused.
        let mac: syn::Macro = syn::parse_quote!(assert_ulps_eq!(x, y, max_ulps = 4));
        assert!(is_assertion_macro(&mac));
        assert!(lift_assertion_macro(&mac).is_err());
    }

    #[test]
    fn vocabulary_is_learned_from_the_name_not_a_closed_list() {
        // exact std + approx names classify as themselves
        assert_eq!(canonical_assertion_name("assert_eq"), Some("assert_eq"));
        assert_eq!(canonical_assertion_name("assert"), Some("assert"));
        assert_eq!(canonical_assertion_name("assert_abs_diff_eq"), Some("assert_abs_diff_eq"));
        // SIBLING tolerance macros never named in any hand-list are classified by the
        // semantic shape of their name -- the vocabulary is open, learned from source.
        assert_eq!(canonical_assertion_name("assert_abs_diff_ne"), Some("assert_abs_diff_eq"));
        assert_eq!(canonical_assertion_name("assert_relative_ne"), Some("assert_relative_eq"));
        assert_eq!(canonical_assertion_name("assert_ulps_ne"), Some("assert_ulps_eq"));
        // non-assertions and unknown assert-shaped names are not misclassified
        assert_eq!(canonical_assertion_name("println"), None);
        assert_eq!(canonical_assertion_name("assert_something_else"), None);
    }

    #[test]
    fn integer_and_float_literals_take_distinct_sorts() {
        // the integral/real distinction at the literal level: i16/i32/i64 literals
        // stay Int (integral); float literals are Real.
        let int_eq: syn::Macro = syn::parse_quote!(assert_eq!(f(a), 5));
        let idump = format!("{:?}", lift_assertion_macro(&int_eq).unwrap());
        assert!(idump.contains("Int"), "integer literal is Int: {idump}");

        let float_eq: syn::Macro = syn::parse_quote!(assert_eq!(g(a), 2.5));
        let fdump = format!("{:?}", lift_assertion_macro(&float_eq).unwrap());
        assert!(fdump.contains("Real"), "float literal is Real: {fdump}");
        assert!(fdump.contains("2.5"), "float decimal preserved: {fdump}");
    }

    fn assert_callsite_name(name: &str, callee: &str) {
        // Literal-arg calls now carry the location-INDEPENDENT EUF identity
        // (`callee#euf#c:callresult_..._aN(...)::assertion`); method / symbolic-arg
        // calls keep the location form (`callee@file:line:col`). Accept whichever
        // applies -- the contract must be ABOUT this callee either way.
        let euf_prefix = format!("{callee}#euf#");
        if !name.starts_with(&euf_prefix) {
            let prefix = format!("{callee}@t.rs:");
            assert!(
                name.starts_with(&prefix),
                "expected `{name}` to start with `{prefix}` or `{euf_prefix}`"
            );
            let rest = &name[prefix.len()..];
            let parts: Vec<_> = rest.split(':').collect();
            assert_eq!(parts.len(), 2, "expected <line>:<col>, got `{rest}`");
            assert!(parts[0].parse::<usize>().unwrap() > 0);
            parts[1].parse::<usize>().unwrap();
        }
        assert!(
            !name.starts_with("parse_int_42::") && !name.starts_with("three_facts::"),
            "old test-owned name leaked: {name}"
        );
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
        assert_callsite_name(&out.decls[0].name, "parse_int");
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
        assert_eq!(names.len(), 3);
        for name in names {
            assert_callsite_name(name, "f");
        }
    }

    #[test]
    fn lifts_assert_with_binop() {
        let src = r#"
            #[test]
            fn nonneg_one() {
                assert!(some_value() > 0);
            }
        "#;
        let f = parse(src);
        let out = lift_file(&f, "t.rs");
        assert_eq!(out.lifted, 1, "warnings: {:?}", out.warnings);
        assert_callsite_name(&out.decls[0].name, "some_value");
    }

    #[test]
    fn lifts_assert_bool_method_call_as_true() {
        let src = r#"
            #[test]
            fn option_is_some() {
                assert!(Some(1).is_some());
            }
        "#;
        let f = parse(src);
        let out = lift_file(&f, "t.rs");
        assert_eq!(out.lifted, 1, "warnings: {:?}", out.warnings);
        assert_callsite_name(&out.decls[0].name, "is_some");
        let inv = out.decls[0].inv.as_ref().expect("inv");
        match &**inv {
            Formula::Atomic { name, args } if name == "=" && args.len() == 2 => match &*args[1] {
                Term::Ctor { name, args } => {
                    assert_eq!(name, "True");
                    assert!(args.is_empty());
                }
                other => panic!("expected True ctor, got {other:?}"),
            },
            other => panic!("expected atomic =, got {other:?}"),
        }
    }

    #[test]
    fn lifts_negated_assert_bool_method_call_as_false() {
        let src = r#"
            #[test]
            fn option_not_none() {
                assert!(!Some(1).is_none());
            }
        "#;
        let f = parse(src);
        let out = lift_file(&f, "t.rs");
        assert_eq!(out.lifted, 1, "warnings: {:?}", out.warnings);
        assert_callsite_name(&out.decls[0].name, "is_none");
        let inv = out.decls[0].inv.as_ref().expect("inv");
        match &**inv {
            Formula::Atomic { name, args } if name == "=" && args.len() == 2 => match &*args[1] {
                Term::Ctor { name, args } => {
                    assert_eq!(name, "False");
                    assert!(args.is_empty());
                }
                other => panic!("expected False ctor, got {other:?}"),
            },
            other => panic!("expected atomic =, got {other:?}"),
        }
    }

    #[test]
    fn let_bound_call_attaches_to_bound_callsite() {
        let src = r#"
            #[test]
            fn test_foo() {
                let r = foo(5);
                assert_eq!(r, 10);
            }
        "#;
        let f = parse(src);
        let out = lift_file(&f, "t.rs");
        assert_eq!(out.lifted, 1, "warnings: {:?}", out.warnings);
        assert_callsite_name(&out.decls[0].name, "foo");
        let inv = out.decls[0].inv.as_ref().expect("inv");
        let lhs = match &**inv {
            Formula::Atomic { name, args } if name == "=" && args.len() == 2 => args[0].clone(),
            other => panic!("expected atomic =, got {other:?}"),
        };
        match &*lhs {
            Term::Ctor { name, args } => {
                assert_eq!(name, "foo");
                assert_eq!(args.len(), 1);
            }
            other => panic!("expected let binding to substitute to foo call, got {other:?}"),
        }
    }

    #[test]
    fn skips_literal_assertion_without_callsite() {
        let src = r#"
            #[test]
            fn literal_fact() {
                assert_eq!(1, 1);
            }
        "#;
        let f = parse(src);
        let out = lift_file(&f, "t.rs");
        assert_eq!(out.seen, 1);
        assert_eq!(out.lifted, 0);
        assert_eq!(out.warnings.len(), 1);
        assert!(
            out.warnings[0].reason.contains("callsite"),
            "warning should explain no callsite: {:?}",
            out.warnings[0]
        );
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
        assert_callsite_name(&out.decls[0].name, "g");
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
        assert_callsite_name(&out.decls[0].name, "len");
        // Inspect the IR: the LHS should be Ctor("len", [str_const("foo")]).
        let inv = out.decls[0].inv.as_ref().expect("inv");
        let lhs = match &**inv {
            Formula::Atomic { name, args } if name == "=" && args.len() == 2 => args[0].clone(),
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
            Formula::Atomic { name, args } if name == "=" && args.len() == 2 => args[0].clone(),
            other => panic!("expected atomic =, got {other:?}"),
        };
        // outer: encode_jcs(<one arg>)
        let inner = match &*lhs {
            Term::Ctor { name, args } if name == "encode_jcs" && args.len() == 1 => args[0].clone(),
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
        assert_eq!(out.lifted, 2, "warnings: {:?}", out.warnings);
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
        assert_eq!(out.lifted, 2, "warnings: {:?}", out.warnings);
    }

    #[test]
    fn skips_format_macro_in_operand_position() {
        // `format!(...)` is `Expr::Macro`: explicitly NOT lifted in v0.5.
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
            out.warnings[0].reason.contains("macro") && out.warnings[0].reason.contains("format"),
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

    // -----------------------------------------------------------------------
    // EvidenceMemento emission tests (lift_file_with_evidence)
    // -----------------------------------------------------------------------

    #[test]
    fn lift_with_evidence_emits_evidence_alongside_decls() {
        let src = r#"
            #[test]
            fn parse_int_42() {
                assert_eq!(parse_int("42"), 42);
            }
        "#;
        let f = parse(src);
        let src_bytes = src.as_bytes();
        let out = lift_file_with_evidence(&f, "t.rs", src_bytes);
        // Both paths populated.
        assert_eq!(out.lifted, 1, "warnings: {:?}", out.warnings);
        assert_eq!(out.decls.len(), 1);
        assert_eq!(out.evidences.len(), 1);
    }

    #[test]
    fn evidence_source_kind_is_test_assertion() {
        let src = r#"
            #[test]
            fn check_something() {
                assert_eq!(f(1), 1);
            }
        "#;
        let f = parse(src);
        let out = lift_file_with_evidence(&f, "t.rs", src.as_bytes());
        assert_eq!(out.evidences.len(), 1);
        assert_eq!(
            out.evidences[0].source_kind,
            provekit_ir_types::SourceKind::TestAssertion
        );
    }

    #[test]
    fn evidence_fields_are_correct() {
        let src = r#"
            #[test]
            fn check_something() {
                assert_eq!(f(1), 1);
            }
        "#;
        let f = parse(src);
        let out = lift_file_with_evidence(&f, "t.rs", src.as_bytes());
        let ev = &out.evidences[0];
        assert_eq!(ev.kind, "evidence");
        assert_eq!(ev.schema_version, "1");
        assert_eq!(ev.confidence_basis_points, 10000);
        assert_eq!(ev.lifter_cid, AUTO_PROMOTE_LIFTER_CID);
        assert!(
            ev.cid.starts_with("blake3-512:"),
            "cid should be blake3-512 prefixed, got: {}",
            ev.cid
        );
        assert_eq!(
            ev.cid.len(),
            11 + 128,
            "cid should be prefix + 128 hex chars"
        );
    }

    #[test]
    fn evidence_extension_fields_contain_test_function_name() {
        let src = r#"
            #[test]
            fn my_specific_test() {
                assert_eq!(compute(5), 10);
            }
        "#;
        let f = parse(src);
        let out = lift_file_with_evidence(&f, "t.rs", src.as_bytes());
        assert_eq!(out.evidences.len(), 1);
        let ev = &out.evidences[0];
        let name = ev
            .extension_fields
            .get("test_function_name")
            .and_then(|v| v.as_str())
            .expect("test_function_name must be present");
        assert_eq!(name, "my_specific_test");
    }

    #[test]
    fn evidence_extension_fields_contain_target_callsite_symbol() {
        let src = r#"
            #[test]
            fn check_compute() {
                assert_eq!(compute(5), 10);
            }
        "#;
        let f = parse(src);
        let out = lift_file_with_evidence(&f, "t.rs", src.as_bytes());
        assert_eq!(out.evidences.len(), 1);
        let ev = &out.evidences[0];
        let symbol = ev
            .extension_fields
            .get("target_callsite_symbol")
            .and_then(|v| v.as_str())
            .expect("target_callsite_symbol must be present");
        // Symbol is the callsite identity: the EUF form `compute#euf#...` for a
        // literal-arg call, else `compute@<file>:<line>:<col>`.
        assert!(
            symbol.starts_with("compute#euf#") || symbol.starts_with("compute@t.rs:"),
            "got: {symbol}"
        );
    }

    #[test]
    fn evidence_test_target_function_cid_present_and_valid_spec_b2() {
        // Spec §10: `test_target_function_cid` is REQUIRED in extension_fields
        // for source_kind "test-assertion". When the callee is in the same
        // syn::File (Option A), the CID must be a valid blake3-512:... string
        // matching the independently-computed function-term CID of the callee.
        //
        // Source has both the production fn `double` and a test that calls it,
        // mirroring the common `mod tests { use super::*; }` inline pattern.
        let src = "fn double(x: i32) -> i32 { x * 2 }\n\
                   #[test]\nfn test_double() {\nassert_eq!(double(3), 6);\n}\n";
        let parsed = parse(src);
        let out = lift_file_with_evidence(&parsed, "t.rs", src.as_bytes());
        assert_eq!(out.evidences.len(), 1, "warnings: {:?}", out.warnings);
        let ev = &out.evidences[0];

        // Field must be present.
        let cid_val = ev
            .extension_fields
            .get("test_target_function_cid")
            .and_then(|v| v.as_str())
            .expect("test_target_function_cid must be present in extension_fields");

        // Must be a valid blake3-512: CID (Option A resolved), not a pending marker.
        assert!(
            cid_val.starts_with("blake3-512:"),
            "test_target_function_cid should be a blake3-512 CID when callee is \
             in the same file; got: {cid_val}"
        );
        assert_eq!(
            cid_val.len(),
            11 + 128,
            "blake3-512 CID should be prefix (11) + 128 hex chars; got len {}",
            cid_val.len()
        );

        // CID must match the independently-computed function-term CID for `double`.
        let double_fn = parsed
            .items
            .iter()
            .find_map(|item| match item {
                syn::Item::Fn(f) if f.sig.ident == "double" => Some(f),
                _ => None,
            })
            .expect("double fn must be in parsed items");
        let expected_cid = provekit_walk::emit::rust_function_term_json_cid(double_fn, "t.rs")
            .expect("rust_function_term_json_cid must succeed for simple fn");
        assert_eq!(
            cid_val, expected_cid,
            "test_target_function_cid must match the lifted CID of the target function"
        );
    }

    #[test]
    fn evidence_test_target_function_cid_pending_when_cross_crate_spec_b2() {
        // When the callee is NOT in the same syn::File (cross-crate call),
        // the lifter must emit a "pending:<symbol>" marker rather than a
        // fabricated CID. This is the Option B path.
        let src = "#[test]\nfn test_external() {\nassert_eq!(external_fn(1), 2);\n}\n";
        let parsed = parse(src);
        let out = lift_file_with_evidence(&parsed, "t.rs", src.as_bytes());
        assert_eq!(out.evidences.len(), 1, "warnings: {:?}", out.warnings);
        let ev = &out.evidences[0];

        let cid_val = ev
            .extension_fields
            .get("test_target_function_cid")
            .and_then(|v| v.as_str())
            .expect("test_target_function_cid must be present even for cross-crate callees");

        assert!(
            cid_val.starts_with("pending:"),
            "cross-crate callee should emit pending:<symbol>; got: {cid_val}"
        );
        assert!(
            !cid_val.starts_with("blake3-512:"),
            "pending marker must NOT look like a real CID; got: {cid_val}"
        );
    }

    #[test]
    fn evidence_source_locator_col_is_0indexed_spec_b1() {
        // Spec §1.1 normative: col counts UTF-8 BYTES within the line, 0-indexed.
        // proc_macro2 with span-locations also uses 0-indexed columns, so the
        // raw column value must be used WITHOUT adding 1.
        //
        // Source layout (line numbers are 1-based per spec):
        //   line 1: ""  (raw string starts with newline)
        //   line 2: "#[test]"
        //   line 3: "fn span_pin() {"
        //   line 4: "assert_eq!(f(0), 0);"  -- f is at byte 11 (0-indexed) on this line
        //   line 5: "}"
        //
        // In the raw string below the indentation is 0 (no leading spaces) so
        // the column is unambiguous and cross-platform stable.
        let src = "#[test]\nfn span_pin() {\nassert_eq!(f(0), 0);\n}\n";
        let f = parse(src);
        let out = lift_file_with_evidence(&f, "t.rs", src.as_bytes());
        assert_eq!(out.evidences.len(), 1, "warnings: {:?}", out.warnings);
        let ev = &out.evidences[0];
        // "assert_eq!(" is 11 bytes; f is at col 11 (0-indexed).
        assert_eq!(
            ev.source_locator.span.start.col, 11,
            "col should be 0-indexed (spec §1.1); got {}. \
             If this is 12, the +1 bug was re-introduced.",
            ev.source_locator.span.start.col
        );
        // Line 3 (1-indexed) is where assert_eq!(f(0), ...) lives.
        assert_eq!(
            ev.source_locator.span.start.line, 3,
            "line should be 1-indexed; got {}",
            ev.source_locator.span.start.line
        );
    }

    #[test]
    fn evidence_source_cid_matches_source_bytes_hash() {
        let src = r#"
            #[test]
            fn hash_stable() {
                assert_eq!(f(1), 1);
            }
        "#;
        let f = parse(src);
        let src_bytes = src.as_bytes();
        let expected_source_cid = provekit_canonicalizer::blake3_512_of(src_bytes);
        let out = lift_file_with_evidence(&f, "t.rs", src_bytes);
        assert_eq!(out.evidences.len(), 1);
        assert_eq!(
            out.evidences[0].source_locator.source_cid,
            expected_source_cid
        );
    }

    #[test]
    fn evidence_cid_is_deterministic() {
        // Same source, same output -> same CID on every call.
        let src = r#"
            #[test]
            fn deterministic_cid() {
                assert_eq!(f(1), 1);
            }
        "#;
        let f = parse(src);
        let src_bytes = src.as_bytes();
        let out1 = lift_file_with_evidence(&f, "t.rs", src_bytes);
        let out2 = lift_file_with_evidence(&f, "t.rs", src_bytes);
        assert_eq!(out1.evidences.len(), 1);
        assert_eq!(out2.evidences.len(), 1);
        assert_eq!(out1.evidences[0].cid, out2.evidences[0].cid);
    }

    #[test]
    fn evidence_three_assertions_yields_three_evidences() {
        let src = r#"
            #[test]
            fn three_facts() {
                assert_eq!(f(1), 1);
                assert_eq!(f(2), 2);
                assert_ne!(f(3), 0);
            }
        "#;
        let f = parse(src);
        let out = lift_file_with_evidence(&f, "t.rs", src.as_bytes());
        assert_eq!(out.lifted, 3, "warnings: {:?}", out.warnings);
        assert_eq!(out.evidences.len(), 3);
        // All evidence CIDs should be distinct (different spans + formulas).
        let cids: std::collections::HashSet<_> = out.evidences.iter().map(|e| &e.cid).collect();
        assert_eq!(cids.len(), 3, "CIDs should be distinct");
    }

    #[test]
    fn lift_with_evidence_skips_literal_only_assertion() {
        // Same skip-behavior as the basic path: no callsite means no evidence.
        let src = r#"
            #[test]
            fn literal_fact() {
                assert_eq!(1, 1);
            }
        "#;
        let f = parse(src);
        let out = lift_file_with_evidence(&f, "t.rs", src.as_bytes());
        assert_eq!(out.seen, 1);
        assert_eq!(out.lifted, 0);
        assert_eq!(out.evidences.len(), 0);
        assert_eq!(out.warnings.len(), 1);
    }

    #[test]
    fn basic_lift_does_not_populate_evidences() {
        // Verify backward compat: lift_file() leaves evidences empty.
        let src = r#"
            #[test]
            fn parse_int_42() {
                assert_eq!(parse_int("42"), 42);
            }
        "#;
        let f = parse(src);
        let out = lift_file(&f, "t.rs");
        assert_eq!(out.lifted, 1);
        assert!(
            out.evidences.is_empty(),
            "lift_file should not populate evidences"
        );
    }
}
