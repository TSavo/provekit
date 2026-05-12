// SPDX-License-Identifier: Apache-2.0
//
// Canonical term-shape algebra.
//
// The shape is a tiny, deliberately-lossy projection of the function
// body's structural skeleton:
//
//   - control-flow constructs are preserved by kind (if / while / for /
//     break / return),
//   - relational ops are preserved by kind (>, <, >=, <=, ==, !=),
//   - arithmetic ops are preserved by kind (+, -, *, /, %, +=, -=),
//   - everything else (variable names, literal values, identifier
//     spelling, method-call sugar) is canonicalized to a single
//     `OPAQUE` leaf.
//
// The shape is then JCS-encoded and BLAKE3-512-hashed; the resulting
// CID is the cluster address. Two functions with different surface
// syntax but the same canonical shape compress to one concept-CID.
//
// This is intentionally smaller than the full provekit-ir-symbolic
// Term/Formula algebra so the smoke test's clustering output is
// readable in the report. The full algebra is what the production
// lifter (provekit-lift) emits; the smoke test demonstrates that
// shape clustering at any granularity yields the compression property.

use provekit_canonicalizer::{blake3_512_of, encode_jcs, Value};
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct TermShape {
    /// A canonical-by-construction tree. Equal trees -> equal CIDs.
    pub root: ShapeNode,
}

#[derive(Debug, Clone)]
pub enum ShapeNode {
    /// A function body summarized as a sequence of statement-shape nodes.
    Body(Vec<ShapeNode>),
    /// `if <cond_shape> { <then_shape> } else { <else_shape> }`.
    If {
        cond: Box<ShapeNode>,
        then_branch: Box<ShapeNode>,
        else_branch: Option<Box<ShapeNode>>,
    },
    /// `while <cond_shape> { <body_shape> }`.
    While {
        cond: Box<ShapeNode>,
        body: Box<ShapeNode>,
    },
    /// `for _ in <iter_shape> { <body_shape> }`.
    For { body: Box<ShapeNode> },
    /// Early exit (`return ..` / `break ..`).
    Exit,
    /// Assignment / mutation statement.
    Assign,
    /// Local `let` binding.
    Let,
    /// Relational predicate, generalized: `Rel { op }` where op is one of
    /// `>` / `<` / `>=` / `<=` / `==` / `!=`.
    Rel { op: String },
    /// Arithmetic op, generalized: `Bin { op }` where op is one of
    /// `+` / `-` / `*` / `/` / `%` / `+=` / `-=`.
    Bin { op: String },
    /// Method or function call; the callee identity is canonicalized to
    /// OPAQUE to preserve compression across different helper names.
    Call,
    /// Block expression: `{ <stmts> }`.
    Block(Vec<ShapeNode>),
    /// Literal or identifier whose value is irrelevant to the shape.
    Opaque,
}

impl TermShape {
    pub fn from_fn(item_fn: &syn::ItemFn) -> Self {
        let mut stmts = Vec::new();
        for stmt in &item_fn.block.stmts {
            stmts.push(shape_of_stmt(stmt));
        }
        TermShape {
            root: ShapeNode::Body(stmts),
        }
    }

    pub fn shape_cid(&self) -> String {
        let v = node_to_value(&self.root);
        blake3_512_of(encode_jcs(&v).as_bytes())
    }

    /// Pretty-print for the report.
    pub fn render(&self) -> String {
        render_node(&self.root, 0)
    }

    /// Cheap classifier: returns one of "retry-loop" / "guard-then-commit"
    /// / "option-default" / "unknown" based on the top-level skeleton.
    /// The seed catalog uses this for the soft-match second pass.
    pub fn classify(&self) -> &'static str {
        classify_node(&self.root)
    }
}

#[derive(Debug, Clone)]
pub struct FormulaShape {
    pub pretty: String,
}

impl FormulaShape {
    pub fn from_fn_body(item_fn: &syn::ItemFn) -> Self {
        // The smoke test does not lift Rust expressions into IrFormula
        // (that is provekit-lift's job). We carry a pretty form for
        // the report only.
        let n = item_fn.block.stmts.len();
        FormulaShape {
            pretty: format!("body[{}]", n),
        }
    }
}

fn shape_of_stmt(stmt: &syn::Stmt) -> ShapeNode {
    match stmt {
        syn::Stmt::Local(local) => {
            // A let with a Binary-op initializer is still a Let node at
            // the top level, but we expose the initializer's shape as
            // a wrapping Block so the classifier can see the arithmetic
            // expression. This keeps the classifier robust against
            // computed-then-committed patterns.
            if let Some(init) = &local.init {
                let init_shape = shape_of_expr(&init.expr);
                ShapeNode::Block(vec![ShapeNode::Let, init_shape])
            } else {
                ShapeNode::Let
            }
        }
        syn::Stmt::Expr(expr, _) => shape_of_expr(expr),
        syn::Stmt::Item(_) => ShapeNode::Opaque,
        syn::Stmt::Macro(_) => ShapeNode::Opaque,
    }
}

