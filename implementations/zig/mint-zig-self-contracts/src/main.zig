// SPDX-License-Identifier: Apache-2.0
//
// mint-zig-self-contracts — Zig peer-implementation orchestrator.
//
// Modes:
//
//   1. CLI:   `mint-zig-self-contracts [<out-dir>]`
//      Mints twice into separate temp dirs to assert byte-determinism,
//      then writes the production .proof to <out-dir> (default: cwd).
//      Prints a human-readable banner.
//
//   2. RPC:   `mint-zig-self-contracts --rpc`
//      Speaks the lift-plugin protocol (provekit-lift/1) over NDJSON on
//      stdin/stdout. Supported methods:
//        - initialize   -> {name, version, capabilities}
//        - lift         -> {kind:"proof-envelope", filename_cid,
//                            contract_set_cid, bytes_base64, diagnostics}
//        - shutdown     -> null   (replies, then exits 0)
//      Stdin EOF is treated as graceful shutdown (architect rule #3 in
//      issue #176; matches the pattern PR #220 established for ts).
//
// Spec: protocol/specs/2026-04-30-lift-plugin-protocol.md
//
// Daemon-lifecycle (PR #220): persistent-daemon-with-explicit-shutdown.
// The Rust dispatcher in implementations/rust/provekit-cli holds the
// child until it explicitly sends `shutdown`; closing stdin also works
// as a fallback for ungraceful client teardown. We do NOT exit after
// the first lift response.

const std = @import("std");

const proof_env = @import("provekit-proof-envelope-zig");
const mint = @import("mint.zig");

const READ_BUF: usize = 256 * 1024;
const WRITE_BUF: usize = 256 * 1024;

// ---------------------------------------------------------------------------
// Tiny JSON-RPC line scanner. We only care about `id` (verbatim
// passthrough) and `method`; the protocol's request shape is small and
// fixed. No third-party JSON dep needed for the request side; responses
// are constructed via std.json.Stringify or hand-built strings.
// ---------------------------------------------------------------------------

const ParsedReq = struct {
    /// Verbatim JSON `id` value (number, string, or `null`). May be
    /// empty when the request omitted the field — caller should default
    /// to `"null"`.
    id_raw: []const u8,
    /// Bare method name (no quotes), or empty when malformed.
    method: []const u8,
};

fn parseReq(line: []const u8) ParsedReq {
    return .{
        .id_raw = extractRawField(line, "id") orelse "null",
        .method = extractStringField(line, "method") orelse "",
    };
}

fn extractRawField(line: []const u8, field: []const u8) ?[]const u8 {
    var key_buf: [64]u8 = undefined;
    if (field.len + 3 > key_buf.len) return null;
    const key = std.fmt.bufPrint(&key_buf, "\"{s}\":", .{field}) catch return null;
    const pos = std.mem.indexOf(u8, line, key) orelse return null;
    var i = pos + key.len;
    while (i < line.len and (line[i] == ' ' or line[i] == '\t')) : (i += 1) {}
    if (i >= line.len) return null;
    if (line[i] == '"') {
        const close = std.mem.indexOfScalarPos(u8, line, i + 1, '"') orelse return null;
        return line[i .. close + 1];
    }
    var j = i;
    while (j < line.len and line[j] != ',' and line[j] != '}' and line[j] != ' ' and line[j] != '\n' and line[j] != '\r') : (j += 1) {}
    return line[i..j];
}

fn extractStringField(line: []const u8, field: []const u8) ?[]const u8 {
    const raw = extractRawField(line, field) orelse return null;
    if (raw.len < 2) return null;
    if (raw[0] != '"' or raw[raw.len - 1] != '"') return null;
    return raw[1 .. raw.len - 1];
}

// ---------------------------------------------------------------------------
// Base64 (standard, with padding) for proof bytes -> RPC `bytes_base64`.
// std.base64.standard.Encoder is fine; this wrapper hides allocation.
// ---------------------------------------------------------------------------

fn base64Encode(alloc: std.mem.Allocator, bytes: []const u8) ![]u8 {
    const enc = std.base64.standard.Encoder;
    const out_len = enc.calcSize(bytes.len);
    const out = try alloc.alloc(u8, out_len);
    _ = enc.encode(out, bytes);
    return out;
}

// ---------------------------------------------------------------------------
// JSON string field writer with conservative escapes (sufficient for
// CIDs (hex), base64, and short ASCII method/error names).
// ---------------------------------------------------------------------------

fn writeJsonString(w: anytype, s: []const u8) !void {
    try w.writeByte('"');
    for (s) |c| {
        switch (c) {
            '"' => try w.writeAll("\\\""),
            '\\' => try w.writeAll("\\\\"),
            '\n' => try w.writeAll("\\n"),
            '\r' => try w.writeAll("\\r"),
            '\t' => try w.writeAll("\\t"),
            else => {
                if (c < 0x20) {
                    try w.print("\\u{x:0>4}", .{c});
                } else {
                    try w.writeByte(c);
                }
            },
        }
    }
    try w.writeByte('"');
}

