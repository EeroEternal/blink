const std = @import("std");

pub fn build(b: *std.Build) void {
    const target = b.standardTargetOptions(.{});
    const optimize = b.standardOptimizeOption(.{});

    // Provide blink-core as a static library
    const lib = b.addStaticLibrary(.{
        .name = "blink-core",
        .root_source_file = b.path("src/blink.zig"),
        .target = target,
        .optimize = optimize,
    });
    
    // Explicitly link against C standard library and libkrun
    lib.linkLibC();
    lib.linkSystemLibrary("krun");

    b.installArtifact(lib);
}