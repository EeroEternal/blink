const std = @import("std");
const blink = @import("blink.zig");
const vsock = @import("vsock.zig");

pub fn main() !void {
    var gpa = std.heap.GeneralPurposeAllocator(.{}){};
    defer _ = gpa.deinit();
    const allocator = gpa.allocator();

    var args_iter = try std.process.argsWithAllocator(allocator);
    defer args_iter.deinit();

    _ = args_iter.skip(); // skip executable name

    const cmd = args_iter.next() orelse {
        std.debug.print("Usage: blink-cli run <script_path>\n", .{});
        std.process.exit(1);
    };

    if (std.mem.eql(u8, cmd, "run")) {
        const script_path = args_iter.next() orelse {
            std.debug.print("Error: 'run' requires a script path.\n", .{});
            std.process.exit(1);
        };

        std.debug.print("Blink Host: Starting V-Hub Dispatcher on port 10000...\n", .{});
        var dispatcher = try vsock.VsockDispatcher.init(allocator, 10000);
        defer dispatcher.deinit();

        // In a real environment with libkrun installed, this boots the VM
        std.debug.print("Blink Host: Booting VM and triggering script: {s}\n", .{script_path});
        
        // We instantiate the blink core here
        const instance = try blink.BlinkInstance.create(allocator, 3); // CID 3
        defer instance.destroy();
        
        // Trigger is currently mocked to show the PathTranslator logic
        // In a production Linux env, this would call krun_start_enter
        try instance.trigger(script_path);

        std.debug.print("Blink Host: Waiting for Agent RpcRequest...\n", .{});
        try dispatcher.serve();
        
        std.debug.print("Blink Host: Task complete. Shutting down.\n", .{});
    } else {
        std.debug.print("Unknown command: {s}\n", .{cmd});
        std.process.exit(1);
    }
}
