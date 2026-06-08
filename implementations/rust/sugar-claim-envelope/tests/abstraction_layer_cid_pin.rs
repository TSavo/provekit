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
//     (E) concept:double-dispatch->ruby:case-type-tuple  -- RealizationDesugaringMemento
//
//   2. Round-trip parity: emit -> re-parse -> re-emit -> byte-identical JCS.
//
//   3. Projection-distance law held by the three RealizationDesugaringMementos:
//      C (c11) carries structural_divergence + domain_narrowing + ub_introduction (heavy).
//      D (java) carries structural_divergence + domain_narrowing (mid; visitor accept/visit).
//      E (ruby) carries only structural_divergence (near-zero; case-match ≈ contract).
//
// To update pinned CIDs after a deliberate schema change:
//   1. Remove the assertion and run the test to see the printed CID.
//   2. Copy the printed CID into the const below.
//   3. Re-run the test to confirm it is stable.

use std::sync::Arc;

use serde_json::Value as Json;
use sugar_canonicalizer::{blake3_512_of, encode_jcs, Value};

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
    assert_eq!(
        cid, DYNAMIC_DISPATCH_CID,
        "concept:dynamic-dispatch CID must be pinned"
    );
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
    assert_eq!(
        cid, DOUBLE_DISPATCH_CID,
        "concept:double-dispatch CID must be pinned"
    );
}

#[test]
fn double_dispatch_jcs_round_trip() {
    assert_jcs_stable(DOUBLE_DISPATCH_CANONICAL, "concept:double-dispatch");
}

// ================================================================
// (C) RealizationDesugaringMemento: double-dispatch -> c11
// ================================================================
//
// Heavy structural_divergence + domain_narrowing + ub_introduction (C is the unforgiving end).
//
// The rhs encodes the full 2D void*-table dispatch:
//   ((fn_ptr)table[tag(receiver)][tag(secondary)])(receiver, secondary, args)
// Steps: concept:member -> concept:index (dim 1) -> concept:index (dim 2)
//        -> concept:cast (void* -> fn-ptr) -> concept:call
//
// Loss-record (3 distinct atomic claims, heaviest end of the bracket):
//   structural_divergence: open_coded_vtable_replaces_single_op (2 index + 1 cast)
//   domain_narrowing:      requires_static_2d_dispatch_table
//   ub_introduction:       out_of_range_tag_is_ub
//
// JCS key order is alphabetical within each object (RFC 8785 §3.2.3).
// TODO: re-sort the "Locked JCS key order" CDDL comments to match actual
// canonicalizer output (alphabetical), not struct declaration order.

const DD_C11_CANONICAL: &str = r#"{"direction":"left-to-right","effects":[],"fn_name":"concept:double-dispatch->c11:2d-fn-ptr-table","formal_sorts":["blake3-512:sort0000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000","blake3-512:sort0000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000","blake3-512:sort1111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111","blake3-512:sort2222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222"],"formals":["receiver","secondary","method_name","args"],"kind":"equation","loss_record":{"domain_narrowing":{"args":[{"kind":"var","name":"receiver"},{"kind":"var","name":"secondary"}],"kind":"atomic","name":"requires_static_2d_dispatch_table"},"structural_divergence":{"args":[{"args":[{"kind":"var","name":"receiver"}],"kind":"ctor","name":"concept:index"},{"args":[{"kind":"var","name":"secondary"}],"kind":"ctor","name":"concept:index"},{"args":[],"kind":"ctor","name":"concept:cast"}],"kind":"atomic","name":"open_coded_vtable_replaces_single_op"},"ub_introduction":{"args":[{"kind":"var","name":"receiver"},{"kind":"var","name":"secondary"}],"kind":"atomic","name":"out_of_range_tag_is_ub"}},"post":{"lhs":{"args":[{"kind":"var","name":"receiver"},{"kind":"var","name":"secondary"},{"kind":"var","name":"method_name"},{"kind":"var","name":"args"}],"kind":"atomic","name":"concept:double-dispatch"},"rhs":{"args":[{"args":[{"args":[{"args":[{"args":[{"kind":"var","name":"receiver"},{"kind":"const","sort":{"kind":"primitive","name":"String"},"value":"dispatch_tbl"}],"kind":"ctor","name":"concept:member"},{"args":[{"kind":"var","name":"receiver"}],"kind":"ctor","name":"concept:tag-of"}],"kind":"ctor","name":"concept:index"},{"args":[{"kind":"var","name":"secondary"}],"kind":"ctor","name":"concept:tag-of"}],"kind":"ctor","name":"concept:index"},{"kind":"const","sort":{"kind":"primitive","name":"String"},"value":"fn_ptr_2d"}],"kind":"ctor","name":"concept:cast"},{"kind":"var","name":"receiver"},{"kind":"var","name":"secondary"},{"kind":"var","name":"args"}],"kind":"atomic","name":"concept:call"}},"pre":{"args":[{"kind":"var","name":"receiver"},{"kind":"var","name":"secondary"}],"kind":"atomic","name":"static_dispatch_table"},"role":"abstraction-realization","target_lang":"c11"}"#;

