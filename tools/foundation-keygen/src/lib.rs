// SPDX-License-Identifier: Apache-2.0
//
// foundation-keygen / sign-catalog shared library.
//
// Two responsibilities, exposed as one library used by two binaries:
//
//   * Compute the v0 "test foundation key": Ed25519 derived from the
//     publicly-known seed `[0x42; 32]`. Documented as a test seed; v1
//     of the foundation key will be HSM-generated.
//
//   * Compute the protocol catalog's CID and produce a signed
//     attestation JSON whose canonical (JCS) bytes are signed by the
//     foundation key.
//
// The attestation file format (committed to the repo) is:
//
//   {
//     "schemaVersion": "1",
//     "protocolName": "provekit-protocol",
//     "protocolVersion": "v1.1.0",
//     "catalogCid": "blake3-512:<hex>",
//     "declaredAt": "<iso8601>",
//     "signer": "ed25519:<base64-pubkey>",
//     "signature": "ed25519:<base64-signature>"
//   }
//
// The signature is computed over the JCS-canonical bytes of the same
// object minus the `signature` field. Verifiers reconstruct the
// six-field object, JCS-encode, then verify.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use provekit_canonicalizer::{blake3_512_of, encode_jcs, Value};
use provekit_proof_envelope::{ed25519_pubkey_string, ed25519_sign_string, Ed25519Seed};
use serde_json::{json, Value as JsonValue};

/// The v0 foundation seed. PUBLICLY KNOWN. Documented as a deterministic
/// test seed; v1 is HSM-generated. A signed catalog under this seed is
/// structurally valid but offers no trust beyond "the bytes match the
/// public seed in the repo." See `protocol/specs/2026-04-30-protocol-versioning.md`.
pub const FOUNDATION_V0_SEED: Ed25519Seed = [0x42u8; 32];

/// Pinned `declaredAt` for v1.1.0. Matches the catalog's own
/// `declaredAt` (`2026-04-30T15:00:00Z`) so re-running the keygen +
/// sign-catalog binaries always produces byte-identical artifacts.
/// Determinism is a v0 design property; v1 may use signing-time clocks.
pub const V1_1_0_DECLARED_AT: &str = "2026-04-30T15:00:00Z";

/// Pinned `declaredAt` for v1.2.0. Same value as v1.1.0 because
/// v1.2.0 is additive over v1.1.0 (no breaking changes); the
/// catalog's declaredAt field carries forward.
pub const V1_2_0_DECLARED_AT: &str = "2026-04-30T15:00:00Z";

/// Pinned `declaredAt` for v1.3.0. Bumped to 2026-05-02; v1.3.0 is
/// additive over v1.2.0 (no breaking changes); attestation is signed
/// against the new catalog CID under the same foundation key.
pub const V1_3_0_DECLARED_AT: &str = "2026-05-02T15:00:00Z";

/// Catalog file path, resolved relative to this crate's manifest dir.
pub fn catalog_path() -> PathBuf {
    repo_root().join("protocol/specs/2026-04-30-protocol-catalog.json")
}

/// `.provekit/keys/foundation-v0.pub` (committed).
pub fn pubkey_path() -> PathBuf {
    repo_root().join(".provekit/keys/foundation-v0.pub")
}

/// `.provekit/keys/foundation-v0.priv` (gitignored).
pub fn privkey_path() -> PathBuf {
    repo_root().join(".provekit/keys/foundation-v0.priv")
}

/// `.provekit/catalog-signatures/v1.1.0.json` (committed).
pub fn signature_path() -> PathBuf {
    repo_root().join(".provekit/catalog-signatures/v1.1.0.json")
}

/// `.provekit/catalog-signatures/<protocol_version>.json` (committed).
/// Generalization over `signature_path()`; e.g. `v1.2.0` -> `.provekit/catalog-signatures/v1.2.0.json`.
pub fn signature_path_for(protocol_version: &str) -> PathBuf {
    repo_root()
        .join(".provekit/catalog-signatures")
        .join(format!("{protocol_version}.json"))
}

fn repo_root() -> PathBuf {
    // <repo>/tools/foundation-keygen/Cargo.toml -> <repo>
    let manifest = env!("CARGO_MANIFEST_DIR");
    Path::new(manifest)
        .parent()
        .and_then(|p| p.parent())
        .expect("manifest dir has two ancestors")
        .to_path_buf()
}

