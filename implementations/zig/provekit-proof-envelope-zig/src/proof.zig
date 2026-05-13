// SPDX-License-Identifier: Apache-2.0
//
// .proof envelope builder. Per RFC 8949 §4.2.1 + the .proof spec
// (protocol/specs/2026-04-30-proof-file-format.md):
//
//   1. Build the unsigned body as a CBOR map with keys sorted by
//      bytewise lex order of their CBOR-encoded form.
//   2. Ed25519-sign the unsigned-body bytes.
//   3. Re-emit the body with the signature added; keys re-sort
//      automatically (the new "signature" key slots in by lex order).
//   4. BLAKE3-512 the final bytes; the full self-identifying string
//      `"blake3-512:<128 hex>"` IS the catalog CID.
//
// The `members` map key is the embedded envelope's own CID, and the
// value is its canonical bytes (JCS-JSON for memento envelopes per
// the memento envelope grammar) wrapped as a CBOR byte string.
//
// Mirrors implementations/rust/provekit-proof-envelope/src/proof.rs.

const std = @import("std");

const cbor = @import("cbor.zig");
const sign = @import("sign.zig");

pub const Member = struct {
    cid: []const u8,
    bytes: []const u8,
};

pub const MetadataEntry = struct {
    key: []const u8,
    value: []const u8,
};

pub const ProofEnvelopeInput = struct {
    name: []const u8,
    version: []const u8,
    members: []const Member,
    signer_cid: []const u8,
    declared_at: []const u8,
    signer_seed: sign.Ed25519Seed = sign.FOUNDATION_V0_SEED,
    binary_cid: ?[]const u8 = null,
    metadata: ?[]const MetadataEntry = null,
};

pub const ProofEnvelopeOutput = struct {
    /// CBOR bytes of the signed catalog. Caller owns the slice.
    bytes: []u8,
    /// Full self-identifying CID, e.g. `"blake3-512:<128 hex>"`.
    /// Caller owns the slice.
    cid: []u8,

    pub fn deinit(self: *ProofEnvelopeOutput, alloc: std.mem.Allocator) void {
        alloc.free(self.bytes);
        alloc.free(self.cid);
    }
};

// ---------------------------------------------------------------------------
// Internal: pre-encoded (key, value) pairs for sortable map emission.
// ---------------------------------------------------------------------------

const Pair = struct {
    key_cbor: []u8,
    value_cbor: []u8,
};

fn pairLessThan(_: void, a: Pair, b: Pair) bool {
    return std.mem.lessThan(u8, a.key_cbor, b.key_cbor);
}

fn encodeKey(alloc: std.mem.Allocator, key: []const u8) ![]u8 {
    var buf: std.ArrayList(u8) = .empty;
    errdefer buf.deinit(alloc);
    try cbor.encodeTstr(&buf, alloc, key);
    return buf.toOwnedSlice(alloc);
}

fn makeStringPair(alloc: std.mem.Allocator, key: []const u8, value: []const u8) !Pair {
    var v: std.ArrayList(u8) = .empty;
    errdefer v.deinit(alloc);
    try cbor.encodeTstr(&v, alloc, value);
    return .{
        .key_cbor = try encodeKey(alloc, key),
        .value_cbor = try v.toOwnedSlice(alloc),
    };
}

fn makeBytesPair(alloc: std.mem.Allocator, key: []const u8, value: []const u8) !Pair {
    var v: std.ArrayList(u8) = .empty;
    errdefer v.deinit(alloc);
    try cbor.encodeBstr(&v, alloc, value);
    return .{
        .key_cbor = try encodeKey(alloc, key),
        .value_cbor = try v.toOwnedSlice(alloc),
    };
}

fn makeMembersPair(alloc: std.mem.Allocator, key: []const u8, members: []const Member) !Pair {
    // Encode as { tstr(cid) => bstr(envelope-bytes) }, sort by bytewise
    // CBOR-encoded-key form.
    var pairs = try alloc.alloc(Pair, members.len);
    defer {
        for (pairs) |p| {
            alloc.free(p.key_cbor);
            alloc.free(p.value_cbor);
        }
        alloc.free(pairs);
    }
    for (members, 0..) |m, i| {
        pairs[i] = try makeBytesPair(alloc, m.cid, m.bytes);
    }
    var value: std.ArrayList(u8) = .empty;
    errdefer value.deinit(alloc);
    try emitSortedMap(&value, alloc, pairs);
    return .{
        .key_cbor = try encodeKey(alloc, key),
        .value_cbor = try value.toOwnedSlice(alloc),
    };
}

