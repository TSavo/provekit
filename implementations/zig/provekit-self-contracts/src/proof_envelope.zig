// SPDX-License-Identifier: Apache-2.0
//
// .proof envelope builder. Per RFC 8949 §4.2.1 + the .proof spec
// (protocol/specs/2026-04-30-proof-file-format.md):
//
//   1. Build the unsigned body as a CBOR map with keys sorted by bytewise
//      lex order of their CBOR-encoded form.
//   2. ed25519-sign the unsigned-body bytes.
//   3. Re-emit the body with the signature added; keys re-sort
//      automatically (the new "signature" key slots in by lex order).
//   4. BLAKE3-512 the final bytes; the full self-identifying string
//      `"blake3-512:<128 hex>"` IS the catalog CID.
//
// The `members` map key is the embedded envelope's own CID, and the value
// is its canonical bytes (JCS-JSON for memento envelopes per the memento
// envelope grammar) wrapped as a CBOR byte string.
//
// Mirrors implementations/rust/provekit-proof-envelope/src/proof.rs 1:1.

const std = @import("std");
const cbor = @import("cbor.zig");
const hash = @import("hash.zig");
const signing = @import("signing.zig");

/// (cid, envelope-bytes) pair for a member of the catalog. Both slices are
/// borrowed; the caller retains ownership. CIDs are full self-identifying
/// strings (`"blake3-512:<128 hex>"`).
pub const Member = struct {
    cid: []const u8,
    bytes: []const u8,
};

/// Optional metadata key/value pair (UTF-8 strings).
pub const MetadataPair = struct {
    key: []const u8,
    value: []const u8,
};

pub const Input = struct {
    name: []const u8,
    version: []const u8,
    /// Optional CID of the compiled binary this proof verifies.
    binary_cid: ?[]const u8 = null,
    /// Optional metadata: tooling/diagnostic key-value map. Included in
    /// the signed payload (tamper-evident) but explicitly NON-NORMATIVE:
    /// verifiers MUST NOT use metadata for logic.
    metadata: []const MetadataPair = &.{},
    /// Members map: CID -> canonical bytes.
    members: []const Member,
    /// CID of the signer's public-key memento (or any resolvable CID).
    signer_cid: []const u8,
    /// Ed25519 seed bytes; deterministic signing.
    signer_seed: signing.Seed,
    /// ISO-8601 string with millisecond precision and trailing 'Z'.
    declared_at: []const u8,
};

pub const Output = struct {
    /// CBOR bytes of the signed catalog. Hash of these bytes IS the CID.
    bytes: []u8,
    /// Full self-identifying CID, e.g. `"blake3-512:<128 hex>"`.
    cid: []u8,

    pub fn deinit(self: Output, alloc: std.mem.Allocator) void {
        alloc.free(self.bytes);
        alloc.free(self.cid);
    }
};

/// Build the unsigned-or-signed body pair list. `extra_signature` is the
/// raw 64-byte signature when present; otherwise null.
fn buildPairs(
    alloc: std.mem.Allocator,
    input: Input,
    extra_signature: ?[]const u8,
) ![]cbor.Pair {
    var list: std.ArrayList(cbor.Pair) = .empty;
    errdefer {
        for (list.items) |p| p.deinit(alloc);
        list.deinit(alloc);
    }

    try list.append(alloc, try cbor.makeStringPair(alloc, "kind", "catalog"));
    try list.append(alloc, try cbor.makeStringPair(alloc, "name", input.name));
    try list.append(alloc, try cbor.makeStringPair(alloc, "version", input.version));

    // members pair: tstr(cid) -> bstr(bytes), inner map sorted bytewise.
    {
        var inner_pairs = try alloc.alloc(cbor.Pair, input.members.len);
        var inner_count: usize = 0;
        errdefer {
            for (inner_pairs[0..inner_count]) |p| p.deinit(alloc);
            alloc.free(inner_pairs);
        }
        for (input.members) |m| {
            inner_pairs[inner_count] = try cbor.makeBytesPair(alloc, m.cid, m.bytes);
            inner_count += 1;
        }
        var inner_buf: std.ArrayList(u8) = .empty;
        defer inner_buf.deinit(alloc);
        try cbor.emitSortedMap(alloc, &inner_buf, inner_pairs);
        for (inner_pairs) |p| p.deinit(alloc);
        alloc.free(inner_pairs);

        try list.append(alloc, try cbor.makeRawPair(alloc, "members", inner_buf.items));
    }

    try list.append(alloc, try cbor.makeStringPair(alloc, "signer", input.signer_cid));
    try list.append(alloc, try cbor.makeStringPair(alloc, "declaredAt", input.declared_at));

    if (input.binary_cid) |bcid| {
        try list.append(alloc, try cbor.makeStringPair(alloc, "binaryCid", bcid));
    }

    if (input.metadata.len > 0) {
        var meta_pairs = try alloc.alloc(cbor.Pair, input.metadata.len);
        var meta_count: usize = 0;
        errdefer {
            for (meta_pairs[0..meta_count]) |p| p.deinit(alloc);
            alloc.free(meta_pairs);
        }
        for (input.metadata) |kv| {
            meta_pairs[meta_count] = try cbor.makeStringPair(alloc, kv.key, kv.value);
            meta_count += 1;
        }
        var meta_buf: std.ArrayList(u8) = .empty;
        defer meta_buf.deinit(alloc);
        try cbor.emitSortedMap(alloc, &meta_buf, meta_pairs);
        for (meta_pairs) |p| p.deinit(alloc);
        alloc.free(meta_pairs);

        try list.append(alloc, try cbor.makeRawPair(alloc, "metadata", meta_buf.items));
    }

    if (extra_signature) |sig| {
        try list.append(alloc, try cbor.makeBytesPair(alloc, "signature", sig));
    }

    return list.toOwnedSlice(alloc);
}

