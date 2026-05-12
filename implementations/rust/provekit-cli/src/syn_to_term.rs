// SPDX-License-Identifier: Apache-2.0
//
// Wave-C: minimal `syn::ItemFn` → `libprovekit::core::Term` converter.
//
// Closes the `bind-stub-body-emitted` transport gap (PR #731) for the slice
// of Rust expressions used by the 11 trinity-roundtrip concepts (and any
// other source-body shapes that only use the same constructs).
//
// Supported slice (intentionally narrow — Supra omnia rectum):
//   - Path expression `name`              → Var
//   - Integer literal `42`, `-3`           → Const(Int)
//   - Bool literal `true`/`false`         → Const(Bool)
//   - Binary ops (+, -, *, /, %, ==, !=, <, <=, >, >=, &&, ||)
//   - Unary ops (-, !)
//   - Index `arr[i]`                       → Op("array-subscript", …)
//   - Method receiver-less calls (e.g. `xs.is_empty()`) for the small subset
//     used by the fixture
//   - Block expression                     → Op("seq", …)
//   - If/Else expression                   → Op("if", cond, then, else)
//   - Let statement                        → Op("decl", var, value)
//   - Return statement                     → Op("return", value)
//   - Trailing expression                  → implicit return
//   - Tuple literal `(a, b)`               → Op("tuple", …) (concept:pair shape)
//   - While loop                           → Op("while", cond, body)
//   - For loop over slice                  → Op("for", var, iter, body)
//   - Compound assignment `acc += v`       → Op("assign", var, add(var, v))
//   - Macro `panic!(…)`                    → Op("panic", …)
//
// Anything outside this slice yields `Ok(None)` (or `Err` for a hard refusal),
// causing the canonical-rewrite path to fall back to the language-idiomatic
// stub body just as before — preserving the v0 contract for unsupported shapes.

use libprovekit::core::{Cid, Term};
use provekit_canonicalizer::blake3_512_of;
use provekit_ir_types::Sort;
use serde_json::json;

/// Build a deterministic op CID for a wave-c synthetic Op name.
fn op_cid_for(name: &str) -> Cid {
    let h = blake3_512_of(format!("provekit-cli/wave-c/syn-to-term:{name}").as_bytes());
    // h is exactly the `blake3-512:<128 hex>` self-identifying string.
    Cid::parse(h).expect("blake3_512_of always returns a valid Cid")
}

fn op(name: &str, args: Vec<Term>) -> Term {
    Term::Op {
        op_cid: op_cid_for(name),
        name: format!("rust:{name}"),
        args,
    }
}

fn int_const(value: i64) -> Term {
    Term::Const {
        value: json!(value),
        sort: Sort::Primitive {
            name: "Int".into(),
        },
    }
}

fn bool_const(value: bool) -> Term {
    Term::Const {
        value: json!(value),
        sort: Sort::Primitive {
            name: "Bool".into(),
        },
    }
}

/// Convert the body of a Rust function into a `Term`, or return `None` if the
/// body uses constructs outside the wave-c slice. The trailing expression is
/// transformed into `return` form so the realizer can emit statement-style
/// target languages (Java, Go, C#) directly.
pub(crate) fn lift_fn_body(item_fn: &syn::ItemFn) -> Option<Term> {
    // A unit-returning fn (`-> ()` or no return type) emits an empty body;
    // the realizer maps an empty-body Term to the language's idiomatic
    // empty function. Mark it explicitly so unit fns don't auto-return Unit.
    let returns_unit = matches!(
        item_fn.sig.output,
        syn::ReturnType::Default
    );
    if returns_unit {
        // For unit-return, the tail expression has no value to return.
        // Lift normally and add a trailing `return` of Unit so the realizer
        // does not try to emit a value where the signature has none.
        let body = lift_block(&item_fn.block)?;
        // If the lifted body is already terminal, leave it.
        if matches!(&body, Term::Op { name, .. } if {
            let n = local_op(name); matches!(n, "return" | "panic" | "break" | "continue")
        }) {
            Some(body)
        } else {
            Some(op("seq", vec![body, op("return", vec![Term::Unit])]))
        }
    } else {
        lift_block_tail(&item_fn.block)
    }
}

