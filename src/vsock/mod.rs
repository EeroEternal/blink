use nix::sys::socket::{self, SockFlag, SockType, AddressFamily, InetAddr, SockAddr};
use tokio::io::AsyncReadExt;
use std::os::fd::{FromRawFd, AsRawFd, OwnedFd, IntoRawFd};
use crate::protocol::{VsockPacketHeader, MessageType, BLINK_MAGIC};

pub struct VsockListener {
    fd: OwnedFd,
    is_vsock: bool,
}

impl VsockListener {
    pub fn bind(port: u32) -> Result<Self, Box<dyn std::error::Error>> {
        // Try AF_VSOCK first
        let fd_res = socket::socket(
            AddressFamily::Vsock,
            SockType::Stream,
            None,
            SockFlag::SOCK_NONBLOCK,
        );

        match fd_res {
            Ok(fd) => {
                let owned_fd = unsafe { OwnedFd::from_raw_fd(fd) };
                let addr = libc::sockaddr_vm {
                    svm_family: libc::AF_VSOCK as u16,
                    svm_reserved1: 0,
                    svm_port: port,
                    svm_cid: libc::VMADDR_CID_ANY,
                    svm_zero: [0; 4],
                };
                let res = unsafe {
                    libc::bind(owned_fd.as_raw_fd(), &addr as *const libc::sockaddr_vm as *const libc::sockaddr, std::mem::size_of::<libc::sockaddr_vm>() as u32)
                };
                if res == 0 {
                    unsafe { libc::listen(owned_fd.as_raw_fd(), 128) };
                    return Ok(Self { fd: owned_fd, is_vsock: true });
                }
            }
            Err(_) => {}
        }

        // Fallback to TCP/IP (localhost)
        println!("[V-Hub] Vsock unavailable, falling back to TCP (127.0.0.1:{})", port);
        let fd = socket::socket(
            AddressFamily::Inet,
            SockType::Stream,
            None,
            SockFlag::SOCK_NONBLOCK,
        )?;
        let owned_fd = unsafe { OwnedFd::from_raw_fd(fd) };
        
        let addr = nix::sys::socket::SockAddr::new_inet(nix::sys::socket::InetAddr::new(
            nix::sys::socket::Ipv4Addr::new(127, 0, 0, 1).into(),
            port as u16,
        ));
        socket::bind(owned_fd.as_raw_fd(), &addr)?;
        socket::listen(owned_fd.as_raw_fd(), 128)?;

        Ok(Self { fd: owned_fd, is_vsock: false })
    }

    pub async fn accept(&self) -> Result<OwnedFd, Box<dyn std::error::Error>> {
        let async_fd = tokio::io::unix::AsyncFd::new(unsafe { OwnedFd::from_raw_fd(self.fd.as_raw_fd()) })?;
        loop {
            let mut guard = async_fd.readable().await?;
            match socket::accept(self.fd.as_raw_fd()) {
                Ok(fd) => return Ok(unsafe { OwnedFd::from_raw_fd(fd) }),
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
    let mut stream = tokio::net::TcpStream::from_std(
        unsafe { std::net::TcpStream::from_raw_fd(owned_fd.into_raw_fd()) }
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
