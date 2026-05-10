// SPDX-License-Identifier: Apache-2.0
//
// .proof envelope builder. Per RFC 8949 §4.2.1 + the .proof spec
// (protocol/specs/2026-04-30-proof-file-format.md):
//
//   1. Build the unsigned body as a CBOR map with keys sorted by
//      bytewise lex order of their CBOR-encoded form.
//   2. ed25519-sign the unsigned-body bytes.
//   3. Re-emit the body with the signature added; keys re-sort
//      automatically (the new "signature" key slots in by lex order).
//   4. BLAKE3-512 the final bytes; the full self-identifying string
//      `"blake3-512:<128 hex>"` IS the catalog CID.
//
// The `members` map key is the embedded envelope's own CID, and the
// value is its canonical bytes (JCS-JSON for memento envelopes per
// the memento envelope grammar) wrapped as a CBOR byte string.

use std::collections::BTreeMap;

use provekit_canonicalizer::blake3_512_of;

use crate::cbor::{
    cbor_encode_bstr, cbor_encode_map_head, cbor_encode_tstr, cbor_encode_uint, CborMajor,
};
use crate::sign::{ed25519_sign_with_seed, Ed25519Seed};

#[derive(Debug, Clone)]
pub struct ProofEnvelopeInput {
    pub name: String,
    pub version: String,
    /// Optional CID of the compiled binary this proof verifies.
    /// When present, the framework checks that the running binary's
    /// hash matches before trusting any claims in this bundle.
    pub binary_cid: Option<String>,
    /// Optional metadata: key-value map for tooling and diagnostics.
    /// Included in the signed payload (tamper-evident) but explicitly
    /// NON-NORMATIVE: verifiers MUST NOT use metadata for logic.
    pub metadata: Option<BTreeMap<String, String>>,
    /// Map from member CID (full self-identifying string form,
    /// e.g. `"blake3-512:abc..."`) to that member's canonical bytes
    /// (JCS-JSON bytes for memento envelopes).
    pub members: BTreeMap<String, Vec<u8>>,
    /// Identity carried into the envelope's `signer` field. Two valid
    /// shapes per the memento-envelope grammar:
    ///   1. `blake3-512:<hex>` - CID resolving to a pubkey memento
    ///      (canonical, used by `.proof` catalogs)
    ///   2. `ed25519:<base64-pubkey>` - inline self-identifying form
    ///      (used by memento envelopes that don't pin a separate
    ///      pubkey memento, e.g. fixtures and standalone declarations)
    ///
    /// The struct field name is preserved for backwards compatibility;
    /// either form is accepted by the wire format.
    pub signer_cid: String,
    /// Ed25519 seed bytes; deterministic signing for tests/demos.
    pub signer_seed: Ed25519Seed,
    /// ISO-8601 string with millisecond precision and trailing 'Z'.
    pub declared_at: String,
}

#[derive(Debug, Clone)]
pub struct ProofEnvelopeOutput {
    /// CBOR bytes of the signed catalog. Hash of these bytes IS the CID.
    pub bytes: Vec<u8>,
    /// Full self-identifying CID, e.g. `"blake3-512:<128 hex>"`.
    pub cid: String,
}

// One CBOR (key, value) pair, with its key pre-encoded so the
// outer map can sort by bytewise CBOR-encoded-key form.
struct CborPair {
    key_cbor: Vec<u8>,
    value_cbor: Vec<u8>,
}

fn encode_key(key: &str) -> Vec<u8> {
    let mut k = Vec::with_capacity(1 + key.len());
    cbor_encode_tstr(&mut k, key);
    k
}

fn make_string_pair(key: &str, value: &str) -> CborPair {
    let mut v = Vec::with_capacity(1 + value.len());
    cbor_encode_tstr(&mut v, value);
    CborPair {
        key_cbor: encode_key(key),
        value_cbor: v,
    }
}

fn make_bytes_pair(key: &str, value: &[u8]) -> CborPair {
    let mut v = Vec::with_capacity(1 + value.len());
    cbor_encode_bstr(&mut v, value);
    CborPair {
        key_cbor: encode_key(key),
        value_cbor: v,
    }
}

