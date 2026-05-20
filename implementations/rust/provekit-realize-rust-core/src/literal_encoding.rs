// SPDX-License-Identifier: Apache-2.0

use provekit_ir_types::LiteralEncodingMemento;
use serde_json::json;

const RUST_KIT_CID: &str = "blake3-512:e3c223b8b6f39382e43cb06c5b04059987e661d96311decd5003d4ec79c7d6f9969de39ae16dd6509cb5236185260d59c63288db7ff772aae00f8123ea826cbd";

// Canonical sort CIDs (from #1282)
const SORT_INT_CID: &str = "blake3-512:30ffc51350121a7172f3e4064a33c45bbd345756979fccff6875cd2ab33e4964d098a99df80cfbdf1ec1a0738c5ac3476f0ff8f75589ea511d1acd82c74ecd58";
const SORT_FLOAT_CID: &str = "blake3-512:b979e70c4d5e53d9bdf13d6f08330be3c5b0714b8c770d69bbd05946b86c36df5274be8145a2683cc29c278155c9c1ee65b6897913524eecb9e4c89c71862f57";
const SORT_STRING_CID: &str = "blake3-512:be8721d24849feb74c4721520bdba02d352a94f49253a627cd509127472aa1c47cbe99cb705cac4159b5365abcce0c9aaa4901fe67630827deb6be1f9daeea10";
const SORT_BOOL_CID: &str = "blake3-512:0ee13bf3fd6b7ecfbee72dfbfc18a7c0ea7f1663de6cca43cefb36f5b4c03665452646094a7c296e819e75d683c6ce4821f3d7db3c3c78ae97f2d4e3451d2074";
const SORT_BYTES_CID: &str = "blake3-512:7116ef6e62e6739b213a8394f975a53c771b89f08c36d27143827acfcfebc0e39e5b82c530be668c3cfd5ec6966ccaa42930b37fdb1f4ac25652a970be10fb6b";
// Rust does not admit Null (no native null at the value layer)

/// Returns the set of LiteralEncodingMemento answers for the Rust kit.
///
/// Rust admits: Int, Float, String, Bool, Bytes (no Null).
/// One memento per admitted sort.
pub fn answers() -> Vec<LiteralEncodingMemento> {
    vec![
        LiteralEncodingMemento::new(
            RUST_KIT_CID.to_string(),
            "rust".to_string(),
            SORT_INT_CID.to_string(),
            "42".to_string(),
            json!(42),
        ),
        // Float: the Rust lift stores IEEE-754 bits as {"__float_bits__": <u64>}.
        // 3.14_f64 -> bits = 4614253070214989087 (0x40091EB851EB851F).
        // Using the actual walk-emitted shape per the bind-lift convention.
        LiteralEncodingMemento::new(
            RUST_KIT_CID.to_string(),
            "rust".to_string(),
            SORT_FLOAT_CID.to_string(),
            "3.14_f64".to_string(),
            json!({"__float_bits__": 4614253070214989087_u64}),
        ),
        LiteralEncodingMemento::new(
            RUST_KIT_CID.to_string(),
            "rust".to_string(),
            SORT_STRING_CID.to_string(),
            "\"hello\"".to_string(),
            json!("hello"),
        ),
        LiteralEncodingMemento::new(
            RUST_KIT_CID.to_string(),
            "rust".to_string(),
            SORT_BOOL_CID.to_string(),
            "true".to_string(),
            json!(true),
        ),
        LiteralEncodingMemento::new(
            RUST_KIT_CID.to_string(),
            "rust".to_string(),
            SORT_BYTES_CID.to_string(),
            "b\"abc\"".to_string(),
            json!("abc"),
        ),
    ]
}