fn lift_block(block: &syn::Block) -> Option<Term> {
    lift_block_with(block, false)
}

fn lift_block_tail(block: &syn::Block) -> Option<Term> {
    lift_block_with(block, true)
}

fn lift_block_with(block: &syn::Block, tail_returns: bool) -> Option<Term> {
    let mut stmts: Vec<Term> = Vec::new();
    let mut iter = block.stmts.iter().peekable();
    while let Some(stmt) = iter.next() {
        let is_last = iter.peek().is_none();
        // The function-body tail return is only applied at the outermost
        // `tail_returns` boundary, recursively pushed through `if`/`block`
        // branches. Non-tail blocks NEVER auto-return.
        let term = lift_stmt(stmt, is_last && tail_returns)?;
        stmts.push(term);
    }
    Some(sequence(stmts))
}

fn sequence(stmts: Vec<Term>) -> Term {
    let mut iter = stmts.into_iter();
    let mut acc = match iter.next() {
        Some(t) => t,
        None => return op("skip", vec![]),
    };
    for next in iter {
        acc = op("seq", vec![acc, next]);
    }
    acc
}

fn lift_stmt(stmt: &syn::Stmt, tail_returns: bool) -> Option<Term> {
    match stmt {
        syn::Stmt::Local(local) => lift_local(local),
        syn::Stmt::Expr(expr, semi) => {
            // A trailing expression with no semicolon is an implicit return
            // (matches the trinity fixture shape for short functions). Push
            // the return into branch leaves so an `if`/`block` tail becomes
            // statement-style with `return` inside each branch, which is what
            // the cross-language realizer expects (Java has no expression-form
            // if/else returning a value).
            if tail_returns && semi.is_none() {
                lift_expr_returning(expr)
            } else {
                lift_expr(expr)
            }
        }
        syn::Stmt::Item(_) => None,
        syn::Stmt::Macro(m) => lift_macro_stmt(&m.mac),
    }
}

/// Lift an expression in tail position of a function or branch. Wraps in
/// `op("return", _)` UNLESS the expression is itself a control-flow construct
/// (if/block/match) whose own branches each carry the return — in which case
/// the `return` is pushed into the leaves, never wrapped around the construct.
fn lift_expr_returning(expr: &syn::Expr) -> Option<Term> {
    match expr {
        syn::Expr::If(e) => {
            let cond = lift_expr(&e.cond)?;
            let then_branch = lift_block_tail(&e.then_branch)?;
            let else_branch = if let Some((_, else_expr)) = &e.else_branch {
                lift_expr_returning(else_expr)?
            } else {
                op("skip", vec![])
            };
            Some(op("if", vec![cond, then_branch, else_branch]))
        }
        syn::Expr::Block(b) => lift_block_tail(&b.block),
        syn::Expr::Paren(p) => lift_expr_returning(&p.expr),
        syn::Expr::Return(_) | syn::Expr::Break(_) | syn::Expr::Continue(_) => lift_expr(expr),
        // Macro tails like `panic!()` already terminate; no return wrap needed.
        syn::Expr::Macro(m) => {
            let term = lift_macro_expr(&m.mac)?;
            if is_terminal_term(&term) {
                Some(term)
            } else {
                Some(op("return", vec![term]))
            }
        }
        // Everything else is a value-producing expression; wrap in return.
        _ => {
            let inner = lift_expr(expr)?;
            Some(op("return", vec![inner]))
        }
    }
}

fn is_terminal_term(term: &Term) -> bool {
    matches!(term, Term::Op { name, .. } if {
        let n = local_op(name);
        matches!(n, "return" | "break" | "continue" | "panic")
    })
}

