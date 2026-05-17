// SPDX-License-Identifier: Apache-2.0
//
// Round-trip, CID, canonicalization, and validation tests for
// ExamManifestMemento.
//
// Source of truth:
//   protocol/specs/2026-05-16-exam-manifest-memento.md section 1, 4, and 5

use provekit_ir_types::{
    ExamManifestContent, ExamManifestEnvelope, ExamManifestHeader, ExamManifestMemento,
    ExamManifestMetadata, ExamManifestValidationError, ExamQuestion, ExamQuestionKind,
};
use serde_json::json;

#[test]
fn exam_manifest_round_trips_as_jcs_bytes() {
    let mut manifest = example_manifest();
    manifest.header.cid = manifest.recompute_header_cid().expect("recompute cid");

    let serialized = manifest.to_jcs_string().expect("canonicalize");
    let parsed: ExamManifestMemento =
        serde_json::from_str(&serialized).expect("parse canonical manifest");

    assert_eq!(parsed, manifest);
    assert_eq!(
        parsed.recompute_header_cid().expect("recompute cid"),
        parsed.header.cid
    );
}

#[test]
fn exam_manifest_recomputes_stable_cid_and_changes_on_content_change() {
    let mut manifest = example_manifest();

    let first = manifest.recompute_header_cid().expect("first cid");
    let second = manifest.recompute_header_cid().expect("second cid");
    assert_eq!(first, second);

    manifest.header.content.concept_hub_version = "concept-hub/v1.0.1".to_string();
    let changed = manifest.recompute_header_cid().expect("changed cid");
    assert_ne!(first, changed);
}

#[test]
fn exam_manifest_canonicalizes_question_kinds_and_questions_for_cid_input() {
    let mut manifest = example_manifest();
    manifest.header.content.question_kinds = vec![
        "sort".to_string(),
        "morphism".to_string(),
        "composition".to_string(),
        "boundary-tag".to_string(),
        "effect".to_string(),
        "realization".to_string(),
    ];
    manifest.header.content.questions = vec![
        question(
            ExamQuestionKind::Realization,
            "concept:add",
            json!({"target_library":"core","target_language":"java"}),
            "RealizationMemento",
        ),
        question(
            ExamQuestionKind::Morphism,
            "concept:add",
            json!({"from_language":"rust"}),
            "MorphismMemento",
        ),
        question(
            ExamQuestionKind::Sort,
            "concept:int",
            json!({"language_type":"i32","language":"rust"}),
            "SortMorphismMemento",
        ),
    ];

    let jcs = manifest.cid_input_jcs_string().expect("cid input jcs");
    let value: serde_json::Value = serde_json::from_str(&jcs).expect("jcs parses");
    let kinds: Vec<&str> = value["content"]["question_kinds"]
        .as_array()
        .expect("question_kinds array")
        .iter()
        .map(|item| item.as_str().expect("kind is string"))
        .collect();
    assert_eq!(
        kinds,
        vec![
            "boundary-tag",
            "composition",
            "effect",
            "morphism",
            "realization",
            "sort"
        ]
    );

    let question_order: Vec<(&str, &str, &str)> = value["content"]["questions"]
        .as_array()
        .expect("questions array")
        .iter()
        .map(|item| {
            (
                item["kind"].as_str().expect("kind"),
                item["concept"].as_str().expect("concept"),
                item["expected_answer_shape"]
                    .as_str()
                    .expect("expected answer shape"),
            )
        })
        .collect();
    assert_eq!(
        question_order,
        vec![
            ("morphism", "concept:add", "MorphismMemento"),
            ("realization", "concept:add", "RealizationMemento"),
            ("sort", "concept:int", "SortMorphismMemento"),
        ]
    );
}

#[test]
fn exam_manifest_validation_rejects_empty_questions() {
    let mut manifest = example_manifest();
    manifest.header.content.questions.clear();

    assert_eq!(
        manifest.validate(),
        Err(ExamManifestValidationError::EmptyQuestions)
    );
}

