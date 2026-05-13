use crate::h::*;
use crate::rust_gen::{Block, Expr, Stmt};

pub fn smt() -> Block {
    block(vec![
        let_mut("free_vars", call("BTreeMap::new", vec![])),
        let_("bound", call("BTreeSet::new", vec![])),
        expr_stmt(call(
            "collect_free_vars_formula",
            vec![var("formula"), raw("&mut free_vars"), raw("&bound")],
        )),
        let_mut("preamble", call("String::new", vec![])),
        push_str("preamble", "(set-logic ALL)\n"),
        Stmt::For {
            var: "(name, sort)".into(),
            iter: method(var("free_vars"), "iter", vec![]),
            body: block(vec![expr_stmt(method(
                var("preamble"),
                "push_str",
                vec![ref_expr(format(
                    "(declare-const {} {})\n",
                    vec![var("name"), var("sort")],
                ))],
            ))]),
        },
        let_(
            "body",
            format(
                "(assert (not {}))\n(check-sat)\n",
                vec![call("emit_formula", vec![var("formula")])],
            ),
        ),
        let_(
            "free_vars_vec",
            method(
                method(var("free_vars"), "into_iter", vec![]),
                "map",
                vec![closure(vec!["(name, sort)"], raw("FreeVar { name, sort }"))],
            ),
        ),
        let_(
            "free_vars_vec",
            method(var("free_vars_vec"), "collect", vec![]),
        ),
        ret(raw(
            "CompiledFormula { preamble, body, free_vars: free_vars_vec }",
        )),
    ])
}

pub fn coq() -> Block {
    block(vec![
        let_mut("free_vars", call("BTreeMap::new", vec![])),
        let_("bound", call("BTreeSet::new", vec![])),
        expr_stmt(call(
            "collect_free_vars_formula",
            vec![var("formula"), raw("&mut free_vars"), raw("&bound")],
        )),
        let_mut("body", call("String::new", vec![])),
        Stmt::For {
            var: "(name, sort)".into(),
            iter: method(var("free_vars"), "iter", vec![]),
            body: block(vec![
                let_(
                    "coq_sort",
                    Expr::Match {
                        expr: Box::new(method(var("sort"), "as_str", vec![])),
                        arms: vec![
                            crate::rust_gen::MatchArm {
                                pattern: r#""Int" | "Real""#.into(),
                                guard: None,
                                body: block(vec![expr_stmt(lit("Z"))]),
                            },
                            crate::rust_gen::MatchArm {
                                pattern: r#""String""#.into(),
                                guard: None,
                                body: block(vec![expr_stmt(lit("string"))]),
                            },
                            crate::rust_gen::MatchArm {
                                pattern: r#""Bool""#.into(),
                                guard: None,
                                body: block(vec![expr_stmt(lit("bool"))]),
                            },
                            crate::rust_gen::MatchArm {
                                pattern: "_".into(),
                                guard: None,
                                body: block(vec![expr_stmt(lit("Z"))]),
                            },
                        ],
                    },
                ),
                expr_stmt(method(
                    var("body"),
                    "push_str",
                    vec![ref_expr(format(
                        "Parameter {} : {}.\n",
                        vec![var("name"), var("coq_sort")],
                    ))],
                )),
            ]),
        },
        push_str("body", "\nGoal "),
        expr_stmt(method(
            var("body"),
            "push_str",
            vec![ref_expr(call("emit_formula", vec![var("formula")]))],
        )),
        push_str("body", ".\n"),
        push_str("body", "Proof.\n  intuition.\n  admit.\nQed.\n"),
        let_(
            "preamble",
            lit_str("Require Import ZArith String List.\nOpen Scope Z.\nOpen Scope string.\n\n"),
        ),
        let_(
            "free_vars_vec",
            method(
                method(var("free_vars"), "into_iter", vec![]),
                "map",
                vec![closure(vec!["(name, sort)"], raw("FreeVar { name, sort }"))],
            ),
        ),
        let_(
            "free_vars_vec",
            method(var("free_vars_vec"), "collect", vec![]),
        ),
        ret(raw("(preamble, body, free_vars_vec)")),
    ])
}
