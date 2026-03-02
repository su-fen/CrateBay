// Windows hypervisor: Hyper-V via Windows Hypervisor Platform (WHP) API.
//
// On Windows, we use the Windows Hypervisor Platform API for near-native VM performance.
// This requires Windows 10 Pro/Enterprise/Education with Hyper-V enabled,
// or Windows 11 with WSL2 integration.
//
// VirtioFS: Windows does not natively support VirtioFS. We use Plan 9 filesystem
// protocol (9P) as a fallback for host-guest file sharing, or virtiofs-windows
// (experimental) via a FUSE-based userspace driver.
//
// Rosetta: Not available on Windows. x86_64 emulation on ARM Windows uses
// Windows' built-in x86 emulation layer.

use crate::hypervisor::{
    Hypervisor, HypervisorError, PortForward, SharedDirectory, VmConfig, VmInfo, VmState,
};
use crate::store::{next_id_for_prefix, VmStore};
use std::collections::HashMap;
use std::sync::Mutex;
use tracing::warn;

/// Windows hypervisor backed by Hyper-V / Windows Hypervisor Platform.
pub struct WindowsHypervisor {
    vms: Mutex<HashMap<String, VmEntry>>,
    next_id: Mutex<u64>,
    store: VmStore,
}

impl Default for WindowsHypervisor {
    fn default() -> Self {
        Self::new()
    }
}

struct VmEntry {
    info: VmInfo,
    /// Plan 9 / VirtioFS share handles.
    _share_handles: HashMap<String, u64>,
}

