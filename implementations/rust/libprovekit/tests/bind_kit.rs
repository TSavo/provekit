// SPDX-License-Identifier: Apache-2.0

use libprovekit::core::{address, bind_term_document, BindKit, BindOptions, Cid, Input, Kit, Term};
use provekit_ir_types::Sort;
use serde_json::{json, Value};

fn primitive_sort(name: &str) -> Sort {
    Sort::Primitive {
        name: name.to_string(),
    }
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
fn bind_kit_transform_to_matches_existing_binder_named_term_document_cid() {
    let term_value = bind_input_value();
    let input_term = Term::Const {
        value: term_value.clone(),
        sort: primitive_sort("LiftPluginResponse"),
    };
    let expected_named =
        bind_term_document(&term_value, &BindOptions::default()).expect("existing binder succeeds");
    let expected_jcs = libprovekit::canonical::serializable_jcs(&expected_named)
        .expect("named term canonicalizes");
    let expected_cid = Cid::try_from(
        libprovekit::canonical::serializable_cid(&expected_named).expect("named term cids"),
    )
    .expect("named term cid parses");

    let claim = BindKit::default()
        .transform(&Input::Term(input_term.clone()))
        .expect("bind kit transforms term input");

    assert_eq!(claim.to, expected_cid);
    assert_eq!(claim.from, vec![address(&input_term)]);
    let payload = claim.payload.as_ref().expect("bind claim carries payload");
    let Term::Const { value, sort } = payload else {
        panic!("bind payload should be a named term document const");
    };
    assert_eq!(sort, &primitive_sort("NamedTermDocument"));
    assert_eq!(
        libprovekit::canonical::json_jcs(&value).expect("payload canonicalizes"),
        expected_jcs
    );
    assert!(
        value["terms"][0].get("namedTermTree").is_some(),
        "payload should retain namedTermTree"
    );
}
