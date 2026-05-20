// SPDX-License-Identifier: Apache-2.0
//
// Source-transform primitives extracted from `provekit-cli::cmd_materialize`
// per `#1335` (umbrella `#1334`). These functions are the mechanical core of
// turning concept-citation carriers in source files into library-bound source
// by composing the existing LowerKit/realize path. They are surfaced here
// (Phase A) so that the upcoming `SiteTransformKit` trait (Phase B) can be
// built on a stable extraction without disturbing the CLI consumer.
//
// Phase A is a verbatim move: no behavior changes, no formatting changes to
// function bodies, no new error types, no new dependencies. The CLI continues
// to call these via a glob `use` re-export.

use std::sync::Arc;

use provekit_canonicalizer::{blake3_512_of, encode_jcs, Value as CanonicalValue};
use serde_json::Value as Json;

/// A captured stub function block: how many lines it spans, and (when the
/// block is well-formed) the signature-and-close pair used by the
/// substrate-honest signature-preservation path. The consumer's signature
/// (including generics, lifetimes, where-clauses) is the signed claim;
/// the body is what materialize fills from the shim's binding (#1332).
pub struct CapturedStubBlock {
    pub line_count: usize,
    pub signature_and_close: Option<StubSignatureAndClose>,
}

pub struct StubSignatureAndClose {
    /// Concatenated text from the `fn ...` line through (and including) the
    /// opening `{`, exactly as written by the consumer. Used as the
    /// emitted function's signature.
    pub signature_text: String,
    /// Indentation of the closing `}` line as the consumer wrote it.
    /// Reused when re-emitting the closing brace so the materialized
    /// function looks like the consumer's original.
    pub close_indent: String,
}

/// Capture the stub function block that follows a carrier comment, if any.
/// Returns a `CapturedStubBlock` describing how many lines to skip and (if
/// the block is well-formed) the consumer's exact signature text plus the
/// indentation of its closing brace. The caller emits the materialized
/// function by splicing the realize plugin's body into this signature
/// (#1332).
///
/// If no stub function follows the carrier (or the block is unbalanced),
/// `line_count` is 0 and `signature_and_close` is `None`; the caller falls
/// back to emitting the realize plugin's full source as-is.
pub fn capture_stub_function_block(lines: &[&str]) -> CapturedStubBlock {
    let none = CapturedStubBlock {
        line_count: 0,
        signature_and_close: None,
    };
    let Some(first_line) = lines.first() else {
        return none;
    };
    if !line_starts_function_declaration(first_line) {
        return none;
    }
    let mut depth: i32 = 0;
    let mut saw_open = false;
    let mut open_line_idx: Option<usize> = None;
    for (offset, line) in lines.iter().enumerate() {
        for ch in line.chars() {
            match ch {
                '{' => {
                    if !saw_open {
                        open_line_idx = Some(offset);
                    }
                    depth += 1;
                    saw_open = true;
                }
                '}' => {
                    depth -= 1;
                    if saw_open && depth == 0 {
                        let line_count = offset + 1;
                        // Signature is lines[0..=open_line_idx], up to and
                        // including the opening `{`. Trim the opening
                        // brace from the signature text and add it back
                        // separately so we can splice a body between.
                        let Some(open_offset) = open_line_idx else {
                            return CapturedStubBlock {
                                line_count,
                                signature_and_close: None,
                            };
                        };
                        let signature_text =
                            lines[..=open_offset].iter().copied().collect::<String>();
                        let close_indent = leading_indent(lines[offset]).to_string();
                        return CapturedStubBlock {
                            line_count,
                            signature_and_close: Some(StubSignatureAndClose {
                                signature_text,
                                close_indent,
                            }),
                        };
                    }
                }
                _ => {}
            }
        }
    }
    none
}

pub fn leading_indent(line: &str) -> &str {
    let trimmed_len = line.trim_start().len();
    &line[..line.len() - trimmed_len]
}

