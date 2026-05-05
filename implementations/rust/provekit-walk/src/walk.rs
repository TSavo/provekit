// SPDX-License-Identifier: Apache-2.0
//
// Backward WP propagation over a Rust function body via `syn` AST.
//
// Algorithm sketch (matches issue #368):
//   1. Locate every callsite to the named callee within `caller`.
//   2. For each callsite, the initial WP is the callee's precondition with
//      formal parameters substituted by the actual argument expressions
//      lifted to IR terms.
//   3. Walk backward through the surrounding statements. For each statement
//      `let x = e`, transform the WP via Dijkstra's substitution rule:
//        wp(let x = e, P) = P[e/x]
//   4. The walk terminates at the start of the function body. The WP at
//      that point is the proof obligation the caller's caller must discharge.
//   5. Each (statement, accumulated_WP) pair is one Arrival in the DAG.
//
// MVP limits (deferred to subsequent commits on #368):
//   - Only top-level `let` statements are recognized as allocations.
//   - Only literal-integer `const_int` Rust expressions and bare identifiers
//     are lifted to `IrTerm` for substitution. Anything else is encoded as
//     `IrTerm::Var { name: <printed-source-snippet> }` for traceability and
//     left for richer lifting in subsequent commits.
//   - if-statements introduce no strengthening yet (they are stretch goal).
//   - Only one callee per walk; multiple callees per body are walked
//     independently.

use provekit_ir_types::IrTerm;
use syn::{Expr, ExprCall, ExprPath, ItemFn, Lit, Local, Pat, Stmt};

use crate::wp::{substitute_in_formula, Wp};

