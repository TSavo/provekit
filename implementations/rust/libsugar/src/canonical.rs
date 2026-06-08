// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use serde::Serialize;
use serde_json::Value as Json;
use sugar_canonicalizer::{blake3_512_of, encode_jcs, Value as CValue};

use crate::{ProvekitError, Result};

pub fn serializable_jcs<T: Serialize>(value: &T) -> Result<String> {
    let json = serde_json::to_value(value)
        .map_err(|e| ProvekitError::Message(format!("serialize JSON: {e}")))?;
    json_jcs(&json)
}

pub fn serializable_cid<T: Serialize>(value: &T) -> Result<String> {
    let jcs = serializable_jcs(value)?;
    Ok(blake3_512_of(jcs.as_bytes()))
}

pub fn json_jcs(value: &Json) -> Result<String> {
    let canonical = json_to_cvalue(value)?;
    Ok(encode_jcs(&canonical))
}

pub fn json_cid(value: &Json) -> Result<String> {
    let jcs = json_jcs(value)?;
    Ok(blake3_512_of(jcs.as_bytes()))
}

/// Canonical operation identity: an op CID is the JSON CID of the op shape.
pub fn op_cid_from_shape(shape: &Json) -> Result<String> {
    json_cid(shape)
}

pub fn is_blake3_512_cid(value: &str) -> bool {
    let Some(hex) = value.strip_prefix("blake3-512:") else {
        return false;
    };
    hex.len() == 128 && hex.chars().all(|c| c.is_ascii_hexdigit())
}

fn json_to_cvalue(value: &Json) -> Result<Arc<CValue>> {
    Ok(match value {
        Json::Null => CValue::null(),
        Json::Bool(b) => CValue::boolean(*b),
        Json::Number(n) => {
            let i = n.as_i64().ok_or_else(|| {
                ProvekitError::Message(format!("non-i64 JSON number cannot be canonicalized: {n}"))
            })?;
            CValue::integer(i)
        }
        Json::String(s) => CValue::string(s.clone()),
        Json::Array(items) => {
            let mut out = Vec::with_capacity(items.len());
            for item in items {
                out.push(json_to_cvalue(item)?);
            }
            CValue::array(out)
        }
        Json::Object(map) => {
            let mut entries = Vec::with_capacity(map.len());
            for (key, item) in map {
                entries.push((key.clone(), json_to_cvalue(item)?));
            }
            CValue::object(entries)
        }
    })
}
