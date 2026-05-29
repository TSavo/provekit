// SPDX-License-Identifier: Apache-2.0

use libprovekit::core::{
    address, bind_term_document, concept_bind_result_cid, grammar_op_cid,
    named_term_document_from_bind_payload, strip_realize_sidecar_from_lift_term, BindKit,
    BindOptions, Input, Kit, Term,
};
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

fn witnessless_concept_input(concept: &str) -> Value {
    let mut value = bind_input_value();
    let entry = value["ir"][0]
        .as_object_mut()
        .expect("bind-lift-entry is object");
    entry.insert("concept_annotation".to_string(), json!(concept));
    entry.insert("witnesses".to_string(), json!([]));
    value
}

fn cluster_input(entries: Vec<Value>) -> Value {
    json!({
        "kind": "ir-document",
        "sourceLanguage": "rust",
        "workspaceRoot": "/tmp/provekit-bind-kit-test",
        "ir": entries
    })
}

fn cluster_entry(fn_name: &str, concept: &str, cid_digit: char, witnesses: Vec<Value>) -> Value {
    json!({
        "kind": "bind-lift-entry",
        "file": "src/lib.rs",
        "fn_name": fn_name,
        "fn_line": 14,
        "concept_annotation": concept,
        "param_names": ["x"],
        "param_types": ["i64"],
        "return_type": "i64",
        "term_shape": {"kind": "bin", "op": "+"},
        "term_shape_cid": format!(
            "blake3-512:{}",
            std::iter::repeat(cid_digit).take(128).collect::<String>()
        ),
        "witnesses": witnesses
    })
}

fn candidate_cluster_manifest(named: &Value) -> &Value {
    named
        .get("candidateClusterManifest")
        .expect("candidateClusterManifest emitted")
}

fn candidate_cluster_count(named: &Value, concept: &str) -> u64 {
    candidate_cluster_manifest(named)["clusters"]
        .as_array()
        .expect("clusters array")
        .iter()
        .find(|cluster| cluster["conceptCluster"] == concept)
        .unwrap_or_else(|| panic!("cluster {concept} missing"))
        .get("candidateCount")
        .and_then(Value::as_u64)
        .expect("candidateCount is u64")
}