/// Splice the realize plugin's body into the consumer's stub signature.
/// The realize plugin returns a full function declaration; we drop its
/// signature and keep only the body inside its outermost braces. The
/// consumer's stub signature wraps that body. Substrate-honest: the
/// consumer's signed signature is preserved exactly, only the body
/// changes (#1332).
pub fn splice_realized_body_into_stub_signature(
    stub: &StubSignatureAndClose,
    realized_source: &str,
) -> String {
    let body = extract_function_body(realized_source).unwrap_or_default();
    let mut out = String::new();
    out.push_str(&stub.signature_text);
    if !stub.signature_text.ends_with('\n') {
        out.push('\n');
    }
    for line in body.lines() {
        out.push_str(line);
        out.push('\n');
    }
    out.push_str(&stub.close_indent);
    out.push('}');
    out.push('\n');
    out
}

/// Extract the contents between the outermost `{` and matching `}` of a
/// function source string. The realize plugin's emitted source is a single
/// `fn ... { ... }` declaration; this returns just the inner body, trimmed
/// to the lines between the outermost braces. Body content is returned
/// without surrounding indentation normalization; the caller is responsible
/// for any wrapping or re-indentation.
pub fn extract_function_body(source: &str) -> Option<String> {
    let bytes = source.as_bytes();
    let mut start: Option<usize> = None;
    let mut depth: i32 = 0;
    let mut end: Option<usize> = None;
    for (i, &b) in bytes.iter().enumerate() {
        match b {
            b'{' => {
                if start.is_none() {
                    start = Some(i + 1);
                }
                depth += 1;
            }
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    end = Some(i);
                    break;
                }
            }
            _ => {}
        }
    }
    match (start, end) {
        (Some(s), Some(e)) if e >= s => {
            let raw = &source[s..e];
            Some(raw.trim_matches('\n').to_string())
        }
        _ => None,
    }
}

pub fn line_starts_function_declaration(line: &str) -> bool {
    let trimmed = line.trim_start();
    const KEYWORDS_TO_STRIP: &[&str] = &[
        "pub ", "async ", "const ", "unsafe ", "extern ", "default ",
    ];
    let mut remaining = trimmed;
    loop {
        let mut stripped = false;
        if remaining.starts_with("pub(") {
            if let Some(rest) = remaining.split_once(')').map(|(_, r)| r.trim_start()) {
                remaining = rest;
                stripped = true;
            }
        }
        for kw in KEYWORDS_TO_STRIP {
            if let Some(rest) = remaining.strip_prefix(kw) {
                remaining = rest.trim_start();
                stripped = true;
                break;
            }
        }
        if !stripped {
            break;
        }
    }
    remaining.starts_with("fn ") || remaining.starts_with("fn(")
}

pub fn concept_payload_from_line(line: &str) -> Option<(&str, &str)> {
    let indent_len = line.len() - line.trim_start().len();
    let indent = &line[..indent_len];
    let normalized = strip_comment_prefix(line.trim_start())?;
    normalized
        .strip_prefix("provekit-concept: ")
        .map(str::trim)
        .map(|payload| (indent, payload))
}

pub fn concept_payload_cid_from_line(line: &str) -> Option<&str> {
    let normalized = strip_comment_prefix(line.trim_start())?;
    normalized
        .strip_prefix("provekit-concept-payload-cid: ")
        .map(str::trim)
}

pub fn strip_comment_prefix(line: &str) -> Option<&str> {
    let body = line
        .strip_prefix("//")
        .or_else(|| line.strip_prefix('#'))
        .or_else(|| line.strip_prefix("/*"))?
        .trim_start();
    Some(
        body.trim_end()
            .strip_suffix("*/")
            .map(str::trim_end)
            .unwrap_or(body),
    )
}

pub fn indent_realized_source(source: &str, indent: &str) -> String {
    if indent.is_empty() {
        return source.to_string();
    }
    source
        .split_inclusive('\n')
        .map(|line| {
            if line.trim().is_empty() {
                line.to_string()
            } else {
                format!("{indent}{line}")
            }
        })
        .collect()
}