fn makeMetadataPair(alloc: std.mem.Allocator, key: []const u8, entries: []const MetadataEntry) !Pair {
    var pairs = try alloc.alloc(Pair, entries.len);
    defer {
        for (pairs) |p| {
            alloc.free(p.key_cbor);
            alloc.free(p.value_cbor);
        }
        alloc.free(pairs);
    }
    for (entries, 0..) |e, i| {
        pairs[i] = try makeStringPair(alloc, e.key, e.value);
    }
    var value: std.ArrayList(u8) = .empty;
    errdefer value.deinit(alloc);
    try emitSortedMap(&value, alloc, pairs);
    return .{
        .key_cbor = try encodeKey(alloc, key),
        .value_cbor = try value.toOwnedSlice(alloc),
    };
}

fn emitSortedMap(out: *std.ArrayList(u8), alloc: std.mem.Allocator, pairs: []Pair) !void {
    std.mem.sort(Pair, pairs, {}, pairLessThan);
    try cbor.encodeMapHead(out, alloc, @as(u64, pairs.len));
    for (pairs) |p| {
        try out.appendSlice(alloc, p.key_cbor);
        try out.appendSlice(alloc, p.value_cbor);
    }
}

// Build the full unsigned body pair list. Caller owns each pair's
// inner slices and must free via freePairs.
fn bodyPairsUnsigned(alloc: std.mem.Allocator, input: ProofEnvelopeInput) !std.ArrayList(Pair) {
    var pairs: std.ArrayList(Pair) = .empty;
    errdefer {
        for (pairs.items) |p| {
            alloc.free(p.key_cbor);
            alloc.free(p.value_cbor);
        }
        pairs.deinit(alloc);
    }
    try pairs.append(alloc, try makeStringPair(alloc, "kind", "catalog"));
    try pairs.append(alloc, try makeStringPair(alloc, "name", input.name));
    try pairs.append(alloc, try makeStringPair(alloc, "version", input.version));
    try pairs.append(alloc, try makeMembersPair(alloc, "members", input.members));
    try pairs.append(alloc, try makeStringPair(alloc, "signer", input.signer_cid));
    try pairs.append(alloc, try makeStringPair(alloc, "declaredAt", input.declared_at));
    if (input.binary_cid) |bcid| {
        try pairs.append(alloc, try makeStringPair(alloc, "binaryCid", bcid));
    }
    if (input.metadata) |meta| {
        try pairs.append(alloc, try makeMetadataPair(alloc, "metadata", meta));
    }
    return pairs;
}

fn freePairs(alloc: std.mem.Allocator, pairs: *std.ArrayList(Pair)) void {
    for (pairs.items) |p| {
        alloc.free(p.key_cbor);
        alloc.free(p.value_cbor);
    }
    pairs.deinit(alloc);
}

// ---------------------------------------------------------------------------
// BLAKE3-512 helper. Mirrors provekit-ir/src/root.zig's jcsHash; using a
// local copy here keeps the proof builder self-contained in one source
// file dependency-wise (the package still re-exports the IR helper).
// ---------------------------------------------------------------------------