#[test]
fn bind_kit_transform_emits_bind_result_op_tree() {
    let term_value = bind_input_value();
    let input_term = Term::Const {
        value: term_value.clone(),
        sort: primitive_sort("LiftPluginResponse"),
    };
    // The bind payload's `source` arg is `strip_fn_name(strip_realize_sidecar(input))`.
    // The fn_name strip preserves the #1093 CID invariant; the realize-sidecar
    // strip preserves canonical content identity (so the CID is stable against
    // attr_pre/attr_post/concept_annotation/operand_bindings/proc_macro_invocations/
    // source_function_name noise).
    let expected_payload_source = {
        let canonical = strip_realize_sidecar_from_lift_term(input_term.clone());
        let Term::Const { mut value, sort } = canonical else {
            unreachable!("input term is Term::Const");
        };
        if let Some(object) = value["ir"][0].as_object_mut() {
            object.remove("fn_name");
        }
        Term::Const { value, sort }
    };
    let mut expected_named =
        bind_term_document(&term_value, &BindOptions::default()).expect("existing binder succeeds");
    for term in &mut expected_named.terms {
        // function is cleared in the wire form (preserving #1093 CID invariant);
        // fn_name_sugar carries the source name as a non-CID-affecting annotation
        if !term.function.is_empty() {
            term.fn_name_sugar = Some(term.function.clone());
        }
        term.function.clear();
        // #1075 federation: the wire op-tree (arg[1] of concept:bind-result, the
        // cross-language CID) clears source-language realize-only DISPLAY metadata
        // so typed-Rust and untyped-Python federate byte-identically. The full
        // NamedTermDocument (with these fields) lives on artifacts[0]; the wire
        // reconstruction recovered here legitimately lacks them. Signature types
        // are preserved on the wire (legacy lower path reads them back). Mirror
        // bind_payload_wire_named_term_document.
        term.visibility.clear();
        term.generic_params.clear();
        term.doc_lines.clear();
    }

    let claim = BindKit::default()
        .transform(&Input::Term(input_term.clone()))
        .expect("bind kit transforms term input");

    // Bind cites the canonical content CID of its input, which matches
    // lift.to under the same canonicalization (strip realize sidecar).
    let canonical_input = strip_realize_sidecar_from_lift_term(input_term.clone());
    assert_eq!(claim.from, vec![address(&canonical_input)]);
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
    // Every op CID in the bind payload must be the CODE shape-authority's
    // computed address for its op name: no dangling/fabricated CIDs, and no
    // dependency on the (deleted) on-disk concept catalog. The vector is the
    // incident of the shape; we check it derives from the authority, not that
    // it equals a frozen number.
    let unresolved = payload
        .walk()
        .filter(|node| grammar_op_cid(node.op_name).as_ref() != Some(node.op_cid))
        .map(|node| (node.op_name.to_string(), node.op_cid.to_string()))
        .collect::<Vec<_>>();
    assert!(
        unresolved.is_empty(),
        "every bind payload op CID must be the shape-authority's address for its op name: {unresolved:?}"
    );
    let recovered =
        named_term_document_from_bind_payload(payload).expect("bind payload recovers named terms");
    assert_eq!(
        serde_json::to_value(&recovered).expect("recovered named term serializes"),
        serde_json::to_value(&expected_named).expect("expected named term serializes")
    );

    // Bind is deterministic: running the same input twice produces the same payload bytes
    let first_jcs =
        libprovekit::canonical::serializable_jcs(payload).expect("payload canonicalizes");
    let second_claim = BindKit::default()
        .transform(&Input::Term(input_term.clone()))
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
fn bind_emits_candidate_cluster_manifest_with_cardinality_per_concept() {
    let input = cluster_input(vec![
        cluster_entry("add_one", "add", '1', vec![]),
        cluster_entry("add_two", "add", '2', vec![]),
        cluster_entry("sub_one", "sub", '3', vec![]),
        cluster_entry("mul_one", "concept:mul", '4', vec![]),
    ]);

    let named = bind_term_document(&input, &BindOptions::default()).expect("bind succeeds");
    let named_json = serde_json::to_value(&named).expect("named term serializes");
    let manifest = candidate_cluster_manifest(&named_json);

    assert_eq!(manifest["kind"], "candidate-cluster-manifest");
    assert_eq!(manifest["schemaVersion"], "1");
    assert_eq!(manifest["totalCandidates"], 4);
    assert_eq!(candidate_cluster_count(&named_json, "concept:add"), 2);
    assert_eq!(candidate_cluster_count(&named_json, "concept:mul"), 1);
    assert_eq!(candidate_cluster_count(&named_json, "concept:sub"), 1);
}

#[test]
fn candidate_cluster_manifest_groups_by_concept_not_function_name() {
    let input = cluster_input(vec![
        cluster_entry("add_one", "add", '1', vec![]),
        cluster_entry("add_two", "add", '1', vec![]),
    ]);

    let named = bind_term_document(&input, &BindOptions::default()).expect("bind succeeds");
    let named_json = serde_json::to_value(&named).expect("named term serializes");
    let clusters = candidate_cluster_manifest(&named_json)["clusters"]
        .as_array()
        .expect("clusters array");

    assert_eq!(clusters.len(), 1);
    assert_eq!(clusters[0]["conceptCluster"], "concept:add");
    assert_eq!(clusters[0]["candidateCount"], 2);
}

#[test]
fn candidate_cluster_manifest_groups_by_concept_not_shape_cid() {
    let input = cluster_input(vec![
        cluster_entry("add_one", "add", '1', vec![]),
        cluster_entry("add_two", "add", '2', vec![]),
    ]);

    let named = bind_term_document(&input, &BindOptions::default()).expect("bind succeeds");
    let named_json = serde_json::to_value(&named).expect("named term serializes");
    let clusters = candidate_cluster_manifest(&named_json)["clusters"]
        .as_array()
        .expect("clusters array");

    assert_eq!(clusters.len(), 1);
    assert_eq!(clusters[0]["conceptCluster"], "concept:add");
    assert_eq!(clusters[0]["candidateCount"], 2);
}

#[test]
fn candidate_cluster_manifest_counts_candidates_not_witnesses() {
    let witness = json!({
        "role": "post",
        "predicate_text": "out == x",
        "source_kind": "annotation"
    });
    let input = cluster_input(vec![
        cluster_entry("add_one", "add", '1', vec![witness.clone(), witness]),
        cluster_entry("add_two", "add", '2', vec![]),
    ]);

    let named = bind_term_document(&input, &BindOptions::default()).expect("bind succeeds");
    let named_json = serde_json::to_value(&named).expect("named term serializes");
    let clusters = candidate_cluster_manifest(&named_json)["clusters"]
        .as_array()
        .expect("clusters array");

    assert_eq!(clusters.len(), 1);
    assert_eq!(clusters[0]["conceptCluster"], "concept:add");
    assert_eq!(clusters[0]["candidateCount"], 2);
}

#[test]
fn bind_gap_record_emits_wp_rule_refusal() {
    let input = witnessless_concept_input("add");

    let named = bind_term_document(
        &input,
        &BindOptions {
            lang: "rust".to_string(),
        },
    )
    .expect("bind succeeds");
    let named_json = serde_json::to_value(&named).expect("named term serializes");
    let gap = named_json["gapRecords"]
        .as_array()
        .expect("gapRecords array")
        .first()
        .expect("gap record emitted");

    assert_eq!(gap["kind"], "TransportGapMemento");
    assert_eq!(gap["gap_kind"], "wp-rule-mismatch");
    assert_eq!(gap["target_concept_op"], "concept:add");
}

#[test]
fn bind_gap_record_not_emitted_for_exact_witnessed_entry() {
    let named = bind_term_document(
        &bind_input_value(),
        &BindOptions {
            lang: "rust".to_string(),
        },
    )
    .expect("bind succeeds");
    let named_json = serde_json::to_value(&named).expect("named term serializes");

    assert!(named_json.get("gapRecords").is_none());
}
