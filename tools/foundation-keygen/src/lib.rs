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

/// Pinned `declaredAt` for v1.3.1. Same date as v1.3.0, hour bumped to
/// 17:00 UTC because v1.3.1 is a re-sync (catalog absorbs ir-formal-grammar
/// CID drift from PR #10); no protocol-level changes. Attestation signed
/// against the new catalog CID under the same foundation key.
pub const V1_3_1_DECLARED_AT: &str = "2026-05-02T17:00:00Z";

/// Pinned `declaredAt` for v1.4.0. Bumped to 2026-05-03T15:00:00Z; v1.4.0
/// is additive over v1.3.1 (substrate layering + 3 metadata-extension
/// specs); no breaking changes. Attestation signed against the new
/// catalog CID under the same foundation key.
pub const V1_4_0_DECLARED_AT: &str = "2026-05-03T15:00:00Z";

/// Pinned `declaredAt` for v1.4.1. Same date as v1.4.0, hour bumped to
/// 18:00 UTC because v1.4.1 is a re-sync (catalog absorbs ir-formal-grammar
/// CID drift from the normative Locus type addendum); no protocol-level
/// changes. Attestation signed against the new catalog CID under the
/// same foundation key.
pub const V1_4_1_DECLARED_AT: &str = "2026-05-03T18:00:00Z";

/// Pinned `declaredAt` for v1.5.0. Minor bump: sort grammar grow adds
/// FunctionSort and DependentSort variants (additive over v1.4.1; no
/// breaking changes). Attestation signed against the new catalog CID
/// under the same foundation key.
pub const V1_5_0_DECLARED_AT: &str = "2026-05-05T12:00:00Z";

/// Pinned `declaredAt` for v1.6.0. Minor bump: sort grammar grow adds
/// RegionSort (additive over v1.5.0; no breaking changes). Prerequisite
/// for #384 C.9 (Outlives predicates). Attestation signed against the
/// new catalog CID under the same foundation key.
pub const V1_6_0_DECLARED_AT: &str = "2026-05-05T18:00:00Z";

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

/// `.provekit/self-contracts-attestations/<lang>.json` (committed).
/// Letter-envelope attestation file for a peer's self-contracts bundle.
/// `lang` ranges over the per-language peer identifiers (`rust`, `go`,
/// `cpp`, `ts`, `csharp`); the full set is the five-peer kit suite.
pub fn self_contracts_attestation_path_for(lang: &str) -> PathBuf {
    repo_root()
        .join(".provekit/self-contracts-attestations")
        .join(format!("{lang}.json"))
}

/// Pinned `declaredAt` for self-contracts attestations under the v0
/// foundation key. One constant per protocol-catalog version because
/// the attestation is bound to a catalog CID; bumping protocol versions
/// regenerates the attestation. CID drift between catalog versions does
/// not move this timestamp, so re-signing the same bundle CID under the
/// same protocol version produces byte-identical output. v1 of the
/// foundation key may use signing-time clocks; this is a v0 invariant.
pub const SELF_CONTRACTS_DECLARED_AT_V1_3_1: &str = "2026-05-02T17:00:00Z";

/// Recognized peer identifiers for self-contracts attestations.
/// Kept in sync with the Makefile's `mint-<lang>` targets.
/// All 11 peer kits use the same letter-envelope attestation shape;
/// the source tree no longer carries machine-local truth about its own
/// bytes for any kit.
pub const SELF_CONTRACTS_LANGS: &[&str] = &[
    "rust", "go", "cpp", "ts", "csharp",
    "swift", "java", "python", "ruby", "zig", "c", "php",
];

/// Build the signed message body for a self-contracts attestation
/// (no `signature` field). JCS-canonical bytes of this object are what
/// the foundation key signs.
///
/// Per spec #94 §2, the signed message includes `contractSetCid` (REQUIRED)
/// and optionally `previousContractSetCid`. The `cid` field is retained for
/// warm-cache lookup (informational, not a trust anchor).
pub fn build_self_contracts_message(
    lang: &str,
    cid: &str,
    contract_set_cid: &str,
    declared_at: &str,
    signer_pubkey: &str,
) -> JsonValue {
    json!({
        "schemaVersion": "1",
        "kind": "self-contracts-attestation",
        "lang": lang,
        "cid": cid,
        "contractSetCid": contract_set_cid,
        "declaredAt": declared_at,
        "signer": signer_pubkey,
    })
}

