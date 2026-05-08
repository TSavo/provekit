// SPDX-License-Identifier: Apache-2.0
//
// provekit-lift-rust-tests / layer 2
//
// Layer 0 (lib.rs above) handles MECHANICAL pattern recognition: a
// `#[test]` body that is a sequence of assert macros, each side a Var,
// literal, or single-arg ctor call. Anything else skips.
//
// Layer 2 sits ABOVE Layer 0 and below the (future) Layer 3 LLM lift.
// It recognizes three structural patterns Layer 0 cannot, then delegates
// the leaf assertion translation back to Layer 0's `lift_assertion_macro`.
//
// PATTERNS (v0 whitelist):
//
//   Pattern 1 , bounded loop as universal quantifier:
//       fn t() {
//           for x in 0..N {           // Range / RangeInclusive only
//               assert!(<binop>);     // single-stmt body
//           }
//       }
//   Lift to: forall x:Int. (lo<=x AND x<hi) implies (assertion).
//   The bound endpoints must be integer literals or unary-neg literals.
//   RangeFrom (`..`), RangeFull, and ranges over collections SKIP.
//   Multi-statement loop bodies SKIP (side effects).
//   Nested loops SKIP (defer to Layer 2.5).
//
//   Pattern 2 , inlined helper functions:
//       fn assert_palindrome(s: &str) {
//           assert_eq!(s, ...);       // single liftable assertion
//       }
//       #[test]
//       fn t() {
//           assert_palindrome("racecar");
//           assert_palindrome("level");
//       }
//   Lift to: ONE memento per call site, body = the helper's lifted
//   assertion with the formal parameter substituted by the literal
//   argument at the call. Helper is defined IN THE SAME `syn::File`.
//   Helpers with multiple statements, multiple params, or non-liftable
//   bodies SKIP.
//
//   Pattern 3 , multi-assertion characterization conjunction:
//       #[test]
//       fn t() {
//           assert_<...>;             // each independently liftable
//           assert_<...>;
//           ...
//       }
//   Lift to: ONE memento, body = and_(...) of all liftable atoms.
//   When ANY top-level statement is not a liftable assert (let, side-
//   effecting call, branching), the whole pattern SKIPS , Layer 0 will
//   then see each assert independently. We deliberately require every
//   statement to be a recognized assertion to keep "characterization"
//   honest: the test body characterizes ONE thing if and only if every
//   stmt in it is a witness about that one thing.
//
// CLAIM SET:
//   `lift_file_layer2` returns a `claimed_tests` set: each test name
//   that Layer 2 took ownership of. The dispatcher in `provekit-lift`
//   passes that set to `lift_file_with_skip` so Layer 0 ignores those
//   functions. This avoids double-minting when Layer 0 partially lifts
//   a Pattern-3 test.
//
// CONTENT-ADDRESSED DETERMINISM:
//   - Pattern 1 wraps with `Formula::Quantifier { name: <loop var ident> }`
//     (NOT the kit's `_xN` placeholder) so the canonical IR is stable
//     across runs.
//   - Pattern 2 contract names: "<test>::call::<i>" zero-indexed in
//     source order. Same call expression in two tests dedups at mint.
//   - Pattern 3 contract name: just "<test>" (no ::N suffix). Marks the
//     test as conjunctively characterized.

use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::rc::Rc;

use provekit_ir_symbolic::{
    and_, atomic_, connective_, gte, lt, lte, make_var, num, ContractDecl, Formula, Int, Term,
};

use crate::{
    is_assertion_macro_pub, lift_assertion_macro_pub, path_to_string_pub, translate_term_pub,
    LiftWarning,
};

/// Output of a Layer 2 pass over a `syn::File`. Carries the same
/// shape as Layer 0's `AdapterOutput` so the dispatcher can fold it
/// uniformly, plus a set of test-fn names Layer 0 should skip.
#[derive(Debug, Default)]
pub struct Layer2Output {
    pub decls: Vec<ContractDecl>,
    pub warnings: Vec<LiftWarning>,
    pub seen: usize,
    pub lifted: usize,
    /// Test-fn names this pass took ownership of. Layer 0 must not also
    /// emit decls for these, regardless of whether the leaf pattern was
    /// liftable.
    pub claimed_tests: BTreeSet<String>,
    /// Counts split by pattern. Useful for the CLI summary.
    pub bounded_loop_lifted: usize,
    pub bounded_loop_skipped: usize,
    pub helper_inlined_lifted: usize,
    pub helper_inlined_skipped: usize,
    pub characterization_lifted: usize,
    pub characterization_skipped: usize,
}

