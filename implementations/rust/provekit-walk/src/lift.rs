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

use provekit_ir_types::{IrFormula, IrTerm};
use syn::{
    BinOp, Expr, ExprBinary, ExprIf, ExprMacro, ExprUnary, ItemFn, Lit, Macro, Stmt, StmtMacro,
    UnOp,
};

use crate::wp::Wp;

/// Lift the implicit precondition from a function body. Walks every
/// statement and conjoins the contribution of each pattern recognized.
///
/// Returns `Wp(true)` if no patterns are recognized — this means the
/// function makes no demands on its caller (a vacuous precondition).
pub fn lift_function_precondition(item_fn: &ItemFn) -> Wp {
    let mut accum: Vec<IrFormula> = Vec::new();
    for stmt in &item_fn.block.stmts {
        if let Some(predicate) = lift_stmt_contribution(stmt) {
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
    let mut accum: Vec<IrFormula> = Vec::new();

    // 1. Same control-flow contributions as the precondition: every
    //    if-then-panic / assert! / etc. that holds at function entry
    //    also holds at every return point in the MVP's recognized
    //    patterns (none of them mutate input variables).
    for stmt in &item_fn.block.stmts {
        if let Some(predicate) = lift_stmt_contribution(stmt) {
            accum.push(predicate);
        }
    }

    // 2. Trailing-expression derivation: if the function body ends with
    //    an expression statement (no trailing semicolon), that
    //    expression is the function's return value. Derive
    //    `result = <lifted expression>` and add to the postcondition.
    if let Some(Stmt::Expr(e, None)) = item_fn.block.stmts.last() {
        if let Some(term) = lift_expr_to_term(e) {
            let result_var = IrTerm::Var {
                name: "result".to_string(),
            };
            accum.push(IrFormula::Atomic {
                name: "=".to_string(),
                args: vec![result_var, term],
            });
        }
    }

    Wp(simplify_conjunction(accum))
}

/// What does this single statement contribute to the function's
/// implicit precondition? Returns None for statements that don't lift
/// (let-bindings, plain expressions, etc.).
fn lift_stmt_contribution(stmt: &Stmt) -> Option<IrFormula> {
    match stmt {
        Stmt::Expr(e, _) => lift_expr_contribution(e),
        // `assert!(c);` at statement position parses to Stmt::Macro
        // (with optional trailing semicolon), not Stmt::Expr(Expr::Macro).
        Stmt::Macro(StmtMacro { mac, .. }) => lift_macro_contribution(mac),
        _ => None,
    }
}

/// Recognize and lift macro contributions at statement or expression
/// position. Used by both `Stmt::Macro` and `Expr::Macro` paths.
fn lift_macro_contribution(mac: &Macro) -> Option<IrFormula> {
    let seg = mac.path.segments.last()?;
    let name = seg.ident.to_string();
    match name.as_str() {
        "assert" | "debug_assert" => {
            let parsed_cond = syn::parse2::<Expr>(mac.tokens.clone()).ok()?;
            // assert!(c) parses to just c. assert!(c, "msg") parses
            // as a tuple-expr; take the first elem.
            let first = match &parsed_cond {
                Expr::Tuple(t) => t.elems.first()?,
                other => other,
            };
            lift_predicate(first)
        }
        _ => None,
    }
}

fn lift_expr_contribution(expr: &Expr) -> Option<IrFormula> {
    // if-then-panic pattern: `if cond { panic!() }` lifts to ¬cond.
    if let Expr::If(ExprIf {
        cond,
        then_branch,
        else_branch,
        ..
    }) = expr
    {
        if else_branch.is_none() && block_only_panics(then_branch) {
            let cond_formula = lift_predicate(cond)?;
            return Some(negate(cond_formula));
        }
    }
    // assert!()-shaped macros sometimes parse as Expr::Macro (e.g. when
    // they're the trailing tail expression of a block).
    if let Expr::Macro(ExprMacro { mac, .. }) = expr {
        if let Some(formula) = lift_macro_contribution(mac) {
            return Some(formula);
        }
    }
    None
}

/// Lift an arbitrary Rust predicate-shaped expression to `IrFormula`.
/// Returns None for shapes the MVP does not yet handle.
pub fn lift_predicate(expr: &Expr) -> Option<IrFormula> {
    match expr {
        Expr::Binary(ExprBinary {
            left, op, right, ..
        }) => match op {
            BinOp::And(_) => {
                let l = lift_predicate(left)?;
                let r = lift_predicate(right)?;
                Some(IrFormula::And { operands: vec![l, r] })
            }
            BinOp::Or(_) => {
                let l = lift_predicate(left)?;
                let r = lift_predicate(right)?;
                Some(IrFormula::Or { operands: vec![l, r] })
            }
            _ => {
                // Comparison: lift both sides as terms, pick the IR predicate name.
                let name = bin_op_to_predicate_name(op)?;
                let l_term = lift_expr_to_term(left)?;
                let r_term = lift_expr_to_term(right)?;
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
            let inner = lift_predicate(expr)?;
            Some(IrFormula::Not {
                operands: vec![inner],
            })
        }
        // Anything else is unrecognized in the MVP.
        _ => None,
    }
}

/// Lift a Rust expression to a canonical `IrTerm`. Supported shapes:
///   - Integer literal: `IrTerm::Const { value: <num>, sort: Int }`.
///   - Bare identifier: `IrTerm::Var { name: <ident> }`.
///   - Parenthesized expression: recurses on the inner expression.
///   - Binary arithmetic (`+`, `-`, `*`, `/`, `%`): lifts to
///     `IrTerm::Ctor { name: <op>, args: [lhs_term, rhs_term] }`.
///   - Unary negation (`-x`): `IrTerm::Ctor { name: "neg", args: [...] }`.
/// Anything else returns None.
pub fn lift_expr_to_term(expr: &Expr) -> Option<IrTerm> {
    match expr {
        Expr::Lit(lit) => match &lit.lit {
            Lit::Int(n) => match n.base10_parse::<i64>() {
                Ok(v) => Some(crate::wp::const_int(v)),
                Err(_) => None,
            },
            _ => None,
        },
        Expr::Path(syn::ExprPath { path, .. }) => {
            let seg = path.segments.last()?;
            Some(crate::wp::var(seg.ident.to_string()))
        }
        Expr::Paren(p) => lift_expr_to_term(&p.expr),
        Expr::Binary(ExprBinary {
            left, op, right, ..
        }) => {
            let op_name = match op {
                BinOp::Add(_) => "+",
                BinOp::Sub(_) => "-",
                BinOp::Mul(_) => "*",
                BinOp::Div(_) => "/",
                BinOp::Rem(_) => "%",
                _ => return None,
            };
            let l = lift_expr_to_term(left)?;
            let r = lift_expr_to_term(right)?;
            Some(IrTerm::Ctor {
                name: op_name.to_string(),
                args: vec![l, r],
            })
        }
        Expr::Unary(ExprUnary {
            op: UnOp::Neg(_),
            expr,
            ..
        }) => {
            let inner = lift_expr_to_term(expr)?;
            Some(IrTerm::Ctor {
                name: "neg".to_string(),
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
    // Special-case ¬(a < b) → a ≥ b, and the other comparison flips.
    // This produces the form Sir's bare demo expects: `if x < 10 panic`
    // lifts to `x ≥ 10`, not `¬(x < 10)`.
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
    IrFormula::Not { operands: vec![f] }
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
        assert!(json.contains("\"≥\""), "post should include x ≥ 10: {}", json);
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
        assert!(json.contains("\"+\""), "post should encode the + ctor: {}", json);
        assert!(json.contains("\"x\""));
    }
}
