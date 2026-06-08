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

fn find_contract<'a>(ir: &'a [serde_json::Value], name: &str) -> &'a serde_json::Value {
    ir.iter()
        .find(|entry| entry["kind"] == "function-contract" && entry["name"] == name)
        .unwrap_or_else(|| panic!("missing function-contract {name}: {ir:#?}"))
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
