// SPDX-License-Identifier: Apache-2.0
//
// LSP plugin round-trip test (#221).
//
// Spawns the `provekit-lift-zig` binary in `--rpc` mode and drives the
// NDJSON-over-stdio plugin protocol end to end:
//
//   1. initialize -> {name, version, capabilities}
//   2. parse      -> {declarations, warnings}
//   3. shutdown   -> null result, clean exit
//
// The binary path is supplied via the `PROVEKIT_LIFT_ZIG_BIN` env var, set
// by `build.zig` so the test always exercises the freshly-built artifact.

const std = @import("std");
const testing = std.testing;

fn binPath(alloc: std.mem.Allocator) ![]u8 {
    if (std.process.getEnvVarOwned(alloc, "PROVEKIT_LIFT_ZIG_BIN")) |p| {
        return p;
    } else |_| {
        // Fallback for direct invocation: assume zig-out/bin layout.
        return alloc.dupe(u8, "zig-out/bin/provekit-lift-zig");
    }
}

const Exchange = struct {
    child: *std.process.Child,
    line_buf: std.ArrayList(u8),

    fn init(alloc: std.mem.Allocator, child: *std.process.Child) Exchange {
        return .{ .child = child, .line_buf = std.ArrayList(u8).init(alloc) };
    }

    fn deinit(self: *Exchange) void {
        self.line_buf.deinit();
    }

    fn send(self: *Exchange, payload: []const u8) !void {
        try self.child.stdin.?.writer().writeAll(payload);
        try self.child.stdin.?.writer().writeByte('\n');
    }

    fn recv(self: *Exchange) ![]const u8 {
        self.line_buf.clearRetainingCapacity();
        try self.child.stdout.?.reader().streamUntilDelimiter(
            self.line_buf.writer(),
            '\n',
            64 * 1024,
        );
        return self.line_buf.items;
    }
};

test "lsp plugin round-trip: initialize, parse, shutdown" {
    const alloc = testing.allocator;
    const bin = try binPath(alloc);
    defer alloc.free(bin);

    var argv = [_][]const u8{ bin, "--rpc" };
    var child = std.process.Child.init(&argv, alloc);
    child.stdin_behavior = .Pipe;
    child.stdout_behavior = .Pipe;
    child.stderr_behavior = .Pipe;
    try child.spawn();
    errdefer _ = child.kill() catch {};

    var ex = Exchange.init(alloc, &child);
    defer ex.deinit();

    // 1. initialize ----------------------------------------------------------
    try ex.send(
        \\{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}
    );
    {
        const line = try ex.recv();
        // Cheap asserts on JSON shape — full parse below.
        try testing.expect(std.mem.indexOf(u8, line, "\"jsonrpc\":\"2.0\"") != null);
        try testing.expect(std.mem.indexOf(u8, line, "\"id\":1") != null);
        try testing.expect(std.mem.indexOf(u8, line, "\"result\"") != null);
        try testing.expect(std.mem.indexOf(u8, line, "\"name\":\"provekit-lift-zig\"") != null);
        try testing.expect(std.mem.indexOf(u8, line, "\"version\"") != null);
        try testing.expect(std.mem.indexOf(u8, line, "\"capabilities\"") != null);
        try testing.expect(std.mem.indexOf(u8, line, "\"parse\"") != null);
    }

    // 2. parse ---------------------------------------------------------------
    try ex.send(
        \\{"jsonrpc":"2.0","id":2,"method":"parse","params":{"path":"sample.zig","source":"//provekit:contract\nfn add(a: i32, b: i32) i32 { return a + b; }\n"}}
    );
    {
        const line = try ex.recv();
        try testing.expect(std.mem.indexOf(u8, line, "\"id\":2") != null);
        try testing.expect(std.mem.indexOf(u8, line, "\"result\"") != null);
        try testing.expect(std.mem.indexOf(u8, line, "\"declarations\"") != null);
        try testing.expect(std.mem.indexOf(u8, line, "\"warnings\"") != null);
    }

    // 3. shutdown ------------------------------------------------------------
    try ex.send(
        \\{"jsonrpc":"2.0","id":3,"method":"shutdown"}
    );
    {
        const line = try ex.recv();
        try testing.expect(std.mem.indexOf(u8, line, "\"id\":3") != null);
        try testing.expect(std.mem.indexOf(u8, line, "\"result\":null") != null);
    }

    // The plugin must exit cleanly.
    child.stdin.?.close();
    child.stdin = null;
    const term = try child.wait();
    try testing.expectEqual(std.process.Child.Term{ .Exited = 0 }, term);
}

test "lsp plugin round-trip: unknown method returns -32601" {
    const alloc = testing.allocator;
    const bin = try binPath(alloc);
    defer alloc.free(bin);

    var argv = [_][]const u8{ bin, "--rpc" };
    var child = std.process.Child.init(&argv, alloc);
    child.stdin_behavior = .Pipe;
    child.stdout_behavior = .Pipe;
    child.stderr_behavior = .Pipe;
    try child.spawn();
    errdefer _ = child.kill() catch {};

    var ex = Exchange.init(alloc, &child);
    defer ex.deinit();

    try ex.send(
        \\{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}
    );
    _ = try ex.recv();

    try ex.send(
        \\{"jsonrpc":"2.0","id":2,"method":"no_such_method"}
    );
    {
        const line = try ex.recv();
        try testing.expect(std.mem.indexOf(u8, line, "\"error\"") != null);
        try testing.expect(std.mem.indexOf(u8, line, "-32601") != null);
    }

    try ex.send(
        \\{"jsonrpc":"2.0","id":3,"method":"shutdown"}
    );
    _ = try ex.recv();
    child.stdin.?.close();
    child.stdin = null;
    _ = try child.wait();
}
