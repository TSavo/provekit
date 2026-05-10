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
    const cicp_vectors = b.addOptions();
    addCicpVectorOptions(b, cicp_vectors);
    lib_mod.addOptions("cicp_vectors", cicp_vectors);

    // Test runner: pulls every src/*.zig test block via root.zig's
    // `comptime` references.
    const lib_unit_tests = b.addTest(.{
        .root_module = lib_mod,
    });
    const run_lib_unit_tests = b.addRunArtifact(lib_unit_tests);

    const test_step = b.step("test", "Run unit tests");
    test_step.dependOn(&run_lib_unit_tests.step);
}

fn addCicpVectorOptions(b: *std.Build, options: *std.Build.Step.Options) void {
    options.addOption([]const u8, "manifest", readCicpVector(b, "vectors.json"));
    options.addOption([]const u8, "blast_radius_rust_kit", readCicpVector(b, "blast-radius-rust-kit.json"));
    options.addOption([]const u8, "blast_radius_rust_kit_next_catalog", readCicpVector(b, "blast-radius-rust-kit-next-catalog.json"));
    options.addOption([]const u8, "job_result_pass", readCicpVector(b, "job-result-pass.json"));
    options.addOption([]const u8, "reuse_identical", readCicpVector(b, "reuse-identical.json"));
    options.addOption([]const u8, "reuse_bridged_by_evolution", readCicpVector(b, "reuse-bridged-by-evolution.json"));
    options.addOption([]const u8, "impact_protocol_extension_only", readCicpVector(b, "impact-protocol-extension-only.json"));
    options.addOption([]const u8, "invalid_blast_radius_open_input_closure", readCicpVector(b, "invalid-blast-radius-open-input-closure.json"));
}

fn readCicpVector(b: *std.Build, name: []const u8) []const u8 {
    const path = b.pathJoin(&.{ "../../../protocol/conformance/cicp", name });
    return std.Io.Dir.cwd().readFileAlloc(b.graph.io, path, b.allocator, .limited(1024 * 1024)) catch |err| {
        std.debug.panic("failed to read {s}: {s}", .{ path, @errorName(err) });
    };
}