const DD_C11_CID: &str =
    "blake3-512:35932da2302a9ca08c4c1ccadc1ee04995b91a0ce005977a2804181a59c786fe90794bf0e2de561a871d8d40e1da6c525b1cf0a3b8517dd8dafc92227e6796f0";

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
// Mid structural_divergence + domain_narrowing (Java is the middle of the bracket).
//
// The rhs encodes the visitor pattern: two sequential itab-method calls.
//   receiver.accept(secondary)     -- dispatches on receiver's type
//   secondary.visit_receiver_type(receiver, args)  -- dispatches on secondary's type
// This is a concept:seq of TWO concept:itab-method ctors, not a single invokeinterface.
//
// Loss-record (2 distinct atomic claims, mid bracket):
//   structural_divergence: visitor_accept_visit_indirection (2 itab-method nodes)
//   domain_narrowing:      visitable_set_fixed_at_declaration

const DD_JAVA_CANONICAL: &str = r#"{"direction":"left-to-right","effects":[],"fn_name":"concept:double-dispatch->jvm:visitor-pattern","formal_sorts":["blake3-512:sort0000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000","blake3-512:sort0000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000","blake3-512:sort1111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111","blake3-512:sort2222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222"],"formals":["receiver","secondary","method_name","args"],"kind":"equation","loss_record":{"domain_narrowing":{"args":[{"kind":"var","name":"receiver"},{"kind":"var","name":"secondary"}],"kind":"atomic","name":"visitable_set_fixed_at_declaration"},"structural_divergence":{"args":[{"args":[{"kind":"var","name":"receiver"}],"kind":"ctor","name":"concept:itab-method"},{"args":[{"kind":"var","name":"secondary"}],"kind":"ctor","name":"concept:itab-method"}],"kind":"atomic","name":"visitor_accept_visit_indirection"}},"post":{"lhs":{"args":[{"kind":"var","name":"receiver"},{"kind":"var","name":"secondary"},{"kind":"var","name":"method_name"},{"kind":"var","name":"args"}],"kind":"atomic","name":"concept:double-dispatch"},"rhs":{"args":[{"args":[{"kind":"var","name":"receiver"},{"kind":"const","sort":{"kind":"primitive","name":"String"},"value":"accept"},{"kind":"var","name":"secondary"}],"kind":"ctor","name":"concept:itab-method"},{"args":[{"kind":"var","name":"secondary"},{"kind":"const","sort":{"kind":"primitive","name":"String"},"value":"visit_receiver_type"},{"kind":"var","name":"receiver"},{"kind":"var","name":"args"}],"kind":"ctor","name":"concept:itab-method"}],"kind":"atomic","name":"concept:seq"}},"role":"abstraction-realization","target_lang":"java"}"#;

const DD_JAVA_CID: &str =
    "blake3-512:a024327ba96392f6b070b3e88b8638cc8318e98c946c46cbaca5a849359a0e9eeffd4e3d81f307e4d1f613f076a903211945226db3bed499b12a7da07ccedf1f";

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
// Near-zero structural_divergence only (Ruby is the loose end of the bracket).
//
// The rhs encodes a case-match over the type tuple, preserving the
// implication structure of the abstraction's contract:
//   case [type(receiver), type(secondary)]
//   when [X, Y] then method_name(receiver, secondary, args)
//   else raise TypeError
//
// This is a concept:match over concept:pair(type-of(receiver), type-of(secondary))
// with a concept:match-arm and a concept:raise fallthrough.
// The realization ≈ the contract -- Ruby just writes out the guarded dispatch directly.
//
// Loss-record (1 atomic claim, loosest end):
//   structural_divergence: case_fallthrough_narrows_open_dispatch
//     (Ruby's open dispatch domain is narrowed by the case fallthrough to TypeError)