// ---------------------------------------------------------------------------
// Method handlers.
// ---------------------------------------------------------------------------

fn handleInitialize(w: *std.Io.Writer, id_raw: []const u8) !void {
    try w.print(
        "{{\"jsonrpc\":\"2.0\",\"id\":{s},\"result\":{{" ++
            "\"name\":\"zig-self-contracts\"," ++
            "\"version\":\"1.0.0\"," ++
            "\"protocol_version\":\"provekit-lift/1\"," ++
            "\"capabilities\":{{" ++
            "\"authoring_surfaces\":[\"zig-self-contracts\"]," ++
            "\"ir_version\":\"v1.1.0\"," ++
            "\"emits_signed_mementos\":true}}}}}}\n",
        .{id_raw},
    );
}

fn handleLift(alloc: std.mem.Allocator, w: *std.Io.Writer, id_raw: []const u8) !void {
    var result = mint.mintSelfProof(alloc) catch |err| {
        try w.print(
            "{{\"jsonrpc\":\"2.0\",\"id\":{s},\"error\":{{\"code\":1005,\"message\":\"LIFT_FAILED: {s}\"}}}}\n",
            .{ id_raw, @errorName(err) },
        );
        return;
    };
    defer result.deinit(alloc);

    const b64 = try base64Encode(alloc, result.proof_bytes);
    defer alloc.free(b64);

    try w.print(
        "{{\"jsonrpc\":\"2.0\",\"id\":{s},\"result\":{{\"kind\":\"proof-envelope\",",
        .{id_raw},
    );
    try w.writeAll("\"filename_cid\":");
    try writeJsonString(w, result.filename_cid);
    try w.writeAll(",\"contract_set_cid\":");
    try writeJsonString(w, result.contract_set_cid);
    try w.writeAll(",\"bytes_base64\":");
    try writeJsonString(w, b64);
    try w.writeAll(",\"diagnostics\":[]}}\n");
}

fn handleShutdown(w: *std.Io.Writer, id_raw: []const u8) !void {
    try w.print(
        "{{\"jsonrpc\":\"2.0\",\"id\":{s},\"result\":null}}\n",
        .{id_raw},
    );
}

fn handleUnknown(w: *std.Io.Writer, id_raw: []const u8, method: []const u8) !void {
    try w.print(
        "{{\"jsonrpc\":\"2.0\",\"id\":{s},\"error\":{{\"code\":-32601,\"message\":\"METHOD_NOT_FOUND: {s}\"}}}}\n",
        .{ id_raw, method },
    );
}

// ---------------------------------------------------------------------------
// RPC dispatch loop.
// ---------------------------------------------------------------------------

fn runRpcMode(alloc: std.mem.Allocator, io: std.Io) !void {
    var read_buf: [READ_BUF]u8 = undefined;
    var write_buf: [WRITE_BUF]u8 = undefined;

    var stdin_file = std.Io.File.stdin().readerStreaming(io, &read_buf);
    var stdin_reader = &stdin_file.interface;

    var stdout_file = std.Io.File.stdout().writerStreaming(io, &write_buf);
    var stdout_writer = &stdout_file.interface;

    while (true) {
        const maybe_line = stdin_reader.takeDelimiter('\n') catch |err| switch (err) {
            error.StreamTooLong => {
                _ = stdin_reader.discardDelimiterInclusive('\n') catch break;
                continue;
            },
            error.ReadFailed => break,
        };
        const line_with_newline = maybe_line orelse break; // EOF -> graceful shutdown.
        // Discard trailing newline byte (takeDelimiter leaves it pending).
        stdin_reader.toss(@min(1, stdin_reader.bufferedLen()));

        // Strip any trailing CR/LF/whitespace.
        var line = line_with_newline;
        while (line.len > 0 and (line[line.len - 1] == '\n' or line[line.len - 1] == '\r' or line[line.len - 1] == ' ' or line[line.len - 1] == '\t')) {
            line = line[0 .. line.len - 1];
        }
        if (line.len == 0) continue;

        const req = parseReq(line);

        if (std.mem.eql(u8, req.method, "initialize")) {
            try handleInitialize(stdout_writer, req.id_raw);
        } else if (std.mem.eql(u8, req.method, "lift")) {
            try handleLift(alloc, stdout_writer, req.id_raw);
        } else if (std.mem.eql(u8, req.method, "shutdown")) {
            try handleShutdown(stdout_writer, req.id_raw);
            try stdout_writer.flush();
            return;
        } else {
            try handleUnknown(stdout_writer, req.id_raw, req.method);
        }
        try stdout_writer.flush();
    }
}

// ---------------------------------------------------------------------------
// CLI mode — banner + determinism check + write production .proof.
// ---------------------------------------------------------------------------

fn writeFile(io: std.Io, dir: std.Io.Dir, name: []const u8, bytes: []const u8) !void {
    const f = try dir.createFile(io, name, .{});
    defer f.close(io);
    try std.Io.File.writeStreamingAll(f, io, bytes);
}