/// Build the signed catalog. Caller owns `result.bytes` and `result.cid`.
pub fn build(alloc: std.mem.Allocator, input: Input) !Output {
    // Step 1: encode unsigned body with sorted keys.
    const unsigned_pairs = try buildPairs(alloc, input, null);
    defer {
        for (unsigned_pairs) |p| p.deinit(alloc);
        alloc.free(unsigned_pairs);
    }
    var unsigned_buf: std.ArrayList(u8) = .empty;
    defer unsigned_buf.deinit(alloc);
    try cbor.emitSortedMap(alloc, &unsigned_buf, unsigned_pairs);

    // Step 2: ed25519-sign the unsigned bytes.
    const sig = signing.signWithSeed(input.signer_seed, unsigned_buf.items);

    // Step 3: re-emit with signature added; keys re-sort automatically.
    const signed_pairs = try buildPairs(alloc, input, &sig);
    defer {
        for (signed_pairs) |p| p.deinit(alloc);
        alloc.free(signed_pairs);
    }
    var signed_buf: std.ArrayList(u8) = .empty;
    defer signed_buf.deinit(alloc);
    try cbor.emitSortedMap(alloc, &signed_buf, signed_pairs);
    const final_bytes = try alloc.dupe(u8, signed_buf.items);
    errdefer alloc.free(final_bytes);

    // Step 4: filename CID = full self-identifying BLAKE3-512.
    const cid = try hash.blake3_512Of(alloc, final_bytes);
    return .{
        .bytes = final_bytes,
        .cid = cid,
    };
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

test "build minimal proof round-trips" {
    const alloc = std.testing.allocator;
    const members = [_]Member{
        .{ .cid = "blake3-512:aa", .bytes = "{\"hello\":\"world\"}" },
    };
    const input = Input{
        .name = "@x/y",
        .version = "0.0.1",
        .members = &members,
        .signer_cid = "blake3-512:bb",
        .signer_seed = @splat(0x11),
        .declared_at = "2026-04-30T00:00:00.000Z",
    };
    const out = try build(alloc, input);
    defer out.deinit(alloc);
    try std.testing.expect(std.mem.startsWith(u8, out.cid, "blake3-512:"));
    // First byte: map head with 7 entries (kind, name, version, members,
    // signer, declaredAt, signature) = 0xa7.
    try std.testing.expectEqual(@as(u8, 0xa7), out.bytes[0]);
}

test "build empty-members catalog signs" {
    const alloc = std.testing.allocator;
    const empty: []const Member = &.{};
    const input = Input{
        .name = "@empty/test",
        .version = "0.0.0",
        .members = empty,
        .signer_cid = "blake3-512:bb",
        .signer_seed = @splat(0x42),
        .declared_at = "2026-05-03T18:00:00Z",
    };
    const out = try build(alloc, input);
    defer out.deinit(alloc);
    try std.testing.expect(out.bytes.len > 0);
    try std.testing.expect(std.mem.startsWith(u8, out.cid, "blake3-512:"));
    // Map head with 7 entries.
    try std.testing.expectEqual(@as(u8, 0xa7), out.bytes[0]);
}
