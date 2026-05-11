// SPDX-License-Identifier: Apache-2.0

use libprovekit::core::{Cid, Term};
use libprovekit::transport::{transport_term, OperationTransport, TermTransport, TransportError};
use provekit_ir_types::Sort;

fn cid(ch: char) -> Cid {
    Cid::try_from(format!("blake3-512:{}", ch.to_string().repeat(128)))
        .expect("fixture cid is valid")
}

fn int_sort() -> Sort {
    Sort::Primitive {
        name: "Int".to_string(),
    }
}

fn var(name: &str) -> Term {
    Term::Var {
        name: name.to_string(),
    }
}

fn int(value: i64) -> Term {
    Term::Const {
        value: serde_json::json!(value),
        sort: int_sort(),
    }
}

fn op(name: &str, op_cid: Cid, args: Vec<Term>) -> Term {
    Term::Op {
        op_cid,
        name: name.to_string(),
        args,
    }
}

#[test]
fn c11_if_term_transports_to_concept_conditional_by_structural_recursion() {
    let c_if = cid('1');
    let c_eq = cid('2');
    let c_return = cid('3');
    let concept_conditional = cid('a');
    let concept_eq = cid('b');
    let concept_return = cid('c');

    let transport = TermTransport::new(vec![
        OperationTransport::new(
            "c11:if",
            c_if.clone(),
            "concept:conditional",
            concept_conditional.clone(),
        ),
        OperationTransport::new("c11:eq", c_eq.clone(), "concept:eq", concept_eq.clone()),
        OperationTransport::new(
            "c11:return",
            c_return.clone(),
            "concept:return",
            concept_return.clone(),
        ),
    ]);

    let source = op(
        "c11:if",
        c_if,
        vec![
            op("c11:eq", c_eq, vec![var("x"), int(0)]),
            op("c11:return", c_return.clone(), vec![int(-22)]),
            op("c11:return", c_return, vec![var("x")]),
        ],
    );

    let expected = op(
        "concept:conditional",
        concept_conditional,
        vec![
            op("concept:eq", concept_eq, vec![var("x"), int(0)]),
            op("concept:return", concept_return.clone(), vec![int(-22)]),
            op("concept:return", concept_return, vec![var("x")]),
        ],
    );

    assert_eq!(transport_term(&transport, &source).unwrap(), expected);
}

#[test]
fn concept_conditional_transports_to_rust_if() {
    let concept_conditional = cid('a');
    let concept_eq = cid('b');
    let rust_if = cid('d');
    let rust_eq = cid('e');

    let transport = TermTransport::new(vec![
        OperationTransport::new(
            "concept:conditional",
            concept_conditional.clone(),
            "rust:if",
            rust_if.clone(),
        ),
        OperationTransport::new("concept:eq", concept_eq.clone(), "rust:eq", rust_eq.clone()),
    ]);

    let source = op(
        "concept:conditional",
        concept_conditional,
        vec![op("concept:eq", concept_eq, vec![var("x"), int(0)])],
    );
    let expected = op(
        "rust:if",
        rust_if,
        vec![op("rust:eq", rust_eq, vec![var("x"), int(0)])],
    );

    assert_eq!(transport_term(&transport, &source).unwrap(), expected);
}

#[test]
fn transport_refuses_unknown_operation() {
    let missing = cid('f');
    let transport = TermTransport::new(Vec::new());
    let source = op("c11:while", missing, Vec::new());

    let err = transport_term(&transport, &source).expect_err("unknown op must refuse");
    assert_eq!(
        err,
        TransportError::MissingOperationMorphism {
            source_name: "c11:while".to_string()
        }
    );
}
