// SPDX-License-Identifier: Apache-2.0
//
// CID-pinning tests for the abstraction-layer mementos.
//
// Source of truth:
//   protocol/specs/2026-05-15-concept-hub-abstraction-layer.md §2.1, §2.2, §2.4
//   protocol/provekit-ir.cddl  (ConceptAbstractionMemento, RealizationDesugaringMemento)
//
// What this file pins:
//
//   1. Five canonical fixture mementos, each serialized to JCS-canonical JSON,
//      each BLAKE3-512 CID pinned.
//
//   Fixtures:
//     (A) concept:dynamic-dispatch  -- ConceptAbstractionMemento
//     (B) concept:double-dispatch   -- ConceptAbstractionMemento
//     (C) concept:double-dispatch->c11:2d-fn-ptr-table   -- RealizationDesugaringMemento
//     (D) concept:double-dispatch->jvm:visitor-pattern   -- RealizationDesugaringMemento
//     (E) concept:double-dispatch->ruby:case-respond_to  -- RealizationDesugaringMemento
//
//   2. Round-trip parity: emit -> re-parse -> re-emit -> byte-identical JCS.
//
//   3. Projection-distance law held by the three RealizationDesugaringMementos:
//      C (c11) carries both structural_divergence AND domain_narrowing (heavy).
//      D (java) carries structural_divergence, less domain_narrowing (mid).
//      E (ruby) carries only structural_divergence (near-zero).
//
// To update pinned CIDs after a deliberate schema change:
//   1. Remove the assertion and run the test to see the printed CID.
//   2. Copy the printed CID into the const below.
//   3. Re-run the test to confirm it is stable.

use std::sync::Arc;

use provekit_canonicalizer::{blake3_512_of, encode_jcs, Value};
use serde_json::Value as Json;

fn json_to_value(j: &Json) -> Arc<Value> {
    match j {
        Json::Null => Value::null(),
        Json::Bool(b) => Value::boolean(*b),
        Json::Number(n) => {
            if let Some(i) = n.as_i64() {
                Value::integer(i)
            } else if let Some(u) = n.as_u64() {
                Value::integer(u as i64)
            } else if let Some(f) = n.as_f64() {
                Value::integer(f as i64)
            } else {
                Value::integer(0)
            }
        }
        Json::String(s) => Value::string(s.clone()),
        Json::Array(items) => {
            let v: Vec<_> = items.iter().map(json_to_value).collect();
            Value::array(v)
        }
        Json::Object(map) => {
            let entries: Vec<(String, _)> = map
                .iter()
                .map(|(k, v)| (k.clone(), json_to_value(v)))
                .collect();
            Arc::new(Value::Object(entries))
        }
    }
}

/// JCS-canonicalize a JSON string and return the BLAKE3-512 CID.
fn cid_of_json(json_str: &str) -> String {
    let j: Json = serde_json::from_str(json_str).expect("parse fixture JSON");
    let v = json_to_value(&j);
    let jcs = encode_jcs(&v);
    blake3_512_of(jcs.as_bytes())
}

/// JCS-canonicalize, then re-parse, then JCS again to prove byte-identity.
fn assert_jcs_stable(json_str: &str, label: &str) {
    let j1: Json = serde_json::from_str(json_str).expect("parse");
    let v1 = json_to_value(&j1);
    let jcs1 = encode_jcs(&v1);

    // re-parse the JCS bytes and re-emit
    let j2: Json = serde_json::from_str(&jcs1).expect("re-parse JCS");
    let v2 = json_to_value(&j2);
    let jcs2 = encode_jcs(&v2);

    assert_eq!(
        jcs1, jcs2,
        "{label}: JCS bytes MUST be byte-identical across emit/parse/emit"
    );
}

// ================================================================
// (A) concept:dynamic-dispatch ConceptAbstractionMemento
// ================================================================

// NOTE: serde_json serializes BTreeMap keys in sorted order. The
// canonical JSON below uses the same key order as the Rust struct
// would emit: serde_json::to_value on the struct gives the same bytes
// as this manually-written canonical form once parsed and JCS'd.
// All keys are already in JCS (lexicographic) order within each object.

