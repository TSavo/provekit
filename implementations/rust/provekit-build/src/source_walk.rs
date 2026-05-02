// SPDX-License-Identifier: Apache-2.0
//
// Source walker for the consumer crate's `src/` tree.
//
// Reads each `*.rs` file via `walkdir`, parses with `syn::parse_file`,
// and uses a `syn::visit::Visit` impl to:
//
//   * record every `#[provekit::contract(...)]` annotation: the
//     decorated function's ident, source path, line, and a coarse
//     classification of the post-condition shape;
//   * record every `#[provekit::verify]` annotation;
//   * inside each verify-target body, record every method-call /
//     free-function call along with any nearby `==` literal check
//     against the call site's bound name (the `if x == 0` shape).
//
// The classifier for post-condition shapes is intentionally tiny: it
// recognizes `gte(out(), num(N))`, `gt(out(), num(N))`, and
// `eq(out(), num(N))`. Anything else is classified `Opaque`. The
// recognizer works on the macro attribute's TOKEN STREAM, never on
// runtime values; a richer shape vocabulary requires either a real
// kit-aware lifter (separate crate) or evaluating the consumer's code,
// neither of which is in scope for v0.

use std::path::{Path, PathBuf};

use proc_macro2::TokenStream as TokenStream2;
use syn::spanned::Spanned;
use syn::visit::Visit;
use syn::{Attribute, BinOp, Expr, ExprBinary, ExprCall, ExprIf, ExprLit, ExprMethodCall, ExprPath,
    ItemFn, Lit, Meta};

/// Coarse classification of a `post = ...` formula. The build-script
/// verifier maps each shape to a deterministic SMT-LIB encoding;
/// `Opaque` skips the encoding and yields `undecidable`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FormulaShape {
    /// `gte(out(), num(N))` — return value is at least N.
    GteConst(i64),
    /// `gt(out(), num(N))` — return value is strictly greater than N.
    GtConst(i64),
    /// `eq(out(), num(N))` — return value is exactly N.
    EqConst(i64),
    /// Anything we couldn't classify. The verifier flags as undecidable.
    Opaque,
}

impl FormulaShape {
    pub fn label(&self) -> &'static str {
        match self {
            Self::GteConst(_) => "gte_const",
            Self::GtConst(_) => "gt_const",
            Self::EqConst(_) => "eq_const",
            Self::Opaque => "opaque",
        }
    }
}

#[derive(Debug, Clone)]
pub struct ContractSite {
    pub fn_name: String,
    pub source_path: PathBuf,
    pub line: usize,
    pub post_shape: FormulaShape,
}

#[derive(Debug, Clone)]
pub struct VerifySite {
    pub fn_name: String,
    pub source_path: PathBuf,
    pub line: usize,
}

#[derive(Debug, Clone)]
pub struct ImplementSite {
    pub fn_name: String,
    pub source_path: PathBuf,
    pub line: usize,
    pub target_contract: String,
}

/// One call site inside a verify-target body. `surrounding_eq_check`
/// is `Some(N)` when the verify body contains an `if <bound> == N`
/// referring to the call's let-binding; this is what flips a discharged
/// contract into a counterexample for the demo's
/// `deliberate_violation` shape.
#[derive(Debug, Clone)]
pub struct CallSite {
    pub verify_fn: String,
    pub callee: String,
    pub source_path: PathBuf,
    pub line: usize,
    pub surrounding_eq_check: Option<i64>,
}

#[derive(Debug, Default, Clone)]
pub struct WalkOutcome {
    pub contracts: Vec<ContractSite>,
    pub verify_targets: Vec<VerifySite>,
    pub implements: Vec<ImplementSite>,
    pub callsites: Vec<CallSite>,
}

