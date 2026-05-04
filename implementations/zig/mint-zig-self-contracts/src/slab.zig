// SPDX-License-Identifier: Apache-2.0
//
// slab.zig — canonical self-contracts for the zig kit.
//
// Mirrors the canonical contract set authored by other Side A kits
// (rust, go, cpp, ts) about the kit's public crypto+canonicalization
// surface. The contract names + pre/post predicates are the kit's
// authored ground truth; the contractSetCid (signer-independent
// blake3-512(JCS(sorted contractCids))) is the cross-kit conformance
// anchor under spec #94 §1.
//
// Honest scope: the IR cannot directly model BLAKE3 collision
// resistance / Ed25519 unforgeability / JCS round-trip equality. What
// the IR CAN say is shape-level: output length bounds, deterministic-by-
// equality (`f(x) = f(x)`), prefix-length sanity. Stronger byte-equality
// invariants live in the kit's own test suite (provekit-proof-envelope-zig
// tests pin the cross-kit reference fixture). The contract bundle here
// is the LIVING DOCS shape for the zig kit's substrate, not a discharge
// claim.
//
// Each Decl is built into a stable arena owned by the slab; callers
// invoke `authorAll` once at mint time and consume the returned
// declarations as borrowed references into the arena.

const std = @import("std");
const provekit = @import("provekit-ir");

const Decl = provekit.Decl;
const Sort = provekit.Sort;
const Term = provekit.Term;
const Formula = provekit.Formula;

/// One authored slab: a label that ties contracts to their source file
/// plus the contracts authored by that file's invariants() function.
pub const AuthoredSlab = struct {
    label: []const u8,
    path: []const u8,
    contracts: []const Decl,
};

/// All authored canonical slabs. Backed by `arena`; lifetime tied to
/// caller's arena lifetime.
pub const Authoring = struct {
    arena: *std.heap.ArenaAllocator,
    slabs: []AuthoredSlab,

    pub fn deinit(self: *Authoring) void {
        const child = self.arena.child_allocator;
        self.arena.deinit();
        child.destroy(self.arena);
    }

    pub fn totalContracts(self: Authoring) usize {
        var n: usize = 0;
        for (self.slabs) |s| n += s.contracts.len;
        return n;
    }
};

/// Author every canonical contract into a fresh arena. The arena is
/// owned by the returned Authoring; call `Authoring.deinit()` when done.
pub fn authorAll(child_alloc: std.mem.Allocator) !Authoring {
    const arena_ptr = try child_alloc.create(std.heap.ArenaAllocator);
    arena_ptr.* = std.heap.ArenaAllocator.init(child_alloc);
    errdefer {
        arena_ptr.deinit();
        child_alloc.destroy(arena_ptr);
    }
    const a = arena_ptr.allocator();

    var slabs_list: std.ArrayList(AuthoredSlab) = .empty;
    try slabs_list.append(a, try buildJcsSlab(a));
    try slabs_list.append(a, try buildHashSlab(a));
    try slabs_list.append(a, try buildSignSlab(a));
    try slabs_list.append(a, try buildCborSlab(a));
    try slabs_list.append(a, try buildProofEnvelopeSlab(a));
    try slabs_list.append(a, try buildLiftPluginProtocolSlab(a));

    return .{
        .arena = arena_ptr,
        .slabs = try slabs_list.toOwnedSlice(a),
    };
}

// ---------------------------------------------------------------------------
// Helpers — arena-allocated formula builders.
// ---------------------------------------------------------------------------

fn dupTerms(a: std.mem.Allocator, ts: []const Term) ![]Term {
    const out = try a.alloc(Term, ts.len);
    @memcpy(out, ts);
    return out;
}

fn ctor0(a: std.mem.Allocator, name: []const u8) !Term {
    const empty = try a.alloc(Term, 0);
    return provekit.Ctor(name, empty);
}

fn ctor1(a: std.mem.Allocator, name: []const u8, arg: Term) !Term {
    const args = try a.alloc(Term, 1);
    args[0] = arg;
    return provekit.Ctor(name, args);
}

fn ctor2(a: std.mem.Allocator, name: []const u8, arg1: Term, arg2: Term) !Term {
    const args = try a.alloc(Term, 2);
    args[0] = arg1;
    args[1] = arg2;
    return provekit.Ctor(name, args);
}

fn atomic2(a: std.mem.Allocator, name: []const u8, lhs: Term, rhs: Term) !Formula {
    const args = try a.alloc(Term, 2);
    args[0] = lhs;
    args[1] = rhs;
    return provekit.Atomic(name, args);
}

/// `len(t) >= n` — string-length lower bound.
fn lenGte(a: std.mem.Allocator, t: Term, n: i64) !Formula {
    const len_t = try ctor1(a, "stringLength", t);
    return atomic2(a, "≥", len_t, provekit.Num(n));
}

/// `len(t) = n` — string-length equality.
fn lenEq(a: std.mem.Allocator, t: Term, n: i64) !Formula {
    const len_t = try ctor1(a, "stringLength", t);
    return atomic2(a, "=", len_t, provekit.Num(n));
}