const DYNAMIC_DISPATCH_CANONICAL: &str = r#"{"contract":{"body":{"operands":[{"args":[{"name":"m","kind":"var"}],"kind":"atomic","name":"defined"},{"args":[{"name":"m","kind":"var"},{"name":"receiver","kind":"var"}],"kind":"atomic","name":"wp_call"}],"kind":"and"},"kind":"choice","sort":{"kind":"primitive","name":"Int"},"varName":"m"},"contract_note":"the call result and effect equal those of the method that resolves from the receiver runtime type for method_name applied to receiver and args; if no such method resolves the behaviour is undefined","formal_sorts":["blake3-512:sort0000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000","blake3-512:sort1111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111","blake3-512:sort2222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222"],"kind":"concept-abstraction","operator":"concept:dynamic-dispatch","realizations":[],"result_sort":"blake3-512:sort3333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333","slots":[{"name":"receiver"},{"name":"method_name"},{"name":"args","variadic":true}],"tier":"abstraction"}"#;

// Pinned BLAKE3-512 CID for concept:dynamic-dispatch fixture.
const DYNAMIC_DISPATCH_CID: &str =
    "blake3-512:ca8ff4e631fe41885b37f897b42906e62e2b4079438ce6480885b5eb0f01207314fd38563591e82304aaa9c1b750e94a8baa24f5e9401cd2ed873da0cd0c90da";

#[test]
fn dynamic_dispatch_cid_stable() {
    let cid = cid_of_json(DYNAMIC_DISPATCH_CANONICAL);
    println!("concept:dynamic-dispatch CID = {cid}");
    assert_eq!(cid, DYNAMIC_DISPATCH_CID, "concept:dynamic-dispatch CID must be pinned");
}

#[test]
fn dynamic_dispatch_jcs_round_trip() {
    assert_jcs_stable(DYNAMIC_DISPATCH_CANONICAL, "concept:dynamic-dispatch");
}

// ================================================================
// (B) concept:double-dispatch ConceptAbstractionMemento
// ================================================================

const DOUBLE_DISPATCH_CANONICAL: &str = r#"{"contract":{"body":{"operands":[{"args":[{"name":"m","kind":"var"}],"kind":"atomic","name":"defined"},{"args":[{"name":"m","kind":"var"},{"name":"receiver","kind":"var"},{"name":"secondary","kind":"var"}],"kind":"atomic","name":"wp_call"}],"kind":"and"},"kind":"choice","sort":{"kind":"primitive","name":"Int"},"varName":"m"},"contract_note":"dispatch is resolved from the conjunction of the receiver runtime type and the secondary runtime type for method_name; if no such method resolves the behaviour is undefined","formal_sorts":["blake3-512:sort0000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000","blake3-512:sort0000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000","blake3-512:sort1111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111","blake3-512:sort2222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222"],"kind":"concept-abstraction","operator":"concept:double-dispatch","realizations":[],"result_sort":"blake3-512:sort3333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333","slots":[{"name":"receiver"},{"name":"secondary"},{"name":"method_name"},{"name":"args","variadic":true}],"tier":"abstraction"}"#;

const DOUBLE_DISPATCH_CID: &str =
    "blake3-512:1cc86fe75917b11e41cb847ee5e5815d67e69f5d5c93f89c0fc54c273312060ae26c6170c8c2d9b22170fabbc627b5d614b3c39c486fcd9edc6e44433cff9909";

#[test]
fn double_dispatch_cid_stable() {
    let cid = cid_of_json(DOUBLE_DISPATCH_CANONICAL);
    println!("concept:double-dispatch CID = {cid}");
    assert_eq!(cid, DOUBLE_DISPATCH_CID, "concept:double-dispatch CID must be pinned");
}

#[test]
fn double_dispatch_jcs_round_trip() {
    assert_jcs_stable(DOUBLE_DISPATCH_CANONICAL, "concept:double-dispatch");
}

// ================================================================
// (C) RealizationDesugaringMemento: double-dispatch -> c11
// ================================================================
//
// Heavy structural_divergence + domain_narrowing (C is the unforgiving end).

