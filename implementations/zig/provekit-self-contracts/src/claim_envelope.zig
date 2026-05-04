// SPDX-License-Identifier: Apache-2.0
//
// claim_envelope — `mint_contract` / `mint_bridge` build a signed memento
// in the v1.2 LAYERED shape introduced by
// `protocol/specs/2026-05-03-substrate-layers-envelope-header-body.md`:
//
//   { "envelope": {...}, "header": {...}, "metadata": {...} }
//
//   * envelope = { signer, declaredAt, signature }
//       The signature is computed over JCS({"header": header, "metadata": metadata}).
//       The envelope's CID (= attestation CID) is BLAKE3-512(JCS(envelope))
//       AFTER the signature has been embedded.
//
//   * header   = substrate-load-bearing data the verifier reads:
//                schemaVersion, kind, cid, plus kind-specific REQUIRED
//                fields (per the kind's normative spec) and the derived
//                hashes (bindingHash, propertyHash, verdict, inputCids)
//                used by the resolve/index pipeline.
//
//   * metadata = everything else (authoring attribution, lifecycle
//                strings like producedBy/producedAt, derived per-formula
//                hashes that are pure tooling convenience). Opaque to
//                the substrate verifier; signed transitively via the
//                envelope.
//
// Mirrors implementations/rust/provekit-claim-envelope/src/lib.rs 1:1.

const std = @import("std");
const jcs = @import("jcs.zig");
const hash = @import("hash.zig");
const signing = @import("signing.zig");

const Value = jcs.Value;
const ObjectBuilder = jcs.ObjectBuilder;
const ArrayBuilder = jcs.ArrayBuilder;

pub const LAYERED_SCHEMA_VERSION: []const u8 = "2";

pub const Error = error{
    EmptyContract,
    EmptyOutBinding,
    OutOfMemory,
};

/// Result of a mint_* call. All slices owned by the caller (allocated
/// through the same allocator the caller passed in).
pub const MintedEnvelope = struct {
    /// JCS-canonical bytes of the full layered memento
    /// (`{envelope, header, metadata}`).
    canonical_bytes: []u8,
    /// The attestation CID: BLAKE3-512(JCS(envelope)) after the signature
    /// has been embedded. Identifies the SIGNED attestation.
    cid: []u8,
    /// The content CID: BLAKE3-512(JCS({name, outBinding, pre?, post?, inv?})).
    /// Signer-independent. Empty for bridges.
    contract_cid: []u8,

    pub fn deinit(self: MintedEnvelope, alloc: std.mem.Allocator) void {
        alloc.free(self.canonical_bytes);
        alloc.free(self.cid);
        alloc.free(self.contract_cid);
    }
};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Hash a Value tree: BLAKE3-512(JCS(v)).
fn hashValue(alloc: std.mem.Allocator, v: *const Value) ![]u8 {
    const enc = try jcs.encode(alloc, v);
    defer alloc.free(enc);
    return hash.blake3_512Of(alloc, enc);
}

/// Build the JCS-canonical bytes of `{"header": header, "metadata": metadata}`.
/// This is the message the envelope's Ed25519 signature covers (spec §2 R2).
fn signingBytes(alloc: std.mem.Allocator, header: *Value, metadata: *Value) ![]u8 {
    // Manual JCS emission to avoid ownership transfer:
    //   {"header":<JCS(header)>,"metadata":<JCS(metadata)>}
    // Since "header" < "metadata" in byte order, this is the JCS form.
    var buf: std.ArrayList(u8) = .empty;
    defer buf.deinit(alloc);
    try buf.appendSlice(alloc, "{\"header\":");
    {
        const enc_h = try jcs.encode(alloc, header);
        defer alloc.free(enc_h);
        try buf.appendSlice(alloc, enc_h);
    }
    try buf.appendSlice(alloc, ",\"metadata\":");
    {
        const enc_m = try jcs.encode(alloc, metadata);
        defer alloc.free(enc_m);
        try buf.appendSlice(alloc, enc_m);
    }
    try buf.append(alloc, '}');
    return buf.toOwnedSlice(alloc);
}