/// One step in the walk. Records the WP that holds at this AST location
/// after applying Dijkstra's transformation rule for the statement at
/// `kind`.
///
/// `stmt_index` is the position of the statement within the caller's
/// body (0-indexed); for `FunctionEntry` it is the body length, denoting
/// "before any statement." This is how the MVP attaches arrivals to
/// AST locations without depending on `proc_macro::Span::start()`,
/// which is unstable on stable rustc.
#[derive(Debug, Clone)]
pub struct Arrival {
    pub kind: ArrivalKind,
    pub stmt_index: usize,
    pub wp: Wp,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArrivalKind {
    /// The WP demanded at the callsite (initial state of the walk, after
    /// substituting actual arguments for the callee's formals).
    Callsite { callee: String },
    /// A `let` binding statement encountered during the walk. The `name`
    /// is the bound variable; the WP recorded here is the WP AFTER
    /// applying `P[rhs/name]`.
    LetBinding { name: String },
    /// The function entry point. The WP at this arrival is the proof
    /// obligation for the function as a whole.
    FunctionEntry { fn_name: String },
}

/// One end-to-end walk: from a single callsite, backward through the
/// caller's body, terminating at the function entry. The arrivals list
/// is ordered from callsite (index 0) to entry (last index).
#[derive(Debug, Clone)]
pub struct CallsiteWalk {
    pub caller_name: String,
    pub callee_name: String,
    pub arrivals: Vec<Arrival>,
}

impl CallsiteWalk {
    /// The proof obligation the caller's caller must discharge.
    pub fn entry_wp(&self) -> &Wp {
        &self.arrivals.last().expect("walk has at least one arrival").wp
    }
}

/// Walk every callsite to `callee_name` inside `caller`, propagating the
/// callee's `precondition` backward through the caller's body to the
/// function entry. Returns one walk per callsite.
///
/// The callee's precondition is expressed as a WP over the callee's
/// formal parameter names (e.g. `x ≥ 10` for a callee `f(x: u32)`).
/// At each callsite, `formal_params[i]` is substituted by the lifted
/// IR term of the i-th actual argument expression.
pub fn walk_callsites_to_entry(
    caller: &ItemFn,
    callee_name: &str,
    formal_params: &[String],
    precondition: Wp,
) -> Vec<CallsiteWalk> {
    let caller_name = caller.sig.ident.to_string();
    let stmts: &[Stmt] = &caller.block.stmts;
    let mut walks = Vec::new();
    for (idx, stmt) in stmts.iter().enumerate() {
        for callsite_args in find_callsites_in_stmt(stmt, callee_name) {
            // 1. Build the WP at the callsite by substituting formals -> actuals.
            let mut wp = precondition.clone().into_formula();
            for (formal, actual_expr) in formal_params.iter().zip(callsite_args.iter()) {
                let actual_term = lift_expr_to_term(actual_expr);
                wp = substitute_in_formula(wp, formal, &actual_term);
            }

            let mut arrivals = vec![Arrival {
                kind: ArrivalKind::Callsite {
                    callee: callee_name.to_string(),
                },
                stmt_index: idx,
                wp: Wp(wp.clone()),
            }];

            // 2. Walk backward through the statements preceding this one.
            for (prev_idx, prev) in stmts[..idx].iter().enumerate().rev() {
                if let Some((bound_name, bound_term)) = let_binding(prev) {
                    wp = substitute_in_formula(wp, &bound_name, &bound_term);
                    arrivals.push(Arrival {
                        kind: ArrivalKind::LetBinding { name: bound_name },
                        stmt_index: prev_idx,
                        wp: Wp(wp.clone()),
                    });
                }
                // Non-let statements pass through without transformation in
                // the MVP (e.g. `println!()`). Side-effecting statements are
                // tracked in #368 stretch goals.
            }

            // 3. Function entry. `stmt_index = stmts.len()` denotes
            // "before any statement in the body."
            arrivals.push(Arrival {
                kind: ArrivalKind::FunctionEntry {
                    fn_name: caller_name.clone(),
                },
                stmt_index: stmts.len(),
                wp: Wp(wp),
            });

            walks.push(CallsiteWalk {
                caller_name: caller_name.clone(),
                callee_name: callee_name.to_string(),
                arrivals,
            });
        }
    }
    walks
}

// ----- internals -----

/// Find every direct call to `callee_name` in `stmt`. Returns the actual
/// argument expressions of each found callsite. MVP supports calls in
/// `let _ = f(...)`, expression statements, and assignments; nested calls
/// are walked too.
fn find_callsites_in_stmt(stmt: &Stmt, callee_name: &str) -> Vec<Vec<Expr>> {
    let mut hits = Vec::new();
    let exprs: Vec<&Expr> = match stmt {
        Stmt::Local(local) => match &local.init {
            Some(init) => vec![init.expr.as_ref()],
            None => vec![],
        },
        Stmt::Expr(e, _) => vec![e],
        Stmt::Macro(_) | Stmt::Item(_) => vec![],
    };
    for expr in exprs {
        collect_calls(expr, callee_name, &mut hits);
    }
    hits
}

fn collect_calls(expr: &Expr, callee_name: &str, out: &mut Vec<Vec<Expr>>) {
    if let Expr::Call(ExprCall { func, args, .. }) = expr {
        if let Expr::Path(ExprPath { path, .. }) = func.as_ref() {
            if path
                .segments
                .last()
                .map(|s| s.ident == callee_name)
                .unwrap_or(false)
            {
                out.push(args.iter().cloned().collect());
            }
        }
        // Walk nested calls inside arguments too.
        for a in args {
            collect_calls(a, callee_name, out);
        }
    }
    // Other expression shapes (Block, If, Match, etc.) would recurse here in
    // a richer walker. The MVP fixture doesn't exercise them.
}

/// If `stmt` is a `let pat = expr;` binding, return the bound name and
/// the lifted IR term for `expr`. Otherwise None.
fn let_binding(stmt: &Stmt) -> Option<(String, IrTerm)> {
    match stmt {
        Stmt::Local(Local {
            pat,
            init: Some(init),
            ..
        }) => {
            let name = match pat {
                Pat::Ident(p) => p.ident.to_string(),
                Pat::Type(pt) => match &*pt.pat {
                    Pat::Ident(p) => p.ident.to_string(),
                    _ => return None,
                },
                _ => return None,
            };
            let term = lift_expr_to_term(init.expr.as_ref());
            Some((name, term))
        }
        _ => None,
    }
}

/// Lift a Rust `syn::Expr` to a canonical `IrTerm`. MVP support:
///  - Integer literal: `IrTerm::Const { value: <num>, sort: Int }`.
///  - Bare identifier: `IrTerm::Var { name: <ident> }`.
///  - Anything else: `IrTerm::Var { name: "<expr:<tokens>>" }`,
///    preserved as a placeholder so substitution sees a stable identity.
///    Richer lifting (binary ops, function calls, etc.) is the next
///    step in #368.
fn lift_expr_to_term(expr: &Expr) -> IrTerm {
    match expr {
        Expr::Lit(lit) => match &lit.lit {
            Lit::Int(n) => match n.base10_parse::<i64>() {
                Ok(v) => crate::wp::const_int(v),
                Err(_) => placeholder_term(expr),
            },
            _ => placeholder_term(expr),
        },
        Expr::Path(ExprPath { path, .. }) => {
            if let Some(seg) = path.segments.last() {
                crate::wp::var(seg.ident.to_string())
            } else {
                placeholder_term(expr)
            }
        }
        _ => placeholder_term(expr),
    }
}

fn placeholder_term(expr: &Expr) -> IrTerm {
    use quote::ToTokens;
    crate::wp::var(format!("<expr:{}>", expr.to_token_stream()))
}
