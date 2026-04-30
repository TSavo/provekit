// SPDX-License-Identifier: Apache-2.0
//
// provekit-proof-envelope
//
// Two responsibilities:
//
// 1. A small deterministic-CBOR encoder enforcing RFC 8949 §4.2.1
//    "Core Deterministic Encoding": shortest-form integer encoding,
//    definite-length items, and map keys sorted by bytewise CBOR
//    encoded form.
//
// 2. The .proof file builder: bundles a `(name, version, members,
//    signer, declaredAt, signature)` catalog into deterministic-CBOR
//    bytes whose BLAKE3-512 hash IS the filename CID.
//
// 3. An Ed25519 signing helper that returns the spec's
//    self-identifying `"ed25519:" + base64(sig)` string form, plus a
//    raw-byte form used for the .proof envelope's `signature` field
//    (which is a raw bstr per the spec).

pub mod cbor;
pub mod sign;
pub mod proof;

pub use cbor::{
    cbor_encode_array_head, cbor_encode_bstr, cbor_encode_map_head, cbor_encode_tstr,
    cbor_encode_uint, CborMajor,
};
pub use proof::{build_proof_envelope, ProofEnvelopeInput, ProofEnvelopeOutput};
pub use sign::{
    ed25519_pubkey_string, ed25519_sign_string, ed25519_sign_with_seed, Ed25519PublicKey,
    Ed25519Seed, Ed25519Signature, ED25519_SIG_PREFIX,
};

#[derive(Debug, thiserror::Error)]
pub enum ProofEnvelopeError {
    #[error("proof-envelope: {0}")]
    Other(String),
}
