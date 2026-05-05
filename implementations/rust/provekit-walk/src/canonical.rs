// SPDX-License-Identifier: Apache-2.0
//
// Bridge from `provekit_ir_types::IrFormula` and `provekit_ir_types::IrTerm`
// (which are serde-serializable) into `provekit_canonicalizer::Value`
// (which is what the JCS encoder consumes).
//
// We go through `serde_json::Value` as a structural intermediate. The IR
// types' serde representation already produces the canonical JSON shape
// (tagged unions with `kind`, etc.) that v1.5.0 mementos expect.

use std::sync::Arc;

use provekit_canonicalizer::{blake3_512_of, encode_jcs, Value};
use provekit_ir_types::{IrFormula, IrTerm};
use serde_json::Value as JsonValue;

/// Convert a serde_json::Value tree into the canonicalizer's Arc<Value> tree.
/// Object key order is preserved at build time; the JCS encoder re-sorts at
/// emit time, so order here does not affect the resulting bytes.
pub fn serde_to_canonical(j: JsonValue) -> Arc<Value> {
    match j {
        JsonValue::Null => Value::null(),
        JsonValue::Bool(b) => Value::boolean(b),
        JsonValue::Number(n) => match n.as_i64() {
            Some(i) => Value::integer(i),
            None => {
                // Fall back to string for floats/u64-out-of-i64-range. The IR's
                // const-int sort is the only numeric path the MVP exercises;
                // anything else is a placeholder.
                Value::string(n.to_string())
            }
        },
        JsonValue::String(s) => Value::string(s),
        JsonValue::Array(items) => {
            let mapped: Vec<Arc<Value>> = items.into_iter().map(serde_to_canonical).collect();
            Value::array(mapped)
        }
        JsonValue::Object(map) => {
            let entries: Vec<(String, Arc<Value>)> = map
                .into_iter()
                .map(|(k, v)| (k, serde_to_canonical(v)))
                .collect();
            Value::object(entries)
        }
    }
}

/// Canonicalize an `IrFormula` into a JCS-canonicalizer Value tree.
pub fn formula_to_canonical(f: &IrFormula) -> Arc<Value> {
    let serde =
        serde_json::to_value(f).expect("IrFormula serializes (provekit-ir-types is generated)");
    serde_to_canonical(serde)
}

/// Canonicalize an `IrTerm` into a JCS-canonicalizer Value tree.
pub fn term_to_canonical(t: &IrTerm) -> Arc<Value> {
    let serde =
        serde_json::to_value(t).expect("IrTerm serializes (provekit-ir-types is generated)");
    serde_to_canonical(serde)
}

/// Compute the BLAKE3-512 CID of a canonicalizer Value, JCS-encoded.
/// Returns the spec's `"blake3-512:<hex>"` self-identifying string.
pub fn cid_of_value(v: &Value) -> String {
    blake3_512_of(encode_jcs(v).as_bytes())
}

/// Encode a canonicalizer Value to JCS bytes.
pub fn jcs_bytes_of_value(v: &Value) -> Vec<u8> {
    encode_jcs(v).into_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wp::{atomic_ge, const_int, var};

    #[test]
    fn formula_canonical_is_deterministic() {
        let f = atomic_ge(var("y"), const_int(10)).into_formula();
        let v1 = formula_to_canonical(&f);
        let v2 = formula_to_canonical(&f);
        assert_eq!(cid_of_value(&v1), cid_of_value(&v2));
    }

    #[test]
    fn distinct_formulas_distinct_cids() {
        let f1 = atomic_ge(var("y"), const_int(10)).into_formula();
        let f2 = atomic_ge(const_int(42), const_int(10)).into_formula();
        let v1 = formula_to_canonical(&f1);
        let v2 = formula_to_canonical(&f2);
        assert_ne!(cid_of_value(&v1), cid_of_value(&v2));
    }
}
