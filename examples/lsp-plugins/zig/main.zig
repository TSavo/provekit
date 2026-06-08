// Sugar LSP Language Plugin: Zig
//
// A standalone binary that speaks sugar-lsp-plugin/1 over stdio.
// Parses Zig source files and extracts sugar annotations.
//
// Usage: zig build-exe main.zig -o sugar-lsp-zig && ./sugar-lsp-zig --rpc
//
// To use this plugin, add to `.sugar/config.toml`:
//   [[language]]
//   name = "zig"
//   extensions = [".zig"]
//   plugin = "sugar-lsp-zig"

const std = @import("std");
const json = std.json;

const Annotation = struct {
    function_name: []const u8,
    kind: []const u8,
    target_cid: ?[]const u8 = null,
    range: Range,
};

const Range = struct {
    start: Position,
    end: Position,
};

const Position = struct {
    line: u32,
    character: u32,
};

fn findAhead(lines: [][]const u8, start: usize, needle: []const u8) []const u8 {
    var j = start + 1;
    while (j < lines.len and j < start + 10) : (j += 1) {
        if (std.mem.indexOf(u8, lines[j], needle)) |_| {
            // Extract fn name: "fn name(...)"
            const line = lines[j];
            const fn_idx = std.mem.indexOf(u8, line, "fn ") orelse continue;
            const after = line[fn_idx + 3 ..];
            const end = std.mem.indexOfAny(u8, after, " (\n") orelse after.len;
            return after[0..end];
        }
    }
    return "unknown";
}

fn parseZig(allocator: std.mem.Allocator, text: []const u8) ![]Annotation {
    var annotations = std.ArrayList(Annotation).init(allocator);
    errdefer annotations.deinit();

    var lines = std.ArrayList([]const u8).init(allocator);
    defer lines.deinit();

    var it = std.mem.splitScalar(u8, text, '\n');
    while (it.next()) |line| {
        try lines.append(line);
    }

    for (lines.items, 0..) |line, i| {
        const line_num: u32 = @intCast(i);

        if (std.mem.indexOf(u8, line, "//sugar:implement ")) |idx| {
            const after = line[idx + 20 ..];
            const end = std.mem.indexOfAny(u8, after, " \n\t") orelse after.len;
            const cid = after[0..end];
            const fn_name = findAhead(lines.items, i, "fn ");
            try annotations.append(.{
                .function_name = fn_name,
                .kind = "implement",
                .target_cid = cid,
                .range = .{
                    .start = .{ .line = line_num, .character = 0 },
                    .end = .{ .line = line_num + 1, .character = 0 },
                },
            });
        }

        if (std.mem.indexOf(u8, line, "//sugar:contract") != null) {
            const fn_name = findAhead(lines.items, i, "fn ");
            try annotations.append(.{
                .function_name = fn_name,
                .kind = "contract",
                .range = .{
                    .start = .{ .line = line_num, .character = 0 },
                    .end = .{ .line = line_num + 1, .character = 0 },
                },
            });
        }

        if (std.mem.indexOf(u8, line, "//sugar:verify") != null) {
            const fn_name = findAhead(lines.items, i, "fn ");
            try annotations.append(.{
                .function_name = fn_name,
                .kind = "verify",
                .range = .{
                    .start = .{ .line = line_num, .character = 0 },
                    .end = .{ .line = line_num + 1, .character = 0 },
                },
            });
        }
    }

    return annotations.toOwnedSlice();
}

fn writeAnnotations(writer: anytype, annotations: []Annotation) !void {
    try writer.writeAll("[{ ");
    for (annotations, 0..) |a, i| {
        if (i > 0) try writer.writeAll(", ");
        try writer.writeAll("{\"function_name\":\"");
        try writer.writeAll(a.function_name);
        try writer.writeAll("\",\"kind\":\"");
        try writer.writeAll(a.kind);
        try writer.writeAll("\"");
        if (a.target_cid) |cid| {
            try writer.writeAll(",\"target_cid\":\"");
            try writer.writeAll(cid);
            try writer.writeAll("\"");
        }
        try writer.print(",\"range\":{{\"start\":{{\"line\":{d},\"character\":0}},\"end\":{{\"line\":{d},\"character\":0}}}}}", .{ a.range.start.line, a.range.end.line });
    }
    try writer.writeAll(" ]");
}

pub fn main() !void {
    var gpa = std.heap.GeneralPurposeAllocator(.{}){};
    defer _ = gpa.deinit();
    const allocator = gpa.allocator();

    const args = try std.process.argsAlloc(allocator);
    defer std.process.argsFree(allocator, args);

    var rpc_mode = false;
    for (args) |arg| {
        if (std.mem.eql(u8, arg, "--rpc")) {
            rpc_mode = true;
            break;
        }
    }

    if (!rpc_mode) {
        std.debug.print("Usage: sugar-lsp-zig --rpc\n", .{});
        std.process.exit(1);
    }

    const stdin = std.io.getStdIn().reader();
    const stdout = std.io.getStdOut().writer();
    var buf: [4096]u8 = undefined;

    while (true) {
        const maybe_line = try stdin.readUntilDelimiterOrEof(&buf, '\n');
        const line = maybe_line orelse break;

        // Parse JSON: very minimal
        const has_init = std.mem.indexOf(u8, line, "\"initialize\"") != null;
        const has_parse = std.mem.indexOf(u8, line, "\"parse\"") != null;
        const has_shutdown = std.mem.indexOf(u8, line, "\"shutdown\"") != null;

        // Extract id
        var id: []const u8 = "null";
        if (std.mem.indexOf(u8, line, "\"id\":")) |id_pos| {
            const after = line[id_pos + 5 ..];
            const start = std.mem.indexOfNone(u8, after, " \t") orelse 0;
            const end = std.mem.indexOfAny(u8, after[start..], ",}") orelse after.len;
            id = after[start .. start + end];
        }

        if (has_init) {
            try stdout.print("{{\"jsonrpc\":\"2.0\",\"id\":{s},\"result\":{{\"name\":\"sugar-lsp-zig\",\"version\":\"0.1.0\",\"capabilities\":[]}}}}\n", .{id});
        } else if (has_parse) {
            // Extract text field: naive
            var text: []const u8 = "";
            if (std.mem.indexOf(u8, line, "\"text\":")) |tp| {
                const after = line[tp + 8 ..];
                const quote = std.mem.indexOf(u8, after, "\"") orelse 0;
                const end_quote = std.mem.indexOf(u8, after[quote + 1 ..], "\"") orelse 0;
                text = after[quote + 1 .. quote + 1 + end_quote];
            }
            const annotations = try parseZig(allocator, text);
            defer allocator.free(annotations);

            try stdout.print("{{\"jsonrpc\":\"2.0\",\"id\":{s},\"result\":{{\"annotations\":", .{id});
            try writeAnnotations(stdout, annotations);
            try stdout.writeAll("}}\n");
        } else if (has_shutdown) {
            try stdout.print("{{\"jsonrpc\":\"2.0\",\"id\":{s},\"result\":null}}\n", .{id});
            return;
        } else {
            try stdout.print("{{\"jsonrpc\":\"2.0\",\"id\":{s},\"error\":{{\"code\":-32601,\"message\":\"unknown method\"}}}}\n", .{id});
        }
    }
}
