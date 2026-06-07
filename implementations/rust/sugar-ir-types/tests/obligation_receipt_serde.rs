// Round-trip and invariant tests for ObligationReceiptMemento.
//
// Source of truth: protocol/specs/2026-05-13-obligation-receipt-memento.md

use std::sync::Arc;

use sugar_canonicalizer::{blake3_512_of, encode_jcs, Value as CValue};
use sugar_ir_types::{InvalidReceiptError, ObligationReceiptMemento};

const CID_A: &str = "blake3-512:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const CID_B: &str = "blake3-512:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
const CID_C: &str = "blake3-512:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc";
const CID_D: &str = "blake3-512:dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd";
const CID_E: &str = "blake3-512:eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee";
const CID_F: &str = "blake3-512:ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff";
const CID_1: &str = "blake3-512:11111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111";

fn receipt(kind: &str, verdict: &str) -> ObligationReceiptMemento {
    ObligationReceiptMemento {
        artifact_cids: Vec::new(),
        backend_cid: CID_A.to_string(),
        backend_version: "backend 1.2.3".to_string(),
        counterexample_cid: None,
        input_formula_cid: CID_B.to_string(),
        model_or_trace_cid: None,
        obligation_cid: CID_C.to_string(),
        provenance_cid: CID_D.to_string(),
        receipt_kind: kind.to_string(),
        tactic_script_cid: None,
        verdict: verdict.to_string(),
    }
}

fn cstring(value: &str) -> Arc<CValue> {
    CValue::string(value)
}

fn cnullable(value: &Option<String>) -> Arc<CValue> {
    match value {
        Some(value) => CValue::string(value),
        None => CValue::null(),
    }
}

fn carray(values: &[String]) -> Arc<CValue> {
    CValue::array(values.iter().map(|value| CValue::string(value)).collect())
}

fn receipt_value(receipt: &ObligationReceiptMemento) -> Arc<CValue> {
    CValue::object([
        ("artifact_cids", carray(&receipt.artifact_cids)),
        ("backend_cid", cstring(&receipt.backend_cid)),
        ("backend_version", cstring(&receipt.backend_version)),
        ("counterexample_cid", cnullable(&receipt.counterexample_cid)),
        ("input_formula_cid", cstring(&receipt.input_formula_cid)),
        ("model_or_trace_cid", cnullable(&receipt.model_or_trace_cid)),
        ("obligation_cid", cstring(&receipt.obligation_cid)),
        ("provenance_cid", cstring(&receipt.provenance_cid)),
        ("receipt_kind", cstring(&receipt.receipt_kind)),
        ("tactic_script_cid", cnullable(&receipt.tactic_script_cid)),
        ("verdict", cstring(&receipt.verdict)),
    ])
}

fn recompute_cid(receipt: &ObligationReceiptMemento) -> String {
    let value = receipt_value(receipt);
    blake3_512_of(encode_jcs(value.as_ref()).as_bytes())
}

#[test]
fn receipt_kinds_round_trip_and_recompute_cid() {
    let mut cases = vec![
        receipt("discharged", "unsat"),
        receipt("counterexample", "sat"),
        receipt("tactic", "unsat"),
        receipt("inconclusive", "unknown"),
        receipt("refused", "malformed-artifact"),
    ];
    cases[1].counterexample_cid = Some(CID_E.to_string());
    cases[2].tactic_script_cid = Some(CID_F.to_string());
    cases[4].artifact_cids = vec![CID_1.to_string()];

    for original in cases {
        original.validate().expect("fixture is valid");
        let serialized = serde_json::to_string(&original).expect("serialize");
        let parsed: ObligationReceiptMemento =
            serde_json::from_str(&serialized).expect("deserialize");

        assert_eq!(parsed, original);
        assert_eq!(recompute_cid(&parsed), recompute_cid(&original));
        assert!(recompute_cid(&parsed).starts_with("blake3-512:"));
        assert_eq!(recompute_cid(&parsed).len(), 139);
    }
}

#[test]
fn serde_emits_explicit_null_optional_cids_and_jcs_key_order() {
    let r = receipt("inconclusive", "timeout");
    let serialized = serde_json::to_string(&r).expect("serialize");

    assert_eq!(
        serialized,
        format!(
            "{{\"artifact_cids\":[],\"backend_cid\":\"{CID_A}\",\"backend_version\":\"backend 1.2.3\",\"counterexample_cid\":null,\"input_formula_cid\":\"{CID_B}\",\"model_or_trace_cid\":null,\"obligation_cid\":\"{CID_C}\",\"provenance_cid\":\"{CID_D}\",\"receipt_kind\":\"inconclusive\",\"tactic_script_cid\":null,\"verdict\":\"timeout\"}}"
        )
    );
}

#[test]
fn validate_accepts_allowed_matrix_combinations() {
    receipt("discharged", "unsat").validate().unwrap();

    let mut discharged_sat = receipt("discharged", "sat");
    discharged_sat.model_or_trace_cid = Some(CID_E.to_string());
    discharged_sat.validate().unwrap();

    let mut counterexample = receipt("counterexample", "sat");
    counterexample.counterexample_cid = Some(CID_E.to_string());
    counterexample.validate().unwrap();

    let mut tactic_unknown = receipt("tactic", "unknown");
    tactic_unknown.tactic_script_cid = Some(CID_F.to_string());
    tactic_unknown.validate().unwrap();

    for verdict in [
        "unknown",
        "timeout",
        "budget-exhausted",
        "backend-disagreement",
    ] {
        let mut inconclusive = receipt("inconclusive", verdict);
        if verdict == "backend-disagreement" {
            inconclusive.artifact_cids = vec![CID_1.to_string()];
        }
        inconclusive.validate().unwrap();
    }

    receipt("refused", "malformed-artifact").validate().unwrap();
    receipt("refused", "example.org/refusal")
        .validate()
        .unwrap();
}

