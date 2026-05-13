// SPDX-License-Identifier: Apache-2.0
//
// JCS-JSON encoder (RFC 8785) + Value tree.
//
// Mirrors implementations/rust/provekit-canonicalizer/src/{jcs.rs,value.rs}
// 1:1. The same rules apply across all 11 kits:
//
//   - Object keys sorted by Unicode code-point order. For ASCII-only keys
//     this collapses to byte-order; the protocol's keys are all ASCII so
//     byte-order suffices. Cross-kit byte-equivalence depends on this.
//   - Numbers: integers serialized as plain decimal digits (we only carry
//     i64; floats are not produced by the kit/mint flow).
//   - Strings: UTF-8 verbatim, escape `"` and `\\` and U+0000..U+001F as
//     `\u00XX` (lowercase hex). RFC 8785 also permits the named short
//     escapes (\n etc.) but the C++/Rust peers chose `\u00XX` for
//     determinism; we match.
//   - true / false / null verbatim.
//   - No whitespace anywhere.
//
// The Value tree is intentionally tiny. We keep insertion order on
// objects so callers can build envelopes naturally; the JCS encoder
// re-sorts keys at emit time.

const std = @import("std");

pub const ValueKind = enum {
    null_,
    bool_,
    integer,
    string,
    array,
    object,
};

/// Owned (key, value) pair for an Object Value. Values are reference-counted-
/// like via pointer ownership: the `Value.deinit()` recursively frees children.
pub const Pair = struct {
    key: []u8,
    value: *Value,
};

pub const Value = union(ValueKind) {
    null_: void,
    bool_: bool,
    integer: i64,
    string: []u8,
    array: []*Value,
    object: []Pair,

    /// Recursively free this Value and all children allocated through `alloc`.
    /// `alloc` MUST be the same allocator that built the tree.
    pub fn deinit(self: *Value, alloc: std.mem.Allocator) void {
        switch (self.*) {
            .null_, .bool_, .integer => {},
            .string => |s| alloc.free(s),
            .array => |items| {
                for (items) |child| {
                    child.deinit(alloc);
                    alloc.destroy(child);
                }
                alloc.free(items);
            },
            .object => |entries| {
                for (entries) |pair| {
                    alloc.free(pair.key);
                    pair.value.deinit(alloc);
                    alloc.destroy(pair.value);
                }
                alloc.free(entries);
            },
        }
    }

    // ---- constructors -----------------------------------------------------

    pub fn newNull(alloc: std.mem.Allocator) !*Value {
        const v = try alloc.create(Value);
        v.* = .{ .null_ = {} };
        return v;
    }

    pub fn newBool(alloc: std.mem.Allocator, b: bool) !*Value {
        const v = try alloc.create(Value);
        v.* = .{ .bool_ = b };
        return v;
    }

    pub fn newInt(alloc: std.mem.Allocator, n: i64) !*Value {
        const v = try alloc.create(Value);
        v.* = .{ .integer = n };
        return v;
    }

    pub fn newString(alloc: std.mem.Allocator, s: []const u8) !*Value {
        const owned = try alloc.dupe(u8, s);
        const v = try alloc.create(Value);
        v.* = .{ .string = owned };
        return v;
    }

    /// Build an Array node taking ownership of `items` (slice of *Value).
    /// `items` must have been allocated through the same `alloc`.
    pub fn newArrayOwned(alloc: std.mem.Allocator, items: []*Value) !*Value {
        const v = try alloc.create(Value);
        v.* = .{ .array = items };
        return v;
    }

    /// Build an Object node taking ownership of `entries`. Keys are
    /// expected to already be `alloc.dupe`'d.
    pub fn newObjectOwned(alloc: std.mem.Allocator, entries: []Pair) !*Value {
        const v = try alloc.create(Value);
        v.* = .{ .object = entries };
        return v;
    }
};

/// Object builder: collects (key, value) pairs in insertion order, then
/// transfers ownership to a freshly-built Object Value. Keys are dupe'd
/// on `add`; values are taken by pointer (the builder does not copy them).
pub const ObjectBuilder = struct {
    alloc: std.mem.Allocator,
    pairs: std.ArrayList(Pair) = .empty,

    pub fn init(alloc: std.mem.Allocator) ObjectBuilder {
        return .{ .alloc = alloc };
    }

    pub fn add(self: *ObjectBuilder, key: []const u8, value: *Value) !void {
        const owned_key = try self.alloc.dupe(u8, key);
        try self.pairs.append(self.alloc, .{ .key = owned_key, .value = value });
    }

    /// Finalize. The returned *Value owns the entries slice and all keys/
    /// child Values transitively.
    pub fn finish(self: *ObjectBuilder) !*Value {
        const entries = try self.pairs.toOwnedSlice(self.alloc);
        return Value.newObjectOwned(self.alloc, entries);
    }
};