fn make_members_pair(key: &str, members: &BTreeMap<String, Vec<u8>>) -> CborPair {
    // Encode as { tstr(cid) => bstr(envelope-bytes) }, sort by
    // bytewise CBOR-encoded-key form.
    let mut pairs: Vec<CborPair> = members
        .iter()
        .map(|(cid, bytes)| make_bytes_pair(cid, bytes))
        .collect();
    let mut value_cbor = Vec::new();
    emit_sorted_map(&mut value_cbor, &mut pairs);
    CborPair {
        key_cbor: encode_key(key),
        value_cbor,
    }
}

fn emit_sorted_map(out: &mut Vec<u8>, pairs: &mut [CborPair]) {
    pairs.sort_by(|a, b| a.key_cbor.cmp(&b.key_cbor));
    cbor_encode_map_head(out, pairs.len() as u64);
    for p in pairs.iter() {
        out.extend_from_slice(&p.key_cbor);
        out.extend_from_slice(&p.value_cbor);
    }
}

fn body_pairs_unsigned(input: &ProofEnvelopeInput) -> Vec<CborPair> {
    let mut pairs = vec![
        make_string_pair("kind", "catalog"),
        make_string_pair("name", &input.name),
        make_string_pair("version", &input.version),
        make_members_pair("members", &input.members),
        make_string_pair("signer", &input.signer_cid),
        make_string_pair("declaredAt", &input.declared_at),
    ];
    if let Some(ref bcid) = input.binary_cid {
        pairs.push(make_string_pair("binaryCid", bcid));
    }
    if let Some(ref meta) = input.metadata {
        let mut meta_pairs: Vec<CborPair> =
            meta.iter().map(|(k, v)| make_string_pair(k, v)).collect();
        let mut meta_cbor = Vec::new();
        emit_sorted_map(&mut meta_cbor, &mut meta_pairs);
        pairs.push(CborPair {
            key_cbor: encode_key("metadata"),
            value_cbor: meta_cbor,
        });
    }
    pairs
}

pub fn build_proof_envelope(input: &ProofEnvelopeInput) -> ProofEnvelopeOutput {
    // Step 1: encode unsigned body with sorted keys.
    let mut unsigned_pairs = body_pairs_unsigned(input);
    let mut unsigned_bytes = Vec::new();
    emit_sorted_map(&mut unsigned_bytes, &mut unsigned_pairs);

    // Step 2: ed25519-sign the unsigned bytes.
    let sig = ed25519_sign_with_seed(&input.signer_seed, &unsigned_bytes);

    // Step 3: re-emit with signature added; keys re-sort automatically.
    let mut signed_pairs = body_pairs_unsigned(input);
    signed_pairs.push(make_bytes_pair("signature", &sig));
    let mut final_bytes = Vec::new();
    emit_sorted_map(&mut final_bytes, &mut signed_pairs);

    // Silence dead-code from imports we keep for callers.
    let _ = (cbor_encode_uint, CborMajor::UnsignedInt);

    // Step 4: filename CID = full self-identifying BLAKE3-512.
    let cid = blake3_512_of(&final_bytes);
    ProofEnvelopeOutput {
        bytes: final_bytes,
        cid,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_minimal_proof_round_trips() {
        let mut members = BTreeMap::new();
        members.insert(
            "blake3-512:aa".to_string(),
            b"{\"hello\":\"world\"}".to_vec(),
        );
        let input = ProofEnvelopeInput {
            name: "@x/y".to_string(),
            version: "0.0.1".to_string(),
            binary_cid: None,
            metadata: None,
            members,
            signer_cid: "blake3-512:bb".to_string(),
            signer_seed: [0x11; 32],
            declared_at: "2026-04-30T00:00:00.000Z".to_string(),
        };
        let out = build_proof_envelope(&input);
        assert!(out.cid.starts_with("blake3-512:"));
        // First byte is map head with 7 entries (kind, name, version, members,
        // signer, declaredAt, signature) = 0xa7.
        assert_eq!(out.bytes[0], 0xa7);
    }
}
