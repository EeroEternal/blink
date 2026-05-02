const std = @import("std");
pub fn main() void {
    const Type = @TypeOf(std.os.environ);
    std.debug.print("Type: {s}\n", .{@typeName(Type)});
}
