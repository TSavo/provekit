use crate::h::*;
use crate::rust_gen::{Block, MatchArm, Stmt};

pub fn term_body() -> Block {
    block(vec![Stmt::Match {
        expr: var("term"),
        arms: vec![
            MatchArm {
                pattern: "Term::Var { name, .. }".into(),
                guard: None,
                body: block(vec![expr_stmt(raw(
                    r#"if !bound.contains(name) { out.entry(name.clone()).or_insert("Int".to_string()); }"#,
                ))]),
            },
            MatchArm {
                pattern: "Term::Const { .. }".into(),
                guard: None,
                body: block(vec![]),
            },
            MatchArm {
                pattern: "Term::Ctor { args, .. }".into(),
                guard: None,
                body: block(vec![Stmt::For {
                    var: "a".into(),
                    iter: var("args"),
                    body: block(vec![expr_stmt(call(
                        "collect_free_vars_term",
                        vec![var("a"), var("out"), var("bound")],
                    ))]),
                }]),
            },
            MatchArm {
                pattern: "Term::Lambda { param_name, param_sort: _, body, .. }".into(),
                guard: None,
                body: block(vec![
                    let_mut("nb", method(var("bound"), "clone", vec![])),
                    expr_stmt(method(
                        var("nb"),
                        "insert",
                        vec![method(var("param_name"), "clone", vec![])],
                    )),
                    expr_stmt(call(
                        "collect_free_vars_term",
                        vec![var("body"), var("out"), ref_expr(var("nb"))],
                    )),
                ]),
            },
            MatchArm {
                pattern: "Term::Let { bindings, body, .. }".into(),
                guard: None,
                body: block(vec![
                    let_mut("current_bound", method(var("bound"), "clone", vec![])),
                    Stmt::For {
                        var: "b".into(),
                        iter: var("bindings"),
                        body: block(vec![
                            expr_stmt(call(
                                "collect_free_vars_term",
                                vec![
                                    ref_expr(field(var("b"), "bound_term")),
                                    var("out"),
                                    ref_expr(var("current_bound")),
                                ],
                            )),
                            expr_stmt(method(
                                var("current_bound"),
                                "insert",
                                vec![method(field(var("b"), "name"), "clone", vec![])],
                            )),
                        ]),
                    },
                    expr_stmt(call(
                        "collect_free_vars_term",
                        vec![var("body"), var("out"), ref_expr(var("current_bound"))],
                    )),
                ]),
            },
        ],
    }])
}

pub fn formula_body() -> Block {
    block(vec![Stmt::Match {
        expr: var("formula"),
        arms: vec![
            MatchArm {
                pattern: "Formula::Atomic { args, .. }".into(),
                guard: None,
                body: block(vec![Stmt::For {
                    var: "a".into(),
                    iter: var("args"),
                    body: block(vec![expr_stmt(call(
                        "collect_free_vars_term",
                        vec![var("a"), var("out"), var("bound")],
                    ))]),
                }]),
            },
            MatchArm {
                pattern: "Formula::And { operands }".into(),
                guard: None,
                body: block(vec![Stmt::For {
                    var: "o".into(),
                    iter: var("operands"),
                    body: block(vec![expr_stmt(call(
                        "collect_free_vars_formula",
                        vec![var("o"), var("out"), var("bound")],
                    ))]),
                }]),
            },
            MatchArm {
                pattern: "Formula::Or { operands }".into(),
                guard: None,
                body: block(vec![Stmt::For {
                    var: "o".into(),
                    iter: var("operands"),
                    body: block(vec![expr_stmt(call(
                        "collect_free_vars_formula",
                        vec![var("o"), var("out"), var("bound")],
                    ))]),
                }]),
            },
            MatchArm {
                pattern: "Formula::Not { operands }".into(),
                guard: None,
                body: block(vec![Stmt::For {
                    var: "o".into(),
                    iter: var("operands"),
                    body: block(vec![expr_stmt(call(
                        "collect_free_vars_formula",
                        vec![var("o"), var("out"), var("bound")],
                    ))]),
                }]),
            },
            MatchArm {
                pattern: "Formula::Implies { operands }".into(),
                guard: None,
                body: block(vec![Stmt::For {
                    var: "o".into(),
                    iter: var("operands"),
                    body: block(vec![expr_stmt(call(
                        "collect_free_vars_formula",
                        vec![var("o"), var("out"), var("bound")],
                    ))]),
                }]),
            },
            MatchArm {
                pattern: "Formula::Forall { name, sort: _, body }".into(),
                guard: None,
                body: block(vec![
                    let_mut("nb", method(var("bound"), "clone", vec![])),
                    expr_stmt(method(
                        var("nb"),
                        "insert",
                        vec![method(var("name"), "clone", vec![])],
                    )),
                    expr_stmt(call(
                        "collect_free_vars_formula",
                        vec![var("body"), var("out"), ref_expr(var("nb"))],
                    )),
                ]),
            },
            MatchArm {
                pattern: "Formula::Exists { name, sort: _, body }".into(),
                guard: None,
                body: block(vec![
                    let_mut("nb", method(var("bound"), "clone", vec![])),
                    expr_stmt(method(
                        var("nb"),
                        "insert",
                        vec![method(var("name"), "clone", vec![])],
                    )),
                    expr_stmt(call(
                        "collect_free_vars_formula",
                        vec![var("body"), var("out"), ref_expr(var("nb"))],
                    )),
                ]),
            },
            MatchArm {
                pattern: "Formula::Choice { var_name, sort: _, body }".into(),
                guard: None,
                body: block(vec![
                    let_mut("nb", method(var("bound"), "clone", vec![])),
                    expr_stmt(method(
                        var("nb"),
                        "insert",
                        vec![method(var("var_name"), "clone", vec![])],
                    )),
                    expr_stmt(call(
                        "collect_free_vars_formula",
                        vec![var("body"), var("out"), ref_expr(var("nb"))],
                    )),
                ]),
            },
        ],
    }])
}