/// Walk a crate manifest dir's `src/` tree.
pub fn walk(manifest_dir: &Path) -> WalkOutcome {
    let src_dir = manifest_dir.join("src");
    let mut out = WalkOutcome::default();
    if !src_dir.exists() {
        return out;
    }
    for entry in walkdir::WalkDir::new(&src_dir)
        .follow_links(false)
        .into_iter()
        .flatten()
    {
        if !entry.file_type().is_file() {
            continue;
        }
        if entry.path().extension().and_then(|s| s.to_str()) != Some("rs") {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(entry.path()) else {
            continue;
        };
        let parsed = match syn::parse_file(&text) {
            Ok(p) => p,
            Err(_) => continue, // ignore unparseable files; not our error to surface
        };
        let mut visitor = FileVisitor {
            source_path: entry.path().to_path_buf(),
            out: &mut out,
            current_verify_fn: None,
            current_let_eq_const: Vec::new(),
        };
        visitor.visit_file(&parsed);
    }
    // Stable order for downstream determinism.
    out.contracts.sort_by(|a, b| {
        a.source_path
            .cmp(&b.source_path)
            .then_with(|| a.fn_name.cmp(&b.fn_name))
    });
    out.verify_targets.sort_by(|a, b| {
        a.source_path
            .cmp(&b.source_path)
            .then_with(|| a.fn_name.cmp(&b.fn_name))
    });
    out.callsites.sort_by(|a, b| {
        a.source_path
            .cmp(&b.source_path)
            .then_with(|| a.line.cmp(&b.line))
            .then_with(|| a.callee.cmp(&b.callee))
    });
    out
}

/// Test entrypoint: parse a single source string at a synthetic path.
pub fn walk_str(source_path: &Path, text: &str) -> WalkOutcome {
    let mut out = WalkOutcome::default();
    let parsed = match syn::parse_file(text) {
        Ok(p) => p,
        Err(_) => return out,
    };
    let mut visitor = FileVisitor {
        source_path: source_path.to_path_buf(),
        out: &mut out,
        current_verify_fn: None,
        current_let_eq_const: Vec::new(),
    };
    visitor.visit_file(&parsed);
    out
}

// ---------------------------------------------------------------------------
// syn visitor
// ---------------------------------------------------------------------------

struct FileVisitor<'a> {
    source_path: PathBuf,
    out: &'a mut WalkOutcome,
    current_verify_fn: Option<String>,
    /// Stack of (binding-name -> equality-checked-int) seen at the
    /// current scope. We push when entering a verify-fn body and any
    /// nested block; pop on exit. The CallSite's
    /// `surrounding_eq_check` is filled by scanning this stack at
    /// callsite recording time.
    current_let_eq_const: Vec<(String, i64)>,
}

impl<'a> FileVisitor<'a> {
    fn record_contract(&mut self, item_fn: &ItemFn, attr: &Attribute) {
        let name = item_fn.sig.ident.to_string();
        let line = attr.span().start().line;
        let shape = classify_post(attr);
        self.out.contracts.push(ContractSite {
            fn_name: name,
            source_path: self.source_path.clone(),
            line,
            post_shape: shape,
        });
    }

    fn record_verify(&mut self, item_fn: &ItemFn, attr: &Attribute) {
        let name = item_fn.sig.ident.to_string();
        let line = attr.span().start().line;
        self.out.verify_targets.push(VerifySite {
            fn_name: name,
            source_path: self.source_path.clone(),
            line,
        });
    }
}

impl<'ast, 'a> Visit<'ast> for FileVisitor<'a> {
    fn visit_item_fn(&mut self, item_fn: &'ast ItemFn) {
        let mut is_verify = false;
        for attr in &item_fn.attrs {
            if attr_path_is(attr, &["provekit", "contract"])
                || attr_path_is(attr, &["contract"])
            {
                self.record_contract(item_fn, attr);
            }
            if attr_path_is(attr, &["provekit", "verify"]) || attr_path_is(attr, &["verify"]) {
                self.record_verify(item_fn, attr);
                is_verify = true;
            }
        }
        if is_verify {
            let prev = std::mem::replace(
                &mut self.current_verify_fn,
                Some(item_fn.sig.ident.to_string()),
            );
            // Walk the body so we capture call sites + surrounding
            // equality checks.
            syn::visit::visit_item_fn(self, item_fn);
            self.current_verify_fn = prev;
        } else {
            // Still descend into nested fns; proc-macros routinely emit
            // wrappers, and `#[verify]` could nest inside an `impl`.
            syn::visit::visit_item_fn(self, item_fn);
        }
    }

