use nix::sys::socket::{self, AddressFamily, SockFlag, SockType, VsockAddr};
use tokio::io::{AsyncReadExt};
use tokio::net::UnixListener; // Note: We use raw sockets for Vsock
use std::os::unix::io::FromRawFd;
use tokio::net::TcpStream; // We need a custom wrapper for Vsock
use crate::protocol::{VsockPacketHeader, MessageType, BLINK_MAGIC};

// Vsock is not natively supported in tokio::net, so we use mio/nix to wrap it
pub struct VsockListener {
    fd: i32,
}

impl VsockListener {
    pub fn bind(port: u32) -> Result<Self, Box<dyn std::error::Error>> {
        let fd = socket::socket(
            AddressFamily::Vsock,
            SockType::Stream,
            socket::SockProtocol::Connect,
            SockFlag::SOCK_NONBLOCK,
        )?;

        let addr = VsockAddr::new(socket::VMADDR_CID_ANY, port);
        socket::bind(fd, &addr)?;
        socket::listen(fd, 128)?;

        Ok(Self { fd })
    }

    pub async fn accept(&self) -> Result<i32, Box<dyn std::error::Error>> {
        loop {
            match socket::accept(self.fd) {
                Ok(client_fd) => return Ok(client_fd),
                Err(e) if e == nix::errno::Errno::EAGAIN => {
                    tokio::task::yield_now().await;
                    continue;
                }
                Err(e) => return Err(e.into()),
            }
        }
    }
}

pub async fn handle_agent(fd: i32) -> Result<(), Box<dyn std::error::Error>> {
    let mut file = unsafe { std::fs::File::from_raw_fd(fd) };
    let mut header_buf = [0u8; std::mem::size_of::<VsockPacketHeader>()];
    
    loop {
        // 1. Read Header
        if let Err(e) = file.read_exact(&mut header_buf).await {
            if e.kind() == std::io::ErrorKind::UnexpectedEof { break; }
            return Err(e.into());
        }

        let header: VsockPacketHeader = unsafe { std::ptr::read(header_buf.as_ptr() as *const _) };
        
        // 2. Read Payload
        let mut payload = vec![0u8; header.payload_len as usize];
        file.read_exact(&mut payload).await?;

        // 3. Logic based on MessageType
        match header.msg_type {
            0x10 => { // RpcRequest
                println!("V-Hub: Received RpcRequest: {}", String::from_utf8_lossy(&payload));
                // Reply would go here...
            },
            0x30 => { // Stdout
                print!("[Guest Stdout] {}", String::from_utf8_lossy(&payload));
            },
            _ => println!("Unhandled message type: {}", header.msg_type),
        }
    }
    Ok(())
}
