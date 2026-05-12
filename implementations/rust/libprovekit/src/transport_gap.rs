// SPDX-License-Identifier: Apache-2.0

//! Transport Gap Mementos -- §1.1, §1.2, §1.4
//!
//! Source of truth: protocol/specs/2026-05-14-transport-gap-and-partial-morphism-protocol.md
//! CDDL schema:     protocol/transport-gap-mementos.cddl
//!
//! Three memento types for when a language op cannot be mapped exactly into
//! the concept hub.  All types serialize to JCS-canonicalized JSON; CIDs are
//! BLAKE3-512 of those bytes.
//!
//! Key-order contract: serde_json with BTreeMap (the default map representation
//! in this crate) emits keys in lexicographic order, which is the JCS canonical
//! order.  Every struct field name here mirrors the schema exactly; do not rename
//! without updating the CDDL.
//!
//! structural_divergence successor-mint note: the "structural_divergence" field in
//! LossRecord is an addable dimension per LSP §4.4.  Existing loss-records that
//! omit it read as structural_divergence = None (formula = false).  CIDs of
//! previously-minted mementos are not affected.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

// ============================================================
// Loss record and dimensions -- §1.3
// ============================================================

/// A map from loss-dimension name to a formula characterizing that dimension's
/// divergence.  An absent key means "no loss in that dimension" (formula = false).
///
/// The `structural_divergence` key is a successor-mint addition (LSP §4.4):
/// it is absent in previously-minted records and reads as false.
///
/// Extension path: new dimensions are added as named Optional fields following
/// the successor-mint protocol (LSP §4.4).  The former `extra: BTreeMap` with
/// `#[serde(flatten)]` was removed (2026-05-11) because `#[serde(flatten)]`
/// emits flattened keys AFTER the named fields in plain serde_json output,
/// breaking lexicographic JCS key order at the struct level.  Any dimension
/// not yet in the named set must be added as a proper typed field.
///
/// JCS key order (lexicographic): domain_narrowing, effect_divergence,
/// structural_divergence, ub_introduction, value_divergence.
/// Fields are declared in this order so plain `serde_json::to_string` output
/// matches Python-emitted bytes (byte-level federation).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct LossRecord {
    /// Inputs the target cannot accept at all.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub domain_narrowing: Option<Value>,

    /// Inputs where the observable effect set differs.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub effect_divergence: Option<Value>,

    /// SUCCESSOR MINT (LSP §4.4): inputs where the target encodes the source
    /// construct as a structurally different shape (e.g. vtable-struct vs
    /// dict-lookup).  Absent in pre-PR1 mementos; reads as false there.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub structural_divergence: Option<Value>,

    /// Inputs where the target introduces UB absent in the source.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ub_introduction: Option<Value>,

    /// Inputs where the result VALUE differs.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value_divergence: Option<Value>,
}

/// Per-dimension advisory severity tags (heuristic, not solver-checked).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct LossSeverity(pub BTreeMap<String, String>);

// ============================================================
// Gap kind -- §1.1
// ============================================================

/// The categorical reason an exact morphism does not exist.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum GapKind {
    /// The op's formal sorts do not align.
    SortMismatch,
    /// Source op dispatches on operand type; hub op is monomorphic.
    PolymorphicSourceOp,
    /// Ops agree structurally but differ semantically.
    DivergentSemantics,
    /// The hub has no op for this source construct.
    MissingTargetConstruct,
    /// The language has no op for this concept node.
    MissingSourceOp,
    /// Effect sets are incompatible in the non-subset direction.
    EffectMismatch,
    /// Slot count or evaluation policy differs.
    ArityShapeMismatch,
    /// Post-WPF: the wp_rules do not refine.
    WpRuleMismatch,
    /// The language has an op the concept hub does not model at all.
    /// This drives "extend the concept hub" requests.
    NoSuchConceptOp,
    /// Gap cannot be characterized with the current schema; TODO pending.
    Unspecified,
}

// ============================================================
// Gap reason -- structured diff -- §1.1
// ============================================================

