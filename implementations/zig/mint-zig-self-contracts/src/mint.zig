// SPDX-License-Identifier: Apache-2.0
//
// mint.zig — slab walker + content CID + proof envelope assembler.
//
// What this module DOES:
//   1. Walks `slab.authorAll` to gather the canonical contract set.
//   2. For each contract, computes the spec #94 §1 signer-independent
//      content CID:
//        contractCid = blake3-512(JCS({name, outBinding, pre?, post?, inv?}))
//   3. Computes contractSetCid = blake3-512(JCS(<sorted contractCids>)).
//   4. Mints each contract as a signed layered memento using the native
//      Zig claim-envelope substrate, then bundles those mementos into a
//      `.proof` catalog envelope.
//   5. Emits the .proof bytes + filename CID + contractSetCid to caller.
//
// Cross-kit anchor: the contractSetCid for the same authored contracts
// is signer-independent across all kits per spec #94 §1. The .proof
// filename CID depends on the signed memento set + name + version +
// declaredAt + signer; it is byte-deterministic across runs but is NOT
// expected to match other kits.

const std = @import("std");
const provekit = @import("provekit-ir");
const proof_env = @import("provekit-proof-envelope-zig");
const sc = @import("provekit-self-contracts");

const slab = @import("slab.zig");
const ScValue = sc.jcs.Value;
const ObjectBuilder = sc.jcs.ObjectBuilder;
const ArrayBuilder = sc.jcs.ArrayBuilder;

pub const PRODUCED_BY: []const u8 = "@provekit/zig-self-contracts@1.0";
pub const DECLARED_AT: []const u8 = "2026-04-30T18:00:00.000Z";
pub const CATALOG_NAME: []const u8 = "@provekit/zig-self-contracts";
pub const CATALOG_VERSION: []const u8 = "1.0.0";

pub const MintResult = struct {
    /// `<full-self-identifying>.proof` filename CID
    /// (`blake3-512:<128 hex>`). Caller owns.
    filename_cid: []u8,
    /// Signer-independent contract set CID (spec #94 §1). Caller owns.
    contract_set_cid: []u8,
    /// .proof bytes (CBOR catalog). Caller owns.
    proof_bytes: []u8,
    /// Number of contracts authored.
    total_contracts: usize,
    /// Per-source-file count for the human-readable banner.
    per_source_counts: []LabeledCount,

    pub fn deinit(self: *MintResult, alloc: std.mem.Allocator) void {
        alloc.free(self.filename_cid);
        alloc.free(self.contract_set_cid);
        alloc.free(self.proof_bytes);
        for (self.per_source_counts) |lc| alloc.free(lc.label);
        alloc.free(self.per_source_counts);
    }
};

pub const LabeledCount = struct {
    label: []u8,
    count: usize,
};

// ---------------------------------------------------------------------------
// Spec #94 §1: contractCid + contractSetCid.
// ---------------------------------------------------------------------------

