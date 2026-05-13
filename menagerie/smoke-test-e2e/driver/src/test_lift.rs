// SPDX-License-Identifier: Apache-2.0
//
// Test-lift.
//
// For each `*.rs` file under tests/, walk `#[test]` fns and find
// `assert!(<expr>)` macro invocations whose argument references one of
// the fixture's public functions. For each match we record:
//
//   - the fn name being asserted on,
//   - the test's source location,
//   - the assert-arg as a pretty string (this is the witness formula).
//
// The smoke test does NOT translate Rust expressions to IrFormula here
// (that is provekit-lift-rust-tests' job). The pretty string is
// transported via the same single-atom encoding shim main.rs uses for
// the attribute lift, and the loss is loudly recorded in the
// discharge verdict.

use std::path::Path;

pub fn collect_witnesses(test_files: &[std::path::PathBuf]) -> Vec<(String, String, String)> {
    // Returns Vec<(source_location, fn_name, formula_text)>.
    let mut out = Vec::new();
    for path in test_files {
        let Ok(src) = std::fs::read_to_string(path) else {
            continue;
        };
        let Ok(file) = syn::parse_file(&src) else {
            continue;
        };
        let rel = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        for item in &file.items {
            if let syn::Item::Fn(item_fn) = item {
                let is_test = item_fn.attrs.iter().any(|a| a.path().is_ident("test"));
                if !is_test {
                    continue;
                }
                // First pass: collect a name -> fn-call-target map from `let <name> = <fn>(..)` bindings.
                let mut let_map = std::collections::BTreeMap::<String, String>::new();
                for stmt in &item_fn.block.stmts {
                    if let syn::Stmt::Local(local) = stmt {
                        let name = pat_to_name(&local.pat);
                        if let (Some(n), Some(init)) = (name, &local.init) {
                            if let Some(callee) = call_target_of(&init.expr) {
                                let_map.insert(n, callee);
                            }
                        }
                    }
                }
                // Second pass: for each assert, resolve its LHS identifier
                // through the let_map.
                for stmt in &item_fn.block.stmts {
                    if let syn::Stmt::Macro(m) = stmt {
                        if let Some(ident) = m.mac.path.get_ident() {
                            if ident == "assert" || ident == "assert_eq" || ident == "assert_ne" {
                                let body = m.mac.tokens.to_string();
                                // Find any identifier that appears before
                                // a relational op; if it is a key in
                                // let_map, use the mapped callee.
                                let lhs_ident = first_ident_before_relop(&body);
                                let target = lhs_ident
                                    .as_deref()
                                    .and_then(|s| let_map.get(s).cloned())
                                    .or_else(|| guess_fn_under_test(&body));
                                if let Some(fn_name) = target {
                                    // Normalize: replace the LHS local
                                    // identifier with the post-binding
                                    // marker `out` and reduce to the
                                    // bare predicate after the relation.
                                    let normalized = rewrite_assert_to_post(
                                        body.split(',').next().unwrap_or(&body).trim(),
                                    );
                                    out.push((format!("{}:{}", rel, ident), fn_name, normalized));
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    out
}

fn pat_to_name(pat: &syn::Pat) -> Option<String> {
    if let syn::Pat::Ident(p) = pat {
        Some(p.ident.to_string())
    } else {
        None
    }
}

fn call_target_of(expr: &syn::Expr) -> Option<String> {
    match expr {
        syn::Expr::Call(c) => path_tail(&c.func),
        syn::Expr::MethodCall(m) => Some(m.method.to_string()),
        _ => None,
    }
}

fn path_tail(expr: &syn::Expr) -> Option<String> {
    if let syn::Expr::Path(p) = expr {
        p.path.segments.last().map(|s| s.ident.to_string())
    } else {
        None
    }
}

fn first_ident_before_relop(s: &str) -> Option<String> {
    // Walk character by character; track the most recent identifier;
    // return it once a relational op is encountered.
    let bytes = s.as_bytes();
    let mut i = 0;
    let mut last_ident: Option<String> = None;
    let mut cur = String::new();
    while i < bytes.len() {
        let c = bytes[i] as char;
        if c.is_alphanumeric() || c == '_' {
            cur.push(c);
        } else {
            if !cur.is_empty() {
                last_ident = Some(std::mem::take(&mut cur));
            }
            // Look for relational op start.
            let two = if i + 1 < bytes.len() {
                std::str::from_utf8(&bytes[i..i + 2]).unwrap_or("")
            } else {
                ""
            };
            if matches!(two, ">=" | "<=" | "==" | "!=") {
                return last_ident;
            }
            if matches!(c, '>' | '<') {
                return last_ident;
            }
        }
        i += 1;
    }
    None
}

/// Heuristic: pick the first identifier the assert calls. We look for
/// `<ident>(` and stop at the first match. For the smoke-test fixture
/// this is `commit_balance_change`. A real implementation walks the
/// AST; the heuristic is good enough for the fixture's one assertion.
fn guess_fn_under_test(body: &str) -> Option<String> {
    let mut cur = String::new();
    for c in body.chars() {
        if c.is_alphanumeric() || c == '_' {
            cur.push(c);
        } else if c == '('
            && !cur.is_empty()
            && cur
                .chars()
                .next()
                .map(|x| x.is_alphabetic())
                .unwrap_or(false)
        {
            if !is_reserved(&cur) {
                return Some(cur);
            }
            cur.clear();
        } else {
            cur.clear();
        }
    }
    None
}

fn is_reserved(s: &str) -> bool {
    matches!(
        s,
        "assert" | "let" | "if" | "for" | "while" | "return" | "match"
    )
}

/// Look up a post-condition derived from a unit-test assertion that
/// exercises `fn_name`. Returns the asserted-condition text as a
/// pretty string. In the fixture, there is exactly one such
/// assertion: `assert!(result >= 0)` after `commit_balance_change`.
///
/// The smoke test reads only that one for the propagation event.
pub fn lift_assertion_for_fn(test_files: &[std::path::PathBuf], fn_name: &str) -> Option<String> {
    let witnesses = collect_witnesses(test_files);
    for (_, target, formula) in &witnesses {
        if target == fn_name {
            // Use only the boolean predicate portion. Strip a leading
            // `result` / `out` placeholder and use the inequality body.
            let body = formula.trim();
            let post_only = body.split(';').next().unwrap_or(body).trim();
            return Some(rewrite_assert_to_post(post_only));
        }
    }
    let _ = Path::new(""); // keep `Path` import alive if compiler complains
    None
}

fn rewrite_assert_to_post(s: &str) -> String {
    // Convert `result >= 0` (with whatever LHS binder name) into a
    // post-condition `out >= 0`. We match on the rightmost relation.
    for op in [">=", "<=", "==", "!=", ">", "<"].iter() {
        if let Some(pos) = s.find(op) {
            let (_, rhs) = s.split_at(pos);
            // rhs starts with the op. Build "out <op><rhs after op>".
            return format!("out {}", rhs);
        }
    }
    s.to_string()
}