/// Structured diff explaining why the morphism was refused.
/// Fields mirror `diff_reason()` in `mint_language_morphisms.py`.
///
/// JCS key order (lexicographic): divergent_tag, effects_delta,
/// formal_sorts_delta, post_delta, pre_delta, source_supported, wp_rule_delta.
/// Fields declared in this order so plain serde_json output matches Python bytes.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct GapReason {
    /// REQUIRED when gap_kind == "divergent-semantics".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub divergent_tag: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub effects_delta: Option<JsonDelta>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub formal_sorts_delta: Option<FormalSortsDelta>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub post_delta: Option<JsonDelta>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub pre_delta: Option<FormulaDelta>,

    /// For missing-source-op: false = language lacks the op.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_supported: Option<bool>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub wp_rule_delta: Option<FormulaDelta>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FormalSortsDelta {
    pub got: Vec<Value>,
    pub want: Vec<Value>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FormulaDelta {
    pub got: Value,
    pub want: Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JsonDelta {
    pub got: Value,
    pub want: Value,
}

// ============================================================
// Resolution option -- §1.1
// ============================================================

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum OptionStatus {
    Recommended,
    Chosen,
    Deferred,
    Rejected,
}

/// One entry in the resolution_options menu.
///
/// JCS key order (lexicographic): dual_view_cid, loss, loss_severity,
/// lossy_morphism_cid, option_kind, partial_morphism_cid, precondition,
/// representation_map_delta, respec_target_to, split_targets, status, tradeoff.
/// Fields declared in this order so plain serde_json output matches Python bytes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolutionOption {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dual_view_cid: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub loss: Option<LossRecord>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub loss_severity: Option<LossSeverity>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub lossy_morphism_cid: Option<String>,

    pub option_kind: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub partial_morphism_cid: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub precondition: Option<Value>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub representation_map_delta: Option<Value>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub respec_target_to: Option<Value>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub split_targets: Option<Vec<String>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<OptionStatus>,

    pub tradeoff: String,
}

// ============================================================
// TransportGapMemento -- §1.1
// ============================================================

/// Records that a language op has no exact morphism into the concept hub.
///
/// Emitted by `mint_language_morphisms.py` for every gap site (see
/// `menagerie/concept-shapes/scripts/mint_language_morphisms.py`).
///
/// JCS key order (lexicographic): fn_name, gap_kind, kind, reason,
/// reason_note, resolution_options, schema_version, signature, source_lang,
/// source_op_cid, target_concept_op, target_op_cid.
/// Fields declared in this order so plain serde_json output matches Python bytes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransportGapMemento {
    pub fn_name: String,
    pub gap_kind: GapKind,
    pub kind: String,
    pub reason: GapReason,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason_note: Option<String>,
    pub resolution_options: Vec<ResolutionOption>,
    pub schema_version: String,
    /// Always present in JCS (never skipped); unsigned mementos carry `null`.
    /// Kept as `Value` (not `Option<Value>`) so Python-emitted `"signature": null`
    /// round-trips byte-identically — skip_serializing_if would omit it, breaking
    /// CID agreement with the Python generator which always writes the key.
    pub signature: Value,
    pub source_lang: String,
    pub source_op_cid: Value,        // null or cid string
    pub target_concept_op: String,
    pub target_op_cid: Value,        // null or cid string
}

impl TransportGapMemento {
    /// Construct an unsigned gap memento.
    pub fn new(
        fn_name: impl Into<String>,
        gap_kind: GapKind,
        source_lang: impl Into<String>,
        source_op_cid: Value,
        target_concept_op: impl Into<String>,
        target_op_cid: Value,
        reason: GapReason,
        resolution_options: Vec<ResolutionOption>,
    ) -> Self {
        Self {
            fn_name: fn_name.into(),
            gap_kind,
            kind: "TransportGapMemento".into(),
            reason,
            reason_note: None,
            resolution_options,
            schema_version: "1".into(),
            signature: Value::Null,
            source_lang: source_lang.into(),
            source_op_cid,
            target_concept_op: target_concept_op.into(),
            target_op_cid,
        }
    }
}

// ============================================================
// PartialMorphismMemento -- §1.2
// ============================================================

