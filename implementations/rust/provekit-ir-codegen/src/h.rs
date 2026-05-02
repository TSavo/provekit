use crate::rust_gen::{Block, Expr, Stmt};

pub fn var(s: &str) -> Expr { Expr::Var(s.into()) }
pub fn raw(s: &str) -> Expr { Expr::Raw(s.into()) }
pub fn lit(s: &str) -> Expr { Expr::LiteralString(s.into()) }
pub fn lit_str(s: &str) -> Expr { method(lit(s), "to_string", vec![]) }
pub fn ref_expr(expr: Expr) -> Expr { Expr::Ref(Box::new(expr)) }
pub fn call(func: &str, args: Vec<Expr>) -> Expr { Expr::Call { func: func.into(), args } }
pub fn method(receiver: Expr, method: &str, args: Vec<Expr>) -> Expr {
    Expr::MethodCall { receiver: Box::new(receiver), method: method.into(), args }
}
pub fn field(base: Expr, field: &str) -> Expr {
    Expr::FieldAccess { base: Box::new(base), field: field.into() }
}
pub fn format(t: &str, args: Vec<Expr>) -> Expr {
    Expr::Format { template: t.into(), args }
}
pub fn closure(params: Vec<&str>, body: Expr) -> Expr {
    Expr::Closure { params: params.into_iter().map(|s| s.into()).collect(), body: Box::new(body) }
}

pub fn let_(name: &str, expr: Expr) -> Stmt { Stmt::Let { name: name.into(), expr } }
pub fn let_typed(name: &str, ty: &str, expr: Expr) -> Stmt { Stmt::LetTyped { name: name.into(), ty: ty.into(), expr } }
pub fn let_mut(name: &str, expr: Expr) -> Stmt { Stmt::LetMut { name: name.into(), expr } }
pub fn ret(expr: Expr) -> Stmt { Stmt::Return(expr) }
pub fn expr_stmt(expr: Expr) -> Stmt { Stmt::Expr(expr) }
pub fn push_str(target: &str, value: &str) -> Stmt {
    Stmt::PushStr { target: target.into(), value: value.into() }
}
pub fn block(stmts: Vec<Stmt>) -> Block { Block { stmts } }
pub fn block_ret(expr: Expr) -> Block { Block { stmts: vec![ret(expr)] } }

pub fn emit_sort_body() -> Block {
    block(vec![Stmt::Match {
        expr: var("sort"),
        arms: vec![crate::rust_gen::MatchArm {
            pattern: "Sort::Primitive { name }".into(),
            guard: None,
            body: block_ret(method(var("name"), "clone", vec![])),
        }],
    }])
}

pub fn emit_sort_body_coq() -> Block {
    block(vec![Stmt::Match {
        expr: var("sort"),
        arms: vec![crate::rust_gen::MatchArm {
            pattern: "Sort::Primitive { name }".into(),
            guard: None,
            body: block(vec![expr_stmt(Expr::Match {
                expr: Box::new(method(var("name"), "as_str", vec![])),
                arms: vec![
                    crate::rust_gen::MatchArm { pattern: r#""Int" | "Real""#.into(), guard: None, body: block(vec![expr_stmt(lit_str("Z"))]) },
                    crate::rust_gen::MatchArm { pattern: r#""String""#.into(), guard: None, body: block(vec![expr_stmt(lit_str("string"))]) },
                    crate::rust_gen::MatchArm { pattern: r#""Bool""#.into(), guard: None, body: block(vec![expr_stmt(lit_str("bool"))]) },
                    crate::rust_gen::MatchArm { pattern: "_".into(), guard: None, body: block(vec![expr_stmt(lit_str("Z"))]) },
                ],
            })]),
        }],
    }])
}

pub fn emit_const_value_body() -> Block {
    block(vec![Stmt::Match {
        expr: var("value"),
        arms: vec![
            crate::rust_gen::MatchArm { pattern: "serde_json::Value::Number(n)".into(), guard: None, body: block_ret(raw("if let Some(i) = n.as_i64() { i.to_string() } else if let Some(u) = n.as_u64() { u.to_string() } else { n.to_string() }")) },
            crate::rust_gen::MatchArm { pattern: "serde_json::Value::Bool(b)".into(), guard: None, body: block_ret(raw(r#"if *b { "true".to_string() } else { "false".to_string() }"#)) },
            crate::rust_gen::MatchArm { pattern: "serde_json::Value::String(s)".into(), guard: None, body: block_ret(format("\"{}\"", vec![var("s")])) },
            crate::rust_gen::MatchArm { pattern: "_".into(), guard: None, body: block_ret(lit_str("0")) },
        ],
    }])
}
