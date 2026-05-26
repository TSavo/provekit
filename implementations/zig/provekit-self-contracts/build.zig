// SPDX-License-Identifier: Apache-2.0
//
// provekit-self-contracts (zig): Side B substrate library.
//
// Native Zig implementation of the substrate primitives required to mint
// claim envelopes (JCS-canonical layered mementos) and proof envelopes
// (deterministic-CBOR signed catalogs) without shelling out to another
// kit. Mirrors the structure of:
//
//   * implementations/rust/provekit-canonicalizer        (jcs + hash)
//   * implementations/rust/provekit-proof-envelope       (cbor + sign + proof)
//   * implementations/rust/provekit-claim-envelope       (layered memento)
//
// Module layout:
//   src/jcs.zig             - Value tree + RFC 8785 JCS encoder
//   src/hash.zig            - BLAKE3-512 + self-identifying string form
//   src/cbor.zig            - RFC 8949 §4.2.1 deterministic CBOR encoder
//   src/signing.zig         - Ed25519 sign/verify (std.crypto.sign.Ed25519)
//   src/claim_envelope.zig  - mint_contract / mint_bridge / mint_implication
//   src/proof_envelope.zig  - build_proof_envelope (signed CBOR catalog)
//   src/foundation.zig      - Foundation v0 ed25519 seed (= [0x42; 32])
//   src/root.zig            - umbrella module re-exporting all of the above
//   src/byte_equivalence_test.zig - cross-kit byte-equivalence pins

const std = @import("std");

pub fn build(b: *std.Build) void {
    const target = b.standardTargetOptions(.{});
    const optimize = b.standardOptimizeOption(.{});

    // Public umbrella module. Consumers depend on `provekit-self-contracts`
    // and reach the sub-modules through namespaced re-exports in root.zig.
    const lib_mod = b.addModule("provekit-self-contracts", .{
        .root_source_file = b.path("src/root.zig"),
        .target = target,
        .optimize = optimize,
    });

    // Test runner: pulls every src/*.zig test block via root.zig's
    // `comptime` references.
    const lib_unit_tests = b.addTest(.{
        .root_module = lib_mod,
    });
    const run_lib_unit_tests = b.addRunArtifact(lib_unit_tests);

    const test_step = b.step("test", "Run unit tests");
    test_step.dependOn(&run_lib_unit_tests.step);
}