/// Array builder: same shape as ObjectBuilder.
pub const ArrayBuilder = struct {
    alloc: std.mem.Allocator,
    items: std.ArrayList(*Value) = .empty,

    pub fn init(alloc: std.mem.Allocator) ArrayBuilder {
        return .{ .alloc = alloc };
    }

    pub fn append(self: *ArrayBuilder, v: *Value) !void {
        try self.items.append(self.alloc, v);
    }

    pub fn finish(self: *ArrayBuilder) !*Value {
        const slice = try self.items.toOwnedSlice(self.alloc);
        return Value.newArrayOwned(self.alloc, slice);
    }
};

// ---------------------------------------------------------------------------
// Encoder
// ---------------------------------------------------------------------------

/// Encode `v` as JCS-canonical JSON. Returned slice is owned by the caller.
pub fn encode(alloc: std.mem.Allocator, v: *const Value) ![]u8 {
    var buf: std.ArrayList(u8) = .empty;
    defer buf.deinit(alloc);
    try encodeValue(alloc, &buf, v);
    return buf.toOwnedSlice(alloc);
}

fn encodeValue(alloc: std.mem.Allocator, out: *std.ArrayList(u8), v: *const Value) !void {
    switch (v.*) {
        .null_ => try out.appendSlice(alloc, "null"),
        .bool_ => |b| try out.appendSlice(alloc, if (b) "true" else "false"),
        .integer => |n| {
            // ECMA-262 ToString applied to a finite integer; std.fmt's `{d}`
            // emits the same form (no leading zeros, leading minus for
            // negatives, no trailing decimal).
            try out.print(alloc, "{d}", .{n});
        },
        .string => |s| try encodeString(alloc, out, s),
        .array => |items| {
            try out.append(alloc, '[');
            for (items, 0..) |child, i| {
                if (i > 0) try out.append(alloc, ',');
                try encodeValue(alloc, out, child);
            }
            try out.append(alloc, ']');
        },
        .object => |entries| {
            // Sort by key (byte-order works for ASCII keys; the protocol's
            // keys are all ASCII). For non-ASCII keys this is still the
            // right answer because UTF-8 byte order matches Unicode
            // code-point order on well-formed input.
            const sorted = try alloc.alloc(Pair, entries.len);
            defer alloc.free(sorted);
            @memcpy(sorted, entries);
            std.mem.sort(Pair, sorted, {}, pairLess);

            try out.append(alloc, '{');
            for (sorted, 0..) |pair, i| {
                if (i > 0) try out.append(alloc, ',');
                try encodeString(alloc, out, pair.key);
                try out.append(alloc, ':');
                try encodeValue(alloc, out, pair.value);
            }
            try out.append(alloc, '}');
        },
    }
}

fn pairLess(_: void, a: Pair, b: Pair) bool {
    return std.mem.lessThan(u8, a.key, b.key);
}