const DD_RUBY_CANONICAL: &str = r#"{"direction":"left-to-right","effects":[],"fn_name":"concept:double-dispatch->ruby:case-type-tuple","formal_sorts":["blake3-512:sort0000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000","blake3-512:sort0000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000","blake3-512:sort1111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111","blake3-512:sort2222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222"],"formals":["receiver","secondary","method_name","args"],"kind":"equation","loss_record":{"structural_divergence":{"args":[{"args":[],"kind":"ctor","name":"concept:raise"}],"kind":"atomic","name":"case_fallthrough_narrows_open_dispatch"}},"post":{"lhs":{"args":[{"kind":"var","name":"receiver"},{"kind":"var","name":"secondary"},{"kind":"var","name":"method_name"},{"kind":"var","name":"args"}],"kind":"atomic","name":"concept:double-dispatch"},"rhs":{"args":[{"args":[{"args":[{"kind":"var","name":"receiver"}],"kind":"ctor","name":"concept:type-of"},{"args":[{"kind":"var","name":"secondary"}],"kind":"ctor","name":"concept:type-of"}],"kind":"ctor","name":"concept:pair"},{"args":[{"args":[{"args":[{"kind":"var","name":"receiver"}],"kind":"ctor","name":"concept:tag-of"},{"args":[{"kind":"var","name":"secondary"}],"kind":"ctor","name":"concept:tag-of"}],"kind":"ctor","name":"concept:pair"},{"args":[{"kind":"var","name":"method_name"},{"kind":"var","name":"receiver"},{"kind":"var","name":"secondary"},{"kind":"var","name":"args"}],"kind":"ctor","name":"concept:call"}],"kind":"ctor","name":"concept:match-arm"},{"args":[{"kind":"const","sort":{"kind":"primitive","name":"String"},"value":"TypeError"}],"kind":"ctor","name":"concept:raise"}],"kind":"atomic","name":"concept:match"}},"role":"abstraction-realization","target_lang":"ruby"}"#;

const DD_RUBY_CID: &str =
    "blake3-512:5f6e90c1f3a832ddd97ad624522a001d0a90091ca796d75cf7a1dccd819a5b12ec0181bf8297653204ed226259ba6acaf48633586dd403ce30ae258260af319f";

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
// projection-distance law from spec §3.0. This is a NON-TRIVIAL assertion:
// the test compares real structural content (loss-record dimension count and
// structural_divergence formula complexity), not merely presence/absence.
//
// Law:
//   C:    3 loss dimensions (structural_divergence + domain_narrowing + ub_introduction)
//         structural_divergence has 3 args (open_coded_vtable: 2 index + 1 cast nodes)
//   Java: 2 loss dimensions (structural_divergence + domain_narrowing)
//         structural_divergence has 2 args (2 itab-method nodes for accept/visit)
//   Ruby: 1 loss dimension (structural_divergence only)
//         structural_divergence has 1 arg (1 raise node for fallthrough)
//
// Strict ordering: C-dims > Java-dims > Ruby-dims and C-sd-args > Java-sd-args > Ruby-sd-args

/// Count the elements of a JSON array (returns 0 for non-arrays).
fn array_len(v: &Json) -> usize {
    v.as_array().map(|a| a.len()).unwrap_or(0)
}

/// Count loss-record dimensions (number of top-level keys in loss_record object).
fn loss_record_dim_count(canonical: &str) -> usize {
    let j: Json = serde_json::from_str(canonical).expect("parse");
    let lr = j.pointer("/loss_record").expect("loss_record present");
    lr.as_object().map(|m| m.len()).unwrap_or(0)
}

/// Count the args array of the structural_divergence formula.
fn sd_arg_count(canonical: &str) -> usize {
    let j: Json = serde_json::from_str(canonical).expect("parse");
    let sd = j
        .pointer("/loss_record/structural_divergence")
        .expect("structural_divergence present");
    array_len(sd.pointer("/args").unwrap_or(&Json::Null))
}

