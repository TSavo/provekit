// SPDX-License-Identifier: Apache-2.0
//
// JCS-JSON encoder (RFC 8785 / "JSON Canonicalization Scheme"). v1.
// Mirrors implementations/cpp/provekit/canonicalizer/jcs.cpp 1:1.
//
// Rules (RFC 8785 + protocol/specs/2026-04-30-canonicalization-grammar.md
// pass 7):
//   - Object keys sorted by Unicode code-point order. For ASCII-only
//     keys this collapses to byte-order; the protocol's keys are all
//     ASCII so byte-order suffices.
//   - Numbers: integers serialized as plain decimal digits (we only
//     carry i64; floats are not produced by the kit/mint flow).
//   - Strings: UTF-8 verbatim, escape `"` and `\\` and U+0000..U+001F
//     as `\u00XX` (lowercase hex). RFC 8785 also permits the named
//     short escapes (\n etc.) but the C++ peer chose `\u00XX` for
//     determinism; we match.
//   - true / false / null verbatim.
//   - No whitespace anywhere.

use crate::value::Value;

pub fn encode_jcs(v: &Value) -> String {
    let mut out = String::new();
    encode_value(v, &mut out);
    out
}

fn encode_value(v: &Value, out: &mut String) {
    match v {
        Value::Null => out.push_str("null"),
        Value::Bool(true) => out.push_str("true"),
        Value::Bool(false) => out.push_str("false"),
        Value::Integer(n) => {
            // i64 toString. Matches ECMA-262 ToString applied to a finite
            // integer. We do not produce floats from the kit, so this
            // covers all integer cases.
            out.push_str(&n.to_string());
        }
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
        Value::Object(entries) => {
            // Sort by key (byte-order sort works for the ASCII keys
            // this protocol uses).
            let mut sorted: Vec<&(String, std::sync::Arc<Value>)> = entries.iter().collect();
            sorted.sort_by(|a, b| a.0.cmp(&b.0));
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

fn encode_string(s: &str, out: &mut String) {
    out.push('"');
    for b in s.as_bytes() {
        let c = *b;
        if c == b'"' {
            out.push_str("\\\"");
        } else if c == b'\\' {
            out.push_str("\\\\");
        } else if c < 0x20 {
            // U+0000..U+001F as \u00XX with lowercase hex
            const HEX: &[u8; 16] = b"0123456789abcdef";
            out.push_str("\\u00");
            out.push(HEX[((c >> 4) & 0xF) as usize] as char);
            out.push(HEX[(c & 0xF) as usize] as char);
        } else {
            // Verbatim UTF-8 byte
            out.push(c as char);
        }
    }
    out.push('"');
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::value::Value;

    #[test]
    fn encode_simple_object_sorts_keys() {
        let v = Value::object([
            ("b", Value::integer(1)),
            ("a", Value::string("x")),
        ]);
        assert_eq!(encode_jcs(&v), r#"{"a":"x","b":1}"#);
    }

    #[test]
    fn encode_nested_array_object() {
        let v = Value::object([
            ("xs", Value::array(vec![Value::integer(1), Value::integer(2)])),
        ]);
        assert_eq!(encode_jcs(&v), r#"{"xs":[1,2]}"#);
    }

    #[test]
    fn escape_quotes_and_backslash() {
        let v = Value::string(r#"a"b\c"#);
        assert_eq!(encode_jcs(&v), r#""a\"b\\c""#);
    }

    #[test]
    fn empty_object_and_array() {
        assert_eq!(encode_jcs(&Value::object([] as [(String, _); 0])), "{}");
        assert_eq!(encode_jcs(&Value::array(vec![])), "[]");
    }
}
