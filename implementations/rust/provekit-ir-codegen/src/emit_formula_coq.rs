use crate::h::*;
use crate::rust_gen::{Block, Expr, MatchArm, Stmt};

pub fn build() -> Block {
    block(vec![Stmt::Match {
        expr: var("formula"),
        arms: vec![
            MatchArm { pattern: "Formula::Atomic { name, args }".into(), guard: None, body: block(vec![
                let_("args_str", method(var("args"), "iter", vec![])),
                let_("args_str", method(var("args_str"), "map", vec![closure(vec!["a"], call("emit_term", vec![var("a")]))])),
                let_typed("args_str", "Vec<String>", method(var("args_str"), "collect", vec![])),
                ret(Expr::Match {
                    expr: Box::new(method(var("name"), "as_str", vec![])),
                    arms: vec![
                        MatchArm { pattern: r#""=""#.into(), guard: None, body: block(vec![expr_stmt(format("({} = {})", vec![raw("args_str[0].clone()"), raw("args_str[1].clone()")]))]) },
                        MatchArm { pattern: r#"">""#.into(), guard: None, body: block(vec![expr_stmt(format("({} > {})", vec![raw("args_str[0].clone()"), raw("args_str[1].clone()")]))]) },
                        MatchArm { pattern: r#""<""#.into(), guard: None, body: block(vec![expr_stmt(format("({} < {})", vec![raw("args_str[0].clone()"), raw("args_str[1].clone()")]))]) },
                        MatchArm { pattern: r#""\u{2265}""#.into(), guard: None, body: block(vec![expr_stmt(format("({} >= {})", vec![raw("args_str[0].clone()"), raw("args_str[1].clone()")]))]) },
                        MatchArm { pattern: r#""\u{2264}""#.into(), guard: None, body: block(vec![expr_stmt(format("({} <= {})", vec![raw("args_str[0].clone()"), raw("args_str[1].clone()")]))]) },
                        MatchArm { pattern: r#""\u{2260}""#.into(), guard: None, body: block(vec![expr_stmt(format("({} <> {})", vec![raw("args_str[0].clone()"), raw("args_str[1].clone()")]))]) },
                        MatchArm { pattern: r#""true""#.into(), guard: None, body: block(vec![expr_stmt(lit_str("True"))]) },
                        MatchArm { pattern: r#""false""#.into(), guard: None, body: block(vec![expr_stmt(lit_str("False"))]) },
                        MatchArm { pattern: "_".into(), guard: None, body: block(vec![expr_stmt(format("{} {}", vec![var("name"), method(var("args_str"), "join", vec![lit(" ")])]))]) },
                    ],
                }),
            ])},
            MatchArm { pattern: "Formula::And { operands }".into(), guard: None, body: block(vec![
                let_("ops", method(var("operands"), "iter", vec![])),
                let_("ops", method(var("ops"), "map", vec![closure(vec!["o"], call("emit_formula", vec![var("o")]))])),
                let_typed("ops", "Vec<String>", method(var("ops"), "collect", vec![])),
                ret(format("({})", vec![method(var("ops"), "join", vec![raw(r##"r#" /\ "#"##)])])),
            ])},
            MatchArm { pattern: "Formula::Or { operands }".into(), guard: None, body: block(vec![
                let_("ops", method(var("operands"), "iter", vec![])),
                let_("ops", method(var("ops"), "map", vec![closure(vec!["o"], call("emit_formula", vec![var("o")]))])),
                let_typed("ops", "Vec<String>", method(var("ops"), "collect", vec![])),
                ret(format("({})", vec![method(var("ops"), "join", vec![raw(r##"r#" \/ "#"##)])])),
            ])},
            MatchArm { pattern: "Formula::Not { operands }".into(), guard: None, body: block_ret(format("(~{})", vec![call("emit_formula", vec![raw("&operands[0]")])])) },
            MatchArm { pattern: "Formula::Implies { operands }".into(), guard: None, body: block_ret(format("({} -> {})", vec![
                call("emit_formula", vec![raw("&operands[0]")]),
                call("emit_formula", vec![raw("&operands[1]")]),
            ]))},
            MatchArm { pattern: "Formula::Forall { name, sort, body }".into(), guard: None, body: block(vec![
                let_("coq_sort", call("sort_to_coq", vec![var("sort")])),
                let_("body_str", call("emit_formula", vec![var("body")])),
                ret(format("forall {} : {}, {}", vec![var("name"), var("coq_sort"), var("body_str")])),
            ])},
            MatchArm { pattern: "Formula::Exists { name, sort, body }".into(), guard: None, body: block(vec![
                let_("coq_sort", call("sort_to_coq", vec![var("sort")])),
                let_("body_str", call("emit_formula", vec![var("body")])),
                ret(format("exists {} : {}, {}", vec![var("name"), var("coq_sort"), var("body_str")])),
            ])},
            MatchArm { pattern: "Formula::Choice { var_name, sort, body }".into(), guard: None, body: block(vec![
                let_("coq_sort", call("sort_to_coq", vec![var("sort")])),
                let_("body_str", call("emit_formula", vec![var("body")])),
                ret(format("@sig {} {} (fun {} => {})", vec![var("var_name"), var("coq_sort"), var("var_name"), var("body_str")])),
            ])},
        ],
    }])
}
