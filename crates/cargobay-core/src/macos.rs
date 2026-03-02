// macOS hypervisor: Apple Virtualization.framework with Rosetta + VirtioFS support.
//
// Rosetta: On Apple Silicon, VZLinuxRosettaDirectoryShare provides x86_64 → arm64
// translation inside Linux VMs. The Rosetta binary is mounted and registered
// via binfmt_misc so x86_64 ELF binaries run transparently.
//
// VirtioFS: VZVirtioFileSystemDeviceConfiguration allows sharing host directories
// with near-native filesystem performance (faster than 9p/NFS).

use crate::hypervisor::{
    Hypervisor, HypervisorError, PortForward, SharedDirectory, VmConfig, VmInfo, VmState,
};
use crate::store::{data_dir, next_id_for_prefix, VmStore};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tracing::{info, warn};

/// macOS hypervisor backed by Apple Virtualization.framework.
pub struct MacOSHypervisor {
    vms: Mutex<HashMap<String, VmEntry>>,
    next_id: Mutex<u64>,
    store: VmStore,
}

impl Default for MacOSHypervisor {
    fn default() -> Self {
        Self::new()
    }
}

struct VmEntry {
    info: VmInfo,
    /// VZ configuration parameters stored for lifecycle management.
    _rosetta_mounted: bool,
    runner_pid: Option<u32>,
    runner: Option<Child>,
}

fn vm_dir(id: &str) -> PathBuf {
    data_dir().join("vms").join(id)
}

fn vm_disk_path(id: &str) -> PathBuf {
    vm_dir(id).join("disk.raw")
}

fn vm_console_log_path(id: &str) -> PathBuf {
    vm_dir(id).join("console.log")
}

fn vm_runner_pid_path(id: &str) -> PathBuf {
    vm_dir(id).join("runner.pid")
}

fn vm_runner_ready_path(id: &str) -> PathBuf {
    vm_dir(id).join("runner.ready")
}

fn read_pid_file(path: &Path) -> Option<u32> {
    let content = std::fs::read_to_string(path).ok()?;
    content.trim().parse::<u32>().ok()
}

fn pid_alive(pid: u32) -> bool {
    let rc = unsafe { libc::kill(pid as i32, 0) };
    if rc == 0 {
        return true;
    }
    let err = std::io::Error::last_os_error();
    matches!(err.raw_os_error(), Some(libc::EPERM))
}

impl MacOSHypervisor {
    pub fn new() -> Self {
        let store = VmStore::new();
        let loaded = match store.load_vms() {
            Ok(v) => v,
            Err(e) => {
                warn!(
                    "Failed to load VM store ({}): {}",
                    store.path().display(),
                    e
                );
                vec![]
            }
        };

        let mut map: HashMap<String, VmEntry> = HashMap::new();
        for mut vm in loaded.iter().cloned() {
            let pid_path = vm_runner_pid_path(&vm.id);
            let ready_path = vm_runner_ready_path(&vm.id);

            let runner_pid = read_pid_file(&pid_path).filter(|pid| pid_alive(*pid));
            if runner_pid.is_some() {
                vm.state = VmState::Running;
            } else {
                if pid_path.exists() {
                    let _ = std::fs::remove_file(&pid_path);
                }
                if ready_path.exists() {
                    let _ = std::fs::remove_file(&ready_path);
                }
                vm.state = VmState::Stopped;
            }

            map.insert(
                vm.id.clone(),
                VmEntry {
                    info: vm,
                    _rosetta_mounted: false,
                    runner_pid,
                    runner: None,
                },
            );
        }

        let next_id = next_id_for_prefix(&loaded, "vz-");
        Self {
            vms: Mutex::new(map),
            next_id: Mutex::new(next_id),
            store,
        }
    }

    /// Check if Rosetta is available on this Mac.
    /// Rosetta is only available on Apple Silicon (aarch64) running macOS 13+.
    fn check_rosetta_availability() -> bool {
        // Runtime check: arch must be aarch64
        #[cfg(target_arch = "aarch64")]
        {
            // Check if the Rosetta runtime exists
            std::path::Path::new("/Library/Apple/usr/libexec/oah/libRosettaRuntime").exists()
                || std::path::Path::new("/usr/libexec/rosetta").exists()
        }
        #[cfg(not(target_arch = "aarch64"))]
        {
            false
        }
    }

