// SPDX-License-Identifier: Apache-2.0
//
// Round-trip and CID recompute tests for the parametric realization substrate.
//
// Source of truth:
//   protocol/specs/2026-05-13-parametric-realization.md §1

use std::sync::Arc;

use serde_json::{json, Value as Json};
use sugar_canonicalizer::{blake3_512_of, encode_jcs, Value};
use sugar_ir_types::{
    EffectSlotDescriptor, ParametricRealizationMemento, RealizationPlanMemento, SlotDescriptor,
};

const BODY_TEMPLATE_CID: &str = "blake3-512:111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111";
const SUGAR_CID: &str = "blake3-512:22222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222";
const PARAM_PROVENANCE_CID: &str = "blake3-512:33333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333";
const SORT_MORPHISM_CID: &str = "blake3-512:44444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444";
const CANDIDATE_SET_CID: &str = "blake3-512:55555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555";
const CONCEPT_SITE_CID: &str = "blake3-512:66666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666";
const LOSS_FUNCTION_CID: &str = "blake3-512:77777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777";
const PLAN_PROVENANCE_CID: &str = "blake3-512:88888888888888888888888888888888888888888888888888888888888888888888888888888888888888888888888888888888888888888888888888888888";
const SELECTED_CANDIDATE_CID: &str = "blake3-512:99999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999";
const SELECTED_REALIZATION_CID: &str = "blake3-512:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

const PARAMETRIC_REALIZATION_CID: &str = "blake3-512:0ade364c6dd99155b5ac2c8c37beb8206dbab8b55059858725b568d250e45d80344b7621f00cda5b0dcd72849b1d04805149dc4f70045b3d97e4b8cc7858a83e";
const REALIZATION_PLAN_CID: &str = "blake3-512:4f5e0ecf43a406ec3c655d8fab0efda0042928dc0c58e444423eda5b73383d5f01ae2468d7e0eb7b8ee35e832e4837567a16a3a382e44314f20f8d8b607458f0";

fn json_to_value(j: &Json) -> Arc<Value> {
    match j {
        Json::Null => Value::null(),
        Json::Bool(b) => Value::boolean(*b),
        Json::Number(n) => {
            if let Some(i) = n.as_i64() {
                Value::integer(i)
            } else if let Some(u) = n.as_u64() {
                Value::integer(u as i64)
            } else {
                panic!("fixture uses a non-integer JSON number")
            }
        }
        Json::String(s) => Value::string(s.clone()),
        Json::Array(items) => Value::array(items.iter().map(json_to_value).collect()),
        Json::Object(map) => Arc::new(Value::Object(
            map.iter()
                .map(|(k, v)| (k.clone(), json_to_value(v)))
                .collect(),
        )),
    }
}

fn cid_for<T: serde::Serialize>(memento: &T) -> String {
    let json = serde_json::to_value(memento).expect("serialize to JSON");
    let canonical = encode_jcs(&json_to_value(&json));
    blake3_512_of(canonical.as_bytes())
}

fn parametric_example() -> ParametricRealizationMemento {
    ParametricRealizationMemento {
        body_template_cids: vec![BODY_TEMPLATE_CID.to_string()],
        concept_pattern: json!({"args": ["T"], "head": "concept:option"}),
        effect_transform_slots: vec![EffectSlotDescriptor {
            concept_effect: "concept:alloc".to_string(),
            slot_name: "effect-slot-0".to_string(),
            target_effect: "java:allocation".to_string(),
        }],
        loss_record_template: json!({"structural_divergence": "template-only"}),
        provenance_cid: PARAM_PROVENANCE_CID.to_string(),
        required_sort_morphism_slots: vec![SlotDescriptor {
            slot_name: "T_to_U".to_string(),
            source_type_variable: "T".to_string(),
            target_type_variable: "U".to_string(),
        }],
        sugar_cids: vec![SUGAR_CID.to_string()],
        target_pattern: json!({"args": ["U"], "head": "java:Optional"}),
        type_variables: vec!["T".to_string(), "U".to_string()],
    }
}

fn plan_example() -> RealizationPlanMemento {
    RealizationPlanMemento {
        candidate_set_cid: CANDIDATE_SET_CID.to_string(),
        concept_site_cid: CONCEPT_SITE_CID.to_string(),
        effect_occurrence_transform: json!({"occurrence:0": "java:allocation"}),
        loss_function_cid: LOSS_FUNCTION_CID.to_string(),
        observation_wrapper_cid: None,
        provenance_cid: PLAN_PROVENANCE_CID.to_string(),
        selected_candidate_cid: SELECTED_CANDIDATE_CID.to_string(),
        selected_realization_cid: SELECTED_REALIZATION_CID.to_string(),
        sort_morphism_cids: vec![SORT_MORPHISM_CID.to_string()],
        total_loss_record: json!({"structural_divergence": 1}),
    }
}

