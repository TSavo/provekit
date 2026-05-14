// SPDX-License-Identifier: Apache-2.0

use std::fs;
use std::path::PathBuf;
use std::process::Command;

use provekit_ir_types::{PromotionDecisionMemento, PromotionGate, PromotionResult, WitnessMemento};
use serde_json::{json, Value};

fn provekit_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_provekit"))
}

#[test]
fn witness_consensus_emits_promotion_decision_for_byte_equal_row_shape() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let catalog = tmp.path().join("catalog");
    fs::create_dir_all(&catalog).expect("create catalog dir");
    let out = tmp.path().join("promotion-receipt.proof");
    let concept = "concept:sql-query";
    let fixture = format!("blake3-512:{}", "a".repeat(128));
    let row_schema = json!({
        "columns": [
            {"name": "id", "declared_type": "INTEGER", "observed_typeof": "integer"},
            {"name": "name", "declared_type": "TEXT", "observed_typeof": "text"}
        ]
    });

    for i in 0..4 {
        let witness = witness(
            concept,
            &format!("ts:getUserById:{i}"),
            &fixture,
            &row_schema,
            i,
        );
        fs::write(
            catalog.join(format!("witness-{i}.json")),
            serde_json::to_string_pretty(&witness).expect("serialize witness"),
        )
        .expect("write witness");
    }

    let output = Command::new(provekit_bin())
        .arg("witness")
        .arg("consensus")
        .arg("--concept")
        .arg(concept)
        .arg("--require-fixture")
        .arg(&fixture)
        .arg("--min-witnesses")
        .arg("4")
        .arg("--catalog")
        .arg(&catalog)
        .arg("--emit")
        .arg(&out)
        .arg("--json")
        .output()
        .expect("spawn provekit witness consensus");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "witness consensus failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    let summary: Value = serde_json::from_slice(&output.stdout).expect("parse summary json");
    assert_eq!(summary["result"], "admitted");
    assert_eq!(summary["witnesses_consulted"], 4);
    assert_eq!(summary["agreement"], "byte-equal");
    assert_eq!(summary["promotion_receipt"], out.display().to_string());

    let raw = fs::read_to_string(&out).expect("promotion receipt written");
    let decision: PromotionDecisionMemento =
        serde_json::from_str(&raw).expect("parse promotion decision");
    decision.validate().expect("promotion decision validates");
    assert_eq!(
        decision.header.cid,
        decision
            .recompute_header_cid()
            .expect("recompute promotion decision cid")
    );
    assert_eq!(decision.header.kind, "promotion-decision");
    assert_eq!(decision.header.gate, PromotionGate::Threshold);
    assert_eq!(decision.header.result, PromotionResult::Admitted);
    assert_eq!(decision.header.evidence_cids.len(), 4);
    assert_eq!(decision.header.decision_payload["promoted_op"], concept);
    assert_eq!(
        decision.header.decision_payload["promotion"],
        "documentary -> empirically-witnessed"
    );
    assert_eq!(decision.header.decision_payload["agreement"], "byte-equal");
    assert_eq!(decision.header.decision_payload["total_observations"], 4);
    assert_eq!(
        decision.header.decision_payload["fixtures_consulted"],
        json!([fixture])
    );
    assert_eq!(decision.header.decision_payload["row_schema"], row_schema);
}

#[test]
fn witness_consensus_rejects_row_shape_disagreement() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let catalog = tmp.path().join("catalog");
    fs::create_dir_all(&catalog).expect("create catalog dir");
    let out = tmp.path().join("promotion-receipt.proof");
    let concept = "concept:sql-query";
    let fixture = format!("blake3-512:{}", "b".repeat(128));
    let row_schema = json!({
        "columns": [
            {"name": "id", "declared_type": "INTEGER", "observed_typeof": "integer"}
        ]
    });
    let divergent_row_schema = json!({
        "columns": [
            {"name": "id", "declared_type": "INTEGER", "observed_typeof": "integer"},
            {"name": "email", "declared_type": "TEXT", "observed_typeof": "text"}
        ]
    });

    for i in 0..3 {
        let witness = witness(
            concept,
            &format!("ts:getUserById:{i}"),
            &fixture,
            &row_schema,
            i,
        );
        fs::write(
            catalog.join(format!("witness-{i}.json")),
            serde_json::to_string_pretty(&witness).expect("serialize witness"),
        )
        .expect("write witness");
    }
    let divergent = witness(
        concept,
        "python:get_user_by_id:0",
        &fixture,
        &divergent_row_schema,
        99,
    );
    fs::write(
        catalog.join("witness-divergent.json"),
        serde_json::to_string_pretty(&divergent).expect("serialize divergent witness"),
    )
    .expect("write divergent witness");

    let output = Command::new(provekit_bin())
        .arg("witness")
        .arg("consensus")
        .arg("--concept")
        .arg(concept)
        .arg("--require-fixture")
        .arg(&fixture)
        .arg("--min-witnesses")
        .arg("4")
        .arg("--catalog")
        .arg(&catalog)
        .arg("--emit")
        .arg(&out)
        .output()
        .expect("spawn provekit witness consensus");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !output.status.success(),
        "row-shape disagreement must reject promotion"
    );
    assert!(
        stderr.contains("measurements.row_schema disagreement"),
        "stderr should identify the discovered loss axis:\n{stderr}"
    );
    assert!(
        !out.exists(),
        "rejected consensus must not emit a promotion decision"
    );
}

fn witness(
    concept: &str,
    subject: &str,
    fixture: &str,
    row_schema: &Value,
    idx: usize,
) -> WitnessMemento {
    let mut witness = WitnessMemento {
        cid: String::new(),
        fixture_state_cid: fixture.to_string(),
        kind: "witness".to_string(),
        measurements: json!({
            "query": {
                "sql": format!("SELECT id, name FROM users WHERE id = {idx}"),
                "sample_args": [idx]
            },
            "row_schema": row_schema,
            "sample_row": {
                "id": idx,
                "name": format!("user-{idx}")
            }
        }),
        observed_at: "2026-05-14T00:00:00.000Z".to_string(),
        outcome: "pass".to_string(),
        sample_count: 1,
        schema_version: "1".to_string(),
        signature: None,
        signed_by: None,
        subject: subject.to_string(),
        witness_for: concept.to_string(),
    };
    witness.cid = witness.recompute_cid().expect("witness cid");
    witness
}
