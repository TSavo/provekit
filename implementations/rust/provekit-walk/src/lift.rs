// SPDX-License-Identifier: Apache-2.0
//
// lift.rs — build predicates from Rust source.
//
// Recognizes the patterns that paper 07 reads as "every if-statement is
// a contract": `if cond { panic!() }`, `assert!(cond)`, `debug_assert!(cond)`.
// Each such pattern contributes a leaf precondition the function's body
// is implicitly demanding from its caller.
//
// MVP scope:
//   - if-then-panic: `if cond { panic!(...) }` → ¬cond holds for the
//     non-panic continuation, so the function's effective precondition
//     accumulates ¬cond.
//   - assert! family: `assert!(cond)` → cond holds afterward (and the
//     caller must have established cond up to here).
//   - Binary comparisons `<`, `≤`, `>`, `≥`, `==`, `!=` lift to the
//     corresponding `AtomicPredicateName`.
//   - Compound `&&` lifts to `IrFormula::And`; `||` to `IrFormula::Or`;
//     `!cond` to `IrFormula::Not`.
//
// Out of scope (later commits on #368):
//   - if-then-else with non-panic else (introduces conditional
//     strengthening, not a flat precondition).
//   - match arms.
//   - early-return patterns beyond `panic!`.
//   - postcondition lifting from `return` expressions.

use std::collections::HashSet;

use provekit_ir_types::{IrFormula, IrTerm};
use syn::{
    BinOp, Expr, ExprBinary, ExprIf, ExprMacro, ExprUnary, ItemFn, Lit, Local, Macro, Pat, Stmt,
    StmtMacro, UnOp,
};

use crate::wp::{free_vars_formula, Wp};

// ---- LiftCtx: scope-tracked name resolution ----
//
// The shadow AST is the structural witness of the source. Per Sir's
// "shadow AST pays rent" directive (#368), the lifter consults a
// scope walker mirroring the shadow tree's structure when emitting IR
// variable references. Each closure binder receives a globally-unique
// id within the formula; references inside the closure body resolve
// to that unique id; references outside (free variables) keep their
// surface name. The result is that lifted IR is in barendregt form
// for closure binders by construction — capture is impossible at the
// lift layer for those binders. The capture-avoiding substitution in
// `wp.rs` is the belt-and-suspenders second line.
//
// The binder counter is per-formula. Two structurally identical
// inputs produce structurally identical IR (deterministic for content
// addressing).
struct LiftCtx {
    next_binder_id: u32,
    /// Stack of frames; each frame holds (surface_name, unique_name) pairs
    /// in declaration order. Innermost frame shadows outer frames.
    scope: Vec<Vec<(String, String)>>,
}

impl LiftCtx {
    fn new() -> Self {
        Self {
            next_binder_id: 0,
            scope: Vec::new(),
        }
    }

    fn push_frame(&mut self) {
        self.scope.push(Vec::new());
    }

    fn pop_frame(&mut self) {
        self.scope.pop();
    }

    /// Bind `base` in the innermost frame to a fresh unique name; return
    /// the unique name. Caller must have pushed at least one frame.
    fn bind(&mut self, base: &str) -> String {
        let id = self.next_binder_id;
        self.next_binder_id += 1;
        let unique = format!("{}#{}", base, id);
        self.scope
            .last_mut()
            .expect("LiftCtx::bind without push_frame")
            .push((base.to_string(), unique.clone()));
        unique
    }

    /// Resolve a surface name to its unique form. If not bound in any
    /// frame, the name is free in this formula and returned unchanged.
    fn resolve(&self, base: &str) -> String {
        for frame in self.scope.iter().rev() {
            for (b, u) in frame.iter().rev() {
                if b == base {
                    return u.clone();
                }
            }
        }
        base.to_string()
    }
}

/// Lift the implicit precondition from a function body. Walks every
/// statement and conjoins the contribution of each pattern recognized.
///
/// Returns `Wp(true)` if no patterns are recognized — this means the
/// function makes no demands on its caller (a vacuous precondition).
pub fn lift_function_precondition(item_fn: &ItemFn) -> Wp {
    let mut ctx = LiftCtx::new();
    let mut accum: Vec<IrFormula> = Vec::new();
    for stmt in &item_fn.block.stmts {
        if let Some(predicate) = lift_stmt_contribution(stmt, &mut ctx) {
            accum.push(predicate);
        }
    }
    Wp(simplify_conjunction(accum))
}

