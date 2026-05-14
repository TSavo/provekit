// SPDX-License-Identifier: Apache-2.0

use libprovekit::promotion_decision_registry::{
    ConsensusPolicy, PromotionDecisionKey, PromotionDecisionRegistry,
};
use provekit_ir_types::{
    PromotionDecisionEnvelope, PromotionDecisionHeader, PromotionDecisionMemento,
    PromotionDecisionMetadata, PromotionGate, PromotionResult,
};
use serde_json::json;

const FIXTURE_STATE_CID: &str = "blake3-512:295e0fd280088fc1e5e00d7bade11a2bf850c932180622e28f2fc92e64f97cd5bd757a73acf07f888b7c523e8efb65d8f0d01d50bc02740e5d771e750485d8f4";

#[test]
fn registry_indexes_admitted_consensus_vector_by_typed_key() {
    let decision = consensus_decision(
        "concept:sql-query",
        FIXTURE_STATE_CID,
        "documentary -> empirically-witnessed",
        8,
        PromotionResult::Admitted,
    );

    let mut registry = PromotionDecisionRegistry::new();
    registry.admit(decision.clone()).expect("decision admitted");

    let key = PromotionDecisionKey::new("concept:sql-query", FIXTURE_STATE_CID);
    let status = registry
        .get(&key)
        .expect("admitted consensus vector should be indexed");

    assert_eq!(status.key, key);
    assert_eq!(status.decision_cids, vec![decision.header.cid.clone()]);
    assert_eq!(
        status.decision_policy_cids,
        vec![decision.header.policy_cid.clone()]
    );
    assert_eq!(status.witnesses_consulted, 8);
    assert_eq!(status.consensus_vector["unique_signers"], 1);
    assert_eq!(status.consensus_vector["unique_fixtures"], 1);
    assert_eq!(status.consensus_vector["total_sample_count"], 8);
}

#[test]
fn consensus_policy_decides_whether_a_vector_is_sufficient() {
    let decision = consensus_decision(
        "concept:sql-query",
        FIXTURE_STATE_CID,
        "documentary -> empirically-witnessed",
        8,
        PromotionResult::Admitted,
    );
    let mut registry = PromotionDecisionRegistry::new();
    registry.admit(decision).expect("decision admitted");
    let status = registry
        .get(&PromotionDecisionKey::new(
            "concept:sql-query",
            FIXTURE_STATE_CID,
        ))
        .expect("status indexed");

    let permissive = ConsensusPolicy::from_json_str(
        r#"{
          "kind": "consensus-policy",
          "schemaVersion": "1",
          "thresholds": [
            {"axis": "min-witnesses-floor", "predicate": "n>=8"},
            {"axis": "environment-diversity", "predicate": "unique_fixtures>=1"},
            {"axis": "sample-depth", "predicate": "total_sample_count>=8"}
          ],
          "allow_failures": false
        }"#,
    )
    .expect("policy parses");
    assert!(permissive.admits(&status).is_ok());

    let stricter = ConsensusPolicy::from_json_str(
        r#"{
          "kind": "consensus-policy",
          "schemaVersion": "1",
          "thresholds": [
            {"axis": "observer-diversity", "predicate": "unique_signers>=2"}
          ],
          "allow_failures": false
        }"#,
    )
    .expect("policy parses");
    let err = stricter
        .admits(&status)
        .expect_err("one unsigned signer must not satisfy signer-diversity policy");
    assert!(
        err.contains("observer-diversity") && err.contains("unique_signers was 1"),
        "unexpected policy rejection: {err}"
    );
}

#[test]
fn consensus_policy_does_not_synthesize_strength_across_decisions() {
    let enough_witnesses_low_diversity = consensus_decision_with_vector(
        "concept:sql-query",
        FIXTURE_STATE_CID,
        "documentary -> empirically-witnessed",
        8,
        PromotionResult::Admitted,
        json!({
            "failure_mode_distribution": [
                {"outcome": "pass", "count": 8},
                {"outcome": "fail", "count": 0},
                {"outcome": "inconclusive", "count": 0}
            ],
            "input_distribution_summary": {"shape": "unspanned"},
            "loss_dim_coverage": {
                "named_in_concept_spec": [],
                "unwitnessed": [],
                "witnessed": []
            },
            "temporal_spread": {
                "first_observed_at": "2026-05-14T00:00:00.000Z",
                "last_observed_at": "2026-05-14T00:00:00.000Z",
                "span_seconds": 0
            },
            "total_sample_count": 8,
            "unique_fixtures": 1,
            "unique_signer_keys": ["unsigned"],
            "unique_signers": 1
        }),
    );
    let high_diversity_too_few_witnesses = consensus_decision_with_vector(
        "concept:sql-query",
        FIXTURE_STATE_CID,
        "documentary -> empirically-witnessed",
        1,
        PromotionResult::Admitted,
        json!({
            "failure_mode_distribution": [
                {"outcome": "pass", "count": 1},
                {"outcome": "fail", "count": 0},
                {"outcome": "inconclusive", "count": 0}
            ],
            "input_distribution_summary": {"shape": "unspanned"},
            "loss_dim_coverage": {
                "named_in_concept_spec": [],
                "unwitnessed": [],
                "witnessed": []
            },
            "temporal_spread": {
                "first_observed_at": "2026-05-14T00:00:00.000Z",
                "last_observed_at": "2026-05-14T00:00:00.000Z",
                "span_seconds": 0
            },
            "total_sample_count": 1,
            "unique_fixtures": 3,
            "unique_signer_keys": ["a", "b", "c"],
            "unique_signers": 3
        }),
    );
    let mut registry = PromotionDecisionRegistry::new();
    registry
        .admit(enough_witnesses_low_diversity)
        .expect("first decision admitted");
    registry
        .admit(high_diversity_too_few_witnesses)
        .expect("second decision admitted");

    let key = PromotionDecisionKey::new("concept:sql-query", FIXTURE_STATE_CID);
    let policy = ConsensusPolicy::from_json_str(
        r#"{
          "kind": "consensus-policy",
          "schemaVersion": "1",
          "thresholds": [
            {"axis": "min-witnesses-floor", "predicate": "n>=8"},
            {"axis": "environment-diversity", "predicate": "unique_fixtures>=3"}
          ],
          "allow_failures": false
        }"#,
    )
    .expect("policy parses");

    assert!(
        registry
            .statuses(&key)
            .iter()
            .all(|status| policy.admits(status).is_err()),
        "policy must evaluate each observed vector independently"
    );
}

