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

use anyhow::{anyhow, bail, Context, Result};
use provekit_canonicalizer::{blake3_512_of, encode_jcs, Value};
use provekit_proof_envelope::ed25519_verify_string;
use serde_json::Value as Json;

/// The protocol catalog CID this CLI declares conformance to. Kept in
/// sync with `protocol/specs/2026-04-30-protocol-versioning.md`. If
/// the catalog changes, bump this string AND ship a new CLI.
/// Currently: v1.6.3 (patch bump over v1.6.2: formalizes the
/// identify-only package inspection lift result without changing the
/// core substrate or all-layer lift semantics).
pub const EXPECTED_CATALOG_CID: &str =
    "blake3-512:dd0cc79889ee67d2594f5cfa20a191bafed15196fb2c5036f85deced7cd976055ae93825edebc10812b6fcf3c6ccf274fbc1137f32705aa0dc5938dc5825e31d";

/// Catalog JSON bytes embedded at compile time. The CLI never reads
/// the on-disk spec file at runtime; `verify-protocol` recomputes from
/// the embedded copy so the answer is about what the binary IS, not
/// where it was invoked from.
pub const EMBEDDED_CATALOG_BYTES: &[u8] = include_bytes!("../assets/protocol-catalog.json");

/// Foundation public key bytes (`ed25519:<base64>` form) embedded at
/// compile time so `verify-protocol --signed` works for an installed
/// binary anywhere on disk. Mirrors the committed
/// `.provekit/keys/foundation-v0.pub`.
pub const EMBEDDED_FOUNDATION_PUBKEY: &[u8] = include_bytes!("../assets/foundation-v0.pub");

/// Signed attestation bytes (the JSON object) embedded at compile
/// time. Mirrors the committed
/// `.provekit/catalog-signatures/v1.6.3.json` (current). The v1.6.2,
/// v1.6.1, v1.6.0, v1.5.0, v1.4.1, v1.4.0, v1.3.1, v1.3.0, v1.2.0, and v1.1.0 attestations remain on-disk and as
/// embedded asset siblings for callers pinning to those versions; pass
/// `--signature-file` + `--catalog` to verify against them explicitly.
pub const EMBEDDED_CATALOG_SIGNATURE: &[u8] =
    include_bytes!("../assets/catalog-signature-v1.6.3.json");

/// Recompute the embedded catalog's CID using the same routine
/// `tools/recompute-spec-cids` uses: parse JSON, JCS-encode, BLAKE3-512.
pub fn compute_embedded_catalog_cid() -> Result<String> {
    let json: Json = serde_json::from_slice(EMBEDDED_CATALOG_BYTES)
        .context("parse embedded protocol catalog JSON")?;
    let canonical = json_to_value(&json)?;
    let jcs = encode_jcs(&canonical);
    Ok(blake3_512_of(jcs.as_bytes()))
}

/// Verification result for a signed catalog attestation. Each field
/// describes one of the three checks `--signed` performs.
#[derive(Debug, Clone)]
pub struct SignedCatalogVerdict {
    /// The catalog CID embedded in the binary.
    pub expected_cid: String,
    /// The catalog CID claimed by the signed attestation.
    pub claimed_cid: String,
    /// The public key string read from the .pub source.
    pub signer_pubkey: String,
    /// True iff the attestation's `signer` matches `signer_pubkey`.
    pub signer_matches: bool,
    /// True iff the Ed25519 signature verifies against the rebuilt
    /// JCS-encoded six-field message.
    pub signature_ok: bool,
    /// True iff the attestation's claimed CID matches the embedded
    /// expected CID.
    pub cid_matches: bool,
}

impl SignedCatalogVerdict {
    pub fn ok(&self) -> bool {
        self.signer_matches && self.signature_ok && self.cid_matches
    }
}

/// Parse a `foundation-v0.pub` style file: the first non-empty,
/// non-comment line is expected to be `ed25519:<base64>`.
pub fn parse_pubkey_bytes(bytes: &[u8]) -> Result<String> {
    let text = std::str::from_utf8(bytes).context("pubkey file is not UTF-8")?;
    for line in text.lines() {
        let s = line.trim();
        if s.is_empty() || s.starts_with('#') {
            continue;
        }
        if !s.starts_with("ed25519:") {
            bail!("pubkey line missing `ed25519:` prefix: {:?}", s);
        }
        return Ok(s.to_string());
    }
    bail!("no pubkey line found in pubkey file")
}

