use cratebay_core::hypervisor::Hypervisor;
use cratebay_core::proto;
use cratebay_core::proto::vm_service_server::VmService;
use cratebay_core::vm::StubHypervisor;
use cratebay_daemon::service::VmServiceImpl;
use std::ffi::OsString;
use std::sync::{Arc, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::Mutex;
use tonic::Request;

static ENV_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

struct EnvVarGuard {
    key: &'static str,
    prev: Option<OsString>,
}

impl EnvVarGuard {
    fn set_path(key: &'static str, value: &std::path::Path) -> Self {
        let prev = std::env::var_os(key);
        std::env::set_var(key, value);
        Self { key, prev }
    }
}

impl Drop for EnvVarGuard {
    fn drop(&mut self) {
        match self.prev.take() {
            Some(v) => std::env::set_var(self.key, v),
            None => std::env::remove_var(self.key),
        }
    }
}

struct TempDirGuard {
    path: std::path::PathBuf,
}

impl TempDirGuard {
    fn new(prefix: &str) -> Self {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let path =
            std::env::temp_dir().join(format!("{}-{}-{}", prefix, std::process::id(), nanos));
        std::fs::create_dir_all(&path).expect("create temp dir");
        Self { path }
    }
}

impl Drop for TempDirGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

#[tokio::test]
async fn grpc_vm_lifecycle_and_mounts() {
    let _env_guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().await;

    let temp = TempDirGuard::new("cratebay-daemon-test");
    let _config_dir = EnvVarGuard::set_path("CRATEBAY_CONFIG_DIR", &temp.path);
    let _data_dir = EnvVarGuard::set_path("CRATEBAY_DATA_DIR", &temp.path);
    let _log_dir = EnvVarGuard::set_path("CRATEBAY_LOG_DIR", &temp.path);

    let hv: Arc<dyn Hypervisor> = Arc::new(StubHypervisor::new());
    let service = VmServiceImpl::new(hv);

    let created = service
        .create_vm(Request::new(proto::CreateVmRequest {
            name: "testvm".into(),
            cpus: 2,
            memory_mb: 256,
            disk_gb: 1,
            rosetta: false,
            shared_dirs: vec![],
        }))
        .await
        .expect("create")
        .into_inner();
    assert!(
        created.vm_id.starts_with("stub-"),
        "unexpected vm id: {}",
        created.vm_id
    );

    service
        .start_vm(Request::new(proto::StartVmRequest {
            vm_id: "testvm".into(),
        }))
        .await
        .expect("start by name");

    let vms = service
        .list_v_ms(Request::new(proto::ListVMsRequest {}))
        .await
        .expect("list")
        .into_inner()
        .vms;
    assert_eq!(vms.len(), 1);
    assert_eq!(vms[0].name, "testvm");
    assert_eq!(vms[0].status, "running");

    service
        .mount_virtio_fs(Request::new(proto::MountVirtioFsRequest {
            vm_id: created.vm_id.clone(),
            share: Some(proto::SharedDirectory {
                tag: "code".into(),
                host_path: "/tmp".into(),
                guest_path: "/mnt/code".into(),
                read_only: false,
            }),
        }))
        .await
        .expect("mount");

    let mounts = service
        .list_virtio_fs_mounts(Request::new(proto::ListVirtioFsMountsRequest {
            vm_id: "testvm".into(),
        }))
        .await
        .expect("list mounts by name")
        .into_inner()
        .mounts;
    assert_eq!(mounts.len(), 1);
    assert_eq!(mounts[0].tag, "code");

    service
        .stop_vm(Request::new(proto::StopVmRequest {
            vm_id: created.vm_id.clone(),
        }))
        .await
        .expect("stop");

    let status = service
        .get_vm_status(Request::new(proto::GetVmStatusRequest {
            vm_id: "testvm".into(),
        }))
        .await
        .expect("status")
        .into_inner();
    assert_eq!(status.status, "stopped");
}
