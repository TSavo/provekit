// SPDX-License-Identifier: Apache-2.0
//
// BLAKE3-512 hash helper. v1.1.0 of the protocol mandates
// self-identifying hashes of the form:
//
//   "blake3-512:" + lowercase-hex(64-byte-digest)
//
// We use std.crypto.hash.Blake3 at its 64-byte (512-bit) extended-output
// length. There is NO truncation. The protocol cut is scorched earth:
// this is the only hash function permitted in v1.1.0, and it is always
// 512 bits wide.
//
// Mirrors implementations/rust/provekit-canonicalizer/src/hash.rs 1:1.

const std = @import("std");

pub const PREFIX: []const u8 = "blake3-512:";

/// Hash arbitrary bytes into the self-identifying string form.
/// Caller owns the returned slice (allocated through `alloc`).
pub fn blake3_512Of(alloc: std.mem.Allocator, bytes: []const u8) ![]u8 {
    var digest: [64]u8 = undefined;
    var hasher = std.crypto.hash.Blake3.init(.{});
    hasher.update(bytes);
    hasher.final(&digest);

    const hex = std.fmt.bytesToHex(digest, .lower);
    var out = try alloc.alloc(u8, PREFIX.len + hex.len);
    @memcpy(out[0..PREFIX.len], PREFIX);
    @memcpy(out[PREFIX.len..], &hex);
    return out;
}

test "empty input produces well-formed CID" {
    const alloc = std.testing.allocator;
    const h = try blake3_512Of(alloc, "");
    defer alloc.free(h);
    try std.testing.expect(std.mem.startsWith(u8, h, PREFIX));
    try std.testing.expectEqual(@as(usize, PREFIX.len + 128), h.len);
}

test "deterministic across calls" {
    const alloc = std.testing.allocator;
    const a = try blake3_512Of(alloc, "hello");
    defer alloc.free(a);
    const b = try blake3_512Of(alloc, "hello");
    defer alloc.free(b);
    try std.testing.expectEqualStrings(a, b);
}

test "distinct inputs distinct hashes" {
    const alloc = std.testing.allocator;
    const a = try blake3_512Of(alloc, "hello");
    defer alloc.free(a);
    const b = try blake3_512Of(alloc, "world");
    defer alloc.free(b);
    try std.testing.expect(!std.mem.eql(u8, a, b));
}

// Cross-kit byte-equivalence pins for BLAKE3-512 are anchored in two
// places:
//   * `provekit-ir/src/cross_kit_bridges.zig` pins BLAKE3-of-JCS for 10
//     contract Decls against rust/python/go/ts.
//   * `byte_equivalence_test.zig` (this package) pins the empty
//     contractSetCid against the value committed in
//     `.provekit/self-contracts-attestations/zig.json` — that pin
//     transitively exercises hash.zig.
// No additional vector is pinned here.