/// Lift the implicit postcondition from a function body. Returns the
/// conjunction of predicates DERIVED from the body's structure that
/// hold at every reachable return point.
///
/// Derivation sources:
///   - if-then-panic: ¬cond holds for the non-panic continuation
///     (Sir's "every if is a free post; every else is the contraposition").
///   - assert!(c): c holds afterward.
///   - Trailing return expression: derives `result = <expr>` where
///     <expr> is lifted to an IrTerm and named-result equates with it.
///
/// This is real derivation: facts the substrate produces from the
/// body's structure that did not appear as explicit annotations. The
/// postcondition is the conjunction of every such derived fact.
pub fn lift_function_postcondition(item_fn: &ItemFn) -> Wp {
    let mut ctx = LiftCtx::new();

    // 1. Collect entry-assertion contributions, but track which names are
    //    subsequently shadowed by `let` bindings. An entry assertion
    //    `assert!(x >= 5)` that is followed LATER by `let x = 0` is UNSOUND to
    //    copy into the postcondition: after `let x = 0` the name `x` means
    //    something else. Drop any entry assertion whose free variables are
    //    shadowed by a `let` at a LATER position in the body.
    //
    //    Algorithm: walk statements in order, collecting (formula, position)
    //    pairs for assertions. Then for each assertion, collect names bound by
    //    `let` statements that come AFTER the assertion's index, and filter out
    //    assertions whose free variables overlap those later-bound names.
    let stmts = &item_fn.block.stmts;
    let mut entry_assertions: Vec<(IrFormula, usize)> = Vec::new();
    for (i, stmt) in stmts.iter().enumerate() {
        if let Some(predicate) = lift_stmt_contribution(stmt, &mut ctx) {
            entry_assertions.push((predicate, i));
        }
    }

    // Keep only assertions whose free variables are NOT shadowed by a LATER
    // `let` binding. A `let` that precedes the assertion is fine (it introduces
    // the name the assertion references); only later rebindings are unsound.
    let mut accum: Vec<IrFormula> = entry_assertions
        .into_iter()
        .filter(|(formula, assert_idx)| {
            let free = free_vars_formula(formula);
            // Collect names bound by `let` statements at positions AFTER this assertion.
            let mut later_bound: HashSet<String> = HashSet::new();
            for stmt in stmts.iter().skip(assert_idx + 1) {
                collect_let_bound_names(stmt, &mut later_bound);
            }
            // Keep this assertion only if none of its free vars are rebound later.
            free.is_disjoint(&later_bound)
        })
        .map(|(formula, _)| formula)
        .collect();

    // 2. Trailing-expression derivation: if the function body ends with
    //    an expression statement (no trailing semicolon), that
    //    expression is the function's return value. Derive
    //    `result = <lifted expression>` and add to the postcondition.
    if let Some(Stmt::Expr(e, None)) = stmts.last() {
        if let Some(term) = lift_tail_expr_to_result_term(e, &mut ctx) {
            let result_var = IrTerm::Var {
                name: "result".to_string(),
            };
            accum.push(IrFormula::Atomic {
                name: "=".to_string(),
                args: vec![result_var, term],
            });
        }
    }

    // 3. Explicit `return expr;` tails. If the body has an explicit
    //    `return <expr>;` statement, derive `result = <lifted expr>`.
    for stmt in stmts {
        if let Some(formula) = lift_return_stmt_postcondition(stmt, &mut ctx) {
            accum.push(formula);
        }
    }

    Wp(simplify_conjunction(accum))
}

fn lift_tail_expr_to_result_term(expr: &Expr, ctx: &mut LiftCtx) -> Option<IrTerm> {
    match expr {
        Expr::If(if_expr) => lift_tail_if_to_ite_term(if_expr, ctx),
        _ => lift_expr_to_term_inner(expr, ctx),
    }
}

fn lift_tail_if_to_ite_term(if_expr: &ExprIf, ctx: &mut LiftCtx) -> Option<IrTerm> {
    let cond = lift_predicate_inner(&if_expr.cond, ctx)?;
    let cond_term = formula_to_term(cond)?;
    let then_expr = block_single_tail_expr(&if_expr.then_branch)?;
    let then_term = lift_expr_to_term_inner(then_expr, ctx)?;
    let (_, else_expr) = if_expr.else_branch.as_ref()?;
    let else_tail = expr_single_tail_expr(else_expr)?;
    let else_term = lift_expr_to_term_inner(else_tail, ctx)?;
    Some(IrTerm::Ctor {
        name: "ite".to_string(),
        args: vec![cond_term, then_term, else_term],
    })
}

fn formula_to_term(formula: IrFormula) -> Option<IrTerm> {
    match formula {
        IrFormula::Atomic { name, args } => Some(IrTerm::Ctor { name, args }),
        IrFormula::And { operands } => formula_operands_to_term("and", operands),
        IrFormula::Or { operands } => formula_operands_to_term("or", operands),
        IrFormula::Not { operands } => formula_operands_to_term("not", operands),
        IrFormula::Implies { operands } => formula_operands_to_term("implies", operands),
        IrFormula::Forall { .. } | IrFormula::Exists { .. } | IrFormula::Choice { .. } => None,
        // Substitute and Apply are meta-level; not reducible to a term here.
        IrFormula::Substitute { .. }
        | IrFormula::Apply { .. }
        | IrFormula::DivergenceBetween { .. } => None,
    }
}

fn formula_operands_to_term(name: &str, operands: Vec<IrFormula>) -> Option<IrTerm> {
    let args = operands
        .into_iter()
        .map(formula_to_term)
        .collect::<Option<Vec<_>>>()?;
    Some(IrTerm::Ctor {
        name: name.to_string(),
        args,
    })
}

fn block_single_tail_expr(block: &syn::Block) -> Option<&Expr> {
    match block.stmts.as_slice() {
        [Stmt::Expr(expr, None)] => Some(expr),
        _ => None,
    }
}

fn expr_single_tail_expr(expr: &Expr) -> Option<&Expr> {
    match expr {
        Expr::Block(block) => block_single_tail_expr(&block.block),
        other => Some(other),
    }
}

/// Collect all names bound by `let` patterns at the top level of a statement.
/// Used for the shadowing check in `lift_function_postcondition`.
fn collect_let_bound_names(stmt: &Stmt, out: &mut HashSet<String>) {
    if let Stmt::Local(Local { pat, .. }) = stmt {
        collect_pat_names(pat, out);
    }
}

/// Recursively collect all bound names from a pattern.
fn collect_pat_names(pat: &Pat, out: &mut HashSet<String>) {
    match pat {
        Pat::Ident(p) => {
            out.insert(p.ident.to_string());
        }
        Pat::Type(pt) => collect_pat_names(&pt.pat, out),
        Pat::Reference(r) => collect_pat_names(&r.pat, out),
        Pat::Paren(p) => collect_pat_names(&p.pat, out),
        Pat::Tuple(t) => {
            for sub in &t.elems {
                collect_pat_names(sub, out);
            }
        }
        Pat::TupleStruct(ts) => {
            for sub in &ts.elems {
                collect_pat_names(sub, out);
            }
        }
        Pat::Struct(s) => {
            for field in &s.fields {
                collect_pat_names(&field.pat, out);
            }
        }
        Pat::Slice(s) => {
            for sub in &s.elems {
                collect_pat_names(sub, out);
            }
        }
        _ => {}
    }
}