    fn visit_local(&mut self, local: &'ast syn::Local) {
        // Capture `let x = callee(...);` so that downstream `if x == K`
        // checks can be tied to a call site's binding name.
        if let Some(init) = &local.init {
            if let Some(name) = pat_ident(&local.pat) {
                if let Some((callee, _line)) = call_callee(&init.expr) {
                    if self.current_verify_fn.is_some() {
                        // Stash the binding -> callee map by recording
                        // the CallSite immediately. We'll patch
                        // surrounding_eq_check when we encounter `if x
                        // == K`.
                        let line = init.expr.span().start().line;
                        self.out.callsites.push(CallSite {
                            verify_fn: self
                                .current_verify_fn
                                .clone()
                                .unwrap_or_default(),
                            callee,
                            source_path: self.source_path.clone(),
                            line,
                            surrounding_eq_check: None,
                        });
                        // Remember the binding ident for later eq
                        // patching.
                        self.current_let_eq_const.push((name, i64::MIN));
                        // sentinel; we replace when we see the if eq.
                    }
                }
            }
        }
        syn::visit::visit_local(self, local);
    }

    fn visit_expr_call(&mut self, call: &'ast ExprCall) {
        // Direct `f(...)` call NOT inside a `let`. We record it
        // unconditionally if we're inside a verify body.
        if self.current_verify_fn.is_some() {
            if let Some((callee, _l)) = call_callee_from_call(call) {
                let line = call.span().start().line;
                let already_recorded = self
                    .out
                    .callsites
                    .iter()
                    .any(|cs| cs.line == line && cs.callee == callee);
                if !already_recorded {
                    self.out.callsites.push(CallSite {
                        verify_fn: self
                            .current_verify_fn
                            .clone()
                            .unwrap_or_default(),
                        callee,
                        source_path: self.source_path.clone(),
                        line,
                        surrounding_eq_check: None,
                    });
                }
            }
        }
        syn::visit::visit_expr_call(self, call);
    }

    fn visit_expr_method_call(&mut self, mc: &'ast ExprMethodCall) {
        // Note method calls but skip them for contract-resolution
        // (contracts are on free fns by name). We still descend.
        syn::visit::visit_expr_method_call(self, mc);
    }

    fn visit_expr_if(&mut self, expr_if: &'ast ExprIf) {
        // Recognize `if <ident> == <int>` and patch the matching
        // CallSite's surrounding_eq_check, which is what makes the
        // demo's `deliberate_violation` flip from discharged to
        // counterexample.
        if let Expr::Binary(ExprBinary {
            op: BinOp::Eq(_), left, right, ..
        }) = &*expr_if.cond
        {
            let lname = expr_path_single_ident(left);
            let rint = expr_int_lit(right);
            let lint = expr_int_lit(left);
            let rname = expr_path_single_ident(right);
            let pair = match (lname, rint, rname, lint) {
                (Some(n), Some(k), _, _) => Some((n, k)),
                (_, _, Some(n), Some(k)) => Some((n, k)),
                _ => None,
            };
            if let Some((bound_name, k)) = pair {
                // Find the most recent matching let-binding.
                if let Some(slot) = self
                    .current_let_eq_const
                    .iter_mut()
                    .rev()
                    .find(|(n, _)| n == &bound_name)
                {
                    slot.1 = k;
                }
                // Patch the corresponding CallSite (last one in our
                // list whose verify_fn matches and whose callee was
                // bound to `bound_name`).
                if let Some(cs) = self.out.callsites.iter_mut().rev().find(|cs| {
                    cs.verify_fn == self.current_verify_fn.clone().unwrap_or_default()
                        && cs.surrounding_eq_check.is_none()
                }) {
                    cs.surrounding_eq_check = Some(k);
                }
            }
        }
        syn::visit::visit_expr_if(self, expr_if);
    }
}

// ---------------------------------------------------------------------------
// Attribute classification
// ---------------------------------------------------------------------------

