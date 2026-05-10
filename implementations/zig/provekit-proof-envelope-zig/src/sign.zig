// SPDX-License-Identifier: Apache-2.0
//
// Ed25519 signing helper. Wraps `std.crypto.sign.Ed25519` so the proof
// envelope builder can request a deterministic 64-byte signature from a
// 32-byte seed. Mirrors implementations/rust/provekit-proof-envelope/src/sign.rs.
//
// Notes on stdlib mapping:
//   * `KeyPair.generateDeterministic(seed)` corresponds to ed25519-dalek's
//     `SigningKey::from_bytes(seed)`: both interpret the 32 bytes as the
//     RFC 8032 secret seed and SHA-512 expand it.
//   * Passing `null` for `noise` to `kp.sign(msg, null)` selects the pure
//     deterministic RFC 8032 mode, which matches ed25519-dalek's default.
//   * `Signature.toBytes()` returns 64 bytes laid out as `R || s`.
//
// v1.1.0 of the protocol mandates self-identifying signatures of the form
// "ed25519:" + base64-stdpad(64-byte-signature) for memento envelopes
// (JCS-JSON), but the .proof catalog itself stores the raw 64-byte
// signature as a CBOR byte string. The helpers here cover both shapes.

const std = @import("std");

const Ed25519 = std.crypto.sign.Ed25519;

pub const Ed25519Seed = [32]u8;
pub const Ed25519Signature = [64]u8;
pub const Ed25519PublicKey = [32]u8;

pub const SIG_PREFIX: []const u8 = "ed25519:";
pub const KEY_PREFIX: []const u8 = "ed25519:";

/// Foundation v0 test seed. Public, well-known. Used by every kit's
/// cross-kit byte-equivalence fixture so signers agree without sharing
/// secrets. Source: tools/foundation-keygen/src/lib.rs FOUNDATION_V0_SEED.
pub const FOUNDATION_V0_SEED: Ed25519Seed = [_]u8{0x42} ** 32;

/// Sign `message` with the Ed25519 private key derived from `seed`.
/// Returns the raw 64-byte signature. Deterministic per RFC 8032
/// (no noise, no randomization).
pub fn signWithSeed(seed: Ed25519Seed, message: []const u8) !Ed25519Signature {
    const kp = try Ed25519.KeyPair.generateDeterministic(seed);
    const sig = try kp.sign(message, null);
    return sig.toBytes();
}

/// Derive the 32-byte public key from a seed.
pub fn pubkeyFromSeed(seed: Ed25519Seed) !Ed25519PublicKey {
    const kp = try Ed25519.KeyPair.generateDeterministic(seed);
    return kp.public_key.toBytes();
}

/// Verify `message` was signed by the private key corresponding to
/// `public_key`. Returns true iff the signature is valid.
pub fn verifyRaw(public_key: Ed25519PublicKey, signature: Ed25519Signature, message: []const u8) bool {
    const pk = Ed25519.PublicKey.fromBytes(public_key) catch return false;
    const sig = Ed25519.Signature.fromBytes(signature);
    sig.verify(message, pk) catch return false;
    return true;
}

/// Sign `message` and return the spec's self-identifying string form
/// (`"ed25519:" + base64-stdpad(sig)`). Caller owns the returned slice.
pub fn signString(alloc: std.mem.Allocator, seed: Ed25519Seed, message: []const u8) ![]u8 {
    const sig = try signWithSeed(seed, message);
    const b64 = std.base64.standard.Encoder;
    const enc_len = b64.calcSize(sig.len);
    var out = try alloc.alloc(u8, SIG_PREFIX.len + enc_len);
    @memcpy(out[0..SIG_PREFIX.len], SIG_PREFIX);
    _ = b64.encode(out[SIG_PREFIX.len..], &sig);
    return out;
}

/// Derive the public key from a seed and return the self-identifying
/// string form (`"ed25519:" + base64-stdpad(pubkey)`). Caller owns the slice.
pub fn pubkeyString(alloc: std.mem.Allocator, seed: Ed25519Seed) ![]u8 {
    const pk = try pubkeyFromSeed(seed);
    const b64 = std.base64.standard.Encoder;
    const enc_len = b64.calcSize(pk.len);
    var out = try alloc.alloc(u8, KEY_PREFIX.len + enc_len);
    @memcpy(out[0..KEY_PREFIX.len], KEY_PREFIX);
    _ = b64.encode(out[KEY_PREFIX.len..], &pk);
    return out;
}

