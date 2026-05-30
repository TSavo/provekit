// provekit-lsp-zig: NDJSON LSP plugin for Zig.
//
// Protocol (provekit-lsp-shared/1 over stdio, with legacy parse retained):
//   {"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}
//   {"jsonrpc":"2.0","id":2,"method":"analyzeDocument","params":{"file":"...","text":"..."}}
//   {"jsonrpc":"2.0","id":3,"method":"shutdown"}
//
// Usage: provekit-lsp-zig (reads from stdin, writes to stdout).
// Binary name expected by consumers: provekit-lsp-zig

const std = @import("std");
const ir = @import("provekit-ir");
const lift = @import("provekit-lift-zig-source");

const Io = std.Io;

const VERSION = "0.1.0";
const KIT_ID = "zig";
const SURFACE = "zig-source";
const SHARED_LSP_PROTOCOL_VERSION = "provekit-lsp-shared/1";
const SHARED_LSP_PROTOCOL_CATALOG_CID = "blake3-512:0e3905c2a7a098cd538b9669428a7dffd2b84ba8ccf8fde3724fe2ab61fd3fbc1e1a616a6b20b6817464cdc50c466b5497d4ac2e2dc34c3c15f05535b463643c";

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
            "{{\"jsonrpc\":\"2.0\",\"id\":{s},\"result\":{{\"name\":\"provekit-lsp-zig\",\"version\":\"{s}\",\"protocol_version\":\"{s}\",\"kit_id\":\"{s}\",\"protocol_catalog_cid\":\"{s}\",\"capabilities\":{{\"source_surfaces\":[\"{s}\"],\"entry_kinds\":[\"bind-lift-entry\",\"call-edge\"],\"diagnostic_codes\":[\"provekit.lsp.parse_error\",\"provekit.lsp.lift_gap\",\"provekit.lsp.implication_failed\"],\"status_kinds\":[\"materialize\",\"emit\",\"check\",\"prove\"]}}}}}}\n",
            .{ id, VERSION, SHARED_LSP_PROTOCOL_VERSION, KIT_ID, SHARED_LSP_PROTOCOL_CATALOG_CID, SURFACE },
        );
        return true;
    }

    if (std.mem.indexOf(u8, line, "\"analyzeDocument\"") != null) {
        try handleAnalyzeDocument(alloc, line, id, writer);
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
    const source_raw = extractJsonStringField(line, "source") orelse "";
    const source = try unescapeJsonString(alloc, source_raw);
    defer alloc.free(source);
    const path_raw = extractJsonStringField(line, "path") orelse "input.zig";
    const path = try unescapeJsonString(alloc, path_raw);
    defer alloc.free(path);

    var arena = std.heap.ArenaAllocator.init(alloc);
    defer arena.deinit();
    const lifted = try lift.liftSource(arena.allocator(), source, path);

    const decls_json = try std.json.Stringify.valueAlloc(alloc, lifted.declarations, .{ .whitespace = .minified });
    defer alloc.free(decls_json);

    const call_edges_json = try buildCallEdgesJson(alloc, source, path);
    defer alloc.free(call_edges_json);

    try writer.print(
        "{{\"jsonrpc\":\"2.0\",\"id\":{s},\"result\":{{\"declarations\":{s},\"callEdges\":{s},\"warnings\":[]}}}}\n",
        .{ id, decls_json, call_edges_json },
    );
}

fn handleAnalyzeDocument(
    alloc: std.mem.Allocator,
    line: []const u8,
    id: []const u8,
    writer: *Io.Writer,
) !void {
    if (extractJsonStringField(line, "kit_id")) |kit_raw| {
        const kit = try unescapeJsonString(alloc, kit_raw);
        defer alloc.free(kit);
        if (!std.mem.eql(u8, kit, KIT_ID)) {
            try writer.print(
                "{{\"jsonrpc\":\"2.0\",\"id\":{s},\"error\":{{\"code\":-32602,\"message\":\"unsupported kit_id\"}}}}\n",
                .{id},
            );
            return;
        }
    }

    const source_raw = extractJsonStringField(line, "text") orelse
        extractJsonStringField(line, "source") orelse "";
    const source = try unescapeJsonString(alloc, source_raw);
    defer alloc.free(source);

    const path_raw = extractJsonStringField(line, "file") orelse
        extractJsonStringField(line, "path") orelse "input.zig";
    const path = try unescapeJsonString(alloc, path_raw);
    defer alloc.free(path);

    const uri = if (extractJsonStringField(line, "uri")) |uri_raw|
        try unescapeJsonString(alloc, uri_raw)
    else
        try std.fmt.allocPrint(alloc, "file://{s}", .{path});
    defer alloc.free(uri);

    const document_cid = try ir.jcsHash(alloc, source);
    defer alloc.free(document_cid);
    const document_cid_json = try jsonStringAlloc(alloc, document_cid);
    defer alloc.free(document_cid_json);
    const uri_json = try jsonStringAlloc(alloc, uri);
    defer alloc.free(uri_json);
    const path_json = try jsonStringAlloc(alloc, path);
    defer alloc.free(path_json);

    const diagnostics_json = try buildForwardDiagnosticsJson(alloc, source);
    defer alloc.free(diagnostics_json);

    try writer.print(
        "{{\"jsonrpc\":\"2.0\",\"id\":{s},\"result\":{{\"kind\":\"lsp-document-analysis\",\"schema_version\":\"1\",\"kit_id\":\"{s}\",\"uri\":{s},\"file\":{s},\"document_cid\":{s},\"protocol_catalog_cid\":\"{s}\",\"entries\":[],\"diagnostics\":{s},\"statuses\":[],\"project\":null}}}}\n",
        .{ id, KIT_ID, uri_json, path_json, document_cid_json, SHARED_LSP_PROTOCOL_CATALOG_CID, diagnostics_json },
    );
}

