const std = @import("std");
const blink = @import("blink.zig");
const vsock = @import("vsock.zig");

pub fn main() !void {
    var gpa = std.heap.GeneralPurposeAllocator(.{}){};
    defer _ = gpa.deinit();
    const allocator = gpa.allocator();

    // Log to stderr so it doesn't pollute stdout where the JSON output will go
    std.debug.print("Blink Host: Starting V-Hub Dispatcher on port 10000...\n", .{});

    var dispatcher = try vsock.VsockDispatcher.init(allocator, 10000);
    defer dispatcher.deinit();

    // Start listening for the agent.
    // In a real environment with libkrun installed:
    // const instance = try blink.BlinkInstance.create(allocator, 3);
    // defer instance.destroy();
    // try instance.trigger("script.py");

    std.debug.print("Blink Host: Waiting for Agent RpcRequest...\n", .{});
    
    // This will block until it receives an RpcRequest from the agent,
    // print the JSON to stdout, and then cleanly exit.
    try dispatcher.serve();
    
    std.debug.print("Blink Host: Shutting down.\n", .{});
}
