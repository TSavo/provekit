// SPDX-License-Identifier: Apache-2.0

use libprovekit::concept::panic_freedom;

#[test]
fn panic_freedom_constants_keep_existing_wire_tokens() {
    assert_eq!(panic_freedom::IS_OK, "is_ok");
    assert_eq!(
        panic_freedom::IS_OK_CONCEPT,
        "concept:panic-freedom.result.ok"
    );
    assert_eq!(panic_freedom::IS_ERR, "is_err");
    assert_eq!(
        panic_freedom::IS_ERR_CONCEPT,
        "concept:panic-freedom.result.err"
    );
    assert_eq!(panic_freedom::IS_SOME, "is_some");
    assert_eq!(
        panic_freedom::IS_SOME_CONCEPT,
        "concept:panic-freedom.option.some"
    );
    assert_eq!(panic_freedom::IS_NONE, "is_none");
    assert_eq!(
        panic_freedom::IS_NONE_CONCEPT,
        "concept:panic-freedom.option.none"
    );
    assert_eq!(panic_freedom::CF_GUARDED, "cf_guarded");
    assert_eq!(
        panic_freedom::CF_GUARDED_CONCEPT,
        "concept:panic-freedom.guard"
    );
    assert_eq!(panic_freedom::CF_ITE, "cf_ite");
    assert_eq!(
        panic_freedom::CF_ITE_CONCEPT,
        "concept:panic-freedom.choice"
    );
    assert_eq!(panic_freedom::METHOD_UNWRAP, "method:unwrap");
    assert_eq!(panic_freedom::METHOD_EXPECT, "method:expect");
    assert_eq!(panic_freedom::METHOD_UNWRAP_ERR, "method:unwrap_err");
}

#[test]
fn result_predicate_concept_aliases_normalize_to_v1_wire_tokens() {
    assert_eq!(
        panic_freedom::normalize_result_predicate_name(panic_freedom::IS_OK),
        panic_freedom::IS_OK
    );
    assert_eq!(
        panic_freedom::normalize_result_predicate_name(panic_freedom::IS_OK_CONCEPT),
        panic_freedom::IS_OK
    );
    assert_eq!(
        panic_freedom::normalize_result_predicate_name(panic_freedom::IS_ERR),
        panic_freedom::IS_ERR
    );
    assert_eq!(
        panic_freedom::normalize_result_predicate_name(panic_freedom::IS_ERR_CONCEPT),
        panic_freedom::IS_ERR
    );
    assert_eq!(
        panic_freedom::normalize_result_predicate_name("concept:panic-freedom.result.OK"),
        "concept:panic-freedom.result.OK"
    );
    assert_eq!(
        panic_freedom::normalize_result_predicate_name("concept:panic-freedom.result.ok "),
        "concept:panic-freedom.result.ok "
    );
}

#[test]
fn option_predicate_concept_aliases_normalize_to_v1_wire_tokens() {
    assert_eq!(
        panic_freedom::normalize_option_predicate_name(panic_freedom::IS_SOME),
        panic_freedom::IS_SOME
    );
    assert_eq!(
        panic_freedom::normalize_option_predicate_name(panic_freedom::IS_SOME_CONCEPT),
        panic_freedom::IS_SOME
    );
    assert_eq!(
        panic_freedom::normalize_option_predicate_name(panic_freedom::IS_NONE),
        panic_freedom::IS_NONE
    );
    assert_eq!(
        panic_freedom::normalize_option_predicate_name(panic_freedom::IS_NONE_CONCEPT),
        panic_freedom::IS_NONE
    );

    assert_eq!(
        panic_freedom::normalize_option_predicate_name("concept:panic-freedom.option.SOME"),
        "concept:panic-freedom.option.SOME"
    );
    assert_eq!(
        panic_freedom::normalize_option_predicate_name("concept:panic-freedom.option.some "),
        "concept:panic-freedom.option.some "
    );
}
