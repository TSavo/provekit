// SPDX-License-Identifier: Apache-2.0
//
// Round-trip serde tests for the transport-gap memento types.
//
// Source of truth:
//   protocol/specs/2026-05-14-transport-gap-and-partial-morphism-protocol.md §1.1-§1.4
//   protocol/provekit-ir.cddl  (TransportGapMemento, PartialMorphismMemento, LossyMorphismMemento)
//
// These tests pin:
//   * Each of the three mementos deserializes from the wire shape the spec defines.
//   * Round-trip parity at the serde layer: parse -> serialize -> parse yields the same value.
//   * Optional fields (reason, reason_note, signature, target_op_cid, gap_memento_cid)
//     are absent from the serialized JSON when None.
//   * `GapKind::NoSuchConceptOp` round-trips correctly (PR amendment).
//   * `LossRecord` with dimensions round-trips inside LossyMorphismMemento.
//
// Byte-exact CID pinning (if any) belongs in provekit-claim-envelope/tests/.

use sugar_ir_types::{
    DivergentSemanticsTag, FieldDelta, GapKind, GapReason, IrFormula, LossSeverityLevel,
    LossyMorphismMemento, OptionStatus, PartialMorphismMemento, ResolutionOptionKind,
    TransportGapMemento,
};

// ================================================================
// Fixture A: python:add -> concept:add ("polymorphic-source-op" gap)
// from spec §2.1
// ================================================================

const PYTHON_ADD_GAP_JSON: &str = r#"{
  "fn_name": "gap:python:add:to:concept:add",
  "gap_kind": "polymorphic-source-op",
  "kind": "TransportGapMemento",
  "reason": {
    "formal_sorts_delta": {
      "got": [{"kind": "atomic", "name": "Value", "args": []}, {"kind": "atomic", "name": "Value", "args": []}],
      "want": [{"kind": "atomic", "name": "Int", "args": []}, {"kind": "atomic", "name": "Int", "args": []}]
    }
  },
  "reason_note": "python:add dispatches on operand sort (int, float, str, list); concept:add is integer-only",
  "resolution_options": [
    {
      "option_kind": "partial-morphism",
      "status": "recommended",
      "tradeoff": "requires lift to establish operands_statically_int at every use-site; dynamic python rarely carries enough static sort info"
    },
    {
      "option_kind": "accept-permanent",
      "status": "deferred",
      "tradeoff": "no exact bridge; gap is permanent for dynamic python"
    }
  ],
  "schema_version": "1",
  "source_lang": "python",
  "source_op_cid": "blake3-512:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
  "target_op": "concept:add",
  "target_op_cid": "blake3-512:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
}"#;

#[test]
fn python_add_gap_deserializes_from_spec_shape() {
    let m: TransportGapMemento =
        serde_json::from_str(PYTHON_ADD_GAP_JSON).expect("parse python:add gap memento");

    assert_eq!(m.fn_name, "gap:python:add:to:concept:add");
    assert_eq!(m.gap_kind, GapKind::PolymorphicSourceOp);
    assert_eq!(m.kind, "TransportGapMemento");
    assert_eq!(m.schema_version, "1");
    assert_eq!(m.source_lang, "python");
    assert_eq!(m.target_op, "concept:add");
    assert!(m.target_op_cid.is_some());
    assert!(m.reason.is_some());
    assert!(m.reason_note.is_some());
    assert_eq!(m.resolution_options.len(), 2);
    assert_eq!(
        m.resolution_options[0].option_kind,
        ResolutionOptionKind::PartialMorphism
    );
    assert_eq!(m.resolution_options[0].status, OptionStatus::Recommended);
    assert_eq!(
        m.resolution_options[1].option_kind,
        ResolutionOptionKind::AcceptPermanent
    );
    assert_eq!(m.resolution_options[1].status, OptionStatus::Deferred);
    assert!(m.signature.is_none());
}

#[test]
fn python_add_gap_round_trips() {
    let m: TransportGapMemento = serde_json::from_str(PYTHON_ADD_GAP_JSON).expect("parse");
    let serialized = serde_json::to_string(&m).expect("serialize");
    let m2: TransportGapMemento = serde_json::from_str(&serialized).expect("re-parse");
    assert_eq!(m, m2);
}

