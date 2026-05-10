// SPDX-License-Identifier: Apache-2.0
//
// Cross-kit conformance bridges (Phase 2): zig kit -> rust kit's
// `lift_plugin_protocol` slab. Mirrors PR #89 (python), PR #92 (go), and
// PR #93 (typescript).
//
// Phase 1 (PR #84) added 10 contracts to the rust self-contracts bundle
// encoding the rules of `protocol/specs/2026-04-30-lift-plugin-protocol.md`
// (v1.2.0 normative). Phase 2 mints, in each peer kit, 10 counterpart
// `ContractDecl`s plus 10 `BridgeDecl`s linking each rust contract (by its
// envelope CID) to its peer-kit counterpart (by its JCS+BLAKE3-512 CID).
//
// Pattern choice: zig follows the **python** pattern, not ts (no signed-
// CBOR envelope pipeline in zig) and not go (no orchestrator-time CID
// resolution in zig). Specifically:
//
//   * Counterpart contract: `ContractDecl` with
//       `inv = atomic("satisfies",
//                     [str_const("zig-lift-adapter"),
//                      str_const(<rust_contract_name>)])`
//     The verifier resolves "satisfies" through the bridge.
//
//   * Counterpart CID = `blake3-512(JCS(ContractDecl))` via `jcsStringify`
//     + `jcsHash`: the helpers already shipped in `root.zig`. The python
//     and zig kits do NOT re-implement rust's signed-CBOR envelope, so
//     peer counterpart CIDs and rust contract CIDs are NOT in the same
//     identity space; the bridge wires them by content.
//
//   * `BridgeDecl.target_contract_cid` = the paired counterpart's CID.
//   * `BridgeDecl.target_proof_cid` = `"deferred:phase-3-proof-bundle"`
//     (phase-3 binary attestation will replace it).
//   * `BridgeDecl.target_layer` = `"zig-kit"`.
//   * `BridgeDecl.source_layer` = `"rust-kit"`.
//   * `BridgeDecl.source_contract_cid` = pinned rust envelope CID
//     (extracted via `cargo run --release -p provekit-self-contracts
//     --bin print-lift-plugin-protocol-cids`; pasted from the python /
//     go / ts kits' pinned tables: they all read the same rust mint).
//
// Test design:
//   * Pin every counterpart CID and the bridges-array CID. Drift in any
//     of the rust source CIDs, the counterpart shape, the JCS emitter,
//     or the BLAKE3 hasher surfaces here with a clear cause.
//   * "bridges array" hash = `blake3-512(JCS(<10 BridgeDecl values
//     wrapped as Decl.bridge>))`. The task statement says "pin the
//     BLAKE3-512 of the bridges array." We pin just the bridge array,
//     not a mixed [counterpart, bridge, ...] array; counterpart CIDs are
//     pinned individually so a counterpart-shape drift surfaces with a
//     specific message.

const std = @import("std");
const root = @import("root.zig");

const Decl = root.Decl;
const Term = root.Term;
const Atomic = root.Atomic;
const Str = root.Str;
const jcsStringify = root.jcsStringify;
const jcsHash = root.jcsHash;

// ---------------------------------------------------------------------------
// Layer / proof constants
// ---------------------------------------------------------------------------

pub const RUST_KIT_LAYER: []const u8 = "rust-kit";
pub const ZIG_KIT_LAYER: []const u8 = "zig-kit";
pub const ZIG_LIFT_ADAPTER_ID: []const u8 = "zig-lift-adapter";
pub const DEFERRED_PROOF_CID: []const u8 = "deferred:phase-3-proof-bundle";
pub const PHASE_2_BRIDGE_NOTES: []const u8 =
    "lift-plugin-protocol conformance bridge; phase 2";