#[test]
fn parametric_realization_round_trips_and_recomputes_cid() {
    let m1 = parametric_example();
    let serialized = serde_json::to_string(&m1).expect("serialize");

    assert_eq!(
        serialized,
        r#"{"body_template_cids":["blake3-512:111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111"],"concept_pattern":{"args":["T"],"head":"concept:option"},"effect_transform_slots":[{"concept_effect":"concept:alloc","slot_name":"effect-slot-0","target_effect":"java:allocation"}],"loss_record_template":{"structural_divergence":"template-only"},"provenance_cid":"blake3-512:33333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333","required_sort_morphism_slots":[{"slot_name":"T_to_U","source_type_variable":"T","target_type_variable":"U"}],"sugar_cids":["blake3-512:22222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222"],"target_pattern":{"args":["U"],"head":"java:Optional"},"type_variables":["T","U"]}"#
    );

    let m2: ParametricRealizationMemento = serde_json::from_str(&serialized).expect("re-parse");
    assert_eq!(m1, m2);
    assert_eq!(cid_for(&m1), PARAMETRIC_REALIZATION_CID);
}

#[test]
fn realization_plan_round_trips_and_recomputes_cid() {
    let m1 = plan_example();
    let serialized = serde_json::to_string(&m1).expect("serialize");

    assert_eq!(
        serialized,
        r#"{"candidate_set_cid":"blake3-512:55555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555","concept_site_cid":"blake3-512:66666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666","effect_occurrence_transform":{"occurrence:0":"java:allocation"},"loss_function_cid":"blake3-512:77777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777777","observation_wrapper_cid":null,"provenance_cid":"blake3-512:88888888888888888888888888888888888888888888888888888888888888888888888888888888888888888888888888888888888888888888888888888888","selected_candidate_cid":"blake3-512:99999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999","selected_realization_cid":"blake3-512:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","sort_morphism_cids":["blake3-512:44444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444"],"total_loss_record":{"structural_divergence":1}}"#
    );

    let m2: RealizationPlanMemento = serde_json::from_str(&serialized).expect("re-parse");
    assert_eq!(m1, m2);
    assert_eq!(cid_for(&m1), REALIZATION_PLAN_CID);
}

#[test]
fn parametric_realization_rejects_empty_type_variables() {
    use sugar_ir_types::{
        ParametricRealizationError, ParametricRealizationMemento, SlotDescriptor,
    };
    let m = ParametricRealizationMemento {
        body_template_cids: vec![],
        concept_pattern: serde_json::json!({}),
        effect_transform_slots: vec![],
        loss_record_template: serde_json::json!({}),
        provenance_cid: "blake3-512:0".repeat(1),
        required_sort_morphism_slots: vec![SlotDescriptor {
            slot_name: "s".to_string(),
            source_type_variable: "T".to_string(),
            target_type_variable: "U".to_string(),
        }],
        sugar_cids: vec![],
        target_pattern: serde_json::json!({}),
        type_variables: vec![],
    };
    assert_eq!(
        m.validate(),
        Err(ParametricRealizationError::EmptyTypeVariables)
    );
}

#[test]
fn parametric_realization_rejects_empty_required_slots() {
    use sugar_ir_types::{ParametricRealizationError, ParametricRealizationMemento};
    let m = ParametricRealizationMemento {
        body_template_cids: vec![],
        concept_pattern: serde_json::json!({}),
        effect_transform_slots: vec![],
        loss_record_template: serde_json::json!({}),
        provenance_cid: "blake3-512:0".repeat(1),
        required_sort_morphism_slots: vec![],
        sugar_cids: vec![],
        target_pattern: serde_json::json!({}),
        type_variables: vec!["T".to_string()],
    };
    assert_eq!(
        m.validate(),
        Err(ParametricRealizationError::EmptyRequiredSortMorphismSlots)
    );
}

