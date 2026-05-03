// provekit-lift-zig/src/lift.zig
//
// Pure parsing logic — no IO.  Imported by provekit-lift-zig (the CLI binary)
// and by provekit-lsp-zig (the LSP plugin).

const std = @import("std");
const provekit = @import("provekit-ir");

pub const Annotation = struct {
    function_name: []const u8,
    kind: Kind,
    target_cid: ?[]const u8 = null,
    line: usize,

    pub const Kind = enum {
        contract,
        implement,
        verify,
    };
};

/// Parse provekit annotations from Zig source text.
/// Caller owns the returned slice; call `alloc.free(slice)` when done.
pub fn parseAnnotations(alloc: std.mem.Allocator, text: []const u8) ![]Annotation {
    var annotations: std.ArrayList(Annotation) = .empty;
    errdefer annotations.deinit(alloc);

    var lines = std.mem.splitScalar(u8, text, '\n');
    var line_num: usize = 0;
    while (lines.next()) |line| : (line_num += 1) {
        const trimmed = std.mem.trim(u8, line, " \t");

        if (std.mem.startsWith(u8, trimmed, "//provekit:implement ")) {
            const cid = std.mem.trim(u8, trimmed[20..], " \t");
            const fn_name = findAheadFnName(text, line_num);
            try annotations.append(alloc, .{
                .function_name = fn_name,
                .kind = .implement,
                .target_cid = cid,
                .line = line_num,
            });
        } else if (std.mem.startsWith(u8, trimmed, "//provekit:contract")) {
            const fn_name = findAheadFnName(text, line_num);
            try annotations.append(alloc, .{
                .function_name = fn_name,
                .kind = .contract,
                .line = line_num,
            });
        } else if (std.mem.startsWith(u8, trimmed, "//provekit:verify")) {
            const fn_name = findAheadFnName(text, line_num);
            try annotations.append(alloc, .{
                .function_name = fn_name,
                .kind = .verify,
                .line = line_num,
            });
        }
    }

    return annotations.toOwnedSlice(alloc);
}

fn findAheadFnName(text: []const u8, start_line: usize) []const u8 {
    var lines = std.mem.splitScalar(u8, text, '\n');
    var current: usize = 0;
    while (lines.next()) |line| : (current += 1) {
        if (current <= start_line) continue;
        if (current > start_line + 10) break;

        const trimmed = std.mem.trim(u8, line, " \t");
        if (std.mem.startsWith(u8, trimmed, "fn ")) {
            const after = trimmed[3..];
            const end = std.mem.indexOfAny(u8, after, " (\n") orelse after.len;
            return after[0..end];
        }
    }
    return "unknown";
}

/// Lift annotations from source text into a slice of provekit IR declarations.
/// Caller owns the returned slice.
pub fn liftToDecls(alloc: std.mem.Allocator, text: []const u8) ![]provekit.Decl {
    const anns = try parseAnnotations(alloc, text);
    defer alloc.free(anns);

    var decls: std.ArrayList(provekit.Decl) = .empty;
    errdefer decls.deinit(alloc);

    for (anns) |ann| {
        switch (ann.kind) {
            .contract => {
                const post_args = [_]provekit.Term{};
                const post = provekit.Atomic("true", &post_args);
                try decls.append(alloc, .{ .contract = .{
                    .name = ann.function_name,
                    .post = post,
                } });
            },
            .implement => {
                if (ann.target_cid) |cid| {
                    try decls.append(alloc, .{ .bridge = .{
                        .name = ann.function_name,
                        .source_symbol = ann.function_name,
                        .source_layer = "zig",
                        .source_contract_cid = "",
                        .target_contract_cid = cid,
                        .target_proof_cid = "",
                        .target_layer = "rust",
                    } });
                }
            },
            .verify => {},
        }
    }

    return decls.toOwnedSlice(alloc);
}

test "parseAnnotations finds contract" {
    const alloc = std.testing.allocator;
    const src =
        \\//provekit:contract
        \\fn myFn(x: i32) void {
        \\    _ = x;
        \\}
    ;
    const anns = try parseAnnotations(alloc, src);
    defer alloc.free(anns);
    try std.testing.expectEqual(@as(usize, 1), anns.len);
    try std.testing.expectEqual(Annotation.Kind.contract, anns[0].kind);
    try std.testing.expectEqualStrings("myFn", anns[0].function_name);
}

test "parseAnnotations finds implement" {
    const alloc = std.testing.allocator;
    const src =
        \\//provekit:implement blake3-512:abc123
        \\fn bridge(x: i32) void {
        \\    _ = x;
        \\}
    ;
    const anns = try parseAnnotations(alloc, src);
    defer alloc.free(anns);
    try std.testing.expectEqual(@as(usize, 1), anns.len);
    try std.testing.expectEqual(Annotation.Kind.implement, anns[0].kind);
    try std.testing.expectEqualStrings("blake3-512:abc123", anns[0].target_cid.?);
}

test "parseAnnotations finds verify" {
    const alloc = std.testing.allocator;
    const src =
        \\//provekit:verify
        \\fn checkFn() void {}
    ;
    const anns = try parseAnnotations(alloc, src);
    defer alloc.free(anns);
    try std.testing.expectEqual(@as(usize, 1), anns.len);
    try std.testing.expectEqual(Annotation.Kind.verify, anns[0].kind);
    try std.testing.expectEqualStrings("checkFn", anns[0].function_name);
}

test "parseAnnotations empty source" {
    const alloc = std.testing.allocator;
    const anns = try parseAnnotations(alloc, "");
    defer alloc.free(anns);
    try std.testing.expectEqual(@as(usize, 0), anns.len);
}

test "liftToDecls contract produces IR" {
    const alloc = std.testing.allocator;
    const src =
        \\//provekit:contract
        \\fn myFn() void {}
    ;
    const decls = try liftToDecls(alloc, src);
    defer alloc.free(decls);
    try std.testing.expectEqual(@as(usize, 1), decls.len);
    switch (decls[0]) {
        .contract => |c| try std.testing.expectEqualStrings("myFn", c.name),
        else => return error.WrongKind,
    }
}