fn shape_of_expr(expr: &syn::Expr) -> ShapeNode {
    match expr {
        syn::Expr::If(e) => ShapeNode::If {
            cond: Box::new(shape_of_expr(&e.cond)),
            then_branch: Box::new(shape_of_block(&e.then_branch)),
            else_branch: e.else_branch.as_ref().map(|(_, b)| Box::new(shape_of_expr(b))),
        },
        syn::Expr::While(e) => ShapeNode::While {
            cond: Box::new(shape_of_expr(&e.cond)),
            body: Box::new(shape_of_block(&e.body)),
        },
        syn::Expr::ForLoop(e) => ShapeNode::For {
            body: Box::new(shape_of_block(&e.body)),
        },
        syn::Expr::Loop(e) => ShapeNode::While {
            cond: Box::new(ShapeNode::Opaque),
            body: Box::new(shape_of_block(&e.body)),
        },
        syn::Expr::Return(_) => ShapeNode::Exit,
        syn::Expr::Break(_) => ShapeNode::Exit,
        syn::Expr::Block(b) => shape_of_block(&b.block),
        syn::Expr::Binary(b) => {
            let op = binop_label(&b.op);
            if is_rel_op(&op) {
                ShapeNode::Rel { op }
            } else {
                ShapeNode::Bin { op }
            }
        }
        syn::Expr::Assign(_) => ShapeNode::Assign,
        syn::Expr::Call(_) => ShapeNode::Call,
        syn::Expr::MethodCall(_) => ShapeNode::Call,
        syn::Expr::Path(_) => ShapeNode::Opaque,
        syn::Expr::Lit(_) => ShapeNode::Opaque,
        _ => ShapeNode::Opaque,
    }
}

fn shape_of_block(block: &syn::Block) -> ShapeNode {
    let mut out = Vec::new();
    for stmt in &block.stmts {
        out.push(shape_of_stmt(stmt));
    }
    ShapeNode::Block(out)
}

fn binop_label(op: &syn::BinOp) -> String {
    match op {
        syn::BinOp::Add(_) => "+".into(),
        syn::BinOp::Sub(_) => "-".into(),
        syn::BinOp::Mul(_) => "*".into(),
        syn::BinOp::Div(_) => "/".into(),
        syn::BinOp::Rem(_) => "%".into(),
        syn::BinOp::Eq(_) => "==".into(),
        syn::BinOp::Ne(_) => "!=".into(),
        syn::BinOp::Lt(_) => "<".into(),
        syn::BinOp::Le(_) => "<=".into(),
        syn::BinOp::Gt(_) => ">".into(),
        syn::BinOp::Ge(_) => ">=".into(),
        syn::BinOp::AddAssign(_) => "+=".into(),
        syn::BinOp::SubAssign(_) => "-=".into(),
        _ => "OTHER".into(),
    }
}

fn is_rel_op(op: &str) -> bool {
    matches!(op, "==" | "!=" | "<" | "<=" | ">" | ">=")
}

fn node_to_value(n: &ShapeNode) -> Arc<Value> {
    match n {
        ShapeNode::Body(items) => Value::object([
            ("k", Value::string("Body")),
            ("items", Value::array(items.iter().map(node_to_value).collect())),
        ]),
        ShapeNode::If { cond, then_branch, else_branch } => {
            let mut kvs: Vec<(String, Arc<Value>)> = vec![
                ("k".into(), Value::string("If")),
                ("cond".into(), node_to_value(cond)),
                ("then".into(), node_to_value(then_branch)),
            ];
            if let Some(e) = else_branch {
                kvs.push(("else".into(), node_to_value(e)));
            }
            Arc::new(Value::Object(kvs))
        }
        ShapeNode::While { cond, body } => Value::object([
            ("k", Value::string("While")),
            ("cond", node_to_value(cond)),
            ("body", node_to_value(body)),
        ]),
        ShapeNode::For { body } => Value::object([
            ("k", Value::string("For")),
            ("body", node_to_value(body)),
        ]),
        ShapeNode::Exit => Value::object([("k", Value::string("Exit"))]),
        ShapeNode::Assign => Value::object([("k", Value::string("Assign"))]),
        ShapeNode::Let => Value::object([("k", Value::string("Let"))]),
        ShapeNode::Rel { op } => Value::object([
            ("k", Value::string("Rel")),
            ("op", Value::string(op.clone())),
        ]),
        ShapeNode::Bin { op } => Value::object([
            ("k", Value::string("Bin")),
            ("op", Value::string(op.clone())),
        ]),
        ShapeNode::Call => Value::object([("k", Value::string("Call"))]),
        ShapeNode::Block(items) => Value::object([
            ("k", Value::string("Block")),
            ("items", Value::array(items.iter().map(node_to_value).collect())),
        ]),
        ShapeNode::Opaque => Value::object([("k", Value::string("Opaque"))]),
    }
}

