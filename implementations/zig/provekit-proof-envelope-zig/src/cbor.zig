// SPDX-License-Identifier: Apache-2.0
//
// Deterministic CBOR encoder. RFC 8949 §4.2.1 rules:
//   - shortest-form integer encoding (smallest of short / u8 / u16 / u32 / u64)
//   - definite-length items only
//   - map keys sorted in bytewise lex order of their CBOR-encoded form
//   - we emit only the major types we need: unsigned int, byte string,
//     text string, array, map.
//
// Mirrors implementations/rust/provekit-proof-envelope/src/cbor.rs 1:1
// and implementations/cpp/provekit/proof-envelope/cbor.cpp.

const std = @import("std");

pub const CborMajor = enum(u8) {
    unsigned_int = 0,
    byte_string = 2,
    text_string = 3,
    array = 4,
    map = 5,
};

pub fn appendHead(out: *std.ArrayList(u8), alloc: std.mem.Allocator, major: CborMajor, arg: u64) !void {
    const mt: u8 = @as(u8, @intFromEnum(major)) << 5;
    if (arg < 24) {
        try out.append(alloc, mt | @as(u8, @intCast(arg)));
        return;
    }
    if (arg <= 0xFF) {
        try out.append(alloc, mt | 24);
        try out.append(alloc, @as(u8, @intCast(arg)));
        return;
    }
    if (arg <= 0xFFFF) {
        try out.append(alloc, mt | 25);
        try out.append(alloc, @as(u8, @intCast(arg >> 8)));
        try out.append(alloc, @as(u8, @intCast(arg & 0xFF)));
        return;
    }
    if (arg <= 0xFFFF_FFFF) {
        try out.append(alloc, mt | 26);
        try out.append(alloc, @as(u8, @intCast(arg >> 24)));
        try out.append(alloc, @as(u8, @intCast((arg >> 16) & 0xFF)));
        try out.append(alloc, @as(u8, @intCast((arg >> 8) & 0xFF)));
        try out.append(alloc, @as(u8, @intCast(arg & 0xFF)));
        return;
    }
    try out.append(alloc, mt | 27);
    var i: u6 = 8;
    while (i > 0) {
        i -= 1;
        try out.append(alloc, @as(u8, @intCast((arg >> (@as(u6, i) * 8)) & 0xFF)));
    }
}

pub fn encodeUint(out: *std.ArrayList(u8), alloc: std.mem.Allocator, value: u64) !void {
    try appendHead(out, alloc, .unsigned_int, value);
}

pub fn encodeBstr(out: *std.ArrayList(u8), alloc: std.mem.Allocator, bytes: []const u8) !void {
    try appendHead(out, alloc, .byte_string, @as(u64, bytes.len));
    try out.appendSlice(alloc, bytes);
}

pub fn encodeTstr(out: *std.ArrayList(u8), alloc: std.mem.Allocator, utf8: []const u8) !void {
    try appendHead(out, alloc, .text_string, @as(u64, utf8.len));
    try out.appendSlice(alloc, utf8);
}

pub fn encodeArrayHead(out: *std.ArrayList(u8), alloc: std.mem.Allocator, count: u64) !void {
    try appendHead(out, alloc, .array, count);
}

pub fn encodeMapHead(out: *std.ArrayList(u8), alloc: std.mem.Allocator, count: u64) !void {
    try appendHead(out, alloc, .map, count);
}

// ---------------------------------------------------------------------------
// Tests: pinned vectors from RFC 8949 §3.3 + the rust kit.
// ---------------------------------------------------------------------------

const testing = std.testing;

test "shortest-form uint: 0..23 single byte" {
    var buf: std.ArrayList(u8) = .empty;
    defer buf.deinit(testing.allocator);

    try encodeUint(&buf, testing.allocator, 0);
    try testing.expectEqualSlices(u8, &.{0x00}, buf.items);

    buf.clearRetainingCapacity();
    try encodeUint(&buf, testing.allocator, 23);
    try testing.expectEqualSlices(u8, &.{0x17}, buf.items);
}

test "shortest-form uint: 24..255 two bytes" {
    var buf: std.ArrayList(u8) = .empty;
    defer buf.deinit(testing.allocator);

    try encodeUint(&buf, testing.allocator, 24);
    try testing.expectEqualSlices(u8, &.{ 0x18, 24 }, buf.items);

    buf.clearRetainingCapacity();
    try encodeUint(&buf, testing.allocator, 255);
    try testing.expectEqualSlices(u8, &.{ 0x18, 0xff }, buf.items);
}

test "shortest-form uint: 256..65535 three bytes" {
    var buf: std.ArrayList(u8) = .empty;
    defer buf.deinit(testing.allocator);

    try encodeUint(&buf, testing.allocator, 256);
    try testing.expectEqualSlices(u8, &.{ 0x19, 0x01, 0x00 }, buf.items);
}

test "shortest-form uint: 65536..2^32-1 five bytes" {
    var buf: std.ArrayList(u8) = .empty;
    defer buf.deinit(testing.allocator);

    try encodeUint(&buf, testing.allocator, 65536);
    try testing.expectEqualSlices(u8, &.{ 0x1a, 0x00, 0x01, 0x00, 0x00 }, buf.items);
}

test "tstr emits major-3 head then utf8 bytes" {
    var buf: std.ArrayList(u8) = .empty;
    defer buf.deinit(testing.allocator);

    try encodeTstr(&buf, testing.allocator, "hello");
    // major 3 (text), len 5 short form: 0x60 | 5 = 0x65, then "hello"
    try testing.expectEqualSlices(u8, &.{ 0x65, 'h', 'e', 'l', 'l', 'o' }, buf.items);
}

test "bstr emits major-2 head then raw bytes" {
    var buf: std.ArrayList(u8) = .empty;
    defer buf.deinit(testing.allocator);

    try encodeBstr(&buf, testing.allocator, &.{ 0xde, 0xad, 0xbe, 0xef });
    try testing.expectEqualSlices(u8, &.{ 0x44, 0xde, 0xad, 0xbe, 0xef }, buf.items);
}

test "map head with 7 entries" {
    var buf: std.ArrayList(u8) = .empty;
    defer buf.deinit(testing.allocator);

    try encodeMapHead(&buf, testing.allocator, 7);
    // major 5 (map), count 7 short form: 0xa0 | 7 = 0xa7
    try testing.expectEqualSlices(u8, &.{0xa7}, buf.items);
}
