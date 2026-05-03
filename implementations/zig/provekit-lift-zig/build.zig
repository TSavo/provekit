const std = @import("std");

pub fn build(b: *std.Build) void {
    const target = b.standardTargetOptions(.{});
    const optimize = b.standardOptimizeOption(.{});

    const provekit_ir = b.createModule(.{
        .root_source_file = b.path("../provekit-ir/src/root.zig"),
    });

    const exe = b.addExecutable(.{
        .name = "provekit-lift-zig",
        .root_source_file = b.path("src/main.zig"),
        .target = target,
        .optimize = optimize,
    });
    exe.root_module.addImport("provekit-ir", provekit_ir);
    b.installArtifact(exe);

    const run_cmd = b.addRunArtifact(exe);
    run_cmd.step.dependOn(b.getInstallStep());
    if (b.args) |args| {
        run_cmd.addArgs(args);
    }
    const run_step = b.step("run", "Run the app");
    run_step.dependOn(&run_cmd.step);

    // LSP plugin round-trip test (#221). Spawns the built `provekit-lift-zig`
    // with `--rpc` and drives initialize/parse/shutdown over NDJSON-on-stdio.
    const lsp_test = b.addTest(.{
        .root_source_file = b.path("test/lsp_round_trip.zig"),
        .target = target,
        .optimize = optimize,
    });
    lsp_test.step.dependOn(b.getInstallStep());
    const lsp_test_run = b.addRunArtifact(lsp_test);
    // Pass the installed binary path through an env var the test reads.
    lsp_test_run.setEnvironmentVariable(
        "PROVEKIT_LIFT_ZIG_BIN",
        b.getInstallPath(.bin, "provekit-lift-zig"),
    );
    const test_step = b.step("test", "Run unit tests");
    test_step.dependOn(&lsp_test_run.step);
}