/// Assemble a layered memento, sign it, compute the attestation CID
/// (= BLAKE3-512(JCS(envelope-with-signature))). Returns the JCS-canonical
/// bytes of the full `{envelope, header, metadata}` object.
///
/// `header` and `metadata` are consumed: their child Values are folded into
/// the returned canonical bytes, then the trees are freed.
fn assembleLayered(
    alloc: std.mem.Allocator,
    header: *Value,
    metadata: *Value,
    declared_at: []const u8,
    signer_seed: signing.Seed,
    content_cid: []u8,
) !MintedEnvelope {
    errdefer alloc.free(content_cid);

    // Sign over JCS({header, metadata}).
    const sign_msg = try signingBytes(alloc, header, metadata);
    defer alloc.free(sign_msg);
    const signature_str = try signing.signString(alloc, signer_seed, sign_msg);
    defer alloc.free(signature_str);
    const signer_str = try signing.pubkeyString(alloc, signer_seed);
    defer alloc.free(signer_str);

    // Build envelope object: {signer, declaredAt, signature}
    // (JCS sorts by code-point at emit time; here we just feed in any order.)
    var envelope_b = ObjectBuilder.init(alloc);
    try envelope_b.add("signer", try Value.newString(alloc, signer_str));
    try envelope_b.add("declaredAt", try Value.newString(alloc, declared_at));
    try envelope_b.add("signature", try Value.newString(alloc, signature_str));
    const envelope_v = try envelope_b.finish();
    // envelope_v owned — we need to encode it (for attestation CID), then
    // re-use it as part of the outer memento.

    const envelope_jcs = try jcs.encode(alloc, envelope_v);
    defer alloc.free(envelope_jcs);
    const attestation_cid = try hash.blake3_512Of(alloc, envelope_jcs);
    errdefer alloc.free(attestation_cid);

    // Build outer memento {envelope, header, metadata}; this transfers
    // ownership of envelope_v / header / metadata to the new tree.
    var outer_b = ObjectBuilder.init(alloc);
    try outer_b.add("envelope", envelope_v);
    try outer_b.add("header", header);
    try outer_b.add("metadata", metadata);
    const outer = try outer_b.finish();
    defer {
        outer.deinit(alloc);
        alloc.destroy(outer);
    }
    const memento_bytes = try jcs.encode(alloc, outer);
    return .{
        .canonical_bytes = memento_bytes,
        .cid = attestation_cid,
        .contract_cid = content_cid,
    };
}

/// Build the kind/cid/schemaVersion-prefixed header object. Takes ownership
/// of every Value in `kind_specific`.
fn buildHeader(
    alloc: std.mem.Allocator,
    kind: []const u8,
    header_cid: []const u8,
    kind_specific: []const KV,
) !*Value {
    var b = ObjectBuilder.init(alloc);
    try b.add("schemaVersion", try Value.newString(alloc, LAYERED_SCHEMA_VERSION));
    try b.add("kind", try Value.newString(alloc, kind));
    try b.add("cid", try Value.newString(alloc, header_cid));
    for (kind_specific) |kv| {
        try b.add(kv.key, kv.value);
    }
    return b.finish();
}

/// Owning (key, value) tuple used by the kind-specific field list.
pub const KV = struct {
    key: []const u8,
    value: *Value,
};

// ---------------------------------------------------------------------------
// mint_contract
// ---------------------------------------------------------------------------

pub const Authoring = union(enum) {
    kit_author: struct {
        author: []const u8,
        note: ?[]const u8 = null,
    },
    lift: struct {
        lifter: []const u8,
        evidence: []const u8,
        source_cid: ?[]const u8 = null,
    },
    llm: struct {
        llm: []const u8,
        llm_version: []const u8,
        prompt_cid: []const u8,
        confidence: f64,
        rationale: ?[]const u8 = null,
    },
};

