// SPDX-License-Identifier: Apache-2.0
//
// Minimal CBOR decoder for .proof catalog reading. The implementation
// moved to `provekit-proof-envelope::cbor_decode` so it can be reused
// from libprovekit (which depends on proof-envelope but not verifier).
// This module re-exports it so existing `crate::cbor_decode::*` paths
// inside provekit-verifier keep working.

pub use provekit_proof_envelope::cbor_decode::{decode, CborDecodeError, CborValue};
