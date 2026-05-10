// SPDX-License-Identifier: Apache-2.0
//
// Stage 2 (enumerate_callsites) tests. Pins:
//   - walks contract.body.pre / .post / .inv looking for ctor terms
//     whose `name` matches a known bridge sourceSymbol
//   - non-contract envelopes are skipped
//   - bridges-without-matching-ctors are not callsites
//   - ctor inside an atomic's args triggers a callsite
//   - nested ctor (ctor inside ctor args) also triggers
//   - the callsite carries the bridge's targetContractCid + layers

use serde_json::json;

use provekit_verifier::{enumerate_callsites, MementoPool};

fn pool_with_bridge_and_contract(
    bridge_symbol: &str,
    target_cid: &str,
    contract_body: serde_json::Value,
) -> MementoPool {
    let mut pool = MementoPool::default();

    let bridge_env = json!({
        "evidence": {
            "kind": "bridge",
            "body": {
                "sourceSymbol": bridge_symbol,
                "sourceLayer": "ts",
                "targetContractCid": target_cid,
                "targetLayer": "rust-kit"
            }
        }
    });
    pool.bridges_by_symbol
        .insert(bridge_symbol.into(), bridge_env);

    let contract_env = json!({
        "evidence": {
            "kind": "contract",
            "body": contract_body
        }
    });
    let contract_cid = format!("blake3-512:c-{bridge_symbol}");
    pool.mementos.insert(contract_cid, contract_env);
    pool
}

// ---------------------------------------------------------------------------
// Happy path: ctor inside an atomic's args triggers a callsite
// ---------------------------------------------------------------------------

#[test]
fn finds_ctor_in_atomic_args_in_pre() {
    let target_cid = "blake3-512:target";
    let pool = pool_with_bridge_and_contract(
        "parseInt",
        target_cid,
        json!({
            "contractName": "useParseInt",
            "pre": {
                "kind": "atomic", "name": ">",
                "args": [
                    {"kind": "ctor", "name": "parseInt", "args": [{"kind": "var", "name": "s"}]},
                    {"kind": "const", "value": 0, "sort": {"kind": "primitive", "name": "Int"}}
                ]
            }
        }),
    );
    let cs = enumerate_callsites::run(&pool);
    assert_eq!(cs.len(), 1);
    assert_eq!(cs[0].bridge_ir_name, "parseInt");
    assert_eq!(cs[0].bridge_target_cid, target_cid);
    assert_eq!(cs[0].bridge_source_layer, "ts");
    assert_eq!(cs[0].bridge_target_layer, "rust-kit");
    assert_eq!(cs[0].property_name, "useParseInt");
}

#[test]
fn finds_ctor_in_post_slot() {
    let target_cid = "blake3-512:target";
    let pool = pool_with_bridge_and_contract(
        "parseInt",
        target_cid,
        json!({
            "contractName": "p",
            "post": {
                "kind": "atomic", "name": "=",
                "args": [
                    {"kind": "var", "name": "out"},
                    {"kind": "ctor", "name": "parseInt", "args": [{"kind": "var", "name": "s"}]}
                ]
            }
        }),
    );
    let cs = enumerate_callsites::run(&pool);
    assert_eq!(cs.len(), 1);
}

#[test]
fn finds_ctor_in_inv_slot() {
    let target_cid = "blake3-512:target";
    let pool = pool_with_bridge_and_contract(
        "parseInt",
        target_cid,
        json!({
            "contractName": "p",
            "inv": {
                "kind": "atomic", "name": ">",
                "args": [
                    {"kind": "ctor", "name": "parseInt", "args": [{"kind": "var", "name": "s"}]},
                    {"kind": "const", "value": 0, "sort": {"kind": "primitive", "name": "Int"}}
                ]
            }
        }),
    );
    let cs = enumerate_callsites::run(&pool);
    assert_eq!(cs.len(), 1);
}

#[test]
fn finds_ctor_under_quantifier_body() {
    let target_cid = "blake3-512:target";
    let pool = pool_with_bridge_and_contract(
        "parseInt",
        target_cid,
        json!({
            "contractName": "p",
            "pre": {
                "kind": "forall", "name": "s",
                "sort": {"kind": "primitive", "name": "String"},
                "body": {
                    "kind": "atomic", "name": ">",
                    "args": [
                        {"kind": "ctor", "name": "parseInt", "args": [{"kind": "var", "name": "s"}]},
                        {"kind": "const", "value": 0, "sort": {"kind": "primitive", "name": "Int"}}
                    ]
                }
            }
        }),
    );
    let cs = enumerate_callsites::run(&pool);
    assert_eq!(cs.len(), 1);
}

#[test]
fn finds_ctor_inside_connective_operands() {
    let target_cid = "blake3-512:target";
    let pool = pool_with_bridge_and_contract(
        "parseInt",
        target_cid,
        json!({
            "contractName": "p",
            "pre": {
                "kind": "and",
                "operands": [
                    {"kind": "atomic", "name": ">",
                     "args": [
                         {"kind": "ctor", "name": "parseInt", "args": [{"kind": "var", "name": "s"}]},
                         {"kind": "const", "value": 0, "sort": {"kind": "primitive", "name": "Int"}}
                     ]
                    }
                ]
            }
        }),
    );
    let cs = enumerate_callsites::run(&pool);
    assert_eq!(cs.len(), 1);
}

// ---------------------------------------------------------------------------
// Negative cases
// ---------------------------------------------------------------------------