    fn persist(&self) -> Result<(), HypervisorError> {
        let vms = self
            .vms
            .lock()
            .unwrap()
            .values()
            .map(|e| e.info.clone())
            .collect::<Vec<_>>();
        self.store.save_vms(&vms)
    }

    fn vz_runner_path() -> PathBuf {
        if let Ok(path) = std::env::var("CARGOBAY_VZ_RUNNER_PATH") {
            return PathBuf::from(path);
        }

        if let Ok(exe) = std::env::current_exe() {
            if let Some(dir) = exe.parent() {
                let candidate = dir.join("cargobay-vz");
                if candidate.is_file() {
                    return candidate;
                }
            }
        }

        PathBuf::from("cargobay-vz")
    }

    fn spawn_vz_runner(&self, vm: &VmInfo) -> Result<Child, HypervisorError> {
        let kernel = std::env::var("CARGOBAY_VZ_KERNEL").map_err(|_| {
            HypervisorError::CreateFailed(
                "CARGOBAY_VZ_KERNEL is required to start a macOS VZ VM".into(),
            )
        })?;
        let initrd = std::env::var("CARGOBAY_VZ_INITRD").ok();
        let cmdline =
            std::env::var("CARGOBAY_VZ_CMDLINE").unwrap_or_else(|_| "console=hvc0".into());

        let disk = vm_disk_path(&vm.id);
        if !disk.exists() {
            return Err(HypervisorError::CreateFailed(format!(
                "VM disk image not found: {}",
                disk.display()
            )));
        }

        let ready_file = vm_runner_ready_path(&vm.id);
        let _ = std::fs::remove_file(&ready_file);

        let console_log = vm_console_log_path(&vm.id);
        let console_file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&console_log)?;
        let console_err = console_file.try_clone()?;

        let mut cmd = Command::new(Self::vz_runner_path());
        cmd.arg("--kernel")
            .arg(kernel)
            .arg("--disk")
            .arg(disk)
            .arg("--cpus")
            .arg(vm.cpus.to_string())
            .arg("--memory-mb")
            .arg(vm.memory_mb.to_string())
            .arg("--cmdline")
            .arg(cmdline)
            .arg("--ready-file")
            .arg(&ready_file);

        if let Some(initrd) = initrd {
            cmd.arg("--initrd").arg(initrd);
        }

        // Pass Rosetta flag if enabled.
        if vm.rosetta_enabled {
            cmd.arg("--rosetta");
        }

        // Pass shared directories.
        for share in &vm.shared_dirs {
            let spec = if share.read_only {
                format!("{}:{}:ro", share.tag, share.host_path)
            } else {
                format!("{}:{}", share.tag, share.host_path)
            };
            cmd.arg("--share").arg(spec);
        }

        cmd.stdin(Stdio::null())
            .stdout(Stdio::from(console_file))
            .stderr(Stdio::from(console_err));

        let child = cmd.spawn()?;
        Ok(child)
    }
}