/// If a statement is an explicit `return <expr>;`, derive
/// `result = <lifted expr>`. Returns None for other statement kinds.
fn lift_return_stmt_postcondition(stmt: &Stmt, ctx: &mut LiftCtx) -> Option<IrFormula> {
    let expr = match stmt {
        Stmt::Expr(e, Some(_)) => e, // Expr with trailing semicolon
        _ => return None,
    };
    if let Expr::Return(ret) = expr {
        if let Some(inner) = &ret.expr {
            if let Some(term) = lift_expr_to_term_inner(inner, ctx) {
                let result_var = IrTerm::Var {
                    name: "result".to_string(),
                };
                return Some(IrFormula::Atomic {
                    name: "=".to_string(),
                    args: vec![result_var, term],
                });
            }
        }
    }
    None
}

/// What does this single statement contribute to the function's
/// implicit precondition? Returns None for statements that don't lift
/// (let-bindings, plain expressions, etc.).
fn lift_stmt_contribution(stmt: &Stmt, ctx: &mut LiftCtx) -> Option<IrFormula> {
    match stmt {
        Stmt::Expr(e, _) => lift_expr_contribution(e, ctx),
        // `assert!(c);` at statement position parses to Stmt::Macro
        // (with optional trailing semicolon), not Stmt::Expr(Expr::Macro).
        Stmt::Macro(StmtMacro { mac, .. }) => lift_macro_contribution(mac, ctx),
        _ => None,
    }
}

/// Recognize and lift macro contributions at statement or expression
/// position. Used by both `Stmt::Macro` and `Expr::Macro` paths.
fn lift_macro_contribution(mac: &Macro, ctx: &mut LiftCtx) -> Option<IrFormula> {
    let seg = mac.path.segments.last()?;
    let name = seg.ident.to_string();
    match name.as_str() {
        "assert" => {
            let parsed_cond = syn::parse2::<Expr>(mac.tokens.clone()).ok()?;
            // assert!(c) parses to just c. assert!(c, "msg") parses
            // as a tuple-expr; take the first elem.
            let first = match &parsed_cond {
                Expr::Tuple(t) => t.elems.first()?,
                other => other,
            };
            lift_predicate_inner(first, ctx)
        }
        // debug_assert! is compiled out in release builds. Lifting its
        // predicate as a real contract would misrepresent what holds in
        // release mode. Skip it entirely.
        _ => None,
    }
}

fn lift_expr_contribution(expr: &Expr, ctx: &mut LiftCtx) -> Option<IrFormula> {
    // if-then-panic pattern: `if cond { panic!() }` lifts to ¬cond.
    if let Expr::If(ExprIf {
        cond,
        then_branch,
        else_branch,
        ..
    }) = expr
    {
        if else_branch.is_none() && block_only_panics(then_branch) {
            let cond_formula = lift_predicate_inner(cond, ctx)?;
            return Some(negate(cond_formula));
        }
    }
    // assert!()-shaped macros sometimes parse as Expr::Macro (e.g. when
    // they're the trailing tail expression of a block).
    if let Expr::Macro(ExprMacro { mac, .. }) = expr {
        if let Some(formula) = lift_macro_contribution(mac, ctx) {
            return Some(formula);
        }
    }
    None
}

/// Lift an arbitrary Rust predicate-shaped expression to `IrFormula`.
/// Returns None for shapes the MVP does not yet handle.
pub fn lift_predicate(expr: &Expr) -> Option<IrFormula> {
    let mut ctx = LiftCtx::new();
    lift_predicate_inner(expr, &mut ctx)
}

fn lift_predicate_inner(expr: &Expr, ctx: &mut LiftCtx) -> Option<IrFormula> {
    match expr {
        Expr::Binary(ExprBinary {
            left, op, right, ..
        }) => match op {
            BinOp::And(_) => {
                let l = lift_predicate_inner(left, ctx)?;
                let r = lift_predicate_inner(right, ctx)?;
                Some(IrFormula::And {
                    operands: vec![l, r],
                })
            }
            BinOp::Or(_) => {
                let l = lift_predicate_inner(left, ctx)?;
                let r = lift_predicate_inner(right, ctx)?;
                Some(IrFormula::Or {
                    operands: vec![l, r],
                })
            }
            _ => {
                // Comparison: lift both sides as terms, pick the IR predicate name.
                let name = bin_op_to_predicate_name(op)?;
                let l_term = lift_expr_to_term_inner(left, ctx)?;
                let r_term = lift_expr_to_term_inner(right, ctx)?;
                Some(IrFormula::Atomic {
                    name: name.to_string(),
                    args: vec![l_term, r_term],
                })
            }
        },
        Expr::Unary(ExprUnary {
            op: UnOp::Not(_),
            expr,
            ..
        }) => {
            let inner = lift_predicate_inner(expr, ctx)?;
            // Apply De Morgan / double-negation via the negate helper,
            // so `!(x >= 10)` lifts to `x < 10`, not `¬(x ≥ 10)`.
            Some(negate(inner))
        }
        Expr::Paren(p) => lift_predicate_inner(&p.expr, ctx),
        // Zero-argument method calls that return bool: `.is_some()`, `.is_none()`,
        // `.is_empty()`, `.is_err()`, `.is_ok()`. These are common predicate shapes
        // in Rust and appear naturally in the dropper's emitted guard code.
        // Each lifts to `IrFormula::Atomic { name: "is_some" (or similar), args: [recv] }`.
        Expr::MethodCall(syn::ExprMethodCall {
            receiver,
            method,
            args,
            ..
        }) if args.is_empty() => {
            let method_name = method.to_string();
            let is_bool_predicate = matches!(
                method_name.as_str(),
                "is_some" | "is_none" | "is_empty" | "is_err" | "is_ok"
            );
            if is_bool_predicate {
                let recv_term = lift_expr_to_term_inner(receiver, ctx)?;
                Some(IrFormula::Atomic {
                    name: method_name,
                    args: vec![recv_term],
                })
            } else {
                None
            }
        }
        // Anything else is unrecognized in the MVP.
        _ => None,
    }
}

