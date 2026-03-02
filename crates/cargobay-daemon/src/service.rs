use cargobay_core::hypervisor::{
    Hypervisor, HypervisorError, PortForward, SharedDirectory, VmConfig, VmState,
};
use cargobay_core::portfwd::PortForwardManager;
use cargobay_core::proto;
use cargobay_core::proto::vm_service_server::VmService;
use std::sync::Arc;
use tonic::{Request, Response, Status};

#[derive(Clone)]
pub struct VmServiceImpl {
    hv: Arc<dyn Hypervisor>,
    port_fwd: PortForwardManager,
}

impl VmServiceImpl {
    pub fn new(hv: Arc<dyn Hypervisor>) -> Self {
        Self {
            hv,
            port_fwd: PortForwardManager::new(),
        }
    }

    fn status_from_error(op: &'static str, err: HypervisorError) -> Status {
        if matches!(&err, HypervisorError::NotFound(_)) {
            tracing::warn!(op = op, error = %err, "hypervisor operation failed");
        } else {
            tracing::error!(op = op, error = %err, "hypervisor operation failed");
        }

        match err {
            HypervisorError::NotFound(id) => Status::not_found(format!("VM not found: {}", id)),
            HypervisorError::Unsupported => Status::unimplemented("unsupported platform"),
            HypervisorError::RosettaUnavailable(msg) => Status::failed_precondition(msg),
            HypervisorError::VirtioFsError(msg) => Status::failed_precondition(msg),
            HypervisorError::CreateFailed(msg) => Status::failed_precondition(msg),
            HypervisorError::Storage(msg) => Status::internal(format!("storage error: {}", msg)),
            HypervisorError::Io(e) => Status::internal(format!("io error: {}", e)),
        }
    }

    fn vm_state_to_string(state: VmState) -> String {
        match state {
            VmState::Running => "running".into(),
            VmState::Stopped => "stopped".into(),
            VmState::Creating => "creating".into(),
        }
    }

    fn proto_shared_dir(dir: SharedDirectory) -> proto::SharedDirectory {
        proto::SharedDirectory {
            tag: dir.tag,
            host_path: dir.host_path,
            guest_path: dir.guest_path,
            read_only: dir.read_only,
        }
    }

    fn core_shared_dir(dir: proto::SharedDirectory) -> SharedDirectory {
        SharedDirectory {
            tag: dir.tag,
            host_path: dir.host_path,
            guest_path: dir.guest_path,
            read_only: dir.read_only,
        }
    }

    fn proto_port_forward(pf: PortForward) -> proto::PortForwardEntry {
        proto::PortForwardEntry {
            host_port: pf.host_port as u32,
            guest_port: pf.guest_port as u32,
            protocol: pf.protocol,
        }
    }

    #[allow(dead_code)]
    fn core_port_forward(pf: proto::PortForwardEntry) -> PortForward {
        PortForward {
            host_port: pf.host_port as u16,
            guest_port: pf.guest_port as u16,
            protocol: pf.protocol,
        }
    }

    #[allow(clippy::result_large_err)]
    fn resolve_vm_id(&self, selector: &str) -> Result<String, Status> {
        let vms = self
            .hv
            .list_vms()
            .map_err(|e| Self::status_from_error("list_vms", e))?;

        if vms.iter().any(|vm| vm.id == selector) {
            return Ok(selector.to_string());
        }

        if let Some(vm) = vms.into_iter().find(|vm| vm.name == selector) {
            return Ok(vm.id);
        }

        Err(Status::not_found(format!("VM not found: {}", selector)))
    }
}

#[tonic::async_trait]
impl VmService for VmServiceImpl {
    async fn create_vm(
        &self,
        request: Request<proto::CreateVmRequest>,
    ) -> Result<Response<proto::CreateVmResponse>, Status> {
        let req = request.into_inner();
        let shared_dirs = req
            .shared_dirs
            .into_iter()
            .map(Self::core_shared_dir)
            .collect::<Vec<_>>();

        let config = VmConfig {
            name: req.name,
            cpus: req.cpus,
            memory_mb: req.memory_mb,
            disk_gb: req.disk_gb,
            rosetta: req.rosetta,
            shared_dirs,
            os_image: None,
            kernel_path: None,
            initrd_path: None,
            disk_path: None,
            port_forwards: vec![],
        };

        let vm_id = self
            .hv
            .create_vm(config)
            .map_err(|e| Self::status_from_error("create_vm", e))?;
        Ok(Response::new(proto::CreateVmResponse { vm_id }))
    }

    async fn start_vm(
        &self,
        request: Request<proto::StartVmRequest>,
    ) -> Result<Response<proto::StartVmResponse>, Status> {
        let req = request.into_inner();
        let vm_id = self.resolve_vm_id(&req.vm_id)?;
        self.hv
            .start_vm(&vm_id)
            .map_err(|e| Self::status_from_error("start_vm", e))?;
        Ok(Response::new(proto::StartVmResponse {}))
    }

