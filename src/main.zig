const std = @import("std");
const blink = @import("blink.zig");

pub fn main() !void {
    var gpa = std.heap.GeneralPurposeAllocator(.{}){};
    defer _ = gpa.deinit();
    const allocator = gpa.allocator();

    // Since we don't have libkrun installed to actually run it natively here,
    // we'll just instantiate the PathTranslator to verify its behavior
    const translator = try blink.path_translator.PathTranslator.init(allocator);
    defer translator.deinit();

    std.debug.print("Host Python Path: {s}\n", .{translator.host_python_path});
    std.debug.print("Mapped Volume: {s}\n", .{translator.mapped_volume});
    std.debug.print("Injected Environment: {s}\n", .{translator.env_pythonpath});
}