fn local_op(name: &str) -> &str {
    name.split_once(':').map(|(_, l)| l).unwrap_or(name)
}

fn lift_local(local: &syn::Local) -> Option<Term> {
    // `let NAME [: TY] = EXPR;` (mutability/patterns flattened to a Var lvalue)
    let var = match &local.pat {
        syn::Pat::Ident(p) => Term::Var {
            name: p.ident.to_string(),
        },
        // `let mut x: i64 = …;`  → also a Pat::Ident inside the syn AST, handled above.
        // Any other pat (tuples, destructuring) is out of slice.
        _ => return None,
    };
    let init = local.init.as_ref()?;
    let value = lift_expr(&init.expr)?;
    Some(op("decl", vec![var, value]))
}

fn lift_expr(expr: &syn::Expr) -> Option<Term> {
    match expr {
        syn::Expr::Path(p) => {
            let ident = p.path.get_ident()?;
            match ident.to_string().as_str() {
                "true" => Some(bool_const(true)),
                "false" => Some(bool_const(false)),
                name => Some(Term::Var {
                    name: name.to_string(),
                }),
            }
        }
        syn::Expr::Lit(l) => lift_lit(&l.lit),
        syn::Expr::Binary(b) => {
            let lhs = lift_expr(&b.left)?;
            let rhs = lift_expr(&b.right)?;
            // Compound assignment (`acc += v`, `x *= 2`, …) desugars to
            // `acc = acc + v`. We only handle the lifted-friendly subset.
            if let Some(inner) = compound_assign_op(&b.op) {
                let combined = op(inner, vec![lhs.clone(), rhs]);
                return Some(op("assign", vec![lhs, combined]));
            }
            let name = bin_op_name(&b.op)?;
            Some(op(name, vec![lhs, rhs]))
        }
        syn::Expr::Unary(u) => {
            let arg = lift_expr(&u.expr)?;
            let name = match &u.op {
                syn::UnOp::Neg(_) => "neg",
                syn::UnOp::Not(_) => "not",
                _ => return None,
            };
            Some(op(name, vec![arg]))
        }
        syn::Expr::Paren(p) => lift_expr(&p.expr),
        syn::Expr::Block(b) => lift_block(&b.block),
        syn::Expr::If(e) => {
            let cond = lift_expr(&e.cond)?;
            let then_branch = lift_block(&e.then_branch)?;
            let else_branch = if let Some((_, else_expr)) = &e.else_branch {
                lift_expr(else_expr)?
            } else {
                op("skip", vec![])
            };
            Some(op("if", vec![cond, then_branch, else_branch]))
        }
        syn::Expr::Return(r) => {
            let inner = match &r.expr {
                Some(e) => lift_expr(e)?,
                None => Term::Unit,
            };
            Some(op("return", vec![inner]))
        }
        syn::Expr::Index(i) => {
            let receiver = lift_expr(&i.expr)?;
            let index = lift_expr(&i.index)?;
            Some(op("array-subscript", vec![receiver, index]))
        }
        syn::Expr::Tuple(t) => {
            let mut args: Vec<Term> = Vec::new();
            for elem in &t.elems {
                args.push(lift_expr(elem)?);
            }
            Some(op("tuple", args))
        }
        syn::Expr::MethodCall(mc) => {
            // We support the small set of receiver method calls used by the
            // trinity fixture (`.is_empty()`, `.len()`). Anything else opts out.
            let receiver = lift_expr(&mc.receiver)?;
            let method = mc.method.to_string();
            if !mc.args.is_empty() {
                return None;
            }
            match method.as_str() {
                "is_empty" => Some(op("is_empty", vec![receiver])),
                "len" => Some(op("len", vec![receiver])),
                _ => None,
            }
        }
        syn::Expr::While(w) => {
            let cond = lift_expr(&w.cond)?;
            let body = lift_block(&w.body)?;
            Some(op("while", vec![cond, body]))
        }
        syn::Expr::ForLoop(f) => {
            let var = match &*f.pat {
                syn::Pat::Ident(p) => Term::Var {
                    name: p.ident.to_string(),
                },
                // `for &v in items` is a Pat::Reference around Pat::Ident.
                syn::Pat::Reference(r) => match &*r.pat {
                    syn::Pat::Ident(p) => Term::Var {
                        name: p.ident.to_string(),
                    },
                    _ => return None,
                },
                _ => return None,
            };
            let iter = lift_expr(&f.expr)?;
            let body = lift_block(&f.body)?;
            Some(op("for", vec![var, iter, body]))
        }
        syn::Expr::Assign(a) => {
            let lhs = lift_expr(&a.left)?;
            let rhs = lift_expr(&a.right)?;
            Some(op("assign", vec![lhs, rhs]))
        }
        syn::Expr::Reference(r) => {
            // `&items` etc. — emit the inner expression; references are not
            // semantically tracked at this level.
            lift_expr(&r.expr)
        }
        syn::Expr::Macro(m) => lift_macro_expr(&m.mac),
        syn::Expr::Break(_) => Some(op("break", vec![])),
        syn::Expr::Continue(_) => Some(op("continue", vec![])),
        _ => None,
    }
}