// ---------------------------------------------------------------------------
// Rust contract CID table (pinned)
// ---------------------------------------------------------------------------
//
// Source: `cargo run --release -p provekit-self-contracts \
//          --bin print-lift-plugin-protocol-cids`: the rust kit's
// extraction binary that walks `mint_self_proof()` and prints
// `(contract_name, cid)` pairs as NDJSON for the lift_plugin_protocol slab.
//
// These 10 CIDs are also pinned in:
//   - implementations/python/.../tests/test_cross_kit_bridges.py
//   - implementations/go/provekit-self-contracts/slabs/lift_plugin_protocol.go
//   - implementations/typescript/src/lift/cross-kit-bridges.invariant.ts
//
// Insertion order is the rust slab's `lift_plugin_protocol::invariants()`
// declaration order. Bridges are emitted in this order to keep the
// bridges-array byte-stable.

pub const LIFT_PLUGIN_PROTOCOL_NAMES = [_][]const u8{
    "lift_plugin_initialize_protocol_version_match",
    "lift_plugin_initialize_capabilities_authoring_surfaces_nonempty",
    "lift_plugin_initialize_capabilities_ir_version_starts_with_v",
    "lift_plugin_lift_request_surface_is_string",
    "lift_plugin_lift_request_source_paths_nonempty",
    "lift_plugin_lift_request_source_paths_each_nonempty",
    "lift_plugin_lift_request_surface_in_capabilities",
    "lift_plugin_lift_response_kind_in_set",
    "lift_plugin_lift_response_ir_document_array",
    "lift_plugin_diagnostic_field_is_array",
};

pub const RUST_CONTRACT_CIDS = [_][]const u8{
    "blake3-512:95163d00976803c3ef381494a8a940bd862529f7bdfb72aa523bd58359b86d6fce017991658932e3e3dee8b4c60b26066bfa270474b2896c19dd2ec85d4aa47a",
    "blake3-512:1898e2518e96628bbe46704f6f6a90cc57572f3b15bb3f4f6a7d8fef28a8c92e31b33b14f21d4011ed7ad11d4ea09c67c1549cbe1c2bf38e53b7e8cfdb656099",
    "blake3-512:08d09e6f677e77f5b501a07a5271cebdadb19c48c52375ae9e6edcb699b6515eacdea2d7966497c3b3aca4054340e7222fe97bbbb8f60e2ee62baaec6ef719f0",
    "blake3-512:bf6ac4f7e481ba1fea26716f9d2e7756c86b1940610e2d9e35a5d6e11faa8993a92cd291f491c4d520e5daf1a54c32aeb492adac5aa8d61d224ca1104adaaf8a",
    "blake3-512:3f2915b063357c28cd2bd8132279e819424999b21a776824d3db9231ca4acb8fdc02ea6e5a8945e55a1d439fda94d07b365d0d160e4ece94b1012fe064ca7c22",
    "blake3-512:f57621c2ba995cbd13d9d06c4209ad9ecdb6369d1e90d902b90996275dd40a38804c986b77b9f28bdf7eefc2b0f242284d1612a4149f5abb0451097a72f95822",
    "blake3-512:61c67906e3b2ff0d0a61419436670140009556402b643516c4afb14212c057a080bf6f29a0c4c374fe2eb45f8016ddfc82ed12fae2735c7384a8b56a7597db51",
    "blake3-512:7642bd5eb5262354921513ee6e01bf70dad917f3467464ad904750685e84d0241ef9b0f40b6e0d66dd73e0d5cc1908e4a0a45d45530dda511e1919786034e2a0",
    "blake3-512:692df8b67bc3ad69943f5909779f489bdc8173bbb08fd61585bb1b8bc0a2c20c6891ba7b9a2a4e4e3a6e5a4441b1191f4618924783446cb07277879c885cbc20",
    "blake3-512:ea5dd139fddc9e5ab6cfcb9854de1ce6bbedcccbe7b070c1aef9fbbef3b8579ebf33ff14cdc97013e1f3e1c391964f275a0275b615b8259037b0cb92d0e0dd35",
};

comptime {
    if (LIFT_PLUGIN_PROTOCOL_NAMES.len != 10) @compileError("expected 10 lift-plugin-protocol contract names");
    if (RUST_CONTRACT_CIDS.len != 10) @compileError("expected 10 rust contract CIDs");
    if (LIFT_PLUGIN_PROTOCOL_NAMES.len != RUST_CONTRACT_CIDS.len)
        @compileError("rust contract names / CIDs length mismatch");
}

