// SPDX-License-Identifier: Apache-2.0
//
// provekit-shim-blake3-rust: blake3 library's @sugar shim.
//
// Each public function ATTESTS that the blake3 crate realizes a
// concept. Sugar = "this code IS a materialization of the concept via
// this library." The shim is the link between concept identity
// (concept:blake3-*) and the blake3 library that realizes it in Rust.
//
// Sister shims in other languages (provekit-shim-blake3-typescript
// wrapping @noble/hashes, provekit-shim-blake3-python wrapping
// blake3-py) carry the SAME concepts via their own libraries. The
// substrate verifies they're equivalent at the boundary level
// (boundary:blake3-512).

pub use blake3::Hasher;

/// `concept:blake3-512-of` — bytes to 64-byte BLAKE3-XOF digest.
/// blake3's sugar. The canonical content-addressing primitive.
#[provekit::sugar(
    concept = "concept:blake3-512-of",
    library = "blake3",
    version = "1",
    family = "concept:family:hash",
    loss = [],
)]
pub fn blake3_512_of(bytes: &[u8]) -> [u8; 64] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(bytes);
    let mut out = [0u8; 64];
    hasher.finalize_xof().fill(&mut out);
    out
}

/// `concept:blake3-hasher-new` — construct an empty BLAKE3 hasher for
/// incremental hashing. blake3's sugar.
#[provekit::sugar(
    concept = "concept:blake3-hasher-new",
    library = "blake3",
    version = "1",
    family = "concept:family:hash",
    loss = [],
)]
pub fn blake3_hasher_new() -> Hasher {
    Hasher::new()
}

/// `concept:blake3-hasher-update` — feed bytes into an incremental
/// BLAKE3 hasher. blake3's sugar.
#[provekit::sugar(
    concept = "concept:blake3-hasher-update",
    library = "blake3",
    version = "1",
    family = "concept:family:hash",
    loss = [],
)]
pub fn blake3_hasher_update(hasher: &mut Hasher, bytes: &[u8]) {
    hasher.update(bytes);
}

/// `concept:blake3-hasher-finalize-xof-64` — extract the canonical
/// 64-byte extended output from a BLAKE3 hasher. blake3's sugar.
#[provekit::sugar(
    concept = "concept:blake3-hasher-finalize-xof-64",
    library = "blake3",
    version = "1",
    family = "concept:family:hash",
    loss = [],
)]
pub fn blake3_hasher_finalize(hasher: &Hasher) -> [u8; 64] {
    let mut out = [0u8; 64];
    hasher.finalize_xof().fill(&mut out);
    out
}

// ---------------------------------------------------------------------------
// Refusals: surfaces this shim declines.
// ---------------------------------------------------------------------------

#[provekit::refuse(
    surface = "blake3::keyed_hash",
    concept = "concept:blake3-keyed-hash",
    reason = "Keyed BLAKE3 is a v0.2 surface; not yet bound to a concept-hub entry. Substrate CIDs use the unkeyed form throughout.",
    would_close_with_cluster = "keyed-hash-with-context",
)]
mod _refuse_keyed_hash {}

#[provekit::refuse(
    surface = "blake3::derive_key",
    concept = "concept:blake3-key-derivation",
    reason = "BLAKE3's derive_key (KDF mode) is not used by the substrate's content-addressing flow; would close once a concept:context-derived-key entry exists in the catalog.",
    would_close_with_cluster = "context-derived-key",
)]
mod _refuse_derive_key {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_input_xof_first_bytes() {
        let out = blake3_512_of(b"");
        assert_eq!(out[0], 0xaf);
        assert_eq!(out[1], 0x13);
        assert_eq!(out[2], 0x49);
        assert_eq!(out[3], 0xb9);
    }

    #[test]
    fn streaming_matches_one_shot() {
        let one_shot = blake3_512_of(b"hello world");
        let mut h = blake3_hasher_new();
        blake3_hasher_update(&mut h, b"hello ");
        blake3_hasher_update(&mut h, b"world");
        let streamed = blake3_hasher_finalize(&h);
        assert_eq!(one_shot, streamed);
    }

    #[test]
    fn deterministic() {
        let a = blake3_512_of(b"deterministic");
        let b = blake3_512_of(b"deterministic");
        assert_eq!(a, b);
    }
}
