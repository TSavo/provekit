const std = @import("std");
const lift = @import("provekit-lift-zig-source");

const Io = std.Io;
const READ_BUF = 256 * 1024;
const WRITE_BUF = 256 * 1024;

pub fn main(init: std.process.Init) !void {
    const alloc = init.gpa;
    const io = init.io;
    const args = try init.minimal.args.toSlice(init.arena.allocator());

    var rpc_mode = false;
    for (args[1..]) |arg| {
        if (std.mem.eql(u8, arg, "--rpc")) rpc_mode = true;
    }

    if (rpc_mode) {
        try runRpc(alloc, io);
        return;
    }

    std.debug.print("usage: provekit-lift-zig-source --rpc\n", .{});
    std.process.exit(1);
}

fn runRpc(alloc: std.mem.Allocator, io: Io) !void {
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
        stdin_reader.toss(@min(1, stdin_reader.bufferedLen()));
        const keep_going = try handleLine(alloc, io, line, stdout_writer);
        try stdout_writer.flush();
        if (!keep_going) break;
    }
}

fn handleLine(alloc: std.mem.Allocator, io: Io, line: []const u8, writer: *Io.Writer) !bool {
    const id = extractId(line);
    if (std.mem.indexOf(u8, line, "\"initialize\"") != null) {
        try writer.print(
            "{{\"jsonrpc\":\"2.0\",\"id\":{s},\"result\":{{\"name\":\"provekit-lift-zig-source\",\"version\":\"" ++ lift.VERSION ++ "\",\"protocol_version\":\"pep/1.7.0\",\"capabilities\":{{\"authoring_surfaces\":[\"" ++ lift.DIALECT ++ "\"],\"emits_signed_mementos\":false,\"ir_version\":\"v1.1.0\"}}}}}}\n",
            .{id},
        );
        return true;
    }
    if (std.mem.indexOf(u8, line, "\"lift\"") != null) {
        try handleLift(alloc, io, line, id, writer);
        return true;
    }
    if (std.mem.indexOf(u8, line, "\"provekit.plugin.recognize\"") != null) {
        try handleRecognize(alloc, io, line, id, writer);
        return true;
    }
    if (std.mem.indexOf(u8, line, "\"shutdown\"") != null) {
        try writer.print("{{\"jsonrpc\":\"2.0\",\"id\":{s},\"result\":null}}\n", .{id});
        return false;
    }
    try writer.print("{{\"jsonrpc\":\"2.0\",\"id\":{s},\"error\":{{\"code\":-32601,\"message\":\"unknown method\"}}}}\n", .{id});
    return true;
}

fn handleLift(alloc: std.mem.Allocator, io: Io, line: []const u8, id: []const u8, writer: *Io.Writer) !void {
    var arena = std.heap.ArenaAllocator.init(alloc);
    defer arena.deinit();
    const arena_alloc = arena.allocator();

    var declarations: std.ArrayList(lift.FunctionContract) = .empty;
    var refusals: std.ArrayList(lift.Refusal) = .empty;
    const verify_layer = std.mem.indexOf(u8, line, "\"layer\":\"verify\"") != null;

    if (extractJsonStringField(line, "source")) |source_raw| {
        const source = try unescapeJsonString(arena_alloc, source_raw);
        const path_raw = extractJsonStringField(line, "path") orelse "input.zig";
        const path = try unescapeJsonString(arena_alloc, path_raw);
        const out = try lift.liftSource(arena_alloc, source, path);
        for (out.declarations) |decl| try declarations.append(arena_alloc, decl);
        for (out.refusals) |refusal| try refusals.append(arena_alloc, refusal);
    } else {
        const workspace_raw = extractJsonStringField(line, "workspace_root") orelse ".";
        const workspace = try unescapeJsonString(arena_alloc, workspace_raw);
        const paths = try extractStringArray(arena_alloc, line, "source_paths");
        if (paths.len == 0) {
            try writer.print("{{\"jsonrpc\":\"2.0\",\"id\":{s},\"error\":{{\"code\":-32602,\"message\":\"source_paths required\"}}}}\n", .{id});
            return;
        }
        var dir = try Io.Dir.openDir(Io.Dir.cwd(), io, workspace, .{ .iterate = true });
        defer dir.close(io);
        for (paths) |rel| try liftPath(arena_alloc, io, dir, rel, &declarations, &refusals);
    }

    if (verify_layer) {
        const verify_declarations = try lift.verifyContracts(arena_alloc, declarations.items);
        declarations.clearRetainingCapacity();
        for (verify_declarations) |decl| try declarations.append(arena_alloc, decl);
    }

    const decls_json = try std.json.Stringify.valueAlloc(alloc, declarations.items, .{ .whitespace = .minified });
    defer alloc.free(decls_json);
    const refusals_json = try std.json.Stringify.valueAlloc(alloc, refusals.items, .{ .whitespace = .minified });
    defer alloc.free(refusals_json);
    try writer.print("{{\"jsonrpc\":\"2.0\",\"id\":{s},\"result\":{{\"kind\":\"ir-document\",\"ir\":{s},\"callEdges\":[],\"diagnostics\":[],\"opacityReport\":[],\"refusals\":{s}}}}}\n", .{ id, decls_json, refusals_json });
}

