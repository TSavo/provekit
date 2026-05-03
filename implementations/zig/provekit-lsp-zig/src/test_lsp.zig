// Integration tests for the provekit-lsp-zig wire protocol.
//
// Tests drive the handleLine logic directly (without spawning a subprocess)
// by calling the same parsing helpers used in main.zig.

const std = @import("std");
const lift = @import("provekit-lift-zig");

// ---------------------------------------------------------------------------
// Helpers re-exported from main.zig logic (duplicated for test isolation).
// These must stay in sync with main.zig.
// ---------------------------------------------------------------------------

fn extractId(line: []const u8) []const u8 {
    const key = "\"id\":";
    const pos = std.mem.indexOf(u8, line, key) orelse return "null";
    const after = std.mem.trimStart(u8, line[pos + key.len ..], " \t");
    const end = std.mem.indexOfAny(u8, after, ",}") orelse after.len;
    return after[0..end];
}

fn extractJsonStringField(line: []const u8, field: []const u8) ?[]const u8 {
    var buf: [64]u8 = undefined;
    if (field.len + 3 > buf.len) return null;
    const key = std.fmt.bufPrint(&buf, "\"{s}\":", .{field}) catch return null;
    const pos = std.mem.indexOf(u8, line, key) orelse return null;
    const after = std.mem.trimStart(u8, line[pos + key.len ..], " \t");
    if (after.len == 0 or after[0] != '"') return null;
    const content = after[1..];
    var i: usize = 0;
    while (i < content.len) {
        if (content[i] == '\\') {
            i += 2;
        } else if (content[i] == '"') {
            return content[0..i];
        } else {
            i += 1;
        }
    }
    return null;
}