pub fn lift_file_layer2(file: &syn::File, source_path: &str) -> Layer2Output {
    let mut out = Layer2Output::default();

    // First pass: gather helper functions for Pattern 2 inlining.
    let helpers = collect_helpers(&file.items);

    // Second pass: classify each #[test] fn and dispatch to the
    // matching pattern. Each test is claimed by AT MOST one pattern.
    walk_items(&file.items, source_path, &helpers, &mut out);

    out
}

fn walk_items(
    items: &[syn::Item],
    source_path: &str,
    helpers: &BTreeMap<String, HelperDef>,
    out: &mut Layer2Output,
) {
    for item in items {
        match item {
            syn::Item::Fn(f) => {
                if has_test_attr(&f.attrs) {
                    classify_and_lift(f, source_path, helpers, out);
                }
                walk_block_for_items(&f.block, source_path, helpers, out);
            }
            syn::Item::Mod(m) => {
                if let Some((_, items)) = &m.content {
                    walk_items(items, source_path, helpers, out);
                }
            }
            _ => {}
        }
    }
}

fn walk_block_for_items(
    block: &syn::Block,
    source_path: &str,
    helpers: &BTreeMap<String, HelperDef>,
    out: &mut Layer2Output,
) {
    for stmt in &block.stmts {
        if let syn::Stmt::Item(item) = stmt {
            walk_items(std::slice::from_ref(item), source_path, helpers, out);
        }
    }
}

fn has_test_attr(attrs: &[syn::Attribute]) -> bool {
    attrs.iter().any(|a| {
        let p = path_to_string_pub(a.path());
        p == "test" || p.ends_with("::test")
    })
}

/// A helper function eligible for Pattern 2 inlining. Must be a non-test
/// function with a single typed parameter and a body that is exactly one
/// liftable assertion macro.
#[derive(Debug, Clone)]
struct HelperDef {
    /// The single parameter name (formal).
    param_name: String,
    /// The leaf assertion macro AST node. We re-translate at each call
    /// site after substituting the actual argument for the formal.
    assertion: syn::Macro,
}

fn collect_helpers(items: &[syn::Item]) -> BTreeMap<String, HelperDef> {
    let mut map = BTreeMap::new();
    for item in items {
        match item {
            syn::Item::Fn(f) => {
                if has_test_attr(&f.attrs) {
                    continue;
                }
                if let Some(h) = helper_def_from_fn(f) {
                    map.insert(f.sig.ident.to_string(), h);
                }
            }
            syn::Item::Mod(m) => {
                if let Some((_, items)) = &m.content {
                    let inner = collect_helpers(items);
                    for (k, v) in inner {
                        map.entry(k).or_insert(v);
                    }
                }
            }
            _ => {}
        }
    }
    map
}

fn helper_def_from_fn(f: &syn::ItemFn) -> Option<HelperDef> {
    // Single typed parameter.
    if f.sig.inputs.len() != 1 {
        return None;
    }
    let arg = f.sig.inputs.first()?;
    let pname = match arg {
        syn::FnArg::Typed(pt) => match &*pt.pat {
            syn::Pat::Ident(pi) => pi.ident.to_string(),
            _ => return None,
        },
        syn::FnArg::Receiver(_) => return None,
    };
    // Body has exactly one assertion-macro statement.
    let stmts = &f.block.stmts;
    if stmts.len() != 1 {
        return None;
    }
    let mac = match &stmts[0] {
        syn::Stmt::Macro(sm) => sm.mac.clone(),
        syn::Stmt::Expr(syn::Expr::Macro(em), _) => em.mac.clone(),
        _ => return None,
    };
    if !is_assertion_macro_pub(&mac) {
        return None;
    }
    Some(HelperDef {
        param_name: pname,
        assertion: mac,
    })
}

