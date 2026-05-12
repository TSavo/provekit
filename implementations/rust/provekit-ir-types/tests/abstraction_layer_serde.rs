// SPDX-License-Identifier: Apache-2.0
//
// Round-trip serde tests for the abstraction-layer types.
//
// Source of truth:
//   protocol/specs/2026-05-15-concept-hub-abstraction-layer.md §2.1, §2.2, §2.4
//   protocol/provekit-ir.cddl  (ConceptAbstractionMemento, RealizationDesugaringMemento)
//
// These tests pin:
//   * The Rust abstraction-layer types deserialize from the wire shape the
//     spec defines.
//   * Round-trip parity at the serde layer: parse -> serialize -> parse
//     yields the same value.
//   * Optional fields (contract_note, superseded_by, refines, pre,
//     discharge_receipt) are absent from the serialized JSON when None.
//   * `effects: []` is always present in the serialized JSON even when empty.
//   * `LossRecord` with `structural_divergence` round-trips correctly.
//
// Byte-exact CID pinning lives in
//   provekit-claim-envelope/tests/abstraction_layer_cid_pin.rs
// because this crate has no JCS encoder.

use std::collections::BTreeMap;

use provekit_ir_types::{
    AbstractionSlot, ConceptAbstractionMemento, IrFormula, LossRecord,
    RealizationDesugaringMemento,
};

// ================================================================
// concept:dynamic-dispatch -- ConceptAbstractionMemento fixture
// ================================================================

const DYNAMIC_DISPATCH_JSON: &str = r#"{
  "kind": "concept-abstraction",
  "operator": "concept:dynamic-dispatch",
  "tier": "abstraction",
  "slots": [
    {"name": "receiver"},
    {"name": "method_name"},
    {"name": "args", "variadic": true}
  ],
  "formal_sorts": [
    "blake3-512:sort0000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000",
    "blake3-512:sort1111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111",
    "blake3-512:sort2222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222"
  ],
  "result_sort": "blake3-512:sort3333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333",
  "contract": {
    "kind": "choice",
    "varName": "m",
    "sort": {"kind": "primitive", "name": "Int"},
    "body": {
      "kind": "and",
      "operands": [
        {"kind": "atomic", "name": "defined", "args": [{"kind": "var", "name": "m"}]},
        {"kind": "atomic", "name": "wp_call", "args": [{"kind": "var", "name": "m"}, {"kind": "var", "name": "receiver"}]}
      ]
    }
  },
  "contract_note": "the call result and effect equal those of the method that resolves from the receiver runtime type for method_name applied to receiver and args; if no such method resolves the behaviour is undefined",
  "realizations": []
}"#;

#[test]
fn dynamic_dispatch_deserializes_from_spec_shape() {
    let m: ConceptAbstractionMemento =
        serde_json::from_str(DYNAMIC_DISPATCH_JSON).expect("parse concept:dynamic-dispatch");

    assert_eq!(m.kind, "concept-abstraction");
    assert_eq!(m.operator, "concept:dynamic-dispatch");
    assert_eq!(m.tier, "abstraction");
    assert_eq!(m.slots.len(), 3);
    assert_eq!(m.slots[0].name, "receiver");
    assert_eq!(m.slots[0].variadic, None);
    assert_eq!(m.slots[1].name, "method_name");
    assert_eq!(m.slots[2].name, "args");
    assert_eq!(m.slots[2].variadic, Some(true));
    assert_eq!(m.formal_sorts.len(), 3);
    assert!(m.formal_sorts[0].starts_with("blake3-512:"));
    assert!(m.result_sort.starts_with("blake3-512:"));
    assert!(m.contract_note.is_some());
    assert_eq!(m.realizations.len(), 0);
    assert!(m.superseded_by.is_none());
    assert!(m.refines.is_none());
}

#[test]
fn dynamic_dispatch_round_trips() {
    let m1: ConceptAbstractionMemento =
        serde_json::from_str(DYNAMIC_DISPATCH_JSON).expect("parse");
    let serialized = serde_json::to_string(&m1).expect("serialize");
    let m2: ConceptAbstractionMemento =
        serde_json::from_str(&serialized).expect("re-parse");
    assert_eq!(m1, m2);
}

