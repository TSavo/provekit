use crate::h::*;
use crate::rust_gen::{Block, MatchArm, Stmt};

pub fn build() -> Block {
    block(vec![Stmt::Match {
        expr: var("term"),
        arms: vec![
            MatchArm {
                pattern: "Term::Var { name, .. }".into(),
                guard: None,
                body: block_ret(method(var("name"), "clone", vec![])),
            },
            MatchArm {
                pattern: "Term::Const { value, sort, .. }".into(),
                guard: None,
                body: block(vec![
                    let_(
                        "sort_name",
                        raw("match sort { Sort::Primitive { name } => name.as_str() }"),
                    ),
                    ret(call(
                        "emit_const_value",
                        vec![var("value"), var("sort_name")],
                    )),
                ]),
            },
            MatchArm {
                pattern: "Term::Ctor { name, args, .. }".into(),
                guard: None,
                body: block(vec![
                    expr_stmt(raw("if args.is_empty() { return name.clone(); }")),
                    let_("args_str", method(var("args"), "iter", vec![])),
                    let_(
                        "args_str",
                        method(
                            var("args_str"),
                            "map",
                            vec![closure(vec!["a"], call("emit_term", vec![var("a")]))],
                        ),
                    ),
                    let_typed(
                        "args_str",
                        "Vec<String>",
                        method(var("args_str"), "collect", vec![]),
                    ),
                    ret(format(
                        "({} {})",
                        vec![var("name"), method(var("args_str"), "join", vec![lit(" ")])],
                    )),
                ]),
            },
            MatchArm {
                pattern: "Term::Lambda { param_name, param_sort, body, .. }".into(),
                guard: None,
                body: block(vec![
                    let_("sort_str", call("emit_sort", vec![var("param_sort")])),
                    let_("body_str", call("emit_term", vec![var("body")])),
                    ret(format(
                        "fun ({} : {}) => {}",
                        vec![var("param_name"), var("sort_str"), var("body_str")],
                    )),
                ]),
            },
            MatchArm {
                pattern: "Term::Let { bindings, body, .. }".into(),
                guard: None,
                body: block(vec![
                    let_mut("parts", call("Vec::new", vec![])),
                    Stmt::For {
                        var: "b".into(),
                        iter: var("bindings"),
                        body: block(vec![expr_stmt(method(
                            var("parts"),
                            "push",
                            vec![format(
                                "let {} := {} in",
                                vec![
                                    field(var("b"), "name"),
                                    call(
                                        "emit_term",
                                        vec![ref_expr(field(var("b"), "bound_term"))],
                                    ),
                                ],
                            )],
                        ))]),
                    },
                    let_("body_str", call("emit_term", vec![var("body")])),
                    ret(format(
                        "{} {}",
                        vec![
                            method(var("parts"), "join", vec![lit(" ")]),
                            var("body_str"),
                        ],
                    )),
                ]),
            },
            MatchArm {
                pattern: "Term::Ctor { name, args }".into(),
                guard: None,
                body: block(vec![
                    expr_stmt(raw("if args.is_empty() { return name.clone(); }")),
                    let_("args_str", method(var("args"), "iter", vec![])),
                    let_(
                        "args_str",
                        method(
                            var("args_str"),
                            "map",
                            vec![closure(vec!["a"], call("emit_term", vec![var("a")]))],
                        ),
                    ),
                    let_typed(
                        "args_str",
                        "Vec<String>",
                        method(var("args_str"), "collect", vec![]),
                    ),
                    ret(format(
                        "({} {})",
                        vec![var("name"), method(var("args_str"), "join", vec![lit(" ")])],
                    )),
                ]),
            },
            MatchArm {
                pattern: "Term::Lambda { param_name, param_sort, body }".into(),
                guard: None,
                body: block(vec![
                    let_("sort_str", call("emit_sort", vec![var("param_sort")])),
                    let_("body_str", call("emit_term", vec![var("body")])),
                    ret(format(
                        "fun ({} : {}) => {}",
                        vec![var("param_name"), var("sort_str"), var("body_str")],
                    )),
                ]),
            },
            MatchArm {
                pattern: "Term::Let { bindings, body }".into(),
                guard: None,
                body: block(vec![
                    let_mut("parts", call("Vec::new", vec![])),
                    Stmt::For {
                        var: "b".into(),
                        iter: var("bindings"),
                        body: block(vec![expr_stmt(method(
                            var("parts"),
                            "push",
                            vec![format(
                                "let {} := {} in",
                                vec![
                                    field(var("b"), "name"),
                                    call(
                                        "emit_term",
                                        vec![ref_expr(field(var("b"), "bound_term"))],
                                    ),
                                ],
                            )],
                        ))]),
                    },
                    let_("body_str", call("emit_term", vec![var("body")])),
                    ret(format(
                        "{} {}",
                        vec![
                            method(var("parts"), "join", vec![lit(" ")]),
                            var("body_str"),
                        ],
                    )),
                ]),
            },
        ],
    }])
}