/// Decide which Pattern (if any) owns this test fn, and lift accordingly.
/// At most one pattern claims a given test. Layer 0 will then skip the
/// claimed names. If none of the three patterns claims it, the test is
/// left for Layer 0 (no warning here , Layer 0 is the right reporter).
fn classify_and_lift(
    f: &syn::ItemFn,
    source_path: &str,
    helpers: &BTreeMap<String, HelperDef>,
    out: &mut Layer2Output,
) {
    let test_name = f.sig.ident.to_string();
    let stmts = &f.block.stmts;

    // PATTERN 1: single bounded `for` loop with a single-stmt body.
    if stmts.len() == 1 {
        if let syn::Stmt::Expr(syn::Expr::ForLoop(fl), _) = &stmts[0] {
            return classify_for_loop(fl, &test_name, source_path, out);
        }
    }

    // PATTERN 2: every top-level stmt is a single-arg call to a
    // recognized helper function. At least one such call.
    let calls = match collect_helper_calls(stmts, helpers) {
        Some(v) if !v.is_empty() => Some(v),
        _ => None,
    };
    if let Some(calls) = calls {
        return classify_helper_inlining(&calls, helpers, &test_name, source_path, out);
    }

    // PATTERN 3: every top-level stmt is an assertion macro AND there
    // are >=2 of them. (>=2 is what makes it a "conjunction"; a single
    // assert is Layer 0's job.)
    let mut macs: Vec<syn::Macro> = Vec::new();
    let mut all_macros = true;
    for stmt in stmts {
        let mac = match stmt {
            syn::Stmt::Macro(sm) => Some(sm.mac.clone()),
            syn::Stmt::Expr(syn::Expr::Macro(em), _) => Some(em.mac.clone()),
            _ => None,
        };
        match mac {
            Some(m) if is_assertion_macro_pub(&m) => macs.push(m),
            _ => {
                all_macros = false;
                break;
            }
        }
    }
    if all_macros && macs.len() >= 2 {
        return classify_characterization(&macs, &test_name, source_path, out);
    }

    // Not a Layer 2 pattern. Leave it for Layer 0.
}

// ---------------------------------------------------------------------------
// PATTERN 1
// ---------------------------------------------------------------------------

fn classify_for_loop(
    fl: &syn::ExprForLoop,
    test_name: &str,
    source_path: &str,
    out: &mut Layer2Output,
) {
    // Claim the test regardless of liftability so Layer 0 doesn't also
    // try to mine it (the body is a single for-loop, not asserts).
    out.claimed_tests.insert(test_name.to_string());
    out.seen += 1;

    // Loop variable must be a simple ident pattern.
    let var_name = match &*fl.pat {
        syn::Pat::Ident(pi) => pi.ident.to_string(),
        _ => {
            out.bounded_loop_skipped += 1;
            out.warnings.push(LiftWarning {
                source_path: source_path.into(),
                item_name: test_name.into(),
                reason: "layer2 bounded-loop: loop pattern is not a simple identifier".into(),
            });
            return;
        }
    };

    // Body must be a single statement, no nested for-loops.
    let body = &fl.body;
    if body.stmts.len() != 1 {
        out.bounded_loop_skipped += 1;
        out.warnings.push(LiftWarning {
            source_path: source_path.into(),
            item_name: test_name.into(),
            reason: format!(
                "layer2 bounded-loop: body has {} stmts (only single-stmt bodies in v0)",
                body.stmts.len()
            ),
        });
        return;
    }

    if has_nested_for_loop(&body.stmts[0]) {
        out.bounded_loop_skipped += 1;
        out.warnings.push(LiftWarning {
            source_path: source_path.into(),
            item_name: test_name.into(),
            reason: "layer2 bounded-loop: nested for-loop detected; deferred to Layer 2.5".into(),
        });
        return;
    }

    // Body must be a single assertion macro.
    let mac = match &body.stmts[0] {
        syn::Stmt::Macro(sm) => Some(sm.mac.clone()),
        syn::Stmt::Expr(syn::Expr::Macro(em), _) => Some(em.mac.clone()),
        _ => None,
    };
    let Some(mac) = mac else {
        out.bounded_loop_skipped += 1;
        out.warnings.push(LiftWarning {
            source_path: source_path.into(),
            item_name: test_name.into(),
            reason: "layer2 bounded-loop: body stmt is not an assertion macro".into(),
        });
        return;
    };
    if !is_assertion_macro_pub(&mac) {
        out.bounded_loop_skipped += 1;
        out.warnings.push(LiftWarning {
            source_path: source_path.into(),
            item_name: test_name.into(),
            reason: "layer2 bounded-loop: body macro is not in the assertion whitelist".into(),
        });
        return;
    }

    // Iterator must be a literal-bounded numeric range.
    let (lo_term, hi_term, inclusive) = match parse_numeric_range(&fl.expr) {
        Some(t) => t,
        None => {
            out.bounded_loop_skipped += 1;
            out.warnings.push(LiftWarning {
                source_path: source_path.into(),
                item_name: test_name.into(),
                reason: "layer2 bounded-loop: iterator is not a literal-bounded numeric range \
                         (got something other than `lo..hi` / `lo..=hi`)"
                    .into(),
            });
            return;
        }
    };

    let inner_formula = match lift_assertion_macro_pub(&mac) {
        Ok(f) => f,
        Err(e) => {
            out.bounded_loop_skipped += 1;
            out.warnings.push(LiftWarning {
                source_path: source_path.into(),
                item_name: test_name.into(),
                reason: format!("layer2 bounded-loop: inner assertion not liftable: {e}"),
            });
            return;
        }
    };

    // Build forall x:Int. (lo <= x AND x </<= hi) -> inner.
    let var_term = make_var(var_name.clone());
    let lower = gte(var_term.clone(), lo_term);
    let upper = if inclusive {
        lte(var_term.clone(), hi_term)
    } else {
        lt(var_term.clone(), hi_term)
    };
    let antecedent = and_(vec![lower, upper]);
    let body_formula = connective_("implies", vec![antecedent, inner_formula]);

    let quantified = Rc::new(Formula::Quantifier {
        kind: "forall".into(),
        name: var_name,
        sort: Int(),
        body: body_formula,
    });

    out.decls.push(ContractDecl {
        name: test_name.to_string(),
        pre: None,
        post: None,
        inv: Some(quantified),
        out_binding: "out".into(),
        evidence: None,
    });
    out.lifted += 1;
    out.bounded_loop_lifted += 1;
}

