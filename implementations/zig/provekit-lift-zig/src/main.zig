const std = @import("std");
const provekit = @import("provekit-ir");

// ProvekIt Lift Tool for Zig
//
// Scans Zig source files for provekit annotations and emits JCS canonical IR.
//
// Usage:
//   provekit-lift-zig --workspace ./src --output ./target/provekit/
//   provekit-lift-zig --rpc              (NDJSON JSON-RPC plugin mode)

const Annotation = struct {
    function_name: []const u8,
    kind: Kind,
    target_cid: ?[]const u8 = null,
    line: usize,

    const Kind = enum {
        contract,
        implement,
        verify,
    };
};

fn parseAnnotations(alloc: std.mem.Allocator, text: []const u8) ![]Annotation {
    var annotations = std.ArrayList(Annotation).init(alloc);
    errdefer annotations.deinit();

    var lines = std.mem.splitScalar(u8, text, '\n');
    var line_num: usize = 0;
    while (lines.next()) |line| : (line_num += 1) {
        const trimmed = std.mem.trim(u8, line, " \t");

        if (std.mem.startsWith(u8, trimmed, "//provekit:implement ")) {
            const cid = std.mem.trim(u8, trimmed[20..], " \t");
            const fn_name = findAheadFnName(text, line_num);
            try annotations.append(.{
                .function_name = fn_name,
                .kind = .implement,
                .target_cid = cid,
                .line = line_num,
            });
        } else if (std.mem.startsWith(u8, trimmed, "//provekit:contract")) {
            const fn_name = findAheadFnName(text, line_num);
            try annotations.append(.{
                .function_name = fn_name,
                .kind = .contract,
                .line = line_num,
            });
        } else if (std.mem.startsWith(u8, trimmed, "//provekit:verify")) {
            const fn_name = findAheadFnName(text, line_num);
            try annotations.append(.{
                .function_name = fn_name,
                .kind = .verify,
                .line = line_num,
            });
        }
    }

    return annotations.toOwnedSlice();
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

fn runRpcMode(alloc: std.mem.Allocator) !void {
    const stdin = std.io.getStdIn().reader();
    const stdout = std.io.getStdOut().writer();
    var buf: [4096]u8 = undefined;

    while (true) {
        const maybe_line = try stdin.readUntilDelimiterOrEof(&buf, '\n');
        const line = maybe_line orelse break;

        const has_init = std.mem.indexOf(u8, line, "\"initialize\"") != null;
        const has_parse = std.mem.indexOf(u8, line, "\"parse\"") != null;
        const has_shutdown = std.mem.indexOf(u8, line, "\"shutdown\"") != null;

        var id: []const u8 = "null";
        if (std.mem.indexOf(u8, line, "\"id\":")) |id_pos| {
            const after = line[id_pos + 5 ..];
            const start = std.mem.indexOfNone(u8, after, " \t") orelse 0;
            const end = std.mem.indexOfAny(u8, after[start..], ",}") orelse after.len;
            id = after[start .. start + end];
        }

        if (has_init) {
            try stdout.print("{{\"jsonrpc\":\"2.0\",\"id\":{s},\"result\":{{\"name\":\"provekit-lift-zig\",\"version\":\"0.1.0\",\"capabilities\":[\"parse\"]}}}}\n", .{id});
        } else if (has_parse) {
            var decls = std.ArrayList(provekit.Decl).init(alloc);
            defer decls.deinit();

            // Minimal: emit a placeholder contract for each annotation found
            // in the current directory. Real implementation would parse params.
            var dir = std.fs.cwd();
            var walker = try dir.walk(alloc);
            defer walker.deinit();
            while (try walker.next()) |entry| {
                if (entry.kind != .file) continue;
                if (!std.mem.endsWith(u8, entry.basename, ".zig")) continue;

                const file_text = try entry.dir.readFileAlloc(alloc, entry.basename, 1 << 20);
                defer alloc.free(file_text);

                const anns = try parseAnnotations(alloc, file_text);
                defer alloc.free(anns);

                for (anns) |ann| {
                    switch (ann.kind) {
                        .contract => {
                            const post_args = [_]provekit.Term{};
                            const post = provekit.Atomic("true", &post_args);
                            try decls.append(.{ .contract = .{
                                .name = ann.function_name,
                                .post = post,
                            } });
                        },
                        .implement => {
                            if (ann.target_cid) |cid| {
                                try decls.append(.{ .bridge = .{
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
            }

            const decls_slice = try decls.toOwnedSlice();
            defer alloc.free(decls_slice);

            var json_buf = std.ArrayList(u8).init(alloc);
            defer json_buf.deinit();
            try std.json.stringify(decls_slice, .{ .whitespace = .minified }, json_buf.writer());

            try stdout.print("{{\"jsonrpc\":\"2.0\",\"id\":{s},\"result\":{{\"declarations\":{s},\"warnings\":[]}}}}\n", .{ id, json_buf.items });
        } else if (has_shutdown) {
            try stdout.print("{{\"jsonrpc\":\"2.0\",\"id\":{s},\"result\":null}}\n", .{id});
            return;
        } else {
            try stdout.print("{{\"jsonrpc\":\"2.0\",\"id\":{s},\"error\":{{\"code\":-32601,\"message\":\"unknown method\"}}}}\n", .{id});
        }
    }
}

fn runStandaloneMode(alloc: std.mem.Allocator, workspace_path: []const u8, output_path: []const u8) !void {
    var decls = std.ArrayList(provekit.Decl).init(alloc);
    defer decls.deinit();

    var dir = try std.fs.cwd().openDir(workspace_path, .{ .iterate = true });
    defer dir.close();

    var walker = try dir.walk(alloc);
    defer walker.deinit();

    while (try walker.next()) |entry| {
        if (entry.kind != .file) continue;
        if (!std.mem.endsWith(u8, entry.basename, ".zig")) continue;

        const file_text = try entry.dir.readFileAlloc(alloc, entry.basename, 1 << 20);
        defer alloc.free(file_text);

        const anns = try parseAnnotations(alloc, file_text);
        defer alloc.free(anns);

        for (anns) |ann| {
            switch (ann.kind) {
                .contract => {
                    const post_args = [_]provekit.Term{};
                    const post = provekit.Atomic("true", &post_args);
                    try decls.append(.{ .contract = .{
                        .name = ann.function_name,
                        .post = post,
                    } });
                },
                .implement => {
                    if (ann.target_cid) |cid| {
                        try decls.append(.{ .bridge = .{
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
    }

    const decls_slice = try decls.toOwnedSlice();
    defer alloc.free(decls_slice);

    const jcs = try provekit.jcsStringify(alloc, decls_slice);
    defer alloc.free(jcs);

    try std.fs.cwd().makePath(output_path);
    const out_path = try std.fs.path.join(alloc, &.{ output_path, "lifted.json" });
    defer alloc.free(out_path);
    const out_file = try std.fs.cwd().createFile(out_path, .{});
    defer out_file.close();
    try out_file.writeAll(jcs);

    std.debug.print("Wrote {d} declarations to {s}\n", .{ decls_slice.len, out_path });
}

pub fn main() !void {
    var gpa = std.heap.GeneralPurposeAllocator(.{}){};
    defer _ = gpa.deinit();
    const alloc = gpa.allocator();

    const args = try std.process.argsAlloc(alloc);
    defer std.process.argsFree(alloc, args);

    var rpc_mode = false;
    var workspace: ?[]const u8 = null;
    var output: ?[]const u8 = null;

    var i: usize = 1;
    while (i < args.len) : (i += 1) {
        const arg = args[i];
        if (std.mem.eql(u8, arg, "--rpc")) {
            rpc_mode = true;
        } else if (std.mem.eql(u8, arg, "--workspace")) {
            i += 1;
            if (i < args.len) workspace = args[i];
        } else if (std.mem.eql(u8, arg, "--output")) {
            i += 1;
            if (i < args.len) output = args[i];
        }
    }

    if (rpc_mode) {
        try runRpcMode(alloc);
    } else {
        try runStandaloneMode(alloc, workspace orelse ".", output orelse "./target/provekit");
    }
}
