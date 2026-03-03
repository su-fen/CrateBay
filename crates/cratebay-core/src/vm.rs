use crate::hypervisor::{
    Hypervisor, HypervisorError, PortForward, SharedDirectory, VmConfig, VmInfo, VmState,
};
use crate::store::{next_id_for_prefix, VmStore};
use std::collections::HashMap;
use std::sync::Mutex;
use tracing::warn;

/// Stub hypervisor for development/testing on unsupported platforms.
pub struct StubHypervisor {
    vms: Mutex<HashMap<String, VmInfo>>,
    next_id: Mutex<u64>,
    store: VmStore,
}

impl Default for StubHypervisor {
    fn default() -> Self {
        Self::new()
    }
}

impl StubHypervisor {
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

        let mut map: HashMap<String, VmInfo> = HashMap::new();
        for vm in loaded.iter().cloned() {
            map.insert(vm.id.clone(), vm);
        }

        let next_id = next_id_for_prefix(&loaded, "stub-");
        Self {
            vms: Mutex::new(map),
            next_id: Mutex::new(next_id),
            store,
        }
    }

    fn persist(&self) -> Result<(), HypervisorError> {
        let vms = self
            .vms
            .lock()
            .unwrap()
            .values()
            .cloned()
            .collect::<Vec<_>>();
        self.store.save_vms(&vms)
    }
}

impl Hypervisor for StubHypervisor {
    fn create_vm(&self, config: VmConfig) -> Result<String, HypervisorError> {
        {
            let vms = crate::lock_or_recover(&self.vms);
            if vms.values().any(|vm| vm.name == config.name) {
                return Err(HypervisorError::CreateFailed(format!(
                    "VM name already exists: {}",
                    config.name
                )));
            }
        }

        let mut id_counter = crate::lock_or_recover(&self.next_id);
        let id = format!("stub-{}", *id_counter);
        *id_counter += 1;

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
            os_image: config.os_image,
        };
        crate::lock_or_recover(&self.vms).insert(id.clone(), info);
        if let Err(e) = self.persist() {
            crate::lock_or_recover(&self.vms).remove(&id);
            return Err(e);
        }
        Ok(id)
    }

    fn start_vm(&self, id: &str) -> Result<(), HypervisorError> {
        let previous = {
            let mut vms = crate::lock_or_recover(&self.vms);
            let vm = vms
                .get_mut(id)
                .ok_or(HypervisorError::NotFound(id.into()))?;
            let prev = vm.state.clone();
            vm.state = VmState::Running;
            prev
        };
        if let Err(e) = self.persist() {
            let mut vms = crate::lock_or_recover(&self.vms);
            if let Some(vm) = vms.get_mut(id) {
                vm.state = previous;
            }
            return Err(e);
        }
        Ok(())
    }

    fn stop_vm(&self, id: &str) -> Result<(), HypervisorError> {
        let previous = {
            let mut vms = crate::lock_or_recover(&self.vms);
            let vm = vms
                .get_mut(id)
                .ok_or(HypervisorError::NotFound(id.into()))?;
            let prev = vm.state.clone();
            vm.state = VmState::Stopped;
            prev
        };
        if let Err(e) = self.persist() {
            let mut vms = crate::lock_or_recover(&self.vms);
            if let Some(vm) = vms.get_mut(id) {
                vm.state = previous;
            }
            return Err(e);
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
            crate::lock_or_recover(&self.vms).insert(id.to_string(), removed);
            return Err(e);
        }
        Ok(())
    }

    fn list_vms(&self) -> Result<Vec<VmInfo>, HypervisorError> {
        Ok(crate::lock_or_recover(&self.vms)
            .values()
            .cloned()
            .collect())
    }

    fn mount_virtiofs(&self, vm_id: &str, share: &SharedDirectory) -> Result<(), HypervisorError> {
        {
            let mut vms = crate::lock_or_recover(&self.vms);
            let vm = vms
                .get_mut(vm_id)
                .ok_or(HypervisorError::NotFound(vm_id.into()))?;
            if vm.shared_dirs.iter().any(|d| d.tag == share.tag) {
                return Err(HypervisorError::VirtioFsError(format!(
                    "Mount tag already exists: {}",
                    share.tag
                )));
            }
            vm.shared_dirs.push(share.clone());
        }
        if let Err(e) = self.persist() {
            let mut vms = crate::lock_or_recover(&self.vms);
            if let Some(vm) = vms.get_mut(vm_id) {
                vm.shared_dirs.retain(|d| d.tag != share.tag);
            }
            return Err(e);
        }
        Ok(())
    }

    fn unmount_virtiofs(&self, vm_id: &str, tag: &str) -> Result<(), HypervisorError> {
        let previous = {
            let mut vms = crate::lock_or_recover(&self.vms);
            let vm = vms
                .get_mut(vm_id)
                .ok_or(HypervisorError::NotFound(vm_id.into()))?;
            let prev = vm.shared_dirs.clone();
            vm.shared_dirs.retain(|d| d.tag != tag);
            prev
        };
        if let Err(e) = self.persist() {
            let mut vms = crate::lock_or_recover(&self.vms);
            if let Some(vm) = vms.get_mut(vm_id) {
                vm.shared_dirs = previous;
            }
            return Err(e);
        }
        Ok(())
    }

    fn list_virtiofs_mounts(&self, vm_id: &str) -> Result<Vec<SharedDirectory>, HypervisorError> {
        let vms = crate::lock_or_recover(&self.vms);
        let vm = vms
            .get(vm_id)
            .ok_or(HypervisorError::NotFound(vm_id.into()))?;
        Ok(vm.shared_dirs.clone())
    }

    fn add_port_forward(&self, vm_id: &str, pf: &PortForward) -> Result<(), HypervisorError> {
        {
            let mut vms = crate::lock_or_recover(&self.vms);
            let vm = vms
                .get_mut(vm_id)
                .ok_or(HypervisorError::NotFound(vm_id.into()))?;
            if vm.port_forwards.iter().any(|p| p.host_port == pf.host_port) {
                return Err(HypervisorError::CreateFailed(format!(
                    "Host port already forwarded: {}",
                    pf.host_port
                )));
            }
            vm.port_forwards.push(pf.clone());
        }
        if let Err(e) = self.persist() {
            let mut vms = crate::lock_or_recover(&self.vms);
            if let Some(vm) = vms.get_mut(vm_id) {
                vm.port_forwards.retain(|p| p.host_port != pf.host_port);
            }
            return Err(e);
        }
        Ok(())
    }

    fn remove_port_forward(&self, vm_id: &str, host_port: u16) -> Result<(), HypervisorError> {
        let previous = {
            let mut vms = crate::lock_or_recover(&self.vms);
            let vm = vms
                .get_mut(vm_id)
                .ok_or(HypervisorError::NotFound(vm_id.into()))?;
            let prev = vm.port_forwards.clone();
            vm.port_forwards.retain(|p| p.host_port != host_port);
            prev
        };
        if let Err(e) = self.persist() {
            let mut vms = crate::lock_or_recover(&self.vms);
            if let Some(vm) = vms.get_mut(vm_id) {
                vm.port_forwards = previous;
            }
            return Err(e);
        }
        Ok(())
    }

    fn list_port_forwards(&self, vm_id: &str) -> Result<Vec<PortForward>, HypervisorError> {
        let vms = crate::lock_or_recover(&self.vms);
        let vm = vms
            .get(vm_id)
            .ok_or(HypervisorError::NotFound(vm_id.into()))?;
        Ok(vm.port_forwards.clone())
    }
}
