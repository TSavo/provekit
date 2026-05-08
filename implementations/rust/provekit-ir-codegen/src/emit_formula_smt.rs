use crate::h::*;
use crate::rust_gen::{Block, MatchArm, Stmt};

pub fn build() -> Block {
    block(vec![Stmt::Match {
        expr: var("formula"),
        arms: vec![
            MatchArm {
                pattern: "Formula::Atomic { name, args }".into(),
                guard: None,
                body: block(vec![
                    let_("smt_name", call("smt_atomic_name", vec![var("name")])),
                    expr_stmt(raw("if args.is_empty() { return smt_name.to_string(); }")),
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
                        vec![
                            var("smt_name"),
                            method(var("args_str"), "join", vec![lit(" ")]),
                        ],
                    )),
                ]),
            },
            MatchArm {
                pattern: "Formula::And { operands }".into(),
                guard: None,
                body: block(vec![
                    let_("ops_str", method(var("operands"), "iter", vec![])),
                    let_(
                        "ops_str",
                        method(
                            var("ops_str"),
                            "map",
                            vec![closure(vec!["o"], call("emit_formula", vec![var("o")]))],
                        ),
                    ),
                    let_typed(
                        "ops_str",
                        "Vec<String>",
                        method(var("ops_str"), "collect", vec![]),
                    ),
                    ret(format(
                        "({} {})",
                        vec![lit("and"), method(var("ops_str"), "join", vec![lit(" ")])],
                    )),
                ]),
            },
            MatchArm {
                pattern: "Formula::Or { operands }".into(),
                guard: None,
                body: block(vec![
                    let_("ops_str", method(var("operands"), "iter", vec![])),
                    let_(
                        "ops_str",
                        method(
                            var("ops_str"),
                            "map",
                            vec![closure(vec!["o"], call("emit_formula", vec![var("o")]))],
                        ),
                    ),
                    let_typed(
                        "ops_str",
                        "Vec<String>",
                        method(var("ops_str"), "collect", vec![]),
                    ),
                    ret(format(
                        "({} {})",
                        vec![lit("or"), method(var("ops_str"), "join", vec![lit(" ")])],
                    )),
                ]),
            },
            MatchArm {
                pattern: "Formula::Not { operands }".into(),
                guard: None,
                body: block_ret(format(
                    "(not {})",
                    vec![call("emit_formula", vec![raw("&operands[0]")])],
                )),
            },
            MatchArm {
                pattern: "Formula::Implies { operands }".into(),
                guard: None,
                body: block_ret(format(
                    "(=> {} {})",
                    vec![
                        call("emit_formula", vec![raw("&operands[0]")]),
                        call("emit_formula", vec![raw("&operands[1]")]),
                    ],
                )),
            },
            MatchArm {
                pattern: "Formula::Forall { name, sort, body }".into(),
                guard: None,
                body: block(vec![
                    let_("sort_str", call("emit_sort", vec![var("sort")])),
                    let_("body_str", call("emit_formula", vec![var("body")])),
                    ret(format(
                        "(forall (({} {})) {})",
                        vec![var("name"), var("sort_str"), var("body_str")],
                    )),
                ]),
            },
            MatchArm {
                pattern: "Formula::Exists { name, sort, body }".into(),
                guard: None,
                body: block(vec![
                    let_("sort_str", call("emit_sort", vec![var("sort")])),
                    let_("body_str", call("emit_formula", vec![var("body")])),
                    ret(format(
                        "(exists (({} {})) {})",
                        vec![var("name"), var("sort_str"), var("body_str")],
                    )),
                ]),
            },
            MatchArm {
                pattern: "Formula::Choice { var_name, sort, body }".into(),
                guard: None,
                body: block(vec![
                    let_("sort_str", call("emit_sort", vec![var("sort")])),
                    let_("body_str", call("emit_formula", vec![var("body")])),
                    let_("var_y", format("{}_y", vec![var("var_name")])),
                    let_(
                        "body_y",
                        method(
                            var("body_str"),
                            "replace",
                            vec![var("var_name"), ref_expr(var("var_y"))],
                        ),
                    ),
                    let_(
                        "unique",
                        format(
                            "(and {} (forall (({} {})) (=> {} (= {} {}))))",
                            vec![
                                var("body_str"),
                                var("var_y"),
                                var("sort_str"),
                                var("body_y"),
                                var("var_y"),
                                var("var_name"),
                            ],
                        ),
                    ),
                    ret(format(
                        "(exists (({} {})) {})",
                        vec![var("var_name"), var("sort_str"), var("unique")],
                    )),
                ]),
            },
        ],
    }])
}

pub fn atomic_name_body() -> Block {
    block(vec![Stmt::Match {
        expr: var("name"),
        arms: vec![
            MatchArm {
                pattern: r#""\u{2260}""#.into(),
                guard: None,
                body: block_ret(lit("distinct")),
            },
            MatchArm {
                pattern: r#""\u{2264}""#.into(),
                guard: None,
                body: block_ret(lit("<=")),
            },
            MatchArm {
                pattern: r#""\u{2265}""#.into(),
                guard: None,
                body: block_ret(lit(">=")),
            },
            MatchArm {
                pattern: "other".into(),
                guard: None,
                body: block_ret(var("other")),
            },
        ],
    }])
}