/// `f(x) = f(x)` — determinism witness for unary fn `name` over sort `s`.
fn determinismContract(
    a: std.mem.Allocator,
    contract_name: []const u8,
    fn_name: []const u8,
    s: Sort,
) !Decl {
    const x1 = provekit.Var("x");
    const x2 = provekit.Var("x");
    const lhs = try ctor1(a, fn_name, x1);
    const rhs = try ctor1(a, fn_name, x2);
    const body_args = try a.alloc(Term, 2);
    body_args[0] = lhs;
    body_args[1] = rhs;
    const eq = provekit.Atomic("=", body_args);
    const body_ptr = try a.create(Formula);
    body_ptr.* = eq;
    const post = provekit.Forall("x", s, body_ptr);
    return .{ .contract = .{
        .name = contract_name,
        .out_binding = "out",
        .post = post,
    } };
}

/// `forall x: s. len(f(x)) = n`.
fn lenEqContract(
    a: std.mem.Allocator,
    contract_name: []const u8,
    fn_name: []const u8,
    n: i64,
    s: Sort,
) !Decl {
    const x = provekit.Var("x");
    const fx = try ctor1(a, fn_name, x);
    const body = try lenEq(a, fx, n);
    const body_ptr = try a.create(Formula);
    body_ptr.* = body;
    const post = provekit.Forall("x", s, body_ptr);
    return .{ .contract = .{
        .name = contract_name,
        .out_binding = "out",
        .post = post,
    } };
}

/// Bare post-only contract: `len(constName) >= n`.
fn constLenGteContract(
    a: std.mem.Allocator,
    contract_name: []const u8,
    const_name: []const u8,
    n: i64,
) !Decl {
    const c = try ctor0(a, const_name);
    const post = try lenGte(a, c, n);
    return .{ .contract = .{
        .name = contract_name,
        .out_binding = "out",
        .post = post,
    } };
}

// ---------------------------------------------------------------------------
// Slab 1 — JCS (provekit-ir/src/root.zig: jcsStringify)
// ---------------------------------------------------------------------------

fn buildJcsSlab(a: std.mem.Allocator) !AuthoredSlab {
    var contracts: std.ArrayList(Decl) = .empty;

    // jcsStringify is deterministic.
    try contracts.append(a, try determinismContract(a, "jcsStringify_is_deterministic", "jcsStringify", Sort.String));

    // jcsStringify output is non-empty (smallest JCS value `0` is len 1).
    {
        const x = provekit.Var("x");
        const fx = try ctor1(a, "jcsStringify", x);
        const body = try lenGte(a, fx, 1);
        const body_ptr = try a.create(Formula);
        body_ptr.* = body;
        const post = provekit.Forall("x", Sort.String, body_ptr);
        try contracts.append(a, .{ .contract = .{
            .name = "jcsStringify_output_nonempty",
            .out_binding = "out",
            .post = post,
        } });
    }

    return .{
        .label = "jcs",
        .path = "implementations/zig/provekit-ir/src/root.zig",
        .contracts = try contracts.toOwnedSlice(a),
    };
}

// ---------------------------------------------------------------------------
// Slab 2 — BLAKE3 hash (provekit-proof-envelope-zig/src/root.zig: blake3_512_of)
// ---------------------------------------------------------------------------

fn buildHashSlab(a: std.mem.Allocator) !AuthoredSlab {
    var contracts: std.ArrayList(Decl) = .empty;

    // Output length exactly 139 = 11 prefix + 128 hex.
    try contracts.append(a, try lenEqContract(a, "blake3_512_of_output_length_eq_139", "blake3_512_of", 139, Sort.String));
    // Deterministic.
    try contracts.append(a, try determinismContract(a, "blake3_512_of_is_deterministic", "blake3_512_of", Sort.String));
    // jcsHash is a sibling helper in provekit-ir; same output-length contract.
    try contracts.append(a, try lenEqContract(a, "jcsHash_output_length_eq_139", "jcsHash", 139, Sort.String));

    return .{
        .label = "hash",
        .path = "implementations/zig/provekit-proof-envelope-zig/src/root.zig",
        .contracts = try contracts.toOwnedSlice(a),
    };
}

// ---------------------------------------------------------------------------
// Slab 3 — Ed25519 sign (provekit-proof-envelope-zig/src/sign.zig)
// ---------------------------------------------------------------------------