/// Match `attr.path()` against a sequence of segment names.
fn attr_path_is(attr: &Attribute, segments: &[&str]) -> bool {
    let path = attr.path();
    if path.segments.len() != segments.len() {
        return false;
    }
    for (seg, want) in path.segments.iter().zip(segments.iter()) {
        if seg.ident != *want {
            return false;
        }
    }
    true
}

/// Walk the attribute body looking for `post = <expr>` and classify
/// the expression's shape. We do a textual match because the consumer
/// code expresses formulas via the kit's primitive helpers (`gte`,
/// `gt`, `eq`, `num`, `out`), which are NOT resolvable at build-script
/// time. Pattern matching on the token stream is sufficient for v0
/// and is documented as such.
fn classify_post(attr: &Attribute) -> FormulaShape {
    let tokens = match &attr.meta {
        Meta::List(list) => list.tokens.clone(),
        _ => return FormulaShape::Opaque,
    };
    let s = tokens.to_string();
    // Normalize whitespace.
    let s: String = s.split_whitespace().collect::<Vec<_>>().join(" ");
    // Look for `post = ...`. Subsequent close-paren / comma terminates.
    let post_pos = match s.find("post =") {
        Some(p) => p + "post =".len(),
        None => return FormulaShape::Opaque,
    };
    let body = &s[post_pos..];
    if let Some(n) = parse_simple_post(body, "gte") {
        FormulaShape::GteConst(n)
    } else if let Some(n) = parse_simple_post(body, "gt") {
        FormulaShape::GtConst(n)
    } else if let Some(n) = parse_simple_post(body, "eq") {
        FormulaShape::EqConst(n)
    } else {
        FormulaShape::Opaque
    }
}

/// Try to recognize `forall(<sort>(), |_| <pred>(out(), num(N)))` or
/// the bare `<pred>(out(), num(N))` shape. Returns `Some(N)` on match.
fn parse_simple_post(body: &str, pred: &str) -> Option<i64> {
    // Trim leading whitespace + up to the first occurrence of `pred(`.
    let idx = body.find(&format!("{pred} ("))
        .or_else(|| body.find(&format!("{pred}(")));
    let Some(start) = idx else { return None; };
    let rest = &body[start..];
    // Confirm shape: `<pred> ( out ( ) , num ( <int> ) )`. We look for
    // the literal substring `num (` or `num(` and then parse the int
    // inside.
    let num_anchor = rest.find("num (").or_else(|| rest.find("num("))?;
    let after = &rest[num_anchor..];
    let open = after.find('(')?;
    let close = after[open..].find(')')?;
    let inner = after[open + 1..open + close].trim();
    inner.parse::<i64>().ok()
}

// ---------------------------------------------------------------------------
// Helpers for the body-walk
// ---------------------------------------------------------------------------

fn pat_ident(pat: &syn::Pat) -> Option<String> {
    match pat {
        syn::Pat::Ident(i) => Some(i.ident.to_string()),
        _ => None,
    }
}

fn call_callee(expr: &Expr) -> Option<(String, usize)> {
    match expr {
        Expr::Call(c) => call_callee_from_call(c),
        _ => None,
    }
}

fn call_callee_from_call(call: &ExprCall) -> Option<(String, usize)> {
    if let Expr::Path(ExprPath { path, .. }) = &*call.func {
        let last = path.segments.last()?;
        let name = last.ident.to_string();
        let line = call.span().start().line;
        return Some((name, line));
    }
    None
}

fn expr_path_single_ident(expr: &Expr) -> Option<String> {
    if let Expr::Path(ExprPath { path, qself: None, .. }) = expr {
        if path.segments.len() == 1 && path.segments[0].arguments.is_empty() {
            return Some(path.segments[0].ident.to_string());
        }
    }
    None
}

fn expr_int_lit(expr: &Expr) -> Option<i64> {
    if let Expr::Lit(ExprLit {
        lit: Lit::Int(li), ..
    }) = expr
    {
        return li.base10_parse::<i64>().ok();
    }
    None
}

// Keep a token-stream import alive: tooling sometimes drops it
// otherwise. Used by the proc-macro2 visitor traversal.
const _PHANTOM: fn() -> Option<TokenStream2> = || None;