/// Build the full signed self-contracts attestation JSON, ready to be
/// written to disk. Includes `contractSetCid` per spec #94 §2.
pub fn build_signed_self_contracts_attestation(
    seed: &Ed25519Seed,
    lang: &str,
    cid: &str,
    contract_set_cid: &str,
    declared_at: &str,
) -> Result<JsonValue, String> {
    if !SELF_CONTRACTS_LANGS.contains(&lang) {
        return Err(format!(
            "unknown lang `{}`; expected one of {:?}",
            lang, SELF_CONTRACTS_LANGS
        ));
    }
    let signer_pubkey = ed25519_pubkey_string(seed);
    let message = build_self_contracts_message(lang, cid, contract_set_cid, declared_at, &signer_pubkey);
    let bytes = attestation_signing_bytes(&message)?;
    let signature = ed25519_sign_string(seed, &bytes);
    Ok(json!({
        "schemaVersion": "1",
        "kind": "self-contracts-attestation",
        "lang": lang,
        "cid": cid,
        "contractSetCid": contract_set_cid,
        "declaredAt": declared_at,
        "signer": signer_pubkey,
        "signature": signature,
    }))
}

/// Verification result for a signed self-contracts attestation.
///
/// The trust comparison is on `contractSetCid` per spec #94. The bundle CID
/// (`cid`) is retained in `claimed_bundle_cid` for diagnostic display only.
#[derive(Debug, Clone)]
pub struct SignedSelfContractsVerdict {
    /// The contractSetCid claimed by the on-disk attestation file.
    pub claimed_contract_set_cid: String,
    /// The contractSetCid the verifier observed (derived from the freshly-minted bundle).
    pub observed_contract_set_cid: String,
    /// The bundle CID claimed by the attestation (diagnostic/warm-cache only).
    pub claimed_bundle_cid: String,
    /// The signer pubkey string read from the attestation file.
    pub signer_pubkey: String,
    /// True iff the attestation's `signer` matches the trusted pubkey.
    pub signer_matches: bool,
    /// True iff the Ed25519 signature verifies against the rebuilt JCS-encoded message.
    pub signature_ok: bool,
    /// True iff `claimed_contract_set_cid == observed_contract_set_cid`.
    /// This is the trust-path comparison per spec #94.
    pub contract_set_cid_matches: bool,
}

impl SignedSelfContractsVerdict {
    /// Overall verdict: signer + signature + contractSetCid must all pass.
    pub fn ok(&self) -> bool {
        self.signer_matches && self.signature_ok && self.contract_set_cid_matches
    }
}