#[test]
fn parametric_realization_rejects_slot_referencing_unknown_type_variable() {
    use sugar_ir_types::{
        ParametricRealizationError, ParametricRealizationMemento, SlotDescriptor,
    };
    let m = ParametricRealizationMemento {
        body_template_cids: vec![],
        concept_pattern: serde_json::json!({}),
        effect_transform_slots: vec![],
        loss_record_template: serde_json::json!({}),
        provenance_cid: "blake3-512:0".repeat(1),
        required_sort_morphism_slots: vec![SlotDescriptor {
            slot_name: "s".to_string(),
            source_type_variable: "T".to_string(),
            target_type_variable: "UnknownU".to_string(),
        }],
        sugar_cids: vec![],
        target_pattern: serde_json::json!({}),
        type_variables: vec!["T".to_string(), "U".to_string()],
    };
    assert_eq!(
        m.validate(),
        Err(
            ParametricRealizationError::SlotReferencesUnknownTypeVariable {
                slot_name: "s".to_string(),
                variable: "UnknownU".to_string(),
                side: "target",
            }
        )
    );
}

#[test]
fn realization_plan_validates_against_matching_realization() {
    use sugar_ir_types::{ParametricRealizationMemento, RealizationPlanMemento, SlotDescriptor};
    let realization = ParametricRealizationMemento {
        body_template_cids: vec![],
        concept_pattern: serde_json::json!({}),
        effect_transform_slots: vec![],
        loss_record_template: serde_json::json!({}),
        provenance_cid: "blake3-512:0".repeat(1),
        required_sort_morphism_slots: vec![
            SlotDescriptor {
                slot_name: "s1".to_string(),
                source_type_variable: "T".to_string(),
                target_type_variable: "U".to_string(),
            },
            SlotDescriptor {
                slot_name: "s2".to_string(),
                source_type_variable: "T".to_string(),
                target_type_variable: "U".to_string(),
            },
        ],
        sugar_cids: vec![],
        target_pattern: serde_json::json!({}),
        type_variables: vec!["T".to_string(), "U".to_string()],
    };
    let plan = RealizationPlanMemento {
        candidate_set_cid: "blake3-512:c".repeat(1),
        concept_site_cid: "blake3-512:s".repeat(1),
        effect_occurrence_transform: serde_json::json!({}),
        loss_function_cid: "blake3-512:l".repeat(1),
        observation_wrapper_cid: None,
        provenance_cid: "blake3-512:p".repeat(1),
        selected_candidate_cid: "blake3-512:x".repeat(1),
        selected_realization_cid: "blake3-512:r".repeat(1),
        sort_morphism_cids: vec!["blake3-512:m1".to_string(), "blake3-512:m2".to_string()],
        total_loss_record: serde_json::json!({}),
    };
    plan.validate_against(&realization)
        .expect("matching counts");
}

#[test]
fn realization_plan_rejects_sort_morphism_count_mismatch() {
    use sugar_ir_types::{
        ParametricRealizationMemento, RealizationPlanError, RealizationPlanMemento, SlotDescriptor,
    };
    let realization = ParametricRealizationMemento {
        body_template_cids: vec![],
        concept_pattern: serde_json::json!({}),
        effect_transform_slots: vec![],
        loss_record_template: serde_json::json!({}),
        provenance_cid: "blake3-512:0".repeat(1),
        required_sort_morphism_slots: vec![SlotDescriptor {
            slot_name: "s".to_string(),
            source_type_variable: "T".to_string(),
            target_type_variable: "U".to_string(),
        }],
        sugar_cids: vec![],
        target_pattern: serde_json::json!({}),
        type_variables: vec!["T".to_string(), "U".to_string()],
    };
    let plan = RealizationPlanMemento {
        candidate_set_cid: "blake3-512:c".repeat(1),
        concept_site_cid: "blake3-512:s".repeat(1),
        effect_occurrence_transform: serde_json::json!({}),
        loss_function_cid: "blake3-512:l".repeat(1),
        observation_wrapper_cid: None,
        provenance_cid: "blake3-512:p".repeat(1),
        selected_candidate_cid: "blake3-512:x".repeat(1),
        selected_realization_cid: "blake3-512:r".repeat(1),
        sort_morphism_cids: vec![
            "blake3-512:m1".to_string(),
            "blake3-512:m2".to_string(),
            "blake3-512:m3".to_string(),
        ],
        total_loss_record: serde_json::json!({}),
    };
    let err = plan
        .validate_against(&realization)
        .expect_err("count mismatch");
    assert_eq!(
        err,
        RealizationPlanError::SortMorphismCountMismatch {
            expected: 1,
            actual: 3,
        }
    );
}
