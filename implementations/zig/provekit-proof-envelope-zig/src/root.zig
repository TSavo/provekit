// SPDX-License-Identifier: Apache-2.0
//
// provekit-proof-envelope-zig — Side B (language-native) crypto
// substrate for the zig kit.
//
// Surface (idiomatic Zig):
//   - BLAKE3-512 hashing                — std.crypto.hash.Blake3 (stdlib)
//   - JCS canonicalization (RFC 8785)   — re-exported from provekit-ir
//   - Ed25519 sign/verify               — std.crypto.sign.Ed25519 (stdlib)
//   - Deterministic CBOR (RFC 8949 §4.2.1) — local encoder, ~90 LOC
//   - .proof envelope build/verify      — local, byte-identical to rust
//
// All four primitives ship from the language stdlib or this package;
// nothing vendored. JCS is hand-rolled in provekit-ir using Zig's
// jsonStringify hook on each IR variant; that gives strict alphabetical
// key order and minified output, RFC 8785 conformant for the IR shapes.
// We re-export it here so the proof envelope substrate has the full
// crypto+canonicalization surface in one import.
//
// Cross-kit byte-equivalence with the rust reference is pinned in
// proof.zig's tests using the canonical two-member fixture from
// implementations/python/provekit-lift-py-tests/tests/test_proof_envelope.py.

const std = @import("std");

// CBOR primitives (deterministic encoder per RFC 8949 §4.2.1).
pub const cbor = @import("cbor.zig");

// Ed25519 helpers (raw + self-identifying string form).
pub const sign = @import("sign.zig");

// Proof envelope builder + verifier.
pub const proof = @import("proof.zig");

// Re-export the high-level proof envelope API at the package root for
// idiomatic call sites.
pub const ProofEnvelopeInput = proof.ProofEnvelopeInput;
pub const ProofEnvelopeOutput = proof.ProofEnvelopeOutput;
pub const Member = proof.Member;
pub const MetadataEntry = proof.MetadataEntry;
pub const buildProofEnvelope = proof.buildProofEnvelope;
pub const verifyRebuilt = proof.verifyRebuilt;

// Re-export the BLAKE3-512 helper signature from sign-adjacent space.
pub const Ed25519Seed = sign.Ed25519Seed;
pub const Ed25519Signature = sign.Ed25519Signature;
pub const Ed25519PublicKey = sign.Ed25519PublicKey;
pub const FOUNDATION_V0_SEED = sign.FOUNDATION_V0_SEED;

// Re-export JCS helpers from provekit-ir for callers that want the full
// substrate (BLAKE3 + JCS + Ed25519 + CBOR + envelope) from one import.
const provekit_ir = @import("provekit-ir");
pub const jcsStringify = provekit_ir.jcsStringify;
pub const jcsHash = provekit_ir.jcsHash;

// ---------------------------------------------------------------------------
// Top-level BLAKE3-512 helper.
//
// Returns the full self-identifying string form `"blake3-512:<128 hex>"`.
// Caller owns the returned slice. Mirrors provekit-canonicalizer's
// `blake3_512_of` and the existing `jcsHash` in provekit-ir.
// ---------------------------------------------------------------------------

pub fn blake3_512_of(alloc: std.mem.Allocator, bytes: []const u8) ![]u8 {
    var hash_out: [64]u8 = undefined;
    var hasher = std.crypto.hash.Blake3.init(.{});
    hasher.update(bytes);
    hasher.final(&hash_out);

    const prefix = "blake3-512:";
    const hex = std.fmt.bytesToHex(hash_out, .lower);
    var result = try alloc.alloc(u8, prefix.len + hex.len);
    @memcpy(result[0..prefix.len], prefix);
    @memcpy(result[prefix.len..], &hex);
    return result;
}

// ---------------------------------------------------------------------------
// Tests — pull in sibling test binaries.
// ---------------------------------------------------------------------------

test {
    _ = @import("cbor.zig");
    _ = @import("sign.zig");
    _ = @import("proof.zig");
}

test "blake3_512_of empty input matches BLAKE3 spec" {
    const alloc = std.testing.allocator;
    const out = try blake3_512_of(alloc, "");
    defer alloc.free(out);
    // BLAKE3-512 of empty input (extended XOF output, first 64 bytes) is
    // a known constant. The first 32 bytes match the standard BLAKE3
    // 256-bit hash of "":
    //   af1349b9f5f9a1a6a0404dea36dcc9499bcb25c9adc112b7cc9a93cae41f3262
    try std.testing.expect(std.mem.startsWith(u8, out, "blake3-512:"));
    try std.testing.expect(std.mem.startsWith(
        u8,
        out["blake3-512:".len..],
        "af1349b9f5f9a1a6a0404dea36dcc9499bcb25c9adc112b7cc9a93cae41f3262",
    ));
}
