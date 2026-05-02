const std = @import("std");

pub fn build(b: *std.Build) void {
    const target = b.standardTargetOptions(.{});
    const optimize = b.standardOptimizeOption(.{});

    const core_mod = b.createModule(.{
        .root_source_file = b.path("src/blink.zig"),
        .target = target,
        .optimize = optimize,
    });

    const lib = b.addLibrary(.{
        .linkage = .static,
        .name = "blink-core",
        .root_module = core_mod,
    });
    lib.linkLibC();
    lib.linkSystemLibrary("krun");
    b.installArtifact(lib);

    const exe_mod = b.createModule(.{
        .root_source_file = b.path("src/main.zig"),
        .target = target,
        .optimize = optimize,
    });

    const exe = b.addExecutable(.{
        .name = "blink-cli",
        .root_module = exe_mod,
    });
    exe.linkLibC();
    exe.linkSystemLibrary("krun");
    b.installArtifact(exe);
}