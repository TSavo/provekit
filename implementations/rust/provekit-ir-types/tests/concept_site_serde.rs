// SPDX-License-Identifier: Apache-2.0
//
// Round-trip serde tests for the ConceptSiteMemento and supporting types.
//
// Source of truth:
//   protocol/specs/2026-05-12-concept-site-memento.md §1, §2, §3
//
// These tests pin:
//   * ConceptSiteMemento and its sub-types deserialize from the wire shape
//     the spec defines.
//   * Round-trip parity at the serde layer: parse -> serialize -> parse
//     yields the same value.
//   * Optional fields (realization_mode_hint, discharge_receipt_cid,
//     refusal_reason) are absent from the serialized JSON when None.
//   * `witnesses: []` is preserved (a binding MAY have zero witnesses).
//   * The three verdicts -- "exact", "loudly-bounded-lossy", "refuse" --
//     all deserialize, and the verdict-consistency rules (§5.2) are
//     ENFORCED externally; this serde layer is purely shape-level.
//
// Byte-exact CID pinning lives in provekit-claim-envelope (this crate has
// no JCS encoder).

use std::collections::BTreeMap;

use provekit_ir_types::{
    CodeSite, CodeSiteSpan, ConceptSiteMemento, ConceptSiteProvenance, Discharge, IrFormula,
    LossRecord, WitnessRef,
};

// 128 hex chars after the "blake3-512:" prefix; deterministic placeholders.
const FN_CID: &str = "blake3-512:fn00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000";
const SRC_CID: &str = "blake3-512:src1111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111";
const CONCEPT_CID: &str = "blake3-512:concept22222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222";
const LOCAL_CID: &str = "blake3-512:local333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333";
const RECEIPT_CID: &str = "blake3-512:rcpt4444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444";
const LIFTER_CID: &str = "blake3-512:lifter5555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555";
const CLUST_CID: &str = "blake3-512:clust666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666";
const DISCH_CID: &str = "blake3-512:disch7777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777";
const WIT_CID_A: &str = "blake3-512:wit888888888888888888888888888888888888888888888888888888888888888888888888888888888888888888888888888888888888888888888888a";
const WIT_CID_B: &str = "blake3-512:wit888888888888888888888888888888888888888888888888888888888888888888888888888888888888888888888888888888888888888888888888b";
const BINDING_CID: &str = "blake3-512:bind999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999";

// ================================================================
// "exact" -- empty loss_record, discharge_receipt present
// ================================================================

const EXACT_FIXTURE: &str = r#"{
  "cid": "blake3-512:bind999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999",
  "code_site": {
    "function_term_cid": "blake3-512:fn00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000",
    "source_cid":        "blake3-512:src1111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111",
    "span": {"end": 1303, "start": 1248}
  },
  "concept_cid": "blake3-512:concept22222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222",
  "discharge": {
    "method":                "wp+witness",
    "verdict":               "exact",
    "discharge_receipt_cid": "blake3-512:rcpt4444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444",
    "loss_record":           {}
  },
  "kind":               "concept-site",
  "local_contract_cid": "blake3-512:local333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333",
  "provenance": {
    "clusterer_cid":  "blake3-512:clust666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666",
    "discharger_cid": "blake3-512:disch7777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777",
    "lifter_cid":     "blake3-512:lifter5555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555"
  },
  "realization_mode_hint": "witness",
  "schemaVersion": "1",
  "witnesses": [
    {"ci_basis_points": 10000, "witness_cid": "blake3-512:wit888888888888888888888888888888888888888888888888888888888888888888888888888888888888888888888888888888888888888888888888a"},
    {"ci_basis_points":  9500, "witness_cid": "blake3-512:wit888888888888888888888888888888888888888888888888888888888888888888888888888888888888888888888888888888888888888888888888b"}
  ]
}"#;

