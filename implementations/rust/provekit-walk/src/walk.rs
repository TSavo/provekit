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

use provekit_ir_types::{IrFormula, IrTerm};
use syn::{Expr, ExprCall, ExprIf, ExprPath, ItemFn, Lit, Local, Pat, Stmt};

use crate::lift::lift_predicate;
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
        for hit in find_callsites_with_context(stmt, callee_name) {
            // 1. Build the WP at the callsite by substituting formals -> actuals.
            let mut wp = precondition.clone().into_formula();
            for (formal, actual_expr) in formal_params.iter().zip(hit.args.iter()) {
                let actual_term = lift_expr_to_term(actual_expr);
                wp = substitute_in_formula(wp, formal, &actual_term);
            }
            // Premise the WP on every in-scope if-condition: if any of the
            // surrounding conditions failed, the callsite would not have
            // been reached, so the WP would not need to hold. Encode this
            // as `(c1 ∧ c2 ∧ ...) → wp` — the conditions are free posts
            // at the callsite, available as premises for substrate
            // discharge.
            if !hit.conditions.is_empty() {
                let premise = match hit.conditions.len() {
                    1 => hit.conditions[0].clone(),
                    _ => IrFormula::And {
                        operands: hit.conditions.clone(),
                    },
                };
                wp = IrFormula::Implies {
                    operands: vec![premise, wp],
                };
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

/// One callsite found during context-aware traversal: the actual
/// arguments at the callsite, plus the conjunction of in-scope
/// if-conditions (each tagged for the branch the callsite was found in,
/// already-negated where the callsite is in an else-branch).
///
/// Sir's framing: every if is a free post. When a callsite sits inside
/// `if cond { ... }`, `cond` is a premise the substrate has for free at
/// that callsite. The walk records these conditions so the substrate's
/// downstream discharge can use them.
#[derive(Debug, Clone)]
pub struct CallsiteHit {
    pub args: Vec<Expr>,
    pub conditions: Vec<IrFormula>,
}

/// Find every callsite to `callee_name` reachable from `stmt`, descending
/// into if-statement branches and tracking the surrounding condition
/// context. The condition context flips sign when descending into the
/// else-branch: `if cond { then-region } else { else-region }` adds
/// `cond` to the then-region's context and `¬cond` to the else-region's.
fn find_callsites_with_context(stmt: &Stmt, callee_name: &str) -> Vec<CallsiteHit> {
    let mut hits = Vec::new();
    walk_stmt_for_callsites(stmt, callee_name, &mut Vec::new(), &mut hits);
    hits
}

fn walk_stmt_for_callsites(
    stmt: &Stmt,
    callee_name: &str,
    conditions: &mut Vec<IrFormula>,
    hits: &mut Vec<CallsiteHit>,
) {
    let exprs: Vec<&Expr> = match stmt {
        Stmt::Local(local) => match &local.init {
            Some(init) => vec![init.expr.as_ref()],
            None => vec![],
        },
        Stmt::Expr(e, _) => vec![e],
        Stmt::Macro(_) | Stmt::Item(_) => vec![],
    };
    for expr in exprs {
        walk_expr_for_callsites(expr, callee_name, conditions, hits);
    }
}

fn walk_expr_for_callsites(
    expr: &Expr,
    callee_name: &str,
    conditions: &mut Vec<IrFormula>,
    hits: &mut Vec<CallsiteHit>,
) {
    match expr {
        Expr::Call(ExprCall { func, args, .. }) => {
            // Direct call check at this expression.
            if let Expr::Path(ExprPath { path, .. }) = func.as_ref() {
                if path
                    .segments
                    .last()
                    .map(|s| s.ident == callee_name)
                    .unwrap_or(false)
                {
                    hits.push(CallsiteHit {
                        args: args.iter().cloned().collect(),
                        conditions: conditions.clone(),
                    });
                }
            }
            // Recurse into argument expressions.
            for a in args {
                walk_expr_for_callsites(a, callee_name, conditions, hits);
            }
        }
        Expr::MethodCall(m) => {
            // Method call `recv.foo(args)` is a callsite to `foo` if
            // foo matches callee_name. The receiver becomes the
            // implicit first argument (paper 07 normalizes
            // `Type::method(recv, args)`).
            if m.method == callee_name {
                let mut all_args: Vec<Expr> = vec![(*m.receiver).clone()];
                for a in &m.args {
                    all_args.push(a.clone());
                }
                hits.push(CallsiteHit {
                    args: all_args,
                    conditions: conditions.clone(),
                });
            }
            // Recurse into receiver and args for nested callsites.
            walk_expr_for_callsites(&m.receiver, callee_name, conditions, hits);
            for a in &m.args {
                walk_expr_for_callsites(a, callee_name, conditions, hits);
            }
        }
        Expr::If(ExprIf {
            cond,
            then_branch,
            else_branch,
            ..
        }) => {
            // Lift the condition; if it doesn't lift to an IrFormula we
            // proceed without adding a context entry (the walker is
            // best-effort: missing conditions are equivalent to `true`).
            let lifted = lift_predicate(cond);
            // Then-branch: condition holds.
            if let Some(c) = &lifted {
                conditions.push(c.clone());
            }
            for s in &then_branch.stmts {
                walk_stmt_for_callsites(s, callee_name, conditions, hits);
            }
            if lifted.is_some() {
                conditions.pop();
            }
            // Else-branch: ¬condition holds.
            if let Some((_, else_expr)) = else_branch {
                if let Some(c) = &lifted {
                    let negated = IrFormula::Not {
                        operands: vec![c.clone()],
                    };
                    conditions.push(negated);
                }
                walk_expr_for_callsites(else_expr, callee_name, conditions, hits);
                if lifted.is_some() {
                    conditions.pop();
                }
            }
        }
        Expr::Block(b) => {
            for s in &b.block.stmts {
                walk_stmt_for_callsites(s, callee_name, conditions, hits);
            }
        }
        // Loops: recurse into the body. The body's callsites are reachable
        // from the loop's pre-state; their conditions are unchanged from
        // outside the loop (we don't yet add loop-iteration invariants
        // here — that would require lift-side invariant inference; for
        // the MVP we walk the body once with the surrounding context).
        Expr::While(w) => {
            for s in &w.body.stmts {
                walk_stmt_for_callsites(s, callee_name, conditions, hits);
            }
        }
        Expr::ForLoop(fl) => {
            for s in &fl.body.stmts {
                walk_stmt_for_callsites(s, callee_name, conditions, hits);
            }
        }
        Expr::Loop(l) => {
            for s in &l.body.stmts {
                walk_stmt_for_callsites(s, callee_name, conditions, hits);
            }
        }
        // Match arms: each arm's body sees its pattern's binding context.
        // For the MVP we descend into every arm's body without
        // narrowing the pattern as a predicate; the postcondition split
        // is captured separately in the lifter (lift_match_postcondition).
        Expr::Match(m) => {
            for arm in &m.arms {
                walk_expr_for_callsites(&arm.body, callee_name, conditions, hits);
            }
        }
        // `?` operator: the success-path continues with the unwrapped
        // value. The MVP recurses into the wrapped expression to find
        // any callsites it contains.
        Expr::Try(t) => {
            walk_expr_for_callsites(&t.expr, callee_name, conditions, hits);
        }
        // Return statements: recurse into the returned expression for
        // callsite discovery.
        Expr::Return(r) => {
            if let Some(inner) = &r.expr {
                walk_expr_for_callsites(inner, callee_name, conditions, hits);
            }
        }
        // Other shapes pass through silently. Stretch goals add closure
        // bodies, async blocks, etc.
        _ => {}
    }
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
