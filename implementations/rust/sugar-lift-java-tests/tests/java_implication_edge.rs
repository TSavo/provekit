use serde_json::json;
use sugar_lift_java_tests::{
    lift_java_implication_bridges_from_source, lift_java_jsr380_contracts_from_source,
};

const GOOD_CHAIN: &str = r#"
package demo;

import javax.validation.constraints.Min;

public final class Chain {
    @Min(0)
    static int producer() {
        return 6;
    }

    static int consumer(@Min(0) int value) {
        if (value < 0) {
            throw new IllegalArgumentException("negative");
        }
        return value;
    }

    static int edge() {
        return consumer(producer());
    }
}
"#;

const BAD_CHAIN: &str = r#"
package demo;

import javax.validation.constraints.Min;

public final class Chain {
    @Min(-5)
    static int producer() {
        return -3;
    }

    static int consumer(@Min(0) int value) {
        if (value < 0) {
            throw new IllegalArgumentException("negative");
        }
        return value;
    }

    static int edge() {
        return consumer(producer());
    }
}
"#;

const BROAD_GOOD_CHAIN: &str = r#"
package demo;

import javax.validation.constraints.Max;
import javax.validation.constraints.Min;
import javax.validation.constraints.NotNull;
import javax.validation.constraints.Size;

public final class Chain {
    @Max(10)
    static int maxProducer() {
        return 6;
    }

    static int maxConsumer(@Max(10) int value) {
        return value;
    }

    @Size(min = 1, max = 5)
    static String sizeProducer() {
        return "abc";
    }

    static int sizeConsumer(@Size(min = 1, max = 5) String value) {
        return value.length();
    }

    @NotNull
    static String notNullProducer() {
        return "value";
    }

    static int notNullConsumer(@NotNull String value) {
        return value.length();
    }

    @Min(0)
    @Max(10)
    static int rangeProducer() {
        return 6;
    }

    static int rangeConsumer(@Min(0) @Max(10) int value) {
        return value;
    }
}
"#;

const BROAD_BAD_CHAIN: &str = r#"
package demo;

import javax.validation.constraints.Max;
import javax.validation.constraints.Min;
import javax.validation.constraints.NotNull;
import javax.validation.constraints.Size;

public final class Chain {
    @Max(100)
    static int maxProducer() {
        return 50;
    }

    static int maxConsumer(@Max(10) int value) {
        return value;
    }

    @Size(min = 0, max = 100)
    static String sizeProducer() {
        return "too-long-value";
    }

    static int sizeConsumer(@Size(min = 0, max = 10) String value) {
        return value.length();
    }

    static String nullableProducer() {
        return null;
    }

    static int notNullConsumer(@NotNull String value) {
        return value.length();
    }

    @Min(0)
    @Max(100)
    static int rangeProducer() {
        return 5;
    }

    static int rangeConsumer(@Min(10) @Max(90) int value) {
        return value;
    }
}
"#;

fn find_contract<'a>(ir: &'a [serde_json::Value], name: &str) -> &'a serde_json::Value {
    ir.iter()
        .find(|entry| entry["kind"] == "function-contract" && entry["name"] == name)
        .unwrap_or_else(|| panic!("missing function-contract {name}: {ir:#?}"))
}

fn maybe_contract<'a>(ir: &'a [serde_json::Value], name: &str) -> Option<&'a serde_json::Value> {
    ir.iter()
        .find(|entry| entry["kind"] == "function-contract" && entry["name"] == name)
}

fn atomic_names(formula: &serde_json::Value, out: &mut Vec<String>) {
    match formula["kind"].as_str() {
        Some("atomic") => {
            if let Some(name) = formula["name"].as_str() {
                out.push(name.to_string());
            }
        }
        Some("and") => {
            for operand in formula["operands"].as_array().into_iter().flatten() {
                atomic_names(operand, out);
            }
        }
        _ => {}
    }
}

fn assert_has_atomic(formula: &serde_json::Value, name: &str) {
    let mut names = Vec::new();
    atomic_names(formula, &mut names);
    assert!(
        names.iter().any(|candidate| candidate == name),
        "expected formula to contain atomic `{name}`, saw {names:?}: {formula:#?}"
    );
}

#[test]
fn jsr380_min_contracts_lift_return_post_and_param_pre() {
    let good = lift_java_jsr380_contracts_from_source(GOOD_CHAIN, "src/main/java/demo/Chain.java")
        .expect("lift good contracts");
    assert!(good.diagnostics.is_empty(), "{:#?}", good.diagnostics);

    let producer = find_contract(&good.ir, "producer");
    assert_eq!(producer["post"]["name"], "≥");
    assert_eq!(
        producer["post"]["args"][0],
        json!({"kind": "var", "name": "result"})
    );
    assert_eq!(producer["post"]["args"][1]["value"], 0);
    assert_eq!(producer["bridgeSourceSymbol"], "producer");

    let consumer = find_contract(&good.ir, "consumer");
    assert_eq!(consumer["pre"]["name"], "≥");
    assert_eq!(
        consumer["pre"]["args"][0],
        json!({"kind": "var", "name": "value"})
    );
    assert_eq!(consumer["pre"]["args"][1]["value"], 0);
    assert_eq!(consumer["formals"], json!(["value"]));

    let bad = lift_java_jsr380_contracts_from_source(BAD_CHAIN, "src/main/java/demo/Chain.java")
        .expect("lift bad contracts");
    let weakened_producer = find_contract(&bad.ir, "producer");
    assert_eq!(
        weakened_producer["post"]["args"][1]["value"], -5,
        "weakening the producer post must be visible to the post|=pre edge"
    );
}