    async fn stop_vm(
        &self,
        request: Request<proto::StopVmRequest>,
    ) -> Result<Response<proto::StopVmResponse>, Status> {
        let req = request.into_inner();
        let vm_id = self.resolve_vm_id(&req.vm_id)?;
        self.hv
            .stop_vm(&vm_id)
            .map_err(|e| Self::status_from_error("stop_vm", e))?;
        Ok(Response::new(proto::StopVmResponse {}))
    }

    async fn delete_vm(
        &self,
        request: Request<proto::DeleteVmRequest>,
    ) -> Result<Response<proto::DeleteVmResponse>, Status> {
        let req = request.into_inner();
        let vm_id = self.resolve_vm_id(&req.vm_id)?;
        self.hv
            .delete_vm(&vm_id)
            .map_err(|e| Self::status_from_error("delete_vm", e))?;
        Ok(Response::new(proto::DeleteVmResponse {}))
    }

    async fn list_v_ms(
        &self,
        _request: Request<proto::ListVMsRequest>,
    ) -> Result<Response<proto::ListVMsResponse>, Status> {
        let vms = self
            .hv
            .list_vms()
            .map_err(|e| Self::status_from_error("list_vms", e))?;
        let out = vms
            .into_iter()
            .map(|vm| proto::VmInfo {
                vm_id: vm.id,
                name: vm.name,
                status: Self::vm_state_to_string(vm.state),
                cpus: vm.cpus,
                memory_mb: vm.memory_mb,
                rosetta_enabled: vm.rosetta_enabled,
                shared_dirs: vm
                    .shared_dirs
                    .into_iter()
                    .map(Self::proto_shared_dir)
                    .collect(),
                disk_gb: vm.disk_gb,
                port_forwards: vm
                    .port_forwards
                    .into_iter()
                    .map(Self::proto_port_forward)
                    .collect(),
            })
            .collect::<Vec<_>>();

        Ok(Response::new(proto::ListVMsResponse { vms: out }))
    }

    async fn get_vm_status(
        &self,
        request: Request<proto::GetVmStatusRequest>,
    ) -> Result<Response<proto::GetVmStatusResponse>, Status> {
        let req = request.into_inner();
        let vms = self
            .hv
            .list_vms()
            .map_err(|e| Self::status_from_error("list_vms", e))?;
        let Some(vm) = vms
            .into_iter()
            .find(|v| v.id == req.vm_id || v.name == req.vm_id)
        else {
            return Err(Status::not_found(format!("VM not found: {}", req.vm_id)));
        };

        Ok(Response::new(proto::GetVmStatusResponse {
            vm_id: vm.id,
            status: Self::vm_state_to_string(vm.state),
            rosetta_enabled: vm.rosetta_enabled,
            shared_dirs: vm
                .shared_dirs
                .into_iter()
                .map(Self::proto_shared_dir)
                .collect(),
            disk_gb: vm.disk_gb,
            port_forwards: vm
                .port_forwards
                .into_iter()
                .map(Self::proto_port_forward)
                .collect(),
        }))
    }

    async fn mount_virtio_fs(
        &self,
        request: Request<proto::MountVirtioFsRequest>,
    ) -> Result<Response<proto::MountVirtioFsResponse>, Status> {
        let req = request.into_inner();
        let vm_id = self.resolve_vm_id(&req.vm_id)?;
        let share = req
            .share
            .ok_or_else(|| Status::invalid_argument("share is required"))?;
        let share = Self::core_shared_dir(share);
        self.hv
            .mount_virtiofs(&vm_id, &share)
            .map_err(|e| Self::status_from_error("mount_virtiofs", e))?;
        Ok(Response::new(proto::MountVirtioFsResponse {}))
    }

    async fn unmount_virtio_fs(
        &self,
        request: Request<proto::UnmountVirtioFsRequest>,
    ) -> Result<Response<proto::UnmountVirtioFsResponse>, Status> {
        let req = request.into_inner();
        let vm_id = self.resolve_vm_id(&req.vm_id)?;
        self.hv
            .unmount_virtiofs(&vm_id, &req.tag)
            .map_err(|e| Self::status_from_error("unmount_virtiofs", e))?;
        Ok(Response::new(proto::UnmountVirtioFsResponse {}))
    }