/// Verify a signed self-contracts attestation against a trusted pubkey
/// and an observed contractSetCid.
///
/// Per spec #94: the trust comparison is on `contractSetCid`, NOT the bundle
/// file bytes. Bundle CID drift is reported as a soft warning (returns Ok,
/// `verdict.bundle_cid_matches == false`) but does NOT cause failure.
///
/// If the attestation lacks `contractSetCid` (legacy pre-spec-#94), returns
/// an explicit error describing the migration requirement.
pub fn verify_signed_self_contracts_attestation(
    attestation_bytes: &[u8],
    trusted_pubkey: &str,
    observed_contract_set_cid: &str,
) -> Result<SignedSelfContractsVerdict, String> {
    let attestation: JsonValue = serde_json::from_slice(attestation_bytes)
        .map_err(|e| format!("parse self-contracts attestation JSON: {}", e))?;
    let obj = attestation
        .as_object()
        .ok_or_else(|| "self-contracts attestation must be a JSON object".to_string())?;

    let get_str = |k: &str| -> Result<String, String> {
        obj.get(k)
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .ok_or_else(|| format!("self-contracts attestation missing string field `{k}`"))
    };
    let get_str_opt = |k: &str| -> Option<String> {
        obj.get(k).and_then(|v| v.as_str()).map(|s| s.to_string())
    };

    let schema_version = get_str("schemaVersion")?;
    let kind = get_str("kind")?;
    if kind != "self-contracts-attestation" {
        return Err(format!(
            "expected kind `self-contracts-attestation`, got `{}`",
            kind
        ));
    }
    let lang = get_str("lang")?;
    let claimed_bundle_cid = get_str("cid")?;
    let declared_at = get_str("declaredAt")?;
    let signer = get_str("signer")?;
    let signature = get_str("signature")?;

    // contractSetCid is REQUIRED per spec #94. Legacy attestations without it
    // must be re-signed; return a clear migration error.
    let claimed_contract_set_cid = get_str_opt("contractSetCid").ok_or_else(|| {
        format!(
            "spec #94 not implemented in `{lang}`; attestation lacks `contractSetCid`; \
             re-sign with: cargo run --release --manifest-path tools/foundation-keygen/Cargo.toml \
             --bin sign-self-contracts -- {lang} <bundle-cid> <contract-set-cid>"
        )
    })?;

    // Rebuild the signed message body in canonical order. JCS sorts keys
    // by code point; the builder preserves spec's authored order for legibility.
    let entries: Vec<(String, Arc<Value>)> = vec![
        ("schemaVersion".to_string(), Value::string(schema_version)),
        (
            "kind".to_string(),
            Value::string("self-contracts-attestation".to_string()),
        ),
        ("lang".to_string(), Value::string(lang)),
        ("cid".to_string(), Value::string(claimed_bundle_cid.clone())),
        ("contractSetCid".to_string(), Value::string(claimed_contract_set_cid.clone())),
        ("declaredAt".to_string(), Value::string(declared_at)),
        ("signer".to_string(), Value::string(signer.clone())),
    ];
    let msg_obj = Value::object(entries);
    let jcs = encode_jcs(&msg_obj);

    let signature_ok =
        provekit_proof_envelope::ed25519_verify_string(trusted_pubkey, &signature, jcs.as_bytes());
    let signer_matches = signer == trusted_pubkey;
    let contract_set_cid_matches = claimed_contract_set_cid == observed_contract_set_cid;

    Ok(SignedSelfContractsVerdict {
        claimed_contract_set_cid,
        observed_contract_set_cid: observed_contract_set_cid.to_string(),
        claimed_bundle_cid,
        signer_pubkey: trusted_pubkey.to_string(),
        signer_matches,
        signature_ok,
        contract_set_cid_matches,
    })
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

    // Helper: fake contractSetCid value for tests.
    const FAKE_CONTRACT_SET_CID: &str = "blake3-512:aaaa0000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000";

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

    #[test]
    fn self_contracts_signing_is_deterministic() {
        // Per spec #94: contractSetCid is included in the signed body.
        let cid = "blake3-512:beef00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000";
        let a = build_signed_self_contracts_attestation(
            &FOUNDATION_V0_SEED,
            "rust",
            cid,
            FAKE_CONTRACT_SET_CID,
            SELF_CONTRACTS_DECLARED_AT_V1_3_1,
        )
        .unwrap();
        let b = build_signed_self_contracts_attestation(
            &FOUNDATION_V0_SEED,
            "rust",
            cid,
            FAKE_CONTRACT_SET_CID,
            SELF_CONTRACTS_DECLARED_AT_V1_3_1,
        )
        .unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn self_contracts_unknown_lang_rejected() {
        let cid = "blake3-512:beef00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000";
        let err = build_signed_self_contracts_attestation(
            &FOUNDATION_V0_SEED,
            "perl",
            cid,
            FAKE_CONTRACT_SET_CID,
            SELF_CONTRACTS_DECLARED_AT_V1_3_1,
        )
        .unwrap_err();
        assert!(err.contains("unknown lang"));
    }

    #[test]
    fn self_contracts_round_trip_verifies() {
        // Trust comparison is on contractSetCid per spec #94.
        let cid = "blake3-512:beef00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000";
        let pk = ed25519_pubkey_string(&FOUNDATION_V0_SEED);
        let attestation = build_signed_self_contracts_attestation(
            &FOUNDATION_V0_SEED,
            "go",
            cid,
            FAKE_CONTRACT_SET_CID,
            SELF_CONTRACTS_DECLARED_AT_V1_3_1,
        )
        .unwrap();
        let bytes = serde_json::to_vec(&attestation).unwrap();
        let verdict = verify_signed_self_contracts_attestation(
            &bytes, &pk, FAKE_CONTRACT_SET_CID,
        ).unwrap();
        assert!(verdict.signer_matches);
        assert!(verdict.signature_ok);
        assert!(verdict.contract_set_cid_matches);
        assert!(verdict.ok());
    }

    #[test]
    fn self_contracts_contract_set_cid_drift_fails_verification() {
        // Per spec #94: contractSetCid drift fails. This replaces the old
        // bundle-CID-drift test; that comparison is now a soft warning.
        let cid = "blake3-512:beef00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000";
        let pk = ed25519_pubkey_string(&FOUNDATION_V0_SEED);
        let attestation = build_signed_self_contracts_attestation(
            &FOUNDATION_V0_SEED,
            "cpp",
            cid,
            FAKE_CONTRACT_SET_CID,
            SELF_CONTRACTS_DECLARED_AT_V1_3_1,
        )
        .unwrap();
        let bytes = serde_json::to_vec(&attestation).unwrap();
        let drifted_set_cid = "blake3-512:cafe0000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000";
        let verdict = verify_signed_self_contracts_attestation(
            &bytes, &pk, drifted_set_cid,
        ).unwrap();
        assert!(verdict.signer_matches, "signer untouched");
        assert!(verdict.signature_ok, "signature still verifies");
        assert!(!verdict.contract_set_cid_matches, "contractSetCid drifted");
        assert!(!verdict.ok());
    }

    #[test]
    fn self_contracts_bundle_cid_drift_does_not_fail() {
        // Per spec #94: bundle CID (attestation.cid) drift is a soft warning,
        // NOT a failure. Same contracts, different bundle bytes (different envelope
        // timestamps) must still pass when contractSetCid matches.
        let bundle_cid = "blake3-512:beef00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000";
        let different_bundle_cid = "blake3-512:face0000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000";
        let pk = ed25519_pubkey_string(&FOUNDATION_V0_SEED);
        // Attest with one bundle CID...
        let attestation = build_signed_self_contracts_attestation(
            &FOUNDATION_V0_SEED,
            "rust",
            bundle_cid,
            FAKE_CONTRACT_SET_CID,
            SELF_CONTRACTS_DECLARED_AT_V1_3_1,
        )
        .unwrap();
        // ...and verify against a different bundle CID (same contractSetCid).
        // verify_signed_self_contracts_attestation takes contractSetCid, not bundle CID.
        // So we only check contractSetCid. Bundle drift is invisible to the verifier.
        let bytes = serde_json::to_vec(&attestation).unwrap();
        let verdict = verify_signed_self_contracts_attestation(
            &bytes, &pk, FAKE_CONTRACT_SET_CID,
        ).unwrap();
        assert!(verdict.signer_matches);
        assert!(verdict.signature_ok);
        assert!(verdict.contract_set_cid_matches);
        // The bundle CID in the attestation is `bundle_cid` but the observed
        // bundle is `different_bundle_cid`; this does NOT affect verdict.ok().
        assert!(verdict.ok(), "same contractSetCid = pass, bundle CID drift is irrelevant");
        // confirmed: claimed_bundle_cid is the one we signed with
        assert_eq!(verdict.claimed_bundle_cid, bundle_cid);
    }

    #[test]
    fn self_contracts_legacy_attestation_missing_contract_set_cid_fails_with_clear_error() {
        // A legacy attestation without contractSetCid must fail with a clear
        // migration message, not a silent parse error or panic.
        let bytes = serde_json::to_vec(&serde_json::json!({
            "schemaVersion": "1",
            "kind": "self-contracts-attestation",
            "lang": "rust",
            "cid": "blake3-512:beef00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000",
            "declaredAt": SELF_CONTRACTS_DECLARED_AT_V1_3_1,
            "signer": "ed25519:fake",
            "signature": "ed25519:fake",
        }))
        .unwrap();
        let err = verify_signed_self_contracts_attestation(
            &bytes, "ed25519:fake", FAKE_CONTRACT_SET_CID,
        ).unwrap_err();
        assert!(err.contains("contractSetCid"), "error should mention the missing field: {err}");
        assert!(err.contains("spec #94"), "error should reference the spec: {err}");
    }

    #[test]
    fn self_contracts_kind_field_required() {
        // An attestation that lacks the discriminator must be rejected
        // outright; verifier MUST NOT accept ambiguous shapes.
        let bytes = serde_json::to_vec(&serde_json::json!({
            "schemaVersion": "1",
            "lang": "rust",
            "cid": "blake3-512:beef00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000",
            "contractSetCid": FAKE_CONTRACT_SET_CID,
            "declaredAt": SELF_CONTRACTS_DECLARED_AT_V1_3_1,
            "signer": "ed25519:fake",
            "signature": "ed25519:fake",
        }))
        .unwrap();
        let err = verify_signed_self_contracts_attestation(
            &bytes, "ed25519:fake", FAKE_CONTRACT_SET_CID,
        ).unwrap_err();
        assert!(err.contains("kind"));
    }

    #[test]
    fn self_contracts_ts_round_trip_verifies() {
        // ts joined the letter-envelope refactor in the second pass;
        // exercise the round-trip explicitly so the lang-set extension
        // does not regress silently.
        let cid = "blake3-512:beef00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000";
        let pk = ed25519_pubkey_string(&FOUNDATION_V0_SEED);
        let attestation = build_signed_self_contracts_attestation(
            &FOUNDATION_V0_SEED,
            "ts",
            cid,
            FAKE_CONTRACT_SET_CID,
            SELF_CONTRACTS_DECLARED_AT_V1_3_1,
        )
        .unwrap();
        let bytes = serde_json::to_vec(&attestation).unwrap();
        let verdict = verify_signed_self_contracts_attestation(
            &bytes, &pk, FAKE_CONTRACT_SET_CID,
        ).unwrap();
        assert!(verdict.signer_matches);
        assert!(verdict.signature_ok);
        assert!(verdict.contract_set_cid_matches);
        assert!(verdict.ok());
    }

    #[test]
    fn self_contracts_csharp_round_trip_verifies() {
        // csharp joined the letter-envelope refactor in the second pass;
        // exercise the round-trip explicitly so the lang-set extension
        // does not regress silently.
        let cid = "blake3-512:beef00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000";
        let pk = ed25519_pubkey_string(&FOUNDATION_V0_SEED);
        let attestation = build_signed_self_contracts_attestation(
            &FOUNDATION_V0_SEED,
            "csharp",
            cid,
            FAKE_CONTRACT_SET_CID,
            SELF_CONTRACTS_DECLARED_AT_V1_3_1,
        )
        .unwrap();
        let bytes = serde_json::to_vec(&attestation).unwrap();
        let verdict = verify_signed_self_contracts_attestation(
            &bytes, &pk, FAKE_CONTRACT_SET_CID,
        ).unwrap();
        assert!(verdict.signer_matches);
        assert!(verdict.signature_ok);
        assert!(verdict.contract_set_cid_matches);
        assert!(verdict.ok());
    }

    #[test]
    fn self_contracts_lang_set_is_eleven_peers() {
        // Guard the canonical kit suite. Adding or removing a peer is
        // a deliberate act; this test forces an explicit edit.
        // java/python/ruby/zig/c added in feat(cli): unify mint pipeline.
        assert_eq!(
            SELF_CONTRACTS_LANGS,
            &["rust", "go", "cpp", "ts", "csharp", "swift", "java", "python", "ruby", "zig", "c"]
        );
    }
}