#[test]
fn concept_abstraction_optional_fields_absent_when_none() {
    // Build a minimal ConceptAbstractionMemento with no optional fields.
    let m = ConceptAbstractionMemento {
        kind: "concept-abstraction".into(),
        operator: "concept:dynamic-dispatch".into(),
        tier: "abstraction".into(),
        slots: vec![AbstractionSlot {
            name: "receiver".into(),
            variadic: None,
        }],
        formal_sorts: vec!["blake3-512:sort0000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000".into()],
        result_sort: "blake3-512:sort1111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111".into(),
        contract: IrFormula::Atomic {
            name: "true".into(),
            args: vec![],
        },
        contract_note: None,
        realizations: vec![],
        superseded_by: None,
        refines: None,
    };

    let json = serde_json::to_string(&m).expect("serialize");
    assert!(!json.contains("contract_note"), "contract_note must be absent when None");
    assert!(!json.contains("superseded_by"), "superseded_by must be absent when None");
    assert!(!json.contains("refines"), "refines must be absent when None");
    assert!(!json.contains("variadic"), "variadic must be absent when None");
}

// ================================================================
// concept:double-dispatch -- ConceptAbstractionMemento fixture
// ================================================================

const DOUBLE_DISPATCH_JSON: &str = r#"{
  "kind": "concept-abstraction",
  "operator": "concept:double-dispatch",
  "tier": "abstraction",
  "slots": [
    {"name": "receiver"},
    {"name": "secondary"},
    {"name": "method_name"},
    {"name": "args", "variadic": true}
  ],
  "formal_sorts": [
    "blake3-512:sort0000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000",
    "blake3-512:sort0000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000",
    "blake3-512:sort1111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111",
    "blake3-512:sort2222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222"
  ],
  "result_sort": "blake3-512:sort3333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333",
  "contract": {
    "kind": "choice",
    "varName": "m",
    "sort": {"kind": "primitive", "name": "Int"},
    "body": {
      "kind": "and",
      "operands": [
        {"kind": "atomic", "name": "defined", "args": [{"kind": "var", "name": "m"}]},
        {"kind": "atomic", "name": "wp_call", "args": [{"kind": "var", "name": "m"}, {"kind": "var", "name": "receiver"}, {"kind": "var", "name": "secondary"}]}
      ]
    }
  },
  "contract_note": "dispatch is resolved from the conjunction of the receiver runtime type and the secondary runtime type for method_name; if no such method resolves the behaviour is undefined",
  "realizations": []
}"#;

#[test]
fn double_dispatch_deserializes_from_spec_shape() {
    let m: ConceptAbstractionMemento =
        serde_json::from_str(DOUBLE_DISPATCH_JSON).expect("parse concept:double-dispatch");

    assert_eq!(m.kind, "concept-abstraction");
    assert_eq!(m.operator, "concept:double-dispatch");
    assert_eq!(m.tier, "abstraction");
    assert_eq!(m.slots.len(), 4);
    assert_eq!(m.slots[0].name, "receiver");
    assert_eq!(m.slots[1].name, "secondary");
    assert_eq!(m.slots[2].name, "method_name");
    assert_eq!(m.slots[3].name, "args");
    assert_eq!(m.slots[3].variadic, Some(true));
    assert_eq!(m.realizations.len(), 0);
}

#[test]
fn double_dispatch_round_trips() {
    let m1: ConceptAbstractionMemento =
        serde_json::from_str(DOUBLE_DISPATCH_JSON).expect("parse");
    let serialized = serde_json::to_string(&m1).expect("serialize");
    let m2: ConceptAbstractionMemento =
        serde_json::from_str(&serialized).expect("re-parse");
    assert_eq!(m1, m2);
}

// ================================================================
// RealizationDesugaringMemento -- double-dispatch x {c, java, ruby}
// ================================================================