const DD_C11_CANONICAL: &str = r#"{"direction":"left-to-right","effects":[],"fn_name":"concept:double-dispatch->c11:2d-fn-ptr-table","formal_sorts":["blake3-512:sort0000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000","blake3-512:sort0000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000","blake3-512:sort1111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111","blake3-512:sort2222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222"],"formals":["receiver","secondary","method_name","args"],"kind":"equation","loss_record":{"domain_narrowing":{"args":[{"name":"receiver","kind":"var"},{"name":"secondary","kind":"var"}],"kind":"atomic","name":"requires_static_2d_dispatch_table"},"structural_divergence":{"args":[],"kind":"atomic","name":"true"}},"post":{"lhs":{"args":[{"name":"receiver","kind":"var"},{"name":"secondary","kind":"var"},{"name":"method_name","kind":"var"},{"name":"args","kind":"var"}],"kind":"atomic","name":"concept:double-dispatch"},"rhs":{"args":[{"args":[{"args":[{"args":[{"kind":"ctor","name":"concept:member","args":[{"name":"receiver","kind":"var"},{"kind":"const","sort":{"kind":"primitive","name":"String"},"value":"dispatch_tbl"}]}],"kind":"ctor","name":"concept:index","args":[{"kind":"ctor","name":"concept:tag-of","args":[{"name":"receiver","kind":"var"}]}]}],"kind":"ctor","name":"concept:index","args":[{"kind":"ctor","name":"concept:tag-of","args":[{"name":"secondary","kind":"var"}]}]}],"kind":"atomic","name":"concept:call"}],"kind":"atomic","name":"concept:call"}},"pre":{"args":[{"name":"receiver","kind":"var"},{"name":"secondary","kind":"var"}],"kind":"atomic","name":"static_dispatch_table"},"role":"abstraction-realization","target_lang":"c11"}"#;

const DD_C11_CID: &str =
    "blake3-512:081b7f07196d75c4a3796953113d73b3d8b4603ab3ffe1993015bb5cd09a33727122567876855875e62f4196d8c28e997e295bfc96f6b7a37462df4782bbb8a0";

#[test]
fn dd_c11_cid_stable() {
    let cid = cid_of_json(DD_C11_CANONICAL);
    println!("dd->c11 CID = {cid}");
    assert_eq!(cid, DD_C11_CID, "dd->c11 CID must be pinned");
}

#[test]
fn dd_c11_jcs_round_trip() {
    assert_jcs_stable(DD_C11_CANONICAL, "dd->c11");
}

// ================================================================
// (D) RealizationDesugaringMemento: double-dispatch -> java
// ================================================================
//
// Mid structural_divergence, minimal domain_narrowing (Java is the middle).

const DD_JAVA_CANONICAL: &str = r#"{"direction":"left-to-right","effects":[],"fn_name":"concept:double-dispatch->jvm:visitor-pattern","formal_sorts":["blake3-512:sort0000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000","blake3-512:sort0000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000","blake3-512:sort1111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111","blake3-512:sort2222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222"],"formals":["receiver","secondary","method_name","args"],"kind":"equation","loss_record":{"structural_divergence":{"args":[],"kind":"atomic","name":"visitor_pattern_indirection"}},"post":{"lhs":{"args":[{"name":"receiver","kind":"var"},{"name":"secondary","kind":"var"},{"name":"method_name","kind":"var"},{"name":"args","kind":"var"}],"kind":"atomic","name":"concept:double-dispatch"},"rhs":{"args":[{"name":"receiver","kind":"var"},{"name":"method_name","kind":"var"},{"name":"secondary","kind":"var"},{"name":"args","kind":"var"}],"kind":"atomic","name":"jvm:invokeinterface"}},"role":"abstraction-realization","target_lang":"java"}"#;

const DD_JAVA_CID: &str =
    "blake3-512:8ea85ecdc492a2c31a9fc2f36626935eb0cf1f7290c1f16c999ba23b2a802e68278c5aa8f019fc56f2ea7e3ab0dcfc3b96c85d9e794d1c77f107c1d8713b0b5c";

#[test]
fn dd_java_cid_stable() {
    let cid = cid_of_json(DD_JAVA_CANONICAL);
    println!("dd->java CID = {cid}");
    assert_eq!(cid, DD_JAVA_CID, "dd->java CID must be pinned");
}

#[test]
fn dd_java_jcs_round_trip() {
    assert_jcs_stable(DD_JAVA_CANONICAL, "dd->java");
}

// ================================================================
// (E) RealizationDesugaringMemento: double-dispatch -> ruby
// ================================================================
//
// Near-zero structural_divergence only (Ruby is the loose end).