/// Lift a Rust expression to a canonical `IrTerm`. Supported shapes:
///   - Integer literal: `IrTerm::Const { value: <num>, sort: Int }`.
///   - Bool literal: `IrTerm::Const { value: <bool>, sort: Bool }`.
///   - Bare identifier: `IrTerm::Var { name: <ident> }`.
///   - Parenthesized expression: recurses on the inner expression.
///   - Reference (`&x`, `&mut x`): unwraps; for substrate purposes
///     a borrow is the value's identity (substitution-equivalent).
///   - Cast (`x as u32`): unwraps to the inner term (the IR's Sort
///     captures type changes; the term-level lift ignores casts).
///   - Field access (`s.f`): `Ctor("field", [s_term, "f"])`.
///   - Index (`a[i]`): `Ctor("index", [a_term, i_term])`.
///   - Method call (`x.foo(args)`): `Ctor("method:foo", [x, ...args])`.
///   - Range (`a..b`, `a..=b`): `Ctor("range", [a, b])` /
///     `Ctor("range_incl", [a, b])`.
///   - Tuple (`(a, b, c)`): `Ctor("tuple", [a, b, c])`.
///   - Unary negation (`-x`), bitwise not (`!x` on integers):
///     `Ctor("neg" / "bit-not", [...])`.
///   - Binary arithmetic (`+`, `-`, `*`, `/`, `%`, `&`, `|`, `^`,
///     `<<`, `>>`): lifts to `Ctor(<op>, [lhs, rhs])`.
///
/// Anything else returns None.
pub fn lift_expr_to_term(expr: &Expr) -> Option<IrTerm> {
    let mut ctx = LiftCtx::new();
    lift_expr_to_term_inner(expr, &mut ctx)
}

fn lift_expr_to_term_inner(expr: &Expr, ctx: &mut LiftCtx) -> Option<IrTerm> {
    match expr {
        Expr::Lit(lit) => match &lit.lit {
            Lit::Int(n) => match n.base10_parse::<i64>() {
                Ok(v) => Some(crate::wp::const_int(v)),
                Err(_) => None,
            },
            Lit::Bool(b) => Some(IrTerm::Const {
                value: serde_json::Value::Bool(b.value),
                sort: provekit_ir_types::Sort::Primitive {
                    name: "Bool".to_string(),
                },
            }),
            _ => None,
        },
        Expr::Path(syn::ExprPath { path, .. }) => {
            let seg = path.segments.last()?;
            // Resolve through the scope stack: bound names map to their
            // unique forms; free variables keep their surface name.
            Some(crate::wp::var(ctx.resolve(&seg.ident.to_string())))
        }
        Expr::Paren(p) => lift_expr_to_term_inner(&p.expr, ctx),
        Expr::Reference(r) => lift_expr_to_term_inner(&r.expr, ctx),
        Expr::Cast(c) => lift_expr_to_term_inner(&c.expr, ctx),
        Expr::Field(f) => {
            let base = lift_expr_to_term_inner(&f.base, ctx)?;
            let name = match &f.member {
                syn::Member::Named(id) => id.to_string(),
                syn::Member::Unnamed(idx) => idx.index.to_string(),
            };
            Some(IrTerm::Ctor {
                name: "field".to_string(),
                args: vec![
                    base,
                    IrTerm::Var {
                        name: format!(".{}", name),
                    },
                ],
            })
        }
        Expr::Index(i) => {
            let arr = lift_expr_to_term_inner(&i.expr, ctx)?;
            let idx = lift_expr_to_term_inner(&i.index, ctx)?;
            Some(IrTerm::Ctor {
                name: "index".to_string(),
                args: vec![arr, idx],
            })
        }
        Expr::MethodCall(m) => {
            let receiver = lift_expr_to_term_inner(&m.receiver, ctx)?;
            let mut args = vec![receiver];
            for a in &m.args {
                let lifted = lift_expr_to_term_inner(a, ctx)?;
                args.push(lifted);
            }
            Some(IrTerm::Ctor {
                name: format!("method:{}", m.method),
                args,
            })
        }
        Expr::Range(r) => {
            let start = match &r.start {
                Some(e) => lift_expr_to_term_inner(e, ctx)?,
                None => crate::wp::var("_"),
            };
            let end = match &r.end {
                Some(e) => lift_expr_to_term_inner(e, ctx)?,
                None => crate::wp::var("_"),
            };
            let name = match r.limits {
                syn::RangeLimits::HalfOpen(_) => "range",
                syn::RangeLimits::Closed(_) => "range_incl",
            };
            Some(IrTerm::Ctor {
                name: name.to_string(),
                args: vec![start, end],
            })
        }
        Expr::Tuple(t) => {
            let mut args = Vec::with_capacity(t.elems.len());
            for e in &t.elems {
                args.push(lift_expr_to_term_inner(e, ctx)?);
            }
            Some(IrTerm::Ctor {
                name: "tuple".to_string(),
                args,
            })
        }
        Expr::Array(a) => {
            let mut args = Vec::with_capacity(a.elems.len());
            for e in &a.elems {
                args.push(lift_expr_to_term_inner(e, ctx)?);
            }
            Some(IrTerm::Ctor {
                name: "array".to_string(),
                args,
            })
        }
        Expr::Repeat(r) => {
            let elem = lift_expr_to_term_inner(&r.expr, ctx)?;
            let count = lift_expr_to_term_inner(&r.len, ctx)?;
            Some(IrTerm::Ctor {
                name: "array_repeat".to_string(),
                args: vec![elem, count],
            })
        }
        Expr::Closure(c) => {
            // `|x| body` lifts to IrTerm::Lambda. Multi-arg closures
            // collapse into nested lambdas (right-associative). Each
            // closure parameter is bound in a fresh scope frame and
            // assigned a globally-unique id by the LiftCtx; references
            // to that name inside the closure body resolve to the
            // unique form. The shadow AST's structural traversal owns
            // this name resolution.
            ctx.push_frame();
            let mut unique_names: Vec<String> = Vec::with_capacity(c.inputs.len());
            for input in &c.inputs {
                let base = match input {
                    syn::Pat::Ident(p) => p.ident.to_string(),
                    syn::Pat::Type(pt) => match &*pt.pat {
                        syn::Pat::Ident(p) => p.ident.to_string(),
                        _ => {
                            ctx.pop_frame();
                            return None;
                        }
                    },
                    _ => {
                        ctx.pop_frame();
                        return None;
                    }
                };
                unique_names.push(ctx.bind(&base));
            }
            let body_lifted = lift_expr_to_term_inner(&c.body, ctx);
            ctx.pop_frame();
            let body = body_lifted?;
            let mut term = body;
            for unique in unique_names.into_iter().rev() {
                term = IrTerm::Lambda {
                    param_name: unique,
                    param_sort: provekit_ir_types::Sort::Primitive {
                        name: "Int".to_string(),
                    },
                    body: Box::new(term),
                };
            }
            Some(term)
        }
        Expr::Await(a) => {
            // `expr.await` desugars to a state machine that yields and
            // resumes; for substrate purposes it produces the awaited
            // value, so we lift as the inner expr.
            lift_expr_to_term_inner(&a.base, ctx)
        }
        Expr::Async(a) => {
            // `async { body }` produces a Future. The substrate sees
            // through to the body's eventual value: lift the trailing
            // expression of the block.
            if let Some(syn::Stmt::Expr(e, None)) = a.block.stmts.last() {
                lift_expr_to_term_inner(e, ctx)
            } else {
                None
            }
        }
        Expr::Call(call) => {
            // Plain function call `f(args)`. Lift to a ctor named by the
            // callee's bare symbol (the last path segment) so the call tree
            // SURVIVES into the contract formula. This is the keystone: the
            // ctor name matches the callee's auto-minted bridge `sourceSymbol`,
            // so `enumerate_callsites` finds the seam, `resolve_target` pulls
            // the callee's precondition, and the runner discharges
            // `producer_post -> callee_pre`. Without this arm the call tree
            // vanished and the postcondition collapsed to a vacuous `true` --
            // the missing edge was invisible to the solver. Mirrors the
            // `Expr::MethodCall` arm. Language-blind once emitted: the catch
            // lives in the verifier, below the source language.
            let Expr::Path(syn::ExprPath { path, .. }) = &*call.func else {
                // Calls through a non-path callee (closure value, fn pointer
                // in a local, etc.) have no stable bridge symbol to name.
                return None;
            };
            let callee = path.segments.last()?.ident.to_string();
            let mut args = Vec::with_capacity(call.args.len());
            for a in &call.args {
                args.push(lift_expr_to_term_inner(a, ctx)?);
            }
            Some(IrTerm::Ctor { name: callee, args })
        }
        Expr::If(_) | Expr::Match(_) | Expr::Block(_) => {
            // Conditional / match / block expressions don't lift to a
            // single canonical IR term in the MVP — they would need a
            // case-analysis ctor. Tagged for future iteration.
            None
        }
        Expr::Binary(ExprBinary {
            left, op, right, ..
        }) => {
            let op_name = match op {
                BinOp::Add(_) => "+",
                BinOp::Sub(_) => "-",
                BinOp::Mul(_) => "*",
                BinOp::Div(_) => "/",
                BinOp::Rem(_) => "%",
                BinOp::BitAnd(_) => "&",
                BinOp::BitOr(_) => "|",
                BinOp::BitXor(_) => "^",
                BinOp::Shl(_) => "<<",
                BinOp::Shr(_) => ">>",
                _ => return None,
            };
            let l = lift_expr_to_term_inner(left, ctx)?;
            let r = lift_expr_to_term_inner(right, ctx)?;
            Some(IrTerm::Ctor {
                name: op_name.to_string(),
                args: vec![l, r],
            })
        }
        Expr::Unary(ExprUnary { op, expr, .. }) => {
            let inner = lift_expr_to_term_inner(expr, ctx)?;
            let name = match op {
                UnOp::Neg(_) => {
                    if let IrTerm::Const { value, sort } = &inner {
                        if let Some(n) = value.as_i64() {
                            return Some(IrTerm::Const {
                                value: serde_json::json!(-n),
                                sort: sort.clone(),
                            });
                        }
                    }
                    "neg"
                }
                UnOp::Not(_) => "bit-not",
                UnOp::Deref(_) => return Some(inner), // *x is x for substitution
                _ => return None,
            };
            Some(IrTerm::Ctor {
                name: name.to_string(),
                args: vec![inner],
            })
        }
        _ => None,
    }
}

