// SPDX-License-Identifier: Apache-2.0

use provekit_realize_rust_core::literal_encoding::answers;

// Rust admits: Int, Float, String, Bool, Bytes (no Null) -- 5 answers total.

// Canonical sort CIDs (from #1282)
const SORT_INT_CID: &str = "blake3-512:30ffc51350121a7172f3e4064a33c45bbd345756979fccff6875cd2ab33e4964d098a99df80cfbdf1ec1a0738c5ac3476f0ff8f75589ea511d1acd82c74ecd58";
const SORT_FLOAT_CID: &str = "blake3-512:b979e70c4d5e53d9bdf13d6f08330be3c5b0714b8c770d69bbd05946b86c36df5274be8145a2683cc29c278155c9c1ee65b6897913524eecb9e4c89c71862f57";
const SORT_STRING_CID: &str = "blake3-512:be8721d24849feb74c4721520bdba02d352a94f49253a627cd509127472aa1c47cbe99cb705cac4159b5365abcce0c9aaa4901fe67630827deb6be1f9daeea10";
const SORT_BOOL_CID: &str = "blake3-512:0ee13bf3fd6b7ecfbee72dfbfc18a7c0ea7f1663de6cca43cefb36f5b4c03665452646094a7c296e819e75d683c6ce4821f3d7db3c3c78ae97f2d4e3451d2074";
const SORT_BYTES_CID: &str = "blake3-512:7116ef6e62e6739b213a8394f975a53c771b89f08c36d27143827acfcfebc0e39e5b82c530be668c3cfd5ec6966ccaa42930b37fdb1f4ac25652a970be10fb6b";

// Golden LiteralEncodingMemento CIDs (kit_cid elided per #1262 / #1271)
const MEMENTO_INT_CID: &str = "blake3-512:04bda76dc5b87ebbcb884bcbf84f1bff15722ed1563edd6848794faf22ee05f8fdf0c6f840553c2937d726273128022e28293e5e0e615b52d64ccbce4f6d7958";
const MEMENTO_FLOAT_CID: &str = "blake3-512:67de53852cb249c8a6d80e70ee7a345d90408018672dbe1178eb35aba41df00936fe56d0fa45f5cc9c690a1148e1baa39f3ccff6883500d268d0111e6bf1c034";
const MEMENTO_STRING_CID: &str = "blake3-512:8e1a352b99dab86a374b1466149434af6d916f961ef7ef27cca2706f7667fb4d6efd327facae346cff7c37060d17151dbe4fac94b9a78bbfc35bf4587c905a80";
const MEMENTO_BOOL_CID: &str = "blake3-512:7986c81b3406b5b3925f5290df3f96412098c9d7b4a8b46d6833cea913853179cbbbe08f21bc1b7183a0d64f32903245218904354903ba7be0a40f76576faae9";
const MEMENTO_BYTES_CID: &str = "blake3-512:0983e550f45174901bfc11d35e3f92f65ceb0c5c2d7252f26c53b0e7be6e38e7a003f2b3fcceed1f2a7de0c311913dc1793d09563824557709bdf26e947cd85a";

#[test]
fn rust_literal_encoding_answers_count() {
    let a = answers();
    assert_eq!(a.len(), 5, "Rust admits Int, Float, String, Bool, Bytes (5 sorts)");
}

#[test]
fn rust_literal_encoding_answers_sort_cids_correct() {
    let a = answers();
    let sort_cids: Vec<&str> = a.iter().map(|m| m.sort_cid.as_str()).collect();
    assert!(sort_cids.contains(&SORT_INT_CID), "must contain Int");
    assert!(sort_cids.contains(&SORT_FLOAT_CID), "must contain Float");
    assert!(sort_cids.contains(&SORT_STRING_CID), "must contain String");
    assert!(sort_cids.contains(&SORT_BOOL_CID), "must contain Bool");
    assert!(sort_cids.contains(&SORT_BYTES_CID), "must contain Bytes");
}

#[test]
fn rust_literal_encoding_answers_all_language_rust() {
    let a = answers();
    for m in &a {
        assert_eq!(m.language, "rust", "language must be rust");
    }
}

#[test]
fn rust_literal_encoding_answers_all_kind_literal_encoding_memento() {
    let a = answers();
    for m in &a {
        assert_eq!(m.kind, "literal-encoding-memento");
    }
}

#[test]
fn rust_literal_encoding_answers_golden_cids() {
    // Regression: golden CIDs must not change. Harvested from first run.
    let a = answers();
    let by_sort: std::collections::BTreeMap<&str, &str> =
        a.iter().map(|m| (m.sort_cid.as_str(), m.cid.as_str())).collect();

    assert_eq!(by_sort[SORT_INT_CID], MEMENTO_INT_CID, "Int golden CID");
    assert_eq!(by_sort[SORT_FLOAT_CID], MEMENTO_FLOAT_CID, "Float golden CID");
    assert_eq!(by_sort[SORT_STRING_CID], MEMENTO_STRING_CID, "String golden CID");
    assert_eq!(by_sort[SORT_BOOL_CID], MEMENTO_BOOL_CID, "Bool golden CID");
    assert_eq!(by_sort[SORT_BYTES_CID], MEMENTO_BYTES_CID, "Bytes golden CID");
}

#[test]
fn rust_literal_encoding_answers_cid_not_empty() {
    let a = answers();
    for m in &a {
        assert!(!m.cid.is_empty(), "CID must not be empty for sort {}", m.sort_cid);
        assert!(
            m.cid.starts_with("blake3-512:"),
            "CID must start with blake3-512: for sort {}",
            m.sort_cid
        );
    }
}

#[test]
fn rust_literal_encoding_answers_rpc_dispatch() {
    let response = provekit_realize_rust_core::dispatch(&serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "provekit.plugin.literal_encoding_answers",
        "params": {}
    }));
    assert_eq!(response["jsonrpc"], "2.0");
    assert_eq!(response["id"], 1);
    let answers = &response["result"]["answers"];
    assert!(answers.is_array());
    assert_eq!(answers.as_array().unwrap().len(), 5);
}
