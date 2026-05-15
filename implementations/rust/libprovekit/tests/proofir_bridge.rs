// SPDX-License-Identifier: Apache-2.0

use std::path::Path;

use libprovekit::proofir_bridge::{BridgeError, CatalogIndex, ResolvedNode, ResolvedTerm};
use libprovekit::proofir_resolve;
use provekit_ir_types::{Sort, Term};
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

    let resolved = proofir_resolve(&add(vec![int_lit(1), int_lit(2)]), &catalog).unwrap();

    match resolved.node {
        ResolvedNode::OpApplication {
            op_definition_cid,
            args,
        } => {
            assert_eq!(op_definition_cid, expected_cid);
            assert_eq!(args.len(), 2);
            assert_eq!(resolved.sort, json!({"args": [], "kind": "ctor", "name": "Int"}));
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
            expected: 2,
            actual: 1,
        }
    );
}

#[test]
fn nested_ops_lift_to_nested_op_application_tree() {
    let catalog = concept_catalog();
    let resolved = proofir_resolve(
        &add(vec![add(vec![int_lit(1), int_lit(2)]), int_lit(3)]),
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

    assert_eq!(args.len(), 2);
    assert!(matches!(
        args[0].node,
        ResolvedNode::OpApplication {
            op_definition_cid: _,
            args: _
        }
    ));
    assert!(matches!(args[1].node, ResolvedNode::Literal { .. }));
}