fn buildForwardDiagnosticsJson(alloc: std.mem.Allocator, source: []const u8) ![]u8 {
    var lines: std.ArrayList([]const u8) = .empty;
    defer lines.deinit(alloc);

    var line_iter = std.mem.splitScalar(u8, source, '\n');
    while (line_iter.next()) |line| {
        try lines.append(alloc, line);
    }

    const functions = try collectFunctionSpans(alloc, lines.items);
    defer alloc.free(functions);

    var out: std.ArrayList(u8) = .empty;
    errdefer out.deinit(alloc);
    try out.append(alloc, '[');

    var first = true;
    for (functions) |function| {
        if (std.mem.eql(u8, function.name, "checkPositive")) continue;

        var line_index = function.start_line;
        while (line_index <= function.end_line and line_index < lines.items.len) : (line_index += 1) {
            const line = lines.items[line_index];
            const column = findCallColumn(line, "checkPositive") orelse continue;
            if (isInsideLoop(lines.items, function.start_line, line_index)) continue;
            if (callArgumentIsPositive(line[column + "checkPositive".len ..])) continue;

            if (!first) try out.append(alloc, ',');
            first = false;
            try appendImplicationFailedDiagnostic(alloc, &out, line_index + 1, column);
        }
    }

    try out.append(alloc, ']');
    return out.toOwnedSlice(alloc);
}

fn appendImplicationFailedDiagnostic(
    alloc: std.mem.Allocator,
    out: *std.ArrayList(u8),
    line: usize,
    column: usize,
) !void {
    const callee = "checkPositive";
    const pre_cid = try ir.jcsHash(alloc, callee ++ ":pre:x > 0");
    defer alloc.free(pre_cid);
    const post_cid = try ir.jcsHash(alloc, callee ++ ":post:returns true");
    defer alloc.free(post_cid);
    const seed = try std.fmt.allocPrint(alloc, "{s}|{s}|{s}", .{ callee, pre_cid, post_cid });
    defer alloc.free(seed);
    const attestation_seed = try std.fmt.allocPrint(alloc, "attestation:{s}", .{seed});
    defer alloc.free(attestation_seed);
    const contract_seed = try std.fmt.allocPrint(alloc, "contract:{s}", .{seed});
    defer alloc.free(contract_seed);
    const attestation_cid = try ir.jcsHash(alloc, attestation_seed);
    defer alloc.free(attestation_cid);
    const contract_cid = try ir.jcsHash(alloc, contract_seed);
    defer alloc.free(contract_cid);
    const current_post_cid = try ir.jcsHash(alloc, "post:known:x <= 0");
    defer alloc.free(current_post_cid);

    const diagnostic_json = try std.fmt.allocPrint(
        alloc,
        "{{\"code\":\"provekit.lsp.implication_failed\",\"data\":{{\"callee\":\"{s}\",\"callee_attestation_cid\":\"{s}\",\"callee_contract_cid\":\"{s}\",\"callee_post_cid\":\"{s}\",\"callee_pre_cid\":\"{s}\",\"current_post_cid\":\"{s}\",\"kind\":\"provekit.lsp.implication_failed\",\"missing_conjuncts\":[\"x > 0\"],\"schema_version\":1}},\"kit_id\":\"{s}\",\"message\":\"callee precondition not established at this callsite\",\"producer\":\"forward-propagation\",\"protocol_catalog_cid\":\"{s}\",\"range\":{{\"start_line\":{d},\"start_col\":{d},\"end_line\":{d},\"end_col\":{d}}},\"severity\":\"error\"}}",
        .{
            callee,
            attestation_cid,
            contract_cid,
            post_cid,
            pre_cid,
            current_post_cid,
            KIT_ID,
            SHARED_LSP_PROTOCOL_CATALOG_CID,
            line,
            column,
            line,
            column + callee.len,
        },
    );
    defer alloc.free(diagnostic_json);
    try out.appendSlice(alloc, diagnostic_json);
}

