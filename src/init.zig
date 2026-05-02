const std = @import("std");
const posix = std.posix;

pub fn main() !void {
    var gpa = std.heap.GeneralPurposeAllocator(.{}){};
    defer _ = gpa.deinit();
    const allocator = gpa.allocator();

    var args_iter = try std.process.argsWithAllocator(allocator);
    defer args_iter.deinit();

    // Skip the first argument which is the path to the init binary itself (e.g., "/init")
    _ = args_iter.skip();

    var cmd_args = std.ArrayList([]const u8).empty;
    defer cmd_args.deinit(allocator);

    while (args_iter.next()) |arg| {
        try cmd_args.append(allocator, arg);
    }

    if (cmd_args.items.len == 0) {
        std.debug.print("Blink-Init: No command provided to execute.\n", .{});
        posix.exit(1);
    }

    std.debug.print("Blink-Init: Booting up. Target payload: {s}\n", .{cmd_args.items[0]});

    const pid = try posix.fork();
    if (pid == 0) {
        // Child process: execute the target payload
        var c_args = try allocator.alloc(?[*:0]const u8, cmd_args.items.len + 1);
        for (cmd_args.items, 0..) |arg, i| {
            // Need null-terminated strings for execveZ
            const z_arg = try allocator.dupeZ(u8, arg);
            c_args[i] = z_arg.ptr;
        }
        c_args[cmd_args.items.len] = null;

        var c_env = try allocator.alloc(?[*:0]const u8, std.os.environ.len + 1);
        for (std.os.environ, 0..) |env_var, i| {
            c_env[i] = env_var;
        }
        c_env[std.os.environ.len] = null;
        const envp: [*:null]const ?[*:0]const u8 = @ptrCast(c_env.ptr);

        const exec_path = try allocator.dupeZ(u8, cmd_args.items[0]);
        
        const err = posix.execveZ(exec_path.ptr, @ptrCast(c_args.ptr), envp);
        
        std.debug.print("Blink-Init: Failed to exec payload '{s}': {}\n", .{exec_path, err});
        posix.exit(1);
    }

    // Parent process: Zombie Reaper loop
    var main_child_exit_status: u8 = 1;

    while (true) {
        const wait_res = posix.waitpid(-1, 0);
        const wpid = wait_res.pid;
        const status = wait_res.status;

        if (wpid == pid) {
            // The main payload exited. We can shut down the sandbox now.
            if (posix.W.IFEXITED(status)) {
                main_child_exit_status = @intCast(posix.W.EXITSTATUS(status));
                std.debug.print("Blink-Init: Main payload exited with status {}\n", .{main_child_exit_status});
            } else if (posix.W.IFSIGNALED(status)) {
                main_child_exit_status = 128 + @as(u8, @intCast(posix.W.TERMSIG(status)));
                std.debug.print("Blink-Init: Main payload terminated by signal {}\n", .{posix.W.TERMSIG(status)});
            }
            break;
        } else if (wpid > 0) {
            // A zombie process was reaped. Just log it for debugging.
            std.debug.print("Blink-Init: Reaped zombie child PID {}\n", .{wpid});
        }
    }

    std.debug.print("Blink-Init: Shutting down sandbox.\n", .{});
    posix.exit(main_child_exit_status);
}
