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
//   4. Builds a `.proof` catalog envelope using the just-landed Side B
//      crypto substrate (provekit-proof-envelope-zig). Member set is
//      empty in this tier-3 bootstrap because zig has no claim-envelope
//      analog yet (no signed-memento minting). The contractSetCid is
//      content-meaningful regardless.
//   5. Emits the .proof bytes + filename CID + contractSetCid to caller.
//
// Cross-kit anchor: the contractSetCid for the same authored contracts
// is signer-independent across all kits per spec #94 §1. The .proof
// filename CID depends on the empty member set + name + version +
// declaredAt + signer; it is byte-deterministic across runs but is NOT
// expected to match other kits (those bundle real signed mementos).
//
// Followups (out of scope for #213):
//   * Port claim-envelope to zig (mint signed mementos).
//   * Add closed-loop bridge declaration.

const std = @import("std");
const provekit = @import("provekit-ir");
const proof_env = @import("provekit-proof-envelope-zig");

const slab = @import("slab.zig");

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
// Public entry point.
// ---------------------------------------------------------------------------

pub fn mintSelfProof(alloc: std.mem.Allocator) !MintResult {
    var authoring = try slab.authorAll(alloc);
    defer authoring.deinit();

    // 1. Compute every contract's content CID.
    var cids_list: std.ArrayList([]u8) = .empty;
    defer {
        for (cids_list.items) |c| alloc.free(c);
        cids_list.deinit(alloc);
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
            const cid = try contractContentCid(alloc, c);
            try cids_list.append(alloc, cid);
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

    // 3. Build the .proof envelope. Members map is empty in this Tier-3
    //    bootstrap (no claim-envelope substrate yet); the contractSetCid
    //    above is the cross-kit content-meaningful anchor.
    const signer_pubkey = try proof_env.sign.pubkeyString(alloc, proof_env.FOUNDATION_V0_SEED);
    defer alloc.free(signer_pubkey);
    const signer_cid = try proof_env.blake3_512_of(alloc, signer_pubkey);
    defer alloc.free(signer_cid);

    const built = try proof_env.buildProofEnvelope(alloc, .{
        .name = CATALOG_NAME,
        .version = CATALOG_VERSION,
        .members = &.{},
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
