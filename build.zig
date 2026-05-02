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
    // Only link krun if we are specifically targeting krun backend or it's available
    // For now, we allow building without it for simulation purposes
    if (b.option(bool, "enable-krun", "Enable libkrun support") orelse false) {
        lib.linkSystemLibrary("krun");
    }
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
    if (b.option(bool, "enable-krun", "Enable libkrun support") orelse false) {
        exe.linkSystemLibrary("krun");
    }
    b.installArtifact(exe);

    // blink-init must be a statically compiled Linux binary (it runs inside the sandbox)
    const init_target = b.resolveTargetQuery(.{
        .cpu_arch = target.result.cpu.arch,
        .os_tag = .linux,
        .abi = .musl,
    });

    const init_mod = b.createModule(.{
        .root_source_file = b.path("src/init.zig"),
        .target = init_target,
        .optimize = optimize,
    });

    const init_exe = b.addExecutable(.{
        .name = "blink-init",
        .root_module = init_mod,
    });
    b.installArtifact(init_exe);
}