/// Compute the catalog CID via the same JCS-then-BLAKE3-512 path used
/// by `tools/recompute-spec-cids/` and the CLI's `verify-protocol`.
pub fn compute_catalog_cid_from_path(path: &Path) -> Result<String, String> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| format!("read {}: {}", path.display(), e))?;
    let json: JsonValue = serde_json::from_str(&text)
        .map_err(|e| format!("parse catalog json: {}", e))?;
    let canon = json_to_value(&json)?;
    let jcs = encode_jcs(&canon);
    Ok(blake3_512_of(jcs.as_bytes()))
}

fn json_to_value(j: &JsonValue) -> Result<Arc<Value>, String> {
    Ok(match j {
        JsonValue::Null => Value::null(),
        JsonValue::Bool(b) => Value::boolean(*b),
        JsonValue::Number(n) => {
            let i = n
                .as_i64()
                .ok_or_else(|| format!("non-i64 number: {}", n))?;
            Value::integer(i)
        }
        JsonValue::String(s) => Value::string(s.clone()),
        JsonValue::Array(items) => {
            let mut out: Vec<Arc<Value>> = Vec::with_capacity(items.len());
            for it in items {
                out.push(json_to_value(it)?);
            }
            Value::array(out)
        }
        JsonValue::Object(map) => {
            let mut entries: Vec<(String, Arc<Value>)> = Vec::with_capacity(map.len());
            for (k, v) in map {
                entries.push((k.clone(), json_to_value(v)?));
            }
            Value::object(entries)
        }
    })
}

/// Build the six-field attestation message body (no `signature` field).
/// Returned as a JSON object preserving the spec's field order.
/// Hardcodes `protocolVersion: "v1.1.0"`; for other versions use
/// `build_attestation_message_for`.
pub fn build_attestation_message(
    catalog_cid: &str,
    declared_at: &str,
    signer_pubkey: &str,
) -> JsonValue {
    build_attestation_message_for("v1.1.0", catalog_cid, declared_at, signer_pubkey)
}

/// Build the attestation message body parameterized by protocol version.
pub fn build_attestation_message_for(
    protocol_version: &str,
    catalog_cid: &str,
    declared_at: &str,
    signer_pubkey: &str,
) -> JsonValue {
    json!({
        "schemaVersion": "1",
        "protocolName": "provekit-protocol",
        "protocolVersion": protocol_version,
        "catalogCid": catalog_cid,
        "declaredAt": declared_at,
        "signer": signer_pubkey,
    })
}

/// JCS-encode the attestation message body and return the bytes the
/// signer signs (and the verifier verifies against).
pub fn attestation_signing_bytes(message: &JsonValue) -> Result<Vec<u8>, String> {
    let canon = json_to_value(message)?;
    let jcs = encode_jcs(&canon);
    Ok(jcs.into_bytes())
}

/// Build the full signed attestation JSON, ready to be written to disk.
/// Hardcodes `protocolVersion: "v1.1.0"`; for other versions use
/// `build_signed_attestation_for`.
pub fn build_signed_attestation(
    seed: &Ed25519Seed,
    catalog_cid: &str,
    declared_at: &str,
) -> Result<JsonValue, String> {
    build_signed_attestation_for("v1.1.0", seed, catalog_cid, declared_at)
}

/// Build the signed attestation parameterized by protocol version.
pub fn build_signed_attestation_for(
    protocol_version: &str,
    seed: &Ed25519Seed,
    catalog_cid: &str,
    declared_at: &str,
) -> Result<JsonValue, String> {
    let signer_pubkey = ed25519_pubkey_string(seed);
    let message =
        build_attestation_message_for(protocol_version, catalog_cid, declared_at, &signer_pubkey);
    let bytes = attestation_signing_bytes(&message)?;
    let signature = ed25519_sign_string(seed, &bytes);
    Ok(json!({
        "schemaVersion": "1",
        "protocolName": "provekit-protocol",
        "protocolVersion": protocol_version,
        "catalogCid": catalog_cid,
        "declaredAt": declared_at,
        "signer": signer_pubkey,
        "signature": signature,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signing_is_deterministic_for_v0_seed() {
        let cid = "blake3-512:dead";
        let a = build_signed_attestation(&FOUNDATION_V0_SEED, cid, V1_1_0_DECLARED_AT).unwrap();
        let b = build_signed_attestation(&FOUNDATION_V0_SEED, cid, V1_1_0_DECLARED_AT).unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn signature_field_excluded_from_signed_bytes() {
        let cid = "blake3-512:dead";
        let pk = ed25519_pubkey_string(&FOUNDATION_V0_SEED);
        let msg = build_attestation_message(cid, V1_1_0_DECLARED_AT, &pk);
        let bytes = attestation_signing_bytes(&msg).unwrap();
        let s = String::from_utf8(bytes).unwrap();
        assert!(!s.contains("signature"));
        assert!(s.contains("catalogCid"));
    }
}