#[test]
fn validate_rejects_forbidden_matrix_combinations() {
    for verdict in [
        "unknown",
        "timeout",
        "budget-exhausted",
        "backend-disagreement",
        "malformed-artifact",
    ] {
        assert!(matches!(
            receipt("discharged", verdict).validate(),
            Err(InvalidReceiptError::InvalidVerdictForKind { .. })
        ));
    }

    let mut discharged_with_counterexample = receipt("discharged", "unsat");
    discharged_with_counterexample.counterexample_cid = Some(CID_E.to_string());
    assert!(matches!(
        discharged_with_counterexample.validate(),
        Err(InvalidReceiptError::ForbiddenCidField { .. })
    ));

    for verdict in [
        "unsat",
        "unknown",
        "timeout",
        "budget-exhausted",
        "backend-disagreement",
        "malformed-artifact",
    ] {
        let mut counterexample = receipt("counterexample", verdict);
        counterexample.counterexample_cid = Some(CID_E.to_string());
        assert!(matches!(
            counterexample.validate(),
            Err(InvalidReceiptError::InvalidVerdictForKind { .. })
        ));
    }

    assert!(matches!(
        receipt("counterexample", "sat").validate(),
        Err(InvalidReceiptError::MissingRequiredCidField { .. })
    ));

    for verdict in [
        "sat",
        "timeout",
        "budget-exhausted",
        "backend-disagreement",
        "malformed-artifact",
    ] {
        let mut tactic = receipt("tactic", verdict);
        tactic.tactic_script_cid = Some(CID_F.to_string());
        assert!(matches!(
            tactic.validate(),
            Err(InvalidReceiptError::InvalidVerdictForKind { .. })
        ));
    }

    assert!(matches!(
        receipt("tactic", "unsat").validate(),
        Err(InvalidReceiptError::MissingRequiredCidField { .. })
    ));

    let mut tactic_with_counterexample = receipt("tactic", "unsat");
    tactic_with_counterexample.tactic_script_cid = Some(CID_F.to_string());
    tactic_with_counterexample.counterexample_cid = Some(CID_E.to_string());
    assert!(matches!(
        tactic_with_counterexample.validate(),
        Err(InvalidReceiptError::ForbiddenCidField { .. })
    ));

    for verdict in ["sat", "unsat", "malformed-artifact"] {
        assert!(matches!(
            receipt("inconclusive", verdict).validate(),
            Err(InvalidReceiptError::InvalidVerdictForKind { .. })
        ));
    }

    assert!(matches!(
        receipt("inconclusive", "backend-disagreement").validate(),
        Err(InvalidReceiptError::MissingDisagreementArtifacts)
    ));

    let mut inconclusive_with_tactic = receipt("inconclusive", "unknown");
    inconclusive_with_tactic.tactic_script_cid = Some(CID_F.to_string());
    assert!(matches!(
        inconclusive_with_tactic.validate(),
        Err(InvalidReceiptError::ForbiddenCidField { .. })
    ));

    let mut inconclusive_with_counterexample = receipt("inconclusive", "unknown");
    inconclusive_with_counterexample.counterexample_cid = Some(CID_E.to_string());
    assert!(matches!(
        inconclusive_with_counterexample.validate(),
        Err(InvalidReceiptError::ForbiddenCidField { .. })
    ));

    for verdict in ["sat", "unsat", "unknown", "timeout", "budget-exhausted"] {
        assert!(matches!(
            receipt("refused", verdict).validate(),
            Err(InvalidReceiptError::InvalidVerdictForKind { .. })
        ));
    }

    let mut refused_with_tactic = receipt("refused", "malformed-artifact");
    refused_with_tactic.tactic_script_cid = Some(CID_F.to_string());
    assert!(matches!(
        refused_with_tactic.validate(),
        Err(InvalidReceiptError::ForbiddenCidField { .. })
    ));
}

#[test]
fn validate_enforces_model_or_trace_must_rules() {
    assert!(matches!(
        receipt("discharged", "sat").validate(),
        Err(InvalidReceiptError::MissingRequiredCidField { .. })
    ));

    let mut refused = receipt("refused", "malformed-artifact");
    refused.model_or_trace_cid = Some(CID_E.to_string());
    assert!(matches!(
        refused.validate(),
        Err(InvalidReceiptError::ForbiddenCidField { .. })
    ));
}

#[test]
fn validate_fails_closed_for_unknown_receipt_kinds_and_unnamespaced_extensions() {
    assert!(matches!(
        receipt("custom-kind", "unknown").validate(),
        Err(InvalidReceiptError::UnknownReceiptKind(_))
    ));

    assert!(matches!(
        receipt("refused", "custom-refusal").validate(),
        Err(InvalidReceiptError::InvalidVerdictForKind { .. })
    ));
}