fn has_nested_for_loop(stmt: &syn::Stmt) -> bool {
    struct Find {
        found: bool,
    }
    impl<'a> syn::visit::Visit<'a> for Find {
        fn visit_expr_for_loop(&mut self, _i: &'a syn::ExprForLoop) {
            self.found = true;
        }
    }
    let mut f = Find { found: false };
    use syn::visit::Visit;
    f.visit_stmt(stmt);
    f.found
}

/// Parse `lo..hi` / `lo..=hi` / `..hi` / `lo..` / `..=hi` etc. Returns
/// `Some((lo, hi, inclusive))` only for fully-bounded literal numeric
/// ranges (`lo..hi` and `lo..=hi`). RangeFrom / RangeTo / RangeFull
/// return None: those would lift to a vacuous or unsound forall.
fn parse_numeric_range(expr: &syn::Expr) -> Option<(Rc<Term>, Rc<Term>, bool)> {
    if let syn::Expr::Range(r) = expr {
        let lo = r.start.as_ref()?;
        let hi = r.end.as_ref()?;
        let lo_t = literal_int_or_var(lo)?;
        let hi_t = literal_int_or_var(hi)?;
        let inclusive = matches!(r.limits, syn::RangeLimits::Closed(_));
        Some((lo_t, hi_t, inclusive))
    } else if let syn::Expr::Paren(p) = expr {
        parse_numeric_range(&p.expr)
    } else {
        None
    }
}

