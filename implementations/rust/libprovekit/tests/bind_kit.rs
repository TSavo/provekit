// SPDX-License-Identifier: Apache-2.0

use std::path::PathBuf;

use libprovekit::core::{
    address, bind_term_document, concept_bind_result_cid, named_term_document_from_bind_payload,
    BindKit, BindOptions, Input, Kit, Term,
};
use libprovekit::proofir_bridge::CatalogIndex;
use provekit_ir_types::Sort;
use serde_json::{json, Value};

fn primitive_sort(name: &str) -> Sort {
    Sort::Primitive {
        name: name.to_string(),
    }
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf()
}

fn concept_catalog() -> CatalogIndex {
    CatalogIndex::from_catalog_root(repo_root().join("menagerie/concept-shapes/catalog"))
        .expect("concept-shapes catalog loads")
}

fn bind_input_value() -> Value {
    json!({
        "kind": "ir-document",
        "sourceLanguage": "rust",
        "workspaceRoot": "/tmp/provekit-bind-kit-test",
        "ir": [{
            "kind": "bind-lift-entry",
            "file": "src/lib.rs",
            "fn_name": "deposit",
            "fn_line": 14,
            "concept_annotation": "deposit-then-balance",
            "param_names": ["balance", "amount"],
            "param_types": ["i64", "i64"],
            "return_type": "i64",
            "term_shape": {
                "kind": "body",
                "stmts": [
                    {"kind": "let"},
                    {"kind": "bin", "op": "+"}
                ]
            },
            "term_shape_cid": "blake3-512:11111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111",
            "witnesses": [{
                "role": "post",
                "predicate_text": "out == balance + amount",
                "source_kind": "annotation"
            }]
        }]
    })
}

#[test]
fn bind_kit_transform_emits_bind_result_op_tree() {
    let term_value = bind_input_value();
    let input_term = Term::Const {
        value: term_value.clone(),
        sort: primitive_sort("LiftPluginResponse"),
    };
    let mut expected_payload_source_value = term_value.clone();
    expected_payload_source_value["ir"][0]
        .as_object_mut()
        .expect("bind-lift-entry object")
        .remove("fn_name");
    let expected_payload_source = Term::Const {
        value: expected_payload_source_value,
        sort: primitive_sort("LiftPluginResponse"),
    };
    let mut expected_named =
        bind_term_document(&term_value, &BindOptions::default()).expect("existing binder succeeds");
    for term in &mut expected_named.terms {
        term.function.clear();
    }

    let claim = BindKit::default()
        .transform(&Input::Term(input_term.clone()))
        .expect("bind kit transforms term input");

    assert_eq!(claim.from, vec![address(&input_term)]);
    let payload = claim.payload.as_ref().expect("bind claim carries payload");
    assert_eq!(claim.to, address(payload));
    let Term::Op {
        op_cid, name, args, ..
    } = payload
    else {
        panic!("bind payload should be a bind-result op tree");
    };
    assert_eq!(op_cid, &concept_bind_result_cid());
    assert_eq!(name, "concept:bind-result");
    assert_eq!(args.len(), 2);
    assert_eq!(args[0], expected_payload_source);
    assert!(
        matches!(args[1], Term::Op { .. }),
        "named form binding should be represented as an op tree"
    );
    assert!(
        payload.walk().count() >= 2,
        "bind output should expose operation nodes to Term::walk"
    );
    let catalog = concept_catalog();
    let unresolved = payload
        .walk()
        .filter(|node| catalog.get(node.op_cid.as_str()).is_none())
        .map(|node| (node.op_name.to_string(), node.op_cid.to_string()))
        .collect::<Vec<_>>();
    assert!(
        unresolved.is_empty(),
        "every bind payload op CID should resolve in the concept catalog: {unresolved:?}"
    );
    let recovered =
        named_term_document_from_bind_payload(payload).expect("bind payload recovers named terms");
    assert_eq!(
        serde_json::to_value(&recovered).expect("recovered named term serializes"),
        serde_json::to_value(&expected_named).expect("expected named term serializes")
    );

    let first_jcs =
        libprovekit::canonical::serializable_jcs(payload).expect("payload canonicalizes");
    let second_claim = BindKit::default()
        .transform(&Input::Term(expected_payload_source.clone()))
        .expect("bind kit transforms term input again");
    let second_payload = second_claim
        .payload
        .as_ref()
        .expect("second bind claim carries payload");
    assert_eq!(
        first_jcs,
        libprovekit::canonical::serializable_jcs(second_payload)
            .expect("second payload canonicalizes")
    );
}