// ---------------------------------------------------------------------------
// Counterpart + bridge name helpers
// ---------------------------------------------------------------------------

/// Counterpart contract name builder: `zig_<rust_name>_counterpart`.
/// Allocated; caller frees.
pub fn counterpartName(alloc: std.mem.Allocator, rust_contract_name: []const u8) ![]u8 {
    return std.fmt.allocPrint(alloc, "zig_{s}_counterpart", .{rust_contract_name});
}

/// Bridge name builder: `bridge_to_<rust_name>`. Matches the python /
/// go / ts kits' convention. Allocated; caller frees.
pub fn bridgeName(alloc: std.mem.Allocator, rust_contract_name: []const u8) ![]u8 {
    return std.fmt.allocPrint(alloc, "bridge_to_{s}", .{rust_contract_name});
}

// ---------------------------------------------------------------------------
// Counterpart construction
// ---------------------------------------------------------------------------

/// Build the zig counterpart `ContractDecl` for the given rust contract.
///
/// Shape (matching python's `_counterpart_contract`):
///
///   ContractDecl {
///       name: "zig_<rust_name>_counterpart",
///       inv: atomic("satisfies",
///                   [str_const("zig-lift-adapter"), str_const(<rust_name>)])
///   }
///
/// The atomic args slice is owned by the caller; this function takes a
/// pointer to a 2-element `[]Term` slot the caller has already populated.
/// Inv-only (no pre/post) so the JCS shape is `{kind, name, outBinding, inv}`.
pub fn buildCounterpartDecl(
    name: []const u8,
    inv_args: *const [2]Term,
) Decl {
    return Decl{ .contract = .{
        .name = name,
        .out_binding = "out",
        .inv = Atomic("satisfies", inv_args),
    } };
}

/// Build the zig `BridgeDecl` for the given rust contract.
pub fn buildBridgeDecl(
    name: []const u8,
    rust_contract_name: []const u8,
    rust_contract_cid: []const u8,
    target_contract_cid: []const u8,
) Decl {
    return Decl{ .bridge = .{
        .name = name,
        .source_symbol = rust_contract_name,
        .source_layer = RUST_KIT_LAYER,
        .source_contract_cid = rust_contract_cid,
        .target_contract_cid = target_contract_cid,
        .target_proof_cid = DEFERRED_PROOF_CID,
        .target_layer = ZIG_KIT_LAYER,
        .notes = PHASE_2_BRIDGE_NOTES,
    } };
}

// ---------------------------------------------------------------------------
// Pinned counterpart CIDs (one per rust contract, in slab order)
// ---------------------------------------------------------------------------

pub const PINNED_COUNTERPART_CIDS = [_][]const u8{
    "blake3-512:a0b9eab3da3d8fe9a61c3ff19b501d24e5e281f74f943f0ba6e5d03b1a89886d48d44020c9eb8d37f6720af2efd3c529c9ab232b46b94becc6200e37dd5eecbb",
    "blake3-512:68dbc7624fe5052823bd749f2ef046ab7528d0e1f6d3c9911a2966e0d597fa70091e8b06bb2cb91f71a69936508f03fa08a60ff15e12b7ce5dc05221b71706c1",
    "blake3-512:315c8116c4f91760bbc6520e3e5649bbff20f8a57442638794304e10d6e273ccd02b63c9e818892fb70a0228577b3cdb1dd674b7cddc801124f561d0d2b2d6d5",
    "blake3-512:b638b7e519c8eaba540e51aa2067eed838ae889cafa946ae089c386924ad89b3b975e63d4eba1fac2f7e663a01da08a0deb3ee2dbf35c77e0ecee2fa2afc4dfe",
    "blake3-512:8449bc3ef8ca85d03c7afad4117f8e08ba8379e9698e6dc9ee25e9eb436e7e2ce86189dbfda65ec6518a08206fd073dabba46478fae02ee8d8b444fe73895982",
    "blake3-512:227a41326dda814465c3fcd631a309c8c3c65229f595c633ea9ae1d25f61cd376cb8fe5bb4f6ef9bc1726d758d0fa8416f040c4c833e7dbc6aba21500b6061b3",
    "blake3-512:7d4f7f87bcdd29c05ff09551243da73df73f3838882fc0c7240eebd883130c4b748897f3a68c005fd7b50c934dcbca68b936d407806a193e8959d26b544fea77",
    "blake3-512:d61ba3ef362c5a0c0439589569a3ab4e75963533ed857baf7f3843148b907b36072e55c45281b2e7520077e86f16e366b0b84a7f4dfa56b7b61f2cd2615e9ae9",
    "blake3-512:9b0875e4d31e4c33be27aeb62c5eae8a25a5af67153056b9d34bce727ff4c163d9df7e6eef699edd45cb1451b10ee612bedbae7e209a6135473ff642d736272e",
    "blake3-512:b9c045ac98f1dbc264aa7f079bdc597300df1b5c36dddd2e7ad4a647e98a6c2a9f2cbca1c44655ee604ff6bbaf8841db27fb7fd706a5d1bffc86d345a9fc5d8f",
};

