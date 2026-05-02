use crate::hypervisor::Hypervisor;
use std::process::Command;

pub struct QemuDriver {
    pub vmlinux: String,
    pub initrd: String,
}

impl Hypervisor for QemuDriver {
    fn setup_filesystem(&self, root_path: &str, mappings: &[String]) -> Result<(), String> {
        // QEMU uses -virtfs for virtio-fs mapping.
        // In a real scenario, this would configure the QEMU command builder.
        println!("[QemuDriver] Configuring Virtio-FS root: {}", root_path);
        for mapping in mappings {
            println!("[QemuDriver] Mapping volume: {}", mapping);
        }
        Ok(())
    }

    fn inject_script(&self, script_path: &str, argv: &[&str], envp: &[&str]) -> Result<(), String> {
        println!("[QemuDriver] Injecting script: {} with args {:?}", script_path, argv);
        // This is where we'd add QEMU command arguments for the guest kernel parameters
        Ok(())
    }

    fn start_vcpu(&self) -> Result<(), String> {
        println!("[QemuDriver] Spawning QEMU process...");
        let mut child = Command::new("qemu-system-x86_64")
            .arg("-enable-kvm")
            .arg("-m").arg("512")
            .arg("-kernel").arg(&self.vmlinux)
            .arg("-initrd").arg(&self.initrd)
            // .arg("-append").arg("console=ttyS0 ...")
            .spawn()
            .map_err(|e| e.to_string())?;
        
        let _ = child.wait().map_err(|e| e.to_string())?;
        Ok(())
    }
}