// ================================================================
// Fixture B: no-such-target-op gap (PR amendment -- new gap_kind)
// target_op_cid is absent; target_op is the placeholder name.
// ================================================================

const NO_CONCEPT_OP_GAP_JSON: &str = r#"{
  "fn_name": "gap:go:channel-receive:to:concept:channel-receive",
  "gap_kind": "no-such-target-op",
  "kind": "TransportGapMemento",
  "reason": {
    "source_supported": false
  },
  "resolution_options": [
    {
      "option_kind": "accept-permanent",
      "status": "recommended",
      "tradeoff": "no concept:channel-receive hub op; accept the gap or mint a new hub op"
    }
  ],
  "schema_version": "1",
  "source_lang": "go",
  "source_op_cid": "blake3-512:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc",
  "target_op": "concept:channel-receive"
}"#;

#[test]
fn no_such_op_name_gap_deserializes() {
    let m: TransportGapMemento =
        serde_json::from_str(NO_CONCEPT_OP_GAP_JSON).expect("parse no-such-target-op gap");

    assert_eq!(m.gap_kind, GapKind::NoSuchConceptOp);
    assert!(
        m.target_op_cid.is_none(),
        "target_op_cid must be absent for no-such-target-op"
    );
    assert_eq!(m.source_lang, "go");
    assert!(m.reason.is_some());
    let reason = m.reason.as_ref().unwrap();
    assert_eq!(reason.source_supported, Some(false));
}

#[test]
fn no_such_op_name_gap_round_trips() {
    let m: TransportGapMemento = serde_json::from_str(NO_CONCEPT_OP_GAP_JSON).expect("parse");
    let serialized = serde_json::to_string(&m).expect("serialize");
    let m2: TransportGapMemento = serde_json::from_str(&serialized).expect("re-parse");
    assert_eq!(m, m2);
}

#[test]
fn no_such_op_name_omits_target_op_cid_when_none() {
    let m: TransportGapMemento = serde_json::from_str(NO_CONCEPT_OP_GAP_JSON).expect("parse");
    let v: serde_json::Value = serde_json::from_str(&serde_json::to_string(&m).unwrap()).unwrap();
    assert!(
        v.get("target_op_cid").is_none(),
        "target_op_cid must be absent in serialized JSON when None"
    );
}

// ================================================================
// Fixture C: rust:rem -> concept:mod ("divergent-semantics" gap)
// from spec §2.2
// ================================================================

const RUST_REM_GAP_JSON: &str = r#"{
  "fn_name": "gap:rust:rem:to:concept:mod",
  "gap_kind": "divergent-semantics",
  "kind": "TransportGapMemento",
  "reason": {
    "divergent_tag": "truncated-vs-floored-modulo"
  },
  "resolution_options": [
    {
      "option_kind": "partial-morphism",
      "status": "recommended",
      "tradeoff": "partial morphism valid on non-negative dividend; requires static proof of sign at use-site"
    },
    {
      "option_kind": "accept-permanent",
      "status": "deferred",
      "tradeoff": "no exact bridge for negative-dividend case"
    }
  ],
  "schema_version": "1",
  "source_lang": "rust",
  "source_op_cid": "blake3-512:dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd",
  "target_op": "concept:mod",
  "target_op_cid": "blake3-512:eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee"
}"#;

#[test]
fn rust_rem_gap_deserializes() {
    let m: TransportGapMemento =
        serde_json::from_str(RUST_REM_GAP_JSON).expect("parse rust:rem gap");

    assert_eq!(m.gap_kind, GapKind::DivergentSemantics);
    assert_eq!(m.source_lang, "rust");
    assert_eq!(m.target_op, "concept:mod");
    assert!(m.reason.is_some());
}

// ================================================================
// Fixture D: python:add -> concept:add PartialMorphismMemento
// from spec §1.2 / §2.1
// ================================================================