#[test]
fn registry_does_not_index_rejected_decisions() {
    let decision = consensus_decision(
        "concept:sql-query",
        FIXTURE_STATE_CID,
        "documentary -> empirically-witnessed",
        8,
        PromotionResult::Rejected,
    );

    let mut registry = PromotionDecisionRegistry::new();
    registry
        .admit(decision)
        .expect("valid rejected decision admitted");

    assert!(registry
        .get(&PromotionDecisionKey::new(
            "concept:sql-query",
            FIXTURE_STATE_CID
        ))
        .is_none());
}

#[test]
fn registry_rejects_header_cid_mismatch() {
    let mut decision = consensus_decision(
        "concept:sql-query",
        FIXTURE_STATE_CID,
        "documentary -> empirically-witnessed",
        8,
        PromotionResult::Admitted,
    );
    decision.header.cid = cid('a');

    let mut registry = PromotionDecisionRegistry::new();
    let err = registry
        .admit(decision)
        .expect_err("mismatched header cid must reject");

    assert!(
        err.to_string().contains("header.cid mismatch"),
        "unexpected error: {err}"
    );
}

fn consensus_decision(
    promoted_op: &str,
    fixture: &str,
    promotion: &str,
    min_witnesses: u64,
    result: PromotionResult,
) -> PromotionDecisionMemento {
    consensus_decision_with_vector(
        promoted_op,
        fixture,
        promotion,
        min_witnesses,
        result,
        json!({
            "failure_mode_distribution": [
                {"outcome": "pass", "count": min_witnesses},
                {"outcome": "fail", "count": 0},
                {"outcome": "inconclusive", "count": 0}
            ],
            "input_distribution_summary": {"shape": "unspanned"},
            "loss_dim_coverage": {
                "named_in_concept_spec": ["row-order"],
                "unwitnessed": ["row-order"],
                "witnessed": []
            },
            "temporal_spread": {
                "first_observed_at": "2026-05-14T00:00:00.000Z",
                "last_observed_at": "2026-05-14T00:00:00.000Z",
                "span_seconds": 0
            },
            "total_sample_count": min_witnesses,
            "unique_fixtures": 1,
            "unique_signer_keys": ["unsigned"],
            "unique_signers": 1
        }),
    )
}

fn consensus_decision_with_vector(
    promoted_op: &str,
    fixture: &str,
    promotion: &str,
    min_witnesses: u64,
    result: PromotionResult,
    consensus_vector: serde_json::Value,
) -> PromotionDecisionMemento {
    let evidence_cids = (0..min_witnesses)
        .map(|idx| cid(char::from(b'1' + (idx as u8 % 8))))
        .collect::<Vec<_>>();
    let mut decision = PromotionDecisionMemento {
        envelope: PromotionDecisionEnvelope {
            declared_at: "2026-05-14T00:00:00.000Z".to_string(),
            signature: String::new(),
            signer: cid('d'),
        },
        header: PromotionDecisionHeader {
            candidate_cid: cid('c'),
            cid: String::new(),
            decider_cid: cid('d'),
            decision_payload: json!({
                "agreement": "byte-equal",
                "fixtures_consulted": [fixture],
                "min_witnesses": min_witnesses,
                "promotion": promotion,
                "promoted_op": promoted_op,
                "reason": "test consensus",
                "total_observations": min_witnesses,
                "witnesses_consulted": evidence_cids,
                "consensus_vector": consensus_vector,
            }),
            evidence_cids,
            gate: PromotionGate::Threshold,
            kind: "promotion-decision".to_string(),
            policy_cid: cid('e'),
            promoted_cid: cid('f'),
            result,
            schema_version: "1".to_string(),
        },
        metadata: PromotionDecisionMetadata::default(),
    };
    decision.header.cid = decision
        .recompute_header_cid()
        .expect("fixture decision cid recomputes");
    decision
}

fn cid(ch: char) -> String {
    format!("blake3-512:{}", ch.to_string().repeat(128))
}
