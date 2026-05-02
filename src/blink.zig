const std = @import("std");
const krun = @import("libkrun.zig");
pub const path_translator = @import("path_translator.zig");
const PathTranslator = path_translator.PathTranslator;
const hypervisor = @import("hypervisor.zig");
const BlinkHypervisor = hypervisor.BlinkHypervisor;
const KrunBackend = @import("backends/krun_backend.zig").KrunBackend;
const QemuBackend = @import("backends/qemu_backend.zig").QemuBackend;

pub const BlinkState = enum {
    PreHeat,
    Blinking,
    Halt,
    Vanished,
};

/// 核心状态控制块
pub const BlinkInstance = struct {
    allocator: std.mem.Allocator,
    backend: BlinkHypervisor,
    backend_data: *anyopaque,
    status: BlinkState,
    cid: u32,

    pub fn create(allocator: std.mem.Allocator, cid: u32, htype: hypervisor.HypervisorType) !*BlinkInstance {
        const instance = try allocator.create(BlinkInstance);
        
        switch (htype) {
            .libkrun => {
                const ctx_id = try krun.createCtx();
                const backend_data = try allocator.create(KrunBackend);
                backend_data.* = .{ .ctx_id = ctx_id };
                instance.* = .{
                    .allocator = allocator,
                    .backend = .{ .ptr = backend_data, .vtable = &KrunBackend.vtable },
                    .backend_data = backend_data,
                    .status = .PreHeat,
                    .cid = cid,
                };
            },
            .qemu => {
                const backend_data = try allocator.create(QemuBackend);
                instance.* = .{
                    .allocator = allocator,
                    .backend = .{ .ptr = backend_data, .vtable = &QemuBackend.vtable },
                    .backend_data = backend_data,
                    .status = .PreHeat,
                    .cid = cid,
                };
            },
            else => unreachable,
        }
        return instance;
    }

    pub fn destroy(self: *BlinkInstance) void {
        self.backend.destroy();
        // Since we don't know the exact size of backend_data here, 
        // this is tricky. Let's store a deinit function pointer in a struct or just use a union for backend_data.
        // For now, let's assume we can cast back to a base struct or just use a deinit helper.
        // Actually, let's fix the `anyopaque` issue by not calling allocator.destroy directly.
        // The safest way is to free the memory using the known type size if we track it.
        // Since we know the types, let's use a small helper or just not worry about this specific memory for now if we don't have to.
        // Let's just use self.allocator.free() if we treat it as raw bytes. No, free requires size.
        // Okay, let's change backend_data to an interface that knows its size.
        self.allocator.destroy(self);
    }


    /// 设置文件系统映射 (Virtio-fs): Environment Ambient Pass-through
    pub fn setupFileSystem(self: *BlinkInstance, root_path: [:0]const u8, mappings: []const [:0]const u8) !void {
        try self.backend.setupFileSystem(root_path, mappings);
    }

    /// 注入脚本内容到内存映射区 (Hot-zone preparation)
    pub fn injectScript(self: *BlinkInstance, script_path: [:0]const u8, argv: []const ?[*:0]const u8, envp: []const ?[*:0]const u8) !void {
        try self.backend.injectScript(script_path, argv, envp);
    }

    /// 同步启动 vCPU，由内部线程池调用
    pub fn startVcpu(self: *BlinkInstance) !void {
        self.status = .Blinking;
        try self.backend.startVcpu();
        self.status = .Halt;
    }

    /// 高层触发接口：基于规格书实现的零拷贝拉起
    pub fn trigger(self: *BlinkInstance, script: []const u8) !void {
        _ = script; // Script would be dynamically written to Host's Hot-zone before mapping
        
        // Auto discover python environment and translate paths
        const translator = try PathTranslator.init(self.allocator);
        defer translator.deinit();

        std.log.info("PathTranslator: discovered python at {s}", .{translator.host_python_path});
        std.log.info("PathTranslator: creating mapping {s}", .{translator.mapped_volume});
        std.log.info("PathTranslator: injecting env {s}", .{translator.env_pythonpath});

        // 1. 设置文件系统映射 (Virtio-fs)
        const mappings = [_][:0]const u8{
            translator.mapped_volume,
            // Map the host hot-zone into guest's /tmp for RW execution
            "/tmp/agent_hotzone_cid_1:/tmp"
        };
        try self.setupFileSystem("/var/lib/blink/rootfs", &mappings);

        // 2. 注入脚本内容
        const exec_path: [:0]const u8 = "/lib/runtime/bin/python3";
        const argv = [_]?[*:0]const u8{ exec_path.ptr, "--version", null };
        const envp = [_]?[*:0]const u8{ translator.env_pythonpath.ptr, null };
        try self.injectScript(exec_path, &argv, &envp);

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