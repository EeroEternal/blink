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
/// It uses epoll for non-blocking asynchronous event loop to route Agent interactions.
pub const VsockDispatcher = struct {
    allocator: std.mem.Allocator,
    epfd: posix.fd_t,
    listen_fd: posix.fd_t,

    pub fn init(allocator: std.mem.Allocator, port: u32) !VsockDispatcher {
        const listen_fd = try posix.socket(AF_VSOCK, posix.SOCK.STREAM | posix.SOCK.NONBLOCK, 0);
        errdefer posix.close(listen_fd);

        var addr = sockaddr_vm{
            .svm_port = port,
            .svm_cid = 2, // VMADDR_CID_HOST
        };

        try posix.bind(listen_fd, @ptrCast(&addr), @sizeOf(sockaddr_vm));
        try posix.listen(listen_fd, 128);

        // epoll setup for asynchronous dispatching
        const epfd = try posix.epoll_create1(0);
        errdefer posix.close(epfd);

        var ev = posix.epoll_event{
            .events = posix.linux.EPOLL.IN,
            .data = .{ .fd = listen_fd },
        };
        try posix.epoll_ctl(epfd, posix.linux.EPOLL.CTL_ADD, listen_fd, &ev);

        return VsockDispatcher{
            .allocator = allocator,
            .epfd = epfd,
            .listen_fd = listen_fd,
        };
    }

    pub fn deinit(self: *VsockDispatcher) void {
        posix.close(self.listen_fd);
        posix.close(self.epfd);
    }

    /// Event loop multiplexing Host and Agents
    pub fn serve(self: *VsockDispatcher) !void {
        var events: [64]posix.epoll_event = undefined;

        while (true) {
            const num_events = posix.epoll_wait(self.epfd, &events, -1);
            if (num_events == 0) continue;

            for (events[0..num_events]) |ev| {
                if (ev.data.fd == self.listen_fd) {
                    try self.acceptConnection();
                } else {
                    self.handleClient(ev) catch |err| {
                        std.log.err("Client handler error: {}", .{err});
                        posix.close(ev.data.fd);
                    };
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

        var new_ev = posix.epoll_event{
            .events = posix.linux.EPOLL.IN | posix.linux.EPOLL.RDHUP,
            .data = .{ .fd = client_fd },
        };
        try posix.epoll_ctl(self.epfd, posix.linux.EPOLL.CTL_ADD, client_fd, &new_ev);
    }

    fn handleClient(self: *VsockDispatcher, ev: posix.epoll_event) !void {
        _ = self;
        if ((ev.events & posix.linux.EPOLL.RDHUP) != 0) {
            posix.close(ev.data.fd);
            return;
        }

        var buf: [4096]u8 = undefined;
        const len = posix.read(ev.data.fd, &buf) catch |err| {
            if (err == error.WouldBlock) return;
            return err;
        };

        if (len == 0) {
            posix.close(ev.data.fd);
            return;
        }

        const msg = buf[0..len];
        std.log.info("V-Hub intercepted {} bytes from Agent. Auditing/Routing...", .{len});
        
        // Proxy routing implementation goes here
        // E.g., matching destination CID and forwarding packets
        _ = try posix.write(ev.data.fd, msg);
    }
};