fn render_node(n: &ShapeNode, depth: usize) -> String {
    let pad = "  ".repeat(depth);
    match n {
        ShapeNode::Body(items) => {
            let inner: Vec<String> = items.iter().map(|n| render_node(n, depth + 1)).collect();
            format!("{}Body[\n{}\n{}]", pad, inner.join("\n"), pad)
        }
        ShapeNode::If { cond, then_branch, else_branch } => {
            let mut out = format!(
                "{}If({}) {{\n{}\n{}}}",
                pad,
                render_node(cond, 0).trim(),
                render_node(then_branch, depth + 1),
                pad
            );
            if let Some(e) = else_branch {
                out.push_str(&format!(
                    " else {{\n{}\n{}}}",
                    render_node(e, depth + 1),
                    pad
                ));
            }
            out
        }
        ShapeNode::While { cond, body } => format!(
            "{}While({}) {{\n{}\n{}}}",
            pad,
            render_node(cond, 0).trim(),
            render_node(body, depth + 1),
            pad
        ),
        ShapeNode::For { body } => format!(
            "{}For {{\n{}\n{}}}",
            pad,
            render_node(body, depth + 1),
            pad
        ),
        ShapeNode::Exit => format!("{}Exit", pad),
        ShapeNode::Assign => format!("{}Assign", pad),
        ShapeNode::Let => format!("{}Let", pad),
        ShapeNode::Rel { op } => format!("{}Rel({})", pad, op),
        ShapeNode::Bin { op } => format!("{}Bin({})", pad, op),
        ShapeNode::Call => format!("{}Call", pad),
        ShapeNode::Block(items) => {
            let inner: Vec<String> = items.iter().map(|n| render_node(n, depth + 1)).collect();
            format!("{}Block[\n{}\n{}]", pad, inner.join("\n"), pad)
        }
        ShapeNode::Opaque => format!("{}Opaque", pad),
    }
}

fn classify_node(n: &ShapeNode) -> &'static str {
    // Top-of-body classifier. Looks one level into the Body for a
    // dominant control-flow construct.
    //
    // The catalog only claims a name for shapes the substrate has been
    // told about. Shapes outside those families intentionally land at
    // "unknown" and surface as UNNAMED-CONCEPT-N for human naming.
    if let ShapeNode::Body(items) = n {
        let mut has_loop = false;
        let mut has_exit_in_loop = false;
        let mut has_guarded_mutation = false;
        let mut has_if_chain_only = false;
        let body_has_arith = items.iter().any(contains_assign);

        for it in items {
            match it {
                ShapeNode::While { body, .. } | ShapeNode::For { body, .. } => {
                    has_loop = true;
                    if contains_exit(body) {
                        has_exit_in_loop = true;
                    }
                }
                ShapeNode::Let => {
                    // Let bindings ahead of the loop don't change classification.
                }
                ShapeNode::If { then_branch, else_branch, .. } => {
                    if !has_loop {
                        // Distinguish a guarded-mutation if (one whose
                        // branches contain an Assign or arithmetic Bin
                        // expression, OR the overall body contains a
                        // computation step that the If is gating) from
                        // a pure if-chain that only selects between
                        // pre-existing values (saturating clamps,
                        // lookup-by-bound, etc).
                        let mutates_or_computes = contains_assign(then_branch)
                            || else_branch.as_deref().map(contains_assign).unwrap_or(false)
                            || body_has_arith;
                        if mutates_or_computes {
                            has_guarded_mutation = true;
                        } else {
                            has_if_chain_only = true;
                        }
                    }
                }
                ShapeNode::Exit => {}
                _ => {}
            }
        }
        if has_loop && has_exit_in_loop {
            return "retry-loop";
        }
        if has_loop {
            return "retry-loop";
        }
        if has_guarded_mutation {
            return "guard-then-commit";
        }
        if has_if_chain_only {
            // Deliberately UNNAMED so the smoke test can demonstrate
            // the naming round-trip on a non-trivial real cluster.
            return "unknown";
        }
    }
    "unknown"
}

fn contains_assign(n: &ShapeNode) -> bool {
    match n {
        ShapeNode::Assign => true,
        ShapeNode::Bin { .. } => true,
        ShapeNode::Body(items) | ShapeNode::Block(items) => items.iter().any(contains_assign),
        ShapeNode::If { then_branch, else_branch, .. } => {
            contains_assign(then_branch)
                || else_branch.as_deref().map(contains_assign).unwrap_or(false)
        }
        ShapeNode::While { body, .. } | ShapeNode::For { body, .. } => contains_assign(body),
        _ => false,
    }
}

fn contains_exit(n: &ShapeNode) -> bool {
    match n {
        ShapeNode::Exit => true,
        ShapeNode::Body(items) | ShapeNode::Block(items) => items.iter().any(contains_exit),
        ShapeNode::If { then_branch, else_branch, .. } => {
            contains_exit(then_branch) || else_branch.as_deref().map(contains_exit).unwrap_or(false)
        }
        ShapeNode::While { body, .. } | ShapeNode::For { body, .. } => contains_exit(body),
        _ => false,
    }
}
