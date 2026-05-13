// SPDX-License-Identifier: Apache-2.0
//
// Ed25519 signing helper. v1.1.0 of the protocol mandates self-identifying
// signatures of the form:
//
//   "ed25519:" + base64-stdpad(64-byte-signature)
//
// And self-identifying public keys of the same form. The .proof file
// envelope itself stores its catalog signature as a RAW 64-byte CBOR byte
// string (not the prefixed string form): only the per-memento
// `producerSignature` field uses the prefixed string form, because memento
// envelopes are JCS-JSON.
//
// Mirrors implementations/rust/provekit-proof-envelope/src/sign.rs 1:1.
// Wraps std.crypto.sign.Ed25519, which is RFC 8032 (Ed25519, not Ed25519ph
// or Ed25519ctx) and produces the same 64-byte signatures as ed25519-dalek.

const std = @import("std");

const Ed25519 = std.crypto.sign.Ed25519;

pub const KEY_PREFIX: []const u8 = "ed25519:";
pub const SIG_PREFIX: []const u8 = "ed25519:";

pub const Seed = [32]u8;
pub const Signature = [64]u8;
pub const PublicKeyBytes = [32]u8;

pub const SigningError = error{
    BadSignature,
    BadPublicKey,
    BadEncoding,
    SignatureMismatch,
};

/// Sign `message` with the Ed25519 private key derived from `seed`.
/// Returns the raw 64-byte signature.
pub fn signWithSeed(seed: Seed, message: []const u8) Signature {
    const kp = Ed25519.KeyPair.generateDeterministic(seed) catch unreachable;
    const sig = kp.sign(message, null) catch unreachable;
    return sig.toBytes();
}

/// Sign `message` and return the spec's self-identifying string form
/// (`"ed25519:" + base64(sig)`). Caller owns returned slice.
pub fn signString(alloc: std.mem.Allocator, seed: Seed, message: []const u8) ![]u8 {
    const sig = signWithSeed(seed, message);
    return encodePrefixed(alloc, SIG_PREFIX, &sig);
}

/// Derive the public key from a seed and return the self-identifying string
/// form (`"ed25519:" + base64(pubkey)`). Caller owns returned slice.
pub fn pubkeyString(alloc: std.mem.Allocator, seed: Seed) ![]u8 {
    const kp = try Ed25519.KeyPair.generateDeterministic(seed);
    const pk_bytes = kp.public_key.toBytes();
    return encodePrefixed(alloc, KEY_PREFIX, &pk_bytes);
}

fn encodePrefixed(alloc: std.mem.Allocator, prefix: []const u8, raw: []const u8) ![]u8 {
    const enc = std.base64.standard.Encoder;
    const b64_len = enc.calcSize(raw.len);
    var out = try alloc.alloc(u8, prefix.len + b64_len);
    @memcpy(out[0..prefix.len], prefix);
    _ = enc.encode(out[prefix.len..], raw);
    return out;
}

/// Verify `message` against `sig_string` (spec form `"ed25519:" + base64(sig)`)
/// using `pubkey_string` (spec form `"ed25519:" + base64(pubkey)`).
/// Returns `true` iff valid, `false` for any malformed input: verifiers
/// must fail closed, on a separate code path.
pub fn verifyString(pubkey_string_: []const u8, sig_string_: []const u8, message: []const u8) bool {
    if (!std.mem.startsWith(u8, pubkey_string_, KEY_PREFIX)) return false;
    if (!std.mem.startsWith(u8, sig_string_, SIG_PREFIX)) return false;
    const pk_b64 = pubkey_string_[KEY_PREFIX.len..];
    const sig_b64 = sig_string_[SIG_PREFIX.len..];

    const dec = std.base64.standard.Decoder;
    var pk_bytes: [32]u8 = undefined;
    var sig_bytes: [64]u8 = undefined;

    const pk_len = dec.calcSizeForSlice(pk_b64) catch return false;
    if (pk_len != 32) return false;
    dec.decode(&pk_bytes, pk_b64) catch return false;

    const sig_len = dec.calcSizeForSlice(sig_b64) catch return false;
    if (sig_len != 64) return false;
    dec.decode(&sig_bytes, sig_b64) catch return false;

    const pk = Ed25519.PublicKey.fromBytes(pk_bytes) catch return false;
    const sig = Ed25519.Signature.fromBytes(sig_bytes);
    sig.verify(message, pk) catch return false;
    return true;
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

test "deterministic signature for fixed seed" {
    const seed: Seed = @splat(0x42);
    const a = signWithSeed(seed, "hello");
    const b = signWithSeed(seed, "hello");
    try std.testing.expectEqualSlices(u8, &a, &b);
}

test "string form has prefix and base64" {
    const alloc = std.testing.allocator;
    const seed: Seed = @splat(0x42);
    const s = try signString(alloc, seed, "hello");
    defer alloc.free(s);
    try std.testing.expect(std.mem.startsWith(u8, s, SIG_PREFIX));
    // 64-byte signature -> 88-byte standard-padding base64.
    try std.testing.expectEqual(@as(usize, SIG_PREFIX.len + 88), s.len);
}

test "pubkey form has prefix" {
    const alloc = std.testing.allocator;
    const seed: Seed = @splat(0x42);
    const s = try pubkeyString(alloc, seed);
    defer alloc.free(s);
    try std.testing.expect(std.mem.startsWith(u8, s, KEY_PREFIX));
    try std.testing.expectEqual(@as(usize, KEY_PREFIX.len + 44), s.len);
}

test "verify round-trip" {
    const alloc = std.testing.allocator;
    const seed: Seed = @splat(0x42);
    const pk = try pubkeyString(alloc, seed);
    defer alloc.free(pk);
    const sig = try signString(alloc, seed, "hello world");
    defer alloc.free(sig);
    try std.testing.expect(verifyString(pk, sig, "hello world"));
    try std.testing.expect(!verifyString(pk, sig, "goodbye world"));
}

test "verify rejects malformed input" {
    try std.testing.expect(!verifyString("not-prefixed", "ed25519:AAAA", "x"));
    try std.testing.expect(!verifyString("ed25519:AAAA", "not-prefixed", "x"));
    try std.testing.expect(!verifyString("ed25519:!!!!", "ed25519:!!!!", "x"));
}

// Cross-kit byte-equivalence pin: the foundation v0 pubkey is the canonical
// anchor for cross-kit signatures. Every kit derives the same string from
// seed [0x42; 32]; this test pins the exact value committed in
// .provekit/self-contracts-attestations/zig.json (signer field).
test "foundation v0 pubkey matches the value pinned in attestation files" {
    const alloc = std.testing.allocator;
    const seed: Seed = @splat(0x42);
    const pk = try pubkeyString(alloc, seed);
    defer alloc.free(pk);
    try std.testing.expectEqualStrings(
        "ed25519:IVL40Zt5HSRFMkLhXy6rbLfP+ntqXtMAl5YOBpiB2xI=",
        pk,
    );
}