const DD_RUBY_CANONICAL: &str = r#"{"direction":"left-to-right","effects":[],"fn_name":"concept:double-dispatch->ruby:case-respond_to","formal_sorts":["blake3-512:sort0000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000","blake3-512:sort0000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000","blake3-512:sort1111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111","blake3-512:sort2222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222"],"formals":["receiver","secondary","method_name","args"],"kind":"equation","loss_record":{"structural_divergence":{"args":[],"kind":"atomic","name":"guarded_case_chain"}},"post":{"lhs":{"args":[{"name":"receiver","kind":"var"},{"name":"secondary","kind":"var"},{"name":"method_name","kind":"var"},{"name":"args","kind":"var"}],"kind":"atomic","name":"concept:double-dispatch"},"rhs":{"args":[{"name":"receiver","kind":"var"},{"name":"method_name","kind":"var"},{"name":"secondary","kind":"var"},{"name":"args","kind":"var"}],"kind":"atomic","name":"ruby:public_send"}},"role":"abstraction-realization","target_lang":"ruby"}"#;

const DD_RUBY_CID: &str =
    "blake3-512:730a633f0e59e0393261ac911be375821cf86a1c49a61fc3135a1cf01720d2e9ba1e339beca4dac2c67e04a3ffc5695305291b8f101c0b7484178e2a0a66b81c";

#[test]
fn dd_ruby_cid_stable() {
    let cid = cid_of_json(DD_RUBY_CANONICAL);
    println!("dd->ruby CID = {cid}");
    assert_eq!(cid, DD_RUBY_CID, "dd->ruby CID must be pinned");
}

#[test]
fn dd_ruby_jcs_round_trip() {
    assert_jcs_stable(DD_RUBY_CANONICAL, "dd->ruby");
}

// ================================================================
// Projection-distance law
// ================================================================
//
// The three double-dispatch realizations demonstrate the C-far/Java-mid/Ruby-near
// projection-distance law from spec §3.0:
//
//   C:    structural_divergence + domain_narrowing  (heavy)
//   Java: structural_divergence only                (mid; visitor pattern indirection)
//   Ruby: structural_divergence only, lighter       (near-zero; native send)
//
// This test reads the canonical JSON for each and asserts the expected
// loss-record shape matches the spec's characterization.

#[test]
fn projection_distance_law_c_has_structural_and_domain_narrowing() {
    let j: Json = serde_json::from_str(DD_C11_CANONICAL).expect("parse dd->c11");
    let lr = j.pointer("/loss_record").expect("loss_record present");
    assert!(
        lr.pointer("/structural_divergence").is_some(),
        "C realization MUST have structural_divergence"
    );
    assert!(
        lr.pointer("/domain_narrowing").is_some(),
        "C realization MUST have domain_narrowing (heavy end)"
    );
}

#[test]
fn projection_distance_law_java_has_structural_no_domain_narrowing() {
    let j: Json = serde_json::from_str(DD_JAVA_CANONICAL).expect("parse dd->java");
    let lr = j.pointer("/loss_record").expect("loss_record present");
    assert!(
        lr.pointer("/structural_divergence").is_some(),
        "Java realization MUST have structural_divergence"
    );
    assert!(
        lr.pointer("/domain_narrowing").is_none(),
        "Java realization MUST NOT have domain_narrowing in this fixture (mid)"
    );
}

#[test]
fn projection_distance_law_ruby_has_structural_only() {
    let j: Json = serde_json::from_str(DD_RUBY_CANONICAL).expect("parse dd->ruby");
    let lr = j.pointer("/loss_record").expect("loss_record present");
    assert!(
        lr.pointer("/structural_divergence").is_some(),
        "Ruby realization MUST have structural_divergence"
    );
    assert!(
        lr.pointer("/domain_narrowing").is_none(),
        "Ruby realization MUST NOT have domain_narrowing (near-zero end)"
    );
    assert!(
        lr.pointer("/ub_introduction").is_none(),
        "Ruby realization MUST NOT have ub_introduction"
    );
    assert!(
        lr.pointer("/value_divergence").is_none(),
        "Ruby realization MUST NOT have value_divergence"
    );
    assert!(
        lr.pointer("/effect_divergence").is_none(),
        "Ruby realization MUST NOT have effect_divergence"
    );
}