/// A LanguageMorphismMemento that holds under a precondition.
///
/// JCS key order: fn_name, gap_memento_cid, homomorphism_obligation, kind,
/// literal_map, operator_map, renaming_map, representation_map,
/// schema_version, signature, source_contract_cid, target_shape_cid,
/// validity_precondition.
/// Fields declared in this order so plain serde_json output matches Python bytes.
///
/// `signature` is always present in JCS (never skipped); unsigned mementos carry `null`.
/// Kept as `Value` (not `Option<Value>`) so Python-emitted `"signature": null`
/// round-trips byte-identically -- skip_serializing_if would omit it, breaking
/// CID agreement with any Python generator that always writes the key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PartialMorphismMemento {
    pub fn_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gap_memento_cid: Option<String>,
    pub homomorphism_obligation: HomomorphismObligation,
    pub kind: String,
    pub literal_map: Value,
    pub operator_map: Value,
    pub renaming_map: Value,
    pub representation_map: Value,
    pub schema_version: String,
    /// Always present in JCS (never skipped); unsigned mementos carry `null`.
    pub signature: Value,
    pub source_contract_cid: String,
    pub target_shape_cid: String,
    pub validity_precondition: Value,
}

impl PartialMorphismMemento {
    pub fn new(
        fn_name: impl Into<String>,
        source_contract_cid: impl Into<String>,
        target_shape_cid: impl Into<String>,
        validity_precondition: Value,
    ) -> Self {
        let src: String = source_contract_cid.into();
        let tgt: String = target_shape_cid.into();
        Self {
            fn_name: fn_name.into(),
            gap_memento_cid: None,
            homomorphism_obligation: HomomorphismObligation {
                kind: "wp-refinement-under-precondition".into(),
                source: src.clone(),
                target: tgt.clone(),
            },
            kind: "PartialMorphismMemento".into(),
            literal_map: Value::Object(Default::default()),
            operator_map: Value::Object(Default::default()),
            renaming_map: Value::Object(Default::default()),
            representation_map: Value::Object(Default::default()),
            schema_version: "1".into(),
            signature: Value::Null,
            source_contract_cid: src,
            target_shape_cid: tgt,
            validity_precondition,
        }
    }
}

// ============================================================
// LossyMorphismMemento -- §1.4
// ============================================================

/// A LanguageMorphismMemento that holds after coarsening the target's contract.
///
/// JCS key order: coarsening_kind, fn_name, gap_memento_cid,
/// homomorphism_obligation, kind, literal_map, loss, loss_severity,
/// operator_map, renaming_map, representation_map, schema_version,
/// signature, source_contract_cid, target_shape_cid.
/// Fields declared in this order so plain serde_json output matches Python bytes.
///
/// `signature` is always present in JCS (never skipped); unsigned mementos carry `null`.
/// Kept as `Value` (not `Option<Value>`) so Python-emitted `"signature": null`
/// round-trips byte-identically -- skip_serializing_if would omit it, breaking
/// CID agreement with any Python generator that always writes the key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LossyMorphismMemento {
    pub coarsening_kind: String,
    pub fn_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gap_memento_cid: Option<String>,
    pub homomorphism_obligation: HomomorphismObligation,
    pub kind: String,
    pub literal_map: Value,
    pub loss: LossRecord,
    pub loss_severity: LossSeverity,
    pub operator_map: Value,
    pub renaming_map: Value,
    pub representation_map: Value,
    pub schema_version: String,
    /// Always present in JCS (never skipped); unsigned mementos carry `null`.
    pub signature: Value,
    pub source_contract_cid: String,
    pub target_shape_cid: String,
}

// ============================================================
// Shared sub-types
// ============================================================

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HomomorphismObligation {
    pub kind: String,
    pub source: String,
    pub target: String,
}