const RecognizeWireRequest = struct {
    params: RecognizeWireParams,
};

const RecognizeWireParams = struct {
    project_root: []const u8,
    source_paths: []const []const u8,
    binding_templates: []const RecognizeWireBinding = &.{},
};

const RecognizeWireBinding = struct {
    concept_name: ?[]const u8 = null,
    library_tag: ?[]const u8 = null,
    target_library_tag: ?[]const u8 = null,
    family: ?[]const u8 = null,
    template_cid: []const u8 = "",
    param_names: []const []const u8 = &.{},
    contract_cid: ?[]const u8 = null,
};

fn handleRecognize(alloc: std.mem.Allocator, io: Io, line: []const u8, id: []const u8, writer: *Io.Writer) !void {
    var arena = std.heap.ArenaAllocator.init(alloc);
    defer arena.deinit();
    const arena_alloc = arena.allocator();

    const parsed = std.json.parseFromSlice(
        RecognizeWireRequest,
        arena_alloc,
        line,
        .{ .ignore_unknown_fields = true },
    ) catch |err| {
        try writer.print("{{\"jsonrpc\":\"2.0\",\"id\":{s},\"error\":{{\"code\":-32602,\"message\":\"invalid recognize params: {t}\"}}}}\n", .{ id, err });
        return;
    };
    defer parsed.deinit();

    var bindings: std.ArrayList(lift.BindingTemplate) = .empty;
    for (parsed.value.params.binding_templates) |binding| {
        if (binding.template_cid.len == 0) continue;
        try bindings.append(arena_alloc, .{
            .concept_name = binding.concept_name,
            .library_tag = binding.library_tag,
            .target_library_tag = binding.target_library_tag,
            .family = binding.family,
            .template_cid = binding.template_cid,
            .param_names = binding.param_names,
            .contract_cid = binding.contract_cid,
        });
    }

    const response = lift.recognizeSourcePaths(
        arena_alloc,
        io,
        parsed.value.params.project_root,
        parsed.value.params.source_paths,
        bindings.items,
    ) catch |err| {
        try writer.print("{{\"jsonrpc\":\"2.0\",\"id\":{s},\"error\":{{\"code\":-32603,\"message\":\"recognize failed: {t}\"}}}}\n", .{ id, err });
        return;
    };

    const tags_json = try std.json.Stringify.valueAlloc(alloc, response.tags, .{ .whitespace = .minified });
    defer alloc.free(tags_json);
    try writer.print("{{\"jsonrpc\":\"2.0\",\"id\":{s},\"result\":{{\"tags\":{s}}}}}\n", .{ id, tags_json });
}