const PYTHON_ADD_PARTIAL_JSON: &str = r#"{
  "fn_name": "partial-morphism:python:add:to:concept:add",
  "gap_memento_cid": "blake3-512:ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff",
  "homomorphism_obligation": {
    "kind": "wp-refinement-under-precondition",
    "source": "blake3-512:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
    "target": "blake3-512:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
  },
  "kind": "PartialMorphismMemento",
  "literal_map": {},
  "operator_map": {},
  "renaming_map": {},
  "representation_map": {},
  "schema_version": "1",
  "source_contract_cid": "blake3-512:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
  "target_shape_cid": "blake3-512:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
  "validity_precondition": {
    "kind": "atomic",
    "name": "operands_statically_int",
    "args": [{"kind": "var", "name": "lhs"}, {"kind": "var", "name": "rhs"}]
  }
}"#;

#[test]
fn python_add_partial_morphism_deserializes() {
    let m: PartialMorphismMemento =
        serde_json::from_str(PYTHON_ADD_PARTIAL_JSON).expect("parse PartialMorphismMemento");

    assert_eq!(m.fn_name, "partial-morphism:python:add:to:concept:add");
    assert_eq!(m.kind, "PartialMorphismMemento");
    assert_eq!(m.schema_version, "1");
    assert_eq!(
        m.homomorphism_obligation.kind,
        "wp-refinement-under-precondition"
    );
    assert!(m.gap_memento_cid.is_some());
    assert!(m.signature.is_none());
    // Confirm validity_precondition parsed
    match &m.validity_precondition {
        IrFormula::Atomic { name, .. } => assert_eq!(name, "operands_statically_int"),
        other => panic!("unexpected formula variant: {:?}", other),
    }
}

#[test]
fn partial_morphism_round_trips() {
    let m: PartialMorphismMemento = serde_json::from_str(PYTHON_ADD_PARTIAL_JSON).expect("parse");
    let serialized = serde_json::to_string(&m).expect("serialize");
    let m2: PartialMorphismMemento = serde_json::from_str(&serialized).expect("re-parse");
    assert_eq!(m, m2);
}

#[test]
fn partial_morphism_omits_signature_when_none() {
    let m: PartialMorphismMemento = serde_json::from_str(PYTHON_ADD_PARTIAL_JSON).expect("parse");
    let v: serde_json::Value = serde_json::from_str(&serde_json::to_string(&m).unwrap()).unwrap();
    assert!(
        v.get("signature").is_none(),
        "signature must be absent in serialized JSON when None"
    );
}

// ================================================================
// Fixture E: python:add -> c11:add LossyMorphismMemento
// from spec §1.4 / §2.1
// ================================================================

const PYTHON_ADD_LOSSY_JSON: &str = r#"{
  "coarsening_kind": "quotient-target-sort",
  "fn_name": "lossy-morphism:python:add:to:c11:add@mod-w",
  "homomorphism_obligation": {
    "kind": "wp-refinement-into-coarsening",
    "source": "blake3-512:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
    "target": "blake3-512:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
  },
  "kind": "LossyMorphismMemento",
  "literal_map": {},
  "loss": {
    "domain_narrowing": {"kind": "atomic", "name": "not_int_operand", "args": [{"kind": "var", "name": "lhs"}, {"kind": "var", "name": "rhs"}]},
    "ub_introduction": {"kind": "atomic", "name": "signed_overflow", "args": [{"kind": "var", "name": "lhs"}, {"kind": "var", "name": "rhs"}]},
    "value_divergence": {"kind": "atomic", "name": "overflow_wraps", "args": [{"kind": "var", "name": "lhs"}, {"kind": "var", "name": "rhs"}]}
  },
  "loss_severity": {
    "domain_narrowing": "safe-bounded",
    "ub_introduction": "lossy-bounded",
    "value_divergence": "lossy-bounded"
  },
  "operator_map": {},
  "renaming_map": {},
  "representation_map": {},
  "schema_version": "1",
  "source_contract_cid": "blake3-512:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
  "target_shape_cid": "blake3-512:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
}"#;

