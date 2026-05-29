const std = @import("std");
const lift = @import("provekit-lift-zig-tests");
const provekit = @import("provekit-ir");

// ProvekIt Zig test/implication lifter.
//
// Scans native `std.testing` unit tests and production callsites and emits
// canonical IR over the lift-plugin RPC seam. It intentionally does not expose
// a standalone IR-authoring mode.

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

        const keep_going = try handleLine(alloc, io, line, stdout_writer);
        try stdout_writer.flush();
        if (!keep_going) break;
    }
}

fn handleLine(
    alloc: std.mem.Allocator,
    io: Io,
    line: []const u8,
    writer: *Io.Writer,
) !bool {
    const id = extractId(line);

    if (std.mem.indexOf(u8, line, "\"initialize\"") != null) {
        try writer.print(
            "{{\"jsonrpc\":\"2.0\",\"id\":{s},\"result\":{{\"name\":\"provekit-lift-zig-tests\",\"version\":\"0.1.0\",\"protocol_version\":\"pep/1.7.0\",\"capabilities\":{{\"authoring_surfaces\":[\"zig-tests\",\"zig-implications\"],\"emits_signed_mementos\":false,\"ir_version\":\"v1.1.0\"}}}}}}\n",
            .{id},
        );
        return true;
    }

    if (std.mem.indexOf(u8, line, "\"lift\"") != null or std.mem.indexOf(u8, line, "\"provekit.plugin.lift_implications\"") != null) {
        try handleLift(alloc, io, line, id, writer);
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

fn handleLift(
    alloc: std.mem.Allocator,
    io: Io,
    line: []const u8,
    id: []const u8,
    writer: *Io.Writer,
) !void {
    const workspace_raw = extractJsonStringField(line, "workspace_root") orelse ".";
    const workspace = try unescapeJsonString(alloc, workspace_raw);
    defer alloc.free(workspace);

    var arena = std.heap.ArenaAllocator.init(alloc);
    defer arena.deinit();
    const arena_alloc = arena.allocator();

    var declarations: std.ArrayList(provekit.Decl) = .empty;
    var implications: std.ArrayList(lift.ImplicationDecl) = .empty;
    const implications_only =
        std.mem.indexOf(u8, line, "\"provekit.plugin.lift_implications\"") != null or
        std.mem.indexOf(u8, line, "\"surface\":\"zig-implications\"") != null or
        std.mem.indexOf(u8, line, "\"layer\":\"implications\"") != null;

    var dir = try Dir.openDir(Dir.cwd(), io, workspace, .{ .iterate = true });
    defer dir.close(io);

    var walker = try dir.walk(arena_alloc);
    defer walker.deinit();

    while (try walker.next(io)) |entry| {
        if (entry.kind != .file) continue;
        if (!std.mem.endsWith(u8, entry.basename, ".zig")) continue;

        const file_text = try Dir.readFileAlloc(entry.dir, io, entry.basename, arena_alloc, .unlimited);
        const lifted = try lift.liftSource(arena_alloc, file_text, entry.basename);
        if (!implications_only) {
            for (lifted.declarations) |decl| try declarations.append(arena_alloc, decl);
        }
        for (lifted.implications) |implication| try implications.append(arena_alloc, implication);
    }

    const declarations_slice = try declarations.toOwnedSlice(arena_alloc);
    const implications_slice = try implications.toOwnedSlice(arena_alloc);
    const decls_json = try std.json.Stringify.valueAlloc(alloc, declarations_slice, .{ .whitespace = .minified });
    defer alloc.free(decls_json);
    const implications_json = try std.json.Stringify.valueAlloc(alloc, implications_slice, .{ .whitespace = .minified });
    defer alloc.free(implications_json);

    try writer.print(
        "{{\"jsonrpc\":\"2.0\",\"id\":{s},\"result\":{{\"kind\":\"ir-document\",\"ir\":{s},\"implications\":{s},\"diagnostics\":[]}}}}\n",
        .{ id, decls_json, implications_json },
    );
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

    var i: usize = 1;
    while (i < args.len) : (i += 1) {
        const arg = args[i];
        if (std.mem.eql(u8, arg, "--rpc")) {
            rpc_mode = true;
        }
    }

    if (rpc_mode) {
        try runRpcMode(alloc, io);
    } else {
        std.debug.print("usage: provekit-lift-zig-tests --rpc\n", .{});
        std.process.exit(1);
    }
}
