// SPDX-License-Identifier: Apache-2.0
//
// provekit-shim-serde-json-rust: serde_json's @sugar shim.
//
// Realizes RFC 8259 JSON parse and emit via the `serde_json` crate.
// Sister shims in other languages wrap their native JSON facility:
//   - TS: provekit-shim-serde-json-typescript wraps `JSON`
//   - Python: provekit-shim-serde-json-python wraps `json`
//   - Java: provekit-shim-serde-json-java wraps Gson / Jackson
// All anchor to the shared boundary contract `boundary:rfc8259-json`.

use serde_json::Value;

pub const PROVEKIT_PROOF_BYTES: &[u8] = include_bytes!(
    "../blake3-512:2887b2f19cc35a2b0381a79cadb43fb69d8a9b9c61062fde7208d54f2d273093b171873cea49cf43459ff6aaa6ebf85fd6c8a3de1d3b070152c158ea0fd9b6b8.proof"
);

/// `concept:json-parse` — serde_json's sugar. Parse one canonical
/// JSON value from a UTF-8 string. Returns `Err` with a human-readable
/// message on parse failure.
#[provekit::sugar(
    concept = "concept:json-parse",
    library = "serde_json",
    version = "1",
    family = "concept:family:json",
    loss = [],
)]
pub fn json_parse(s: &str) -> Result<Value, String> {
    serde_json::from_str(s).map_err(|e| e.to_string())
}

/// `concept:json-serialize` — serde_json's sugar. Serialize a JSON
/// value to its compact RFC 8259 string form. Note: this is NOT
/// canonical; for content-addressing use the JCS shim instead.
#[provekit::sugar(
    concept = "concept:json-serialize",
    library = "serde_json",
    version = "1",
    family = "concept:family:json",
    loss = ["non-canonical-key-order"],
)]
pub fn json_serialize(v: &Value) -> Result<String, String> {
    serde_json::to_string(v).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parse_basic_object() {
        let v = json_parse(r#"{"a":1,"b":[true,null]}"#).unwrap();
        assert_eq!(v["a"], 1);
        assert_eq!(v["b"][0], true);
        assert_eq!(v["b"][1], Value::Null);
    }

    #[test]
    fn parse_error_is_human_readable() {
        let err = json_parse("{not-json").unwrap_err();
        assert!(!err.is_empty());
    }

    #[test]
    fn serialize_round_trip() {
        let original = json!({"x": 42, "y": "hi"});
        let s = json_serialize(&original).unwrap();
        let parsed = json_parse(&s).unwrap();
        assert_eq!(parsed, original);
    }
}
