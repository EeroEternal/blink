const std = @import("std");
const krun = @import("libkrun.zig");

pub const BlinkState = enum {
    PreHeat,
    Blinking,
    Halt,
    Vanished,
};

/// 核心状态控制块
pub const BlinkInstance = struct {
    allocator: std.mem.Allocator,
    ctx_id: u32,
    status: BlinkState,
    cid: u32,

    pub fn create(allocator: std.mem.Allocator, cid: u32) !*BlinkInstance {
        const ctx_id = try krun.createCtx();
        errdefer krun.freeCtx(ctx_id);

        const instance = try allocator.create(BlinkInstance);
        instance.* = .{
            .allocator = allocator,
            .ctx_id = ctx_id,
            .status = .PreHeat,
            .cid = cid,
        };
        return instance;
    }

    pub fn destroy(self: *BlinkInstance) void {
        if (self.status != .Vanished) {
            krun.freeCtx(self.ctx_id);
            self.status = .Vanished;
        }
        self.allocator.destroy(self);
    }

    /// 设置文件系统映射 (Virtio-fs): Environment Ambient Pass-through
    pub fn setupFileSystem(self: *BlinkInstance, root_path: [:0]const u8, mappings: []const [:0]const u8) !void {
        // Ephemeral Root mapping (Read-only rootfs)
        try krun.setRoot(self.ctx_id, root_path);

        if (mappings.len > 0) {
            // libkrun expects a null-terminated array of pointers
            var c_vols = try self.allocator.alloc(?[*:0]const u8, mappings.len + 1);
            defer self.allocator.free(c_vols);
            
            for (mappings, 0..) |vol, i| {
                c_vols[i] = vol.ptr;
            }
            c_vols[mappings.len] = null;
            
            // "host_path:guest_path:options" mapped via virtio-fs DAX
            try krun.setMappedVolumes(self.ctx_id, c_vols);
        }
    }

    /// 注入脚本内容到内存映射区 (Hot-zone preparation)
    pub fn injectScript(self: *BlinkInstance, script_path: [:0]const u8) !void {
        // Agent's only write space is the hot-zone (/tmp)
        try krun.setWorkdir(self.ctx_id, "/tmp");
        
        // Execute the targeted runtime mapping
        const args = [_]?[*:0]const u8{ script_path.ptr, null };
        try krun.setExec(self.ctx_id, script_path, &args);
    }

    /// 同步启动 vCPU，由内部线程池调用
    pub fn startVcpu(self: *BlinkInstance) !void {
        self.status = .Blinking;
        // Blocks until guest shuts down, executing the injected python/ts payload
        try krun.startEnter(self.ctx_id);
        
        // Retain memory state, but vCPU stops
        self.status = .Halt;
    }

    /// 高层触发接口：基于规格书实现的零拷贝拉起
    pub fn trigger(self: *BlinkInstance, script: []const u8) !void {
        _ = script; // Script would be dynamically written to Host's Hot-zone before mapping
        
        // 1. 设置文件系统映射 (Virtio-fs)
        const mappings = [_][:0]const u8{
            // Map python runtime directly into the sandbox
            "/opt/runtimes/python3.11:/opt/runtimes/python3.11",
            // Map the host hot-zone into guest's /tmp for RW execution
            "/tmp/agent_hotzone_cid_1:/tmp"
        };
        try self.setupFileSystem("/var/lib/blink/rootfs", &mappings);

        // 2. 注入脚本内容 (assuming script was dropped into hot-zone)
        try self.injectScript("/opt/runtimes/python3.11/bin/python3");

        // 3. 异步启动 vCPU
        const thread = try std.Thread.spawn(.{}, startVcpuThread, .{self});
        thread.detach();
    }

    fn startVcpuThread(self: *BlinkInstance) void {
        self.startVcpu() catch |err| {
            std.log.err("Blink Execution failed for CID {}: {}", .{self.cid, err});
            self.status = .Halt;
        };
    }
};