// ---------------------------------------------------------------------------
// Pinned bridges-array CID
// ---------------------------------------------------------------------------
//
// `blake3-512(JCS(<10 BridgeDecl values, in slab order>))`. JCS-canonical
// bytes are emitted as a JSON array via `std.json.Stringify.valueAlloc`
// over a `[]const Decl` slice; the per-element JCS shape comes from
// `Decl.jsonStringify`.

pub const PINNED_BRIDGES_ARRAY_CID: []const u8 =
    "blake3-512:de79fef648fdfb8c3651aae038e9695fb91786b7c638110244e262d48351cbbeca819e32545da07d825171540684e165094d7bdced8aeecbbf55be0a51858bc0";

// ---------------------------------------------------------------------------
// Test support: build all decls into caller-provided arenas
// ---------------------------------------------------------------------------

/// Bundle returned by `buildAll`. All slices live in `arena`.
pub const BuiltBridges = struct {
    counterpart_names: [10][]u8,
    counterpart_decls: [10]Decl,
    counterpart_cids: [10][]u8,
    bridge_names: [10][]u8,
    bridges: [10]Decl,
};

/// Build counterparts (with computed CIDs) and bridges. Caller owns
/// `arena.allocator()`-backed strings and slices via the arena.
///
/// Memory layout: every []u8 (names, CIDs) and every backing Term array
/// for atomic-formula args is allocated through `arena`. The returned
/// `BuiltBridges` is a value struct containing only Decls + slices that
/// point into the arena, so it stays valid for the arena's lifetime.
pub fn buildAll(arena: *std.heap.ArenaAllocator) !BuiltBridges {
    const aa = arena.allocator();

    var out: BuiltBridges = undefined;

    // Backing storage for the per-counterpart `Atomic("satisfies", &[2]Term)`
    // args lives in the arena, NOT on this function's stack: `out.bridges`
    // is returned by value and references these slices via each
    // counterpart Decl's `inv` formula. Arena lifetime > buildAll lifetime.
    const inv_args_storage = try aa.alloc([2]Term, 10);

    for (LIFT_PLUGIN_PROTOCOL_NAMES, 0..) |rust_name, i| {
        const cp_name = try counterpartName(aa, rust_name);
        out.counterpart_names[i] = cp_name;

        inv_args_storage[i] = [_]Term{
            Str(ZIG_LIFT_ADAPTER_ID),
            Str(rust_name),
        };

        out.counterpart_decls[i] = buildCounterpartDecl(
            cp_name,
            &inv_args_storage[i],
        );

        const jcs = try jcsStringify(aa, out.counterpart_decls[i]);
        out.counterpart_cids[i] = try jcsHash(aa, jcs);
    }

    for (LIFT_PLUGIN_PROTOCOL_NAMES, 0..) |rust_name, i| {
        const br_name = try bridgeName(aa, rust_name);
        out.bridge_names[i] = br_name;

        out.bridges[i] = buildBridgeDecl(
            br_name,
            rust_name,
            RUST_CONTRACT_CIDS[i],
            out.counterpart_cids[i],
        );
    }

    return out;
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

test "lift-plugin-protocol cross-kit bridges: 10 counterpart + 10 bridge decls" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();

    const built = try buildAll(&arena);

    try std.testing.expectEqual(@as(usize, 10), built.counterpart_decls.len);
    try std.testing.expectEqual(@as(usize, 10), built.bridges.len);

    for (built.counterpart_decls, 0..) |d, i| {
        try std.testing.expect(d == .contract);
        try std.testing.expectEqualStrings(built.counterpart_names[i], d.contract.name);
        try std.testing.expect(d.contract.inv != null);
        try std.testing.expect(d.contract.pre == null);
        try std.testing.expect(d.contract.post == null);
    }

    for (built.bridges, 0..) |d, i| {
        try std.testing.expect(d == .bridge);
        const b = d.bridge;
        try std.testing.expectEqualStrings(LIFT_PLUGIN_PROTOCOL_NAMES[i], b.source_symbol);
        try std.testing.expectEqualStrings(RUST_KIT_LAYER, b.source_layer);
        try std.testing.expectEqualStrings(ZIG_KIT_LAYER, b.target_layer);
        try std.testing.expectEqualStrings(DEFERRED_PROOF_CID, b.target_proof_cid);
        try std.testing.expectEqualStrings(RUST_CONTRACT_CIDS[i], b.source_contract_cid);
        try std.testing.expectEqualStrings(built.counterpart_cids[i], b.target_contract_cid);
        try std.testing.expect(b.notes != null);
        try std.testing.expectEqualStrings(PHASE_2_BRIDGE_NOTES, b.notes.?);
    }
}

