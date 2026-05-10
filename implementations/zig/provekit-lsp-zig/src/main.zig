// provekit-lsp-zig: NDJSON LSP plugin for Zig.
//
// Protocol (canonical wire shape):
//   {"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}
//     -> {"jsonrpc":"2.0","id":1,"result":{"name":"provekit-lsp-zig","version":"0.1.0","capabilities":["parse"]}}
//
//   {"jsonrpc":"2.0","id":2,"method":"parse","params":{"path":"...","source":"..."}}
//     -> {"jsonrpc":"2.0","id":2,"result":{"declarations":[...],"callEdges":[],"warnings":[]}}
//
//   {"jsonrpc":"2.0","id":3,"method":"shutdown"}
//     -> {"jsonrpc":"2.0","id":3,"result":null}
//
// Usage: provekit-lsp-zig (reads from stdin, writes to stdout).
// Binary name expected by consumers: provekit-lsp-zig

const std = @import("std");
const lift = @import("provekit-lift-zig");
const provekit = @import("provekit-ir");

const Io = std.Io;

// Buffer sizes for stdin/stdout.  Must fit the largest expected JSON line.
// 256 KiB covers very large source files passed inline.
const READ_BUF = 256 * 1024;
const WRITE_BUF = 256 * 1024;

pub fn main(init: std.process.Init) !void {
    const io = init.io;
    const alloc = init.gpa;

    var read_buf: [READ_BUF]u8 = undefined;
    var write_buf: [WRITE_BUF]u8 = undefined;

    var stdin_file = Io.File.stdin().readerStreaming(io, &read_buf);
    var stdin_reader = &stdin_file.interface;

    var stdout_file = Io.File.stdout().writerStreaming(io, &write_buf);
    var stdout_writer = &stdout_file.interface;

    while (true) {
        // takeDelimiter returns null when stream ends with no remaining data.
        const maybe_line = stdin_reader.takeDelimiter('\n') catch |err| switch (err) {
            error.StreamTooLong => {
                // Line too long: discard and continue.
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

/// Process a single NDJSON line.  Returns false on shutdown.
fn handleLine(
    alloc: std.mem.Allocator,
    line: []const u8,
    writer: *Io.Writer,
) !bool {
    // Extract "id" value naively: we want the raw token (number or string).
    const id = extractId(line);

    if (std.mem.indexOf(u8, line, "\"initialize\"") != null) {
        try writer.print(
            "{{\"jsonrpc\":\"2.0\",\"id\":{s},\"result\":{{\"name\":\"provekit-lsp-zig\",\"version\":\"0.1.0\",\"capabilities\":[\"parse\"]}}}}\n",
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
    // Extract "source" field value: naive string extraction.
    // The source is a JSON string with escape sequences.  We unescape only the
    // common cases (\n, \t, \\, \") since we only need to scan for annotations.
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
    // Search for `"field":` followed by optional whitespace and a `"`.
    var buf: [64]u8 = undefined;
    if (field.len + 3 > buf.len) return null;
    const key = std.fmt.bufPrint(&buf, "\"{s}\":", .{field}) catch return null;
    const pos = std.mem.indexOf(u8, line, key) orelse return null;
    const after = std.mem.trimStart(u8, line[pos + key.len ..], " \t");
    if (after.len == 0 or after[0] != '"') return null;
    const content = after[1..]; // skip opening quote
    // Find closing unescaped quote.
    var i: usize = 0;
    while (i < content.len) {
        if (content[i] == '\\') {
            i += 2; // skip escape sequence
        } else if (content[i] == '"') {
            return content[0..i];
        } else {
            i += 1;
        }
    }
    return null;
}

/// Unescape a JSON string's raw content (the part between the outer quotes).
/// Handles \n, \t, \\, \", \r, \/.  Other escapes are left as-is.
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