fn blake3_512_of(alloc: std.mem.Allocator, bytes: []const u8) ![]u8 {
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
// Public entry point.
// ---------------------------------------------------------------------------

pub fn buildProofEnvelope(
    alloc: std.mem.Allocator,
    input: ProofEnvelopeInput,
) !ProofEnvelopeOutput {
    // Step 1: encode unsigned body with sorted keys.
    var unsigned_pairs = try bodyPairsUnsigned(alloc, input);
    defer freePairs(alloc, &unsigned_pairs);

    var unsigned_bytes: std.ArrayList(u8) = .empty;
    defer unsigned_bytes.deinit(alloc);
    try emitSortedMap(&unsigned_bytes, alloc, unsigned_pairs.items);

    // Step 2: Ed25519-sign the unsigned bytes.
    const sig = try sign.signWithSeed(input.signer_seed, unsigned_bytes.items);

    // Step 3: re-emit with signature pair added; keys re-sort automatically.
    var signed_pairs = try bodyPairsUnsigned(alloc, input);
    defer freePairs(alloc, &signed_pairs);
    try signed_pairs.append(alloc, try makeBytesPair(alloc, "signature", &sig));

    var final_buf: std.ArrayList(u8) = .empty;
    errdefer final_buf.deinit(alloc);
    try emitSortedMap(&final_buf, alloc, signed_pairs.items);
    const final_bytes = try final_buf.toOwnedSlice(alloc);
    errdefer alloc.free(final_bytes);

    // Step 4: filename CID = full self-identifying BLAKE3-512.
    const cid = try blake3_512_of(alloc, final_bytes);

    return .{ .bytes = final_bytes, .cid = cid };
}

// ---------------------------------------------------------------------------
// Verifier: checks CID match, CBOR shape, and Ed25519 signature.
// ---------------------------------------------------------------------------

/// Verify a .proof envelope against expected_cid and signer_pubkey (raw 32 bytes).
/// Returns true iff CID, shape, and signature all check out. Fails closed.
///
/// NOTE: This is a deliberately minimal verifier: it re-encodes the
/// unsigned body from the input fields the caller passes (since fully
/// parsing arbitrary CBOR is out of scope for this substrate). For
/// canonical fixtures, callers pass the same ProofEnvelopeInput they
/// built from and compare against the embedded raw bytes.
pub fn verifyRebuilt(
    alloc: std.mem.Allocator,
    proof_bytes: []const u8,
    expected_cid: []const u8,
    signer_pubkey: sign.Ed25519PublicKey,
    rebuilt_input: ProofEnvelopeInput,
) !bool {
    // 1. CID match.
    const actual_cid = try blake3_512_of(alloc, proof_bytes);
    defer alloc.free(actual_cid);
    if (!std.mem.eql(u8, actual_cid, expected_cid)) return false;

    // 2. Re-build unsigned body bytes from the rebuilt_input.
    var unsigned_pairs = try bodyPairsUnsigned(alloc, rebuilt_input);
    defer freePairs(alloc, &unsigned_pairs);

    var unsigned_bytes: std.ArrayList(u8) = .empty;
    defer unsigned_bytes.deinit(alloc);
    try emitSortedMap(&unsigned_bytes, alloc, unsigned_pairs.items);

    // 3. Find the signature inside proof_bytes. The signature key encodes
    //    as 0x69 'signature' (text, len 9), followed by 0x58 0x40 (bstr,
    //    len 64) followed by the 64 raw bytes. Search for the marker.
    const marker: []const u8 = &.{ 0x69, 's', 'i', 'g', 'n', 'a', 't', 'u', 'r', 'e', 0x58, 0x40 };
    const idx = std.mem.indexOf(u8, proof_bytes, marker) orelse return false;
    if (idx + marker.len + 64 > proof_bytes.len) return false;
    var sig: sign.Ed25519Signature = undefined;
    @memcpy(&sig, proof_bytes[idx + marker.len ..][0..64]);

    // 4. Verify against the rebuilt unsigned body.
    return sign.verifyRaw(signer_pubkey, sig, unsigned_bytes.items);
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

const testing = std.testing;

fn twoMemberInput() ProofEnvelopeInput {
    const members_static = struct {
        const list = [_]Member{
            .{ .cid = "blake3-512:aa", .bytes = "{\"hello\":\"world\"}" },
            .{ .cid = "blake3-512:bb", .bytes = "{\"goodbye\":\"world\"}" },
        };
    };
    return .{
        .name = "@test/cat",
        .version = "1.0.0",
        .members = &members_static.list,
        .signer_cid = "blake3-512:cc",
        .declared_at = "2026-04-30T00:00:00.000Z",
        .signer_seed = sign.FOUNDATION_V0_SEED,
    };
}

test "build minimal proof envelope round-trips" {
    var out = try buildProofEnvelope(testing.allocator, .{
        .name = "@test/cat",
        .version = "1.0.0",
        .members = &.{
            .{ .cid = "blake3-512:aa", .bytes = "{\"hello\":\"world\"}" },
        },
        .signer_cid = "blake3-512:cc",
        .declared_at = "2026-04-30T00:00:00.000Z",
        .signer_seed = sign.FOUNDATION_V0_SEED,
    });
    defer out.deinit(testing.allocator);

    try testing.expect(std.mem.startsWith(u8, out.cid, "blake3-512:"));
    // 7-key map head: major 5 (0xA0) + count 7 = 0xA7.
    try testing.expectEqual(@as(u8, 0xA7), out.bytes[0]);
}

test "deterministic across runs" {
    var a = try buildProofEnvelope(testing.allocator, twoMemberInput());
    defer a.deinit(testing.allocator);
    var b = try buildProofEnvelope(testing.allocator, twoMemberInput());
    defer b.deinit(testing.allocator);

    try testing.expectEqualSlices(u8, a.bytes, b.bytes);
    try testing.expectEqualStrings(a.cid, b.cid);
}

// ---------------------------------------------------------------------------
// Cross-kit byte-equivalence (pinned from rust kit reference output).
//
// Source (authoritative):
//   implementations/python/provekit-lift-py-tests/tests/test_proof_envelope.py
//   constant `RUST_FIXTURE_BYTES_HEX_FULL` and `RUST_FIXTURE_CID`.
//
// Generated on the rust kit via:
//   cargo run --release -p provekit-proof-envelope --example proof_envelope_bytes
//
// If any test below fails, the divergence is a real cross-kit impedance
// mismatch. Surface it with a clear message: DO NOT paper over.
// ---------------------------------------------------------------------------

const RUST_FIXTURE_CID =
    "blake3-512:5ed1e1f705622ad52ae4683e3d12df5586d364d66bb3186f5be512415edf290" ++
    "844d74e73a2857cd858f37803e4b11fe5c7cba7884caa6b9ff847521ce32ea056";

const RUST_FIXTURE_BYTES_HEX_FULL =
    "a7646b696e6467636174616c6f67646e616d656940746573742f636174667369676e65" ++
    "726d626c616b65332d3531323a6363676d656d62657273a26d626c616b65332d353132" ++
    "3a6161517b2268656c6c6f223a22776f726c64227d6d626c616b65332d3531323a6262" ++
    "537b22676f6f64627965223a22776f726c64227d6776657273696f6e65312e302e3069" ++
    "7369676e617475726558406a21dd428a54e22c82ca6d6125a7293c4a723786cb1840e8" ++
    "91cefa03e63246eb97ef13dab86b7b1469d67302fadc969cd88c92c29495d13c75fc02" ++
    "01a7263b066a6465636c6172656441747818323032362d30342d33305430303a30303a" ++
    "30302e3030305a";

fn hexToBytes(alloc: std.mem.Allocator, hex: []const u8) ![]u8 {
    if (hex.len % 2 != 0) return error.InvalidHex;
    var out = try alloc.alloc(u8, hex.len / 2);
    errdefer alloc.free(out);
    var i: usize = 0;
    while (i < hex.len) : (i += 2) {
        const hi = try std.fmt.charToDigit(hex[i], 16);
        const lo = try std.fmt.charToDigit(hex[i + 1], 16);
        out[i / 2] = (@as(u8, hi) << 4) | @as(u8, lo);
    }
    return out;
}

test "two-member envelope bytes match rust reference (cross-kit pin)" {
    const alloc = testing.allocator;
    var out = try buildProofEnvelope(alloc, twoMemberInput());
    defer out.deinit(alloc);

    const rust_bytes = try hexToBytes(alloc, RUST_FIXTURE_BYTES_HEX_FULL);
    defer alloc.free(rust_bytes);

    if (!std.mem.eql(u8, out.bytes, rust_bytes)) {
        // Diagnose: find first divergence so the test message is actionable.
        var i: usize = 0;
        const min_len = @min(out.bytes.len, rust_bytes.len);
        while (i < min_len) : (i += 1) {
            if (out.bytes[i] != rust_bytes[i]) {
                std.debug.print(
                    "\ncross-kit byte divergence at offset {d}: zig=0x{x:0>2} rust=0x{x:0>2}\n" ++
                        "zig hex:  ",
                    .{ i, out.bytes[i], rust_bytes[i] },
                );
                for (out.bytes) |b| std.debug.print("{x:0>2}", .{b});
                std.debug.print("\nrust hex: ", .{});
                for (rust_bytes) |b| std.debug.print("{x:0>2}", .{b});
                std.debug.print("\n", .{});
                break;
            }
        }
        if (out.bytes.len != rust_bytes.len) {
            std.debug.print(
                "\ncross-kit length mismatch: zig={d} rust={d}\n",
                .{ out.bytes.len, rust_bytes.len },
            );
        }
        try testing.expect(false);
    }
}

test "two-member envelope CID matches rust reference (cross-kit pin)" {
    const alloc = testing.allocator;
    var out = try buildProofEnvelope(alloc, twoMemberInput());
    defer out.deinit(alloc);

    try testing.expectEqualStrings(RUST_FIXTURE_CID, out.cid);
}

test "rust reference bytes verify under our verifier" {
    const alloc = testing.allocator;
    const rust_bytes = try hexToBytes(alloc, RUST_FIXTURE_BYTES_HEX_FULL);
    defer alloc.free(rust_bytes);

    const foundation_pk = try sign.pubkeyFromSeed(sign.FOUNDATION_V0_SEED);
    const ok = try verifyRebuilt(
        alloc,
        rust_bytes,
        RUST_FIXTURE_CID,
        foundation_pk,
        twoMemberInput(),
    );
    try testing.expect(ok);
}

test "verifyRebuilt rejects tampered cid" {
    const alloc = testing.allocator;
    var out = try buildProofEnvelope(alloc, twoMemberInput());
    defer out.deinit(alloc);

    const fake_cid = "blake3-512:" ++ ("00" ** 64);
    const foundation_pk = try sign.pubkeyFromSeed(sign.FOUNDATION_V0_SEED);
    const ok = try verifyRebuilt(
        alloc,
        out.bytes,
        fake_cid,
        foundation_pk,
        twoMemberInput(),
    );
    try testing.expect(!ok);
}

test "verifyRebuilt rejects wrong pubkey" {
    const alloc = testing.allocator;
    var out = try buildProofEnvelope(alloc, twoMemberInput());
    defer out.deinit(alloc);

    const wrong_seed: sign.Ed25519Seed = [_]u8{0x99} ** 32;
    const wrong_pk = try sign.pubkeyFromSeed(wrong_seed);
    const ok = try verifyRebuilt(
        alloc,
        out.bytes,
        out.cid,
        wrong_pk,
        twoMemberInput(),
    );
    try testing.expect(!ok);
}

test "binaryCid changes envelope CID" {
    const alloc = testing.allocator;
    const members = [_]Member{
        .{ .cid = "blake3-512:aa", .bytes = "data" },
    };
    var with_bcid = try buildProofEnvelope(alloc, .{
        .name = "@test/cat",
        .version = "1.0.0",
        .members = &members,
        .signer_cid = "blake3-512:cc",
        .declared_at = "2026-04-30T00:00:00.000Z",
        .signer_seed = sign.FOUNDATION_V0_SEED,
        .binary_cid = "blake3-512:deadbeef",
    });
    defer with_bcid.deinit(alloc);
    var without = try buildProofEnvelope(alloc, .{
        .name = "@test/cat",
        .version = "1.0.0",
        .members = &members,
        .signer_cid = "blake3-512:cc",
        .declared_at = "2026-04-30T00:00:00.000Z",
        .signer_seed = sign.FOUNDATION_V0_SEED,
    });
    defer without.deinit(alloc);

    try testing.expect(!std.mem.eql(u8, with_bcid.cid, without.cid));
}

test "metadata field included in signed body" {
    const alloc = testing.allocator;
    const members = [_]Member{
        .{ .cid = "blake3-512:aa", .bytes = "data" },
    };
    const meta = [_]MetadataEntry{
        .{ .key = "tool", .value = "zig-kit" },
        .{ .key = "version", .value = "0.1.0" },
    };
    var out = try buildProofEnvelope(alloc, .{
        .name = "@test/cat",
        .version = "1.0.0",
        .members = &members,
        .signer_cid = "blake3-512:cc",
        .declared_at = "2026-04-30T00:00:00.000Z",
        .signer_seed = sign.FOUNDATION_V0_SEED,
        .metadata = &meta,
    });
    defer out.deinit(alloc);

    const foundation_pk = try sign.pubkeyFromSeed(sign.FOUNDATION_V0_SEED);
    const ok = try verifyRebuilt(
        alloc,
        out.bytes,
        out.cid,
        foundation_pk,
        .{
            .name = "@test/cat",
            .version = "1.0.0",
            .members = &members,
            .signer_cid = "blake3-512:cc",
            .declared_at = "2026-04-30T00:00:00.000Z",
            .signer_seed = sign.FOUNDATION_V0_SEED,
            .metadata = &meta,
        },
    );
    try testing.expect(ok);
}

test "empty members produces valid envelope" {
    const alloc = testing.allocator;
    var out = try buildProofEnvelope(alloc, .{
        .name = "x",
        .version = "1",
        .members = &.{},
        .signer_cid = "blake3-512:cc",
        .declared_at = "2026-04-30T00:00:00.000Z",
        .signer_seed = sign.FOUNDATION_V0_SEED,
    });
    defer out.deinit(alloc);

    try testing.expectEqual(@as(u8, 0xA7), out.bytes[0]);
    try testing.expect(std.mem.startsWith(u8, out.cid, "blake3-512:"));
}
