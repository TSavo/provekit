const std = @import("std");

pub fn build(b: *std.Build) void {
    const target = b.standardTargetOptions(.{});
    const optimize = b.standardOptimizeOption(.{});

    const provekit_ir = b.createModule(.{
        .root_source_file = b.path("../provekit-ir/src/root.zig"),
        .target = target,
        .optimize = optimize,
    });

    const lift_mod = b.addModule("provekit-lift-zig-tests", .{
        .root_source_file = b.path("src/lift.zig"),
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
            .{ .name = "provekit-lift-zig-tests", .module = lift_mod },
        },
    });

    const exe = b.addExecutable(.{
        .name = "provekit-lift-zig-tests",
        .root_module = exe_mod,
    });
    b.installArtifact(exe);

    const run_cmd = b.addRunArtifact(exe);
    run_cmd.step.dependOn(b.getInstallStep());
    if (b.args) |args| {
        run_cmd.addArgs(args);
    }
    const run_step = b.step("run", "Run the app");
    run_step.dependOn(&run_cmd.step);

    // Unit tests for the lift module.
    const lift_tests = b.addTest(.{
        .root_module = lift_mod,
    });
    const run_lift_tests = b.addRunArtifact(lift_tests);

    const test_step = b.step("test", "Run unit tests");
    test_step.dependOn(&run_lift_tests.step);
}
