const std = @import("std");
const posix = std.posix;
const protocol = @import("protocol.zig");

// Linux AF_VSOCK is 40
pub const AF_VSOCK: u16 = 40;

pub const sockaddr_vm = extern struct {
    svm_family: u16 = AF_VSOCK,
    svm_reserved1: u16 = 0,
    svm_port: u32,
    svm_cid: u32,
    svm_zero: [4]u8 = [_]u8{0} ** 4,
};

pub fn main() !void {
    var gpa = std.heap.GeneralPurposeAllocator(.{}){};
    defer _ = gpa.deinit();
    const allocator = gpa.allocator();

    var args_iter = try std.process.argsWithAllocator(allocator);
    defer args_iter.deinit();

    _ = args_iter.skip(); // skip /init

    var cmd_args = std.ArrayList([]const u8).empty;
    defer cmd_args.deinit(allocator);

    while (args_iter.next()) |arg| {
        try cmd_args.append(allocator, arg);
    }

    if (cmd_args.items.len == 0) {
        std.debug.print("Blink-Init: No command provided.\n", .{});
        posix.exit(1);
    }

    // Connect to V-Hub (Host) before forking
    const vsock_fd = try posix.socket(AF_VSOCK, posix.SOCK.STREAM, 0);
    defer posix.close(vsock_fd);

    var addr = sockaddr_vm{
        .svm_port = 10000,
        .svm_cid = 2, // VMADDR_CID_HOST
    };

    // Try to connect to host (V-Hub)
    posix.connect(vsock_fd, @ptrCast(&addr), @sizeOf(sockaddr_vm)) catch |err| {
        std.debug.print("Blink-Init: Failed to connect to V-Hub: {}\n", .{err});
        // Continue anyway, but we won't be able to stream logs
    };

    // Create pipes for stdout/stderr redirection
    const pipe_out = try posix.pipe();
    const pipe_err = try posix.pipe();

    const pid = try posix.fork();
    if (pid == 0) {
        // Child: Redirection
        try posix.dup2(pipe_out[1], posix.STDOUT_FILENO);
        try posix.dup2(pipe_err[1], posix.STDERR_FILENO);
        posix.close(pipe_out[0]);
        posix.close(pipe_out[1]);
        posix.close(pipe_err[0]);
        posix.close(pipe_err[1]);

        var c_args = try allocator.alloc(?[*:0]const u8, cmd_args.items.len + 1);
        for (cmd_args.items, 0..) |arg, i| {
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
        const exec_err = posix.execveZ(exec_path.ptr, @ptrCast(c_args.ptr), envp);
        std.debug.print("Blink-Init: execveZ failed: {}\n", .{exec_err});
        posix.exit(1);
    }

    // Parent: Protocol Relay
    posix.close(pipe_out[1]);
    posix.close(pipe_err[1]);

    var poll_fds = [_]posix.pollfd{
        .{ .fd = pipe_out[0], .events = posix.POLL.IN, .revents = 0 },
        .{ .fd = pipe_err[0], .events = posix.POLL.IN, .revents = 0 },
    };

    var buf: [4096]u8 = undefined;
    var running_count: usize = 2;

    while (running_count > 0) {
        const ready = try posix.poll(&poll_fds, -1);
        if (ready == 0) continue;

        for (&poll_fds) |*pfd| {
            if (pfd.revents == 0) continue;

            if ((pfd.revents & (posix.POLL.IN | posix.POLL.HUP)) != 0) {
                const len = posix.read(pfd.fd, &buf) catch 0;
                if (len == 0) {
                    pfd.fd = -1; // Stop polling this fd
                    running_count -= 1;
                    continue;
                }

                const msg_type: protocol.MessageType = if (pfd.fd == pipe_out[0]) .Stdout else .Stderr;
                
                // Wrap and send over Vsock
                const packet = try protocol.VsockProtocol.encodePacketAlloc(allocator, msg_type, 0, buf[0..len]);
                defer allocator.free(packet);
                _ = posix.write(vsock_fd, packet) catch |err| {
                    std.debug.print("Blink-Init: Vsock write error: {}\n", .{err});
                };
            }
        }
    }

    const wait_res = posix.waitpid(pid, 0);
    posix.exit(@intCast(posix.W.EXITSTATUS(wait_res.status)));
}