fn bin_op_to_predicate_name(op: &BinOp) -> Option<&'static str> {
    match op {
        BinOp::Eq(_) => Some("="),
        BinOp::Ne(_) => Some("≠"),
        BinOp::Lt(_) => Some("<"),
        BinOp::Le(_) => Some("≤"),
        BinOp::Gt(_) => Some(">"),
        BinOp::Ge(_) => Some("≥"),
        _ => None,
    }
}

fn block_only_panics(block: &syn::Block) -> bool {
    if block.stmts.len() != 1 {
        return false;
    }
    let stmt = &block.stmts[0];
    let mac: &Macro = match stmt {
        Stmt::Expr(Expr::Macro(ExprMacro { mac, .. }), _) => mac,
        Stmt::Macro(StmtMacro { mac, .. }) => mac,
        _ => return false,
    };
    mac.path
        .segments
        .last()
        .map(|s| s.ident == "panic")
        .unwrap_or(false)
}

fn negate(f: IrFormula) -> IrFormula {
    // Comparison flips: `if x < 10 panic` lifts to `x ≥ 10`, not `¬(x < 10)`.
    if let IrFormula::Atomic { name, args } = &f {
        let flipped = match name.as_str() {
            "<" => Some("≥"),
            "≤" => Some(">"),
            ">" => Some("≤"),
            "≥" => Some("<"),
            "=" => Some("≠"),
            "≠" => Some("="),
            _ => None,
        };
        if let Some(new_name) = flipped {
            return IrFormula::Atomic {
                name: new_name.to_string(),
                args: args.clone(),
            };
        }
    }
    // De Morgan's laws: push negation inward.
    //   ¬(a ∧ b) → ¬a ∨ ¬b
    //   ¬(a ∨ b) → ¬a ∧ ¬b
    // Sir's "every else is the contraposition" — when the lifter
    // produces the contraposition of `a && b` for an if-then-panic,
    // the result is `¬a ∨ ¬b`, not the harder-to-discharge `¬(a ∧ b)`.
    match f {
        IrFormula::And { operands } => IrFormula::Or {
            operands: operands.into_iter().map(negate).collect(),
        },
        IrFormula::Or { operands } => IrFormula::And {
            operands: operands.into_iter().map(negate).collect(),
        },
        IrFormula::Not { mut operands } if operands.len() == 1 => {
            // Double-negation elimination: ¬¬a → a.
            operands.pop().unwrap()
        }
        other => IrFormula::Not {
            operands: vec![other],
        },
    }
}