// ============================================================
// Tests -- roundtrip and pinned-CID fixture
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn make_gap() -> TransportGapMemento {
        TransportGapMemento {
            fn_name: "gap:python:add:to:concept:add".into(),
            gap_kind: GapKind::PolymorphicSourceOp,
            kind: "TransportGapMemento".into(),
            reason: GapReason {
                divergent_tag: None,
                effects_delta: None,
                formal_sorts_delta: None,
                post_delta: None,
                pre_delta: None,
                source_supported: None,
                wp_rule_delta: None,
            },
            resolution_options: vec![ResolutionOption {
                option_kind: "accept-permanent".into(),
                precondition: None,
                loss: None,
                loss_severity: None,
                split_targets: None,
                respec_target_to: None,
                representation_map_delta: None,
                partial_morphism_cid: None,
                lossy_morphism_cid: None,
                dual_view_cid: None,
                tradeoff: "python:add is polymorphic; concept:add is integer-only; no bridge".into(),
                status: Some(OptionStatus::Recommended),
            }],
            schema_version: "1".into(),
            signature: Value::Null,
            source_lang: "python".into(),
            source_op_cid: Value::Null,
            target_concept_op: "concept:add".into(),
            target_op_cid: Value::Null,
            reason_note: None,
        }
    }

    #[test]
    fn transport_gap_memento_roundtrip() {
        let gap = make_gap();
        let json_str = serde_json::to_string(&gap).expect("serialize");
        let back: TransportGapMemento = serde_json::from_str(&json_str).expect("deserialize");
        assert_eq!(gap, back);
    }

    #[test]
    fn transport_gap_memento_kind_field() {
        let gap = make_gap();
        let v: serde_json::Value = serde_json::to_value(&gap).expect("to_value");
        assert_eq!(v["kind"], "TransportGapMemento");
        assert_eq!(v["schema_version"], "1");
        assert_eq!(v["gap_kind"], "polymorphic-source-op");
    }

    #[test]
    fn loss_record_successor_mint_absent_reads_as_none() {
        // A loss-record JSON without structural_divergence must still deserialize.
        let json_str = r#"{"value_divergence": {"kind": "atomic", "name": "true", "args": []}}"#;
        let lr: LossRecord = serde_json::from_str(json_str).expect("deserialize");
        assert!(lr.structural_divergence.is_none());
        assert!(lr.value_divergence.is_some());
    }

    #[test]
    fn loss_record_structural_divergence_roundtrip() {
        let mut lr = LossRecord::default();
        lr.structural_divergence = Some(json!({"kind": "atomic", "name": "uses_vtable", "args": []}));
        let json_str = serde_json::to_string(&lr).expect("serialize");
        let back: LossRecord = serde_json::from_str(&json_str).expect("deserialize");
        assert_eq!(lr, back);
    }

    #[test]
    fn gap_kind_no_such_concept_op_serializes() {
        let kind = GapKind::NoSuchConceptOp;
        let v: serde_json::Value = serde_json::to_value(&kind).expect("to_value");
        assert_eq!(v, "no-such-concept-op");
        let back: GapKind = serde_json::from_value(v).expect("from_value");
        assert_eq!(kind, back);
    }

    #[test]
    fn partial_morphism_memento_roundtrip() {
        let pm = PartialMorphismMemento::new(
            "partial-morphism:python:add:to:concept:add",
            "blake3-512:aaaa",
            "blake3-512:bbbb",
            json!({"kind": "atomic", "name": "operands_statically_int", "args": []}),
        );
        let json_str = serde_json::to_string(&pm).expect("serialize");
        let back: PartialMorphismMemento = serde_json::from_str(&json_str).expect("deserialize");
        assert_eq!(pm.kind, back.kind);
        assert_eq!(pm.schema_version, back.schema_version);
    }

    #[test]
    fn lossy_morphism_memento_roundtrip() {
        let lm = LossyMorphismMemento {
            coarsening_kind: "widen-target-postcondition".into(),
            fn_name: "lossy-morphism:python:add:to:c11:add@mod-w".into(),
            gap_memento_cid: None,
            homomorphism_obligation: HomomorphismObligation {
                kind: "wp-refinement-into-coarsening".into(),
                source: "blake3-512:cccc".into(),
                target: "blake3-512:dddd".into(),
            },
            kind: "LossyMorphismMemento".into(),
            literal_map: Value::Object(Default::default()),
            loss: LossRecord {
                value_divergence: Some(json!("c11_result == python_result mod 2^w")),
                ub_introduction: Some(json!("signed_overflow(add(lhs,rhs))")),
                domain_narrowing: None,
                effect_divergence: None,
                structural_divergence: None,
            },
            loss_severity: LossSeverity(BTreeMap::new()),
            operator_map: Value::Object(Default::default()),
            renaming_map: Value::Object(Default::default()),
            representation_map: Value::Object(Default::default()),
            schema_version: "1".into(),
            signature: Value::Null,
            source_contract_cid: "blake3-512:cccc".into(),
            target_shape_cid: "blake3-512:dddd".into(),
        };
        let json_str = serde_json::to_string(&lm).expect("serialize");
        let back: LossyMorphismMemento = serde_json::from_str(&json_str).expect("deserialize");
        assert_eq!(lm.kind, back.kind);
        assert_eq!(lm.schema_version, back.schema_version);
        assert_eq!(lm.loss.value_divergence, back.loss.value_divergence);
    }

    #[test]
    fn resolution_option_status_kebab_case() {
        let status = OptionStatus::Recommended;
        let v: serde_json::Value = serde_json::to_value(status).expect("to_value");
        assert_eq!(v, "recommended");
    }

    #[test]
    fn gap_kind_polymorphic_source_op_serializes() {
        let v: serde_json::Value =
            serde_json::to_value(GapKind::PolymorphicSourceOp).expect("to_value");
        assert_eq!(v, "polymorphic-source-op");
    }

    #[test]
    fn gap_kind_wp_rule_mismatch_serializes() {
        let v: serde_json::Value =
            serde_json::to_value(GapKind::WpRuleMismatch).expect("to_value");
        assert_eq!(v, "wp-rule-mismatch");
    }

    #[test]
    fn gap_kind_sort_mismatch_roundtrip() {
        let kind = GapKind::SortMismatch;
        let s = serde_json::to_string(&kind).expect("to_string");
        let back: GapKind = serde_json::from_str(&s).expect("from_str");
        assert_eq!(back, GapKind::SortMismatch);
        assert_eq!(s, "\"sort-mismatch\"");
    }

    #[test]
    fn gap_kind_missing_source_op_serializes() {
        let v: serde_json::Value =
            serde_json::to_value(GapKind::MissingSourceOp).expect("to_value");
        assert_eq!(v, "missing-source-op");
    }

    // ----------------------------------------------------------------
    // CID-pinning tests: deserialize Python-emitted fixture files, re-
    // serialize via Rust, compute BLAKE3-512, assert == catalog CID.
    //
    // These tests are the gate that was missing before this fix.  They
    // would have caught the field-order bug immediately:
    //   old: Rust serializes `reason_note` LAST (position 11),
    //        Python emits it at position 4 (alphabetic, JCS).
    //   new: Rust declaration order == JCS order → byte-identical.
    //
    // Fixture selection:
    //   1. gap_java_postdec  — has reason_note + formal_sorts_delta
    //   2. gap_python_ne     — has source_supported, no formal_sorts
    //   3. gap_rust_deref    — has pre_delta in reason
    // ----------------------------------------------------------------

    /// Read a catalog gap fixture file and return (memento_json_value, expected_cid).
    fn load_gap_fixture(filename_stem: &str) -> (serde_json::Value, String) {
        let manifest = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR");
        let gaps_dir = std::path::Path::new(&manifest)
            .join("../../../menagerie/concept-shapes/catalog/gaps");
        let entries = std::fs::read_dir(&gaps_dir)
            .unwrap_or_else(|e| panic!("cannot read gaps dir {:?}: {}", gaps_dir, e));
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name_str = name.to_string_lossy();
            if name_str.starts_with(filename_stem) && name_str.ends_with(".json") {
                let bytes = std::fs::read(entry.path()).expect("read fixture");
                let outer: serde_json::Value =
                    serde_json::from_slice(&bytes).expect("parse fixture JSON");
                let cid = outer["cid"].as_str().expect("cid field").to_string();
                let memento = outer["memento"].clone();
                return (memento, cid);
            }
        }
        panic!("fixture not found for stem: {}", filename_stem);
    }

    /// Deserialize a gap memento value into the Rust struct, serialize back, compare CID.
    fn assert_gap_cid_pins(fixture_stem: &str) {
        let (memento_val, expected_cid) = load_gap_fixture(fixture_stem);

        // Deserialize Python-emitted JSON into Rust struct.
        let memento: TransportGapMemento =
            serde_json::from_value(memento_val.clone()).unwrap_or_else(|e| {
                panic!("deserialize {} failed: {}", fixture_stem, e)
            });

        // Re-serialize via Rust and compute CID through JCS canonicalizer.
        // This is the primary federation gate: both Python (sort_keys=True) and
        // Rust (encode_jcs sorts keys) must produce the same CID from the same
        // key-value data.  If field declaration order in the Rust struct does NOT
        // match JCS order, the CID produced by plain serde_json::to_string would
        // differ from the JCS-based CID -- catching the bug the reviewer identified.
        let computed_cid = crate::canonical::serializable_cid(&memento)
            .unwrap_or_else(|e| panic!("serializable_cid {} failed: {}", fixture_stem, e));

        assert_eq!(
            computed_cid, expected_cid,
            "CID mismatch for {}: Rust re-serialization produces a different CID than \
             the Python-emitted catalog file.  This means the Rust JCS bytes diverge \
             from what the Python generator computed.  The field order fix in \
             transport_gap.rs (Blocker 1) may be incomplete.",
            fixture_stem
        );
    }

    /// Verify that the top-level TransportGapMemento struct fields are declared in
    /// JCS (lexicographic) order.  If they are, serde_json::to_string (declaration
    /// order) == serializable_jcs (sorted) for a struct with no opaque Value nesting.
    ///
    /// This test uses a hand-constructed memento with typed values only so that
    /// nested-Value insertion order does not interfere.
    #[test]
    fn transport_gap_top_level_field_order_is_jcs() {
        let memento = TransportGapMemento {
            fn_name: "gap:test:add:to:concept:add".into(),
            gap_kind: GapKind::SortMismatch,
            kind: "TransportGapMemento".into(),
            reason: GapReason {
                divergent_tag: None,
                effects_delta: None,
                formal_sorts_delta: None,
                post_delta: None,
                pre_delta: None,
                source_supported: Some(false),
                wp_rule_delta: None,
            },
            reason_note: Some("test reason".into()),
            resolution_options: vec![ResolutionOption {
                dual_view_cid: None,
                loss: None,
                loss_severity: None,
                lossy_morphism_cid: None,
                option_kind: "accept-permanent".into(),
                partial_morphism_cid: None,
                precondition: None,
                representation_map_delta: None,
                respec_target_to: None,
                split_targets: None,
                status: Some(OptionStatus::Recommended),
                tradeoff: "no bridge".into(),
            }],
            schema_version: "1".into(),
            signature: Value::Null,
            source_lang: "test".into(),
            source_op_cid: Value::Null,
            target_concept_op: "concept:add".into(),
            target_op_cid: Value::Null,
        };
        let rust_compact = serde_json::to_string(&memento).expect("to_string");
        let rust_jcs = crate::canonical::serializable_jcs(&memento)
            .expect("serializable_jcs");
        assert_eq!(
            rust_compact, rust_jcs,
            "Field declaration order != JCS order: serde_json::to_string differs from \
             encode_jcs output.  This means the struct fields are NOT in lexicographic \
             order.  The Blocker-1 fix is incomplete."
        );
    }

    /// Fixture 1: gap:java:postdec — has reason_note (was at position 11 before fix,
    /// JCS requires position 4) and formal_sorts_delta in reason.
    #[test]
    fn cid_pinning_gap_java_postdec_to_concept_postdec() {
        assert_gap_cid_pins("gap_java_postdec_to_concept_postdec");
    }

    /// Fixture 2: gap:python:ne — has source_supported in reason (tests GapReason order),
    /// and reason_note populated.
    #[test]
    fn cid_pinning_gap_python_ne_to_concept_ne() {
        assert_gap_cid_pins("gap_python_ne_to_concept_ne");
    }

    /// Fixture 3: gap:rust:deref — has pre_delta in reason (tests GapReason ordering
    /// for a different populated field).
    #[test]
    fn cid_pinning_gap_rust_deref_to_concept_deref() {
        assert_gap_cid_pins("gap_rust_deref_to_concept_deref");
    }

    // ----------------------------------------------------------------
    // Lock-by-construction tests for PartialMorphismMemento,
    // LossyMorphismMemento, and LossRecord.
    //
    // These tests mirror transport_gap_top_level_field_order_is_jcs and
    // extend the gate-promotion to cover the entire bug-class, not just
    // the TransportGapMemento instance where the bug was first caught.
    //
    // To planted-regression-test each:
    //   1. Swap two adjacent fields in the struct declaration.
    //   2. `cargo test -p libprovekit -- <test_name>` must FAIL.
    //   3. The other two lock-tests must still PASS.
    //   4. Revert the swap.
    // ----------------------------------------------------------------

    /// Verify that PartialMorphismMemento top-level fields are declared in JCS
    /// (lexicographic) order.  If they are, serde_json::to_string (declaration
    /// order) == serializable_jcs (sorted) for a struct with only typed fields.
    #[test]
    fn partial_morphism_top_level_field_order_is_jcs() {
        let memento = PartialMorphismMemento {
            fn_name: "partial:test:add:to:concept:add".into(),
            gap_memento_cid: None,
            homomorphism_obligation: HomomorphismObligation {
                kind: "wp-refinement-under-precondition".into(),
                source: "blake3-512:aaaa".into(),
                target: "blake3-512:bbbb".into(),
            },
            kind: "PartialMorphismMemento".into(),
            literal_map: Value::Object(Default::default()),
            operator_map: Value::Object(Default::default()),
            renaming_map: Value::Object(Default::default()),
            representation_map: Value::Object(Default::default()),
            schema_version: "1".into(),
            signature: Value::Null,
            source_contract_cid: "blake3-512:aaaa".into(),
            target_shape_cid: "blake3-512:bbbb".into(),
            validity_precondition: Value::Bool(true),
        };
        let rust_compact = serde_json::to_string(&memento).expect("to_string");
        let rust_jcs = crate::canonical::serializable_jcs(&memento)
            .expect("serializable_jcs");
        assert_eq!(
            rust_compact, rust_jcs,
            "Field declaration order != JCS order: serde_json::to_string differs from \
             encode_jcs output for PartialMorphismMemento.  Struct fields are NOT in \
             lexicographic order -- fix the declaration order."
        );
    }

    /// Verify that LossyMorphismMemento top-level fields are declared in JCS
    /// (lexicographic) order.
    #[test]
    fn lossy_morphism_top_level_field_order_is_jcs() {
        let memento = LossyMorphismMemento {
            coarsening_kind: "widen-target-postcondition".into(),
            fn_name: "lossy:test:add:to:c11:add".into(),
            gap_memento_cid: None,
            homomorphism_obligation: HomomorphismObligation {
                kind: "wp-refinement-into-coarsening".into(),
                source: "blake3-512:cccc".into(),
                target: "blake3-512:dddd".into(),
            },
            kind: "LossyMorphismMemento".into(),
            literal_map: Value::Object(Default::default()),
            loss: LossRecord::default(),
            loss_severity: LossSeverity(BTreeMap::new()),
            operator_map: Value::Object(Default::default()),
            renaming_map: Value::Object(Default::default()),
            representation_map: Value::Object(Default::default()),
            schema_version: "1".into(),
            signature: Value::Null,
            source_contract_cid: "blake3-512:cccc".into(),
            target_shape_cid: "blake3-512:dddd".into(),
        };
        let rust_compact = serde_json::to_string(&memento).expect("to_string");
        let rust_jcs = crate::canonical::serializable_jcs(&memento)
            .expect("serializable_jcs");
        assert_eq!(
            rust_compact, rust_jcs,
            "Field declaration order != JCS order: serde_json::to_string differs from \
             encode_jcs output for LossyMorphismMemento.  Struct fields are NOT in \
             lexicographic order -- fix the declaration order."
        );
    }

    /// Verify that LossRecord top-level fields are declared in JCS (lexicographic)
    /// order.  LossRecord is a JCS-locked struct (embedded in every lossy memento)
    /// and belongs in the same gate-promotion.
    ///
    /// This test uses scalar Value::String entries (not nested Value::Object maps)
    /// so that nested-Value insertion order does not interfere with the assertion.
    /// The goal is to catch top-level declaration-order drift, not sub-key order.
    #[test]
    fn loss_record_top_level_field_order_is_jcs() {
        let lr = LossRecord {
            domain_narrowing: Some(Value::String("narrow".into())),
            effect_divergence: None,
            structural_divergence: None,
            ub_introduction: None,
            value_divergence: Some(Value::String("diff".into())),
        };
        let rust_compact = serde_json::to_string(&lr).expect("to_string");
        let rust_jcs = crate::canonical::serializable_jcs(&lr)
            .expect("serializable_jcs");
        assert_eq!(
            rust_compact, rust_jcs,
            "Field declaration order != JCS order: serde_json::to_string differs from \
             encode_jcs output for LossRecord.  Struct fields are NOT in \
             lexicographic order -- fix the declaration order."
        );
    }
}