/// Verify `message` against `sig_string` (spec form
/// `"ed25519:" + base64(sig)`) using `pubkey_string`. Returns false for
/// any malformed input rather than erroring: verifiers fail closed.
pub fn verifyString(pubkey_string: []const u8, sig_string: []const u8, message: []const u8) bool {
    if (!std.mem.startsWith(u8, pubkey_string, KEY_PREFIX)) return false;
    if (!std.mem.startsWith(u8, sig_string, SIG_PREFIX)) return false;
    const pk_b64 = pubkey_string[KEY_PREFIX.len..];
    const sig_b64 = sig_string[SIG_PREFIX.len..];

    const dec = std.base64.standard.Decoder;
    var pk_buf: [32]u8 = undefined;
    var sig_buf: [64]u8 = undefined;
    const pk_dec_len = dec.calcSizeForSlice(pk_b64) catch return false;
    if (pk_dec_len != 32) return false;
    dec.decode(&pk_buf, pk_b64) catch return false;
    const sig_dec_len = dec.calcSizeForSlice(sig_b64) catch return false;
    if (sig_dec_len != 64) return false;
    dec.decode(&sig_buf, sig_b64) catch return false;

    return verifyRaw(pk_buf, sig_buf, message);
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

const testing = std.testing;

test "deterministic signature for fixed seed" {
    const a = try signWithSeed(FOUNDATION_V0_SEED, "hello");
    const b = try signWithSeed(FOUNDATION_V0_SEED, "hello");
    try testing.expectEqualSlices(u8, &a, &b);
}

test "signature is 64 bytes" {
    const sig = try signWithSeed(FOUNDATION_V0_SEED, "hello");
    try testing.expectEqual(@as(usize, 64), sig.len);
}

test "raw verify round-trip" {
    const seed: Ed25519Seed = [_]u8{0x42} ** 32;
    const sig = try signWithSeed(seed, "hello world");
    const pk = try pubkeyFromSeed(seed);
    try testing.expect(verifyRaw(pk, sig, "hello world"));
    try testing.expect(!verifyRaw(pk, sig, "goodbye world"));
}

test "signString has prefix and decodes" {
    const seed: Ed25519Seed = [_]u8{0x42} ** 32;
    const s = try signString(testing.allocator, seed, "hello");
    defer testing.allocator.free(s);
    try testing.expect(std.mem.startsWith(u8, s, SIG_PREFIX));
}

test "verify string round-trip" {
    const seed: Ed25519Seed = [_]u8{0x42} ** 32;
    const sig_s = try signString(testing.allocator, seed, "hello world");
    defer testing.allocator.free(sig_s);
    const pk_s = try pubkeyString(testing.allocator, seed);
    defer testing.allocator.free(pk_s);

    try testing.expect(verifyString(pk_s, sig_s, "hello world"));
    try testing.expect(!verifyString(pk_s, sig_s, "wrong message"));
}

test "verify string rejects malformed" {
    try testing.expect(!verifyString("not-prefixed", "ed25519:AAAA", "x"));
    try testing.expect(!verifyString("ed25519:AAAA", "not-prefixed", "x"));
    try testing.expect(!verifyString("ed25519:!!!!", "ed25519:!!!!", "x"));
}

// Pinned cross-kit Ed25519 vector. The .proof envelope cross-kit fixture
// extracts a 64-byte signature embedded inside the rust output bytes.
// Unsigned-body bytes (the message that ends up signed) for the canonical
// two-member fixture are derived in proof.zig's tests; here we pin the
// raw signature that ed25519-dalek produces for that exact byte stream.
//
// Bytes pulled from RUST_FIXTURE_BYTES_HEX_FULL in
// implementations/python/provekit-lift-py-tests/tests/test_proof_envelope.py:
// the 64 bytes following the `signature` key + bstr-head `0x5840`:
//   6a21dd428a54e22c82ca6d6125a7293c4a723786cb1840e891cefa03e63246eb
//   97ef13dab86b7b1469d67302fadc969cd88c92c29495d13c75fc0201a7263b06
//
// If this test fails the entire envelope parity collapses; the failure
// surfaces here with a clear cause rather than a nine-screen byte diff.
test "stdlib Ed25519 matches ed25519-dalek for foundation seed and canonical unsigned body" {
    // Hand-build the canonical unsigned body for the two-member fixture.
    // Same construction as proof.zig's build_proof_envelope; we compute
    // it here in isolation to discriminate Ed25519 drift from CBOR drift.
    const cbor = @import("cbor.zig");
    const alloc = testing.allocator;

    // Build the unsigned body (6-key map, no signature yet) with sorted
    // keys. Members map sub-CBOR built first.
    var members: std.ArrayList(u8) = .empty;
    defer members.deinit(alloc);
    // Two members; key form sorts naturally because aa < bb lex.
    try cbor.encodeMapHead(&members, alloc, 2);
    try cbor.encodeTstr(&members, alloc, "blake3-512:aa");
    try cbor.encodeBstr(&members, alloc, "{\"hello\":\"world\"}");
    try cbor.encodeTstr(&members, alloc, "blake3-512:bb");
    try cbor.encodeBstr(&members, alloc, "{\"goodbye\":\"world\"}");

    // The 6-key body: kind, name, version, members, signer, declaredAt.
    // Sort by bytewise lex of CBOR-encoded key. All keys are short text
    // strings, so the prefix byte is the same and lex follows raw key
    // bytes after that. Hand-sorted: kind < name < signer < members
    // < version < declaredAt? No: let's compute properly: each key is
    // tstr, head 0x6N (text + len), so the first byte differs by len.
    //   "kind"      (4) -> 0x64
    //   "name"      (4) -> 0x64
    //   "signer"    (6) -> 0x66
    //   "members"   (7) -> 0x67
    //   "version"   (7) -> 0x67
    //   "declaredAt"(10)-> 0x6a
    // After head matches, compare bytes. So lex order on encoded form:
    //   kind, name, signer, members, version, declaredAt.
    //
    // (declaredAt sorts last because its head 0x6a is greater than 0x67.)

    var unsigned: std.ArrayList(u8) = .empty;
    defer unsigned.deinit(alloc);
    try cbor.encodeMapHead(&unsigned, alloc, 6);
    try cbor.encodeTstr(&unsigned, alloc, "kind");
    try cbor.encodeTstr(&unsigned, alloc, "catalog");
    try cbor.encodeTstr(&unsigned, alloc, "name");
    try cbor.encodeTstr(&unsigned, alloc, "@test/cat");
    try cbor.encodeTstr(&unsigned, alloc, "signer");
    try cbor.encodeTstr(&unsigned, alloc, "blake3-512:cc");
    try cbor.encodeTstr(&unsigned, alloc, "members");
    try unsigned.appendSlice(alloc, members.items);
    try cbor.encodeTstr(&unsigned, alloc, "version");
    try cbor.encodeTstr(&unsigned, alloc, "1.0.0");
    try cbor.encodeTstr(&unsigned, alloc, "declaredAt");
    try cbor.encodeTstr(&unsigned, alloc, "2026-04-30T00:00:00.000Z");

    const sig = try signWithSeed(FOUNDATION_V0_SEED, unsigned.items);

    // Pinned 64-byte signature from the rust kit's reference output.
    // If this fails: zig stdlib Ed25519 disagrees with ed25519-dalek
    // for the same seed + message. Hard blocker: STOP.
    const expected_sig: [64]u8 = .{
        0x6a, 0x21, 0xdd, 0x42, 0x8a, 0x54, 0xe2, 0x2c,
        0x82, 0xca, 0x6d, 0x61, 0x25, 0xa7, 0x29, 0x3c,
        0x4a, 0x72, 0x37, 0x86, 0xcb, 0x18, 0x40, 0xe8,
        0x91, 0xce, 0xfa, 0x03, 0xe6, 0x32, 0x46, 0xeb,
        0x97, 0xef, 0x13, 0xda, 0xb8, 0x6b, 0x7b, 0x14,
        0x69, 0xd6, 0x73, 0x02, 0xfa, 0xdc, 0x96, 0x9c,
        0xd8, 0x8c, 0x92, 0xc2, 0x94, 0x95, 0xd1, 0x3c,
        0x75, 0xfc, 0x02, 0x01, 0xa7, 0x26, 0x3b, 0x06,
    };
    try testing.expectEqualSlices(u8, &expected_sig, &sig);
}
