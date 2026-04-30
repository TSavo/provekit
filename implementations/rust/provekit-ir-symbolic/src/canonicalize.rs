//! Test-only canonicalizer stand-in.
//!
//! The real byte-deterministic canonicalizer lives in TS at
//! `src/canonicalizer/serialize.ts` and a Rust port is downstream
//! infrastructure. For this crate, the cross-language equivalence check
//! is the JSON serialization of the IrFormula struct: serde-derived
//! field order matches TS object-literal order, so
//! `serde_json::to_string_pretty(&formula)` round-trips byte-for-byte
//! against a TS-produced fixture for the same logical claim.

use serde::Serialize;
use serde_json::Value as JsonValue;

/// Serialize any IR value to a pretty-printed JSON string. The output is
/// byte-equivalent to TS `JSON.stringify(value, null, 2)` when the IR
/// shape and field order match (which the type definitions enforce).
pub fn to_canonical_json<T: Serialize>(value: &T) -> Result<String, serde_json::Error> {
    serde_json::to_string_pretty(value)
}

/// Round-trip an IR value through serde_json::Value. Useful for fixture
/// comparison: deserialize the expected JSON to a Value, serialize the IR
/// to a Value, compare with `==`. This compares structurally (object
/// key order is normalized) rather than lexically.
pub fn to_json_value<T: Serialize>(value: &T) -> Result<JsonValue, serde_json::Error> {
    serde_json::to_value(value)
}