#[test]
fn exact_deserializes_from_spec_shape() {
    let m: ConceptSiteMemento = serde_json::from_str(EXACT_FIXTURE).expect("parse exact");

    assert_eq!(m.kind, "concept-site");
    assert_eq!(m.schema_version, "1");
    assert_eq!(m.cid, BINDING_CID);
    assert_eq!(m.code_site.function_term_cid, FN_CID);
    assert_eq!(m.code_site.source_cid, SRC_CID);
    assert_eq!(m.code_site.span.start, 1248);
    assert_eq!(m.code_site.span.end, 1303);
    assert_eq!(m.concept_cid, CONCEPT_CID);
    assert_eq!(m.local_contract_cid, LOCAL_CID);

    assert_eq!(m.discharge.method, "wp+witness");
    assert_eq!(m.discharge.verdict, "exact");
    assert!(m.discharge.refusal_reason.is_none());
    assert_eq!(m.discharge.discharge_receipt_cid.as_deref(), Some(RECEIPT_CID));
    assert!(m.discharge.loss_record.0.is_empty());

    assert_eq!(m.provenance.lifter_cid, LIFTER_CID);
    assert_eq!(m.provenance.clusterer_cid, CLUST_CID);
    assert_eq!(m.provenance.discharger_cid, DISCH_CID);

    assert_eq!(m.realization_mode_hint.as_deref(), Some("witness"));

    assert_eq!(m.witnesses.len(), 2);
    assert_eq!(m.witnesses[0].ci_basis_points, 10000);
    assert_eq!(m.witnesses[0].witness_cid, WIT_CID_A);
    assert_eq!(m.witnesses[1].ci_basis_points, 9500);
    assert_eq!(m.witnesses[1].witness_cid, WIT_CID_B);
}

#[test]
fn exact_round_trips() {
    let m1: ConceptSiteMemento = serde_json::from_str(EXACT_FIXTURE).expect("parse");
    let serialized = serde_json::to_string(&m1).expect("serialize");
    let m2: ConceptSiteMemento = serde_json::from_str(&serialized).expect("re-parse");
    assert_eq!(m1, m2);
}

// ================================================================
// "loudly-bounded-lossy" -- non-empty loss_record, discharge_receipt present
// ================================================================

const LOUDLY_LOSSY_FIXTURE: &str = r#"{
  "cid": "blake3-512:bind999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999",
  "code_site": {
    "function_term_cid": "blake3-512:fn00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000",
    "source_cid":        "blake3-512:src1111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111",
    "span": {"end": 1303, "start": 1248}
  },
  "concept_cid": "blake3-512:concept22222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222",
  "discharge": {
    "method":                "wp",
    "verdict":               "loudly-bounded-lossy",
    "discharge_receipt_cid": "blake3-512:rcpt4444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444",
    "loss_record": {
      "ub_introduction": {
        "kind": "atomic",
        "name": ">",
        "args": [
          {"kind": "var", "name": "x"},
          {"kind": "const", "value": 4611686018427387903, "sort": {"kind": "primitive", "name": "Int"}}
        ]
      }
    }
  },
  "kind":               "concept-site",
  "local_contract_cid": "blake3-512:local333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333",
  "provenance": {
    "clusterer_cid":  "blake3-512:clust666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666",
    "discharger_cid": "blake3-512:disch7777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777",
    "lifter_cid":     "blake3-512:lifter5555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555"
  },
  "schemaVersion": "1",
  "witnesses": []
}"#;

#[test]
fn loudly_lossy_deserializes_from_spec_shape() {
    let m: ConceptSiteMemento =
        serde_json::from_str(LOUDLY_LOSSY_FIXTURE).expect("parse loudly-bounded-lossy");

    assert_eq!(m.discharge.verdict, "loudly-bounded-lossy");
    assert_eq!(m.discharge.method, "wp");
    assert!(m.discharge.refusal_reason.is_none());
    assert!(m.discharge.discharge_receipt_cid.is_some());

    // The loss_record has exactly one non-empty dimension: ub_introduction.
    assert_eq!(m.discharge.loss_record.0.len(), 1);
    assert!(m.discharge.loss_record.0.contains_key("ub_introduction"));

    // No realization_mode_hint in this fixture.
    assert!(m.realization_mode_hint.is_none());

    // witnesses is an explicit empty array; it is REQUIRED to be present.
    assert!(m.witnesses.is_empty());
}