const DD_TO_C_JSON: &str = r#"{
  "kind": "equation",
  "fn_name": "concept:double-dispatch->c11:2d-fn-ptr-table",
  "formals": ["receiver", "secondary", "method_name", "args"],
  "formal_sorts": [
    "blake3-512:sort0000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000",
    "blake3-512:sort0000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000",
    "blake3-512:sort1111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111",
    "blake3-512:sort2222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222"
  ],
  "pre": {
    "kind": "atomic",
    "name": "static_dispatch_table",
    "args": [{"kind": "var", "name": "receiver"}, {"kind": "var", "name": "secondary"}]
  },
  "post": {
    "lhs": {
      "kind": "atomic",
      "name": "concept:double-dispatch",
      "args": [
        {"kind": "var", "name": "receiver"},
        {"kind": "var", "name": "secondary"},
        {"kind": "var", "name": "method_name"},
        {"kind": "var", "name": "args"}
      ]
    },
    "rhs": {
      "kind": "atomic",
      "name": "concept:call",
      "args": [
        {"kind": "ctor", "name": "concept:cast", "args": [
          {"kind": "ctor", "name": "concept:index", "args": [
            {"kind": "ctor", "name": "concept:index", "args": [
              {"kind": "ctor", "name": "concept:member", "args": [
                {"kind": "var", "name": "receiver"},
                {"kind": "const", "sort": {"kind": "primitive", "name": "String"}, "value": "dispatch_tbl"}
              ]},
              {"kind": "ctor", "name": "concept:tag-of", "args": [{"kind": "var", "name": "receiver"}]}
            ]},
            {"kind": "ctor", "name": "concept:tag-of", "args": [{"kind": "var", "name": "secondary"}]}
          ]},
          {"kind": "const", "sort": {"kind": "primitive", "name": "String"}, "value": "fn_ptr_2d"}
        ]},
        {"kind": "var", "name": "receiver"},
        {"kind": "var", "name": "secondary"},
        {"kind": "var", "name": "args"}
      ]
    }
  },
  "role": "abstraction-realization",
  "direction": "left-to-right",
  "target_lang": "c11",
  "loss_record": {
    "domain_narrowing": {
      "kind": "atomic",
      "name": "requires_static_2d_dispatch_table",
      "args": [{"kind": "var", "name": "receiver"}, {"kind": "var", "name": "secondary"}]
    },
    "structural_divergence": {
      "kind": "atomic",
      "name": "open_coded_vtable_replaces_single_op",
      "args": [
        {"kind": "ctor", "name": "concept:index", "args": [{"kind": "var", "name": "receiver"}]},
        {"kind": "ctor", "name": "concept:index", "args": [{"kind": "var", "name": "secondary"}]},
        {"kind": "ctor", "name": "concept:cast", "args": []}
      ]
    },
    "ub_introduction": {
      "kind": "atomic",
      "name": "out_of_range_tag_is_ub",
      "args": [{"kind": "var", "name": "receiver"}, {"kind": "var", "name": "secondary"}]
    }
  },
  "effects": []
}"#;

const DD_TO_JAVA_JSON: &str = r#"{
  "kind": "equation",
  "fn_name": "concept:double-dispatch->jvm:visitor-pattern",
  "formals": ["receiver", "secondary", "method_name", "args"],
  "formal_sorts": [
    "blake3-512:sort0000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000",
    "blake3-512:sort0000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000",
    "blake3-512:sort1111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111",
    "blake3-512:sort2222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222"
  ],
  "post": {
    "lhs": {
      "kind": "atomic",
      "name": "concept:double-dispatch",
      "args": [
        {"kind": "var", "name": "receiver"},
        {"kind": "var", "name": "secondary"},
        {"kind": "var", "name": "method_name"},
        {"kind": "var", "name": "args"}
      ]
    },
    "rhs": {
      "kind": "atomic",
      "name": "concept:seq",
      "args": [
        {"kind": "ctor", "name": "concept:itab-method", "args": [
          {"kind": "var", "name": "receiver"},
          {"kind": "const", "sort": {"kind": "primitive", "name": "String"}, "value": "accept"},
          {"kind": "var", "name": "secondary"}
        ]},
        {"kind": "ctor", "name": "concept:itab-method", "args": [
          {"kind": "var", "name": "secondary"},
          {"kind": "const", "sort": {"kind": "primitive", "name": "String"}, "value": "visit_receiver_type"},
          {"kind": "var", "name": "receiver"},
          {"kind": "var", "name": "args"}
        ]}
      ]
    }
  },
  "role": "abstraction-realization",
  "direction": "left-to-right",
  "target_lang": "java",
  "loss_record": {
    "domain_narrowing": {
      "kind": "atomic",
      "name": "visitable_set_fixed_at_declaration",
      "args": [{"kind": "var", "name": "receiver"}, {"kind": "var", "name": "secondary"}]
    },
    "structural_divergence": {
      "kind": "atomic",
      "name": "visitor_accept_visit_indirection",
      "args": [
        {"kind": "ctor", "name": "concept:itab-method", "args": [{"kind": "var", "name": "receiver"}]},
        {"kind": "ctor", "name": "concept:itab-method", "args": [{"kind": "var", "name": "secondary"}]}
      ]
    }
  },
  "effects": []
}"#;

