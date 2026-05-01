// SPDX-License-Identifier: Apache-2.0
//
// Protocol catalog wiring for the CLI.
//
// The CLI declares conformance to a single protocol catalog CID. We
// hard-code the expected CID here AND ship the catalog JSON bytes via
// `include_bytes!` so `verify-protocol` can recompute the CID from
// what the binary actually carries. If the recompute doesn't match the
// expected constant, the binary itself is corrupt or drifted; the
// subcommand surfaces that loud.

use std::sync::Arc;

use anyhow::{anyhow, Context, Result};
use provekit_canonicalizer::{blake3_512_of, encode_jcs, Value};
use serde_json::Value as Json;

/// The protocol catalog CID this CLI declares conformance to. Kept in
/// sync with `protocol/specs/2026-04-30-protocol-versioning.md`. If
/// the catalog changes, bump this string AND ship a new CLI.
pub const EXPECTED_CATALOG_CID: &str =
    "blake3-512:9d57c5e47083b92e8cc5dab365a718fc0afee6556d34ffe40b303dd7ad4d9caa88dbbc6248e318cc76e57b30a0b2ad49f6f9dbf1916ac164a89df44324d6c106";

/// Catalog JSON bytes embedded at compile time. The CLI never reads
/// the on-disk spec file at runtime; `verify-protocol` recomputes from
/// the embedded copy so the answer is about what the binary IS, not
/// where it was invoked from.
pub const EMBEDDED_CATALOG_BYTES: &[u8] =
    include_bytes!("../assets/protocol-catalog.json");

/// Recompute the embedded catalog's CID using the same routine
/// `tools/recompute-spec-cids` uses: parse JSON, JCS-encode, BLAKE3-512.
pub fn compute_embedded_catalog_cid() -> Result<String> {
    let json: Json = serde_json::from_slice(EMBEDDED_CATALOG_BYTES)
        .context("parse embedded protocol catalog JSON")?;
    let canonical = json_to_value(&json)?;
    let jcs = encode_jcs(&canonical);
    Ok(blake3_512_of(jcs.as_bytes()))
}

fn json_to_value(j: &Json) -> Result<Arc<Value>> {
    Ok(match j {
        Json::Null => Value::null(),
        Json::Bool(b) => Value::boolean(*b),
        Json::Number(n) => {
            let i = n
                .as_i64()
                .ok_or_else(|| anyhow!("non-i64 number in catalog: {n}"))?;
            Value::integer(i)
        }
        Json::String(s) => Value::string(s.clone()),
        Json::Array(items) => {
            let mut out: Vec<Arc<Value>> = Vec::with_capacity(items.len());
            for it in items {
                out.push(json_to_value(it)?);
            }
            Value::array(out)
        }
        Json::Object(map) => {
            let mut entries: Vec<(String, Arc<Value>)> = Vec::with_capacity(map.len());
            for (k, v) in map {
                entries.push((k.clone(), json_to_value(v)?));
            }
            Value::object(entries)
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_catalog_recomputes_to_expected_cid() {
        let cid = compute_embedded_catalog_cid().expect("recompute");
        assert_eq!(
            cid, EXPECTED_CATALOG_CID,
            "embedded catalog CID drifted from expected; either the catalog \
             file in the crate's assets/ or the EXPECTED_CATALOG_CID constant \
             is out of date"
        );
    }

    #[test]
    fn embedded_catalog_is_valid_json() {
        let v: Json = serde_json::from_slice(EMBEDDED_CATALOG_BYTES).expect("parse");
        assert!(v.is_object(), "catalog must be a JSON object");
        let kind = v
            .get("kind")
            .and_then(|x| x.as_str())
            .expect("kind field");
        assert_eq!(kind, "catalog");
    }

    #[test]
    fn expected_cid_has_correct_shape() {
        // "blake3-512:" + 128 hex chars.
        assert!(EXPECTED_CATALOG_CID.starts_with("blake3-512:"));
        let hex = &EXPECTED_CATALOG_CID["blake3-512:".len()..];
        assert_eq!(hex.len(), 128);
        assert!(hex.chars().all(|c| c.is_ascii_hexdigit()));
    }
}