#[test]
fn loudly_lossy_round_trips() {
    let m1: ConceptSiteMemento = serde_json::from_str(LOUDLY_LOSSY_FIXTURE).expect("parse");
    let serialized = serde_json::to_string(&m1).expect("serialize");
    let m2: ConceptSiteMemento = serde_json::from_str(&serialized).expect("re-parse");
    assert_eq!(m1, m2);
}

// ================================================================
// "refuse" -- discharge_receipt OMITTED, refusal_reason present
// ================================================================

const REFUSE_FIXTURE: &str = r#"{
  "cid": "blake3-512:bind999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999",
  "code_site": {
    "function_term_cid": "blake3-512:fn00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000",
    "source_cid":        "blake3-512:src1111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111",
    "span": {"end": 1303, "start": 1248}
  },
  "concept_cid": "blake3-512:concept22222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222",
  "discharge": {
    "method":         "wp+witness",
    "refusal_reason": "wp and witness disagreed: wp claims wp_concept on every state, witness sample W shows result != 2*x at x = i64::MAX",
    "verdict":        "refuse",
    "loss_record":    {}
  },
  "kind":               "concept-site",
  "local_contract_cid": "blake3-512:local333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333",
  "provenance": {
    "clusterer_cid":  "blake3-512:clust666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666",
    "discharger_cid": "blake3-512:disch7777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777",
    "lifter_cid":     "blake3-512:lifter5555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555"
  },
  "schemaVersion": "1",
  "witnesses": []
}"#;

#[test]
fn refuse_deserializes_from_spec_shape() {
    let m: ConceptSiteMemento = serde_json::from_str(REFUSE_FIXTURE).expect("parse refuse");

    assert_eq!(m.discharge.verdict, "refuse");
    assert!(m.discharge.refusal_reason.is_some());
    // discharge_receipt_cid is OMITTED in the wire bytes; serde deserializes None.
    assert!(m.discharge.discharge_receipt_cid.is_none());
    assert!(m.discharge.loss_record.0.is_empty());
}

#[test]
fn refuse_round_trips() {
    let m1: ConceptSiteMemento = serde_json::from_str(REFUSE_FIXTURE).expect("parse");
    let serialized = serde_json::to_string(&m1).expect("serialize");
    let m2: ConceptSiteMemento = serde_json::from_str(&serialized).expect("re-parse");
    assert_eq!(m1, m2);
}

// ================================================================
// Optional-field absence: realization_mode_hint, refusal_reason,
// discharge_receipt_cid are OMITTED when None on serialization.
// ================================================================

#[test]
fn optional_fields_absent_when_none() {
    let mut loss = LossRecord::default();
    loss.0.insert(
        "value_divergence".into(),
        IrFormula::Atomic {
            name: "true".into(),
            args: vec![],
        },
    );

    let m = ConceptSiteMemento {
        cid: BINDING_CID.into(),
        code_site: CodeSite {
            function_term_cid: FN_CID.into(),
            source_cid: SRC_CID.into(),
            span: CodeSiteSpan { end: 10, start: 0 },
        },
        concept_cid: CONCEPT_CID.into(),
        discharge: Discharge {
            method: "wp".into(),
            refusal_reason: None,
            verdict: "loudly-bounded-lossy".into(),
            discharge_receipt_cid: Some(RECEIPT_CID.into()),
            loss_record: loss,
        },
        kind: "concept-site".into(),
        local_contract_cid: LOCAL_CID.into(),
        provenance: ConceptSiteProvenance {
            clusterer_cid: CLUST_CID.into(),
            discharger_cid: DISCH_CID.into(),
            lifter_cid: LIFTER_CID.into(),
        },
        realization_mode_hint: None,
        schema_version: "1".into(),
        witnesses: vec![],
    };

    let serialized = serde_json::to_string(&m).expect("serialize");

    // realization_mode_hint OMITTED.
    assert!(
        !serialized.contains("realization_mode_hint"),
        "realization_mode_hint should be OMITTED when None: {}",
        serialized
    );
    // refusal_reason OMITTED.
    assert!(
        !serialized.contains("refusal_reason"),
        "refusal_reason should be OMITTED when None: {}",
        serialized
    );
    // discharge_receipt_cid PRESENT (Some on this fixture).
    assert!(
        serialized.contains("discharge_receipt_cid"),
        "discharge_receipt_cid should be present when Some: {}",
        serialized
    );
    // witnesses ALWAYS present (required field; serializes even when empty).
    assert!(
        serialized.contains("\"witnesses\":[]"),
        "witnesses MUST always be present, even when empty: {}",
        serialized
    );
}