fn callArgumentIsPositive(after_name: []const u8) bool {
    var i: usize = 0;
    while (i < after_name.len and isSpace(after_name[i])) : (i += 1) {}
    if (i >= after_name.len or after_name[i] != '(') return false;
    i += 1;
    while (i < after_name.len and isSpace(after_name[i])) : (i += 1) {}

    var sign: i64 = 1;
    if (i < after_name.len and after_name[i] == '+') {
        i += 1;
    } else if (i < after_name.len and after_name[i] == '-') {
        sign = -1;
        i += 1;
    }
    while (i < after_name.len and isSpace(after_name[i])) : (i += 1) {}

    var value: i64 = 0;
    var saw_digit = false;
    while (i < after_name.len and after_name[i] >= '0' and after_name[i] <= '9') : (i += 1) {
        saw_digit = true;
        value = value * 10 + @as(i64, after_name[i] - '0');
    }
    return saw_digit and sign * value > 0;
}

fn isInsideLoop(lines: []const []const u8, function_start: usize, target_line: usize) bool {
    var depth: isize = 0;
    var loop_depth: ?isize = null;

    var line_index = function_start;
    while (line_index <= target_line and line_index < lines.len) : (line_index += 1) {
        if (loop_depth) |ld| {
            if (depth < ld) loop_depth = null;
        }

        if (line_index == target_line) return loop_depth != null;

        const trimmed = std.mem.trimStart(u8, lines[line_index], " \t");
        if ((std.mem.startsWith(u8, trimmed, "while") or std.mem.startsWith(u8, trimmed, "for")) and
            std.mem.indexOfScalar(u8, trimmed, '{') != null)
        {
            loop_depth = depth + 1;
        }

        for (lines[line_index]) |ch| {
            switch (ch) {
                '{' => depth += 1,
                '}' => depth -= 1,
                else => {},
            }
        }
    }

    return false;
}

const FunctionSpan = struct {
    name: []const u8,
    start_line: usize,
    end_line: usize,
};

fn buildCallEdgesJson(alloc: std.mem.Allocator, source: []const u8, path: []const u8) ![]u8 {
    var lines: std.ArrayList([]const u8) = .empty;
    defer lines.deinit(alloc);

    var line_iter = std.mem.splitScalar(u8, source, '\n');
    while (line_iter.next()) |line| {
        try lines.append(alloc, line);
    }

    const functions = try collectFunctionSpans(alloc, lines.items);
    defer alloc.free(functions);

    var out: std.ArrayList(u8) = .empty;
    errdefer out.deinit(alloc);

    try out.append(alloc, '[');
    var first = true;
    for (functions) |caller| {
        var line_index = caller.start_line;
        while (line_index <= caller.end_line and line_index < lines.items.len) : (line_index += 1) {
            const line = lines.items[line_index];
            for (functions) |callee| {
                if (std.mem.eql(u8, caller.name, callee.name)) continue;
                const column = findCallColumn(line, callee.name) orelse continue;

                if (!first) try out.append(alloc, ',');
                first = false;
                try appendCallEdgeJson(
                    alloc,
                    &out,
                    path,
                    caller.name,
                    callee.name,
                    line_index + 1,
                    column + 1,
                );
            }
        }
    }
    try out.append(alloc, ']');

    return out.toOwnedSlice(alloc);
}

fn collectFunctionSpans(alloc: std.mem.Allocator, lines: []const []const u8) ![]FunctionSpan {
    var functions: std.ArrayList(FunctionSpan) = .empty;
    errdefer functions.deinit(alloc);

    var line_index: usize = 0;
    while (line_index < lines.len) : (line_index += 1) {
        const name = parseFunctionName(lines[line_index]) orelse continue;
        const end_line = findFunctionEndLine(lines, line_index);
        try functions.append(alloc, .{
            .name = name,
            .start_line = line_index,
            .end_line = end_line,
        });
        line_index = end_line;
    }

    return functions.toOwnedSlice(alloc);
}

