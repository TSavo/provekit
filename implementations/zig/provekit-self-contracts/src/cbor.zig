// SPDX-License-Identifier: Apache-2.0
//
// Deterministic CBOR encoder. RFC 8949 §4.2.1 "Core Deterministic Encoding":
//   - shortest-form integer encoding (smallest of short / u8 / u16 / u32 / u64)
//   - definite-length items only
//   - map keys sorted in bytewise lex order of their CBOR-encoded form
//   - we emit only the major types we need: unsigned int, byte string,
//     text string, array, map.
//
// Mirrors implementations/rust/provekit-proof-envelope/src/cbor.rs and
// implementations/cpp/provekit/proof-envelope/cbor.cpp 1:1.

const std = @import("std");

pub const Major = enum(u3) {
    unsigned_int = 0,
    byte_string = 2,
    text_string = 3,
    array = 4,
    map = 5,
};

/// Append a CBOR head: `(major << 5) | tagOrLen`, plus the appropriate
/// number of trailing big-endian bytes (0 / 1 / 2 / 4 / 8) per the
/// shortest-form rule.
pub fn appendHead(alloc: std.mem.Allocator, out: *std.ArrayList(u8), major: Major, arg: u64) !void {
    const mt: u8 = @as(u8, @intFromEnum(major)) << 5;
    if (arg < 24) {
        try out.append(alloc, mt | @as(u8, @intCast(arg)));
        return;
    }
    if (arg <= 0xFF) {
        try out.append(alloc, mt | 24);
        try out.append(alloc, @intCast(arg));
        return;
    }
    if (arg <= 0xFFFF) {
        try out.append(alloc, mt | 25);
        try out.append(alloc, @intCast((arg >> 8) & 0xFF));
        try out.append(alloc, @intCast(arg & 0xFF));
        return;
    }
    if (arg <= 0xFFFF_FFFF) {
        try out.append(alloc, mt | 26);
        try out.append(alloc, @intCast((arg >> 24) & 0xFF));
        try out.append(alloc, @intCast((arg >> 16) & 0xFF));
        try out.append(alloc, @intCast((arg >> 8) & 0xFF));
        try out.append(alloc, @intCast(arg & 0xFF));
        return;
    }
    try out.append(alloc, mt | 27);
    var i: u6 = 8;
    while (i > 0) {
        i -= 1;
        try out.append(alloc, @intCast((arg >> (@as(u6, i) * 8)) & 0xFF));
    }
}

pub fn encodeUint(alloc: std.mem.Allocator, out: *std.ArrayList(u8), value: u64) !void {
    try appendHead(alloc, out, .unsigned_int, value);
}

pub fn encodeBstr(alloc: std.mem.Allocator, out: *std.ArrayList(u8), bytes: []const u8) !void {
    try appendHead(alloc, out, .byte_string, bytes.len);
    try out.appendSlice(alloc, bytes);
}

pub fn encodeTstr(alloc: std.mem.Allocator, out: *std.ArrayList(u8), s: []const u8) !void {
    try appendHead(alloc, out, .text_string, s.len);
    try out.appendSlice(alloc, s);
}

pub fn encodeArrayHead(alloc: std.mem.Allocator, out: *std.ArrayList(u8), count: u64) !void {
    try appendHead(alloc, out, .array, count);
}

pub fn encodeMapHead(alloc: std.mem.Allocator, out: *std.ArrayList(u8), count: u64) !void {
    try appendHead(alloc, out, .map, count);
}

// ---------------------------------------------------------------------------
// Sorted-map emitter
// ---------------------------------------------------------------------------

/// One CBOR (key, value) pair, with the key already encoded so the outer
/// map can sort by bytewise CBOR-encoded-key form per RFC 8949 §4.2.1.
pub const Pair = struct {
    key_cbor: []u8,
    value_cbor: []u8,

    pub fn deinit(self: Pair, alloc: std.mem.Allocator) void {
        alloc.free(self.key_cbor);
        alloc.free(self.value_cbor);
    }
};

fn pairLess(_: void, a: Pair, b: Pair) bool {
    return std.mem.lessThan(u8, a.key_cbor, b.key_cbor);
}

/// Emit a sorted map. `pairs` is sorted in place (bytewise on key_cbor).
/// Caller still owns the pair buffers; this function does not free them.
pub fn emitSortedMap(alloc: std.mem.Allocator, out: *std.ArrayList(u8), pairs: []Pair) !void {
    std.mem.sort(Pair, pairs, {}, pairLess);
    try encodeMapHead(alloc, out, pairs.len);
    for (pairs) |p| {
        try out.appendSlice(alloc, p.key_cbor);
        try out.appendSlice(alloc, p.value_cbor);
    }
}

/// Helper: produce a Pair where the key is a tstr (text string) and the
/// value is a tstr.
pub fn makeStringPair(alloc: std.mem.Allocator, key: []const u8, value: []const u8) !Pair {
    var k_buf: std.ArrayList(u8) = .empty;
    errdefer k_buf.deinit(alloc);
    try encodeTstr(alloc, &k_buf, key);
    var v_buf: std.ArrayList(u8) = .empty;
    errdefer v_buf.deinit(alloc);
    try encodeTstr(alloc, &v_buf, value);
    return .{
        .key_cbor = try k_buf.toOwnedSlice(alloc),
        .value_cbor = try v_buf.toOwnedSlice(alloc),
    };
}