/// Build the spec-mandated JCS-canonicalizable shape for one contract.
///
/// Field order is alphabetical per RFC 8785; std.json.Stringify with
/// `.whitespace = .minified` together with the IR types' jsonStringify
/// hooks emits keys in alphabetical order, matching what every other
/// kit does (rust's `provekit_claim_envelope::contract_cid`, go's
/// `claim_envelope.ContractCIDFromArgs`, ts's `contractCidFromArgs`).
///
/// We hand-build a small struct shape with only the four spec fields
/// (name + outBinding + optional pre/post/inv). Optional fields that
/// are null are OMITTED, matching every other kit's behavior under
/// JCS (`{"a":null}` is NOT equivalent to `{}` under spec #94).
fn jcsForContractContent(
    alloc: std.mem.Allocator,
    contract: provekit.Decl.ContractDecl,
) ![]u8 {
    // Build the JCS string by hand to control which fields are present.
    // Keys must be in alphabetical order: inv, name, outBinding, post, pre.
    var buf: std.ArrayList(u8) = .empty;
    errdefer buf.deinit(alloc);

    try buf.append(alloc, '{');
    var emitted_any = false;

    if (contract.inv) |inv| {
        if (emitted_any) try buf.append(alloc, ',');
        emitted_any = true;
        try buf.appendSlice(alloc, "\"inv\":");
        const v = try provekit.jcsStringify(alloc, inv);
        defer alloc.free(v);
        try buf.appendSlice(alloc, v);
    }

    {
        if (emitted_any) try buf.append(alloc, ',');
        emitted_any = true;
        try buf.appendSlice(alloc, "\"name\":");
        const v = try provekit.jcsStringify(alloc, contract.name);
        defer alloc.free(v);
        try buf.appendSlice(alloc, v);
    }

    {
        if (emitted_any) try buf.append(alloc, ',');
        emitted_any = true;
        try buf.appendSlice(alloc, "\"outBinding\":");
        const v = try provekit.jcsStringify(alloc, contract.out_binding);
        defer alloc.free(v);
        try buf.appendSlice(alloc, v);
    }

    if (contract.post) |post| {
        if (emitted_any) try buf.append(alloc, ',');
        emitted_any = true;
        try buf.appendSlice(alloc, "\"post\":");
        const v = try provekit.jcsStringify(alloc, post);
        defer alloc.free(v);
        try buf.appendSlice(alloc, v);
    }

    if (contract.pre) |pre| {
        if (emitted_any) try buf.append(alloc, ',');
        try buf.appendSlice(alloc, "\"pre\":");
        const v = try provekit.jcsStringify(alloc, pre);
        defer alloc.free(v);
        try buf.appendSlice(alloc, v);
    }

    try buf.append(alloc, '}');
    return buf.toOwnedSlice(alloc);
}

/// Compute one contract's signer-independent contentCid.
fn contractContentCid(
    alloc: std.mem.Allocator,
    contract: provekit.Decl.ContractDecl,
) ![]u8 {
    const jcs_bytes = try jcsForContractContent(alloc, contract);
    defer alloc.free(jcs_bytes);
    return provekit.jcsHash(alloc, jcs_bytes);
}

/// Compute the contract set CID per spec #94 §1.
///   contractSetCid := blake3-512(JCS(<sorted contractCids>))
/// Sort is bytewise lex on the cid strings; result is signer-independent.
fn computeContractSetCid(
    alloc: std.mem.Allocator,
    cids: [][]const u8,
) ![]u8 {
    // Sort in-place.
    std.mem.sort([]const u8, cids, {}, struct {
        fn lt(_: void, a: []const u8, b: []const u8) bool {
            return std.mem.lessThan(u8, a, b);
        }
    }.lt);
    // JCS-encode as a JSON array of strings. std.json.Stringify on
    // `[][]const u8` emits the alphabetical-sorted-by-content array
    // format with no whitespace; that IS JCS for arrays-of-strings.
    const jcs_bytes = try provekit.jcsStringify(alloc, cids);
    defer alloc.free(jcs_bytes);
    return provekit.jcsHash(alloc, jcs_bytes);
}

// ---------------------------------------------------------------------------
// IR -> provekit-self-contracts JCS Value conversion.
// ---------------------------------------------------------------------------

fn sortToValue(alloc: std.mem.Allocator, sort: provekit.Sort) !*ScValue {
    var b = ObjectBuilder.init(alloc);
    switch (sort) {
        .primitive => |name| {
            try b.add("kind", try ScValue.newString(alloc, "primitive"));
            try b.add("name", try ScValue.newString(alloc, name));
        },
        .function => |f| {
            var args = ArrayBuilder.init(alloc);
            for (f.args) |arg| try args.append(try sortToValue(alloc, arg.*));
            try b.add("args", try args.finish());
            try b.add("kind", try ScValue.newString(alloc, "function"));
            try b.add("return", try sortToValue(alloc, f.return_.*));
        },
        .dependent => |d| {
            try b.add("indexSort", try sortToValue(alloc, d.index_sort.*));
            try b.add("indexVar", try ScValue.newString(alloc, d.index_var));
            try b.add("kind", try ScValue.newString(alloc, "dependent"));
            try b.add("name", try ScValue.newString(alloc, d.name));
        },
        .region => |r| {
            try b.add("kind", try ScValue.newString(alloc, "region"));
            try b.add("name", try ScValue.newString(alloc, r.name));
        },
    }
    return b.finish();
}