#[test]
fn jsr380_breadth_lifts_max_size_notnull_and_combined_range() {
    let good =
        lift_java_jsr380_contracts_from_source(BROAD_GOOD_CHAIN, "src/main/java/demo/Chain.java")
            .expect("lift good broad contracts");
    assert!(good.diagnostics.is_empty(), "{:#?}", good.diagnostics);

    let max_producer = find_contract(&good.ir, "maxProducer");
    assert_eq!(max_producer["post"]["name"], "≤");
    assert_eq!(max_producer["post"]["args"][1]["value"], 10);
    let max_consumer = find_contract(&good.ir, "maxConsumer");
    assert_eq!(max_consumer["pre"]["name"], "≤");
    assert_eq!(max_consumer["pre"]["args"][1]["value"], 10);

    let size_producer = find_contract(&good.ir, "sizeProducer");
    assert_eq!(size_producer["post"]["kind"], "and");
    assert_has_atomic(&size_producer["post"], "≥");
    assert_has_atomic(&size_producer["post"], "≤");
    assert_eq!(
        size_producer["post"]["operands"][0]["args"][0],
        json!({"kind": "ctor", "name": "length", "args": [{"kind": "var", "name": "result"}]})
    );
    let size_consumer = find_contract(&good.ir, "sizeConsumer");
    assert_eq!(size_consumer["pre"]["kind"], "and");
    assert_has_atomic(&size_consumer["pre"], "≥");
    assert_has_atomic(&size_consumer["pre"], "≤");

    let not_null_producer = find_contract(&good.ir, "notNullProducer");
    assert_eq!(not_null_producer["post"]["name"], "not-null");
    let not_null_consumer = find_contract(&good.ir, "notNullConsumer");
    assert_eq!(not_null_consumer["pre"]["name"], "not-null");

    let range_producer = find_contract(&good.ir, "rangeProducer");
    assert_eq!(range_producer["post"]["kind"], "and");
    assert_has_atomic(&range_producer["post"], "≥");
    assert_has_atomic(&range_producer["post"], "≤");
    let range_consumer = find_contract(&good.ir, "rangeConsumer");
    assert_eq!(range_consumer["pre"]["kind"], "and");
    assert_has_atomic(&range_consumer["pre"], "≥");
    assert_has_atomic(&range_consumer["pre"], "≤");
}

#[test]
fn jsr380_breadth_discrimination_surfaces_weakened_posts_and_nullness_silence() {
    let bad =
        lift_java_jsr380_contracts_from_source(BROAD_BAD_CHAIN, "src/main/java/demo/Chain.java")
            .expect("lift bad broad contracts");
    assert!(bad.diagnostics.is_empty(), "{:#?}", bad.diagnostics);

    let max_producer = find_contract(&bad.ir, "maxProducer");
    assert_eq!(
        max_producer["post"]["args"][1]["value"], 100,
        "weakened @Max producer post must stay visible to the implication edge"
    );
    let max_consumer = find_contract(&bad.ir, "maxConsumer");
    assert_eq!(max_consumer["pre"]["args"][1]["value"], 10);

    let size_producer = find_contract(&bad.ir, "sizeProducer");
    assert_eq!(
        size_producer["post"]["operands"][1]["args"][1]["value"], 100,
        "weakened @Size max must stay visible to the implication edge"
    );
    let size_consumer = find_contract(&bad.ir, "sizeConsumer");
    assert_eq!(size_consumer["pre"]["operands"][1]["args"][1]["value"], 10);

    assert!(
        maybe_contract(&bad.ir, "nullableProducer").is_none(),
        "unannotated return must not manufacture a @NotNull post"
    );
    let not_null_consumer = find_contract(&bad.ir, "notNullConsumer");
    assert_eq!(not_null_consumer["pre"]["name"], "not-null");

    let range_producer = find_contract(&bad.ir, "rangeProducer");
    assert_eq!(range_producer["post"]["kind"], "and");
    assert_eq!(range_producer["post"]["operands"][0]["args"][1]["value"], 0);
    assert_eq!(
        range_producer["post"]["operands"][1]["args"][1]["value"],
        100
    );
    let range_consumer = find_contract(&bad.ir, "rangeConsumer");
    assert_eq!(range_consumer["pre"]["operands"][0]["args"][1]["value"], 10);
    assert_eq!(range_consumer["pre"]["operands"][1]["args"][1]["value"], 90);
}

#[test]
fn java_implication_bridge_emits_consumer_callsite_with_producer_arg() {
    let bindings = vec![
        json!({
            "name": "consumer",
            "contract_cid": "blake3-512:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
        }),
        json!({
            "name": "producer",
            "contract_cid": "blake3-512:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
        }),
    ];
    let out = lift_java_implication_bridges_from_source(
        GOOD_CHAIN,
        "src/main/java/demo/Chain.java",
        &bindings,
    )
    .expect("lift bridge");

    assert!(out.diagnostics.is_empty(), "{:#?}", out.diagnostics);
    let bridge = out
        .ir
        .iter()
        .find(|entry| entry["kind"] == "bridge" && entry["sourceSymbol"] == "consumer")
        .unwrap_or_else(|| panic!("missing consumer bridge: {:#?}", out.ir));
    assert_eq!(bridge["targetContractCid"], bindings[0]["contract_cid"]);
    assert_eq!(bridge["sourceLayer"], "java");
    assert_eq!(bridge["targetLayer"], "java-jsr380-contracts");
    assert_eq!(bridge["callsite"]["file"], "src/main/java/demo/Chain.java");
}
