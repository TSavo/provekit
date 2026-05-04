// SPDX-License-Identifier: Apache-2.0
//
// Build for mint-zig-self-contracts.
//
// Side A (substrate-driven conformance) orchestrator for the zig kit.
// Walks an authored slab of canonical contracts, computes their content
// CIDs and the contract set CID per spec #94 §1, builds a `.proof`
// envelope using the Side B crypto substrate from
// provekit-proof-envelope-zig (PR #227), and emits the result either as
// a human-readable banner (CLI mode) or as a JSON-RPC `proof-envelope`
// response (--rpc mode, lift-plugin-protocol speaker).
//
// The Rust CLI dispatcher in implementations/rust/provekit-cli spawns
// this binary with `--rpc` per the manifest at
// implementations/zig/.provekit/lift/zig-self-contracts/manifest.toml.
//
// Module imports:
//   provekit-ir                — IR types + JCS + BLAKE3 helper
//   provekit-proof-envelope-zig — buildProofEnvelope + sign helpers

const std = @import("std");

pub fn build(b: *std.Build) void {
    const target = b.standardTargetOptions(.{});
    const optimize = b.standardOptimizeOption(.{});

    const provekit_ir = b.createModule(.{
        .root_source_file = b.path("../provekit-ir/src/root.zig"),
        .target = target,
        .optimize = optimize,
    });

    const provekit_proof_envelope = b.createModule(.{
        .root_source_file = b.path("../provekit-proof-envelope-zig/src/root.zig"),
        .target = target,
        .optimize = optimize,
        .imports = &.{
            .{ .name = "provekit-ir", .module = provekit_ir },
        },
    });

    const exe_mod = b.createModule(.{
        .root_source_file = b.path("src/main.zig"),
        .target = target,
        .optimize = optimize,
        .imports = &.{
            .{ .name = "provekit-ir", .module = provekit_ir },
            .{ .name = "provekit-proof-envelope-zig", .module = provekit_proof_envelope },
        },
    });

    const exe = b.addExecutable(.{
        .name = "mint-zig-self-contracts",
        .root_module = exe_mod,
    });
    b.installArtifact(exe);

    const run_cmd = b.addRunArtifact(exe);
    run_cmd.step.dependOn(b.getInstallStep());
    if (b.args) |args| run_cmd.addArgs(args);
    const run_step = b.step("run", "Run the mint-zig-self-contracts orchestrator");
    run_step.dependOn(&run_cmd.step);

    const exe_unit_tests = b.addTest(.{
        .root_module = exe_mod,
    });
    const run_exe_unit_tests = b.addRunArtifact(exe_unit_tests);
    const test_step = b.step("test", "Run unit tests");
    test_step.dependOn(&run_exe_unit_tests.step);
}