const DD_TO_RUBY_JSON: &str = r#"{
  "kind": "equation",
  "fn_name": "concept:double-dispatch->ruby:case-type-tuple",
  "formals": ["receiver", "secondary", "method_name", "args"],
  "formal_sorts": [
    "blake3-512:sort0000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000",
    "blake3-512:sort0000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000",
    "blake3-512:sort1111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111",
    "blake3-512:sort2222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222"
  ],
  "post": {
    "lhs": {
      "kind": "atomic",
      "name": "concept:double-dispatch",
      "args": [
        {"kind": "var", "name": "receiver"},
        {"kind": "var", "name": "secondary"},
        {"kind": "var", "name": "method_name"},
        {"kind": "var", "name": "args"}
      ]
    },
    "rhs": {
      "kind": "atomic",
      "name": "concept:match",
      "args": [
        {"kind": "ctor", "name": "concept:pair", "args": [
          {"kind": "ctor", "name": "concept:type-of", "args": [{"kind": "var", "name": "receiver"}]},
          {"kind": "ctor", "name": "concept:type-of", "args": [{"kind": "var", "name": "secondary"}]}
        ]},
        {"kind": "ctor", "name": "concept:match-arm", "args": [
          {"kind": "ctor", "name": "concept:pair", "args": [
            {"kind": "ctor", "name": "concept:tag-of", "args": [{"kind": "var", "name": "receiver"}]},
            {"kind": "ctor", "name": "concept:tag-of", "args": [{"kind": "var", "name": "secondary"}]}
          ]},
          {"kind": "ctor", "name": "concept:call", "args": [
            {"kind": "var", "name": "method_name"},
            {"kind": "var", "name": "receiver"},
            {"kind": "var", "name": "secondary"},
            {"kind": "var", "name": "args"}
          ]}
        ]},
        {"kind": "ctor", "name": "concept:raise", "args": [
          {"kind": "const", "sort": {"kind": "primitive", "name": "String"}, "value": "TypeError"}
        ]}
      ]
    }
  },
  "role": "abstraction-realization",
  "direction": "left-to-right",
  "target_lang": "ruby",
  "loss_record": {
    "structural_divergence": {
      "kind": "atomic",
      "name": "case_fallthrough_narrows_open_dispatch",
      "args": [
        {"kind": "ctor", "name": "concept:raise", "args": []}
      ]
    }
  },
  "effects": []
}"#;

#[test]
fn realization_c_deserializes_and_round_trips() {
    let m: RealizationDesugaringMemento =
        serde_json::from_str(DD_TO_C_JSON).expect("parse dd->c11");

    assert_eq!(m.kind, "equation");
    assert_eq!(m.fn_name, "concept:double-dispatch->c11:2d-fn-ptr-table");
    assert_eq!(m.role, "abstraction-realization");
    assert_eq!(m.direction, "left-to-right");
    assert_eq!(m.target_lang, "c11");
    assert!(m.pre.is_some(), "c11 realization has a pre-condition");
    assert!(m.discharge_receipt.is_none(), "discharge_receipt absent in PR1");
    assert_eq!(m.effects, Vec::<String>::new(), "effects must be empty slice");

    // structural_divergence, domain_narrowing AND ub_introduction all set for C (heaviest end)
    assert!(
        m.loss_record.0.contains_key("structural_divergence"),
        "C realization must have structural_divergence"
    );
    assert!(
        m.loss_record.0.contains_key("domain_narrowing"),
        "C realization must have domain_narrowing"
    );
    assert!(
        m.loss_record.0.contains_key("ub_introduction"),
        "C realization must have ub_introduction (out-of-range tag -> UB)"
    );

    // Round-trip
    let s = serde_json::to_string(&m).expect("serialize");
    let m2: RealizationDesugaringMemento = serde_json::from_str(&s).expect("re-parse");
    assert_eq!(m, m2);
}

