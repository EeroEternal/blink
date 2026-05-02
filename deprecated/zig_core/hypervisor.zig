const std = @import("std");

pub const HypervisorType = enum {
    libkrun,
    qemu,
    firecracker,
};

/// Interface that any hypervisor backend must implement
pub const BlinkHypervisor = struct {
    ptr: *anyopaque,
    vtable: *const VTable,

    pub const VTable = struct {
        setupFileSystem: *const fn (ctx: *anyopaque, root_path: [:0]const u8, mappings: []const [:0]const u8) anyerror!void,
        injectScript: *const fn (ctx: *anyopaque, script_path: [:0]const u8, argv: []const ?[*:0]const u8, envp: []const ?[*:0]const u8) anyerror!void,
        startVcpu: *const fn (ctx: *anyopaque) anyerror!void,
        destroy: *const fn (ctx: *anyopaque) void,
    };

    pub fn setupFileSystem(self: *BlinkHypervisor, root_path: [:0]const u8, mappings: []const [:0]const u8) !void {
        return self.vtable.setupFileSystem(self.ptr, root_path, mappings);
    }

    pub fn injectScript(self: *BlinkHypervisor, script_path: [:0]const u8, argv: []const ?[*:0]const u8, envp: []const ?[*:0]const u8) !void {
        return self.vtable.injectScript(self.ptr, script_path, argv, envp);
    }

    pub fn startVcpu(self: *BlinkHypervisor) !void {
        return self.vtable.startVcpu(self.ptr);
    }

    pub fn destroy(self: *BlinkHypervisor) void {
        self.vtable.destroy(self.ptr);
    }
};