impl WindowsHypervisor {
    pub fn new() -> Self {
        let store = VmStore::new();
        let mut loaded = match store.load_vms() {
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

        for vm in &mut loaded {
            if vm.state != VmState::Stopped {
                vm.state = VmState::Stopped;
            }
        }

        let mut map: HashMap<String, VmEntry> = HashMap::new();
        for vm in loaded.iter().cloned() {
            map.insert(
                vm.id.clone(),
                VmEntry {
                    info: vm,
                    _share_handles: HashMap::new(),
                },
            );
        }

        let next_id = next_id_for_prefix(&loaded, "hv-");
        Self {
            vms: Mutex::new(map),
            next_id: Mutex::new(next_id),
            store,
        }
    }

    /// Check if Hyper-V / Windows Hypervisor Platform is available.
    pub fn hyperv_available() -> bool {
        // Check for WHvCapabilityCodeHypervisorPresent
        // In real implementation, call WHvGetCapability from WinHvPlatform.dll
        #[cfg(target_os = "windows")]
        {
            // Check if Hyper-V service is running
            std::path::Path::new(r"C:\Windows\System32\vmcompute.exe").exists()
        }
        #[cfg(not(target_os = "windows"))]
        {
            false
        }
    }

    /// Detect Docker socket on Windows.
    /// Docker Desktop on Windows uses named pipe: //./pipe/docker_engine
    pub fn detect_docker_socket() -> Option<String> {
        let candidates = [
            r"//./pipe/docker_engine".to_string(),
            r"//./pipe/dockerDesktopLinuxEngine".to_string(),
        ];

        // On Windows, check if named pipes exist
        for pipe in &candidates {
            #[cfg(target_os = "windows")]
            {
                if std::path::Path::new(pipe).exists() {
                    return Some(pipe.clone());
                }
            }
            #[cfg(not(target_os = "windows"))]
            {
                let _ = pipe;
            }
        }

        // Fallback: check WSL2 Docker socket
        #[cfg(target_os = "windows")]
        {
            let userprofile = std::env::var("USERPROFILE").unwrap_or_default();
            let wsl_sock = format!(r"{}\\.docker\run\docker.sock", userprofile);
            if std::path::Path::new(&wsl_sock).exists() {
                return Some(wsl_sock);
            }
        }

        None
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
}

impl Hypervisor for WindowsHypervisor {
    fn create_vm(&self, config: VmConfig) -> Result<String, HypervisorError> {
        if !Self::hyperv_available() {
            return Err(HypervisorError::CreateFailed(
                "Hyper-V not available. Enable Hyper-V in Windows Features (requires Windows 10 Pro+ or Windows 11).".into(),
            ));
        }

        if config.rosetta {
            return Err(HypervisorError::RosettaUnavailable(
                "Rosetta is only available on macOS Apple Silicon. Windows ARM uses its own x86 emulation.".into(),
            ));
        }

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
        let id = format!("hv-{}", *id_counter);
        *id_counter += 1;

        let info = VmInfo {
            id: id.clone(),
            name: config.name,
            state: VmState::Stopped,
            cpus: config.cpus,
            memory_mb: config.memory_mb,
            disk_gb: config.disk_gb,
            rosetta_enabled: false,
            shared_dirs: config.shared_dirs,
            port_forwards: config.port_forwards,
        };

        let entry = VmEntry {
            info,
            _share_handles: HashMap::new(),
        };

        self.vms.lock().unwrap().insert(id.clone(), entry);
        if let Err(e) = self.persist() {
            self.vms.lock().unwrap().remove(&id);
            return Err(e);
        }

        // TODO: Real implementation using Windows Hypervisor Platform:
        // 1. WHvCreatePartition()
        // 2. WHvSetPartitionProperty() — set processor count, memory
        // 3. WHvSetupPartition()
        // 4. WHvMapGpaRange() — map memory
        // 5. WHvCreateVirtualProcessor() — create vCPUs
        // 6. Load kernel + initrd
        // 7. Set up virtio devices (virtio-net, virtio-blk)
        // 8. For file sharing: use Plan 9 / SMB pass-through
        //    (native VirtioFS not yet supported on Windows host)

        Ok(id)
    }

    fn start_vm(&self, id: &str) -> Result<(), HypervisorError> {
        let previous = {
            let mut vms = self.vms.lock().unwrap();
            let entry = vms
                .get_mut(id)
                .ok_or(HypervisorError::NotFound(id.into()))?;
            let prev = entry.info.state.clone();
            entry.info.state = VmState::Running;
            prev
        };
        if let Err(e) = self.persist() {
            let mut vms = self.vms.lock().unwrap();
            if let Some(entry) = vms.get_mut(id) {
                entry.info.state = previous;
            }
            return Err(e);
        }

        // TODO: Real implementation:
        // 1. WHvRunVirtualProcessor() in separate threads per vCPU
        // 2. Handle VM exits (I/O, MMIO, hypercalls)
        // 3. Set up Plan 9 / SMB shares for file sharing
        // 4. Optional: Start WSL2 integration for Docker compatibility

        Ok(())
    }

    fn stop_vm(&self, id: &str) -> Result<(), HypervisorError> {
        let (previous, previous_handles) = {
            let mut vms = self.vms.lock().unwrap();
            let entry = vms
                .get_mut(id)
                .ok_or(HypervisorError::NotFound(id.into()))?;
            let prev = entry.info.state.clone();
            let handles = entry._share_handles.clone();
            entry.info.state = VmState::Stopped;
            entry._share_handles.clear();
            (prev, handles)
        };
        if let Err(e) = self.persist() {
            let mut vms = self.vms.lock().unwrap();
            if let Some(entry) = vms.get_mut(id) {
                entry.info.state = previous;
                entry._share_handles = previous_handles;
            }
            return Err(e);
        }

        // TODO: WHvCancelRunVirtualProcessor(), clean up

        Ok(())
    }

    fn delete_vm(&self, id: &str) -> Result<(), HypervisorError> {
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

        // TODO: WHvDeletePartition()

        Ok(())
    }

    fn list_vms(&self) -> Result<Vec<VmInfo>, HypervisorError> {
        Ok(self
            .vms
            .lock()
            .unwrap()
            .values()
            .map(|e| e.info.clone())
            .collect())
    }

    fn rosetta_available(&self) -> bool {
        false // Rosetta is macOS-only
    }

    fn mount_virtiofs(&self, vm_id: &str, share: &SharedDirectory) -> Result<(), HypervisorError> {
        if !std::path::Path::new(&share.host_path).exists() {
            return Err(HypervisorError::VirtioFsError(format!(
                "Host path does not exist: {}",
                share.host_path
            )));
        }

        {
            let mut vms = self.vms.lock().unwrap();
            let entry = vms
                .get_mut(vm_id)
                .ok_or(HypervisorError::NotFound(vm_id.into()))?;

            if entry.info.shared_dirs.iter().any(|d| d.tag == share.tag) {
                return Err(HypervisorError::VirtioFsError(format!(
                    "Mount tag already exists: {}",
                    share.tag
                )));
            }

            entry.info.shared_dirs.push(share.clone());
        }
        if let Err(e) = self.persist() {
            let mut vms = self.vms.lock().unwrap();
            if let Some(entry) = vms.get_mut(vm_id) {
                entry.info.shared_dirs.retain(|d| d.tag != share.tag);
            }
            return Err(e);
        }

        // TODO: On Windows, use Plan 9 protocol (9P) or SMB for file sharing.
        // Native VirtioFS is not supported on Windows host yet.
        // Fallback: net use \\<vm-ip>\share or Hyper-V integration services.

        Ok(())
    }

    fn unmount_virtiofs(&self, vm_id: &str, tag: &str) -> Result<(), HypervisorError> {
        let (previous_dirs, previous_handles) = {
            let mut vms = self.vms.lock().unwrap();
            let entry = vms
                .get_mut(vm_id)
                .ok_or(HypervisorError::NotFound(vm_id.into()))?;
            let prev_dirs = entry.info.shared_dirs.clone();
            let prev_handles = entry._share_handles.clone();
            entry.info.shared_dirs.retain(|d| d.tag != tag);
            entry._share_handles.remove(tag);
            (prev_dirs, prev_handles)
        };
        if let Err(e) = self.persist() {
            let mut vms = self.vms.lock().unwrap();
            if let Some(entry) = vms.get_mut(vm_id) {
                entry.info.shared_dirs = previous_dirs;
                entry._share_handles = previous_handles;
            }
            return Err(e);
        }
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
