// SPDX-License-Identifier: Apache-2.0
//
// Build for provekit-proof-envelope-zig.
//
// Side B (language-native) crypto substrate for the zig kit. Exposes:
//   - BLAKE3-512 hashing (re-exported from std.crypto.hash.Blake3)
//   - Deterministic CBOR encoder (RFC 8949 §4.2.1) — ~90 LOC, mirrors
//     the rust kit's provekit-proof-envelope/src/cbor.rs
//   - Ed25519 sign/verify (wraps std.crypto.sign.Ed25519)
//   - .proof envelope builder, byte-identical to the rust kit's
//     provekit-proof-envelope::build_proof_envelope for the same input
//
// JCS canonicalization for the IR ships in provekit-ir/src/root.zig
// (jcsStringify + jcsHash); this package re-uses it via module import.

const std = @import("std");

pub fn build(b: *std.Build) void {
    const target = b.standardTargetOptions(.{});
    const optimize = b.standardOptimizeOption(.{});

    // Sibling module: provekit-ir provides JCS helpers + IR types. We
    // import it so the public API of this package exposes the unified
    // crypto substrate (BLAKE3 + JCS + Ed25519 + CBOR + envelope).
    const provekit_ir = b.createModule(.{
        .root_source_file = b.path("../provekit-ir/src/root.zig"),
        .target = target,
        .optimize = optimize,
    });

    const lib = b.addModule("provekit-proof-envelope-zig", .{
        .root_source_file = b.path("src/root.zig"),
        .target = target,
        .optimize = optimize,
        .imports = &.{
            .{ .name = "provekit-ir", .module = provekit_ir },
        },
    });

    const lib_unit_tests = b.addTest(.{
        .root_module = lib,
    });

    const run_lib_unit_tests = b.addRunArtifact(lib_unit_tests);
    const test_step = b.step("test", "Run unit tests");
    test_step.dependOn(&run_lib_unit_tests.step);
}