fn authoringToValue(alloc: std.mem.Allocator, a: Authoring) !*Value {
    var b = ObjectBuilder.init(alloc);
    switch (a) {
        .kit_author => |k| {
            try b.add("producerKind", try Value.newString(alloc, "kit-author"));
            try b.add("author", try Value.newString(alloc, k.author));
            if (k.note) |n| if (n.len > 0) try b.add("note", try Value.newString(alloc, n));
        },
        .lift => |l| {
            try b.add("producerKind", try Value.newString(alloc, "lift"));
            try b.add("lifter", try Value.newString(alloc, l.lifter));
            try b.add("evidence", try Value.newString(alloc, l.evidence));
            if (l.source_cid) |c| if (c.len > 0) try b.add("sourceCid", try Value.newString(alloc, c));
        },
        .llm => |m| {
            try b.add("producerKind", try Value.newString(alloc, "llm"));
            try b.add("llm", try Value.newString(alloc, m.llm));
            try b.add("llmVersion", try Value.newString(alloc, m.llm_version));
            try b.add("promptCid", try Value.newString(alloc, m.prompt_cid));
            // Match the rust kit's confidence encoding: integer with 3-decimal scale.
            const conf_int: i64 = @intFromFloat(m.confidence * 1000.0);
            try b.add("confidence", try Value.newInt(alloc, conf_int));
            if (m.rationale) |r| if (r.len > 0) try b.add("rationale", try Value.newString(alloc, r));
        },
    }
    return b.finish();
}

pub const MintContractArgs = struct {
    contract_name: []const u8,
    /// pre, post, inv: at least one of the three MUST be present. Caller
    /// passes ownership of the *Value to mint_contract; the function frees
    /// them on every code path.
    pre: ?*Value = null,
    post: ?*Value = null,
    inv: ?*Value = null,
    out_binding: []const u8,
    produced_by: []const u8,
    produced_at: []const u8,
    input_cids: []const []const u8 = &.{},
    authoring: Authoring,
    signer_seed: signing.Seed,
};

/// Compute the **content** CID of a contract (signer-independent). Per spec
/// `contract-cid-vs-attestation-cid.md` §1: BLAKE3-512(JCS({name, outBinding,
/// pre?, post?, inv?})). Two distinct signers attesting to the same logical
/// contract produce the same CID.
///
/// Caller owns the returned slice. Does NOT consume `args.pre/post/inv`.
pub fn contractCid(alloc: std.mem.Allocator, args: MintContractArgs) ![]u8 {
    var b = ObjectBuilder.init(alloc);
    errdefer {
        // ObjectBuilder doesn't expose a "discard without freeing children"
        // path; if we error before finishing, the keys we added are leaked.
        // The clones below are the only Values added; on success they're
        // transferred to the finished object.
    }
    try b.add("name", try Value.newString(alloc, args.contract_name));
    try b.add("outBinding", try Value.newString(alloc, args.out_binding));
    if (args.pre) |p| try b.add("pre", try cloneValue(alloc, p));
    if (args.post) |p| try b.add("post", try cloneValue(alloc, p));
    if (args.inv) |p| try b.add("inv", try cloneValue(alloc, p));
    const v = try b.finish();
    defer {
        v.deinit(alloc);
        alloc.destroy(v);
    }
    return hashValue(alloc, v);
}

/// Deep-clone a Value tree. Used because the hash + header + property-hash
/// computations all need access to the same pre/post/inv subtrees, but
/// each consumer wants ownership.
fn cloneValue(alloc: std.mem.Allocator, v: *const Value) std.mem.Allocator.Error!*Value {
    return switch (v.*) {
        .null_ => Value.newNull(alloc),
        .bool_ => |b| Value.newBool(alloc, b),
        .integer => |n| Value.newInt(alloc, n),
        .string => |s| Value.newString(alloc, s),
        .array => |items| blk: {
            var ab = ArrayBuilder.init(alloc);
            for (items) |child| try ab.append(try cloneValue(alloc, child));
            break :blk try ab.finish();
        },
        .object => |entries| blk: {
            var ob = ObjectBuilder.init(alloc);
            for (entries) |pair| try ob.add(pair.key, try cloneValue(alloc, pair.value));
            break :blk try ob.finish();
        },
    };
}

