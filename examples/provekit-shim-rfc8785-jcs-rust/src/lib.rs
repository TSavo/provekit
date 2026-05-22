// SPDX-License-Identifier: Apache-2.0
//
// provekit-shim-rfc8785-jcs-rust: RFC 8785's @sugar shim.
//
// Realizes `concept:rfc8785-jcs-encode` by inlining the RFC 8785 JSON
// Canonicalization Scheme algorithm. The spec is the boundary contract
// (boundary:rfc8785-canonical-json); each language has its own shim
// providing the same algorithm via its native JSON value type.
//
// Sister shims: provekit-shim-rfc8785-jcs-typescript, -python, -jvm,
// etc. — all attest the same concept via their language's JSON
// libraries / value types. Cross-platform consumers declare @boundary
// pointing at one of these per target.

use serde_json::Value;

/// `concept:rfc8785-jcs-encode` — RFC 8785's sugar. JCS-encode a
/// JSON value to its canonical byte string. Rules:
///
/// * Object keys sorted by Unicode code-point order (byte order for
///   ASCII keys, which the substrate's IR uses exclusively).
/// * Numbers: integers serialized as plain decimal digits.
/// * Strings: UTF-8 verbatim; escape `"`, `\\`, and U+0000..U+001F
///   as `\u00XX` (lowercase hex).
/// * `true` / `false` / `null` literal.
/// * No whitespace.
///
/// Note: the substrate builds `serde_json` with `preserve_order`, so
/// this implementation explicitly collect-and-sorts object keys
/// rather than relying on Map iteration order.
#[provekit::sugar(
    concept = "concept:rfc8785-jcs-encode",
    library = "serde_json",
    version = "1",
    family = "concept:family:json-canonicalization",
    // Inherits loss from recursive encode_value call (per-value, not per-top-level).
    loss = ["rfc8785-number-serialization-non-ecma262"],
)]
pub fn encode_jcs(v: &Value) -> String {
    let mut out = String::new();
    encode_value(v, &mut out);
    out
}

#[provekit::sugar(
    concept = "concept:rfc8785-jcs-encode-value",
    library = "provekit-shim-rfc8785-jcs-rust",
    version = "0.1",
    family = "concept:family:json-canonicalization",
    // Substrate-honest loss: serde_json::Number::to_string() does NOT emit
    // ECMA-262 §7.1.12.1 conformant strings (RFC 8785 §3.2.2.3 requires it).
    // Recoverable by minting concept:ecma262-number-format and substituting.
    loss = ["rfc8785-number-serialization-non-ecma262"],
)]
fn encode_value(v: &Value, out: &mut String) {
    match v {
        Value::Null => out.push_str("null"),
        Value::Bool(true) => out.push_str("true"),
        Value::Bool(false) => out.push_str("false"),
        Value::Number(n) => out.push_str(&n.to_string()),
        Value::String(s) => encode_string(s, out),
        Value::Array(items) => {
            out.push('[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                encode_value(item, out);
            }
            out.push(']');
        }
        Value::Object(map) => {
            let mut sorted: Vec<(&String, &Value)> = map.iter().collect();
            sorted.sort_by(|a, b| a.0.cmp(b.0));
            out.push('{');
            for (i, (k, val)) in sorted.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                encode_string(k, out);
                out.push(':');
                encode_value(val, out);
            }
            out.push('}');
        }
    }
}

#[provekit::sugar(
    concept = "concept:rfc8785-jcs-encode-string",
    library = "provekit-shim-rfc8785-jcs-rust",
    version = "0.1",
    family = "concept:family:json-canonicalization",
    loss = [],
)]
fn encode_string(s: &str, out: &mut String) {
    out.push('"');
    for c in s.chars() {
        if c == '"' {
            out.push_str("\\\"");
        } else if c == '\\' {
            out.push_str("\\\\");
        } else if (c as u32) < 0x20 {
            const HEX: &[u8; 16] = b"0123456789abcdef";
            let n = c as u32;
            out.push_str("\\u00");
            out.push(HEX[((n >> 4) & 0xF) as usize] as char);
            out.push(HEX[(n & 0xF) as usize] as char);
        } else {
            out.push(c);
        }
    }
    out.push('"');
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn primitives_round_trip() {
        assert_eq!(encode_jcs(&json!(null)), "null");
        assert_eq!(encode_jcs(&json!(true)), "true");
        assert_eq!(encode_jcs(&json!(false)), "false");
        assert_eq!(encode_jcs(&json!(42)), "42");
        assert_eq!(encode_jcs(&json!("hi")), "\"hi\"");
    }

    #[test]
    fn object_keys_sorted() {
        let v = json!({"z": 1, "a": 2});
        assert_eq!(encode_jcs(&v), "{\"a\":2,\"z\":1}");
    }

    #[test]
    fn control_chars_escaped_lowercase_hex() {
        let v = json!("\u{0001}\u{001f}");
        let s = encode_jcs(&v);
        assert_eq!(s, "\"\\u0001\\u001f\"");
    }
}
