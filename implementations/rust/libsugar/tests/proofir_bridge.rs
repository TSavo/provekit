// SPDX-License-Identifier: Apache-2.0

use std::path::Path;

use libsugar::proofir_bridge::{BridgeError, CatalogIndex, ResolvedNode, ResolvedTerm};
use libsugar::{proofir_resolve, proofir_unresolve};
use sugar_ir_types::{Sort, Term};
use serde_json::json;

fn int_sort() -> Sort {
    Sort::Primitive { name: "Int".into() }
}

fn int_lit(value: i64) -> Term {
    Term::Const {
        value: json!(value),
        sort: int_sort(),
    }
}

fn overflow_mode(value: &str) -> Term {
    Term::Const {
        value: json!(value),
        sort: Sort::Primitive {
            name: "ArithmeticOverflowMode".into(),
        },
    }
}

fn add(args: Vec<Term>) -> Term {
    Term::Ctor {
        name: "concept:add".into(),
        args,
    }
}

fn concept_catalog() -> CatalogIndex {
    CatalogIndex::from_catalog_root(Path::new("menagerie/concept-shapes/catalog"))
        .expect("concept-shapes catalog loads")
}

#[test]
fn add_op_resolves_to_op_application_with_catalog_cid() {
    let catalog = concept_catalog();
    let expected_cid = catalog
        .op_definition_cid("concept:add")
        .expect("concept:add is cataloged")
        .to_string();

    let resolved = proofir_resolve(
        &add(vec![int_lit(1), int_lit(2), overflow_mode("Checked")]),
        &catalog,
    )
    .unwrap();

    match resolved.node {
        ResolvedNode::OpApplication {
            op_definition_cid,
            args,
        } => {
            assert_eq!(op_definition_cid, expected_cid);
            assert_eq!(args.len(), 3);
            assert_eq!(
                resolved.sort,
                json!({"args": [], "kind": "ctor", "name": "Int"})
            );
        }
        ResolvedNode::Literal { .. } => panic!("expected op-application"),
    }
}

#[test]
fn unknown_op_refuses_with_op_name() {
    let catalog = concept_catalog();
    let err = proofir_resolve(
        &Term::Ctor {
            name: "concept:not-in-catalog".into(),
            args: vec![int_lit(1)],
        },
        &catalog,
    )
    .unwrap_err();

    assert_eq!(err, BridgeError::UnknownOp("concept:not-in-catalog".into()));
}

#[test]
fn arity_mismatch_refuses_with_expected_and_actual_counts() {
    let catalog = concept_catalog();
    let err = proofir_resolve(&add(vec![int_lit(1)]), &catalog).unwrap_err();

    assert_eq!(
        err,
        BridgeError::ArityMismatch {
            expected: 3,
            actual: 1,
        }
    );
}

#[test]
fn nested_ops_lift_to_nested_op_application_tree() {
    let catalog = concept_catalog();
    let resolved = proofir_resolve(
        &add(vec![
            add(vec![int_lit(1), int_lit(2), overflow_mode("Checked")]),
            int_lit(3),
            overflow_mode("Checked"),
        ]),
        &catalog,
    )
    .unwrap();

    let ResolvedTerm {
        node: ResolvedNode::OpApplication { args, .. },
        ..
    } = resolved
    else {
        panic!("expected outer op-application");
    };

    assert_eq!(args.len(), 3);
    assert!(matches!(
        args[0].node,
        ResolvedNode::OpApplication {
            op_definition_cid: _,
            args: _
        }
    ));
    assert!(matches!(args[1].node, ResolvedNode::Literal { .. }));
    assert!(matches!(args[2].node, ResolvedNode::Literal { .. }));
}

#[test]
fn resolved_term_unresolves_to_original_proofir_term() {
    let catalog = concept_catalog();
    let term = add(vec![int_lit(1), int_lit(2), overflow_mode("Checked")]);
    let resolved = proofir_resolve(&term, &catalog).unwrap();

    let unresolved = proofir_unresolve(&resolved, &catalog).unwrap();

    assert_eq!(unresolved, term);
}

#[test]
fn unknown_op_cid_refuses_with_cid() {
    let catalog = concept_catalog();
    let unknown_cid = "bafy-not-in-catalog".to_string();
    let resolved = ResolvedTerm {
        node: ResolvedNode::OpApplication {
            op_definition_cid: unknown_cid.clone(),
            args: Vec::new(),
        },
        sort: json!({"args": [], "kind": "ctor", "name": "Int"}),
    };

    let err = proofir_unresolve(&resolved, &catalog).unwrap_err();

    assert_eq!(err, BridgeError::UnknownOpCid(unknown_cid));
}

#[test]
fn resolved_literal_unresolves_to_const_with_same_value_and_sort() {
    let catalog = concept_catalog();
    let resolved = ResolvedTerm {
        node: ResolvedNode::Literal { value: json!(42) },
        sort: json!({"args": [], "kind": "ctor", "name": "Int"}),
    };

    let unresolved = proofir_unresolve(&resolved, &catalog).unwrap();

    assert_eq!(unresolved, int_lit(42));
}

#[test]
fn nested_resolved_term_unresolves_to_original_proofir_tree() {
    let catalog = concept_catalog();
    let term = add(vec![
        add(vec![int_lit(1), int_lit(2), overflow_mode("Checked")]),
        int_lit(3),
        overflow_mode("Checked"),
    ]);
    let resolved = proofir_resolve(&term, &catalog).unwrap();

    let unresolved = proofir_unresolve(&resolved, &catalog).unwrap();

    assert_eq!(unresolved, term);
}
