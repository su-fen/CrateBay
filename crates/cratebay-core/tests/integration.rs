//! Integration tests for cratebay-core.
//!
//! These tests exercise public APIs across modules without requiring
//! Docker, a running daemon, or network access.

use std::ffi::OsString;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

// ---------------------------------------------------------------------------
// Test utilities
// ---------------------------------------------------------------------------

/// Global lock to serialize tests that manipulate environment variables.
/// Since env vars are process-wide, parallel tests can stomp on each other.
static ENV_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

fn env_lock() -> std::sync::MutexGuard<'static, ()> {
    ENV_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .expect("env lock")
}

struct EnvGuard {
    key: String,
    prev: Option<OsString>,
}

impl EnvGuard {
    fn set(key: &str, value: &str) -> Self {
        let prev = std::env::var_os(key);
        std::env::set_var(key, value);
        Self {
            key: key.to_string(),
            prev,
        }
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        match self.prev.take() {
            Some(v) => std::env::set_var(&self.key, v),
            None => std::env::remove_var(&self.key),
        }
    }
}

struct TempDir {
    path: PathBuf,
}

impl TempDir {
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

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

// ---------------------------------------------------------------------------
// Cross-crate integration: core public API
// ---------------------------------------------------------------------------

#[test]
fn core_config_and_data_dirs_are_consistent() {
    let _lock = env_lock();
    let tmp = TempDir::new("cratebay-int-test");
    let _g1 = EnvGuard::set(
        "CRATEBAY_CONFIG_DIR",
        tmp.path.join("config").to_str().unwrap(),
    );
    let _g2 = EnvGuard::set("CRATEBAY_DATA_DIR", tmp.path.join("data").to_str().unwrap());
    let _g3 = EnvGuard::set("CRATEBAY_LOG_DIR", tmp.path.join("logs").to_str().unwrap());

    let config = cratebay_core::config_dir();
    let data = cratebay_core::data_dir();
    let log = cratebay_core::log_dir();

    assert_eq!(config, tmp.path.join("config"));
    assert_eq!(data, tmp.path.join("data"));
    assert_eq!(log, tmp.path.join("logs"));

    // Verify they are distinct
    assert_ne!(config, data);
    assert_ne!(data, log);
}

#[test]
fn core_platform_info_is_not_empty() {
    let info = cratebay_core::platform_info();
    assert!(
        !info.is_empty(),
        "platform_info should return a non-empty string"
    );
}

// ---------------------------------------------------------------------------
// Cross-crate integration: hypervisor + store (via StubHypervisor)
// ---------------------------------------------------------------------------

#[test]
fn stub_hypervisor_create_and_list() {
    use cratebay_core::hypervisor::{Hypervisor, VmConfig, VmState};
    use cratebay_core::vm::StubHypervisor;

    let _lock = env_lock();
    let tmp = TempDir::new("cratebay-stub-hv");
    let _g1 = EnvGuard::set("CRATEBAY_CONFIG_DIR", tmp.path.to_str().unwrap());
    let _g2 = EnvGuard::set("CRATEBAY_DATA_DIR", tmp.path.to_str().unwrap());

    let hv = StubHypervisor::new();
    let config = VmConfig {
        name: "integration-test-vm".into(),
        cpus: 1,
        memory_mb: 512,
        disk_gb: 10,
        ..VmConfig::default()
    };

    let id = hv.create_vm(config).expect("create vm");
    assert!(
        id.starts_with("stub-"),
        "id should have stub- prefix: {}",
        id
    );

    let vms = hv.list_vms().expect("list vms");
    assert_eq!(vms.len(), 1);
    assert_eq!(vms[0].name, "integration-test-vm");
    assert_eq!(vms[0].state, VmState::Stopped);
}

#[test]
fn stub_hypervisor_lifecycle() {
    use cratebay_core::hypervisor::{Hypervisor, VmConfig, VmState};
    use cratebay_core::vm::StubHypervisor;

    let _lock = env_lock();
    let tmp = TempDir::new("cratebay-lifecycle");
    let _g1 = EnvGuard::set("CRATEBAY_CONFIG_DIR", tmp.path.to_str().unwrap());
    let _g2 = EnvGuard::set("CRATEBAY_DATA_DIR", tmp.path.to_str().unwrap());

    let hv = StubHypervisor::new();
    let id = hv
        .create_vm(VmConfig {
            name: "lifecycle-vm".into(),
            ..VmConfig::default()
        })
        .expect("create");

    // Start
    hv.start_vm(&id).expect("start");
    let vms = hv.list_vms().unwrap();
    assert_eq!(vms[0].state, VmState::Running);

    // Stop
    hv.stop_vm(&id).expect("stop");
    let vms = hv.list_vms().unwrap();
    assert_eq!(vms[0].state, VmState::Stopped);

    // Delete
    hv.delete_vm(&id).expect("delete");
    let vms = hv.list_vms().unwrap();
    assert!(vms.is_empty());
}

#[test]
fn stub_hypervisor_duplicate_name_fails() {
    use cratebay_core::hypervisor::{Hypervisor, VmConfig};
    use cratebay_core::vm::StubHypervisor;

    let _lock = env_lock();
    let tmp = TempDir::new("cratebay-dup-name");
    let _g1 = EnvGuard::set("CRATEBAY_CONFIG_DIR", tmp.path.to_str().unwrap());
    let _g2 = EnvGuard::set("CRATEBAY_DATA_DIR", tmp.path.to_str().unwrap());

    let hv = StubHypervisor::new();
    hv.create_vm(VmConfig {
        name: "dupvm".into(),
        ..VmConfig::default()
    })
    .expect("first create");

    let result = hv.create_vm(VmConfig {
        name: "dupvm".into(),
        ..VmConfig::default()
    });
    assert!(result.is_err(), "duplicate name should fail");
}

#[test]
fn stub_hypervisor_virtiofs_mount_lifecycle() {
    use cratebay_core::hypervisor::{Hypervisor, SharedDirectory, VmConfig};
    use cratebay_core::vm::StubHypervisor;

    let _lock = env_lock();
    let tmp = TempDir::new("cratebay-virtiofs");
    let _g1 = EnvGuard::set("CRATEBAY_CONFIG_DIR", tmp.path.to_str().unwrap());
    let _g2 = EnvGuard::set("CRATEBAY_DATA_DIR", tmp.path.to_str().unwrap());

    let hv = StubHypervisor::new();
    let id = hv
        .create_vm(VmConfig {
            name: "mount-vm".into(),
            ..VmConfig::default()
        })
        .expect("create");

    let share = SharedDirectory {
        tag: "code".into(),
        host_path: "/tmp/code".into(),
        guest_path: "/mnt/code".into(),
        read_only: false,
    };
    hv.mount_virtiofs(&id, &share).expect("mount");

    let mounts = hv.list_virtiofs_mounts(&id).expect("list mounts");
    assert_eq!(mounts.len(), 1);
    assert_eq!(mounts[0].tag, "code");

    // Duplicate tag should fail
    let dup_result = hv.mount_virtiofs(&id, &share);
    assert!(dup_result.is_err(), "duplicate mount tag should fail");

    // Unmount
    hv.unmount_virtiofs(&id, "code").expect("unmount");
    let mounts = hv.list_virtiofs_mounts(&id).expect("list after unmount");
    assert!(mounts.is_empty());
}

#[test]
fn stub_hypervisor_port_forward_lifecycle() {
    use cratebay_core::hypervisor::{Hypervisor, PortForward, VmConfig};
    use cratebay_core::vm::StubHypervisor;

    let _lock = env_lock();
    let tmp = TempDir::new("cratebay-portfwd");
    let _g1 = EnvGuard::set("CRATEBAY_CONFIG_DIR", tmp.path.to_str().unwrap());
    let _g2 = EnvGuard::set("CRATEBAY_DATA_DIR", tmp.path.to_str().unwrap());

    let hv = StubHypervisor::new();
    let id = hv
        .create_vm(VmConfig {
            name: "pf-vm".into(),
            ..VmConfig::default()
        })
        .expect("create");

    let pf = PortForward {
        host_port: 8080,
        guest_port: 80,
        protocol: "tcp".into(),
    };
    hv.add_port_forward(&id, &pf).expect("add port forward");

    let forwards = hv.list_port_forwards(&id).expect("list");
    assert_eq!(forwards.len(), 1);
    assert_eq!(forwards[0].host_port, 8080);

    // Duplicate host port should fail
    let dup_result = hv.add_port_forward(&id, &pf);
    assert!(dup_result.is_err(), "duplicate host port should fail");

    // Remove
    hv.remove_port_forward(&id, 8080).expect("remove");
    let forwards = hv.list_port_forwards(&id).expect("list after remove");
    assert!(forwards.is_empty());
}

// ---------------------------------------------------------------------------
// Cross-crate integration: images catalog
// ---------------------------------------------------------------------------

#[test]
fn image_catalog_and_find_work_together() {
    let catalog = cratebay_core::images::builtin_catalog();
    for entry in &catalog {
        let found = cratebay_core::images::find_image(&entry.id);
        assert!(
            found.is_some(),
            "find_image should find catalog entry: {}",
            entry.id
        );
    }
}

#[test]
fn image_paths_are_under_data_dir() {
    let _lock = env_lock();
    let tmp = TempDir::new("cratebay-img-paths");
    let _g = EnvGuard::set("CRATEBAY_DATA_DIR", tmp.path.to_str().unwrap());

    let paths = cratebay_core::images::image_paths("alpine-3.19");
    let data_dir = cratebay_core::data_dir();

    assert!(
        paths.kernel_path.starts_with(&data_dir),
        "kernel path should be under data dir"
    );
    assert!(
        paths.initrd_path.starts_with(&data_dir),
        "initrd path should be under data dir"
    );
    assert!(
        paths.rootfs_path.starts_with(&data_dir),
        "rootfs path should be under data dir"
    );
}

// ---------------------------------------------------------------------------
// Cross-crate integration: K3s paths depend on store
// ---------------------------------------------------------------------------

#[test]
fn k3s_kubeconfig_path_under_data_dir() {
    let _lock = env_lock();
    let tmp = TempDir::new("cratebay-k3s-int");
    let _g1 = EnvGuard::set("CRATEBAY_DATA_DIR", tmp.path.to_str().unwrap());
    let _g2 = EnvGuard::set("CRATEBAY_CONFIG_DIR", tmp.path.to_str().unwrap());

    let kc = cratebay_core::k3s::K3sManager::kubeconfig_path();
    let data = cratebay_core::data_dir();
    assert!(
        kc.starts_with(&data),
        "kubeconfig {:?} should be under data dir {:?}",
        kc,
        data
    );
}

// ---------------------------------------------------------------------------
// Cross-crate integration: VM console log path
// ---------------------------------------------------------------------------

#[test]
fn vm_console_log_path_is_under_data_dir() {
    let _lock = env_lock();
    let tmp = TempDir::new("cratebay-console");
    let _g = EnvGuard::set("CRATEBAY_DATA_DIR", tmp.path.to_str().unwrap());

    let path = cratebay_core::vm_console_log_path("test-vm-42");
    let data = cratebay_core::data_dir();
    assert!(
        path.starts_with(&data),
        "console log path {:?} should be under data dir {:?}",
        path,
        data
    );
    assert!(
        path.to_string_lossy().contains("test-vm-42"),
        "path should contain the VM id"
    );
}

// ---------------------------------------------------------------------------
// Store persistence across StubHypervisor instances
// ---------------------------------------------------------------------------

#[test]
fn stub_hypervisor_persists_vms_across_instances() {
    use cratebay_core::hypervisor::{Hypervisor, VmConfig};
    use cratebay_core::vm::StubHypervisor;

    let _lock = env_lock();
    let tmp = TempDir::new("cratebay-persist");
    let _g1 = EnvGuard::set("CRATEBAY_CONFIG_DIR", tmp.path.to_str().unwrap());
    let _g2 = EnvGuard::set("CRATEBAY_DATA_DIR", tmp.path.to_str().unwrap());

    // Create a VM with the first instance
    let vm_id = {
        let hv = StubHypervisor::new();
        hv.create_vm(VmConfig {
            name: "persistent-vm".into(),
            cpus: 4,
            memory_mb: 4096,
            ..VmConfig::default()
        })
        .expect("create")
    };

    // Load with a new instance -- VM should still be there
    {
        let hv = StubHypervisor::new();
        let vms = hv.list_vms().expect("list");
        assert_eq!(vms.len(), 1, "VM should persist across instances");
        let found = vms.iter().find(|v| v.id == vm_id);
        assert!(
            found.is_some(),
            "VM {} should persist across instances",
            vm_id
        );
        let found = found.unwrap();
        assert_eq!(found.name, "persistent-vm");
        assert_eq!(found.cpus, 4);
    }
}
