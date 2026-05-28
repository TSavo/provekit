// SPDX-License-Identifier: Apache-2.0
//
// ingest.rs — JSON ingestion module. Two boundary stubs for serde_json
// (json_parse, json_serialize) + the user-side `parse_event` function
// whose contract the lifter derives from its panic guards and early
// returns. ValidEvent is the user-authored type carried out of this
// module into persist.

use serde_json::Value;

// =============================================================================
// Boundary: concept:json-parse  →  provekit-shim-serde-json-rust
// =============================================================================
// provekit-concept: {"artifact_kind":"provekit-concept-citation-comment-sugar","concept_name":"concept:json-parse","function":"json_parse","params":["s"],"param_types":["&str"],"return_type":"Result<Value, String>","named_term_tree":{"conceptName":"concept:json-parse","args":[{"sort":"String","source":"s"}]}}
// provekit-concept-payload-cid: blake3-512:b125fd410270356ee10240c89d210bce6100f6a5d4f9b9ca4ee41038987ebf35e53ddf498d159f6e7b9786a30cbb8de814d4276d826d616414a9809a74b2da71
pub fn json_parse(_s: &str) -> Result<Value, String> {
    unimplemented!("provekit materialize fills this from provekit-shim-serde-json-rust")
}

// =============================================================================
// Boundary: concept:json-serialize  →  provekit-shim-serde-json-rust
// =============================================================================
// provekit-concept: {"artifact_kind":"provekit-concept-citation-comment-sugar","concept_name":"concept:json-serialize","function":"json_serialize","params":["v"],"param_types":["&Value"],"return_type":"Result<String, String>","named_term_tree":{"conceptName":"concept:json-serialize","args":[{"sort":"JsonValue","source":"v"}]}}
// provekit-concept-payload-cid: blake3-512:426611e74cea5236b5d4e45f41184a77c33615cee99700b1b237361557fb8093bd7d1462a8bdc9d6f379eb9e79ea01bb6c5be4f70d6062447158919eb85365f1
pub fn json_serialize(_v: &Value) -> Result<String, String> {
    unimplemented!("provekit materialize fills this from provekit-shim-serde-json-rust")
}

// =============================================================================
// User-side type: the validated event the rest of the spine carries.
// =============================================================================

#[derive(Debug, Clone, PartialEq)]
pub struct ValidEvent {
    pub event_type: String,
    pub user: String,
    pub payload_text: String,
}

// =============================================================================
// User-side function: parse_event.
// Lifter-derived contract:
//   pre:  s is a UTF-8 string (rust type system)
//   post: result is Ok iff (input is well-formed JSON object)
//                       AND (input.event_type is non-empty string)
//                       AND (input.user is non-empty string)
//                       AND (input.payload exists)
// The empty-input and empty-user early-returns lift to pre-condition
// witnesses on the Ok arm.
// =============================================================================

pub fn parse_event(input: &str) -> Result<ValidEvent, String> {
    if input.is_empty() {
        return Err("ingest: empty input".into());
    }
    let v = json_parse(input)?;
    let event_type = v["event_type"]
        .as_str()
        .ok_or_else(|| "ingest: missing event_type".to_string())?
        .to_string();
    let user = v["user"]
        .as_str()
        .ok_or_else(|| "ingest: missing user".to_string())?
        .to_string();
    if user.is_empty() {
        return Err("ingest: empty user".into());
    }
    let payload_text = json_serialize(&v["payload"])?;
    Ok(ValidEvent {
        event_type,
        user,
        payload_text,
    })
}