fn buildSignSlab(a: std.mem.Allocator) !AuthoredSlab {
    var contracts: std.ArrayList(Decl) = .empty;

    // signWithSeed is deterministic (RFC 8032 deterministic mode).
    try contracts.append(a, try determinismContract(a, "signWithSeed_is_deterministic", "signWithSeed", Sort.String));
    // Self-identifying signString output: at least len(`ed25519:`) = 8 bytes.
    try contracts.append(a, try lenEqContract(a, "signString_prefix_length_eq_8", "ed25519PrefixOf_signString", 8, Sort.String));
    // pubkeyString minimum length: prefix 8 + base64(32 bytes) = 8 + 44 = 52.
    {
        const x = provekit.Var("x");
        const fx = try ctor1(a, "pubkeyString", x);
        const body = try lenGte(a, fx, 52);
        const body_ptr = try a.create(Formula);
        body_ptr.* = body;
        const post = provekit.Forall("x", Sort.String, body_ptr);
        try contracts.append(a, .{ .contract = .{
            .name = "pubkeyString_output_length_gte_52",
            .out_binding = "out",
            .post = post,
        } });
    }

    return .{
        .label = "sign",
        .path = "implementations/zig/provekit-proof-envelope-zig/src/sign.zig",
        .contracts = try contracts.toOwnedSlice(a),
    };
}

// ---------------------------------------------------------------------------
// Slab 4 — CBOR (provekit-proof-envelope-zig/src/cbor.zig)
// ---------------------------------------------------------------------------

fn buildCborSlab(a: std.mem.Allocator) !AuthoredSlab {
    var contracts: std.ArrayList(Decl) = .empty;

    // encodeTstr deterministic (same input -> same byte sequence).
    try contracts.append(a, try determinismContract(a, "encodeTstr_is_deterministic", "encodeTstr", Sort.String));
    // encodeBstr deterministic.
    try contracts.append(a, try determinismContract(a, "encodeBstr_is_deterministic", "encodeBstr", Sort.String));

    return .{
        .label = "cbor",
        .path = "implementations/zig/provekit-proof-envelope-zig/src/cbor.zig",
        .contracts = try contracts.toOwnedSlice(a),
    };
}

// ---------------------------------------------------------------------------
// Slab 5 — proof envelope (provekit-proof-envelope-zig/src/proof.zig)
// ---------------------------------------------------------------------------

fn buildProofEnvelopeSlab(a: std.mem.Allocator) !AuthoredSlab {
    var contracts: std.ArrayList(Decl) = .empty;

    // buildProofEnvelope deterministic (same input -> same bytes; the
    // package's own test "deterministic across runs" is the operational
    // witness).
    try contracts.append(a, try determinismContract(a, "buildProofEnvelope_is_deterministic", "buildProofEnvelope", Sort.String));
    // .proof catalog CID is a self-identifying blake3-512 string -> length 139.
    try contracts.append(a, try lenEqContract(a, "proofEnvelope_filenameCid_length_eq_139", "proofEnvelopeFilenameCid", 139, Sort.String));

    return .{
        .label = "proof-envelope",
        .path = "implementations/zig/provekit-proof-envelope-zig/src/proof.zig",
        .contracts = try contracts.toOwnedSlice(a),
    };
}

// ---------------------------------------------------------------------------
// Slab 6 — lift-plugin-protocol (provekit-lift-zig speakers)
//
// Spec: protocol/specs/2026-04-30-lift-plugin-protocol.md
// The kit's --rpc speaker (this binary, plus provekit-lift-zig's --rpc
// mode) declares these shape contracts about the protocol it speaks.
// ---------------------------------------------------------------------------

fn buildLiftPluginProtocolSlab(a: std.mem.Allocator) !AuthoredSlab {
    var contracts: std.ArrayList(Decl) = .empty;

    // Protocol-version literal length sanity ("provekit-lift/1" = 15 chars).
    try contracts.append(a, try constLenGteContract(a, "liftProtocolVersion_min_length", "liftProtocolVersion", 10));
    // The kit's `name` advertised in `initialize` is non-empty.
    try contracts.append(a, try constLenGteContract(a, "kitInitializeName_nonempty", "kitInitializeName", 1));

    return .{
        .label = "lift-plugin-protocol",
        .path = "implementations/zig/mint-zig-self-contracts/src/main.zig",
        .contracts = try contracts.toOwnedSlice(a),
    };
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

const testing = std.testing;

test "authorAll yields the canonical 14 contracts across 6 slabs" {
    var authoring = try authorAll(testing.allocator);
    defer authoring.deinit();
    try testing.expectEqual(@as(usize, 6), authoring.slabs.len);
    try testing.expectEqual(@as(usize, 14), authoring.totalContracts());
}

test "every contract has a non-empty name" {
    var authoring = try authorAll(testing.allocator);
    defer authoring.deinit();
    for (authoring.slabs) |s| {
        for (s.contracts) |d| {
            switch (d) {
                .contract => |c| try testing.expect(c.name.len > 0),
                .bridge => unreachable,
            }
        }
    }
}

test "contract names are unique across slabs" {
    var authoring = try authorAll(testing.allocator);
    defer authoring.deinit();
    var seen = std.StringHashMap(void).init(testing.allocator);
    defer seen.deinit();
    for (authoring.slabs) |s| {
        for (s.contracts) |d| {
            const c = d.contract;
            const gop = try seen.getOrPut(c.name);
            try testing.expect(!gop.found_existing);
        }
    }
}
