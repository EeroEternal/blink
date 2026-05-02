const std = @import("std");

/// Libkrun C API declarations directly mapped to avoid header dependency.
/// This guarantees zero-overhead integration with the C ABI.
pub const c = struct {
    pub extern "C" fn krun_create_ctx() i32;
    pub extern "C" fn krun_free_ctx(ctx_id: u32) i32;
    pub extern "C" fn krun_set_root(ctx_id: u32, root_path: [*:0]const u8) i32;
    pub extern "C" fn krun_set_mapped_volumes(ctx_id: u32, mapped_volumes: [*]const ?[*:0]const u8) i32;
    pub extern "C" fn krun_set_workdir(ctx_id: u32, workdir: [*:0]const u8) i32;
    pub extern "C" fn krun_set_exec(ctx_id: u32, exec_path: [*:0]const u8, argv: [*]const ?[*:0]const u8, envp: ?[*]const ?[*:0]const u8) i32;
    pub extern "C" fn krun_start_enter(ctx_id: u32) i32;
};

pub const KrunError = error{
    ContextCreationFailed,
    ConfigError,
    ExecutionFailed,
};

pub fn createCtx() !u32 {
    const ctx = c.krun_create_ctx();
    if (ctx < 0) return error.ContextCreationFailed;
    return @intCast(ctx);
}

pub fn freeCtx(ctx: u32) void {
    _ = c.krun_free_ctx(ctx);
}

pub fn setRoot(ctx: u32, root_path: [:0]const u8) !void {
    if (c.krun_set_root(ctx, root_path.ptr) < 0) {
        return error.ConfigError;
    }
}

pub fn setMappedVolumes(ctx: u32, mapped_volumes: []const ?[*:0]const u8) !void {
    if (c.krun_set_mapped_volumes(ctx, mapped_volumes.ptr) < 0) {
        return error.ConfigError;
    }
}

pub fn setWorkdir(ctx: u32, workdir: [:0]const u8) !void {
    if (c.krun_set_workdir(ctx, workdir.ptr) < 0) {
        return error.ConfigError;
    }
}

pub fn setExec(ctx: u32, exec_path: [:0]const u8, argv: []const ?[*:0]const u8, envp: ?[]const ?[*:0]const u8) !void {
    const envp_ptr = if (envp) |e| e.ptr else null;
    if (c.krun_set_exec(ctx, exec_path.ptr, argv.ptr, envp_ptr) < 0) {
        return error.ConfigError;
    }
}

pub fn startEnter(ctx: u32) !void {
    if (c.krun_start_enter(ctx) < 0) {
        return error.ExecutionFailed;
    }
}