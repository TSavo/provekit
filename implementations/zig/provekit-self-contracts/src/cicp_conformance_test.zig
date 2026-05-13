// SPDX-License-Identifier: Apache-2.0
//
// CICP golden-vector coverage for the Zig language kit.
//
// The protocol/conformance/cicp vectors are intentionally data-only. This
// test embeds those bodies, parses them as semantic JSON, re-emits them through
// the native Zig JCS path, and derives the BLAKE3-512 CID from those bytes.

const std = @import("std");
const sc = @import("root.zig");
const vector_data = @import("cicp_vectors");

const Value = sc.jcs.Value;
const ObjectBuilder = sc.jcs.ObjectBuilder;

const Vector = struct {
    name: []const u8,
    body: []const u8,
    expected_cid: ?[]const u8,
    should_pass: bool,
};

const PASSING_VECTORS = [_]Vector{
    .{
        .name = "blast-radius-rust-kit",
        .body = vector_data.blast_radius_rust_kit,
        .expected_cid = "blake3-512:b46ed4acaa333e1c67d34435914543235529eee7beb8e70ca5075fd5d4417a3a5685625532e3631f81c65d748e6d0b354c158b5f8043dc70aa1eb654a4ee9550",
        .should_pass = true,
    },
    .{
        .name = "blast-radius-rust-kit-next-catalog",
        .body = vector_data.blast_radius_rust_kit_next_catalog,
        .expected_cid = "blake3-512:add810e8496aa2b72de4db0e15f789a76e092b13dd0233d7e0e78c4155d17916f5a6104cfa011af734eec760f1c627bd13357cd64d1670407686d9136ac1f7b8",
        .should_pass = true,
    },
    .{
        .name = "job-result-pass",
        .body = vector_data.job_result_pass,
        .expected_cid = "blake3-512:1c426f1cc560a02623931abd9349b855150f64507d1f5a231312fb0c017fe38a9224f60dc611087aaffb225411cbc5dbe936a2b2a469546387a5f304052cc141",
        .should_pass = true,
    },
    .{
        .name = "reuse-identical",
        .body = vector_data.reuse_identical,
        .expected_cid = "blake3-512:4236da5414741b5c24e2347e5308ee60adf764ccf741f97865f0e149f2869547bde3e3e5d8d5b43e7be0389fc97ec2e4bcee37bfe5797f907db84337e921c961",
        .should_pass = true,
    },
    .{
        .name = "reuse-bridged-by-evolution",
        .body = vector_data.reuse_bridged_by_evolution,
        .expected_cid = "blake3-512:1c83f9e79533e2c0254ec66e76e1478e332fc6c72ea751566f3698976c7050269a45cb149af0f22c931edb3d5724b7df112fe89df93c47c750b0718cc0b16dbd",
        .should_pass = true,
    },
    .{
        .name = "impact-protocol-extension-only",
        .body = vector_data.impact_protocol_extension_only,
        .expected_cid = "blake3-512:53f2eed2f4b6b87f62ae3348d5686ec9177c140965110abacaa5a68330fc27dbc6bca42673798bc0ab7f1e9a665b74aa4fa39900fd10ae0258a5a0efedcd817e",
        .should_pass = true,
    },
};

const INVALID_VECTOR = Vector{
    .name = "invalid-blast-radius-open-input-closure",
    .body = vector_data.invalid_blast_radius_open_input_closure,
    .expected_cid = null,
    .should_pass = false,
};

test "CICP manifest coverage stays in sync with embedded Zig vectors" {
    const alloc = std.testing.allocator;
    const Manifest = struct {
        catalogVersion: []const u8,
        catalogCid: []const u8,
        protocol: []const u8,
        vectors: []struct {
            name: []const u8,
            capability: []const u8,
            body: []const u8,
            expectedCid: ?[]const u8 = null,
            shouldPass: bool,
            errorContains: ?[]const u8 = null,
        },
    };

    var parsed = try std.json.parseFromSlice(Manifest, alloc, vector_data.manifest, .{});
    defer parsed.deinit();

    try std.testing.expectEqualStrings("v1.6.2-2026-05-07", parsed.value.catalogVersion);
    try std.testing.expectEqualStrings("content-addressed-ci-protocol", parsed.value.protocol);
    try std.testing.expectEqual(@as(usize, PASSING_VECTORS.len + 1), parsed.value.vectors.len);

    var passing_count: usize = 0;
    var failing_count: usize = 0;
    for (parsed.value.vectors) |manifest_vector| {
        if (manifest_vector.shouldPass) {
            passing_count += 1;
            try std.testing.expect(manifest_vector.expectedCid != null);
        } else {
            failing_count += 1;
            try std.testing.expectEqualStrings(INVALID_VECTOR.name, manifest_vector.name);
            try std.testing.expectEqualStrings("inputCids missing required CID", manifest_vector.errorContains.?);
        }
    }

    try std.testing.expectEqual(@as(usize, PASSING_VECTORS.len), passing_count);
    try std.testing.expectEqual(@as(usize, 1), failing_count);
}