impl Hypervisor for MacOSHypervisor {
    fn create_vm(&self, config: VmConfig) -> Result<String, HypervisorError> {
        // Validate Rosetta request
        if config.rosetta && !Self::check_rosetta_availability() {
            return Err(HypervisorError::RosettaUnavailable(
                "Rosetta is only available on Apple Silicon Macs with macOS 13+".into(),
            ));
        }

        // Validate shared directory paths
        for dir in &config.shared_dirs {
            if !std::path::Path::new(&dir.host_path).exists() {
                return Err(HypervisorError::VirtioFsError(format!(
                    "Host path does not exist: {}",
                    dir.host_path
                )));
            }
        }

        {
            let vms = self.vms.lock().unwrap();
            if vms.values().any(|e| e.info.name == config.name) {
                return Err(HypervisorError::CreateFailed(format!(
                    "VM name already exists: {}",
                    config.name
                )));
            }
        }

        let mut id_counter = self.next_id.lock().unwrap();
        let id = format!("vz-{}", *id_counter);
        *id_counter += 1;

        let vm_dir = vm_dir(&id);
        std::fs::create_dir_all(&vm_dir)?;
        let disk_path = vm_disk_path(&id);
        let disk_bytes = config
            .disk_gb
            .checked_mul(1024 * 1024 * 1024)
            .ok_or_else(|| HypervisorError::CreateFailed("disk size overflow".into()))?;
        {
            let file = std::fs::File::create(&disk_path)?;
            file.set_len(disk_bytes)?;
        }

        let info = VmInfo {
            id: id.clone(),
            name: config.name,
            state: VmState::Stopped,
            cpus: config.cpus,
            memory_mb: config.memory_mb,
            disk_gb: config.disk_gb,
            rosetta_enabled: config.rosetta,
            shared_dirs: config.shared_dirs,
            port_forwards: config.port_forwards,
        };

        let entry = VmEntry {
            info,
            _rosetta_mounted: false,
            runner_pid: None,
            runner: None,
        };

        self.vms.lock().unwrap().insert(id.clone(), entry);
        if let Err(e) = self.persist() {
            self.vms.lock().unwrap().remove(&id);
            let _ = std::fs::remove_dir_all(&vm_dir);
            return Err(e);
        }

        // TODO: Real implementation using Virtualization.framework FFI:
        // 1. Create VZVirtualMachineConfiguration
        // 2. Set VZLinuxBootLoader with kernel/initrd
        // 3. Configure VZVirtioNetworkDeviceConfiguration
        // 4. Configure VZVirtioBlockDeviceConfiguration for disk
        // 5. If rosetta: Add VZLinuxRosettaDirectoryShare
        // 6. For each shared_dir: Add VZVirtioFileSystemDeviceConfiguration
        //    with VZSharedDirectory → VZSingleDirectoryShare
        // 7. Validate configuration

        Ok(id)
    }

    fn start_vm(&self, id: &str) -> Result<(), HypervisorError> {
        let (already_running, need_persist, vm_info) = {
            let mut vms = self.vms.lock().unwrap();
            let entry = vms
                .get_mut(id)
                .ok_or(HypervisorError::NotFound(id.into()))?;

            let mut already_running = false;
            let mut need_persist = false;

            if let Some(pid) = entry.runner_pid {
                if pid_alive(pid) {
                    already_running = true;
                    need_persist = entry.info.state != VmState::Running;
                    entry.info.state = VmState::Running;
                } else {
                    entry.runner_pid = None;
                    let _ = std::fs::remove_file(vm_runner_pid_path(id));
                    let _ = std::fs::remove_file(vm_runner_ready_path(id));
                }
            }

            if !already_running && entry.runner.is_some() {
                already_running = true;
                need_persist = entry.info.state != VmState::Running;
                entry.info.state = VmState::Running;
            }

            (already_running, need_persist, entry.info.clone())
        };

        if already_running {
            if need_persist {
                let _ = self.persist();
            }
            return Ok(());
        }

        let mut child = self.spawn_vz_runner(&vm_info)?;

        let ready_file = vm_runner_ready_path(&vm_info.id);
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            if ready_file.exists() {
                break;
            }

            if let Ok(Some(status)) = child.try_wait() {
                return Err(HypervisorError::CreateFailed(format!(
                    "cargobay-vz exited early: {}",
                    status
                )));
            }

            if Instant::now() >= deadline {
                let _ = child.kill();
                let _ = child.wait();
                return Err(HypervisorError::CreateFailed(
                    "Timed out waiting for VM to start".into(),
                ));
            }

            std::thread::sleep(Duration::from_millis(200));
        }

        let pid = child.id();

        let previous_state = {
            let mut vms = self.vms.lock().unwrap();
            let entry = vms
                .get_mut(id)
                .ok_or(HypervisorError::NotFound(id.into()))?;
            let prev = entry.info.state.clone();
            entry.info.state = VmState::Running;
            entry.runner_pid = Some(pid);
            entry.runner = Some(child);
            prev
        };

        if let Err(e) = self.persist() {
            let mut vms = self.vms.lock().unwrap();
            if let Some(entry) = vms.get_mut(id) {
                entry.info.state = previous_state;
                if let Some(mut child) = entry.runner.take() {
                    let _ = child.kill();
                    let _ = child.wait();
                }
                entry.runner_pid = None;
            }
            return Err(e);
        }