fn runCliMode(alloc: std.mem.Allocator, io: std.Io, out_dir: []const u8) !void {
    const stderr_w = std.debug.print;

    // Mint #1 (determinism check).
    var det = try mint.mintSelfProof(alloc);
    errdefer det.deinit(alloc);
    // Mint #2 (production).
    var prod = try mint.mintSelfProof(alloc);
    defer {
        det.deinit(alloc);
        prod.deinit(alloc);
    }

    if (!std.mem.eql(u8, det.filename_cid, prod.filename_cid)) {
        stderr_w("ERROR: byte-determinism check FAILED:\n  run A CID: {s}\n  run B CID: {s}\n", .{ det.filename_cid, prod.filename_cid });
        return error.DeterminismFailure;
    }
    if (!std.mem.eql(u8, det.contract_set_cid, prod.contract_set_cid)) {
        stderr_w("ERROR: contractSetCid determinism check FAILED:\n  run A: {s}\n  run B: {s}\n", .{ det.contract_set_cid, prod.contract_set_cid });
        return error.DeterminismFailure;
    }

    try std.Io.Dir.createDirPath(std.Io.Dir.cwd(), io, out_dir);
    var dir = try std.Io.Dir.openDir(std.Io.Dir.cwd(), io, out_dir, .{});
    defer dir.close(io);

    const filename = try std.fmt.allocPrint(alloc, "{s}.proof", .{prod.filename_cid});
    defer alloc.free(filename);
    try writeFile(io, dir, filename, prod.proof_bytes);

    stderr_w("== ProvekIt Zig self-contracts orchestrator ==\n", .{});
    stderr_w("\noutput dir: {s}\n", .{out_dir});
    stderr_w("\nauthored:\n", .{});
    for (prod.per_source_counts) |lc| {
        stderr_w("  {s:<22}  {d:>2} contracts\n", .{ lc.label, lc.count });
    }
    stderr_w("  {s:<22}  {d:>2} contracts (TOTAL)\n", .{ "[ALL]", prod.total_contracts });
    stderr_w("\nminted:\n", .{});
    stderr_w("  .proof file:        {s}/{s}\n", .{ out_dir, filename });
    stderr_w("  bytes:              {d}\n", .{prod.proof_bytes.len});
    stderr_w("  total contracts:    {d}\n", .{prod.total_contracts});
    stderr_w("  catalog CID:        {s}\n", .{prod.filename_cid});
    stderr_w("  contractSetCid:     {s}\n", .{prod.contract_set_cid});
    stderr_w("  determinism check:  OK (two runs produced identical CIDs and contractSetCid)\n", .{});
    stderr_w("\n== done. Zig self-application: live. ==\n", .{});
}

// ---------------------------------------------------------------------------
// Entry point.
// ---------------------------------------------------------------------------

pub fn main(init: std.process.Init) !void {
    const alloc = init.gpa;
    const io = init.io;

    const args = try init.minimal.args.toSlice(init.arena.allocator());

    var rpc_mode = false;
    var out_dir: []const u8 = ".";
    var saw_positional = false;

    var i: usize = 1;
    while (i < args.len) : (i += 1) {
        const a = args[i];
        if (std.mem.eql(u8, a, "--rpc")) {
            rpc_mode = true;
        } else if (a.len > 0 and a[0] != '-' and !saw_positional) {
            out_dir = a;
            saw_positional = true;
        }
    }

    if (rpc_mode) {
        try runRpcMode(alloc, io);
    } else {
        try runCliMode(alloc, io, out_dir);
    }
}

// ---------------------------------------------------------------------------
// Tests — pull in sibling test files.
// ---------------------------------------------------------------------------

test {
    _ = @import("slab.zig");
    _ = @import("mint.zig");
}

test "parseReq extracts numeric id and method" {
    const r = parseReq("{\"jsonrpc\":\"2.0\",\"id\":7,\"method\":\"initialize\"}");
    try std.testing.expectEqualStrings("7", r.id_raw);
    try std.testing.expectEqualStrings("initialize", r.method);
}

test "parseReq extracts string id and method" {
    const r = parseReq("{\"jsonrpc\":\"2.0\",\"id\":\"abc\",\"method\":\"lift\"}");
    try std.testing.expectEqualStrings("\"abc\"", r.id_raw);
    try std.testing.expectEqualStrings("lift", r.method);
}

test "parseReq returns null id when missing" {
    const r = parseReq("{\"jsonrpc\":\"2.0\",\"method\":\"shutdown\"}");
    try std.testing.expectEqualStrings("null", r.id_raw);
    try std.testing.expectEqualStrings("shutdown", r.method);
}

test "base64Encode round-trip" {
    const out = try base64Encode(std.testing.allocator, "hello");
    defer std.testing.allocator.free(out);
    try std.testing.expectEqualStrings("aGVsbG8=", out);
}
