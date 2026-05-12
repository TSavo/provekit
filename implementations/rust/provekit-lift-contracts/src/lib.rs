// SPDX-License-Identifier: Apache-2.0
//
// provekit-lift-contracts
//
// Walks the syn AST of a Rust source file looking for functions with
// `#[contracts::requires(...)]` and `#[contracts::ensures(...)]`
// attributes (the `contracts` crate). Translates each predicate to
// canonical IR and emits one ContractDecl per function.
//
// Recognized attribute paths:
//   #[requires(<expr>)]
//   #[ensures(<expr>)]
//   #[invariant(<expr>)]
//   #[contracts::requires(<expr>)]
//   #[contracts::ensures(<expr>)]
//   #[contracts::invariant(<expr>)]
//
// LIFTABLE PREDICATE SHAPE: same v0 whitelist as proptest:
//   <var|lit|single-arg-call> <binop> <var|lit|single-arg-call>
// where binop is one of >, >=, <, <=, ==, !=.
//
// The function's parameters define the universally-quantified
// variables. `ret` (when used in #[ensures]) maps to the contract's
// outBinding (default "out").
//
// NAMING ROUND-TRIP
// -----------------
// If the caller passes the raw source text via `lift_file_with_source`,
// the lifter also scans lines immediately preceding each function for
// a `// concept: <name>` annotation (or the `/// concept: <name>` doc
// comment form) and attaches it to `ContractDecl::concept_hint`.
//
// Canonical annotation format (emitted by the substrate rewriter):
//   // concept: retry-with-jitter
// Doc-comment form (also accepted, idiomatic when users want IDE hover):
//   /// concept: retry-with-jitter
//
// Placeholder names (`UNNAMED-CONCEPT-N`) are stored as-is; the
// downstream binding step distinguishes them from human names.
//
// `concept_hint` is METADATA ONLY — it does NOT participate in
// `canonical_bytes` / CID derivation.  Changing or removing the
// annotation never rewrites the shape identity.

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

/// Lift all contract attributes found in `file`.
///
/// Equivalent to `lift_file_with_source(file, source_path, None)`.
pub fn lift_file(file: &syn::File, source_path: &str) -> AdapterOutput {
    lift_file_with_source(file, source_path, None)
}

/// Lift all contract attributes found in `file`, optionally scanning
/// `source_text` for `// concept: <name>` annotations that precede each
/// function.  When `source_text` is `None`, `concept_hint` is always `None`.
pub fn lift_file_with_source(
    file: &syn::File,
    source_path: &str,
    source_text: Option<&str>,
) -> AdapterOutput {
    let source_lines: Option<Vec<&str>> = source_text.map(|s| s.lines().collect());
    let mut out = AdapterOutput::default();
    walk_items(&file.items, source_path, source_lines.as_deref(), &mut out);
    out
}

fn walk_items(
    items: &[syn::Item],
    source_path: &str,
    source_lines: Option<&[&str]>,
    out: &mut AdapterOutput,
) {
    for item in items {
        match item {
            syn::Item::Fn(f) => visit_fn(f, source_path, source_lines, out),
            syn::Item::Mod(m) => {
                if let Some((_, items)) = &m.content {
                    walk_items(items, source_path, source_lines, out);
                }
            }
            syn::Item::Impl(i) => {
                for it in &i.items {
                    if let syn::ImplItem::Fn(f) = it {
                        visit_impl_fn(f, source_path, source_lines, out);
                    }
                }
            }
            _ => {}
        }
    }
}

fn visit_fn(
    f: &syn::ItemFn,
    source_path: &str,
    source_lines: Option<&[&str]>,
    out: &mut AdapterOutput,
) {
    let attrs = &f.attrs;
    let name = f.sig.ident.to_string();
    if !any_contract_attr(attrs) {
        return;
    }
    out.seen += 1;
    // syn span lines are 1-based; subtract 1 for 0-based slice index.
    let fn_line = f.sig.ident.span().start().line;
    let concept_hint = concept_hint_from_span(fn_line, attrs, source_lines);
    process(name, attrs, &f.sig, source_path, concept_hint, out);
}