#[test]
fn projection_distance_law_dimension_counts_strict_ordering() {
    let c_dims = loss_record_dim_count(DD_C11_CANONICAL);
    let j_dims = loss_record_dim_count(DD_JAVA_CANONICAL);
    let rb_dims = loss_record_dim_count(DD_RUBY_CANONICAL);

    assert_eq!(
        c_dims, 3,
        "C MUST have 3 loss dimensions (structural + domain + ub)"
    );
    assert_eq!(
        j_dims, 2,
        "Java MUST have 2 loss dimensions (structural + domain)"
    );
    assert_eq!(
        rb_dims, 1,
        "Ruby MUST have 1 loss dimension (structural only)"
    );

    assert!(
        c_dims > j_dims,
        "Projection-distance law: C loss dims ({c_dims}) MUST exceed Java ({j_dims})"
    );
    assert!(
        j_dims > rb_dims,
        "Projection-distance law: Java loss dims ({j_dims}) MUST exceed Ruby ({rb_dims})"
    );
}

#[test]
fn projection_distance_law_structural_divergence_complexity_strict_ordering() {
    let c_args = sd_arg_count(DD_C11_CANONICAL);
    let j_args = sd_arg_count(DD_JAVA_CANONICAL);
    let rb_args = sd_arg_count(DD_RUBY_CANONICAL);

    assert_eq!(
        c_args, 3,
        "C structural_divergence MUST have 3 args (2 index + 1 cast)"
    );
    assert_eq!(
        j_args, 2,
        "Java structural_divergence MUST have 2 args (2 itab-method)"
    );
    assert_eq!(
        rb_args, 1,
        "Ruby structural_divergence MUST have 1 arg (1 raise)"
    );

    assert!(
        c_args > j_args,
        "C sd complexity ({c_args}) MUST exceed Java ({j_args})"
    );
    assert!(
        j_args > rb_args,
        "Java sd complexity ({j_args}) MUST exceed Ruby ({rb_args})"
    );
}

#[test]
fn projection_distance_law_c_has_ub_introduction_others_do_not() {
    let c_j: Json = serde_json::from_str(DD_C11_CANONICAL).expect("parse c11");
    let j_j: Json = serde_json::from_str(DD_JAVA_CANONICAL).expect("parse java");
    let rb_j: Json = serde_json::from_str(DD_RUBY_CANONICAL).expect("parse ruby");

    assert!(
        c_j.pointer("/loss_record/ub_introduction").is_some(),
        "C MUST have ub_introduction (out-of-range tag = UB)"
    );
    assert!(
        j_j.pointer("/loss_record/ub_introduction").is_none(),
        "Java MUST NOT have ub_introduction"
    );
    assert!(
        rb_j.pointer("/loss_record/ub_introduction").is_none(),
        "Ruby MUST NOT have ub_introduction"
    );
}

#[test]
fn projection_distance_law_ruby_has_no_domain_narrowing() {
    let rb_j: Json = serde_json::from_str(DD_RUBY_CANONICAL).expect("parse ruby");
    assert!(
        rb_j.pointer("/loss_record/domain_narrowing").is_none(),
        "Ruby MUST NOT have domain_narrowing (near-zero end: no fixed interface required)"
    );
    assert!(
        rb_j.pointer("/loss_record/value_divergence").is_none(),
        "Ruby MUST NOT have value_divergence"
    );
    assert!(
        rb_j.pointer("/loss_record/effect_divergence").is_none(),
        "Ruby MUST NOT have effect_divergence"
    );
}

// ================================================================
// Permanent duplicate-key audit guard (reviewer required action 1)
// ================================================================
//
// serde_json silently deduplicates duplicate object keys (last-key-wins).
// This means a hand-authored canonical string with duplicate keys can
// pass a round-trip test while encoding a corrupted structure -- exactly
// the bug that caused DD_C11's original CID (081b7f07) to be wrong.
//
// This test audits all five canonical fixture strings by scanning the
// raw JSON bytes for duplicate keys within any object at any nesting
// depth, using a custom recursive walk that processes the token stream
// before serde deduplication occurs.
//
// If this test fails, a hand-authored fixture has duplicate keys and
// the CID is pinned to a corrupted (serde-deduped) structure.