/// Verify a signed catalog attestation. The attestation message is
/// rebuilt from the file's six non-signature fields, JCS-encoded, and
/// verified via Ed25519 against `pubkey_string`.
pub fn verify_signed_attestation(
    signature_file_bytes: &[u8],
    pubkey_string: &str,
    expected_cid: &str,
) -> Result<SignedCatalogVerdict> {
    let attestation: Json =
        serde_json::from_slice(signature_file_bytes).context("parse signed attestation JSON")?;
    let obj = attestation
        .as_object()
        .ok_or_else(|| anyhow!("signed attestation must be a JSON object"))?;

    let get_str = |k: &str| -> Result<String> {
        obj.get(k)
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .ok_or_else(|| anyhow!("signed attestation missing string field `{k}`"))
    };

    let schema_version = get_str("schemaVersion")?;
    let protocol_name = get_str("protocolName")?;
    let protocol_version = get_str("protocolVersion")?;
    let catalog_cid = get_str("catalogCid")?;
    let declared_at = get_str("declaredAt")?;
    let signer = get_str("signer")?;
    let signature = get_str("signature")?;

    // Rebuild the six-field message in canonical order. Field order
    // does not affect JCS output (keys are re-sorted by code point),
    // but we keep the original spec order for readability.
    let entries: Vec<(String, Arc<Value>)> = vec![
        ("schemaVersion".to_string(), Value::string(schema_version)),
        ("protocolName".to_string(), Value::string(protocol_name)),
        (
            "protocolVersion".to_string(),
            Value::string(protocol_version),
        ),
        ("catalogCid".to_string(), Value::string(catalog_cid.clone())),
        ("declaredAt".to_string(), Value::string(declared_at)),
        ("signer".to_string(), Value::string(signer.clone())),
    ];
    let msg_obj = Value::object(entries);
    let jcs = encode_jcs(&msg_obj);

    let signature_ok = ed25519_verify_string(pubkey_string, &signature, jcs.as_bytes());
    let signer_matches = signer == pubkey_string;
    let cid_matches = catalog_cid == expected_cid;

    Ok(SignedCatalogVerdict {
        expected_cid: expected_cid.to_string(),
        claimed_cid: catalog_cid,
        signer_pubkey: pubkey_string.to_string(),
        signer_matches,
        signature_ok,
        cid_matches,
    })
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
        let kind = v.get("kind").and_then(|x| x.as_str()).expect("kind field");
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

    #[test]
    fn embedded_pubkey_parses() {
        let pk = parse_pubkey_bytes(EMBEDDED_FOUNDATION_PUBKEY).expect("parse");
        assert!(pk.starts_with("ed25519:"));
    }

    #[test]
    fn embedded_signature_verifies_against_embedded_pubkey() {
        let pk = parse_pubkey_bytes(EMBEDDED_FOUNDATION_PUBKEY).expect("parse");
        let verdict =
            verify_signed_attestation(EMBEDDED_CATALOG_SIGNATURE, &pk, EXPECTED_CATALOG_CID)
                .expect("verify");
        assert!(verdict.signer_matches, "signer must match pubkey");
        assert!(verdict.cid_matches, "claimed CID must match expected");
        assert!(verdict.signature_ok, "Ed25519 signature must verify");
        assert!(verdict.ok());
    }

    #[test]
    fn tampered_signature_fails_verification() {
        // Decode the embedded JSON, mutate the `signature` value (flip
        // a base64 character that's still valid base64 but produces
        // different bytes), re-serialize, and re-verify. The result
        // must fail verification.
        let mut v: Json = serde_json::from_slice(EMBEDDED_CATALOG_SIGNATURE).expect("parse");
        let obj = v.as_object_mut().unwrap();
        let sig = obj.get("signature").unwrap().as_str().unwrap().to_string();
        // Replace the first base64 byte after the "ed25519:" prefix
        // with one that's both valid base64 and produces a different
        // 64-byte signature.
        let prefix = "ed25519:";
        let body = &sig[prefix.len()..];
        let first = body.chars().next().unwrap();
        let replacement = if first == 'A' { 'B' } else { 'A' };
        let mut tampered = String::from(prefix);
        tampered.push(replacement);
        tampered.push_str(&body[1..]);
        obj.insert("signature".into(), Json::String(tampered));
        let bytes = serde_json::to_vec(&v).unwrap();

        let pk = parse_pubkey_bytes(EMBEDDED_FOUNDATION_PUBKEY).expect("parse");
        let verdict = verify_signed_attestation(&bytes, &pk, EXPECTED_CATALOG_CID).expect("verify");
        assert!(verdict.cid_matches, "CID untouched");
        assert!(verdict.signer_matches, "signer untouched");
        assert!(!verdict.signature_ok, "tampered signature must fail");
        assert!(!verdict.ok());
    }

    #[test]
    fn tampered_cid_fails_verification() {
        let mut v: Json = serde_json::from_slice(EMBEDDED_CATALOG_SIGNATURE).expect("parse");
        let obj = v.as_object_mut().unwrap();
        obj.insert(
            "catalogCid".into(),
            Json::String(
                "blake3-512:0000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000".into(),
            ),
        );
        let bytes = serde_json::to_vec(&v).unwrap();

        let pk = parse_pubkey_bytes(EMBEDDED_FOUNDATION_PUBKEY).expect("parse");
        let verdict = verify_signed_attestation(&bytes, &pk, EXPECTED_CATALOG_CID).expect("verify");
        // Both CID-mismatch AND signature-fail surface; verifier must
        // refuse on either.
        assert!(!verdict.cid_matches);
        assert!(!verdict.signature_ok);
        assert!(!verdict.ok());
    }
}