#[test]
fn python_add_lossy_morphism_deserializes() {
    let m: LossyMorphismMemento =
        serde_json::from_str(PYTHON_ADD_LOSSY_JSON).expect("parse LossyMorphismMemento");

    assert_eq!(m.fn_name, "lossy-morphism:python:add:to:c11:add@mod-w");
    assert_eq!(m.kind, "LossyMorphismMemento");
    assert_eq!(m.schema_version, "1");
    assert_eq!(m.coarsening_kind, "quotient-target-sort");
    assert_eq!(
        m.homomorphism_obligation.kind,
        "wp-refinement-into-coarsening"
    );
    assert!(m.gap_memento_cid.is_none());
    assert!(m.signature.is_none());
    // Confirm loss dimensions
    assert!(m.loss.0.contains_key("domain_narrowing"));
    assert!(m.loss.0.contains_key("ub_introduction"));
    assert!(m.loss.0.contains_key("value_divergence"));
    // Confirm severity tags
    assert_eq!(
        m.loss_severity.get("domain_narrowing"),
        Some(&LossSeverityLevel::SafeBounded)
    );
    assert_eq!(
        m.loss_severity.get("ub_introduction"),
        Some(&LossSeverityLevel::LossyBounded)
    );
}

#[test]
fn lossy_morphism_round_trips() {
    let m: LossyMorphismMemento = serde_json::from_str(PYTHON_ADD_LOSSY_JSON).expect("parse");
    let serialized = serde_json::to_string(&m).expect("serialize");
    let m2: LossyMorphismMemento = serde_json::from_str(&serialized).expect("re-parse");
    assert_eq!(m, m2);
}

#[test]
fn lossy_morphism_omits_gap_memento_cid_when_none() {
    let m: LossyMorphismMemento = serde_json::from_str(PYTHON_ADD_LOSSY_JSON).expect("parse");
    let v: serde_json::Value = serde_json::from_str(&serde_json::to_string(&m).unwrap()).unwrap();
    assert!(
        v.get("gap_memento_cid").is_none(),
        "gap_memento_cid must be absent in serialized JSON when None"
    );
}

// ================================================================
// Gap reason tests
// ================================================================

#[test]
fn gap_reason_omits_all_fields_when_default() {
    let r = GapReason::default();
    let v: serde_json::Value = serde_json::to_value(&r).expect("serialize empty GapReason");
    // All fields are Option + skip_serializing_if=None, so the object should be empty.
    assert_eq!(v, serde_json::json!({}));
}

#[test]
fn gap_reason_source_supported_false_round_trips() {
    let r = GapReason {
        source_supported: Some(false),
        ..Default::default()
    };
    let serialized = serde_json::to_string(&r).expect("serialize");
    let r2: GapReason = serde_json::from_str(&serialized).expect("re-parse");
    assert_eq!(r.source_supported, r2.source_supported);
    assert_eq!(r2.source_supported, Some(false));
}

// ================================================================
// Regression: Blocker 1 — DivergentSemanticsTag named variants
// Previously: #[serde(untagged)] + unit variants caused every named variant
// to serialize as "null" and every spec string to deserialize as Other(...).
// ================================================================

#[test]
fn divergent_tag_named_variant_serializes_to_spec_string() {
    // Each named variant must serialize to its spec-defined kebab string, NOT "null".
    let cases: &[(DivergentSemanticsTag, &str)] = &[
        (
            DivergentSemanticsTag::TruncatedVsFlooredModulo,
            "\"truncated-vs-floored-modulo\"",
        ),
        (
            DivergentSemanticsTag::BoundedVsUnboundedInteger,
            "\"bounded-vs-unbounded-integer\"",
        ),
        (
            DivergentSemanticsTag::IntegerVsTrueDivision,
            "\"integer-vs-true-division\"",
        ),
        (
            DivergentSemanticsTag::OverflowBehavior,
            "\"overflow-behavior\"",
        ),
        (DivergentSemanticsTag::RoundingMode, "\"rounding-mode\""),
        (
            DivergentSemanticsTag::ShortCircuitVsEager,
            "\"short-circuit-vs-eager\"",
        ),
    ];
    for (tag, expected) in cases {
        let json = serde_json::to_string(tag).unwrap();
        assert_eq!(
            json, *expected,
            "DivergentSemanticsTag::{:?} serialized to {} instead of {}",
            tag, json, expected
        );
    }
}