fn simplify_conjunction(parts: Vec<IrFormula>) -> IrFormula {
    if parts.is_empty() {
        IrFormula::Atomic {
            name: "true".to_string(),
            args: vec![],
        }
    } else if parts.len() == 1 {
        parts.into_iter().next().unwrap()
    } else {
        IrFormula::And { operands: parts }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wp::{atomic_ge, atomic_true, const_int, var};

    fn parse_fn(src: &str) -> ItemFn {
        let file: syn::File = syn::parse_str(src).expect("parses");
        file.items
            .into_iter()
            .find_map(|item| match item {
                syn::Item::Fn(f) => Some(f),
                _ => None,
            })
            .expect("function present")
    }

    #[test]
    fn lifts_if_then_panic_as_negated_condition() {
        let item_fn = parse_fn(
            r#"
            fn f(x: u32) -> u32 {
                if x < 10 {
                    panic!("x must be >= 10");
                }
                x * 2
            }
        "#,
        );
        let pre = lift_function_precondition(&item_fn);
        // ¬(x < 10) simplifies to x ≥ 10 via negate's comparison flip.
        assert_eq!(
            pre.as_formula(),
            atomic_ge(var("x"), const_int(10)).as_formula()
        );
    }

    #[test]
    fn lifts_assert_macro_as_predicate() {
        let item_fn = parse_fn(
            r#"
            fn g(x: u32) -> u32 {
                assert!(x >= 5);
                x * 3
            }
        "#,
        );
        let pre = lift_function_precondition(&item_fn);
        assert_eq!(
            pre.as_formula(),
            atomic_ge(var("x"), const_int(5)).as_formula()
        );
    }

    #[test]
    fn empty_body_lifts_to_vacuous_true() {
        let item_fn = parse_fn(r#"fn h() {}"#);
        let pre = lift_function_precondition(&item_fn);
        assert_eq!(pre.as_formula(), atomic_true().as_formula());
    }

    #[test]
    fn function_without_preconditions_lifts_to_true() {
        let item_fn = parse_fn(
            r#"
            fn k(x: u32) -> u32 {
                x + 1
            }
        "#,
        );
        let pre = lift_function_precondition(&item_fn);
        assert_eq!(pre.as_formula(), atomic_true().as_formula());
    }

    #[test]
    fn multiple_assertions_conjoin() {
        let item_fn = parse_fn(
            r#"
            fn m(x: u32, y: u32) -> u32 {
                assert!(x >= 1);
                assert!(y >= 2);
                x + y
            }
        "#,
        );
        let pre = lift_function_precondition(&item_fn);
        let expected = IrFormula::And {
            operands: vec![
                atomic_ge(var("x"), const_int(1)).into_formula(),
                atomic_ge(var("y"), const_int(2)).into_formula(),
            ],
        };
        assert_eq!(pre.as_formula(), &expected);
    }

    #[test]
    fn postcondition_derives_return_value_relation() {
        // f's body: `if x < 10 panic; x * 2`.
        // Derived post: `(x ≥ 10) ∧ (result = x * 2)`.
        // The first conjunct is the contraposition lifted from the
        // if-then-panic. The second is derived from the trailing
        // return expression `x * 2`.
        let item_fn = parse_fn(
            r#"
            fn f(x: u32) -> u32 {
                if x < 10 { panic!(); }
                x * 2
            }
        "#,
        );
        let post = lift_function_postcondition(&item_fn);
        let json = serde_json::to_string(post.as_formula()).unwrap();
        assert!(
            json.contains("\"≥\""),
            "post should include x ≥ 10: {}",
            json
        );
        // The return-value derivation: result = x * 2.
        assert!(
            json.contains("\"result\""),
            "post should include `result` variable: {}",
            json
        );
        assert!(
            json.contains("\"*\""),
            "post should include the multiplication ctor: {}",
            json
        );
    }

    #[test]
    fn binary_ops_lift_to_ctor_terms() {
        // `x + 5` lifts to Ctor("+", [Var("x"), Const(5)]).
        let item_fn = parse_fn(
            r#"
            fn k(x: u32) -> u32 {
                x + 5
            }
        "#,
        );
        let post = lift_function_postcondition(&item_fn);
        let json = serde_json::to_string(post.as_formula()).unwrap();
        assert!(
            json.contains("\"+\""),
            "post should encode the + ctor: {}",
            json
        );
        assert!(json.contains("\"x\""));
    }

    #[test]
    fn postcondition_for_tail_if_expression_is_branch_sensitive() {
        let item_fn = parse_fn(
            r#"
            fn foo(x: i32) -> i32 {
                if x == 0 { -22 } else { x }
            }
        "#,
        );
        let post = lift_function_postcondition(&item_fn).into_formula();
        let expected = IrFormula::Atomic {
            name: "=".to_string(),
            args: vec![
                IrTerm::Var {
                    name: "result".to_string(),
                },
                IrTerm::Ctor {
                    name: "ite".to_string(),
                    args: vec![
                        IrTerm::Ctor {
                            name: "=".to_string(),
                            args: vec![var("x"), const_int(0)],
                        },
                        const_int(-22),
                        var("x"),
                    ],
                },
            ],
        };
        assert_eq!(post, expected);
    }

    #[test]
    fn lifts_or_condition_with_de_morgan() {
        // `if x < 10 || y < 5 panic` lifts to ¬(x<10 ∨ y<5)
        // which simplifies via De Morgan to (x≥10 ∧ y≥5).
        let item_fn = parse_fn(
            r#"
            fn h(x: u32, y: u32) -> u32 {
                if x < 10 || y < 5 {
                    panic!();
                }
                x * y
            }
        "#,
        );
        let pre = lift_function_precondition(&item_fn);
        let json = serde_json::to_string(pre.as_formula()).unwrap();
        // Expect an `and` of two `≥` atoms (De Morgan applied + comparison flips).
        assert!(
            json.contains("\"and\""),
            "pre should be a conjunction: {}",
            json
        );
        assert!(
            json.contains("\"≥\""),
            "pre should contain ≥ atoms: {}",
            json
        );
        assert!(json.contains("\"x\"") && json.contains("\"y\""));
    }

    #[test]
    fn lifts_and_condition_with_de_morgan() {
        // `if x < 10 && y < 5 panic` lifts to ¬(x<10 ∧ y<5)
        // which is (x≥10 ∨ y≥5).
        let item_fn = parse_fn(
            r#"
            fn h(x: u32, y: u32) -> u32 {
                if x < 10 && y < 5 {
                    panic!();
                }
                x + y
            }
        "#,
        );
        let pre = lift_function_precondition(&item_fn);
        let json = serde_json::to_string(pre.as_formula()).unwrap();
        // Expect an `or` of two `≥` atoms.
        assert!(
            json.contains("\"or\""),
            "pre should be a disjunction: {}",
            json
        );
        assert!(
            json.contains("\"≥\""),
            "pre should contain ≥ atoms: {}",
            json
        );
    }

    #[test]
    fn double_negation_eliminated() {
        // `if !(x >= 10) panic` is equivalent to `if x < 10 panic`.
        // The lifter should produce `x ≥ 10` (not `¬¬(x ≥ 10)`).
        let item_fn = parse_fn(
            r#"
            fn n(x: u32) -> u32 {
                if !(x >= 10) {
                    panic!();
                }
                x * 2
            }
        "#,
        );
        let pre = lift_function_precondition(&item_fn);
        let json = serde_json::to_string(pre.as_formula()).unwrap();
        // The atomic ≥ should appear directly, with no `not` wrapper.
        assert!(
            json.contains("\"≥\""),
            "pre should contain ≥ atom: {}",
            json
        );
        assert!(
            !json.contains("\"not\""),
            "pre should NOT contain a `not` wrapper (double negation eliminated): {}",
            json
        );
    }

    // ---- shadow-AST scope tracking ----

    #[test]
    fn lift_closure_assigns_unique_param_id() {
        // |x| x  ->  Lambda { x#0, body: Var x#0 }
        let expr: Expr = syn::parse_str("|x| x").unwrap();
        let term = lift_expr_to_term(&expr).unwrap();
        match term {
            IrTerm::Lambda {
                param_name, body, ..
            } => {
                assert!(
                    param_name.starts_with("x#"),
                    "expected x#N, got {}",
                    param_name
                );
                match *body {
                    IrTerm::Var { name } => assert_eq!(
                        name, param_name,
                        "body's `x` must resolve to the closure's unique id"
                    ),
                    other => panic!("expected Var, got {:?}", other),
                }
            }
            other => panic!("expected Lambda, got {:?}", other),
        }
    }

    #[test]
    fn lift_nested_closures_get_distinct_ids() {
        // |x| |x| x  ->  the inner `x` shadows the outer; the inner's
        // unique id is what the body resolves to.
        let expr: Expr = syn::parse_str("|x| |x| x").unwrap();
        let term = lift_expr_to_term(&expr).unwrap();
        match term {
            IrTerm::Lambda {
                param_name: outer,
                body,
                ..
            } => match *body {
                IrTerm::Lambda {
                    param_name: inner,
                    body: inner_body,
                    ..
                } => {
                    assert_ne!(outer, inner, "outer and inner ids must differ");
                    assert!(outer.starts_with("x#"));
                    assert!(inner.starts_with("x#"));
                    match *inner_body {
                        IrTerm::Var { name } => {
                            assert_eq!(name, inner, "innermost binding wins");
                        }
                        other => panic!("expected Var, got {:?}", other),
                    }
                }
                other => panic!("expected nested Lambda, got {:?}", other),
            },
            other => panic!("expected Lambda, got {:?}", other),
        }
    }

    #[test]
    fn lift_free_variable_keeps_original_name() {
        // Bare `y` lifts to Var("y") — y is free in this context.
        let expr: Expr = syn::parse_str("y").unwrap();
        let term = lift_expr_to_term(&expr).unwrap();
        match term {
            IrTerm::Var { name } => {
                assert_eq!(name, "y", "free variable keeps surface name");
            }
            other => panic!("expected Var, got {:?}", other),
        }
    }

    #[test]
    fn lift_closure_does_not_capture_outer_reference() {
        // |x| (x + y)  -- inside the closure, x is bound, y is free.
        let expr: Expr = syn::parse_str("|x| x + y").unwrap();
        let term = lift_expr_to_term(&expr).unwrap();
        match term {
            IrTerm::Lambda {
                param_name, body, ..
            } => {
                assert!(param_name.starts_with("x#"));
                let json = serde_json::to_string(&body).unwrap();
                // Body should reference the unique x#N for x and bare "y" for y.
                assert!(
                    json.contains(&format!("\"{}\"", param_name)),
                    "body should reference the unique x: {}",
                    json
                );
                assert!(
                    json.contains("\"y\""),
                    "body should reference free y unchanged: {}",
                    json
                );
            }
            other => panic!("expected Lambda, got {:?}", other),
        }
    }

    #[test]
    fn lift_function_with_closure_keeps_formal_unscoped() {
        // fn f(x) { let _ = |x| x; x }
        // The trailing `x` is the function's formal — should be plain "x".
        // The closure's body `x` is the closure's param — should be "x#N".
        let item_fn = parse_fn(
            r#"
            fn f(x: u32) -> u32 {
                let _ = |x: u32| x;
                x
            }
        "#,
        );
        let post = lift_function_postcondition(&item_fn);
        let json = serde_json::to_string(post.as_formula()).unwrap();
        // The trailing-expression derivation gives `result = x` (plain x).
        assert!(
            json.contains("\"result\""),
            "post should derive result = x: {}",
            json
        );
        // The plain "x" formal (not "x#N") should appear.
        assert!(
            json.contains("\"x\""),
            "post should reference the formal x: {}",
            json
        );
    }

    // ---- bug-fix regression tests ----

    #[test]
    fn call_expr_body_lifts_call_tree_into_postcondition() {
        // A function whose body is a nested call must derive a post that
        // CONTAINS the call tree as ctor terms. Otherwise the callees are
        // invisible to `enumerate_callsites` and the missing-edge seam can
        // never be discharged (it was the false-green hole: this collapsed to
        // a vacuous `true` because `Expr::Call` had no lift arm).
        let item_fn = parse_fn(
            r#"
            fn address_of(value: i64) -> i64 {
                content_address(serialize(value))
            }
        "#,
        );
        let post = lift_function_postcondition(&item_fn);
        let json = serde_json::to_string(post.as_formula()).unwrap();
        assert!(
            json.contains("\"result\""),
            "post must derive result = <body>: {json}"
        );
        assert!(
            json.contains("content_address"),
            "post must contain the outer call ctor `content_address`: {json}"
        );
        assert!(
            json.contains("serialize"),
            "post must contain the nested call ctor `serialize`: {json}"
        );
    }

    #[test]
    fn debug_assert_not_lifted_to_postcondition() {
        // Bug #5: `debug_assert!` is compiled out in release builds.
        // It must NOT contribute to the postcondition.
        let item_fn = parse_fn(
            r#"
            fn f(x: u32) -> u32 {
                debug_assert!(x >= 5);
                x * 2
            }
        "#,
        );
        let post = lift_function_postcondition(&item_fn);
        let json = serde_json::to_string(post.as_formula()).unwrap();
        // The postcondition should NOT include the debug_assert predicate.
        // It should only have the trailing-expression derivation.
        assert!(
            !json.contains("\"≥\""),
            "debug_assert! must NOT appear in postcondition: {}",
            json
        );
        // The trailing `x * 2` should still derive a result postcondition.
        assert!(
            json.contains("\"result\""),
            "postcondition should still include result = x * 2: {}",
            json
        );
    }

    #[test]
    fn assert_shadowed_by_later_let_dropped_from_postcondition() {
        // Bug #6: `assert!(x >= 5); let x = 0; x` — the assert refers to
        // the original `x`, but `let x = 0` rebinds `x` afterward.
        // The assert is UNSOUND in the postcondition and must be dropped.
        let item_fn = parse_fn(
            r#"
            fn f(x: u32) -> u32 {
                assert!(x >= 5);
                let x = 0u32;
                x
            }
        "#,
        );
        let post = lift_function_postcondition(&item_fn);
        let json = serde_json::to_string(post.as_formula()).unwrap();
        // The original assert!(x >= 5) is unsound after let x = 0.
        // It must NOT appear in the postcondition.
        assert!(
            !json.contains("\"≥\""),
            "shadowed assert! must NOT appear in postcondition: {}",
            json
        );
    }

    #[test]
    fn assert_not_shadowed_stays_in_postcondition() {
        // Bug #6 complementary: when no later `let` shadows the assert's
        // free variables, the assert correctly stays in the postcondition.
        let item_fn = parse_fn(
            r#"
            fn f(x: u32, y: u32) -> u32 {
                assert!(x >= 5);
                let z = 0u32;   // shadows `z`, not `x`
                x + y
            }
        "#,
        );
        let post = lift_function_postcondition(&item_fn);
        let json = serde_json::to_string(post.as_formula()).unwrap();
        // `x` is NOT rebound; the assert should remain.
        assert!(
            json.contains("\"≥\""),
            "non-shadowed assert! should remain in postcondition: {}",
            json
        );
    }

    #[test]
    fn explicit_return_derives_result_postcondition() {
        // Bug #7: `fn f() -> i32 { return x + 1; }` must derive
        // `result = x + 1` in the postcondition.
        let item_fn = parse_fn(
            r#"
            fn f(x: i32) -> i32 {
                return x + 1;
            }
        "#,
        );
        let post = lift_function_postcondition(&item_fn);
        let json = serde_json::to_string(post.as_formula()).unwrap();
        assert!(
            json.contains("\"result\""),
            "explicit return expr must derive result = ...: {}",
            json
        );
        assert!(
            json.contains("\"+\""),
            "explicit return expr must include the + ctor: {}",
            json
        );
        assert!(
            json.contains("\"x\""),
            "explicit return expr must reference x: {}",
            json
        );
    }

    #[test]
    fn lifts_is_none_method_call_as_atomic_predicate() {
        // `if x.is_none() { panic!() }` lifts to is_none(x) at the
        // precondition (via the if-then-panic path: ¬panic_cond = ¬is_none(x)).
        // This is the shape the dropper emits for the Defensive template.
        let item_fn = parse_fn(
            r#"
            fn caller(x: Option<i32>) {
                if x.is_none() { panic!("not_null: x must be Some"); }
            }
        "#,
        );
        let pre = lift_function_precondition(&item_fn);
        let json = serde_json::to_string(pre.as_formula()).unwrap();
        // The if-then-panic lifts ¬is_none(x) as the precondition.
        // The JSON should contain "is_none" to confirm the method-call lift fired.
        assert!(
            json.contains("is_none"),
            "if x.is_none() panic must lift is_none to precondition: {}",
            json
        );
    }

    #[test]
    fn lifts_is_some_method_call_as_atomic_predicate() {
        // `if !x.is_some() { panic!() }` (double-negation via De Morgan)
        // should also lift correctly.
        let item_fn = parse_fn(
            r#"
            fn caller(x: Option<i32>) {
                if !x.is_some() { panic!("not_null: x must be Some"); }
            }
        "#,
        );
        let pre = lift_function_precondition(&item_fn);
        let json = serde_json::to_string(pre.as_formula()).unwrap();
        assert!(
            json.contains("is_some"),
            "if !x.is_some() panic must lift is_some to precondition: {}",
            json
        );
    }
}