/// Compute the **contract set CID** from a slice of already-computed
/// contractCid strings. Sort lex on the raw `blake3-512:hex` strings; CID
/// is BLAKE3-512(JCS(<sorted array>)). Order-independent.
pub fn computeContractSetCid(alloc: std.mem.Allocator, contract_cids: [][]const u8) ![]u8 {
    // Sort a duplicated slice (don't mutate caller's input).
    const sorted = try alloc.dupe([]const u8, contract_cids);
    defer alloc.free(sorted);
    std.mem.sort([]const u8, sorted, {}, slicesLessThan);

    var ab = ArrayBuilder.init(alloc);
    for (sorted) |cid| try ab.append(try Value.newString(alloc, cid));
    const v = try ab.finish();
    defer {
        v.deinit(alloc);
        alloc.destroy(v);
    }
    const enc = try jcs.encode(alloc, v);
    defer alloc.free(enc);
    return hash.blake3_512Of(alloc, enc);
}

fn slicesLessThan(_: void, a: []const u8, b: []const u8) bool {
    return std.mem.lessThan(u8, a, b);
}

/// Mint a contract memento. Consumes args.pre/post/inv on every path.
pub fn mintContract(
    alloc: std.mem.Allocator,
    args: MintContractArgs,
) !MintedEnvelope {
    // Free pre/post/inv on every error path. On success they're cloned into
    // the returned envelope's bytes (we free them at end).
    const pre = args.pre;
    const post = args.post;
    const inv = args.inv;
    defer {
        if (pre) |p| {
            p.deinit(alloc);
            alloc.destroy(p);
        }
        if (post) |p| {
            p.deinit(alloc);
            alloc.destroy(p);
        }
        if (inv) |p| {
            p.deinit(alloc);
            alloc.destroy(p);
        }
    }

    if (pre == null and post == null and inv == null) return Error.EmptyContract;
    if (args.out_binding.len == 0) return Error.EmptyOutBinding;

    // DERIVED hashes:
    //   propertyHash = hash(JCS({pre?, post?, inv?, outBinding}))
    //   bindingHash  = hash(JCS({producerId, contractName, propertyHash}))
    var ph_b = ObjectBuilder.init(alloc);
    if (pre) |p| try ph_b.add("pre", try cloneValue(alloc, p));
    if (post) |p| try ph_b.add("post", try cloneValue(alloc, p));
    if (inv) |p| try ph_b.add("inv", try cloneValue(alloc, p));
    try ph_b.add("outBinding", try Value.newString(alloc, args.out_binding));
    const ph_v = try ph_b.finish();
    const property_hash = blk: {
        defer {
            ph_v.deinit(alloc);
            alloc.destroy(ph_v);
        }
        break :blk try hashValue(alloc, ph_v);
    };
    defer alloc.free(property_hash);

    var bh_b = ObjectBuilder.init(alloc);
    try bh_b.add("producerId", try Value.newString(alloc, args.produced_by));
    try bh_b.add("contractName", try Value.newString(alloc, args.contract_name));
    try bh_b.add("propertyHash", try Value.newString(alloc, property_hash));
    const bh_v = try bh_b.finish();
    const binding_hash = blk: {
        defer {
            bh_v.deinit(alloc);
            alloc.destroy(bh_v);
        }
        break :blk try hashValue(alloc, bh_v);
    };
    defer alloc.free(binding_hash);

    // Header: kind-specific REQUIRED fields, in spec order.
    const header_cid = try contractCid(alloc, args);
    errdefer alloc.free(header_cid);

    // sorted_inputs (lex)
    const sorted_inputs = try alloc.dupe([]const u8, args.input_cids);
    defer alloc.free(sorted_inputs);
    std.mem.sort([]const u8, sorted_inputs, {}, slicesLessThan);

    var inputs_ab = ArrayBuilder.init(alloc);
    for (sorted_inputs) |c| try inputs_ab.append(try Value.newString(alloc, c));
    const inputs_arr = try inputs_ab.finish();

    var kind_specific: std.ArrayList(KV) = .empty;
    defer kind_specific.deinit(alloc);
    try kind_specific.append(alloc, .{ .key = "name", .value = try Value.newString(alloc, args.contract_name) });
    try kind_specific.append(alloc, .{ .key = "outBinding", .value = try Value.newString(alloc, args.out_binding) });
    if (pre) |p| {
        try kind_specific.append(alloc, .{ .key = "pre", .value = try cloneValue(alloc, p) });
    }
    if (post) |p| {
        try kind_specific.append(alloc, .{ .key = "post", .value = try cloneValue(alloc, p) });
    }
    if (inv) |p| {
        try kind_specific.append(alloc, .{ .key = "inv", .value = try cloneValue(alloc, p) });
    }
    try kind_specific.append(alloc, .{ .key = "verdict", .value = try Value.newString(alloc, "holds") });
    try kind_specific.append(alloc, .{ .key = "bindingHash", .value = try Value.newString(alloc, binding_hash) });
    try kind_specific.append(alloc, .{ .key = "propertyHash", .value = try Value.newString(alloc, property_hash) });
    try kind_specific.append(alloc, .{ .key = "inputCids", .value = inputs_arr });

    const header = try buildHeader(alloc, "contract", header_cid, kind_specific.items);

    // Metadata.
    var meta_b = ObjectBuilder.init(alloc);
    try meta_b.add("authoring", try authoringToValue(alloc, args.authoring));
    try meta_b.add("producedBy", try Value.newString(alloc, args.produced_by));
    try meta_b.add("producedAt", try Value.newString(alloc, args.produced_at));
    if (pre) |p| {
        const ph = try hashValue(alloc, p);
        defer alloc.free(ph);
        try meta_b.add("preHash", try Value.newString(alloc, ph));
    }
    if (post) |p| {
        const ph = try hashValue(alloc, p);
        defer alloc.free(ph);
        try meta_b.add("postHash", try Value.newString(alloc, ph));
    }
    if (inv) |p| {
        const ph = try hashValue(alloc, p);
        defer alloc.free(ph);
        try meta_b.add("invHash", try Value.newString(alloc, ph));
    }
    const metadata = try meta_b.finish();

    // assembleLayered consumes header + metadata + content_cid.
    return assembleLayered(
        alloc,
        header,
        metadata,
        args.produced_at,
        args.signer_seed,
        header_cid,
    );
}

