use std::os::fd::{FromRawFd, AsRawFd, OwnedFd, IntoRawFd};
use tokio::io::AsyncReadExt;
use crate::protocol::{VsockPacketHeader, MessageType, BLINK_MAGIC};

pub struct VsockListener {
    fd: OwnedFd,
}

impl VsockListener {
    pub fn bind(port: u32) -> Result<Self, Box<dyn std::error::Error>> {
        // Try AF_VSOCK
        let fd = unsafe {
            libc::socket(
                libc::AF_VSOCK,
                libc::SOCK_STREAM | libc::SOCK_NONBLOCK,
                0,
            )
        };
        
        let owned_fd = if fd >= 0 {
            let addr = libc::sockaddr_vm {
                svm_family: libc::AF_VSOCK as u16,
                svm_reserved1: 0,
                svm_port: port,
                svm_cid: libc::VMADDR_CID_ANY,
                svm_zero: [0; 4],
            };
            
            unsafe {
                libc::bind(fd, &addr as *const libc::sockaddr_vm as *const libc::sockaddr, std::mem::size_of::<libc::sockaddr_vm>() as u32);
                libc::listen(fd, 128);
                OwnedFd::from_raw_fd(fd)
            }
        } else {
            // Fallback TCP
            println!("[V-Hub] Vsock unavailable, falling back to TCP (127.0.0.1:{})", port);
            let tcp_fd = unsafe {
                libc::socket(
                    libc::AF_INET,
                    libc::SOCK_STREAM | libc::SOCK_NONBLOCK,
                    0,
                )
            };
            let tcp_fd = unsafe { OwnedFd::from_raw_fd(tcp_fd) };
            
            let addr = libc::sockaddr_in {
                sin_family: libc::AF_INET as u16,
                sin_port: port.to_be(),
                sin_addr: libc::in_addr { s_addr: 0x0100007f }, // 127.0.0.1
                sin_zero: [0; 8],
            };
            
            unsafe {
                libc::bind(tcp_fd.as_raw_fd(), &addr as *const libc::sockaddr_in as *const libc::sockaddr, std::mem::size_of::<libc::sockaddr_in>() as u32);
                libc::listen(tcp_fd.as_raw_fd(), 128);
            }
            tcp_fd
        };

        Ok(Self { fd: owned_fd })
    }

    pub async fn accept(&self) -> Result<OwnedFd, Box<dyn std::error::Error>> {
        let async_fd = tokio::io::unix::AsyncFd::new(unsafe { OwnedFd::from_raw_fd(self.fd.as_raw_fd()) })?;
        loop {
            let mut guard = async_fd.readable().await?;
            let client_fd = unsafe { libc::accept(self.fd.as_raw_fd(), std::ptr::null_mut(), std::ptr::null_mut()) };
            
            if client_fd >= 0 {
                return Ok(unsafe { OwnedFd::from_raw_fd(client_fd) });
            } else {
                guard.clear_ready();
                continue;
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