test "CICP golden vectors derive the pinned BLAKE3-512 CIDs" {
    const alloc = std.testing.allocator;

    inline for (PASSING_VECTORS) |vector| {
        try std.testing.expect(vector.should_pass);

        var parsed = try std.json.parseFromSlice(std.json.Value, alloc, vector.body, .{});
        defer parsed.deinit();

        const root = try jsonToJcs(alloc, parsed.value);
        defer {
            root.deinit(alloc);
            alloc.destroy(root);
        }

        try validateInputClosure(parsed.value);

        const jcs_bytes = try sc.jcs.encode(alloc, root);
        defer alloc.free(jcs_bytes);

        const cid = try sc.hash.blake3_512Of(alloc, jcs_bytes);
        defer alloc.free(cid);

        try std.testing.expectEqualStrings(vector.expected_cid.?, cid);
    }
}

test "CICP invalid vector fails closed on missing inputCids dependency" {
    const alloc = std.testing.allocator;
    try std.testing.expect(!INVALID_VECTOR.should_pass);

    var parsed = try std.json.parseFromSlice(std.json.Value, alloc, INVALID_VECTOR.body, .{});
    defer parsed.deinit();

    try std.testing.expectError(error.InputCidsMissingRequiredCid, validateInputClosure(parsed.value));
}

fn jsonToJcs(alloc: std.mem.Allocator, value: std.json.Value) !*Value {
    return switch (value) {
        .null => Value.newNull(alloc),
        .bool => |b| Value.newBool(alloc, b),
        .integer => |n| Value.newInt(alloc, n),
        .string => |s| Value.newString(alloc, s),
        .array => |array| blk: {
            var builder = sc.jcs.ArrayBuilder.init(alloc);
            errdefer {
                for (builder.items.items) |child| {
                    child.deinit(alloc);
                    alloc.destroy(child);
                }
                builder.items.deinit(alloc);
            }

            for (array.items) |item| {
                try builder.append(try jsonToJcs(alloc, item));
            }
            break :blk builder.finish();
        },
        .object => |object| blk: {
            var builder = ObjectBuilder.init(alloc);
            errdefer {
                for (builder.pairs.items) |pair| {
                    alloc.free(pair.key);
                    pair.value.deinit(alloc);
                    alloc.destroy(pair.value);
                }
                builder.pairs.deinit(alloc);
            }

            var it = object.iterator();
            while (it.next()) |entry| {
                try builder.add(entry.key_ptr.*, try jsonToJcs(alloc, entry.value_ptr.*));
            }
            break :blk builder.finish();
        },
        .float, .number_string => error.UnsupportedCicpNumber,
    };
}

fn validateInputClosure(body: std.json.Value) !void {
    const object = switch (body) {
        .object => |object| object,
        else => return error.ExpectedObject,
    };

    const input_cids_value = object.get("inputCids") orelse return error.MissingInputCids;
    const input_cids = switch (input_cids_value) {
        .array => |array| array,
        else => return error.InputCidsNotArray,
    };

    if (object.get("kind")) |kind_value| {
        if (kind_value == .string and std.mem.eql(u8, kind_value.string, "CIBlastRadius")) {
            try requireCidField(object, input_cids, "protocolCatalogCid");
            try requireCidField(object, input_cids, "jobDefinitionCid");
            try requireCidField(object, input_cids, "commandCid");
            try requireCidField(object, input_cids, "runnerIdentityCid");
            try requireCidField(object, input_cids, "sourceClosureCid");
            try requireCidField(object, input_cids, "policyCid");
            try requireCidArrayField(object, input_cids, "toolchainCids");
            try requireCidArrayField(object, input_cids, "lockfileCids");
            try requireCidArrayField(object, input_cids, "generatedInputCids");
            try requireCidArrayField(object, input_cids, "fixtureCids");
            try requireCidArrayField(object, input_cids, "relevantSpecCids");
        }
    }
}

fn requireCidField(object: std.json.ObjectMap, input_cids: std.json.Array, field_name: []const u8) !void {
    const value = object.get(field_name) orelse return error.MissingRequiredCidField;
    const cid = switch (value) {
        .string => |s| s,
        else => return error.RequiredCidFieldNotString,
    };
    try requireInputCid(input_cids, cid);
}

fn requireCidArrayField(object: std.json.ObjectMap, input_cids: std.json.Array, field_name: []const u8) !void {
    const value = object.get(field_name) orelse return error.MissingRequiredCidField;
    const array = switch (value) {
        .array => |array| array,
        else => return error.RequiredCidFieldNotArray,
    };

    for (array.items) |item| {
        const cid = switch (item) {
            .string => |s| s,
            else => return error.RequiredCidFieldNotString,
        };
        try requireInputCid(input_cids, cid);
    }
}

fn requireInputCid(input_cids: std.json.Array, required: []const u8) !void {
    for (input_cids.items) |item| {
        if (item == .string and std.mem.eql(u8, item.string, required)) return;
    }
    return error.InputCidsMissingRequiredCid;
}