#[test]
fn divergent_tag_spec_string_deserializes_to_named_variant() {
    // Each spec-defined string must deserialize to the named variant, NOT Other(...).
    let tag: DivergentSemanticsTag =
        serde_json::from_str("\"truncated-vs-floored-modulo\"").unwrap();
    assert!(
        matches!(tag, DivergentSemanticsTag::TruncatedVsFlooredModulo),
        "expected TruncatedVsFlooredModulo, got {:?}",
        tag
    );

    let tag2: DivergentSemanticsTag =
        serde_json::from_str("\"bounded-vs-unbounded-integer\"").unwrap();
    assert!(matches!(
        tag2,
        DivergentSemanticsTag::BoundedVsUnboundedInteger
    ));

    let tag3: DivergentSemanticsTag = serde_json::from_str("\"integer-vs-true-division\"").unwrap();
    assert!(matches!(tag3, DivergentSemanticsTag::IntegerVsTrueDivision));

    let tag4: DivergentSemanticsTag = serde_json::from_str("\"overflow-behavior\"").unwrap();
    assert!(matches!(tag4, DivergentSemanticsTag::OverflowBehavior));

    let tag5: DivergentSemanticsTag = serde_json::from_str("\"rounding-mode\"").unwrap();
    assert!(matches!(tag5, DivergentSemanticsTag::RoundingMode));

    let tag6: DivergentSemanticsTag = serde_json::from_str("\"short-circuit-vs-eager\"").unwrap();
    assert!(matches!(tag6, DivergentSemanticsTag::ShortCircuitVsEager));
}

#[test]
fn divergent_tag_unknown_string_deserializes_to_other() {
    let tag: DivergentSemanticsTag = serde_json::from_str("\"some-future-tag\"").unwrap();
    assert!(
        matches!(tag, DivergentSemanticsTag::Other(ref s) if s == "some-future-tag"),
        "expected Other(\"some-future-tag\"), got {:?}",
        tag
    );
}

#[test]
fn divergent_tag_round_trips_for_all_named_variants() {
    let tags = vec![
        DivergentSemanticsTag::TruncatedVsFlooredModulo,
        DivergentSemanticsTag::BoundedVsUnboundedInteger,
        DivergentSemanticsTag::IntegerVsTrueDivision,
        DivergentSemanticsTag::OverflowBehavior,
        DivergentSemanticsTag::RoundingMode,
        DivergentSemanticsTag::ShortCircuitVsEager,
        DivergentSemanticsTag::Other("novel-tag".to_string()),
    ];
    for tag in tags {
        let json = serde_json::to_string(&tag).unwrap();
        let back: DivergentSemanticsTag = serde_json::from_str(&json).unwrap();
        assert_eq!(tag, back, "round-trip failed for {:?}", tag);
    }
}

// ================================================================
// Regression: Blocker 4 — GapReason field order must be alphabetical
// `divergent_tag` must serialize FIRST (before `effects_delta`).
// ================================================================

#[test]
fn gap_reason_field_order_is_alphabetical() {
    let r = GapReason {
        divergent_tag: Some(DivergentSemanticsTag::TruncatedVsFlooredModulo),
        effects_delta: Some(FieldDelta {
            got: serde_json::json!("a"),
            want: serde_json::json!("b"),
        }),
        source_supported: Some(false),
        ..Default::default()
    };
    let json = serde_json::to_string(&r).unwrap();
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    let keys: Vec<&str> = v.as_object().unwrap().keys().map(|k| k.as_str()).collect();
    // Keys must appear in alphabetical order: divergent_tag < effects_delta < source_supported
    assert_eq!(
        keys,
        vec!["divergent_tag", "effects_delta", "source_supported"],
        "GapReason fields must serialize in alphabetical order; got: {:?}",
        keys
    );
    // Confirm divergent_tag appears before effects_delta in raw JSON string
    let dt_pos = json.find("divergent_tag").unwrap();
    let ed_pos = json.find("effects_delta").unwrap();
    assert!(
        dt_pos < ed_pos,
        "divergent_tag ({}) must appear before effects_delta ({}) in JSON",
        dt_pos,
        ed_pos
    );
}