        let _ = std::fs::write(vm_runner_pid_path(&vm_info.id), format!("{}\n", pid));
        info!("Started VZ VM {} (pid {})", vm_info.id, pid);
        Ok(())
    }

    fn stop_vm(&self, id: &str) -> Result<(), HypervisorError> {
        let (child, pid_opt, previous_state, rosetta_prev) = {
            let mut vms = self.vms.lock().unwrap();
            let entry = vms
                .get_mut(id)
                .ok_or(HypervisorError::NotFound(id.into()))?;
            let prev = entry.info.state.clone();
            let rosetta_prev = entry._rosetta_mounted;
            let child = entry.runner.take();
            let pid_opt = entry.runner_pid;
            entry.info.state = VmState::Stopped;
            entry._rosetta_mounted = false;
            entry.runner_pid = None;
            (child, pid_opt, prev, rosetta_prev)
        };

        if let Some(mut child) = child {
            let _ = child.kill();
            let _ = child.wait();
        } else if let Some(pid) = pid_opt {
            let _ = unsafe { libc::kill(pid as i32, libc::SIGKILL) };
        }

        let _ = std::fs::remove_file(vm_runner_pid_path(id));
        let _ = std::fs::remove_file(vm_runner_ready_path(id));

        if let Err(e) = self.persist() {
            let mut vms = self.vms.lock().unwrap();
            if let Some(entry) = vms.get_mut(id) {
                entry.info.state = previous_state;
                entry._rosetta_mounted = rosetta_prev;
                entry.runner_pid = pid_opt;
            }
            return Err(e);
        }

        info!("Stopped VZ VM {}", id);
        Ok(())
    }

    fn delete_vm(&self, id: &str) -> Result<(), HypervisorError> {
        // Best-effort stop before deletion.
        let _ = self.stop_vm(id);

        let removed = self
            .vms
            .lock()
            .unwrap()
            .remove(id)
            .ok_or(HypervisorError::NotFound(id.into()))?;
        if let Err(e) = self.persist() {
            self.vms.lock().unwrap().insert(id.to_string(), removed);
            return Err(e);
        }

        let _ = std::fs::remove_dir_all(vm_dir(id));
        Ok(())
    }

    fn list_vms(&self) -> Result<Vec<VmInfo>, HypervisorError> {
        let mut changed = false;
        {
            let mut vms = self.vms.lock().unwrap();
            for entry in vms.values_mut() {
                if entry
                    .runner
                    .as_mut()
                    .and_then(|c| c.try_wait().ok())
                    .flatten()
                    .is_some()
                {
                    entry.runner = None;
                    entry.runner_pid = None;
                    entry.info.state = VmState::Stopped;
                    let _ = std::fs::remove_file(vm_runner_pid_path(&entry.info.id));
                    let _ = std::fs::remove_file(vm_runner_ready_path(&entry.info.id));
                    changed = true;
                    continue;
                }

                if let Some(pid) = entry.runner_pid {
                    if !pid_alive(pid) {
                        entry.runner_pid = None;
                        entry.info.state = VmState::Stopped;
                        let _ = std::fs::remove_file(vm_runner_pid_path(&entry.info.id));
                        let _ = std::fs::remove_file(vm_runner_ready_path(&entry.info.id));
                        changed = true;
                        continue;
                    }
                    if entry.info.state != VmState::Running {
                        entry.info.state = VmState::Running;
                        changed = true;
                    }
                }
            }
        }
        if changed {
            let _ = self.persist();
        }

        Ok(self
            .vms
            .lock()
            .unwrap()
            .values()
            .map(|e| e.info.clone())
            .collect())
    }

    fn rosetta_available(&self) -> bool {
        Self::check_rosetta_availability()
    }

    fn mount_virtiofs(&self, vm_id: &str, share: &SharedDirectory) -> Result<(), HypervisorError> {
        if !std::path::Path::new(&share.host_path).exists() {
            return Err(HypervisorError::VirtioFsError(format!(
                "Host path does not exist: {}",
                share.host_path
            )));
        }

        let mut vms = self.vms.lock().unwrap();
        let entry = vms
            .get_mut(vm_id)
            .ok_or(HypervisorError::NotFound(vm_id.into()))?;

        // Check for duplicate tag
        if entry.info.shared_dirs.iter().any(|d| d.tag == share.tag) {
            return Err(HypervisorError::VirtioFsError(format!(
                "Mount tag already exists: {}",
                share.tag
            )));
        }

        entry.info.shared_dirs.push(share.clone());
        drop(vms);
        if let Err(e) = self.persist() {
            let mut vms = self.vms.lock().unwrap();
            if let Some(entry) = vms.get_mut(vm_id) {
                entry.info.shared_dirs.retain(|d| d.tag != share.tag);
            }
            return Err(e);
        }

        // TODO: Real implementation using Virtualization.framework:
        // 1. Create VZSharedDirectory(url: hostPath, readOnly: readOnly)
        // 2. Create VZSingleDirectoryShare(directory: sharedDir)
        // 3. Create VZVirtioFileSystemDeviceConfiguration(tag: tag)
        // 4. Attach to running VM
        // 5. mount -t virtiofs <tag> <guest_path> inside VM via agent

        Ok(())
    }

    fn unmount_virtiofs(&self, vm_id: &str, tag: &str) -> Result<(), HypervisorError> {
        let previous = {
            let mut vms = self.vms.lock().unwrap();
            let entry = vms
                .get_mut(vm_id)
                .ok_or(HypervisorError::NotFound(vm_id.into()))?;
            let prev = entry.info.shared_dirs.clone();
            entry.info.shared_dirs.retain(|d| d.tag != tag);
            prev
        };
        if let Err(e) = self.persist() {
            let mut vms = self.vms.lock().unwrap();
            if let Some(entry) = vms.get_mut(vm_id) {
                entry.info.shared_dirs = previous;
            }
            return Err(e);
        }

        // TODO: umount <guest_path> inside VM, detach VZ device

        Ok(())
    }

    fn list_virtiofs_mounts(&self, vm_id: &str) -> Result<Vec<SharedDirectory>, HypervisorError> {
        let vms = self.vms.lock().unwrap();
        let entry = vms
            .get(vm_id)
            .ok_or(HypervisorError::NotFound(vm_id.into()))?;
        Ok(entry.info.shared_dirs.clone())
    }

    fn add_port_forward(&self, vm_id: &str, pf: &PortForward) -> Result<(), HypervisorError> {
        {
            let mut vms = self.vms.lock().unwrap();
            let entry = vms
                .get_mut(vm_id)
                .ok_or(HypervisorError::NotFound(vm_id.into()))?;
            if entry
                .info
                .port_forwards
                .iter()
                .any(|p| p.host_port == pf.host_port)
            {
                return Err(HypervisorError::CreateFailed(format!(
                    "Host port already forwarded: {}",
                    pf.host_port
                )));
            }
            entry.info.port_forwards.push(pf.clone());
        }
        if let Err(e) = self.persist() {
            let mut vms = self.vms.lock().unwrap();
            if let Some(entry) = vms.get_mut(vm_id) {
                entry
                    .info
                    .port_forwards
                    .retain(|p| p.host_port != pf.host_port);
            }
            return Err(e);
        }
        Ok(())
    }

    fn remove_port_forward(&self, vm_id: &str, host_port: u16) -> Result<(), HypervisorError> {
        let previous = {
            let mut vms = self.vms.lock().unwrap();
            let entry = vms
                .get_mut(vm_id)
                .ok_or(HypervisorError::NotFound(vm_id.into()))?;
            let prev = entry.info.port_forwards.clone();
            entry
                .info
                .port_forwards
                .retain(|p| p.host_port != host_port);
            prev
        };
        if let Err(e) = self.persist() {
            let mut vms = self.vms.lock().unwrap();
            if let Some(entry) = vms.get_mut(vm_id) {
                entry.info.port_forwards = previous;
            }
            return Err(e);
        }
        Ok(())
    }

    fn list_port_forwards(&self, vm_id: &str) -> Result<Vec<PortForward>, HypervisorError> {
        let vms = self.vms.lock().unwrap();
        let entry = vms
            .get(vm_id)
            .ok_or(HypervisorError::NotFound(vm_id.into()))?;
        Ok(entry.info.port_forwards.clone())
    }
}
