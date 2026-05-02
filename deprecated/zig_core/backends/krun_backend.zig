const std = @import("std");
const hypervisor = @import("../hypervisor.zig");
const krun = @import("../libkrun.zig");

pub const KrunBackend = struct {
    ctx_id: u32,

    pub fn setupFileSystem(ctx: *anyopaque, root_path: [:0]const u8, mappings: []const [:0]const u8) !void {
        const self: *KrunBackend = @ptrCast(@alignCast(ctx));
        try krun.setRoot(self.ctx_id, root_path);
        
        var arena = std.heap.ArenaAllocator.init(std.heap.page_allocator);
        defer arena.deinit();
        
        var c_vols = try arena.allocator().alloc(?[*:0]const u8, mappings.len + 1);
        for (mappings, 0..) |vol, i| {
            c_vols[i] = vol.ptr;
        }
        c_vols[mappings.len] = null;
        try krun.setMappedVolumes(self.ctx_id, c_vols);
    }

    pub fn injectScript(ctx: *anyopaque, script_path: [:0]const u8, argv: []const ?[*:0]const u8, envp: []const ?[*:0]const u8) !void {
        const self: *KrunBackend = @ptrCast(@alignCast(ctx));
        try krun.setWorkdir(self.ctx_id, "/tmp");
        try krun.setExec(self.ctx_id, script_path, argv, envp);
    }

    pub fn startVcpu(ctx: *anyopaque) !void {
        const self: *KrunBackend = @ptrCast(@alignCast(ctx));
        try krun.startEnter(self.ctx_id);
    }

    pub fn destroy(ctx: *anyopaque) void {
        const self: *KrunBackend = @ptrCast(@alignCast(ctx));
        krun.freeCtx(self.ctx_id);
    }

    pub const vtable = hypervisor.BlinkHypervisor.VTable{
        .setupFileSystem = setupFileSystem,
        .injectScript = injectScript,
        .startVcpu = startVcpu,
        .destroy = destroy,
    };
};
