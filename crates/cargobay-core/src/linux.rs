// Linux hypervisor: KVM via rust-vmm with VirtioFS support.
//
// VirtioFS: Uses virtiofsd (or its Rust equivalent) to provide high-performance
// shared filesystem between host and guest. The virtiofsd daemon runs on the host
// and communicates with the guest kernel's virtiofs driver via VHOST-USER protocol.
//
// Rosetta: Not available on Linux (Apple-only technology). x86_64 containers
// on ARM Linux would use QEMU user-mode emulation instead.

use crate::hypervisor::{
    Hypervisor, HypervisorError, PortForward, SharedDirectory, VmConfig, VmInfo, VmState,
};
use crate::store::{next_id_for_prefix, VmStore};
use std::collections::HashMap;
use std::sync::Mutex;
use tracing::warn;

/// Linux hypervisor backed by KVM (via rust-vmm).
pub struct LinuxHypervisor {
    vms: Mutex<HashMap<String, VmEntry>>,
    next_id: Mutex<u64>,
    store: VmStore,
}

impl Default for LinuxHypervisor {
    fn default() -> Self {
        Self::new()
    }
}

struct VmEntry {
    info: VmInfo,
    /// PIDs of virtiofsd processes for each mount tag.
    _virtiofsd_pids: HashMap<String, u32>,
}

impl LinuxHypervisor {
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
                    _virtiofsd_pids: HashMap::new(),
                },
            );
        }

        let next_id = next_id_for_prefix(&loaded, "kvm-");
        Self {
            vms: Mutex::new(map),
            next_id: Mutex::new(next_id),
            store,
        }
    }

    /// Check if KVM is available on this system.
    pub fn kvm_available() -> bool {
        std::path::Path::new("/dev/kvm").exists()
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

impl Hypervisor for LinuxHypervisor {
    fn create_vm(&self, config: VmConfig) -> Result<String, HypervisorError> {
        if !Self::kvm_available() {
            return Err(HypervisorError::CreateFailed(
                "KVM not available. Ensure /dev/kvm exists and you have permissions.".into(),
            ));
        }

        if config.rosetta {
            return Err(HypervisorError::RosettaUnavailable(
                "Rosetta is only available on macOS Apple Silicon. Use QEMU user-mode for x86_64 emulation on Linux.".into(),
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
        let id = format!("kvm-{}", *id_counter);
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
            _virtiofsd_pids: HashMap::new(),
        };

        self.vms.lock().unwrap().insert(id.clone(), entry);
        if let Err(e) = self.persist() {
            self.vms.lock().unwrap().remove(&id);
            return Err(e);
        }

        // TODO: Real implementation using rust-vmm crates:
        // 1. Open /dev/kvm, create VM fd (KVM_CREATE_VM)
        // 2. Configure memory regions (KVM_SET_USER_MEMORY_REGION)
        // 3. Create vCPUs (KVM_CREATE_VCPU)
        // 4. Load kernel + initrd into memory
        // 5. Set up virtio-net, virtio-blk devices
        // 6. For each shared_dir:
        //    - Spawn virtiofsd: virtiofsd --socket-path=/tmp/<tag>.sock --shared-dir=<host_path>
        //    - Configure vhost-user-fs device connected to the socket
        // 7. Set up boot parameters

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
        // 1. Run vCPU loop (KVM_RUN) in separate threads
        // 2. Start virtiofsd processes for VirtioFS mounts
        // 3. Handle VM exits and I/O

        Ok(())
    }

    fn stop_vm(&self, id: &str) -> Result<(), HypervisorError> {
        let previous = {
            let mut vms = self.vms.lock().unwrap();
            let entry = vms
                .get_mut(id)
                .ok_or(HypervisorError::NotFound(id.into()))?;
            let prev = entry.info.state.clone();
            entry.info.state = VmState::Stopped;
            prev
        };
        if let Err(e) = self.persist() {
            let mut vms = self.vms.lock().unwrap();
            if let Some(entry) = vms.get_mut(id) {
                entry.info.state = previous;
            }
            return Err(e);
        }

        // TODO: Stop virtiofsd processes, clean up vCPU threads

        let mut vms = self.vms.lock().unwrap();
        if let Some(entry) = vms.get_mut(id) {
            entry._virtiofsd_pids.clear();
        }
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

        // TODO: Real implementation:
        // 1. Spawn virtiofsd --socket-path=/tmp/<tag>.sock --shared-dir=<host_path>
        //    [--sandbox=none if read_only is false]
        // 2. Connect vhost-user-fs device to the socket
        // 3. Inside VM: mount -t virtiofs <tag> <guest_path>
        // 4. Store virtiofsd PID for cleanup

        Ok(())
    }

    fn unmount_virtiofs(&self, vm_id: &str, tag: &str) -> Result<(), HypervisorError> {
        let (previous_dirs, previous_pids) = {
            let mut vms = self.vms.lock().unwrap();
            let entry = vms
                .get_mut(vm_id)
                .ok_or(HypervisorError::NotFound(vm_id.into()))?;
            let prev_dirs = entry.info.shared_dirs.clone();
            let prev_pids = entry._virtiofsd_pids.clone();
            entry.info.shared_dirs.retain(|d| d.tag != tag);

            // TODO: Kill virtiofsd process, umount inside VM

            entry._virtiofsd_pids.remove(tag);
            (prev_dirs, prev_pids)
        };
        if let Err(e) = self.persist() {
            let mut vms = self.vms.lock().unwrap();
            if let Some(entry) = vms.get_mut(vm_id) {
                entry.info.shared_dirs = previous_dirs;
                entry._virtiofsd_pids = previous_pids;
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
