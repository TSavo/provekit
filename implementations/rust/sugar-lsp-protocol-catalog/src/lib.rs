// SPDX-License-Identifier: Apache-2.0
//
// Reference computation for the shared ProvekIt LSP protocol catalog CID.
//
// The CID rule mirrors the protocol catalog pattern used by provekit-cli:
// parse JSON, RFC 8785 JCS-canonicalize it with provekit-canonicalizer, then
// compute the self-identifying BLAKE3-512 CID.

use std::fmt;
use std::fs;
use std::path::Path;
use std::sync::Arc;

use sugar_canonicalizer::{blake3_512_of, encode_jcs, Value as CValue};
use serde_json::Value as Json;

pub const LSP_PROTOCOL_CATALOG_REPO_PATH: &str =
    "protocol/catalogs/provekit-lsp-shared-1.catalog.json";

pub const EXPECTED_LSP_PROTOCOL_CATALOG_CID: &str =
    "blake3-512:0e3905c2a7a098cd538b9669428a7dffd2b84ba8ccf8fde3724fe2ab61fd3fbc1e1a616a6b20b6817464cdc50c466b5497d4ac2e2dc34c3c15f05535b463643c";

pub const EMBEDDED_LSP_PROTOCOL_CATALOG_BYTES: &[u8] =
    include_bytes!("../../../../protocol/catalogs/provekit-lsp-shared-1.catalog.json");

#[derive(Debug)]
pub enum ProtocolCatalogError {
    Io(std::io::Error),
    Json(serde_json::Error),
    NonIntegerNumber(String),
}

impl fmt::Display for ProtocolCatalogError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ProtocolCatalogError::Io(err) => write!(f, "I/O error: {err}"),
            ProtocolCatalogError::Json(err) => write!(f, "JSON error: {err}"),
            ProtocolCatalogError::NonIntegerNumber(number) => {
                write!(f, "catalog contains non-i64 JSON number: {number}")
            }
        }
    }
}

impl std::error::Error for ProtocolCatalogError {}

impl From<std::io::Error> for ProtocolCatalogError {
    fn from(err: std::io::Error) -> Self {
        ProtocolCatalogError::Io(err)
    }
}

impl From<serde_json::Error> for ProtocolCatalogError {
    fn from(err: serde_json::Error) -> Self {
        ProtocolCatalogError::Json(err)
    }
}

pub fn protocol_catalog_cid_from_repo(
    repo_root: impl AsRef<Path>,
) -> Result<String, ProtocolCatalogError> {
    let bytes = fs::read(repo_root.as_ref().join(LSP_PROTOCOL_CATALOG_REPO_PATH))?;
    protocol_catalog_cid_from_bytes(&bytes)
}

pub fn embedded_protocol_catalog_cid() -> Result<String, ProtocolCatalogError> {
    protocol_catalog_cid_from_bytes(EMBEDDED_LSP_PROTOCOL_CATALOG_BYTES)
}

pub fn protocol_catalog_cid_from_bytes(bytes: &[u8]) -> Result<String, ProtocolCatalogError> {
    let json: Json = serde_json::from_slice(bytes)?;
    let canonical = json_to_cvalue(&json)?;
    let jcs = encode_jcs(&canonical);
    Ok(blake3_512_of(jcs.as_bytes()))
}

fn json_to_cvalue(value: &Json) -> Result<Arc<CValue>, ProtocolCatalogError> {
    Ok(match value {
        Json::Null => CValue::null(),
        Json::Bool(value) => CValue::boolean(*value),
        Json::Number(value) => {
            let integer = value
                .as_i64()
                .ok_or_else(|| ProtocolCatalogError::NonIntegerNumber(value.to_string()))?;
            CValue::integer(integer)
        }
        Json::String(value) => CValue::string(value.clone()),
        Json::Array(items) => {
            let mut out = Vec::with_capacity(items.len());
            for item in items {
                out.push(json_to_cvalue(item)?);
            }
            CValue::array(out)
        }
        Json::Object(map) => {
            let mut out = Vec::with_capacity(map.len());
            for (key, item) in map {
                out.push((key.clone(), json_to_cvalue(item)?));
            }
            CValue::object(out)
        }
    })
}