fn encodeString(alloc: std.mem.Allocator, out: *std.ArrayList(u8), s: []const u8) !void {
    try out.append(alloc, '"');
    // Iterate by byte: non-ASCII bytes (>= 0x80) are emitted verbatim,
    // preserving the input's UTF-8 encoding. The Rust peer iterates by
    // Unicode scalar; the result is identical for well-formed UTF-8 input
    // because we only special-case ASCII-range characters.
    for (s) |b| {
        if (b == '"') {
            try out.appendSlice(alloc, "\\\"");
        } else if (b == '\\') {
            try out.appendSlice(alloc, "\\\\");
        } else if (b < 0x20) {
            const hex = "0123456789abcdef";
            try out.appendSlice(alloc, "\\u00");
            try out.append(alloc, hex[(b >> 4) & 0xF]);
            try out.append(alloc, hex[b & 0xF]);
        } else {
            try out.append(alloc, b);
        }
    }
    try out.append(alloc, '"');
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

test "encode_simple_object_sorts_keys" {
    const alloc = std.testing.allocator;
    var ob = ObjectBuilder.init(alloc);
    try ob.add("b", try Value.newInt(alloc, 1));
    try ob.add("a", try Value.newString(alloc, "x"));
    const v = try ob.finish();
    defer {
        v.deinit(alloc);
        alloc.destroy(v);
    }
    const enc = try encode(alloc, v);
    defer alloc.free(enc);
    try std.testing.expectEqualStrings("{\"a\":\"x\",\"b\":1}", enc);
}

test "encode_nested_array_object" {
    const alloc = std.testing.allocator;
    var arr = ArrayBuilder.init(alloc);
    try arr.append(try Value.newInt(alloc, 1));
    try arr.append(try Value.newInt(alloc, 2));
    var ob = ObjectBuilder.init(alloc);
    try ob.add("xs", try arr.finish());
    const v = try ob.finish();
    defer {
        v.deinit(alloc);
        alloc.destroy(v);
    }
    const enc = try encode(alloc, v);
    defer alloc.free(enc);
    try std.testing.expectEqualStrings("{\"xs\":[1,2]}", enc);
}

test "escape_quotes_and_backslash" {
    const alloc = std.testing.allocator;
    const v = try Value.newString(alloc, "a\"b\\c");
    defer {
        v.deinit(alloc);
        alloc.destroy(v);
    }
    const enc = try encode(alloc, v);
    defer alloc.free(enc);
    try std.testing.expectEqualStrings("\"a\\\"b\\\\c\"", enc);
}

test "empty_object_and_array" {
    const alloc = std.testing.allocator;
    {
        var ob = ObjectBuilder.init(alloc);
        const v = try ob.finish();
        defer {
            v.deinit(alloc);
            alloc.destroy(v);
        }
        const enc = try encode(alloc, v);
        defer alloc.free(enc);
        try std.testing.expectEqualStrings("{}", enc);
    }
    {
        var arr = ArrayBuilder.init(alloc);
        const v = try arr.finish();
        defer {
            v.deinit(alloc);
            alloc.destroy(v);
        }
        const enc = try encode(alloc, v);
        defer alloc.free(enc);
        try std.testing.expectEqualStrings("[]", enc);
    }
}

test "unicode_atomic_predicates_round_trip_verbatim" {
    // Regression: cross-language hash agreement depends on this. The kit's
    // atomic predicate names use exactly these (>=, <=, !=). The Rust JCS
    // emitter preserves these chars verbatim (UTF-8 bytes >= 0x80 emitted
    // as-is); we match.
    const alloc = std.testing.allocator;
    const inputs = [_][]const u8{ "\u{2265}", "\u{2264}", "\u{2260}" };
    for (inputs) |sym| {
        const v = try Value.newString(alloc, sym);
        defer {
            v.deinit(alloc);
            alloc.destroy(v);
        }
        const enc = try encode(alloc, v);
        defer alloc.free(enc);
        // Encoded form is `"<sym>"`: the same UTF-8 bytes wrapped in quotes.
        try std.testing.expectEqual(@as(usize, sym.len + 2), enc.len);
        try std.testing.expectEqual(@as(u8, '"'), enc[0]);
        try std.testing.expectEqual(@as(u8, '"'), enc[enc.len - 1]);
        try std.testing.expectEqualSlices(u8, sym, enc[1 .. enc.len - 1]);
    }
}

test "negative_integer_serializes_with_leading_minus" {
    const alloc = std.testing.allocator;
    const v = try Value.newInt(alloc, -42);
    defer {
        v.deinit(alloc);
        alloc.destroy(v);
    }
    const enc = try encode(alloc, v);
    defer alloc.free(enc);
    try std.testing.expectEqualStrings("-42", enc);
}

test "control_char_escaped_lowercase_hex" {
    const alloc = std.testing.allocator;
    const v = try Value.newString(alloc, "\x1f");
    defer {
        v.deinit(alloc);
        alloc.destroy(v);
    }
    const enc = try encode(alloc, v);
    defer alloc.free(enc);
    try std.testing.expectEqualStrings("\"\\u001f\"", enc);
}

test "object_key_sort_is_byte_order" {
    // Per RFC 8785 §3.2.3: sort by Unicode code-point. For ASCII keys this
    // is byte-order; "Z" (0x5A) sorts before "a" (0x61).
    const alloc = std.testing.allocator;
    var ob = ObjectBuilder.init(alloc);
    try ob.add("a", try Value.newInt(alloc, 1));
    try ob.add("Z", try Value.newInt(alloc, 2));
    const v = try ob.finish();
    defer {
        v.deinit(alloc);
        alloc.destroy(v);
    }
    const enc = try encode(alloc, v);
    defer alloc.free(enc);
    try std.testing.expectEqualStrings("{\"Z\":2,\"a\":1}", enc);
}