pub fn verify_payload_cid(payload: &str, declared_cid: &str) -> Result<(), String> {
    let parsed: Json = serde_json::from_str(payload)
        .map_err(|error| format!("parse provekit-concept payload JSON: {error}"))?;
    let canonical = canonical_value_from_json(&parsed)?;
    let actual_cid = blake3_512_of(encode_jcs(canonical.as_ref()).as_bytes());
    if actual_cid != declared_cid {
        return Err(format!(
            "provekit-concept-payload-cid mismatch: declared {declared_cid}, computed {actual_cid}"
        ));
    }
    Ok(())
}

pub fn canonical_value_from_json(value: &Json) -> Result<Arc<CanonicalValue>, String> {
    match value {
        Json::Null => Ok(CanonicalValue::null()),
        Json::Bool(value) => Ok(CanonicalValue::boolean(*value)),
        Json::Number(value) => value.as_i64().map(CanonicalValue::integer).ok_or_else(|| {
            format!("provekit-concept payload contains non-integer number `{value}`")
        }),
        Json::String(value) => Ok(CanonicalValue::string(value)),
        Json::Array(values) => values
            .iter()
            .map(canonical_value_from_json)
            .collect::<Result<Vec<_>, _>>()
            .map(CanonicalValue::array),
        Json::Object(entries) => entries
            .iter()
            .map(|(key, value)| canonical_value_from_json(value).map(|value| (key.clone(), value)))
            .collect::<Result<Vec<_>, _>>()
            .map(CanonicalValue::object),
    }
}

// Permissive-defaults for carrier payloads. The materialize command synthesizes
// defaults for missing carrier-payload fields (function, params, param_types,
// return_type) to reduce friction during development. The substrate-honest
// alternative is refuse-on-missing-fields: require each carrier author to
// provide a complete payload. The permissive shape is intentional for the
// build-pipeline ergonomics; the trade-off is that incomplete carriers may
// produce stub-like realize output. A future strict mode (e.g., a
// --strict-payloads flag) could refuse incomplete carriers up front.
pub fn realize_spec_from_payload(payload: &str) -> Result<Json, String> {
    let mut value: Json = serde_json::from_str(payload)
        .map_err(|error| format!("parse provekit-concept payload JSON: {error}"))?;
    let object = value
        .as_object_mut()
        .ok_or_else(|| "provekit-concept payload must be a JSON object".to_string())?;
    let concept_name = object
        .get("concept_name")
        .or_else(|| object.get("conceptName"))
        .and_then(Json::as_str)
        .ok_or_else(|| "provekit-concept payload missing concept_name".to_string())?
        .to_string();
    object.insert(
        "kind".to_string(),
        Json::String("RealizeRequest".to_string()),
    );
    object.insert("concept_name".to_string(), Json::String(concept_name));
    if !object.contains_key("function") {
        object.insert(
            "function".to_string(),
            Json::String("provekit_materialized".to_string()),
        );
    }
    if !object.contains_key("params") {
        object.insert("params".to_string(), Json::Array(Vec::new()));
    }
    if !object.contains_key("param_types") {
        if let Some(param_types) = object.remove("paramTypes") {
            object.insert("param_types".to_string(), param_types);
        } else {
            object.insert("param_types".to_string(), Json::Array(Vec::new()));
        }
    }
    if !object.contains_key("return_type") {
        if let Some(return_type) = object.remove("returnType") {
            object.insert("return_type".to_string(), return_type);
        } else {
            object.insert("return_type".to_string(), Json::String("void".to_string()));
        }
    }
    if !object.contains_key("named_term_tree") {
        if let Some(named_term_tree) = object.remove("namedTermTree") {
            object.insert("named_term_tree".to_string(), named_term_tree);
        }
    }
    Ok(value)
}