test "lift-plugin-protocol cross-kit bridges: counterpart names follow zig_*_counterpart" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();

    const built = try buildAll(&arena);

    for (built.counterpart_names, 0..) |name, i| {
        try std.testing.expect(std.mem.startsWith(u8, name, "zig_"));
        try std.testing.expect(std.mem.endsWith(u8, name, "_counterpart"));
        try std.testing.expect(std.mem.indexOf(u8, name, LIFT_PLUGIN_PROTOCOL_NAMES[i]) != null);
    }

    for (built.bridge_names, 0..) |name, i| {
        try std.testing.expect(std.mem.startsWith(u8, name, "bridge_to_"));
        try std.testing.expect(std.mem.indexOf(u8, name, LIFT_PLUGIN_PROTOCOL_NAMES[i]) != null);
    }
}

test "lift-plugin-protocol cross-kit bridges: rust contract CIDs are blake3-512 well-formed" {
    for (RUST_CONTRACT_CIDS) |cid| {
        try std.testing.expect(std.mem.startsWith(u8, cid, "blake3-512:"));
        const hex = cid["blake3-512:".len..];
        try std.testing.expectEqual(@as(usize, 128), hex.len);
        for (hex) |c| {
            const ok = (c >= '0' and c <= '9') or (c >= 'a' and c <= 'f');
            try std.testing.expect(ok);
        }
    }
}

test "lift-plugin-protocol cross-kit bridges: counterpart CIDs match pins" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();

    const built = try buildAll(&arena);

    for (built.counterpart_cids, 0..) |actual, i| {
        try std.testing.expectEqualStrings(PINNED_COUNTERPART_CIDS[i], actual);
    }
}

test "lift-plugin-protocol cross-kit bridges: bridges-array CID matches pin" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();

    const built = try buildAll(&arena);

    // Encode the 10 bridges as a single JSON array via Decl.jsonStringify.
    const aa = arena.allocator();
    const jcs = try jcsStringify(aa, built.bridges[0..]);
    const cid = try jcsHash(aa, jcs);

    try std.testing.expectEqualStrings(PINNED_BRIDGES_ARRAY_CID, cid);
}
