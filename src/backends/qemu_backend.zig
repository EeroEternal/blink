const std = @import("std");
const hypervisor = @import("hypervisor.zig");

pub const QemuBackend = struct {
    
    pub fn setupFileSystem(ctx: *anyopaque, root_path: [:0]const u8, mappings: []const [:0]const u8) !void {
        _ = ctx;
        std.debug.print("[QemuBackend] Mock: Setting root to {s}\n", .{root_path});
        for (mappings) |m| {
            std.debug.print("[QemuBackend] Mock: Mapping {s}\n", .{m});
        }
    }

    pub fn injectScript(ctx: *anyopaque, script_path: [:0]const u8, argv: []const ?[*:0]const u8, envp: []const ?[*:0]const u8) !void {
        _ = ctx;
        _ = argv;
        _ = envp;
        std.debug.print("[QemuBackend] Mock: Executing script at {s}\n", .{script_path});
    }

    pub fn startVcpu(ctx: *anyopaque) !void {
        _ = ctx;
        std.debug.print("[QemuBackend] Mock: Starting VCPU (Process-based emulation)\n", .{});
    }

    pub fn destroy(ctx: *anyopaque) void {
        _ = ctx;
    }

    pub const vtable = hypervisor.BlinkHypervisor.VTable{
        .setupFileSystem = setupFileSystem,
        .injectScript = injectScript,
        .startVcpu = startVcpu,
        .destroy = destroy,
    };
};