fn constValueToValue(alloc: std.mem.Allocator, value: provekit.Term.ConstValue) !*ScValue {
    return switch (value) {
        .int => |n| ScValue.newInt(alloc, n),
        .string => |s| ScValue.newString(alloc, s),
        .bool => |b| ScValue.newBool(alloc, b),
        .null_void => ScValue.newNull(alloc),
    };
}

fn termToValue(alloc: std.mem.Allocator, term: provekit.Term) !*ScValue {
    var b = ObjectBuilder.init(alloc);
    switch (term) {
        .var_term => |t| {
            try b.add("kind", try ScValue.newString(alloc, "var"));
            try b.add("name", try ScValue.newString(alloc, t.name));
        },
        .const_term => |t| {
            try b.add("kind", try ScValue.newString(alloc, "const"));
            try b.add("sort", try sortToValue(alloc, t.sort));
            try b.add("value", try constValueToValue(alloc, t.value));
        },
        .ctor_term => |t| {
            var args = ArrayBuilder.init(alloc);
            for (t.args) |arg| try args.append(try termToValue(alloc, arg));
            try b.add("args", try args.finish());
            try b.add("kind", try ScValue.newString(alloc, "ctor"));
            try b.add("name", try ScValue.newString(alloc, t.name));
        },
    }
    return b.finish();
}

fn connectiveKindName(kind: provekit.Formula.ConnectiveKind) []const u8 {
    return switch (kind) {
        .@"and" => "and",
        .@"or" => "or",
        .not => "not",
        .implies => "implies",
    };
}

fn quantifierKindName(kind: provekit.Formula.QuantifierKind) []const u8 {
    return switch (kind) {
        .forall => "forall",
        .exists => "exists",
    };
}

fn formulaToValue(alloc: std.mem.Allocator, formula: provekit.Formula) !*ScValue {
    var b = ObjectBuilder.init(alloc);
    switch (formula) {
        .atomic => |f| {
            var args = ArrayBuilder.init(alloc);
            for (f.args) |arg| try args.append(try termToValue(alloc, arg));
            try b.add("args", try args.finish());
            try b.add("kind", try ScValue.newString(alloc, "atomic"));
            try b.add("name", try ScValue.newString(alloc, f.name));
        },
        .connective => |f| {
            var operands = ArrayBuilder.init(alloc);
            for (f.operands) |operand| try operands.append(try formulaToValue(alloc, operand));
            try b.add("kind", try ScValue.newString(alloc, connectiveKindName(f.kind)));
            try b.add("operands", try operands.finish());
        },
        .quantifier => |f| {
            try b.add("body", try formulaToValue(alloc, f.body.*));
            try b.add("kind", try ScValue.newString(alloc, quantifierKindName(f.kind)));
            try b.add("name", try ScValue.newString(alloc, f.name));
            try b.add("sort", try sortToValue(alloc, f.sort));
        },
    }
    return b.finish();
}

// ---------------------------------------------------------------------------
// Public entry point.
// ---------------------------------------------------------------------------