// ================================================================
// witnesses array preserves order and content
// ================================================================

#[test]
fn witnesses_round_trip_preserves_order_and_values() {
    let m1: ConceptSiteMemento = serde_json::from_str(EXACT_FIXTURE).expect("parse");
    assert_eq!(m1.witnesses.len(), 2);
    let serialized = serde_json::to_string(&m1).expect("serialize");
    let m2: ConceptSiteMemento = serde_json::from_str(&serialized).expect("re-parse");
    assert_eq!(m1.witnesses, m2.witnesses);
    // First witness's ci is exactly 10000 (100.00%).
    assert_eq!(m2.witnesses[0].ci_basis_points, 10000);
    // Second is 9500 (95.00%).
    assert_eq!(m2.witnesses[1].ci_basis_points, 9500);
}

// ================================================================
// loss_record with multiple non-empty dimensions round-trips.
// ================================================================

#[test]
fn loss_record_multidim_round_trips() {
    let mut loss = LossRecord::default();
    let f_true = IrFormula::Atomic {
        name: "true".into(),
        args: vec![],
    };
    let f_false = IrFormula::Atomic {
        name: "false".into(),
        args: vec![],
    };
    loss.0.insert("domain_narrowing".into(), f_true.clone());
    loss.0.insert("effect_divergence".into(), f_false.clone());
    loss.0.insert("structural_divergence".into(), f_true.clone());
    loss.0.insert("ub_introduction".into(), f_false.clone());
    loss.0.insert("value_divergence".into(), f_true.clone());

    let m1 = ConceptSiteMemento {
        cid: BINDING_CID.into(),
        code_site: CodeSite {
            function_term_cid: FN_CID.into(),
            source_cid: SRC_CID.into(),
            span: CodeSiteSpan { end: 99, start: 0 },
        },
        concept_cid: CONCEPT_CID.into(),
        discharge: Discharge {
            method: "wp".into(),
            refusal_reason: None,
            verdict: "loudly-bounded-lossy".into(),
            discharge_receipt_cid: Some(RECEIPT_CID.into()),
            loss_record: loss,
        },
        kind: "concept-site".into(),
        local_contract_cid: LOCAL_CID.into(),
        provenance: ConceptSiteProvenance {
            clusterer_cid: CLUST_CID.into(),
            discharger_cid: DISCH_CID.into(),
            lifter_cid: LIFTER_CID.into(),
        },
        realization_mode_hint: Some("monitor".into()),
        schema_version: "1".into(),
        witnesses: vec![WitnessRef {
            ci_basis_points: 7500,
            witness_cid: WIT_CID_A.into(),
        }],
    };
    let serialized = serde_json::to_string(&m1).expect("serialize");
    let m2: ConceptSiteMemento = serde_json::from_str(&serialized).expect("re-parse");
    assert_eq!(m1, m2);
    assert_eq!(m2.discharge.loss_record.0.len(), 5);
    // BTreeMap iteration order is sorted; that is the JCS-canonical order
    // for the loss_record sub-object.
    let keys: Vec<&String> = m2.discharge.loss_record.0.keys().collect();
    assert_eq!(
        keys,
        vec![
            &"domain_narrowing".to_string(),
            &"effect_divergence".to_string(),
            &"structural_divergence".to_string(),
            &"ub_introduction".to_string(),
            &"value_divergence".to_string(),
        ]
    );
}

// Defensive: ensure the IrFormula reference is used so the import doesn't
// regress under refactors. (The above tests construct IrFormula via type
// inference; this is a no-op assertion to keep the linter happy if the
// optimizer ever inlines it away.)
#[test]
fn _ir_formula_link() {
    let _f: IrFormula = IrFormula::Atomic {
        name: "true".into(),
        args: vec![],
    };
    // Reference BTreeMap to keep the import live (LossRecord wraps it).
    let _b: BTreeMap<String, IrFormula> = BTreeMap::new();
}
