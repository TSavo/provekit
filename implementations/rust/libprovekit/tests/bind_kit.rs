// SPDX-License-Identifier: Apache-2.0

use std::path::PathBuf;

use libprovekit::core::{
    address, bind_term_document, concept_bind_result_cid, named_term_document_from_bind_payload,
    BindKit, BindOptions, Input, Kit, Term,
};
use libprovekit::proofir_bridge::CatalogIndex;
use provekit_ir_types::{ExamManifestMemento, Sort};
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

fn v1_1_exam_manifest() -> ExamManifestMemento {
    libprovekit::exam_manifest::load_default_exam_manifest().expect("v1.1 exam manifest loads")
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

fn witnessless_concept_input(concept: &str) -> Value {
    let mut value = bind_input_value();
    let entry = value["ir"][0]
        .as_object_mut()
        .expect("bind-lift-entry is object");
    entry.insert("concept_annotation".to_string(), json!(concept));
    entry.insert("witnesses".to_string(), json!([]));
    value
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

#[test]
fn bind_gap_record_cites_exam_question_when_wp_rule_synthesis_refuses() {
    let manifest = v1_1_exam_manifest();
    let input = witnessless_concept_input("add");

    let named = bind_term_document(
        &input,
        &BindOptions {
            lang: "rust".to_string(),
            exam_manifest: Some(manifest.clone()),
        },
    )
    .expect("bind succeeds");
    let named_json = serde_json::to_value(&named).expect("named term serializes");
    let gap = named_json["gapRecords"]
        .as_array()
        .expect("gapRecords array")
        .first()
        .expect("gap record emitted");
    let expected_question = libprovekit::exam_manifest::exam_question_cid_for(
        &manifest,
        "morphism",
        "concept:add",
        "rust",
    )
    .expect("lookup add/rust")
    .expect("add/rust question exists");

    assert_eq!(gap["kind"], "TransportGapMemento");
    assert_eq!(gap["gap_kind"], "wp-rule-mismatch");
    assert_eq!(gap["target_concept_op"], "concept:add");
    assert_eq!(gap["exam_manifest_cid"], manifest.header.cid);
    assert_eq!(gap["exam_question_cid"], expected_question);
}

#[test]
fn bind_gap_record_not_emitted_for_exact_witnessed_entry() {
    let named = bind_term_document(
        &bind_input_value(),
        &BindOptions {
            lang: "rust".to_string(),
            exam_manifest: Some(v1_1_exam_manifest()),
        },
    )
    .expect("bind succeeds");
    let named_json = serde_json::to_value(&named).expect("named term serializes");

    assert!(named_json.get("gapRecords").is_none());
}

#[test]
fn bind_gap_record_cites_exact_question_not_related_concept() {
    let manifest = v1_1_exam_manifest();
    let input = witnessless_concept_input("add");

    let named = bind_term_document(
        &input,
        &BindOptions {
            lang: "rust".to_string(),
            exam_manifest: Some(manifest.clone()),
        },
    )
    .expect("bind succeeds");
    let named_json = serde_json::to_value(&named).expect("named term serializes");
    let gap_question = named_json["gapRecords"][0]["exam_question_cid"]
        .as_str()
        .expect("gap cites question");
    let related_question = libprovekit::exam_manifest::exam_question_cid_for(
        &manifest,
        "morphism",
        "concept:sub",
        "rust",
    )
    .expect("lookup sub/rust")
    .expect("sub/rust question exists");

    assert_ne!(gap_question, related_question);
}
