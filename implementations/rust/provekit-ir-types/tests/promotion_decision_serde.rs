// SPDX-License-Identifier: Apache-2.0
//
// Round-trip and CID-pin tests for PromotionDecisionMemento.
//
// Source of truth:
//   protocol/specs/2026-05-13-promotion-decision-memento.md §1 and §4

use provekit_ir_types::PromotionDecisionMemento;

const PROMOTION_DECISION_FIXTURE: &str = r#"{"envelope":{"declaredAt":"2026-05-13T12:00:00Z","signature":"ed25519:fixture-signature","signer":"ed25519:fixture-signer"},"header":{"candidate_cid":"blake3-512:111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111","cid":"blake3-512:4c6902a6c15b4a539f956e89d43fbf1fb0a7b243993e9b6c9ce0dd96646a8d956c9981d7313086a82bf04f897767666f320002f58560616c21c50b0bee9ca9ba","decider_cid":"blake3-512:222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222","decision_payload":{"distinctness_axis":"language","evaluated_score":17,"required_score":12,"result":"admitted"},"evidence_cids":["blake3-512:333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333","blake3-512:444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444"],"gate":"threshold","kind":"promotion-decision","policy_cid":"blake3-512:555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555","promoted_cid":"blake3-512:666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666","result":"admitted","schemaVersion":"1"},"metadata":{"counterexample_cids":[],"note":"Promotion accepted by threshold policy.","source_url":"https://example.test/provekit/promotion/791"}}"#;

#[test]
fn promotion_decision_round_trips_as_jcs_bytes() {
    let memento: PromotionDecisionMemento =
        serde_json::from_str(PROMOTION_DECISION_FIXTURE).expect("parse fixture");

    let serialized = memento.to_jcs_string().expect("canonicalize");

    assert_eq!(serialized, PROMOTION_DECISION_FIXTURE);
}

#[test]
fn promotion_decision_recomputes_pinned_header_cid() {
    let memento: PromotionDecisionMemento =
        serde_json::from_str(PROMOTION_DECISION_FIXTURE).expect("parse fixture");

    assert_eq!(
        memento.recompute_header_cid().expect("recompute cid"),
        memento.header.cid
    );
}

#[test]
fn promotion_decision_rejects_empty_evidence_cids() {
    use provekit_ir_types::PromotionDecisionInvariantError;
    let mut m: PromotionDecisionMemento =
        serde_json::from_str(PROMOTION_DECISION_FIXTURE).expect("parse fixture");
    m.header.evidence_cids.clear();
    assert_eq!(
        m.validate(),
        Err(PromotionDecisionInvariantError::EmptyEvidenceCids)
    );
}

#[test]
fn promotion_decision_validate_accepts_fixture() {
    let m: PromotionDecisionMemento =
        serde_json::from_str(PROMOTION_DECISION_FIXTURE).expect("parse fixture");
    m.validate().expect("fixture has non-empty evidence_cids");
}
