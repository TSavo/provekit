use serde_json::json;
use sugar_canonicalizer::{blake3_512_of, encode_jcs, Value as CValue};
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

const BODYGUARD_CHAIN: &str = r#"
package demo;

public final class Chain {
    static int digit(int radix) {
        if (radix < Character.MIN_RADIX || radix > Character.MAX_RADIX) {
            throw new IllegalArgumentException("radix");
        }
        return radix;
    }

    static int goodEdge() {
        return digit(16);
    }

    static int badEdge() {
        return digit(1);
    }
}
"#;

const OBJECT_BODYGUARD_CHAIN: &str = r#"
package demo;

public final class ObjectGuard {
    static final class Fool {
        Fool(int value) {}
    }

    static int distinct(Fool lhs, Fool rhs) {
        if (lhs == rhs) {
            throw new IllegalArgumentException("same");
        }
        return 1;
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

fn cvalue_from_json(value: &serde_json::Value) -> std::sync::Arc<CValue> {
    match value {
        serde_json::Value::Null => CValue::null(),
        serde_json::Value::Bool(value) => CValue::boolean(*value),
        serde_json::Value::Number(number) => CValue::integer(
            number
                .as_i64()
                .expect("canonical formulas use integer numbers"),
        ),
        serde_json::Value::String(value) => CValue::string(value.clone()),
        serde_json::Value::Array(values) => {
            CValue::array(values.iter().map(cvalue_from_json).collect())
        }
        serde_json::Value::Object(values) => CValue::object(
            values
                .iter()
                .map(|(key, value)| (key.clone(), cvalue_from_json(value))),
        ),
    }
}

fn formula_jcs(formula: &serde_json::Value) -> String {
    encode_jcs(&cvalue_from_json(formula))
}

fn formula_cid(formula: &serde_json::Value) -> String {
    blake3_512_of(formula_jcs(formula).as_bytes())
}

fn rust_radix_precondition_formula() -> serde_json::Value {
    let item_fn: syn::ItemFn = syn::parse_str(
        r#"
        fn to_digit(radix: u32) {
            assert!(
                radix >= 2 && radix <= 36,
                "to_digit: invalid radix"
            );
        }
        "#,
    )
    .expect("parse rust function");
    let pre = sugar_walk::lift::lift_function_precondition(&item_fn);
    serde_json::to_value(pre.as_formula()).expect("serialize rust precondition")
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
fn java_bodyguard_throw_lifts_to_precondition_without_annotations() {
    let out =
        lift_java_jsr380_contracts_from_source(BODYGUARD_CHAIN, "src/main/java/demo/Chain.java")
            .expect("lift bodyguard contracts");
    assert!(out.diagnostics.is_empty(), "{:#?}", out.diagnostics);

    let digit = find_contract(&out.ir, "digit");
    assert_eq!(digit["formals"], json!(["radix"]));
    assert_eq!(
        digit["source"]["contractSource"],
        "java-source-bodyguard-precondition"
    );
    assert_eq!(digit["pre"]["kind"], "and");
    assert_eq!(digit["pre"]["operands"][0]["name"], "≥");
    assert_eq!(
        digit["pre"]["operands"][0]["args"][0],
        json!({"kind": "var", "name": "radix"})
    );
    assert_eq!(digit["pre"]["operands"][0]["args"][1]["value"], 2);
    assert_eq!(digit["pre"]["operands"][1]["name"], "≤");
    assert_eq!(
        digit["pre"]["operands"][1]["args"][0],
        json!({"kind": "var", "name": "radix"})
    );
    assert_eq!(digit["pre"]["operands"][1]["args"][1]["value"], 36);

    let good_edge = find_contract(&out.ir, "goodEdge");
    assert_eq!(good_edge["post"]["name"], "=");
    assert_eq!(good_edge["post"]["args"][1]["name"], "digit");
    assert_eq!(good_edge["post"]["args"][1]["args"][0]["value"], 16);

    let bad_edge = find_contract(&out.ir, "badEdge");
    assert_eq!(bad_edge["post"]["name"], "=");
    assert_eq!(bad_edge["post"]["args"][1]["name"], "digit");
    assert_eq!(bad_edge["post"]["args"][1]["args"][0]["value"], 1);
}

#[test]
fn java_bodyguard_precondition_formula_is_byte_identical_to_rust() {
    let out =
        lift_java_jsr380_contracts_from_source(BODYGUARD_CHAIN, "src/main/java/demo/Chain.java")
            .expect("lift bodyguard contracts");
    let java_pre = find_contract(&out.ir, "digit")["pre"].clone();
    let rust_pre = rust_radix_precondition_formula();
    let java_pre_jcs = formula_jcs(&java_pre);
    let rust_pre_jcs = formula_jcs(&rust_pre);
    let java_pre_cid = formula_cid(&java_pre);
    let rust_pre_cid = formula_cid(&rust_pre);

    println!("java_pre_cid={java_pre_cid}");
    println!("rust_pre_cid={rust_pre_cid}");

    assert_eq!(
        java_pre_jcs, rust_pre_jcs,
        "canonical JCS bytes must match for federation"
    );
    assert_eq!(
        java_pre_cid, rust_pre_cid,
        "same formula bytes must produce the same CID"
    );
}

#[test]
fn java_bodyguard_object_reference_eq_lifts_as_dispatch_precondition() {
    let out = lift_java_jsr380_contracts_from_source(
        OBJECT_BODYGUARD_CHAIN,
        "src/main/java/demo/ObjectGuard.java",
    )
    .expect("lift object bodyguard contracts");
    assert!(out.diagnostics.is_empty(), "{:#?}", out.diagnostics);
    let contract = find_contract(&out.ir, "distinct");
    assert_eq!(
        contract["source"]["contractSource"],
        "java-source-bodyguard-precondition"
    );
    let pre = &contract["pre"];
    assert_eq!(pre["name"], "=");
    assert_eq!(pre["args"][0]["name"], "call:eq:Fool");
    assert_eq!(
        pre["args"][0]["args"][0],
        json!({"kind": "var", "name": "lhs"})
    );
    assert_eq!(
        pre["args"][0]["args"][1],
        json!({"kind": "var", "name": "rhs"})
    );
    assert_eq!(
        pre["args"][1],
        json!({"kind": "const", "sort": {"kind": "primitive", "name": "Bool"}, "value": false})
    );
}

#[test]
fn java_implication_bridge_carries_bodyguard_actuals_for_good_and_bad_callers() {
    let bindings = vec![json!({
        "name": "digit",
        "contract_cid": "blake3-512:dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd"
    })];
    let out = lift_java_implication_bridges_from_source(
        BODYGUARD_CHAIN,
        "src/main/java/demo/Chain.java",
        &bindings,
    )
    .expect("lift bodyguard bridges");
    assert!(out.diagnostics.is_empty(), "{:#?}", out.diagnostics);

    let mut actuals = out
        .ir
        .iter()
        .filter(|entry| entry["kind"] == "bridge" && entry["sourceSymbol"] == "digit")
        .map(|entry| entry["callsite"]["formalActuals"]["radix"]["value"].as_i64())
        .collect::<Vec<_>>();
    actuals.sort();
    println!("formalActuals.radix={actuals:?}");
    assert_eq!(actuals, vec![Some(1), Some(16)]);
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