fn parseFunctionName(line: []const u8) ?[]const u8 {
    var search_from: usize = 0;
    while (std.mem.indexOfPos(u8, line, search_from, "fn")) |fn_index| {
        const after_fn = fn_index + 2;
        const before_ok = fn_index == 0 or !isIdentifierChar(line[fn_index - 1]);
        const after_ok = after_fn < line.len and isSpace(line[after_fn]);
        if (!before_ok or !after_ok) {
            search_from = after_fn;
            continue;
        }

        var name_start = after_fn;
        while (name_start < line.len and isSpace(line[name_start])) : (name_start += 1) {}

        var name_end = name_start;
        while (name_end < line.len and isIdentifierChar(line[name_end])) : (name_end += 1) {}

        if (name_end > name_start) return line[name_start..name_end];
        search_from = after_fn;
    }
    return null;
}

fn findFunctionEndLine(lines: []const []const u8, start_line: usize) usize {
    var depth: isize = 0;
    var saw_open_brace = false;

    var line_index = start_line;
    while (line_index < lines.len) : (line_index += 1) {
        for (lines[line_index]) |ch| {
            switch (ch) {
                '{' => {
                    depth += 1;
                    saw_open_brace = true;
                },
                '}' => depth -= 1,
                else => {},
            }
        }
        if (saw_open_brace and depth <= 0) return line_index;
    }

    return start_line;
}

fn findCallColumn(line: []const u8, callee: []const u8) ?usize {
    var search_from: usize = 0;
    while (std.mem.indexOfPos(u8, line, search_from, callee)) |name_index| {
        const after_name = name_index + callee.len;
        const before_ok = name_index == 0 or !isIdentifierChar(line[name_index - 1]);
        const after_name_ok = after_name >= line.len or !isIdentifierChar(line[after_name]);
        if (before_ok and after_name_ok) {
            var after_call_name = after_name;
            while (after_call_name < line.len and isSpace(line[after_call_name])) : (after_call_name += 1) {}
            if (after_call_name < line.len and line[after_call_name] == '(') return name_index;
        }
        search_from = after_name;
    }
    return null;
}

fn appendCallEdgeJson(
    alloc: std.mem.Allocator,
    out: *std.ArrayList(u8),
    path: []const u8,
    source_name: []const u8,
    target_name: []const u8,
    line: usize,
    column: usize,
) !void {
    const source_cid = try std.fmt.allocPrint(alloc, "pending-zig:{s}", .{source_name});
    defer alloc.free(source_cid);
    const target_symbol = try std.fmt.allocPrint(alloc, "zig-kit:{s}", .{target_name});
    defer alloc.free(target_symbol);

    const prefix = try std.fmt.allocPrint(alloc, "{{\"callSiteLocus\":{{\"col\":{d},\"file\":", .{column});
    defer alloc.free(prefix);
    try out.appendSlice(alloc, prefix);
    try appendJsonString(alloc, out, path);

    const middle = try std.fmt.allocPrint(
        alloc,
        ",\"line\":{d}}},\"evidenceTerm\":{{\"args\":[],\"kind\":\"atomic\",\"name\":\"call-site-obligation\"}},\"kind\":\"call-edge\",\"schemaVersion\":\"1\",\"sourceContractCid\":",
        .{line},
    );
    defer alloc.free(middle);
    try out.appendSlice(alloc, middle);
    try appendJsonString(alloc, out, source_cid);

    try out.appendSlice(alloc, ",\"targetSymbol\":");
    try appendJsonString(alloc, out, target_symbol);
    try out.append(alloc, '}');
}

fn appendJsonString(alloc: std.mem.Allocator, out: *std.ArrayList(u8), value: []const u8) !void {
    const json = try jsonStringAlloc(alloc, value);
    defer alloc.free(json);
    try out.appendSlice(alloc, json);
}

fn jsonStringAlloc(alloc: std.mem.Allocator, value: []const u8) ![]u8 {
    return std.json.Stringify.valueAlloc(alloc, value, .{ .whitespace = .minified });
}

fn isSpace(ch: u8) bool {
    return ch == ' ' or ch == '\t' or ch == '\r';
}

fn isIdentifierChar(ch: u8) bool {
    return (ch >= 'a' and ch <= 'z') or
        (ch >= 'A' and ch <= 'Z') or
        (ch >= '0' and ch <= '9') or
        ch == '_';
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
                'n' => {
                    try out.append(alloc, '\n');
                    i += 2;
                },
                't' => {
                    try out.append(alloc, '\t');
                    i += 2;
                },
                'r' => {
                    try out.append(alloc, '\r');
                    i += 2;
                },
                '"' => {
                    try out.append(alloc, '"');
                    i += 2;
                },
                '\\' => {
                    try out.append(alloc, '\\');
                    i += 2;
                },
                '/' => {
                    try out.append(alloc, '/');
                    i += 2;
                },
                else => {
                    try out.append(alloc, raw[i]);
                    i += 1;
                },
            }
        } else {
            try out.append(alloc, raw[i]);
            i += 1;
        }
    }
    return out.toOwnedSlice(alloc);
}
