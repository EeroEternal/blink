const std = @import("std");

pub const PathTranslator = struct {
    allocator: std.mem.Allocator,
    host_python_path: [:0]u8,
    mapped_volume: [:0]u8,
    env_pythonpath: [:0]u8,

    /// Initializes the PathTranslator by discovering the host's Python environment.
    pub fn init(allocator: std.mem.Allocator) !*PathTranslator {
        // 1. Get host python path using sys.prefix
        const argv = [_][]const u8{ "python3", "-c", "import sys; print(sys.prefix, end='')" };
        
        const result = try std.process.Child.run(.{
            .allocator = allocator,
            .argv = &argv,
        });
        defer allocator.free(result.stdout);
        defer allocator.free(result.stderr);

        if (result.stdout.len == 0) {
            std.log.err("Failed to discover host python path", .{});
            return error.PythonNotFound;
        }

        // 2. Format the mapping string for krun_set_mapped_volumes
        // Format: host_path:guest_path
        const host_path = result.stdout;
        const guest_path = "/lib/runtime";
        
        const mapped_volume_str = try std.fmt.allocPrint(allocator, "{s}:{s}", .{ host_path, guest_path });
        defer allocator.free(mapped_volume_str);
        const mapped_volume = try allocator.dupeZ(u8, mapped_volume_str);
        errdefer allocator.free(mapped_volume);

        const host_python_path = try allocator.dupeZ(u8, host_path);
        errdefer allocator.free(host_python_path);

        // 3. Construct PYTHONPATH to point to the guest path
        // We add both /lib/runtime and /lib/runtime/lib to the PYTHONPATH
        const env_pythonpath_str = try std.fmt.allocPrint(allocator, "PYTHONPATH={s}:{s}/lib", .{ guest_path, guest_path });
        defer allocator.free(env_pythonpath_str);
        const env_pythonpath = try allocator.dupeZ(u8, env_pythonpath_str);
        errdefer allocator.free(env_pythonpath);

        const self = try allocator.create(PathTranslator);
        self.* = .{
            .allocator = allocator,
            .host_python_path = host_python_path,
            .mapped_volume = mapped_volume,
            .env_pythonpath = env_pythonpath,
        };
        return self;
    }

    /// Free allocated strings and the object itself.
    pub fn deinit(self: *PathTranslator) void {
        self.allocator.free(self.host_python_path);
        self.allocator.free(self.mapped_volume);
        self.allocator.free(self.env_pythonpath);
        self.allocator.destroy(self);
    }
};

test "PathTranslator basic" {
    var gpa = std.heap.GeneralPurposeAllocator(.{}){};
    defer _ = gpa.deinit();
    const allocator = gpa.allocator();

    const translator = try PathTranslator.init(allocator);
    defer translator.deinit();
    
    std.debug.print("\nHost Python: {s}\n", .{translator.host_python_path});
    std.debug.print("Mapped Volume: {s}\n", .{translator.mapped_volume});
    std.debug.print("PYTHONPATH: {s}\n", .{translator.env_pythonpath});
}
