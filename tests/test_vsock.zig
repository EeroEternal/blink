const std = @import("std");
const posix = std.posix;
pub fn main() void {
    var gpa = std.heap.GeneralPurposeAllocator(.{}){};
    const allocator = gpa.allocator();
    var list = std.ArrayList(posix.pollfd).init(allocator);
    _ = list;
}
