const std = @import("std");
pub fn main() void {
    var gpa = std.heap.GeneralPurposeAllocator(.{}){};
    const allocator = gpa.allocator();
    var list = std.ArrayList([]const u8).init(allocator);
    _ = list;
}
