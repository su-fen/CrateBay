use serde::{Deserialize, Serialize};
use std::io::{Read, Seek, SeekFrom};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum HypervisorError {
    #[error("VM creation failed: {0}")]
    CreateFailed(String),
    #[error("VM not found: {0}")]
    NotFound(String),
    #[error("unsupported platform")]
    Unsupported,
    #[error("Rosetta not available: {0}")]
    RosettaUnavailable(String),
    #[error("VirtioFS error: {0}")]
    VirtioFsError(String),
    #[error("storage error: {0}")]
    Storage(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

/// Unified hypervisor interface across platforms.
pub trait Hypervisor: Send + Sync {
    fn create_vm(&self, config: VmConfig) -> Result<String, HypervisorError>;
    fn start_vm(&self, id: &str) -> Result<(), HypervisorError>;
    fn stop_vm(&self, id: &str) -> Result<(), HypervisorError>;
    fn delete_vm(&self, id: &str) -> Result<(), HypervisorError>;
    fn list_vms(&self) -> Result<Vec<VmInfo>, HypervisorError>;

    /// Check if Rosetta x86_64 translation is available on this platform.
    fn rosetta_available(&self) -> bool {
        false
    }

    /// Mount a host directory into the VM via VirtioFS.
    fn mount_virtiofs(
        &self,
        _vm_id: &str,
        _share: &SharedDirectory,
    ) -> Result<(), HypervisorError> {
        Err(HypervisorError::VirtioFsError(
            "VirtioFS not supported on this platform".into(),
        ))
    }

    /// Unmount a VirtioFS share from the VM.
    fn unmount_virtiofs(&self, _vm_id: &str, _tag: &str) -> Result<(), HypervisorError> {
        Err(HypervisorError::VirtioFsError(
            "VirtioFS not supported on this platform".into(),
        ))
    }

    /// List active VirtioFS mounts for a VM.
    fn list_virtiofs_mounts(&self, _vm_id: &str) -> Result<Vec<SharedDirectory>, HypervisorError> {
        Ok(vec![])
    }

    /// Read console output for a VM starting from the given byte offset.
    /// Returns the data read and the new offset (for incremental reads).
    fn read_vm_console(&self, vm_id: &str, offset: u64) -> Result<(String, u64), HypervisorError> {
        let path = crate::store::vm_console_log_path(vm_id);
        if !path.exists() {
            return Ok((String::new(), 0));
        }
        let mut file = std::fs::File::open(&path)?;
        let metadata = file.metadata()?;
        let file_len = metadata.len();
        if offset >= file_len {
            return Ok((String::new(), file_len));
        }
        file.seek(SeekFrom::Start(offset))?;
        let mut buf = Vec::new();
        file.read_to_end(&mut buf)?;
        let data = String::from_utf8_lossy(&buf).into_owned();
        let new_offset = offset + buf.len() as u64;
        Ok((data, new_offset))
    }

    /// Add a port forward rule to a VM (persisted in the VM store).
    fn add_port_forward(&self, _vm_id: &str, _pf: &PortForward) -> Result<(), HypervisorError> {
        Err(HypervisorError::Unsupported)
    }

    /// Remove a port forward rule from a VM.
    fn remove_port_forward(&self, _vm_id: &str, _host_port: u16) -> Result<(), HypervisorError> {
        Err(HypervisorError::Unsupported)
    }

    /// List port forwards for a VM.
    fn list_port_forwards(&self, _vm_id: &str) -> Result<Vec<PortForward>, HypervisorError> {
        Ok(vec![])
    }
}

/// A single host-port to guest-port forwarding rule.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortForward {
    pub host_port: u16,
    pub guest_port: u16,
    /// "tcp" or "udp"
    pub protocol: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VmConfig {
    pub name: String,
    pub cpus: u32,
    pub memory_mb: u64,
    pub disk_gb: u64,
    /// Enable Rosetta x86_64 translation (macOS Apple Silicon only).
    pub rosetta: bool,
    /// Directories to share via VirtioFS.
    pub shared_dirs: Vec<SharedDirectory>,
    /// OS image id from the image catalog (e.g. "alpine-3.19").
    #[serde(default)]
    pub os_image: Option<String>,
    /// Explicit path to the kernel file (set automatically from os_image).
    #[serde(default)]
    pub kernel_path: Option<String>,
    /// Explicit path to the initrd file (set automatically from os_image).
    #[serde(default)]
    pub initrd_path: Option<String>,
    /// Explicit path to the disk/rootfs file (set automatically from os_image).
    #[serde(default)]
    pub disk_path: Option<String>,
    /// Port forwards from host to guest.
    #[serde(default)]
    pub port_forwards: Vec<PortForward>,
}

impl Default for VmConfig {
    fn default() -> Self {
        Self {
            name: "default".into(),
            cpus: 2,
            memory_mb: 2048,
            disk_gb: 20,
            rosetta: false,
            shared_dirs: vec![],
            os_image: None,
            kernel_path: None,
            initrd_path: None,
            disk_path: None,
            port_forwards: vec![],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SharedDirectory {
    /// Tag used to identify the mount inside the VM.
    pub tag: String,
    /// Host path to share.
    pub host_path: String,
    /// Guest mount point (e.g., /mnt/host).
    pub guest_path: String,
    /// Read-only mount.
    pub read_only: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VmInfo {
    pub id: String,
    pub name: String,
    pub state: VmState,
    pub cpus: u32,
    pub memory_mb: u64,
    #[serde(default = "default_disk_gb")]
    pub disk_gb: u64,
    /// Whether Rosetta x86_64 translation is enabled.
    pub rosetta_enabled: bool,
    /// Active VirtioFS mounts.
    pub shared_dirs: Vec<SharedDirectory>,
    /// Active port forwards.
    #[serde(default)]
    pub port_forwards: Vec<PortForward>,
    /// OS image id from the catalog (e.g. "alpine-3.19"), persisted for restart.
    #[serde(default)]
    pub os_image: Option<String>,
}

fn default_disk_gb() -> u64 {
    20
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum VmState {
    Running,
    Stopped,
    Creating,
}

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // VmState tests
    // -----------------------------------------------------------------------

    #[test]
    fn vm_state_equality() {
        assert_eq!(VmState::Running, VmState::Running);
        assert_eq!(VmState::Stopped, VmState::Stopped);
        assert_eq!(VmState::Creating, VmState::Creating);
        assert_ne!(VmState::Running, VmState::Stopped);
        assert_ne!(VmState::Running, VmState::Creating);
        assert_ne!(VmState::Stopped, VmState::Creating);
    }

    #[test]
    fn vm_state_clone() {
        let state = VmState::Running;
        let cloned = state.clone();
        assert_eq!(state, cloned);
    }

    #[test]
    fn vm_state_debug_format() {
        assert_eq!(format!("{:?}", VmState::Running), "Running");
        assert_eq!(format!("{:?}", VmState::Stopped), "Stopped");
        assert_eq!(format!("{:?}", VmState::Creating), "Creating");
    }

    #[test]
    fn vm_state_serde_round_trip() {
        for state in [VmState::Running, VmState::Stopped, VmState::Creating] {
            let json = serde_json::to_string(&state).unwrap();
            let deserialized: VmState = serde_json::from_str(&json).unwrap();
            assert_eq!(deserialized, state);
        }
    }

    // -----------------------------------------------------------------------
    // VmConfig tests
    // -----------------------------------------------------------------------

    #[test]
    fn vm_config_default_values() {
        let cfg = VmConfig::default();
        assert_eq!(cfg.name, "default");
        assert_eq!(cfg.cpus, 2);
        assert_eq!(cfg.memory_mb, 2048);
        assert_eq!(cfg.disk_gb, 20);
        assert!(!cfg.rosetta);
        assert!(cfg.shared_dirs.is_empty());
        assert!(cfg.os_image.is_none());
        assert!(cfg.kernel_path.is_none());
        assert!(cfg.initrd_path.is_none());
        assert!(cfg.disk_path.is_none());
        assert!(cfg.port_forwards.is_empty());
    }

    #[test]
    fn vm_config_custom_construction() {
        let cfg = VmConfig {
            name: "my-vm".into(),
            cpus: 8,
            memory_mb: 16384,
            disk_gb: 100,
            rosetta: true,
            shared_dirs: vec![SharedDirectory {
                tag: "code".into(),
                host_path: "/Users/test/code".into(),
                guest_path: "/mnt/code".into(),
                read_only: true,
            }],
            os_image: Some("alpine-3.19".into()),
            kernel_path: Some("/path/to/kernel".into()),
            initrd_path: Some("/path/to/initrd".into()),
            disk_path: Some("/path/to/disk".into()),
            port_forwards: vec![PortForward {
                host_port: 8080,
                guest_port: 80,
                protocol: "tcp".into(),
            }],
        };
        assert_eq!(cfg.name, "my-vm");
        assert_eq!(cfg.cpus, 8);
        assert_eq!(cfg.memory_mb, 16384);
        assert!(cfg.rosetta);
        assert_eq!(cfg.shared_dirs.len(), 1);
        assert_eq!(cfg.shared_dirs[0].tag, "code");
        assert!(cfg.shared_dirs[0].read_only);
        assert_eq!(cfg.os_image.as_deref(), Some("alpine-3.19"));
        assert_eq!(cfg.port_forwards.len(), 1);
        assert_eq!(cfg.port_forwards[0].host_port, 8080);
    }

    #[test]
    fn vm_config_serde_round_trip() {
        let cfg = VmConfig {
            name: "test".into(),
            cpus: 4,
            memory_mb: 8192,
            disk_gb: 50,
            rosetta: false,
            shared_dirs: vec![],
            os_image: None,
            kernel_path: None,
            initrd_path: None,
            disk_path: None,
            port_forwards: vec![],
        };
        let json = serde_json::to_string(&cfg).unwrap();
        let deserialized: VmConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.name, "test");
        assert_eq!(deserialized.cpus, 4);
        assert_eq!(deserialized.memory_mb, 8192);
    }

    // -----------------------------------------------------------------------
    // VmInfo tests
    // -----------------------------------------------------------------------

    #[test]
    fn vm_info_serde_round_trip() {
        let info = VmInfo {
            id: "stub-1".into(),
            name: "my-vm".into(),
            state: VmState::Running,
            cpus: 2,
            memory_mb: 2048,
            disk_gb: 20,
            rosetta_enabled: false,
            shared_dirs: vec![],
            port_forwards: vec![],
            os_image: Some("alpine-3.19".into()),
        };
        let json = serde_json::to_string(&info).unwrap();
        let deserialized: VmInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.id, "stub-1");
        assert_eq!(deserialized.name, "my-vm");
        assert_eq!(deserialized.state, VmState::Running);
        assert_eq!(deserialized.os_image.as_deref(), Some("alpine-3.19"));
    }

    #[test]
    fn vm_info_default_disk_gb() {
        // Test that missing disk_gb in JSON defaults to 20.
        let json = r#"{"id":"t","name":"t","state":"Stopped","cpus":1,"memory_mb":512,"rosetta_enabled":false,"shared_dirs":[]}"#;
        let info: VmInfo = serde_json::from_str(json).unwrap();
        assert_eq!(info.disk_gb, 20, "missing disk_gb should default to 20");
    }

    // -----------------------------------------------------------------------
    // SharedDirectory tests
    // -----------------------------------------------------------------------

    #[test]
    fn shared_directory_construction() {
        let dir = SharedDirectory {
            tag: "data".into(),
            host_path: "/host/data".into(),
            guest_path: "/mnt/data".into(),
            read_only: false,
        };
        assert_eq!(dir.tag, "data");
        assert_eq!(dir.host_path, "/host/data");
        assert_eq!(dir.guest_path, "/mnt/data");
        assert!(!dir.read_only);
    }

    // -----------------------------------------------------------------------
    // PortForward tests
    // -----------------------------------------------------------------------

    #[test]
    fn port_forward_construction() {
        let pf = PortForward {
            host_port: 3000,
            guest_port: 3000,
            protocol: "tcp".into(),
        };
        assert_eq!(pf.host_port, 3000);
        assert_eq!(pf.guest_port, 3000);
        assert_eq!(pf.protocol, "tcp");
    }

    #[test]
    fn port_forward_serde_round_trip() {
        let pf = PortForward {
            host_port: 443,
            guest_port: 8443,
            protocol: "tcp".into(),
        };
        let json = serde_json::to_string(&pf).unwrap();
        let deserialized: PortForward = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.host_port, 443);
        assert_eq!(deserialized.guest_port, 8443);
    }

    // -----------------------------------------------------------------------
    // HypervisorError tests
    // -----------------------------------------------------------------------

    #[test]
    fn hypervisor_error_display() {
        assert_eq!(
            HypervisorError::CreateFailed("boom".into()).to_string(),
            "VM creation failed: boom"
        );
        assert_eq!(
            HypervisorError::NotFound("vm-1".into()).to_string(),
            "VM not found: vm-1"
        );
        assert_eq!(
            HypervisorError::Unsupported.to_string(),
            "unsupported platform"
        );
        assert_eq!(
            HypervisorError::RosettaUnavailable("msg".into()).to_string(),
            "Rosetta not available: msg"
        );
        assert_eq!(
            HypervisorError::VirtioFsError("err".into()).to_string(),
            "VirtioFS error: err"
        );
        assert_eq!(
            HypervisorError::Storage("db".into()).to_string(),
            "storage error: db"
        );
    }

    // -----------------------------------------------------------------------
    // default_disk_gb test
    // -----------------------------------------------------------------------

    #[test]
    fn default_disk_gb_is_twenty() {
        assert_eq!(default_disk_gb(), 20);
    }
}
