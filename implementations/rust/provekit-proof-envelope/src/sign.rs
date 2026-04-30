// SPDX-License-Identifier: Apache-2.0
//
// Ed25519 signing helper. v1.1.0 of the protocol mandates
// self-identifying signatures of the form:
//
//   "ed25519:" + base64-stdpad(64-byte-signature)
//
// And self-identifying public keys of the same form. The .proof file
// envelope itself stores its catalog signature as a RAW 64-byte CBOR
// byte string (not the prefixed string form): only the per-memento
// `producerSignature` field uses the prefixed string form, because
// memento envelopes are JCS-JSON.

use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
use ed25519_dalek::{Signer, SigningKey, VerifyingKey};

pub type Ed25519Seed = [u8; 32];
pub type Ed25519Signature = [u8; 64];
pub type Ed25519PublicKey = [u8; 32];

pub const ED25519_SIG_PREFIX: &str = "ed25519:";
pub const ED25519_KEY_PREFIX: &str = "ed25519:";

/// Sign `message` with the Ed25519 private key derived from `seed`.
/// Returns the raw 64-byte signature. Mirrors the C++ helper.
pub fn ed25519_sign_with_seed(seed: &Ed25519Seed, message: &[u8]) -> Ed25519Signature {
    let key = SigningKey::from_bytes(seed);
    let sig = key.sign(message);
    sig.to_bytes()
}

/// Sign `message` and return the spec's self-identifying string form
/// (`"ed25519:" + base64(sig)`).
pub fn ed25519_sign_string(seed: &Ed25519Seed, message: &[u8]) -> String {
    let sig = ed25519_sign_with_seed(seed, message);
    let mut s = String::with_capacity(ED25519_SIG_PREFIX.len() + 88);
    s.push_str(ED25519_SIG_PREFIX);
    s.push_str(&B64.encode(sig));
    s
}

/// Derive the public key from a seed and return the self-identifying
/// string form (`"ed25519:" + base64(pubkey)`).
pub fn ed25519_pubkey_string(seed: &Ed25519Seed) -> String {
    let sk = SigningKey::from_bytes(seed);
    let vk: VerifyingKey = sk.verifying_key();
    let bytes = vk.to_bytes();
    let mut s = String::with_capacity(ED25519_KEY_PREFIX.len() + 44);
    s.push_str(ED25519_KEY_PREFIX);
    s.push_str(&B64.encode(bytes));
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic_signature_for_fixed_seed() {
        let seed: Ed25519Seed = [0x42; 32];
        let a = ed25519_sign_with_seed(&seed, b"hello");
        let b = ed25519_sign_with_seed(&seed, b"hello");
        assert_eq!(a, b);
    }

    #[test]
    fn string_form_has_prefix_and_base64() {
        let seed: Ed25519Seed = [0x42; 32];
        let s = ed25519_sign_string(&seed, b"hello");
        assert!(s.starts_with(ED25519_SIG_PREFIX));
    }

    #[test]
    fn pubkey_form_has_prefix() {
        let seed: Ed25519Seed = [0x42; 32];
        let s = ed25519_pubkey_string(&seed);
        assert!(s.starts_with(ED25519_KEY_PREFIX));
    }
}
