use nix::sys::socket::{self, AddressFamily, SockFlag, SockType, SockProtocol};
use tokio::io::AsyncReadExt;
use std::os::fd::{FromRawFd, AsRawFd, OwnedFd};
use crate::protocol::{VsockPacketHeader, MessageType, BLINK_MAGIC};

pub struct VsockListener {
    fd: OwnedFd,
}

impl VsockListener {
    pub fn bind(port: u32) -> Result<Self, Box<dyn std::error::Error>> {
        let fd = socket::socket(
            AddressFamily::Vsock,
            SockType::Stream,
            SockProtocol::from(0),
            SockFlag::SOCK_NONBLOCK,
        )?;
        
        let owned_fd = unsafe { OwnedFd::from_raw_fd(fd) };

        // For AF_VSOCK, we need to manually construct sockaddr_vm or use nix if supported.
        // Since nix might not expose sockaddr_vm easily, we use raw libc structures if needed.
        // Actually nix::sys::socket::UnixAddr is for unix. 
        // Let's use libc::sockaddr_vm directly for vsock.
        let addr = libc::sockaddr_vm {
            svm_family: libc::AF_VSOCK as u16,
            svm_reserved1: 0,
            svm_port: port,
            svm_cid: libc::VMADDR_CID_ANY,
            svm_zero: [0; 4],
        };

        let addr_ptr = &addr as *const libc::sockaddr_vm as *const libc::sockaddr;
        socket::bind(owned_fd.as_raw_fd(), addr_ptr, std::mem::size_of::<libc::sockaddr_vm>() as u32)?;
        socket::listen(owned_fd.as_raw_fd(), 128)?;

        Ok(Self { fd: owned_fd })
    }

    pub async fn accept(&self) -> Result<OwnedFd, Box<dyn std::error::Error>> {
        let async_fd = tokio::io::unix::AsyncFd::new(unsafe { OwnedFd::from_raw_fd(self.fd.as_raw_fd()) })?;
        loop {
            let mut guard = async_fd.readable().await?;
            match socket::accept(self.fd.as_raw_fd()) {
                Ok(fd) => {
                    return Ok(unsafe { OwnedFd::from_raw_fd(fd) });
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

pub async fn handle_agent(owned_fd: OwnedFd) -> Result<(), Box<dyn std::error::Error>> {
    let mut stream = tokio::net::UnixStream::from_std(
        unsafe { std::os::unix::net::UnixStream::from_raw_fd(owned_fd.into_raw_fd()) }
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