#[test]
fn no_callsite_when_no_bridges_registered() {
    let mut pool = MementoPool::default();
    pool.mementos.insert(
        "blake3-512:c".into(),
        json!({
            "evidence": {
                "kind": "contract",
                "body": {
                    "contractName": "p",
                    "pre": {
                        "kind": "atomic", "name": ">",
                        "args": [
                            {"kind": "ctor", "name": "parseInt", "args": []},
                            {"kind": "const", "value": 0, "sort": {"kind": "primitive", "name": "Int"}}
                        ]
                    }
                }
            }
        }),
    );
    let cs = enumerate_callsites::run(&pool);
    assert_eq!(cs.len(), 0);
}

#[test]
fn no_callsite_for_ctor_name_with_no_matching_bridge() {
    let target_cid = "blake3-512:target";
    let pool = pool_with_bridge_and_contract(
        "parseInt",
        target_cid,
        json!({
            "contractName": "p",
            "pre": {
                "kind": "atomic", "name": ">",
                "args": [
                    {"kind": "ctor", "name": "atoi", "args": []},
                    {"kind": "const", "value": 0, "sort": {"kind": "primitive", "name": "Int"}}
                ]
            }
        }),
    );
    let cs = enumerate_callsites::run(&pool);
    assert_eq!(cs.len(), 0);
}

#[test]
fn skips_non_contract_envelopes() {
    let mut pool = MementoPool::default();
    // A bridge envelope (kind=bridge): should not be walked.
    pool.mementos.insert(
        "blake3-512:bridge".into(),
        json!({
            "evidence": {
                "kind": "bridge",
                "body": {
                    "sourceSymbol": "parseInt",
                    "pre": {
                        "kind": "atomic", "name": ">",
                        "args": [
                            {"kind": "ctor", "name": "parseInt", "args": []},
                            {"kind": "const", "value": 0, "sort": {"kind": "primitive", "name": "Int"}}
                        ]
                    }
                }
            }
        }),
    );
    pool.bridges_by_symbol.insert(
        "parseInt".into(),
        json!({
            "evidence": {
                "kind": "bridge",
                "body": {
                    "sourceSymbol": "parseInt",
                    "targetContractCid": "blake3-512:t"
                }
            }
        }),
    );
    let cs = enumerate_callsites::run(&pool);
    assert_eq!(cs.len(), 0);
}

#[test]
fn nested_ctor_in_ctor_args_also_finds_callsite() {
    let target_cid = "blake3-512:target";
    let pool = pool_with_bridge_and_contract(
        "parseInt",
        target_cid,
        json!({
            "contractName": "p",
            "pre": {
                "kind": "atomic", "name": "=",
                "args": [
                    {"kind": "ctor", "name": "wrap", "args": [
                        {"kind": "ctor", "name": "parseInt", "args": [{"kind": "var", "name": "s"}]}
                    ]},
                    {"kind": "var", "name": "out"}
                ]
            }
        }),
    );
    let cs = enumerate_callsites::run(&pool);
    assert_eq!(cs.len(), 1);
}

#[test]
fn property_name_falls_back_to_cid_prefix_when_contract_name_absent() {
    let target_cid = "blake3-512:target";
    let mut pool = pool_with_bridge_and_contract(
        "parseInt",
        target_cid,
        json!({
            // no contractName
            "pre": {
                "kind": "atomic", "name": ">",
                "args": [
                    {"kind": "ctor", "name": "parseInt", "args": []},
                    {"kind": "const", "value": 0, "sort": {"kind": "primitive", "name": "Int"}}
                ]
            }
        }),
    );
    // contract CID was set to "blake3-512:c-parseInt" by the helper.
    let cs = enumerate_callsites::run(&pool);
    assert_eq!(cs.len(), 1);
    // Fallback prefix is the first 12 chars of the CID + "...".
    assert!(cs[0].property_name.ends_with("..."));
    assert_eq!(cs[0].property_name.chars().take(12).count(), 12);
    let _ = &mut pool;
}

#[test]
fn callsite_carries_arg_term_from_atomic() {
    let target_cid = "blake3-512:target";
    let pool = pool_with_bridge_and_contract(
        "parseInt",
        target_cid,
        json!({
            "contractName": "p",
            "pre": {
                "kind": "atomic", "name": ">",
                "args": [
                    {"kind": "ctor", "name": "parseInt", "args": [{"kind": "var", "name": "input_string"}]},
                    {"kind": "const", "value": 0, "sort": {"kind": "primitive", "name": "Int"}}
                ]
            }
        }),
    );
    let cs = enumerate_callsites::run(&pool);
    assert_eq!(cs.len(), 1);
    let arg = cs[0].arg_term.as_ref().expect("arg present");
    assert_eq!(arg.get("name").unwrap(), "input_string");
}

#[test]
fn multiple_callsites_in_same_contract_each_listed() {
    let target_cid = "blake3-512:target";
    let pool = pool_with_bridge_and_contract(
        "parseInt",
        target_cid,
        json!({
            "contractName": "p",
            "pre": {
                "kind": "and",
                "operands": [
                    {"kind": "atomic", "name": ">",
                     "args": [
                         {"kind": "ctor", "name": "parseInt", "args": [{"kind": "var", "name": "a"}]},
                         {"kind": "const", "value": 0, "sort": {"kind": "primitive", "name": "Int"}}
                     ]},
                    {"kind": "atomic", "name": ">",
                     "args": [
                         {"kind": "ctor", "name": "parseInt", "args": [{"kind": "var", "name": "b"}]},
                         {"kind": "const", "value": 0, "sort": {"kind": "primitive", "name": "Int"}}
                     ]}
                ]
            }
        }),
    );
    let cs = enumerate_callsites::run(&pool);
    assert_eq!(cs.len(), 2);
}
