const std = @import("std");
const posix = std.posix;

pub const AF_VSOCK: u16 = 40; // Linux specific Address Family for Vsock

pub const sockaddr_vm = extern struct {
    svm_family: u16 = AF_VSOCK,
    svm_reserved1: u16 = 0,
    svm_port: u32,
    svm_cid: u32,
    svm_zero: [4]u8 = [_]u8{0} ** 4,
};

/// VsockDispatcher serves as the "Message Gateway" for the V-Hub.
/// It uses a standard non-blocking poll loop to route Agent interactions.
pub const VsockDispatcher = struct {
    allocator: std.mem.Allocator,
    listen_fd: posix.fd_t,
    poll_fds: std.ArrayList(posix.pollfd),

    pub fn init(allocator: std.mem.Allocator, port: u32) !VsockDispatcher {
        const listen_fd = try posix.socket(AF_VSOCK, posix.SOCK.STREAM | posix.SOCK.NONBLOCK, 0);
        errdefer posix.close(listen_fd);

        var addr = sockaddr_vm{
            .svm_port = port,
            .svm_cid = 2, // VMADDR_CID_HOST
        };

        try posix.bind(listen_fd, @ptrCast(&addr), @sizeOf(sockaddr_vm));
        try posix.listen(listen_fd, 128);

        var poll_fds = std.ArrayList(posix.pollfd).init(allocator);
        try poll_fds.append(.{
            .fd = listen_fd,
            .events = posix.POLL.IN,
            .revents = 0,
        });

        return VsockDispatcher{
            .allocator = allocator,
            .listen_fd = listen_fd,
            .poll_fds = poll_fds,
        };
    }

    pub fn deinit(self: *VsockDispatcher) void {
        for (self.poll_fds.items) |pfd| {
            posix.close(pfd.fd);
        }
        self.poll_fds.deinit();
    }

    /// Event loop multiplexing Host and Agents
    pub fn serve(self: *VsockDispatcher) !void {
        while (true) {
            const num_events = posix.poll(self.poll_fds.items, -1) catch |err| {
                std.log.err("poll failed: {}", .{err});
                continue;
            };
            if (num_events == 0) continue;

            // Iterate backwards to safely remove closed fds
            var i: usize = self.poll_fds.items.len;
            while (i > 0) {
                i -= 1;
                const pfd = self.poll_fds.items[i];

                if (pfd.revents == 0) continue;

                if (pfd.fd == self.listen_fd) {
                    if ((pfd.revents & posix.POLL.IN) != 0) {
                        try self.acceptConnection();
                    }
                } else {
                    const keep_alive = self.handleClient(pfd.fd, pfd.revents) catch |err| blk: {
                        std.log.err("Client handler error: {}", .{err});
                        break :blk false;
                    };

                    if (!keep_alive) {
                        posix.close(pfd.fd);
                        _ = self.poll_fds.orderedRemove(i);
                    }
                }
            }
        }
    }

    fn acceptConnection(self: *VsockDispatcher) !void {
        var client_addr: sockaddr_vm = undefined;
        var addr_len: posix.socklen_t = @sizeOf(sockaddr_vm);
        const client_fd = posix.accept(self.listen_fd, @ptrCast(&client_addr), &addr_len, posix.SOCK.NONBLOCK) catch |err| {
            if (err == error.WouldBlock) return;
            return err;
        };

        std.log.info("New Agent connection from CID: {}", .{client_addr.svm_cid});

        try self.poll_fds.append(.{
            .fd = client_fd,
            .events = posix.POLL.IN | posix.POLL.ERR | posix.POLL.HUP,
            .revents = 0,
        });
    }

    /// Returns true if the connection should be kept alive, false if it should be closed
    fn handleClient(self: *VsockDispatcher, fd: posix.fd_t, revents: i16) !bool {
        _ = self;
        if ((revents & posix.POLL.HUP) != 0 or (revents & posix.POLL.ERR) != 0) {
            return false;
        }

        var buf: [4096]u8 = undefined;
        const len = posix.read(fd, &buf) catch |err| {
            if (err == error.WouldBlock) return true;
            return err;
        };

        if (len == 0) {
            return false; // EOF
        }

        const msg = buf[0..len];
        std.log.info("V-Hub intercepted {} bytes from Agent: {s}", .{len, msg});
        
        // Protocol Implementation: Handshake
        if (std.mem.startsWith(u8, msg, "Hello, Host!")) {
            std.log.info("V-Hub: Answering 'Hello, Blink!'", .{});
            _ = try posix.write(fd, "Hello, Blink!");
        } else {
            // Proxy routing implementation goes here
            _ = try posix.write(fd, msg);
        }

        return true;
    }
};