use crate::hypervisor::{HypervisorError, VmInfo};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};

#[derive(Debug, Serialize, Deserialize, Default)]
struct VmStoreFile {
    version: u32,
    #[serde(default)]
    vms: Vec<VmInfo>,
}

#[derive(Debug, Clone)]
pub struct VmStore {
    path: PathBuf,
}

impl Default for VmStore {
    fn default() -> Self {
        Self::new()
    }
}

impl VmStore {
    pub fn new() -> Self {
        let path = config_dir().join("vms.json");
        Self { path }
    }

    pub fn load_vms(&self) -> Result<Vec<VmInfo>, HypervisorError> {
        if !self.path.exists() {
            return Ok(vec![]);
        }

        let content = std::fs::read_to_string(&self.path)?;
        let mut file: VmStoreFile =
            serde_json::from_str(&content).map_err(|e| HypervisorError::Storage(e.to_string()))?;

        if file.version == 0 {
            file.version = 1;
        }

        // De-dupe by id (last one wins).
        let mut by_id: HashMap<String, VmInfo> = HashMap::new();
        for vm in file.vms {
            by_id.insert(vm.id.clone(), vm);
        }

        Ok(by_id.into_values().collect())
    }

    pub fn save_vms(&self, vms: &[VmInfo]) -> Result<(), HypervisorError> {
        let file = VmStoreFile {
            version: 1,
            vms: vms.to_vec(),
        };

        let json = serde_json::to_vec_pretty(&file)
            .map_err(|e| HypervisorError::Storage(e.to_string()))?;
        write_atomic(&self.path, &json)?;
        Ok(())
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

pub fn next_id_for_prefix(vms: &[VmInfo], prefix: &str) -> u64 {
    vms.iter()
        .filter_map(|vm| vm.id.strip_prefix(prefix))
        .filter_map(|rest| rest.parse::<u64>().ok())
        .max()
        .unwrap_or(0)
        .saturating_add(1)
        .max(1)
}

pub fn config_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("CARGOBAY_CONFIG_DIR") {
        return PathBuf::from(dir);
    }

    #[cfg(target_os = "macos")]
    {
        if let Ok(home) = std::env::var("HOME") {
            return PathBuf::from(home)
                .join("Library")
                .join("Application Support")
                .join("com.cargobay.app");
        }
    }

    #[cfg(target_os = "linux")]
    {
        if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME") {
            return PathBuf::from(xdg).join("cargobay");
        }
        if let Ok(home) = std::env::var("HOME") {
            return PathBuf::from(home).join(".config").join("cargobay");
        }
    }

    #[cfg(target_os = "windows")]
    {
        if let Some(appdata) = std::env::var_os("APPDATA") {
            return PathBuf::from(appdata).join("cargobay");
        }
    }

    std::env::temp_dir().join("cargobay")
}

pub fn data_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("CARGOBAY_DATA_DIR") {
        return PathBuf::from(dir);
    }

    #[cfg(target_os = "linux")]
    {
        if let Ok(xdg) = std::env::var("XDG_DATA_HOME") {
            return PathBuf::from(xdg).join("cargobay");
        }
        if let Ok(home) = std::env::var("HOME") {
            return PathBuf::from(home)
                .join(".local")
                .join("share")
                .join("cargobay");
        }
    }

    // Default: same as config dir (macOS/Windows per docs).
    config_dir()
}

pub fn log_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("CARGOBAY_LOG_DIR") {
        return PathBuf::from(dir);
    }

    #[cfg(target_os = "linux")]
    {
        data_dir()
    }

    #[cfg(not(target_os = "linux"))]
    {
        config_dir()
    }
}

fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(dir)?;

    let file_name = path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("vms.json");
    let unique = format!(
        "{}.{}.{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos(),
        file_name
    );
    let tmp_path = dir.join(format!(".{}.tmp", unique));

    {
        let mut file = std::fs::File::create(&tmp_path)?;
        file.write_all(bytes)?;
        file.write_all(b"\n")?;
        file.sync_all()?;
    }

    match std::fs::rename(&tmp_path, path) {
        Ok(()) => Ok(()),
        Err(e) => {
            // Windows fails rename if destination exists.
            if path.exists() {
                let _ = std::fs::remove_file(path);
                std::fs::rename(&tmp_path, path).map_err(|_| e)?;
                return Ok(());
            }
            let _ = std::fs::remove_file(&tmp_path);
            Err(e)
        }
    }
}