/// Permit a literal integer (possibly negated) or a bare identifier as
/// a range endpoint. The latter accommodates `for x in 0..N` where N is
/// a const elsewhere , we lift it as a free Var, the verifier can then
/// prove or disprove the universal under whatever N binding is in scope.
fn literal_int_or_var(expr: &syn::Expr) -> Option<Rc<Term>> {
    match expr {
        syn::Expr::Lit(l) => match &l.lit {
            syn::Lit::Int(li) => {
                let n: i64 = li.base10_parse().ok()?;
                Some(num(n))
            }
            _ => None,
        },
        syn::Expr::Unary(u) => {
            if matches!(u.op, syn::UnOp::Neg(_)) {
                if let syn::Expr::Lit(l) = &*u.expr {
                    if let syn::Lit::Int(li) = &l.lit {
                        let n: i64 = li.base10_parse().ok()?;
                        return Some(num(-n));
                    }
                }
            }
            None
        }
        syn::Expr::Path(p) => p.path.get_ident().map(|id| make_var(id.to_string())),
        syn::Expr::Paren(p) => literal_int_or_var(&p.expr),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// PATTERN 2
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct HelperCall {
    helper_name: String,
    /// The single argument expression at the call site.
    arg: syn::Expr,
}

/// If every top-level statement is a call to a known helper, return the
/// list of (helper, arg). Otherwise return None.
fn collect_helper_calls(
    stmts: &[syn::Stmt],
    helpers: &BTreeMap<String, HelperDef>,
) -> Option<Vec<HelperCall>> {
    let mut calls = Vec::new();
    for stmt in stmts {
        let expr = match stmt {
            syn::Stmt::Expr(e, _) => e,
            syn::Stmt::Macro(_) => return None,
            _ => return None,
        };
        let call = match expr {
            syn::Expr::Call(c) => c,
            _ => return None,
        };
        let callee = match &*call.func {
            syn::Expr::Path(p) => match p.path.get_ident() {
                Some(id) => id.to_string(),
                None => return None,
            },
            _ => return None,
        };
        if !helpers.contains_key(&callee) {
            return None;
        }
        if call.args.len() != 1 {
            return None;
        }
        let arg = call.args.first().unwrap().clone();
        calls.push(HelperCall {
            helper_name: callee,
            arg,
        });
    }
    Some(calls)
}

fn classify_helper_inlining(
    calls: &[HelperCall],
    helpers: &BTreeMap<String, HelperDef>,
    test_name: &str,
    source_path: &str,
    out: &mut Layer2Output,
) {
    out.claimed_tests.insert(test_name.to_string());

    for (i, call) in calls.iter().enumerate() {
        out.seen += 1;
        let memento_name = format!("{test_name}::call::{i}");
        let helper = helpers
            .get(&call.helper_name)
            .expect("helper presence verified by collect_helper_calls");

        // Translate the actual argument as a Term in the Layer 0 sense.
        // If the argument shape isn't liftable, skip THIS call but keep
        // claim on the test (the WHOLE shape is helper-inlining; we
        // don't want Layer 0 to retry).
        let arg_term = match translate_term_pub(&call.arg) {
            Ok(t) => t,
            Err(e) => {
                out.helper_inlined_skipped += 1;
                out.warnings.push(LiftWarning {
                    source_path: source_path.into(),
                    item_name: memento_name,
                    reason: format!("layer2 helper-inline: argument not liftable: {e}"),
                });
                continue;
            }
        };

        // Lift the helper's assertion using Layer 0, then substitute the
        // formal parameter by the actual argument.
        let raw_formula = match lift_assertion_macro_pub(&helper.assertion) {
            Ok(f) => f,
            Err(e) => {
                out.helper_inlined_skipped += 1;
                out.warnings.push(LiftWarning {
                    source_path: source_path.into(),
                    item_name: memento_name,
                    reason: format!(
                        "layer2 helper-inline: helper `{}` body not liftable: {e}",
                        call.helper_name
                    ),
                });
                continue;
            }
        };
        let inlined = subst_var_in_formula(&raw_formula, &helper.param_name, &arg_term);

        out.decls.push(ContractDecl {
            name: memento_name,
            pre: None,
            post: None,
            inv: Some(inlined),
            out_binding: "out".into(),
            evidence: None,
        });
        out.lifted += 1;
        out.helper_inlined_lifted += 1;
    }
}

fn subst_var_in_formula(f: &Rc<Formula>, formal: &str, actual: &Rc<Term>) -> Rc<Formula> {
    match &**f {
        Formula::Atomic { name, args } => {
            let new_args: Vec<Rc<Term>> = args
                .iter()
                .map(|a| subst_var_in_term(a, formal, actual))
                .collect();
            atomic_(name.clone(), new_args)
        }
        Formula::Connective { kind, operands } => {
            let new_ops: Vec<Rc<Formula>> = operands
                .iter()
                .map(|o| subst_var_in_formula(o, formal, actual))
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
            // Don't substitute under a shadowing binder.
            if name == formal {
                f.clone()
            } else {
                Rc::new(Formula::Quantifier {
                    kind: kind.clone(),
                    name: name.clone(),
                    sort: sort.clone(),
                    body: subst_var_in_formula(body, formal, actual),
                })
            }
        }
        Formula::Choice {
            var_name,
            sort,
            body,
        } => {
            // Don't substitute under a shadowing binder.
            if var_name == formal {
                f.clone()
            } else {
                Rc::new(Formula::Choice {
                    var_name: var_name.clone(),
                    sort: sort.clone(),
                    body: subst_var_in_formula(body, formal, actual),
                })
            }
        }
    }
}

fn subst_var_in_term(t: &Rc<Term>, formal: &str, actual: &Rc<Term>) -> Rc<Term> {
    match &**t {
        Term::Var { name } if name == formal => actual.clone(),
        Term::Var { .. } => t.clone(),
        Term::Const { .. } => t.clone(),
        Term::Ctor { name, args } => {
            let new_args: Vec<Rc<Term>> = args
                .iter()
                .map(|a| subst_var_in_term(a, formal, actual))
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
            if param_name == formal {
                t.clone() // shadowed
            } else {
                Rc::new(Term::Lambda {
                    param_name: param_name.clone(),
                    param_sort: param_sort.clone(),
                    body: subst_var_in_term(body, formal, actual),
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
                        bound_term: subst_var_in_term(&b.bound_term, formal, actual),
                    });
                    if b.name == formal {
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
                subst_var_in_term(body, formal, actual)
            };
            Rc::new(Term::Let {
                bindings: new_bindings,
                body: new_body,
            })
        }
    }
}

// ---------------------------------------------------------------------------
// PATTERN 3
// ---------------------------------------------------------------------------

fn classify_characterization(
    macs: &[syn::Macro],
    test_name: &str,
    source_path: &str,
    out: &mut Layer2Output,
) {
    out.claimed_tests.insert(test_name.to_string());
    out.seen += 1;

    let mut atoms: Vec<Rc<Formula>> = Vec::new();
    let mut skipped_atoms: Vec<String> = Vec::new();
    for (i, m) in macs.iter().enumerate() {
        match lift_assertion_macro_pub(m) {
            Ok(f) => atoms.push(f),
            Err(e) => skipped_atoms.push(format!("#{i}: {e}")),
        }
    }
    if atoms.len() < 2 {
        // Not enough liftable atoms to call this a characterization.
        // Drop the claim so Layer 0 can still try the individual asserts.
        out.claimed_tests.remove(test_name);
        out.characterization_skipped += 1;
        out.warnings.push(LiftWarning {
            source_path: source_path.into(),
            item_name: test_name.into(),
            reason: format!(
                "layer2 characterization: only {} of {} asserts were liftable; releasing to layer 0",
                atoms.len(),
                macs.len()
            ),
        });
        return;
    }

    let body = and_(atoms);
    out.decls.push(ContractDecl {
        name: test_name.to_string(),
        pre: None,
        post: None,
        inv: Some(body),
        out_binding: "out".into(),
        evidence: None,
    });
    out.lifted += 1;
    out.characterization_lifted += 1;

    if !skipped_atoms.is_empty() {
        out.warnings.push(LiftWarning {
            source_path: source_path.into(),
            item_name: test_name.into(),
            reason: format!(
                "layer2 characterization: {} atoms skipped from conjunction: {}",
                skipped_atoms.len(),
                skipped_atoms.join("; ")
            ),
        });
    }
}

// ---------------------------------------------------------------------------
// Test-private helpers (kept here so layer2.rs is self-contained).
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(src: &str) -> syn::File {
        syn::parse_file(src).unwrap()
    }

    #[test]
    fn pattern1_bounded_loop_lifts_to_forall_implies() {
        let src = r#"
            #[test]
            fn squares_are_nonneg() {
                for x in 0..100 {
                    assert!(x >= 0);
                }
            }
        "#;
        let f = parse(src);
        let out = lift_file_layer2(&f, "t.rs");
        assert_eq!(out.lifted, 1, "warnings: {:?}", out.warnings);
        assert_eq!(out.bounded_loop_lifted, 1);
        assert!(out.claimed_tests.contains("squares_are_nonneg"));
        let inv = out.decls[0].inv.as_ref().unwrap();
        match &**inv {
            Formula::Quantifier { kind, name, .. } => {
                assert_eq!(kind, "forall");
                assert_eq!(name, "x", "loop var name preserved for stable CID");
            }
            _ => panic!("expected forall"),
        }
    }

    #[test]
    fn pattern1_inclusive_range() {
        let src = r#"
            #[test]
            fn inclusive() {
                for x in 0..=10 {
                    assert!(x >= 0);
                }
            }
        "#;
        let f = parse(src);
        let out = lift_file_layer2(&f, "t.rs");
        assert_eq!(out.lifted, 1);
    }

    #[test]
    fn pattern1_skips_nested_loop_with_warning() {
        let src = r#"
            #[test]
            fn nested() {
                for x in 0..10 {
                    for y in 0..10 {
                        assert!(x >= 0);
                    }
                }
            }
        "#;
        let f = parse(src);
        let out = lift_file_layer2(&f, "t.rs");
        assert_eq!(out.lifted, 0);
        assert_eq!(out.bounded_loop_skipped, 1);
        assert!(out.warnings[0].reason.contains("nested"));
        // Even on skip, the test is claimed so Layer 0 doesn't retry.
        assert!(out.claimed_tests.contains("nested"));
    }

    #[test]
    fn pattern1_skips_unbounded_range() {
        let src = r#"
            #[test]
            fn unbounded() {
                for x in 0.. {
                    assert!(x >= 0);
                }
            }
        "#;
        let f = parse(src);
        let out = lift_file_layer2(&f, "t.rs");
        assert_eq!(out.lifted, 0);
        assert!(out.warnings[0].reason.contains("range"));
    }

    #[test]
    fn pattern2_helper_inlines_each_call() {
        let src = r#"
            fn assert_palindrome(s: &str) {
                assert_eq!(s, "racecar");
            }
            #[test]
            fn palindromes() {
                assert_palindrome("racecar");
                assert_palindrome("level");
            }
        "#;
        let f = parse(src);
        let out = lift_file_layer2(&f, "t.rs");
        assert_eq!(out.lifted, 2, "warnings: {:?}", out.warnings);
        assert_eq!(out.helper_inlined_lifted, 2);
        let names: Vec<_> = out.decls.iter().map(|d| d.name.as_str()).collect();
        assert!(names.contains(&"palindromes::call::0"));
        assert!(names.contains(&"palindromes::call::1"));
    }

    #[test]
    fn pattern3_characterization_lifts_to_conjunction() {
        let src = r#"
            #[test]
            fn three_facts() {
                assert_eq!(f(1), 1);
                assert_eq!(f(2), 2);
                assert_ne!(f(3), 0);
            }
        "#;
        let f = parse(src);
        let out = lift_file_layer2(&f, "t.rs");
        assert_eq!(out.lifted, 1, "warnings: {:?}", out.warnings);
        assert_eq!(out.characterization_lifted, 1);
        let inv = out.decls[0].inv.as_ref().unwrap();
        match &**inv {
            Formula::Connective { kind, operands } => {
                assert_eq!(kind, "and");
                assert_eq!(operands.len(), 3);
            }
            _ => panic!("expected and"),
        }
    }

    #[test]
    fn pattern3_releases_claim_when_only_one_atom_lifts() {
        // f(1) lifts; format!(...) operand does not (Expr::Macro skip);
        // 1 atom < 2 -> not a characterization; claim released.
        //
        // Note: the original version of this test used `"hello".len()` as
        // the "doesn't lift" shape. v0.5 widened the operand whitelist to
        // include method calls, so that shape now lifts and the test
        // needs a genuinely-unsupported shape. `format!(...)` is the
        // documented v0.5 negative shape.
        let src = r#"
            #[test]
            fn mixed() {
                assert_eq!(f(1), 1);
                assert_eq!(s, format!("{}", x));
            }
        "#;
        let f = parse(src);
        let out = lift_file_layer2(&f, "t.rs");
        assert_eq!(out.characterization_lifted, 0);
        assert!(!out.claimed_tests.contains("mixed"));
    }

    #[test]
    fn no_layer2_pattern_means_no_claim() {
        let src = r#"
            #[test]
            fn just_one() {
                assert_eq!(f(1), 1);
            }
        "#;
        let f = parse(src);
        let out = lift_file_layer2(&f, "t.rs");
        assert_eq!(out.lifted, 0);
        assert!(out.claimed_tests.is_empty());
    }
}
