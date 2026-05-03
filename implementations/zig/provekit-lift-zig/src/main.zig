const std = @import("std");
const lift = @import("provekit-lift-zig");
const provekit = @import("provekit-ir");

// ProvekIt Lift Tool for Zig
//
// Scans Zig source files for provekit annotations and emits JCS canonical IR.
//
// Usage:
//   provekit-lift-zig --workspace ./src --output ./target/provekit/
//   provekit-lift-zig --rpc              (NDJSON JSON-RPC plugin mode)

const Io = std.Io;
const Dir = std.Io.Dir;

// Buffer sizes for stdin/stdout.  Must fit the largest expected JSON line.
const READ_BUF = 256 * 1024;
const WRITE_BUF = 256 * 1024;

fn runRpcMode(alloc: std.mem.Allocator, io: Io) !void {
    var read_buf: [READ_BUF]u8 = undefined;
    var write_buf: [WRITE_BUF]u8 = undefined;

    var stdin_file = Io.File.stdin().readerStreaming(io, &read_buf);
    var stdin_reader = &stdin_file.interface;

    var stdout_file = Io.File.stdout().writerStreaming(io, &write_buf);
    var stdout_writer = &stdout_file.interface;

    while (true) {
        const maybe_line = stdin_reader.takeDelimiter('\n') catch |err| switch (err) {
            error.StreamTooLong => {
                _ = stdin_reader.discardDelimiterInclusive('\n') catch break;
                continue;
            },
            error.ReadFailed => break,
        };
        const line = maybe_line orelse break;
        // Discard the trailing newline byte that takeDelimiter left in buffer.
        stdin_reader.toss(@min(1, stdin_reader.bufferedLen()));

        const keep_going = try handleLine(alloc, line, stdout_writer);
        try stdout_writer.flush();
        if (!keep_going) break;
    }
}

fn handleLine(
    alloc: std.mem.Allocator,
    line: []const u8,
    writer: *Io.Writer,
) !bool {
    const id = extractId(line);

    if (std.mem.indexOf(u8, line, "\"initialize\"") != null) {
        try writer.print(
            "{{\"jsonrpc\":\"2.0\",\"id\":{s},\"result\":{{\"name\":\"provekit-lift-zig\",\"version\":\"0.1.0\",\"capabilities\":[\"parse\"]}}}}\n",
            .{id},
        );
        return true;
    }

    if (std.mem.indexOf(u8, line, "\"parse\"") != null) {
        try handleParse(alloc, line, id, writer);
        return true;
    }

    if (std.mem.indexOf(u8, line, "\"shutdown\"") != null) {
        try writer.print(
            "{{\"jsonrpc\":\"2.0\",\"id\":{s},\"result\":null}}\n",
            .{id},
        );
        return false;
    }

    // Unknown method.
    try writer.print(
        "{{\"jsonrpc\":\"2.0\",\"id\":{s},\"error\":{{\"code\":-32601,\"message\":\"unknown method\"}}}}\n",
        .{id},
    );
    return true;
}

fn handleParse(
    alloc: std.mem.Allocator,
    line: []const u8,
    id: []const u8,
    writer: *Io.Writer,
) !void {
    // Extract "source" field value — naive string extraction.
    const source_raw = extractJsonStringField(line, "source") orelse "";
    const source = try unescapeJsonString(alloc, source_raw);
    defer alloc.free(source);

    // Lift to IR declarations via the lift module.
    const decls = try lift.liftToDecls(alloc, source);
    defer alloc.free(decls);

    // Serialize declarations as a JSON array.
    const decls_json = try std.json.Stringify.valueAlloc(alloc, decls, .{ .whitespace = .minified });
    defer alloc.free(decls_json);

    // callEdges: zig kit emits empty array (no cross-kit call tracking yet).
    // warnings: empty.
    try writer.print(
        "{{\"jsonrpc\":\"2.0\",\"id\":{s},\"result\":{{\"declarations\":{s},\"callEdges\":[],\"warnings\":[]}}}}\n",
        .{ id, decls_json },
    );
}

fn runStandaloneMode(alloc: std.mem.Allocator, io: Io, workspace_path: []const u8, output_path: []const u8) !void {
    var decls: std.ArrayList(provekit.Decl) = .empty;
    defer decls.deinit(alloc);

    var dir = try Dir.openDir(Dir.cwd(), io, workspace_path, .{ .iterate = true });
    defer dir.close(io);

    var walker = try dir.walk(alloc);
    defer walker.deinit();

    while (try walker.next(io)) |entry| {
        if (entry.kind != .file) continue;
        if (!std.mem.endsWith(u8, entry.basename, ".zig")) continue;

        const file_text = try Dir.readFileAlloc(entry.dir, io, entry.basename, alloc, .unlimited);
        defer alloc.free(file_text);

        const file_decls = try lift.liftToDecls(alloc, file_text);
        defer alloc.free(file_decls);

        for (file_decls) |decl| {
            try decls.append(alloc, decl);
        }
    }

    const decls_slice = try decls.toOwnedSlice(alloc);
    defer alloc.free(decls_slice);

    const jcs = try provekit.jcsStringify(alloc, decls_slice);
    defer alloc.free(jcs);

    try Dir.createDirPath(Dir.cwd(), io, output_path);
    const out_path = try std.fs.path.join(alloc, &.{ output_path, "lifted.json" });
    defer alloc.free(out_path);
    const out_file = try Dir.createFile(Dir.cwd(), io, out_path, .{});
    defer out_file.close(io);
    try Io.File.writeStreamingAll(out_file, io, jcs);

    std.debug.print("Wrote {d} declarations to {s}\n", .{ decls_slice.len, out_path });
}

/// Extract the raw JSON "id" value token from a NDJSON line.
/// Returns "null" if not found.
fn extractId(line: []const u8) []const u8 {
    const key = "\"id\":";
    const pos = std.mem.indexOf(u8, line, key) orelse return "null";
    const after = std.mem.trimStart(u8, line[pos + key.len ..], " \t");
    const end = std.mem.indexOfAny(u8, after, ",}") orelse after.len;
    return after[0..end];
}

/// Extract a JSON string field value (the raw escaped string contents between
/// the outer quotes) from a JSON line.  Returns null if not found.
fn extractJsonStringField(line: []const u8, field: []const u8) ?[]const u8 {
    var buf: [64]u8 = undefined;
    if (field.len + 3 > buf.len) return null;
    const key = std.fmt.bufPrint(&buf, "\"{s}\":", .{field}) catch return null;
    const pos = std.mem.indexOf(u8, line, key) orelse return null;
    const after = std.mem.trimStart(u8, line[pos + key.len ..], " \t");
    if (after.len == 0 or after[0] != '"') return null;
    const content = after[1..]; // skip opening quote
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

/// Unescape a JSON string's raw content (the part between the outer quotes).
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

pub fn main(init: std.process.Init) !void {
    const alloc = init.gpa;
    const io = init.io;

    const args = try init.minimal.args.toSlice(init.arena.allocator());

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
        try runRpcMode(alloc, io);
    } else {
        try runStandaloneMode(alloc, io, workspace orelse ".", output orelse "./target/provekit");
    }
}