fn unescapeJsonString(alloc: std.mem.Allocator, raw: []const u8) ![]u8 {
    var out: std.ArrayList(u8) = .empty;
    errdefer out.deinit(alloc);
    var i: usize = 0;
    while (i < raw.len) {
        if (raw[i] == '\\' and i + 1 < raw.len) {
            switch (raw[i + 1]) {
                'n' => { try out.append(alloc, '\n'); i += 2; },
                't' => { try out.append(alloc, '\t'); i += 2; },
                'r' => { try out.append(alloc, '\r'); i += 2; },
                '"' => { try out.append(alloc, '"'); i += 2; },
                '\\' => { try out.append(alloc, '\\'); i += 2; },
                '/' => { try out.append(alloc, '/'); i += 2; },
                else => { try out.append(alloc, raw[i]); i += 1; },
            }
        } else {
            try out.append(alloc, raw[i]);
            i += 1;
        }
    }
    return out.toOwnedSlice(alloc);
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

test "extractId numeric id" {
    const line =
        \\{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}
    ;
    try std.testing.expectEqualStrings("1", extractId(line));
}

test "extractId string id" {
    const line =
        \\{"jsonrpc":"2.0","id":"abc","method":"parse","params":{}}
    ;
    try std.testing.expectEqualStrings("\"abc\"", extractId(line));
}

test "extractId null when missing" {
    const line =
        \\{"jsonrpc":"2.0","method":"shutdown"}
    ;
    try std.testing.expectEqualStrings("null", extractId(line));
}

test "extractJsonStringField source" {
    const line =
        \\{"jsonrpc":"2.0","id":1,"method":"parse","params":{"path":"foo.zig","source":"//provekit:contract\nfn myFn() void {}"}}
    ;
    const field = extractJsonStringField(line, "source");
    try std.testing.expect(field != null);
    // Should contain the escaped content
    try std.testing.expect(std.mem.indexOf(u8, field.?, "//provekit:contract") != null);
}

test "unescapeJsonString basic escapes" {
    const alloc = std.testing.allocator;
    const raw = "hello\\nworld\\ttab\\\\backslash";
    const result = try unescapeJsonString(alloc, raw);
    defer alloc.free(result);
    try std.testing.expectEqualStrings("hello\nworld\ttab\\backslash", result);
}

test "unescapeJsonString empty string" {
    const alloc = std.testing.allocator;
    const result = try unescapeJsonString(alloc, "");
    defer alloc.free(result);
    try std.testing.expectEqual(@as(usize, 0), result.len);
}

test "initialize response shape" {
    // Simulate the response for an initialize call.
    // This is a unit test of the format string, not actual IO.
    const alloc = std.testing.allocator;
    const id: []const u8 = "1";
    const response = try std.fmt.allocPrint(
        alloc,
        "{{\"jsonrpc\":\"2.0\",\"id\":{s},\"result\":{{\"name\":\"provekit-lsp-zig\",\"version\":\"0.1.0\",\"capabilities\":[\"parse\"]}}}}",
        .{id},
    );
    defer alloc.free(response);

    // Verify canonical fields are present.
    try std.testing.expect(std.mem.indexOf(u8, response, "\"name\":\"provekit-lsp-zig\"") != null);
    try std.testing.expect(std.mem.indexOf(u8, response, "\"version\":\"0.1.0\"") != null);
    try std.testing.expect(std.mem.indexOf(u8, response, "\"capabilities\":[\"parse\"]") != null);
}

test "parse response includes declarations callEdges warnings" {
    // Simulate parse of a fixture .zig file with a contract annotation.
    const alloc = std.testing.allocator;
    const fixture_source = "//provekit:contract\nfn myFn(x: i32) void { _ = x; }";

    const decls = try lift.liftToDecls(alloc, fixture_source);
    defer alloc.free(decls);

    try std.testing.expect(decls.len == 1);

    const decls_json = try std.json.Stringify.valueAlloc(alloc, decls, .{ .whitespace = .minified });
    defer alloc.free(decls_json);

    const response = try std.fmt.allocPrint(
        alloc,
        "{{\"jsonrpc\":\"2.0\",\"id\":2,\"result\":{{\"declarations\":{s},\"callEdges\":[],\"warnings\":[]}}}}",
        .{decls_json},
    );
    defer alloc.free(response);

    try std.testing.expect(std.mem.indexOf(u8, response, "\"declarations\"") != null);
    try std.testing.expect(std.mem.indexOf(u8, response, "\"callEdges\":[]") != null);
    try std.testing.expect(std.mem.indexOf(u8, response, "\"warnings\":[]") != null);
    try std.testing.expect(std.mem.indexOf(u8, response, "\"kind\":\"contract\"") != null);
}

test "parse empty source returns empty declarations" {
    const alloc = std.testing.allocator;

    const decls = try lift.liftToDecls(alloc, "");
    defer alloc.free(decls);

    try std.testing.expectEqual(@as(usize, 0), decls.len);

    const decls_json = try std.json.Stringify.valueAlloc(alloc, decls, .{ .whitespace = .minified });
    defer alloc.free(decls_json);

    try std.testing.expectEqualStrings("[]", decls_json);
}

test "parse fixture zig file with implement annotation" {
    const alloc = std.testing.allocator;
    const fixture =
        \\//provekit:implement blake3-512:deadbeef
        \\fn bridgeFn(x: i32) i32 {
        \\    return x;
        \\}
    ;

    const decls = try lift.liftToDecls(alloc, fixture);
    defer alloc.free(decls);

    try std.testing.expectEqual(@as(usize, 1), decls.len);
    switch (decls[0]) {
        .bridge => |b| {
            try std.testing.expectEqualStrings("bridgeFn", b.name);
            try std.testing.expectEqualStrings("blake3-512:deadbeef", b.target_contract_cid);
            try std.testing.expectEqualStrings("zig", b.source_layer);
        },
        else => return error.WrongKind,
    }
}

test "shutdown response is null result" {
    const alloc = std.testing.allocator;
    const id: []const u8 = "99";
    const response = try std.fmt.allocPrint(
        alloc,
        "{{\"jsonrpc\":\"2.0\",\"id\":{s},\"result\":null}}",
        .{id},
    );
    defer alloc.free(response);

    try std.testing.expect(std.mem.indexOf(u8, response, "\"result\":null") != null);
}
