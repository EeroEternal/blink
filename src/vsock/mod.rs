use nix::sys::socket::{self, AddressFamily, SockFlag, SockType, SockProtocol};
use tokio::io::AsyncReadExt;
use std::os::fd::{FromRawFd, AsRawFd};
use crate::protocol::{VsockPacketHeader, MessageType, BLINK_MAGIC};

pub struct VsockListener {
    fd: i32,
}

impl VsockListener {
    pub fn bind(port: u32) -> Result<Self, Box<dyn std::error::Error>> {
        let fd = socket::socket(
            AddressFamily::Vsock,
            SockType::Stream,
            SockProtocol::from(0), // AF_VSOCK protocol
            SockFlag::SOCK_NONBLOCK,
        )?;
        
        use nix::sys::socket::sockaddr_vm;
        let addr = sockaddr_vm::new(nix::sys::socket::VMADDR_CID_ANY, port);
        socket::bind(fd, &addr)?;
        socket::listen(fd, 128)?;

        Ok(Self { fd })
    }

    pub async fn accept(&self) -> Result<i32, Box<dyn std::error::Error>> {
        let async_fd = tokio::io::unix::AsyncFd::new(unsafe { std::os::unix::io::OwnedFd::from_raw_fd(self.fd) })?;
        loop {
            let mut guard = async_fd.readable().await?;
            match socket::accept(guard.get_ref().as_raw_fd()) {
                Ok(fd) => {
                    let _ = async_fd.into_inner().into_raw_fd(); 
                    return Ok(fd);
                },
                Err(e) if e == nix::errno::Errno::EAGAIN => {
                    guard.clear_ready();
                    continue;
                }
                Err(e) => return Err(e.into()),
            }
        }
    }
}

pub async fn handle_agent(fd: i32) -> Result<(), Box<dyn std::error::Error>> {
    let mut stream = tokio::net::UnixStream::from_std(
        unsafe { std::os::unix::net::UnixStream::from_raw_fd(fd) }
    )?;
    
    let mut header_buf = [0u8; std::mem::size_of::<VsockPacketHeader>()];
    
    loop {
        match stream.read_exact(&mut header_buf).await {
            Ok(_) => {
                let header: VsockPacketHeader = unsafe { std::ptr::read(header_buf.as_ptr() as *const _) };
                let mut payload = vec![0u8; header.payload_len as usize];
                stream.read_exact(&mut payload).await?;

                match header.msg_type {
                    0x10 => println!("V-Hub: Received RpcRequest: {}", String::from_utf8_lossy(&payload)),
                    0x30 => print!("[Guest Stdout] {}", String::from_utf8_lossy(&payload)),
                    _ => println!("Unhandled message type: {}", header.msg_type),
                }
            },
            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
            Err(e) => return Err(e.into()),
        }
    }
    Ok(())
}
