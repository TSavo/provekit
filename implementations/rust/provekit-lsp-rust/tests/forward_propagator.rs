// SPDX-License-Identifier: Apache-2.0

use provekit_lsp_rust::forward_propagator::{
    BaselineEntry, ForwardPropagator, LspRange, Post, Stmt,
};

fn unwrap_entry() -> BaselineEntry {
    BaselineEntry::new(
        "std::option::Option::unwrap",
        Some(Post::known(["receiver is Some"])),
        Some(Post::known(["returns inner value"])),
    )
}

fn call_unwrap() -> Stmt {
    Stmt::Call {
        callee_id: "std::option::Option::unwrap".into(),
        range: LspRange::single_line(4, 12, 18),
    }
}

#[test]
fn callsite_satisfies_pre_no_diagnostic() {
    let propagator = ForwardPropagator::new([unwrap_entry()]);
    let body = vec![
        Stmt::Assign {
            post: Post::known(["receiver is Some", "caller kept an extra fact"]),
        },
        call_unwrap(),
    ];

    let diagnostics = propagator.emit_diagnostics(&body);

    assert!(
        diagnostics.is_empty(),
        "extra caller facts still imply the callee precondition: {diagnostics:#?}"
    );
}

#[test]
fn callsite_violates_pre_diagnostic_emitted() {
    let propagator = ForwardPropagator::new([unwrap_entry()]);
    let body = vec![
        Stmt::Assign {
            post: Post::known(["receiver is None"]),
        },
        call_unwrap(),
    ];

    let diagnostics = propagator.emit_diagnostics(&body);

    assert_eq!(diagnostics.len(), 1, "{diagnostics:#?}");
    let diagnostic = &diagnostics[0];
    assert_eq!(diagnostic.code, "implication-failed");
    assert_eq!(diagnostic.source, "provekit");
    assert_eq!(diagnostic.severity, 1);
    assert_eq!(diagnostic.range, LspRange::single_line(4, 12, 18));
    assert_eq!(diagnostic.data.callee, "std::option::Option::unwrap");
    assert_eq!(diagnostic.data.missing_conjuncts, vec!["receiver is Some"]);
    assert!(diagnostic.data.current_post_cid.starts_with("blake3-512:"));
    assert!(diagnostic
        .data
        .baseline_index_cid
        .starts_with("blake3-512:"));

    let as_json = diagnostic.to_lsp_json();
    assert_eq!(as_json["code"], "implication-failed");
    assert_eq!(as_json["source"], "provekit");
    assert_eq!(as_json["data"]["kind"], "provekit.lsp.implication_failed");
}

#[test]
fn branch_merge_partial_satisfaction_diagnostic_on_join_path() {
    let propagator = ForwardPropagator::new([unwrap_entry()]);
    let body = vec![
        Stmt::IfElse {
            then_branch: vec![Stmt::Assign {
                post: Post::known(["receiver is Some"]),
            }],
            else_branch: vec![Stmt::Assign {
                post: Post::known([]),
            }],
        },
        call_unwrap(),
    ];

    let diagnostics = propagator.emit_diagnostics(&body);

    assert_eq!(diagnostics.len(), 1, "{diagnostics:#?}");
    assert_eq!(diagnostics[0].code, "implication-failed");
    assert_eq!(
        diagnostics[0].data.missing_conjuncts,
        vec!["receiver is Some"]
    );
}

#[test]
fn top_fallback_suppresses_false_positive() {
    let propagator = ForwardPropagator::new([unwrap_entry()]);
    let body = vec![Stmt::Unsupported, call_unwrap()];

    let diagnostics = propagator.emit_diagnostics(&body);

    assert!(
        diagnostics.is_empty(),
        "top fallback is loss of precision and must not become a diagnostic"
    );
}