pub fn mintSelfProof(alloc: std.mem.Allocator) !MintResult {
    var authoring = try slab.authorAll(alloc);
    defer authoring.deinit();

    // 1. Mint every contract as a real signed memento and collect its
    // signer-independent content CID for contractSetCid.
    var cids_list: std.ArrayList([]u8) = .empty;
    defer {
        for (cids_list.items) |c| alloc.free(c);
        cids_list.deinit(alloc);
    }
    var members_list: std.ArrayList(proof_env.Member) = .empty;
    defer {
        for (members_list.items) |m| {
            alloc.free(m.cid);
            alloc.free(m.bytes);
        }
        members_list.deinit(alloc);
    }
    var per_source: std.ArrayList(LabeledCount) = .empty;
    errdefer {
        for (per_source.items) |lc| alloc.free(lc.label);
        per_source.deinit(alloc);
    }

    var total: usize = 0;
    for (authoring.slabs) |s| {
        const label_dup = try alloc.dupe(u8, s.label);
        try per_source.append(alloc, .{ .label = label_dup, .count = s.contracts.len });
        for (s.contracts) |d| {
            const c = d.contract;
            var minted = try sc.claim_envelope.mintContract(alloc, .{
                .contract_name = c.name,
                .pre = if (c.pre) |pre| try formulaToValue(alloc, pre) else null,
                .post = if (c.post) |post| try formulaToValue(alloc, post) else null,
                .inv = if (c.inv) |inv| try formulaToValue(alloc, inv) else null,
                .out_binding = c.out_binding,
                .produced_by = PRODUCED_BY,
                .produced_at = DECLARED_AT,
                .input_cids = &.{},
                .authoring = .{ .kit_author = .{
                    .author = PRODUCED_BY,
                    .note = "self-contract from zig slab",
                } },
                .signer_seed = sc.foundation.SEED,
            });
            var minted_transferred = false;
            errdefer if (!minted_transferred) minted.deinit(alloc);
            try cids_list.append(alloc, minted.contract_cid);
            try members_list.append(alloc, .{
                .cid = minted.cid,
                .bytes = minted.canonical_bytes,
            });
            minted_transferred = true;
        }
        total += s.contracts.len;
    }

    // 2. Compute contractSetCid (sorts `cids_list` in place).
    // Build a []const u8 view for the sort+JCS.
    var cids_view: std.ArrayList([]const u8) = .empty;
    defer cids_view.deinit(alloc);
    try cids_view.ensureTotalCapacity(alloc, cids_list.items.len);
    for (cids_list.items) |c| cids_view.appendAssumeCapacity(c);
    const contract_set_cid = try computeContractSetCid(alloc, cids_view.items);
    errdefer alloc.free(contract_set_cid);

    // 3. Build the .proof envelope from the kit-emitted mementos.
    const signer_pubkey = try proof_env.sign.pubkeyString(alloc, proof_env.FOUNDATION_V0_SEED);
    defer alloc.free(signer_pubkey);
    const signer_cid = try proof_env.blake3_512_of(alloc, signer_pubkey);
    defer alloc.free(signer_cid);

    const built = try proof_env.buildProofEnvelope(alloc, .{
        .name = CATALOG_NAME,
        .version = CATALOG_VERSION,
        .members = members_list.items,
        .signer_cid = signer_cid,
        .declared_at = DECLARED_AT,
        .signer_seed = proof_env.FOUNDATION_V0_SEED,
    });
    // built owns bytes + cid; we hand them to the caller.

    return .{
        .filename_cid = built.cid,
        .contract_set_cid = contract_set_cid,
        .proof_bytes = built.bytes,
        .total_contracts = total,
        .per_source_counts = try per_source.toOwnedSlice(alloc),
    };
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

const testing = std.testing;

test "mintSelfProof produces a content-meaningful contractSetCid" {
    var r = try mintSelfProof(testing.allocator);
    defer r.deinit(testing.allocator);

    // Must NOT be the empty-set sentinel.
    const empty_set_cid = "blake3-512:d53d18c23212ea7b6300594bb89bce60218f6eff2b9d628b8cc42d3e79bbd5ab09994845815cc7185113418f9fc2edc7606b06f0d57a6d581e7cff5b290f3229";
    try testing.expect(!std.mem.eql(u8, r.contract_set_cid, empty_set_cid));
    try testing.expect(std.mem.startsWith(u8, r.contract_set_cid, "blake3-512:"));
    try testing.expect(std.mem.startsWith(u8, r.filename_cid, "blake3-512:"));
    try testing.expect(r.total_contracts > 0);
    try testing.expect(r.proof_bytes.len > 0);
    try testing.expect(r.proof_bytes.len > 1024);
}

test "mintSelfProof is byte-deterministic" {
    var a = try mintSelfProof(testing.allocator);
    defer a.deinit(testing.allocator);
    var b = try mintSelfProof(testing.allocator);
    defer b.deinit(testing.allocator);

    try testing.expectEqualStrings(a.filename_cid, b.filename_cid);
    try testing.expectEqualStrings(a.contract_set_cid, b.contract_set_cid);
    try testing.expectEqualSlices(u8, a.proof_bytes, b.proof_bytes);
}
