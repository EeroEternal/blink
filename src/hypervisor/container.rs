use crate::hypervisor::Hypervisor;
use nix::sched::{clone, CloneFlags};
use nix::sys::wait::waitpid;
use nix::unistd::Pid;
use std::process::Command;

pub struct ContainerDriver {
    pub root_path: String,
}

impl Hypervisor for ContainerDriver {
    fn setup_filesystem(&self, root_path: &str, mappings: &[String]) -> Result<(), String> {
        // In a container context, we would perform 'pivot_root' or 'chroot'
        println!("[ContainerDriver] Configuring container root: {}", root_path);
        for mapping in mappings {
            println!("[ContainerDriver] Mapping volume: {}", mapping);
        }
        Ok(())
    }

    fn inject_script(&self, script_path: &str, argv: &[&str], envp: &[&str]) -> Result<(), String> {
        println!("[ContainerDriver] Preparing to run script: {} with args {:?}", script_path, argv);
        Ok(())
    }

    fn start_vcpu(&self) -> Result<(), String> {
        println!("[ContainerDriver] Cloning isolated process...");

        // Define a stack for the child process
        let mut stack = [0u8; 1024 * 1024];

        // Clone with Namespaces
        let flags = CloneFlags::CLONE_NEWPID 
            | CloneFlags::CLONE_NEWNS 
            | CloneFlags::CLONE_NEWUTS 
            | CloneFlags::CLONE_NEWIPC;

        let cb = Box::new(|| -> isize {
            println!("[ContainerDriver] Inside isolated namespace (PID: 1)");
            0
        });

        let child_pid = clone(cb, &mut stack, flags, Some(libc::SIGCHLD as i32))
            .map_err(|e| format!("Clone failed: {}", e))?;

        println!("[ContainerDriver] Spawned child process: {}", child_pid);

        // Wait for child
        waitpid(child_pid, None).map_err(|e| format!("Wait failed: {}", e))?;
        
        Ok(())
    }
}