/// Helper: tstr key with a bstr (byte string) value.
pub fn makeBytesPair(alloc: std.mem.Allocator, key: []const u8, value: []const u8) !Pair {
    var k_buf: std.ArrayList(u8) = .empty;
    errdefer k_buf.deinit(alloc);
    try encodeTstr(alloc, &k_buf, key);
    var v_buf: std.ArrayList(u8) = .empty;
    errdefer v_buf.deinit(alloc);
    try encodeBstr(alloc, &v_buf, value);
    return .{
        .key_cbor = try k_buf.toOwnedSlice(alloc),
        .value_cbor = try v_buf.toOwnedSlice(alloc),
    };
}

/// Helper: tstr key with a raw-encoded value (caller pre-encoded the value).
pub fn makeRawPair(alloc: std.mem.Allocator, key: []const u8, raw_value: []const u8) !Pair {
    var k_buf: std.ArrayList(u8) = .empty;
    errdefer k_buf.deinit(alloc);
    try encodeTstr(alloc, &k_buf, key);
    const v_owned = try alloc.dupe(u8, raw_value);
    return .{
        .key_cbor = try k_buf.toOwnedSlice(alloc),
        .value_cbor = v_owned,
    };
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

test "shortest-form uint" {
    const alloc = std.testing.allocator;
    {
        var buf: std.ArrayList(u8) = .empty;
        defer buf.deinit(alloc);
        try encodeUint(alloc, &buf, 0);
        try std.testing.expectEqualSlices(u8, &.{0x00}, buf.items);
    }
    {
        var buf: std.ArrayList(u8) = .empty;
        defer buf.deinit(alloc);
        try encodeUint(alloc, &buf, 23);
        try std.testing.expectEqualSlices(u8, &.{0x17}, buf.items);
    }
    {
        var buf: std.ArrayList(u8) = .empty;
        defer buf.deinit(alloc);
        try encodeUint(alloc, &buf, 24);
        try std.testing.expectEqualSlices(u8, &.{ 0x18, 24 }, buf.items);
    }
    {
        var buf: std.ArrayList(u8) = .empty;
        defer buf.deinit(alloc);
        try encodeUint(alloc, &buf, 256);
        try std.testing.expectEqualSlices(u8, &.{ 0x19, 0x01, 0x00 }, buf.items);
    }
    {
        var buf: std.ArrayList(u8) = .empty;
        defer buf.deinit(alloc);
        try encodeUint(alloc, &buf, 65536);
        try std.testing.expectEqualSlices(u8, &.{ 0x1a, 0x00, 0x01, 0x00, 0x00 }, buf.items);
    }
    {
        var buf: std.ArrayList(u8) = .empty;
        defer buf.deinit(alloc);
        try encodeUint(alloc, &buf, 0x1_0000_0000);
        try std.testing.expectEqualSlices(
            u8,
            &.{ 0x1b, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00 },
            buf.items,
        );
    }
}

test "tstr round-trip head" {
    const alloc = std.testing.allocator;
    var buf: std.ArrayList(u8) = .empty;
    defer buf.deinit(alloc);
    try encodeTstr(alloc, &buf, "hello");
    try std.testing.expectEqualSlices(
        u8,
        &.{ 0x65, 'h', 'e', 'l', 'l', 'o' },
        buf.items,
    );
}

test "bstr empty" {
    const alloc = std.testing.allocator;
    var buf: std.ArrayList(u8) = .empty;
    defer buf.deinit(alloc);
    try encodeBstr(alloc, &buf, "");
    try std.testing.expectEqualSlices(u8, &.{0x40}, buf.items);
}

test "map head zero" {
    const alloc = std.testing.allocator;
    var buf: std.ArrayList(u8) = .empty;
    defer buf.deinit(alloc);
    try encodeMapHead(alloc, &buf, 0);
    try std.testing.expectEqualSlices(u8, &.{0xa0}, buf.items);
}

test "sortedMap orders by bytewise CBOR-encoded key" {
    // RFC 8949 §4.2.1: keys are sorted by their CBOR-encoded form. For
    // tstr keys the sort is bytewise on (head + UTF-8 body), which
    // collapses to "shorter keys first; equal-length keys in byte order"
    // because the head's length-byte dominates the comparison.
    const alloc = std.testing.allocator;
    var pairs: [3]Pair = undefined;
    pairs[0] = try makeStringPair(alloc, "version", "0.0.1");
    pairs[1] = try makeStringPair(alloc, "kind", "catalog");
    pairs[2] = try makeStringPair(alloc, "name", "@x/y");
    defer for (pairs) |p| p.deinit(alloc);

    var out: std.ArrayList(u8) = .empty;
    defer out.deinit(alloc);
    try emitSortedMap(alloc, &out, &pairs);
    // First byte: map head with 3 entries -> 0xa3
    try std.testing.expectEqual(@as(u8, 0xa3), out.items[0]);
    // After head: shortest tstr key first ("kind"=4 bytes < "name"=4
    // == kind sorts first because 'k' < 'n'). Just sanity-check that the
    // head byte for the first key is 0x64 ("tstr len 4").
    try std.testing.expectEqual(@as(u8, 0x64), out.items[1]);
}
