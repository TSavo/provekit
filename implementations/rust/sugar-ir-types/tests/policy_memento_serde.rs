// Round-trip serde tests for the PolicyMemento family.
//
// Source of truth:
//   protocol/specs/2026-05-13-policy-memento.md

use sugar_canonicalizer::blake3_512_of;
use sugar_ir_types::PolicyMemento;

fn assert_round_trip_and_cid_stable(json: &str) {
    let parsed: PolicyMemento = serde_json::from_str(json).expect("parse policy memento");
    let serialized = serde_json::to_string(&parsed).expect("serialize policy memento");
    let reparsed: PolicyMemento =
        serde_json::from_str(&serialized).expect("reparse policy memento");
    let reserialized = serde_json::to_string(&reparsed).expect("reserialize policy memento");

    assert_eq!(parsed, reparsed);
    assert_eq!(serialized, reserialized);

    let cid = blake3_512_of(serialized.as_bytes());
    let recomputed_cid = blake3_512_of(reserialized.as_bytes());
    assert_eq!(cid, recomputed_cid);
    assert!(cid.starts_with("blake3-512:"));
    assert_eq!(cid.len(), "blake3-512:".len() + 128);
}

#[test]
fn threshold_policy_round_trip_and_cid_recompute() {
    let json = r#"{
  "admission_rule": "sugar.threshold:v1",
  "count_field_path": ["trial_summary", "passed"],
  "decision_payload_schema": {
    "required": ["trial_summary"]
  },
  "input_requirements": {
    "required": ["trial_summary"]
  },
  "policy_kind": "threshold",
  "policy_version": "2026-05-13",
  "provenance_cid": "b3.example.provenance",
  "refusal_rule": "sugar.threshold_refusal:v1",
  "score_field_path": [],
  "threshold_comparator": "gte",
  "threshold_value": 100
}"#;

    assert_round_trip_and_cid_stable(json);
}

#[test]
fn property_policy_round_trip_and_cid_recompute() {
    let json = r#"{
  "admission_rule": "sugar.property:v1",
  "decision_payload_schema": {
    "required": ["property_result"]
  },
  "generator_cid": "b3.example.generator",
  "input_requirements": {
    "required": ["property_result"]
  },
  "policy_kind": "property",
  "policy_version": "2026-05-13",
  "property_cid": "b3.example.property",
  "provenance_cid": "b3.example.provenance",
  "refusal_rule": "sugar.property_refusal:v1",
  "result_field_path": ["property_result"],
  "success_criteria": "sugar.property_success:v1"
}"#;

    assert_round_trip_and_cid_stable(json);
}

#[test]
fn signature_policy_round_trip_and_cid_recompute() {
    let json = r#"{
  "admission_rule": "sugar.signature:v1",
  "allowed_signature_suites": ["ed25519-jcs-blake3-512"],
  "decision_payload_schema": {
    "required": ["signatures"]
  },
  "input_requirements": {
    "required": ["signatures"]
  },
  "policy_kind": "signature",
  "policy_version": "2026-05-13",
  "provenance_cid": "b3.example.provenance",
  "quorum_size": 2,
  "refusal_rule": "sugar.signature_refusal:v1",
  "required_signers_cids": [
    "b3.example.signer.alice",
    "b3.example.signer.bob",
    "b3.example.signer.carol"
  ],
  "signature_payload_schema": {
    "required": ["payload_cid", "signature", "signer_cid"]
  }
}"#;

    assert_round_trip_and_cid_stable(json);
}

#[test]
fn human_acceptance_policy_round_trip_and_cid_recompute() {
    let json = r#"{
  "acceptance_record_schema": {
    "required": ["accepted_at", "reviewer_cid"]
  },
  "admission_rule": "sugar.human_acceptance:v1",
  "decision_payload_schema": {
    "required": ["acceptances"]
  },
  "delegation_policy_cid": "b3.example.delegation",
  "input_requirements": {
    "required": ["acceptances"]
  },
  "policy_kind": "human_acceptance",
  "policy_version": "2026-05-13",
  "provenance_cid": "b3.example.provenance",
  "refusal_rule": "sugar.human_acceptance_refusal:v1",
  "required_acceptances": 2,
  "reviewer_roster_cid": "b3.example.reviewers"
}"#;

    assert_round_trip_and_cid_stable(json);
}

#[test]
fn proof_gate_policy_round_trip_and_cid_recompute() {
    let json = r#"{
  "admission_rule": "sugar.proof_gate:v1",
  "checker_cid": "b3.example.checker",
  "decision_payload_schema": {
    "required": ["proof_artifact"]
  },
  "input_requirements": {
    "required": ["proof_artifact"]
  },
  "policy_kind": "proof_gate",
  "policy_version": "2026-05-13",
  "proof_artifact_schema": {
    "required": ["artifact_cid", "checker_output"]
  },
  "proof_system": "lean4",
  "provenance_cid": "b3.example.provenance",
  "refusal_rule": "sugar.proof_gate_refusal:v1",
  "theorem_ref": "Example.Theorem",
  "trusted_base_cid": "b3.example.trusted-base"
}"#;

    assert_round_trip_and_cid_stable(json);
}

#[test]
fn namespaced_extension_policy_round_trips() {
    let json = r#"{
  "admission_rule": "example.policy:v1",
  "decision_payload_schema": {
    "required": ["example"]
  },
  "extension_profile_cid": "b3.example.extension-profile",
  "input_requirements": {
    "required": ["example"]
  },
  "policy_kind": "example:custom_gate",
  "policy_version": "2026-05-13",
  "provenance_cid": "b3.example.provenance",
  "refusal_rule": "example.policy_refusal:v1"
}"#;

    assert_round_trip_and_cid_stable(json);
}

#[test]
fn unknown_bare_policy_kind_fails_closed() {
    let json = r#"{
  "admission_rule": "example.policy:v1",
  "decision_payload_schema": {},
  "input_requirements": {},
  "policy_kind": "custom_gate",
  "policy_version": "2026-05-13",
  "provenance_cid": "b3.example.provenance",
  "refusal_rule": "example.policy_refusal:v1"
}"#;

    let err = serde_json::from_str::<PolicyMemento>(json).expect_err("bare kind must fail");
    assert!(err.to_string().contains("unknown bare policy_kind"));
}