#[test]
fn realization_java_deserializes_and_round_trips() {
    let m: RealizationDesugaringMemento =
        serde_json::from_str(DD_TO_JAVA_JSON).expect("parse dd->java");

    assert_eq!(m.kind, "equation");
    assert_eq!(m.fn_name, "concept:double-dispatch->jvm:visitor-pattern");
    assert_eq!(m.target_lang, "java");
    assert!(m.pre.is_none(), "java realization has no pre-condition");

    // structural_divergence AND domain_narrowing set for Java (mid: visitor adds accept/visit
    // indirection; visitable-set is fixed at interface declaration time)
    assert!(
        m.loss_record.0.contains_key("structural_divergence"),
        "Java realization must have structural_divergence"
    );
    assert!(
        m.loss_record.0.contains_key("domain_narrowing"),
        "Java realization must have domain_narrowing (visitable-set fixed at declaration)"
    );
    // Java must NOT have ub_introduction (that's the heavy C end only)
    assert!(
        !m.loss_record.0.contains_key("ub_introduction"),
        "Java realization must NOT have ub_introduction"
    );

    let s = serde_json::to_string(&m).expect("serialize");
    let m2: RealizationDesugaringMemento = serde_json::from_str(&s).expect("re-parse");
    assert_eq!(m, m2);
}

#[test]
fn realization_ruby_deserializes_and_round_trips() {
    let m: RealizationDesugaringMemento =
        serde_json::from_str(DD_TO_RUBY_JSON).expect("parse dd->ruby");

    assert_eq!(m.kind, "equation");
    assert_eq!(m.fn_name, "concept:double-dispatch->ruby:case-type-tuple");
    assert_eq!(m.target_lang, "ruby");
    assert!(m.pre.is_none(), "ruby realization has no pre-condition");

    // structural_divergence only, near-zero (Ruby writes the implication structure directly
    // via a case-match over the type tuple -- the realization ≈ the contract)
    assert!(
        m.loss_record.0.contains_key("structural_divergence"),
        "Ruby realization must have structural_divergence"
    );
    // Ruby must NOT have domain_narrowing (no fixed interface required) or ub_introduction
    assert!(
        !m.loss_record.0.contains_key("domain_narrowing"),
        "Ruby realization must NOT have domain_narrowing"
    );
    assert!(
        !m.loss_record.0.contains_key("ub_introduction"),
        "Ruby realization must NOT have ub_introduction"
    );

    let s = serde_json::to_string(&m).expect("serialize");
    let m2: RealizationDesugaringMemento = serde_json::from_str(&s).expect("re-parse");
    assert_eq!(m, m2);
}

#[test]
fn realization_effects_always_present_in_json() {
    // effects: [] is a required field; it must appear in the serialized JSON
    // even when the Vec is empty. This test guards against accidental
    // `#[serde(skip_serializing_if = "Vec::is_empty")]` on that field.
    let m: RealizationDesugaringMemento =
        serde_json::from_str(DD_TO_RUBY_JSON).expect("parse");
    let json = serde_json::to_string(&m).expect("serialize");
    assert!(
        json.contains("\"effects\":"),
        "effects field MUST always be present in serialized JSON (even when empty)"
    );
    assert!(
        json.contains("\"effects\":[]"),
        "effects field MUST serialize as empty array []"
    );
}

#[test]
fn loss_record_structural_divergence_round_trips() {
    // Verify LossRecord with structural_divergence serializes and round-trips.
    let mut map = BTreeMap::new();
    map.insert(
        "structural_divergence".to_string(),
        IrFormula::Atomic {
            name: "true".into(),
            args: vec![],
        },
    );
    let lr = LossRecord(map);

    let json = serde_json::to_string(&lr).expect("serialize");
    assert!(json.contains("structural_divergence"), "structural_divergence must appear in JSON");

    let lr2: LossRecord = serde_json::from_str(&json).expect("re-parse");
    assert_eq!(lr, lr2);
}

#[test]
fn loss_record_empty_round_trips() {
    // An empty LossRecord (no dimensions set) is valid: all dimensions = ∅.
    let lr = LossRecord::default();
    let json = serde_json::to_string(&lr).expect("serialize");
    let lr2: LossRecord = serde_json::from_str(&json).expect("re-parse");
    assert_eq!(lr, lr2);
}