/// Scan a JSON string recursively for duplicate keys in any object at any
/// nesting depth. Returns `Err(description)` on the first duplicate found.
///
/// Uses `serde_json::Deserializer` with a custom `serde::de::Visitor` that
/// processes keys BEFORE serde_json's BTreeMap/IndexMap deduplication step.
/// Both objects and arrays are walked recursively so nested objects are
/// fully audited.
fn scan_for_dup_keys(json_str: &str) -> Result<(), String> {
    struct AnyChecker;

    impl<'de> serde::de::Deserialize<'de> for AnyChecker {
        fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
            d.deserialize_any(AnyVisitor)?;
            Ok(AnyChecker)
        }
    }

    struct AnyVisitor;

    impl<'de> serde::de::Visitor<'de> for AnyVisitor {
        type Value = ();

        fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "any JSON value")
        }

        fn visit_bool<E: serde::de::Error>(self, _: bool) -> Result<(), E> {
            Ok(())
        }
        fn visit_i64<E: serde::de::Error>(self, _: i64) -> Result<(), E> {
            Ok(())
        }
        fn visit_u64<E: serde::de::Error>(self, _: u64) -> Result<(), E> {
            Ok(())
        }
        fn visit_f64<E: serde::de::Error>(self, _: f64) -> Result<(), E> {
            Ok(())
        }
        fn visit_str<E: serde::de::Error>(self, _: &str) -> Result<(), E> {
            Ok(())
        }
        fn visit_none<E: serde::de::Error>(self) -> Result<(), E> {
            Ok(())
        }
        fn visit_unit<E: serde::de::Error>(self) -> Result<(), E> {
            Ok(())
        }

        fn visit_some<D: serde::Deserializer<'de>>(self, d: D) -> Result<(), D::Error> {
            d.deserialize_any(AnyVisitor)
        }

        fn visit_seq<A: serde::de::SeqAccess<'de>>(self, mut seq: A) -> Result<(), A::Error> {
            while seq.next_element::<AnyChecker>()?.is_some() {}
            Ok(())
        }

        fn visit_map<A: serde::de::MapAccess<'de>>(self, mut map: A) -> Result<(), A::Error> {
            let mut seen: Vec<String> = Vec::new();
            while let Some(key) = map.next_key::<String>()? {
                if seen.contains(&key) {
                    return Err(serde::de::Error::custom(format!(
                        "duplicate JSON object key: '{key}'"
                    )));
                }
                seen.push(key);
                map.next_value::<AnyChecker>()?;
            }
            Ok(())
        }
    }

    use serde::Deserializer as _;
    let mut de = serde_json::Deserializer::from_str(json_str);
    de.deserialize_any(AnyVisitor)
        .map_err(|e| format!("duplicate key detected: {e}"))
}

/// Assert that a canonical JSON string contains no duplicate object keys
/// at any nesting depth. Panics with a descriptive message on failure.
fn assert_no_duplicate_keys(json_str: &str, label: &str) {
    if let Err(msg) = scan_for_dup_keys(json_str) {
        panic!(
            "{label}: canonical fixture has duplicate JSON keys (CID is pinned to corrupted structure)\n  {msg}"
        );
    }
}

#[test]
fn all_five_fixtures_have_no_duplicate_keys() {
    // This test permanently guards against the class of bug that caused
    // DD_C11's original CID (081b7f07) to be wrong: hand-authored JSON
    // with duplicate "args" keys inside nested concept:index ctors.
    // serde_json silently last-key-wins deduplicates, so the round-trip
    // test passed while the CID was pinned to a corrupted structure.
    assert_no_duplicate_keys(DYNAMIC_DISPATCH_CANONICAL, "concept:dynamic-dispatch");
    assert_no_duplicate_keys(DOUBLE_DISPATCH_CANONICAL, "concept:double-dispatch");
    assert_no_duplicate_keys(DD_C11_CANONICAL, "dd->c11:2d-fn-ptr-table");
    assert_no_duplicate_keys(DD_JAVA_CANONICAL, "dd->jvm:visitor-pattern");
    assert_no_duplicate_keys(DD_RUBY_CANONICAL, "dd->ruby:case-type-tuple");
}
