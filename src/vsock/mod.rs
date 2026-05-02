use nix::sys::socket::{self, AddressFamily, SockFlag, SockType};
use tokio::io::unix::AsyncFd;
use std::os::fd::{FromRawFd, IntoRawFd};
use crate::protocol::{VsockPacketHeader, MessageType, BLINK_MAGIC};

pub struct VsockListener {
    async_fd: AsyncFd<std::os::unix::io::OwnedFd>,
}

impl VsockListener {
    pub fn bind(port: u32) -> Result<Self, Box<dyn std::error::Error>> {
        let fd = socket::socket(
            AddressFamily::Vsock,
            SockType::Stream,
            socket::SockProtocol::Connect,
            SockFlag::SOCK_NONBLOCK,
        )?;
        
        let owned_fd = unsafe { std::os::unix::io::OwnedFd::from_raw_fd(fd) };

        // Bind and Listen using nix
        // Note: For Vsock, we use VsockAddr, but we must use it correctly with nix
        use nix::sys::socket::sockaddr_vm;
        let addr = sockaddr_vm::new(2, port); // VMADDR_CID_HOST
        socket::bind(owned_fd.as_raw_fd(), &addr)?;
        socket::listen(owned_fd.as_raw_fd(), 128)?;

        Ok(Self { async_fd: AsyncFd::new(owned_fd)? })
    }

    pub async fn accept(&self) -> Result<std::os::unix::io::OwnedFd, Box<dyn std::error::Error>> {
        loop {
            let mut guard = self.async_fd.readable().await?;
            match socket::accept(guard.get_ref().as_raw_fd()) {
                Ok(fd) => return Ok(unsafe { std::os::unix::io::OwnedFd::from_raw_fd(fd) }),
                Err(e) if e == nix::errno::Errno::EAGAIN => {
                    guard.clear_ready();
                    continue;
                }
                Err(e) => return Err(e.into()),
            }
        }
    }
}

pub async fn handle_agent(owned_fd: std::os::unix::io::OwnedFd) -> Result<(), Box<dyn std::error::Error>> {
    let mut async_fd = AsyncFd::new(owned_fd)?;
    let mut header_buf = [0u8; std::mem::size_of::<VsockPacketHeader>()];
    
    loop {
        let mut guard = async_fd.readable().await?;
        
        // Non-blocking read for header
        let mut reader = &*guard.get_ref();
        use std::io::Read;
        
        match reader.read_exact(&mut header_buf) {
            Ok(_) => {
                let header: VsockPacketHeader = unsafe { std::ptr::read(header_buf.as_ptr() as *const _) };
                let mut payload = vec![0u8; header.payload_len as usize];
                reader.read_exact(&mut payload)?;

                match header.msg_type {
                    0x10 => println!("V-Hub: Received RpcRequest: {}", String::from_utf8_lossy(&payload)),
                    0x30 => print!("[Guest Stdout] {}", String::from_utf8_lossy(&payload)),
                    _ => println!("Unhandled message type: {}", header.msg_type),
                }
            },
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                guard.clear_ready();
                continue;
            },
            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
            Err(e) => return Err(e.into()),
        }
    }
    Ok(())
}