// ---------------------------------------------------------------------------
// mint_bridge_v14 (v1.4 layered envelope/header/body, tagged-union target)
//
// Per protocol/specs/2026-05-03-bridge-target-dimensionality.md §1.R1-R6.
// Canonical reference: rust/provekit-claim-envelope/src/lib.rs fn mint_bridge_v14.
// ---------------------------------------------------------------------------

pub const BridgeTargetKind = enum { contract, contractSet };

pub const BridgeTargetV14 = struct {
    kind: BridgeTargetKind,
    cid: []const u8,
};

pub const MintBridgeV14Args = struct {
    // header (7 fields per §1.R3)
    name: []const u8,
    source_symbol: []const u8,
    source_layer: []const u8,
    source_contract_cid: []const u8,
    target: BridgeTargetV14,

    // metadata (null_string = omit per §1.R2)
    target_witness_cid: ?[]const u8 = null,
    target_binary_cid: ?[]const u8 = null,
    target_layer: ?[]const u8 = null,
    target_contract_set_cid: ?[]const u8 = null,
    produced_by: ?[]const u8 = null,
    produced_at: ?[]const u8 = null,

    declared_at: []const u8,
    signer_seed: signing.Seed,
};

pub fn mintBridgeV14(alloc: std.mem.Allocator, args: MintBridgeV14Args) !MintedEnvelope {
    // Build target
    const target_kind = if (args.target.kind == .contract) "contract" else "contractSet";
    var t_b = ObjectBuilder.init(alloc);
    try t_b.add("kind", try Value.newString(alloc, target_kind));
    try t_b.add("cid", try Value.newString(alloc, args.target.cid));
    const target_v = try t_b.finish();
    defer {
        target_v.deinit(alloc);
        alloc.destroy(target_v);
    }

    // Build header (7 canonical fields)
    var h_b = ObjectBuilder.init(alloc);
    try h_b.add("schemaVersion", try Value.newString(alloc, "1"));
    try h_b.add("kind", try Value.newString(alloc, "bridge"));
    try h_b.add("name", try Value.newString(alloc, args.name));
    try h_b.add("sourceSymbol", try Value.newString(alloc, args.source_symbol));
    try h_b.add("sourceLayer", try Value.newString(alloc, args.source_layer));
    try h_b.add("sourceContractCid", try Value.newString(alloc, args.source_contract_cid));
    try h_b.add("target", try cloneValue(alloc, target_v));
    const header = try h_b.finish();
    defer {
        header.deinit(alloc);
        alloc.destroy(header);
    }

    // Build metadata (only non-null fields)
    var m_b = ObjectBuilder.init(alloc);
    if (args.target_witness_cid) |v| try m_b.add("targetWitnessCid", try Value.newString(alloc, v));
    if (args.target_binary_cid) |v| try m_b.add("targetBinaryCid", try Value.newString(alloc, v));
    if (args.target_layer) |v| try m_b.add("targetLayer", try Value.newString(alloc, v));
    if (args.target_contract_set_cid) |v| try m_b.add("targetContractSetCid", try Value.newString(alloc, v));
    if (args.produced_by) |v| try m_b.add("producedBy", try Value.newString(alloc, v));
    if (args.produced_at) |v| try m_b.add("producedAt", try Value.newString(alloc, v));
    const metadata = try m_b.finish();
    defer {
        metadata.deinit(alloc);
        alloc.destroy(metadata);
    }

    // Sign: JCS({header, metadata})
    const sig_bytes = try signingBytes(alloc, header, metadata);
    defer alloc.free(sig_bytes);
    const sig = try signing.signWithSeed(args.signer_seed, sig_bytes);
    defer alloc.free(sig);
    const sig_b64 = try alloc.alloc(u8, std.base64.standard.Encoder.calcSize(sig.len));
    defer alloc.free(sig_b64);
    _ = std.base64.standard.Encoder.encode(sig_b64, sig);
    const sig_str = try std.fmt.allocPrint(alloc, "ed25519:{s}", .{sig_b64});
    defer alloc.free(sig_str);

    // Build envelope
    const pubkey = try signing.pubkeyString(alloc, args.signer_seed);
    defer alloc.free(pubkey);
    var e_b = ObjectBuilder.init(alloc);
    try e_b.add("signer", try Value.newString(alloc, pubkey));
    try e_b.add("declaredAt", try Value.newString(alloc, args.declared_at));
    try e_b.add("signature", try Value.newString(alloc, sig_str));
    const envelope = try e_b.finish();
    defer {
        envelope.deinit(alloc);
        alloc.destroy(envelope);
    }

    // Full memento: {envelope, header, metadata}
    var mem_b = ObjectBuilder.init(alloc);
    try mem_b.add("envelope", try cloneValue(alloc, envelope));
    try mem_b.add("header", try cloneValue(alloc, header));
    try mem_b.add("metadata", try cloneValue(alloc, metadata));
    const memento = try mem_b.finish();
    defer {
        memento.deinit(alloc);
        alloc.destroy(memento);
    }

    const canonical = try jcs.encode(alloc, memento);
    const cid = try hash.blake3_512Of(alloc, canonical);

    return MintedEnvelope{
        .canonical_bytes = canonical,
        .cid = cid,
        .contract_cid = &[_]u8{},
    };
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

test "empty contract rejected" {
    const alloc = std.testing.allocator;
    const args = MintContractArgs{
        .contract_name = "x",
        .out_binding = "out",
        .produced_by = "test",
        .produced_at = "2026-04-30T00:00:00.000Z",
        .authoring = .{ .kit_author = .{ .author = "test" } },
        .signer_seed = @splat(0x42),
    };
    try std.testing.expectError(Error.EmptyContract, mintContract(alloc, args));
}

test "empty out binding rejected" {
    const alloc = std.testing.allocator;
    const pre = try Value.newBool(alloc, true);
    const args = MintContractArgs{
        .contract_name = "x",
        .pre = pre,
        .out_binding = "",
        .produced_by = "test",
        .produced_at = "2026-04-30T00:00:00.000Z",
        .authoring = .{ .kit_author = .{ .author = "test" } },
        .signer_seed = @splat(0x42),
    };
    try std.testing.expectError(Error.EmptyOutBinding, mintContract(alloc, args));
}

test "minimal contract mint produces well-formed CID" {
    const alloc = std.testing.allocator;
    // pre = atomic(">") — we just feed a small Value; full IR-shape ergonomics
    // live in the sibling provekit-ir package.
    var pre_b = ObjectBuilder.init(alloc);
    try pre_b.add("kind", try Value.newString(alloc, "atomic"));
    try pre_b.add("name", try Value.newString(alloc, ">"));
    var args_arr = ArrayBuilder.init(alloc);
    {
        var v_b = ObjectBuilder.init(alloc);
        try v_b.add("kind", try Value.newString(alloc, "var"));
        try v_b.add("name", try Value.newString(alloc, "n"));
        try args_arr.append(try v_b.finish());
    }
    {
        var v_b = ObjectBuilder.init(alloc);
        try v_b.add("kind", try Value.newString(alloc, "const"));
        try v_b.add("value", try Value.newInt(alloc, 0));
        var sort_b = ObjectBuilder.init(alloc);
        try sort_b.add("kind", try Value.newString(alloc, "primitive"));
        try sort_b.add("name", try Value.newString(alloc, "Int"));
        try v_b.add("sort", try sort_b.finish());
        try args_arr.append(try v_b.finish());
    }
    try pre_b.add("args", try args_arr.finish());
    const pre = try pre_b.finish();

    const args = MintContractArgs{
        .contract_name = "parseInt",
        .pre = pre,
        .out_binding = "out",
        .produced_by = "zig-kit@1.0",
        .produced_at = "2026-04-30T00:00:00.000Z",
        .authoring = .{ .kit_author = .{ .author = "zig-kit@1.0" } },
        .signer_seed = @splat(0x42),
    };
    const m = try mintContract(alloc, args);
    defer m.deinit(alloc);
    try std.testing.expect(std.mem.startsWith(u8, m.cid, "blake3-512:"));
    try std.testing.expectEqual(@as(usize, "blake3-512:".len + 128), m.cid.len);
    try std.testing.expect(std.mem.startsWith(u8, m.contract_cid, "blake3-512:"));
}

test "compute contract set CID order-independent" {
    const alloc = std.testing.allocator;
    var ids_a = [_][]const u8{
        "blake3-512:bbb",
        "blake3-512:aaa",
        "blake3-512:ccc",
    };
    var ids_b = [_][]const u8{
        "blake3-512:ccc",
        "blake3-512:aaa",
        "blake3-512:bbb",
    };
    const a = try computeContractSetCid(alloc, &ids_a);
    defer alloc.free(a);
    const b = try computeContractSetCid(alloc, &ids_b);
    defer alloc.free(b);
    try std.testing.expectEqualStrings(a, b);
}

test "empty contract set CID matches the value pinned in attestation" {
    // The pinned value in .provekit/self-contracts-attestations/zig.json is
    // the contractSetCid for the empty contract array — i.e. JCS([]) hashed.
    const alloc = std.testing.allocator;
    const empty: [][]const u8 = &.{};
    const cid = try computeContractSetCid(alloc, @constCast(empty));
    defer alloc.free(cid);
    try std.testing.expectEqualStrings(
        "blake3-512:d53d18c23212ea7b6300594bb89bce60218f6eff2b9d628b8cc42d3e79bbd5ab09994845815cc7185113418f9fc2edc7606b06f0d57a6d581e7cff5b290f3229",
        cid,
    );
}