fn lift_lit(lit: &syn::Lit) -> Option<Term> {
    match lit {
        syn::Lit::Int(i) => {
            let v: i64 = i.base10_parse().ok()?;
            Some(int_const(v))
        }
        syn::Lit::Bool(b) => Some(bool_const(b.value)),
        _ => None,
    }
}

fn compound_assign_op(op: &syn::BinOp) -> Option<&'static str> {
    Some(match op {
        syn::BinOp::AddAssign(_) => "add",
        syn::BinOp::SubAssign(_) => "sub",
        syn::BinOp::MulAssign(_) => "mul",
        syn::BinOp::DivAssign(_) => "div",
        syn::BinOp::RemAssign(_) => "mod",
        syn::BinOp::BitAndAssign(_) => "bitand",
        syn::BinOp::BitOrAssign(_) => "bitor",
        syn::BinOp::BitXorAssign(_) => "bitxor",
        _ => return None,
    })
}

fn bin_op_name(op: &syn::BinOp) -> Option<&'static str> {
    Some(match op {
        syn::BinOp::Add(_) => "add",
        syn::BinOp::Sub(_) => "sub",
        syn::BinOp::Mul(_) => "mul",
        syn::BinOp::Div(_) => "div",
        syn::BinOp::Rem(_) => "mod",
        syn::BinOp::Eq(_) => "eq",
        syn::BinOp::Ne(_) => "ne",
        syn::BinOp::Lt(_) => "lt",
        syn::BinOp::Le(_) => "le",
        syn::BinOp::Gt(_) => "gt",
        syn::BinOp::Ge(_) => "ge",
        syn::BinOp::And(_) => "and",
        syn::BinOp::Or(_) => "or",
        syn::BinOp::BitAnd(_) => "bitand",
        syn::BinOp::BitOr(_) => "bitor",
        syn::BinOp::BitXor(_) => "bitxor",
        // Compound assignment is desugared by the caller for `Expr::Assign`,
        // but `acc += v` arrives as `syn::Expr::Binary` with `op = AddAssign`.
        // We desugar at the Binary level into `acc = acc + v` if needed.
        _ => return None,
    })
}

// Macro statements: `panic!(...)`, `unimplemented!(...)`, etc.
fn lift_macro_stmt(mac: &syn::Macro) -> Option<Term> {
    lift_macro_expr(mac)
}

fn lift_macro_expr(mac: &syn::Macro) -> Option<Term> {
    let path = mac.path.segments.iter().last()?;
    match path.ident.to_string().as_str() {
        "panic" => Some(op("panic", vec![])),
        "unimplemented" | "todo" | "unreachable" => Some(op("panic", vec![])),
        _ => None,
    }
}
