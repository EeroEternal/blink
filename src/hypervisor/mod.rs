use crate::protocol::VsockPacketHeader;

pub mod qemu;
pub use qemu::QemuDriver;
pub mod container;
pub use container::ContainerDriver;

pub trait Hypervisor {
    fn setup_filesystem(&self, root_path: &str, mappings: &[String]) -> Result<(), String>;
    fn inject_script(&self, script: &str, argv: &[&str], envp: &[&str]) -> Result<(), String>;
    fn start_vcpu(&self) -> Result<(), String>;
}