fn liftPath(
    alloc: std.mem.Allocator,
    io: Io,
    root: Io.Dir,
    rel: []const u8,
    declarations: *std.ArrayList(lift.FunctionContract),
    refusals: *std.ArrayList(lift.Refusal),
) !void {
    const file_text = Io.Dir.readFileAlloc(root, io, rel, alloc, .unlimited) catch |err| switch (err) {
        error.IsDir => {
            var subdir = Io.Dir.openDir(root, io, rel, .{ .iterate = true }) catch |open_err| {
                try refusals.append(alloc, .{ .kind = "io-error", .function = null, .line = 0, .reason = try std.fmt.allocPrint(alloc, "open {s}: {t}", .{ rel, open_err }) });
                return;
            };
            defer subdir.close(io);
            var walker = try subdir.walk(alloc);
            defer walker.deinit();
            while (try walker.next(io)) |entry| {
                if (entry.kind != .file) continue;
                if (!std.mem.endsWith(u8, entry.basename, ".zig")) continue;
                const text = Io.Dir.readFileAlloc(entry.dir, io, entry.basename, alloc, .unlimited) catch |read_err| {
                    try refusals.append(alloc, .{ .kind = "io-error", .function = null, .line = 0, .reason = try std.fmt.allocPrint(alloc, "read {s}: {t}", .{ entry.path, read_err }) });
                    continue;
                };
                const path = if (std.mem.eql(u8, rel, ".")) entry.path else try std.fmt.allocPrint(alloc, "{s}/{s}", .{ rel, entry.path });
                const out = try lift.liftSource(alloc, text, path);
                for (out.declarations) |decl| try declarations.append(alloc, decl);
                for (out.refusals) |refusal| try refusals.append(alloc, refusal);
            }
            return;
        },
        else => {
            try refusals.append(alloc, .{ .kind = "io-error", .function = null, .line = 0, .reason = try std.fmt.allocPrint(alloc, "read {s}: {t}", .{ rel, err }) });
            return;
        },
    };
    const out = try lift.liftSource(alloc, file_text, rel);
    for (out.declarations) |decl| try declarations.append(alloc, decl);
    for (out.refusals) |refusal| try refusals.append(alloc, refusal);
}

fn extractId(line: []const u8) []const u8 {
    const key = "\"id\":";
    const pos = std.mem.indexOf(u8, line, key) orelse return "null";
    var i = pos + key.len;
    while (i < line.len and (line[i] == ' ' or line[i] == '\t')) i += 1;
    const start = i;
    if (i < line.len and line[i] == '"') {
        i += 1;
        while (i < line.len) : (i += 1) {
            if (line[i] == '"' and line[i - 1] != '\\') {
                i += 1;
                break;
            }
        }
        return line[start..@min(i, line.len)];
    }
    while (i < line.len and line[i] != ',' and line[i] != '}') i += 1;
    return std.mem.trim(u8, line[start..i], " \t");
}

fn extractJsonStringField(line: []const u8, field: []const u8) ?[]const u8 {
    const quoted = std.fmt.allocPrint(std.heap.page_allocator, "\"{s}\"", .{field}) catch return null;
    defer std.heap.page_allocator.free(quoted);
    var pos = std.mem.indexOf(u8, line, quoted) orelse return null;
    pos += quoted.len;
    while (pos < line.len and (line[pos] == ' ' or line[pos] == '\t' or line[pos] == '\r' or line[pos] == '\n')) pos += 1;
    if (pos >= line.len or line[pos] != ':') return null;
    pos += 1;
    while (pos < line.len and (line[pos] == ' ' or line[pos] == '\t' or line[pos] == '\r' or line[pos] == '\n')) pos += 1;
    if (pos >= line.len or line[pos] != '"') return null;
    pos += 1;
    const start = pos;
    while (pos < line.len) : (pos += 1) {
        if (line[pos] == '"' and (pos == start or line[pos - 1] != '\\')) return line[start..pos];
    }
    return null;
}

fn extractStringArray(alloc: std.mem.Allocator, line: []const u8, field: []const u8) ![]const []const u8 {
    const quoted = try std.fmt.allocPrint(alloc, "\"{s}\"", .{field});
    var pos = std.mem.indexOf(u8, line, quoted) orelse return &.{};
    pos += quoted.len;
    while (pos < line.len and (line[pos] == ' ' or line[pos] == '\t' or line[pos] == '\r' or line[pos] == '\n')) pos += 1;
    if (pos >= line.len or line[pos] != ':') return &.{};
    pos += 1;
    while (pos < line.len and (line[pos] == ' ' or line[pos] == '\t' or line[pos] == '\r' or line[pos] == '\n')) pos += 1;
    if (pos >= line.len or line[pos] != '[') return &.{};
    pos += 1;
    var out: std.ArrayList([]const u8) = .empty;
    while (pos < line.len and line[pos] != ']') : (pos += 1) {
        if (line[pos] != '"') continue;
        pos += 1;
        const start = pos;
        while (pos < line.len) : (pos += 1) {
            if (line[pos] == '"' and line[pos - 1] != '\\') break;
        }
        try out.append(alloc, try unescapeJsonString(alloc, line[start..pos]));
    }
    return out.toOwnedSlice(alloc);
}

fn unescapeJsonString(alloc: std.mem.Allocator, raw: []const u8) ![]u8 {
    var out: std.ArrayList(u8) = .empty;
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

test {
    _ = lift;
}
