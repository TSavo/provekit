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

/// `concept:json-serialize` -- serde_json's sugar. Serialize a JSON
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

// ---------------------------------------------------------------------------
// Value-totality wrappers (Phase 2 Tier D-lib)
//
// `serde_json::to_string::<serde_json::Value>` is TOTAL: it ALWAYS returns
// Ok. This is a genuine type-level fact, not a vacuous "total" label:
//
//   1. Map keys: `Value::Object` uses `IndexMap<String, Value>` (or
//      `BTreeMap<String, Value>` with the `preserve_order` feature). Keys
//      are ALWAYS `String` -- serde_json's type guarantees this at
//      construction time. The serializer therefore never encounters a
//      non-string key (the only source of an error in RFC 7159 map
//      serialization).
//
//   2. Numbers: `serde_json::Number` rejects NaN and +/-Infinity at
//      construction time (via `Number::from_f64` returning None for
//      non-finite values). A `Value::Number` is always a finite,
//      representable JSON number. The serializer never encounters a
//      non-representable float.
//
//   3. No IO: `serde_json::to_string` writes to an in-memory `Vec<u8>`
//      buffer. There is no IO syscall and therefore no IO error.
//
//   4. No custom Serialize impls: `Value` uses serde_json's own
//      internally-derived serializer, which has no fallible branches on
//      the `Value` variants.
//
// Consequence: `serde_json::to_string(&v)` with `v: &serde_json::Value`
// ALWAYS returns `Ok(...)`. The `.unwrap()` on such a result CANNOT panic.
// The postcondition `is_ok(result)` is SOUND for this specialization.
//
// SCOPE: this totality applies ONLY to the `serde_json::Value`
// specialization. A generic `serde_json::to_string::<T>` for an arbitrary
// `T: Serialize` can fail (e.g. a `T` with a BTreeMap<i32, V> key, or a
// custom Serialize that returns Err). The concept names below explicitly
// carry "value" to prevent misapplication.
// ---------------------------------------------------------------------------

/// `library:serde-json-to-string-value` -- Value-specialized totality.
///
/// Wraps `serde_json::to_string(&v)` where `v: &serde_json::Value`.
///
/// POSTCONDITION: `is_ok(result)` -- the returned Result is ALWAYS Ok.
/// See module-level comment above for the soundness argument.
///
/// NOTE: the `@sugar` lift emits this as a named contract in the proof
/// catalog. The postcondition `is_ok(result)` is the D-lib fact that the
/// verifier's `callee_post_guard_fact` mechanism reads to establish that
/// a downstream `.unwrap()` on this result CANNOT panic.
#[provekit::sugar(
    concept = "library:serde-json-to-string-value",
    library = "serde_json",
    version = "1",
    family = "concept:family:json",
    loss = [],
    totality = "result_ok",
)]
pub fn serde_json_to_string_value(v: &Value) -> Result<String, serde_json::Error> {
    // This ALWAYS succeeds for `Value` -- see the soundness argument above.
    // The `?` is never taken; it exists only to preserve the Result signature
    // that the verifier's bridge mechanism requires (the unwrap site bridges
    // to result_unwrap whose pre = is_ok(result)).
    serde_json::to_string(v)
}

/// `library:serde-json-to-string-pretty-value` -- Value-specialized totality
/// for pretty-printing.
///
/// Wraps `serde_json::to_string_pretty(&v)` where `v: &serde_json::Value`.
///
/// POSTCONDITION: `is_ok(result)` -- the returned Result is ALWAYS Ok.
/// Same soundness argument as `serde_json_to_string_value` above: same
/// serializer, same Value type invariants, no IO, no custom Serialize.
#[provekit::sugar(
    concept = "library:serde-json-to-string-pretty-value",
    library = "serde_json",
    version = "1",
    family = "concept:family:json",
    loss = [],
    totality = "result_ok",
)]
pub fn serde_json_to_string_pretty_value(v: &Value) -> Result<String, serde_json::Error> {
    serde_json::to_string_pretty(v)
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