    async fn list_virtio_fs_mounts(
        &self,
        request: Request<proto::ListVirtioFsMountsRequest>,
    ) -> Result<Response<proto::ListVirtioFsMountsResponse>, Status> {
        let req = request.into_inner();
        let vm_id = self.resolve_vm_id(&req.vm_id)?;
        let mounts = self
            .hv
            .list_virtiofs_mounts(&vm_id)
            .map_err(|e| Self::status_from_error("list_virtiofs_mounts", e))?;
        Ok(Response::new(proto::ListVirtioFsMountsResponse {
            mounts: mounts.into_iter().map(Self::proto_shared_dir).collect(),
        }))
    }

    async fn get_vm_console(
        &self,
        request: Request<proto::GetVmConsoleRequest>,
    ) -> Result<Response<proto::GetVmConsoleResponse>, Status> {
        let req = request.into_inner();
        let vm_id = self.resolve_vm_id(&req.vm_id)?;
        let (data, new_offset) = self
            .hv
            .read_vm_console(&vm_id, req.offset)
            .map_err(|e| Self::status_from_error("read_vm_console", e))?;
        Ok(Response::new(proto::GetVmConsoleResponse {
            data,
            new_offset,
        }))
    }

    async fn add_port_forward(
        &self,
        request: Request<proto::AddPortForwardRequest>,
    ) -> Result<Response<proto::AddPortForwardResponse>, Status> {
        let req = request.into_inner();
        let vm_id = self.resolve_vm_id(&req.vm_id)?;
        let host_port = req.host_port as u16;
        let guest_port = req.guest_port as u16;
        let protocol = if req.protocol.is_empty() {
            "tcp".to_string()
        } else {
            req.protocol.clone()
        };

        let pf = PortForward {
            host_port,
            guest_port,
            protocol: protocol.clone(),
        };

        // Persist the forward in the hypervisor store first.
        self.hv
            .add_port_forward(&vm_id, &pf)
            .map_err(|e| Self::status_from_error("add_port_forward", e))?;

        // Start the TCP proxy listener.
        // Use 127.0.0.1 as the VM guest address for now (real VM IP will come later).
        let guest_addr = "127.0.0.1";
        if let Err(e) = self
            .port_fwd
            .add(&vm_id, host_port, guest_addr, guest_port, &protocol)
            .await
        {
            // Rollback the persisted forward on proxy failure.
            let _ = self.hv.remove_port_forward(&vm_id, host_port);
            return Err(Status::internal(e));
        }

        Ok(Response::new(proto::AddPortForwardResponse {}))
    }

    async fn remove_port_forward(
        &self,
        request: Request<proto::RemovePortForwardRequest>,
    ) -> Result<Response<proto::RemovePortForwardResponse>, Status> {
        let req = request.into_inner();
        let vm_id = self.resolve_vm_id(&req.vm_id)?;
        let host_port = req.host_port as u16;

        // Stop the TCP proxy listener (best-effort -- it may not be running).
        let _ = self.port_fwd.remove(&vm_id, host_port).await;

        // Remove from the persisted store.
        self.hv
            .remove_port_forward(&vm_id, host_port)
            .map_err(|e| Self::status_from_error("remove_port_forward", e))?;

        Ok(Response::new(proto::RemovePortForwardResponse {}))
    }

    async fn list_port_forwards(
        &self,
        request: Request<proto::ListPortForwardsRequest>,
    ) -> Result<Response<proto::ListPortForwardsResponse>, Status> {
        let req = request.into_inner();
        let vm_id = self.resolve_vm_id(&req.vm_id)?;

        let forwards = self
            .hv
            .list_port_forwards(&vm_id)
            .map_err(|e| Self::status_from_error("list_port_forwards", e))?
            .into_iter()
            .map(Self::proto_port_forward)
            .collect();

        Ok(Response::new(proto::ListPortForwardsResponse { forwards }))
    }

    async fn get_vm_stats(
        &self,
        request: Request<proto::GetVmStatsRequest>,
    ) -> Result<Response<proto::GetVmStatsResponse>, Status> {
        let req = request.into_inner();
        let vm_id = self.resolve_vm_id(&req.vm_id)?;

        // Check the VM exists and is running
        let vms = self
            .hv
            .list_vms()
            .map_err(|e| Self::status_from_error("list_vms", e))?;
        let Some(vm) = vms.into_iter().find(|v| v.id == vm_id) else {
            return Err(Status::not_found(format!("VM not found: {}", vm_id)));
        };

        if vm.state != VmState::Running {
            return Ok(Response::new(proto::GetVmStatsResponse {
                vm_id,
                cpu_percent: 0.0,
                memory_usage_mb: 0,
                disk_usage_gb: 0,
            }));
        }

        // For now return stub stats for the VM (real implementation would
        // read from the vz runner process stats).
        Ok(Response::new(proto::GetVmStatsResponse {
            vm_id,
            cpu_percent: 0.0,
            memory_usage_mb: 0,
            disk_usage_gb: vm.disk_gb,
        }))
    }
}