#[test]
fn exam_manifest_validation_rejects_concept_without_prefix() {
    let mut manifest = example_manifest();
    manifest.header.content.questions[0].concept = "add".to_string();

    assert_eq!(
        manifest.validate(),
        Err(ExamManifestValidationError::InvalidConcept {
            index: 0,
            concept: "add".to_string()
        })
    );
}

#[test]
fn exam_manifest_accepts_future_question_kind_at_shape_level() {
    let raw = json!({
        "envelope": {
            "declaredAt": "2026-05-16T12:00:00Z",
            "signature": "ed25519:fixture-signature",
            "signer": "ed25519:fixture-signer"
        },
        "header": {
            "cid": "blake3-512:fixture",
            "content": {
                "concept_hub_version": "concept-hub/v1.0.0",
                "question_kinds": ["morphism", "some_future_kind"],
                "questions": [{
                    "concept": "concept:add",
                    "expected_answer_shape": "FutureAnswerMemento",
                    "kind": "some_future_kind",
                    "parameters": {"from_language": "future-lang"}
                }]
            }
        },
        "metadata": {
            "schemaVersion": "provekit-exam-manifest/v1"
        }
    });

    let manifest: ExamManifestMemento =
        serde_json::from_value(raw).expect("future kind parses as open enum");
    assert_eq!(
        manifest.header.content.questions[0].kind,
        ExamQuestionKind::Other("some_future_kind".to_string())
    );
    manifest
        .validate()
        .expect("future kind validates at shape level");
}

#[test]
fn exam_manifest_accepts_v1_1_schema_version() {
    let mut manifest = example_manifest();
    manifest.metadata.schema_version = "provekit-exam-manifest/v1.1".to_string();
    manifest.header.content.question_kinds = vec![
        "boundary-realization".to_string(),
        "boundary-tag".to_string(),
        "composition".to_string(),
        "concept-realization".to_string(),
        "effect-classification".to_string(),
        "morphism".to_string(),
        "sort-classification".to_string(),
    ];
    manifest.header.content.questions = vec![question(
        ExamQuestionKind::Other("concept-realization".to_string()),
        "concept:add",
        json!({"language": "rust"}),
        "RealizationMemento",
    )];

    manifest.validate().expect("v1.1 manifest validates");
}

fn example_manifest() -> ExamManifestMemento {
    ExamManifestMemento {
        envelope: ExamManifestEnvelope {
            declared_at: "2026-05-16T12:00:00Z".to_string(),
            signature: "ed25519:fixture-signature".to_string(),
            signer: "ed25519:fixture-signer".to_string(),
        },
        header: ExamManifestHeader {
            cid: "blake3-512:fixture".to_string(),
            content: ExamManifestContent {
                concept_hub_version: "concept-hub/v1.0.0".to_string(),
                question_kinds: vec![
                    "boundary-tag".to_string(),
                    "composition".to_string(),
                    "effect".to_string(),
                    "morphism".to_string(),
                    "realization".to_string(),
                    "sort".to_string(),
                ],
                questions: vec![
                    question(
                        ExamQuestionKind::Morphism,
                        "concept:add",
                        json!({"from_language":"rust"}),
                        "MorphismMemento",
                    ),
                    question(
                        ExamQuestionKind::Realization,
                        "concept:add",
                        json!({"target_language":"java","target_library":"core"}),
                        "RealizationMemento",
                    ),
                ],
            },
        },
        metadata: ExamManifestMetadata {
            schema_version: "provekit-exam-manifest/v1".to_string(),
        },
    }
}

fn question(
    kind: ExamQuestionKind,
    concept: &str,
    parameters: serde_json::Value,
    expected_answer_shape: &str,
) -> ExamQuestion {
    ExamQuestion {
        concept: concept.to_string(),
        expected_answer_shape: expected_answer_shape.to_string(),
        kind,
        parameters,
    }
}