fn visit_impl_fn(
    f: &syn::ImplItemFn,
    source_path: &str,
    source_lines: Option<&[&str]>,
    out: &mut AdapterOutput,
) {
    let attrs = &f.attrs;
    let name = f.sig.ident.to_string();
    if !any_contract_attr(attrs) {
        return;
    }
    out.seen += 1;
    let fn_line = f.sig.ident.span().start().line;
    let concept_hint = concept_hint_from_span(fn_line, attrs, source_lines);
    process(name, attrs, &f.sig, source_path, concept_hint, out);
}

fn any_contract_attr(attrs: &[syn::Attribute]) -> bool {
    attrs.iter().any(|a| classify_attr(a).is_some())
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
        "requires" | "contracts::requires" => Some(Slot::Pre),
        "ensures" | "contracts::ensures" => Some(Slot::Post),
        "invariant" | "contracts::invariant" => Some(Slot::Inv),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Concept-hint extraction (naming round-trip, lifter side)
// ---------------------------------------------------------------------------

/// Regex pattern for a concept annotation.  Matches:
///   `// concept: <name>`   -- regular comment (via raw source scan)
///   `/// concept: <name>`  -- doc comment (via #[doc = "..."] attribute)
///
/// Name grammar: starts with `[a-zA-Z]`, then `[a-zA-Z0-9\-:_]*`.
/// Surrounding whitespace is trimmed.  Names containing spaces or other
/// characters are rejected (returns None) rather than propagating garbage.
const CONCEPT_ANNOTATION_PREFIX: &str = "concept:";

/// Validate that `name` matches `[a-zA-Z][a-zA-Z0-9\-:_]*`.
fn is_valid_concept_name(name: &str) -> bool {
    let mut chars = name.chars();
    match chars.next() {
        None => false,
        Some(c) if !c.is_ascii_alphabetic() => false,
        _ => chars.all(|c| c.is_ascii_alphanumeric() || c == '-' || c == ':' || c == '_'),
    }
}

/// Parse a single text line (stripped of its `//` or `///` prefix and
/// surrounding whitespace) as a concept annotation.  Returns the concept
/// name string if the line matches `concept: <valid-name>`, otherwise None.
fn parse_concept_annotation(line: &str) -> Option<String> {
    let trimmed = line.trim();
    let rest = trimmed.strip_prefix(CONCEPT_ANNOTATION_PREFIX)?;
    let name = rest.trim();
    if is_valid_concept_name(name) {
        Some(name.to_string())
    } else {
        None
    }
}

/// Extract a concept hint for the function whose first token is at
/// `fn_line` (1-based, matching `proc_macro2::Span::start().line`).
///
/// Search order (first match wins):
///
/// 1. Doc attributes (`#[doc = "..."]`) on the function itself, scanned
///    in reverse until a non-doc attribute or the start is reached.
///    This catches `/// concept: <name>` written directly above the fn.
///
/// 2. Raw source lines immediately preceding the function (requires
///    `source_lines` to be Some).  Scans backwards from `fn_line - 1`,
///    skipping blank lines, until a `// concept:` line or a non-comment
///    line is encountered.
///
/// Returns `None` if neither source is available or no matching annotation
/// is found.
fn concept_hint_from_span(
    fn_line: usize,
    attrs: &[syn::Attribute],
    source_lines: Option<&[&str]>,
) -> Option<String> {
    // --- Path 1: doc attributes (/// concept: <name>) ---
    // Scan ALL attrs in reverse; skip non-doc attrs, return first matching doc attr.
    for attr in attrs.iter().rev() {
        if !attr.path().is_ident("doc") {
            continue;
        }
        // Extract the string value of the #[doc = "..."] attribute.
        if let syn::Meta::NameValue(nv) = &attr.meta {
            if let syn::Expr::Lit(el) = &nv.value {
                if let syn::Lit::Str(ls) = &el.lit {
                    let text = ls.value();
                    // `///` comments come through as `" concept: foo"` (leading space).
                    if let Some(hint) = parse_concept_annotation(&text) {
                        return Some(hint);
                    }
                }
            }
        }
    }

    // --- Path 2: raw source lines (// concept: <name>) ---
    let lines = source_lines?;
    // fn_line is 1-based; the line AT that index in a 0-based slice is
    // lines[fn_line - 1].  We want the lines BEFORE it.
    if fn_line == 0 {
        return None;
    }
    // Walk backwards from the line immediately before the fn definition.
    let mut idx = fn_line.saturating_sub(2); // 0-based index of line fn_line-1
    loop {
        let raw = if idx < lines.len() { lines[idx] } else { break };
        let trimmed = raw.trim();

        if trimmed.is_empty() {
            // Skip blank lines between comment and fn keyword.
            if idx == 0 {
                break;
            }
            idx -= 1;
            continue;
        }

        // Skip attribute lines (#[...]) — they appear between the
        // concept annotation and the `fn` keyword and must be stepped over.
        if trimmed.starts_with("#[") || trimmed.starts_with("#![") {
            if idx == 0 {
                break;
            }
            idx -= 1;
            continue;
        }

        // Accept both `// concept:` and `/// concept:`.
        let rest = if let Some(r) = trimmed.strip_prefix("///") {
            r
        } else if let Some(r) = trimmed.strip_prefix("//") {
            r
        } else {
            // Not a comment line; stop scanning.
            break;
        };

        if let Some(hint) = parse_concept_annotation(rest) {
            return Some(hint);
        }

        // It's a comment but not a concept annotation; keep scanning
        // upward in case the annotation is on an earlier line.
        if idx == 0 {
            break;
        }
        idx -= 1;
    }

    None
}

fn process(
    name: String,
    attrs: &[syn::Attribute],
    sig: &syn::Signature,
    source_path: &str,
    concept_hint: Option<String>,
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
                Slot::Post => post_atoms.push(f),
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
                reason: "no liftable contracts attrs".into(),
            });
        }
        return;
    }

    let pre = combine(pre_atoms);
    let post = combine(post_atoms);
    let inv = combine(inv_atoms);

    // For each non-empty slot, wrap in forall over the function params.
    let pre = pre.map(|f| wrap_forall(&params, 0, f));
    let post = post.map(|f| wrap_forall(&params, 0, f));
    let inv = inv.map(|f| wrap_forall(&params, 0, f));

    out.decls.push(ContractDecl {
        name,
        pre,
        post,
        inv,
        out_binding: "out".into(),
        evidence: None,
        concept_hint,
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
        _ => f.clone(), // Choice: TODO: implement
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
        _ => t.clone(), // Lambda, Let: TODO: implement
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
                _ => Err(format!("unsupported binop: {:?}", b.op)),
            }
        }
        syn::Expr::Paren(p) => translate_bool_expr(&p.expr),
        _ => Err("contract expression must be a comparison".into()),
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
    fn lifts_requires_and_ensures() {
        let src = r#"
            #[requires(x > 0)]
            #[ensures(ret >= 0)]
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
    fn lifts_namespaced_contracts_attr() {
        let src = r#"
            #[contracts::requires(x > 0)]
            fn f(x: i64) -> i64 { x }
        "#;
        let f = parse(src);
        let out = lift_file(&f, "test.rs");
        assert_eq!(out.lifted, 1);
    }

    #[test]
    fn skips_method_call_with_warning() {
        let src = r#"
            #[requires(s.len() > 0)]
            fn f(s: String) { }
        "#;
        let f = parse(src);
        let out = lift_file(&f, "test.rs");
        assert_eq!(out.lifted, 0);
        assert!(!out.warnings.is_empty());
    }

    // ------------------------------------------------------------------
    // Naming round-trip: concept_hint extraction
    // ------------------------------------------------------------------

    /// Human-supplied name is extracted from `// concept: retry-with-jitter`
    /// immediately preceding the function.
    #[test]
    fn concept_hint_human_name_extracted() {
        let src = "// concept: retry-with-jitter\n#[requires(x > 0)]\nfn retry(x: i64) -> i64 { x }\n";
        let f = parse(src);
        let out = lift_file_with_source(&f, "test.rs", Some(src));
        assert_eq!(out.lifted, 1, "warnings: {:?}", out.warnings);
        assert_eq!(
            out.decls[0].concept_hint.as_deref(),
            Some("retry-with-jitter"),
            "expected human concept name"
        );
    }

    /// Placeholder `UNNAMED-CONCEPT-N` is extracted verbatim — the downstream
    /// binding step distinguishes it from a human name by the prefix.
    #[test]
    fn concept_hint_unnamed_placeholder_extracted() {
        let src = "// concept: UNNAMED-CONCEPT-3\n#[requires(x > 0)]\nfn retry(x: i64) -> i64 { x }\n";
        let f = parse(src);
        let out = lift_file_with_source(&f, "test.rs", Some(src));
        assert_eq!(out.lifted, 1);
        assert_eq!(
            out.decls[0].concept_hint.as_deref(),
            Some("UNNAMED-CONCEPT-3"),
            "expected UNNAMED placeholder to be preserved verbatim"
        );
    }

    /// When no concept annotation is present, `concept_hint` is `None`.
    #[test]
    fn concept_hint_absent_returns_none() {
        let src = "// some other comment\n#[requires(x > 0)]\nfn f(x: i64) -> i64 { x }\n";
        let f = parse(src);
        let out = lift_file_with_source(&f, "test.rs", Some(src));
        assert_eq!(out.lifted, 1);
        assert_eq!(
            out.decls[0].concept_hint,
            None,
            "non-concept comment must not produce a hint"
        );
    }

    /// A malformed annotation (`concept: foo bar` — space in name) is
    /// ignored; `concept_hint` stays `None`.
    #[test]
    fn concept_hint_malformed_name_rejected() {
        let src = "// concept: foo bar\n#[requires(x > 0)]\nfn f(x: i64) -> i64 { x }\n";
        let f = parse(src);
        let out = lift_file_with_source(&f, "test.rs", Some(src));
        assert_eq!(out.lifted, 1);
        assert_eq!(
            out.decls[0].concept_hint,
            None,
            "malformed name (space) must be rejected"
        );
    }

    /// Doc comment (`/// concept: <name>`) is also accepted, via the
    /// `#[doc = "..."]` attribute path.
    #[test]
    fn concept_hint_doc_comment_form_accepted() {
        let src = r#"
            /// concept: retry-with-jitter
            #[requires(x > 0)]
            fn retry(x: i64) -> i64 { x }
        "#;
        let f = parse(src);
        // source_text not needed for doc-comment path — attrs carry the value.
        let out = lift_file_with_source(&f, "test.rs", Some(src));
        assert_eq!(out.lifted, 1, "warnings: {:?}", out.warnings);
        assert_eq!(
            out.decls[0].concept_hint.as_deref(),
            Some("retry-with-jitter"),
            "doc-comment concept annotation must be extracted"
        );
    }

    /// Regression: idiomatic Rust ordering is `[doc, requires]`; reverse iter
    /// sees `[requires, doc]`.  The old `break` on `requires` caused the doc
    /// attr to be silently skipped.  The fix changes `break` -> `continue` so
    /// all attrs are scanned and the doc attr is found.
    ///
    /// This test MUST fail against pre-fix HEAD and pass after the fix.
    #[test]
    fn concept_hint_doc_above_requires_extracts_correctly() {
        let src = r#"
            /// concept: retry-with-jitter
            #[requires(x > 0)]
            fn retry(x: i32) -> i32 { x }
        "#;
        let f = parse(src);
        // lift_file (no source_text) — mirrors the production caller in lift_pass.rs.
        let out = lift_file(&f, "test.rs");
        assert_eq!(out.lifted, 1, "warnings: {:?}", out.warnings);
        assert_eq!(
            out.decls[0].concept_hint.as_deref(),
            Some("retry-with-jitter"),
            "doc attr above #[requires] must be found despite non-doc attr in reverse iter"
        );
    }
}
