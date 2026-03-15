use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use crate::hypervisor::{Hypervisor, HypervisorError, VmConfig, VmState};

pub const DEFAULT_RUNTIME_VM_NAME: &str = "cratebay-runtime";
pub const DEFAULT_DOCKER_VSOCK_PORT: u32 = 6237;
pub const DEFAULT_RUNTIME_ASSETS_SUBDIR: &str = "runtime-images";
#[cfg(target_os = "windows")]
pub const DEFAULT_WSL_ASSETS_SUBDIR: &str = "runtime-wsl";

#[cfg(target_os = "windows")]
pub const DEFAULT_WSL_DOCKER_PORT: u16 = 2375;

static RUNTIME_VM_NAME: OnceLock<String> = OnceLock::new();
static DOCKER_VSOCK_PORT: OnceLock<u32> = OnceLock::new();
static DOCKER_SOCKET_PATH: OnceLock<PathBuf> = OnceLock::new();
static RUNTIME_OS_IMAGE_ID: OnceLock<String> = OnceLock::new();

/// The VM name CrateBay uses for its built-in container runtime.
pub fn runtime_vm_name() -> &'static str {
    RUNTIME_VM_NAME
        .get_or_init(|| {
            std::env::var("CRATEBAY_RUNTIME_VM_NAME")
                .ok()
                .filter(|v| !v.trim().is_empty())
                .unwrap_or_else(|| DEFAULT_RUNTIME_VM_NAME.to_string())
        })
        .as_str()
}

/// The guest vsock port for the Docker API proxy inside the runtime VM.
pub fn docker_vsock_port() -> u32 {
    *DOCKER_VSOCK_PORT.get_or_init(|| {
        std::env::var("CRATEBAY_DOCKER_VSOCK_PORT")
            .ok()
            .and_then(|v| v.parse::<u32>().ok())
            .filter(|v| *v > 0)
            .unwrap_or(DEFAULT_DOCKER_VSOCK_PORT)
    })
}

/// The host-side Docker-compatible Unix socket path exposed by CrateBay.
///
/// Defaults to `$HOME/.cratebay/run/docker.sock` to avoid spaces in paths like
/// `~/Library/Application Support/...` (which can break URL parsing in tooling
/// that consumes `DOCKER_HOST=unix://...`).
pub fn host_docker_socket_path() -> &'static Path {
    DOCKER_SOCKET_PATH
        .get_or_init(|| {
            if let Ok(p) = std::env::var("CRATEBAY_DOCKER_SOCKET_PATH") {
                if !p.trim().is_empty() {
                    return PathBuf::from(p);
                }
            }

            if let Ok(home) = std::env::var("HOME") {
                return PathBuf::from(home)
                    .join(".cratebay")
                    .join("run")
                    .join("docker.sock");
            }

            crate::store::data_dir().join("run").join("docker.sock")
        })
        .as_path()
}

/// OS image id used for the built-in runtime VM.
///
/// Can be overridden via `CRATEBAY_RUNTIME_OS_IMAGE_ID`.
pub fn runtime_os_image_id() -> &'static str {
    RUNTIME_OS_IMAGE_ID
        .get_or_init(|| {
            if let Ok(id) = std::env::var("CRATEBAY_RUNTIME_OS_IMAGE_ID") {
                if !id.trim().is_empty() {
                    return id;
                }
            }

            #[cfg(target_arch = "aarch64")]
            {
                "cratebay-runtime-aarch64".to_string()
            }

            #[cfg(target_arch = "x86_64")]
            {
                "cratebay-runtime-x86_64".to_string()
            }

            #[cfg(not(any(target_arch = "aarch64", target_arch = "x86_64")))]
            {
                "cratebay-runtime-aarch64".to_string()
            }
        })
        .as_str()
}

pub fn runtime_image_ready() -> bool {
    crate::images::is_image_ready(runtime_os_image_id())
}

fn runtime_images_dir_from_root(root: &Path) -> Option<PathBuf> {
    if root
        .file_name()
        .is_some_and(|n| n == DEFAULT_RUNTIME_ASSETS_SUBDIR)
    {
        if root.is_dir() {
            return Some(root.to_path_buf());
        }
    }

    let dir = root.join(DEFAULT_RUNTIME_ASSETS_SUBDIR);
    if dir.is_dir() {
        Some(dir)
    } else {
        None
    }
}

fn runtime_assets_root_dir_from_current_exe() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let exe_dir = exe.parent()?;

    runtime_assets_root_dir_from_exe_dir(exe_dir)
}

fn runtime_assets_root_dir_from_exe_dir(exe_dir: &Path) -> Option<PathBuf> {
    // macOS app bundle layout: <App>.app/Contents/MacOS/<exe>
    #[cfg(target_os = "macos")]
    {
        if let Some(macos_dir) = exe_dir.file_name().and_then(|s| s.to_str()) {
            if macos_dir == "MacOS" {
                if let Some(contents_dir) = exe_dir.parent() {
                    if contents_dir
                        .file_name()
                        .and_then(|s| s.to_str())
                        .is_some_and(|n| n == "Contents")
                    {
                        let resources = contents_dir.join("Resources");
                        if resources.is_dir() {
                            return Some(resources);
                        }
                    }
                }
            }
        }
    }

    // Tauri Windows/Linux layout (and common app installers): resources are placed
    // under a sibling `resources/` directory next to the executable.
    let direct_resources = exe_dir.join("resources");
    if direct_resources.is_dir() {
        return Some(direct_resources);
    }
    if let Some(parent) = exe_dir.parent() {
        let parent_resources = parent.join("resources");
        if parent_resources.is_dir() {
            return Some(parent_resources);
        }
    }

    Some(exe_dir.to_path_buf())
}

fn runtime_images_dir() -> Option<PathBuf> {
    if let Ok(dir) = std::env::var("CRATEBAY_RUNTIME_ASSETS_DIR") {
        if !dir.trim().is_empty() {
            let root = PathBuf::from(dir);
            if let Some(d) = runtime_images_dir_from_root(&root) {
                return Some(d);
            }
        }
    }

    let root = runtime_assets_root_dir_from_current_exe()?;
    runtime_images_dir_from_root(&root)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runtime_assets_root_prefers_resources_next_to_exe_dir() {
        let dir = tempfile::tempdir().unwrap();
        let exe_dir = dir.path().join("bin");
        std::fs::create_dir_all(&exe_dir).unwrap();
        std::fs::create_dir_all(exe_dir.join("resources")).unwrap();

        let root = runtime_assets_root_dir_from_exe_dir(&exe_dir).unwrap();
        assert_eq!(root, exe_dir.join("resources"));
    }

    #[test]
    fn runtime_assets_root_falls_back_to_parent_resources_dir() {
        let dir = tempfile::tempdir().unwrap();
        let exe_dir = dir.path().join("bin");
        std::fs::create_dir_all(&exe_dir).unwrap();
        std::fs::create_dir_all(dir.path().join("resources")).unwrap();

        let root = runtime_assets_root_dir_from_exe_dir(&exe_dir).unwrap();
        assert_eq!(root, dir.path().join("resources"));
    }

    #[test]
    fn runtime_assets_root_falls_back_to_exe_dir_when_no_resources() {
        let dir = tempfile::tempdir().unwrap();
        let exe_dir = dir.path().join("bin");
        std::fs::create_dir_all(&exe_dir).unwrap();

        let root = runtime_assets_root_dir_from_exe_dir(&exe_dir).unwrap();
        assert_eq!(root, exe_dir);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn runtime_assets_root_detects_macos_app_bundle_resources() {
        let dir = tempfile::tempdir().unwrap();
        let contents_dir = dir.path().join("CrateBay.app").join("Contents");
        let macos_dir = contents_dir.join("MacOS");
        let resources_dir = contents_dir.join("Resources");
        std::fs::create_dir_all(&macos_dir).unwrap();
        std::fs::create_dir_all(&resources_dir).unwrap();

        let root = runtime_assets_root_dir_from_exe_dir(&macos_dir).unwrap();
        assert_eq!(root, resources_dir);
    }
}

fn runtime_image_assets_dir(image_id: &str) -> Option<PathBuf> {
    let dir = runtime_images_dir()?.join(image_id);
    if dir.is_dir() {
        Some(dir)
    } else {
        None
    }
}

fn write_ready_metadata(image_id: &str) -> Result<(), HypervisorError> {
    let dir = crate::images::image_dir(image_id);
    std::fs::create_dir_all(&dir)?;

    let path = dir.join("metadata.json");
    let json = serde_json::to_vec_pretty(&serde_json::json!({ "status": "ready" }))
        .map_err(|e| HypervisorError::CreateFailed(e.to_string()))?;
    crate::store::write_atomic(&path, &json)?;
    Ok(())
}

fn install_runtime_image_from_assets(image_id: &str) -> Result<(), HypervisorError> {
    let Some(assets_dir) = runtime_image_assets_dir(image_id) else {
        return Err(HypervisorError::CreateFailed(format!(
            "CrateBay Runtime assets not found for image '{}'. Ensure the desktop app is installed correctly or set CRATEBAY_RUNTIME_ASSETS_DIR.",
            image_id
        )));
    };

    let dest_dir = crate::images::image_dir(image_id);
    std::fs::create_dir_all(&dest_dir)?;

    let rootfs_required = crate::images::find_image(image_id)
        .map(|e| !e.rootfs_url.trim().is_empty())
        .unwrap_or(true);

    let copy_required = |name: &str| -> Result<(), HypervisorError> {
        let src = assets_dir.join(name);
        if !src.is_file() {
            return Err(HypervisorError::CreateFailed(format!(
                "Missing runtime asset '{}': {}",
                name,
                src.display()
            )));
        }
        if let Ok(meta) = std::fs::metadata(&src) {
            if meta.len() < 1024 {
                if let Ok(txt) = std::fs::read_to_string(&src) {
                    if txt.contains("PLACEHOLDER") {
                        return Err(HypervisorError::CreateFailed(format!(
                            "Runtime asset '{}' is a placeholder. Fetch real assets before using CrateBay Runtime.",
                            src.display()
                        )));
                    }
                }
            }
        }
        let dest = dest_dir.join(name);
        crate::fsutil::copy_file_fast(&src, &dest)?;
        Ok(())
    };

    copy_required("vmlinuz")?;
    copy_required("initramfs")?;
    if rootfs_required {
        copy_required("rootfs.img")?;
    }

    write_ready_metadata(image_id)?;
    Ok(())
}

fn ensure_runtime_image_ready(image_id: &str) -> Result<(), HypervisorError> {
    if crate::images::is_image_ready(image_id) {
        return Ok(());
    }

    install_runtime_image_from_assets(image_id)?;
    if !crate::images::is_image_ready(image_id) {
        return Err(HypervisorError::CreateFailed(format!(
            "Runtime OS image '{}' was installed but is still not marked ready",
            image_id
        )));
    }
    Ok(())
}

/// Ensure the built-in runtime VM exists, returning its VM id.
pub fn ensure_runtime_vm_exists(hv: &dyn Hypervisor) -> Result<String, HypervisorError> {
    let name = runtime_vm_name();
    let vms = hv.list_vms()?;
    if let Some(vm) = vms.into_iter().find(|vm| vm.name == name) {
        return Ok(vm.id);
    }

    let image_id = runtime_os_image_id();
    if crate::images::find_image(image_id).is_none() {
        return Err(HypervisorError::CreateFailed(format!(
            "Runtime OS image '{}' not found in catalog",
            image_id
        )));
    }

    ensure_runtime_image_ready(image_id)?;

    let paths = crate::images::image_paths(image_id);
    let config = VmConfig {
        name: name.to_string(),
        cpus: 2,
        memory_mb: 2048,
        disk_gb: 20,
        rosetta: hv.rosetta_available(),
        shared_dirs: vec![],
        os_image: Some(image_id.to_string()),
        kernel_path: Some(paths.kernel_path.to_string_lossy().into_owned()),
        initrd_path: Some(paths.initrd_path.to_string_lossy().into_owned()),
        disk_path: None,
        port_forwards: vec![],
    };

    hv.create_vm(config)
}

/// Ensure the built-in runtime VM is running and the host Docker socket exists.
pub fn ensure_runtime_vm_running(hv: &dyn Hypervisor) -> Result<String, HypervisorError> {
    let vm_id = ensure_runtime_vm_exists(hv)?;

    let vms = hv.list_vms()?;
    let state = vms
        .into_iter()
        .find(|vm| vm.id == vm_id)
        .map(|vm| vm.state)
        .unwrap_or(VmState::Stopped);

    if state != VmState::Running {
        hv.start_vm(&vm_id)?;
    }

    Ok(vm_id)
}

// ---------------------------------------------------------------------------
// Windows runtime: WSL2 distro + dockerd on TCP
// ---------------------------------------------------------------------------

#[cfg(target_os = "windows")]
fn wsl_docker_port() -> u16 {
    std::env::var("CRATEBAY_WSL_DOCKER_PORT")
        .ok()
        .and_then(|v| v.parse::<u16>().ok())
        .filter(|p| *p > 0)
        .unwrap_or(DEFAULT_WSL_DOCKER_PORT)
}

#[cfg(target_os = "windows")]
fn wsl_list_distros() -> Result<Vec<String>, HypervisorError> {
    use std::process::Command;

    let out = Command::new("wsl.exe")
        .args(["-l", "-q"])
        .output()
        .map_err(|e| HypervisorError::CreateFailed(format!("Failed to run wsl.exe: {}", e)))?;

    if !out.status.success() {
        return Err(HypervisorError::CreateFailed(format!(
            "wsl.exe -l failed (exit {}): {}",
            out.status,
            String::from_utf8_lossy(&out.stderr)
        )));
    }

    let stdout = String::from_utf8_lossy(&out.stdout);
    Ok(stdout
        .lines()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .collect())
}

#[cfg(target_os = "windows")]
fn wsl_distro_exists(name: &str) -> Result<bool, HypervisorError> {
    let distros = wsl_list_distros()?;
    Ok(distros.iter().any(|d| d == name))
}

#[cfg(target_os = "windows")]
fn runtime_wsl_assets_dir_from_root(root: &Path) -> Option<PathBuf> {
    if root
        .file_name()
        .is_some_and(|n| n == DEFAULT_WSL_ASSETS_SUBDIR)
        && root.is_dir()
    {
        return Some(root.to_path_buf());
    }
    let dir = root.join(DEFAULT_WSL_ASSETS_SUBDIR);
    if dir.is_dir() {
        Some(dir)
    } else {
        None
    }
}

#[cfg(target_os = "windows")]
fn runtime_wsl_assets_dir() -> Option<PathBuf> {
    if let Ok(dir) = std::env::var("CRATEBAY_RUNTIME_ASSETS_DIR") {
        if !dir.trim().is_empty() {
            let root = PathBuf::from(dir);
            if let Some(d) = runtime_wsl_assets_dir_from_root(&root) {
                return Some(d);
            }
        }
    }

    let root = runtime_assets_root_dir_from_current_exe()?;
    runtime_wsl_assets_dir_from_root(&root)
}

#[cfg(target_os = "windows")]
fn runtime_wsl_image_id() -> String {
    #[cfg(target_arch = "aarch64")]
    {
        "cratebay-runtime-wsl-aarch64".to_string()
    }
    #[cfg(target_arch = "x86_64")]
    {
        "cratebay-runtime-wsl-x86_64".to_string()
    }
    #[cfg(not(any(target_arch = "aarch64", target_arch = "x86_64")))]
    {
        "cratebay-runtime-wsl-x86_64".to_string()
    }
}

#[cfg(target_os = "windows")]
fn runtime_wsl_rootfs_tar_path() -> Result<PathBuf, HypervisorError> {
    if let Ok(p) = std::env::var("CRATEBAY_WSL_ROOTFS_TAR") {
        if !p.trim().is_empty() {
            return Ok(PathBuf::from(p));
        }
    }

    let Some(dir) = runtime_wsl_assets_dir() else {
        return Err(HypervisorError::CreateFailed(
            "CrateBay WSL runtime assets not found. Ensure the desktop app is installed correctly or set CRATEBAY_RUNTIME_ASSETS_DIR / CRATEBAY_WSL_ROOTFS_TAR.".into(),
        ));
    };

    let image_dir = dir.join(runtime_wsl_image_id());
    let path = image_dir.join("rootfs.tar");
    if !path.is_file() {
        return Err(HypervisorError::CreateFailed(format!(
            "Missing WSL runtime asset rootfs.tar: {}",
            path.display()
        )));
    }

    if let Ok(meta) = std::fs::metadata(&path) {
        if meta.len() < 1024 {
            if let Ok(txt) = std::fs::read_to_string(&path) {
                if txt.contains("PLACEHOLDER") {
                    return Err(HypervisorError::CreateFailed(format!(
                        "WSL runtime asset '{}' is a placeholder. Fetch real assets before using CrateBay Runtime.",
                        path.display()
                    )));
                }
            }
        }
    }

    Ok(path)
}

#[cfg(target_os = "windows")]
fn wsl_import_runtime_distro(distro: &str) -> Result<(), HypervisorError> {
    use std::process::Command;

    let rootfs = runtime_wsl_rootfs_tar_path()?;
    let install_dir = crate::store::data_dir().join("wsl").join(distro);
    std::fs::create_dir_all(&install_dir)?;

    // WSL requires the install directory to be empty.
    if let Ok(mut it) = std::fs::read_dir(&install_dir) {
        if it.next().is_some() {
            return Err(HypervisorError::CreateFailed(format!(
                "WSL install directory is not empty: {}",
                install_dir.display()
            )));
        }
    }

    let out = Command::new("wsl.exe")
        .args([
            "--import",
            distro,
            &install_dir.to_string_lossy(),
            &rootfs.to_string_lossy(),
            "--version",
            "2",
        ])
        .output()
        .map_err(|e| HypervisorError::CreateFailed(format!("Failed to run wsl.exe: {}", e)))?;

    if !out.status.success() {
        return Err(HypervisorError::CreateFailed(format!(
            "wsl.exe --import failed (exit {}): {}",
            out.status,
            String::from_utf8_lossy(&out.stderr)
        )));
    }

    Ok(())
}

#[cfg(target_os = "windows")]
fn wsl_exec(distro: &str, shell_cmd: &str) -> Result<String, HypervisorError> {
    use std::process::Command;

    // Run via `sh -lc` to keep quoting predictable.
    let out = Command::new("wsl.exe")
        .args(["-d", distro, "--", "sh", "-lc", shell_cmd])
        .output()
        .map_err(|e| HypervisorError::CreateFailed(format!("Failed to run wsl.exe: {}", e)))?;

    if !out.status.success() {
        return Err(HypervisorError::CreateFailed(format!(
            "wsl.exe exec failed (exit {}): {}",
            out.status,
            String::from_utf8_lossy(&out.stderr)
        )));
    }

    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

#[cfg(target_os = "windows")]
fn wsl_guest_ip(distro: &str) -> Result<String, HypervisorError> {
    let out = wsl_exec(
        distro,
        "ip -4 -o addr show scope global 2>/dev/null | awk '{print $4}' | cut -d/ -f1 | head -n1",
    )
    .unwrap_or_default();
    if !out.trim().is_empty() {
        return Ok(out.trim().to_string());
    }

    let out = wsl_exec(
        distro,
        "hostname -I 2>/dev/null | awk '{print $1}' | tr -d '\\r'",
    )
    .unwrap_or_default();
    if !out.trim().is_empty() {
        return Ok(out.trim().to_string());
    }

    Err(HypervisorError::CreateFailed(
        "Failed to determine WSL guest IP (ensure iproute2 is installed in the runtime rootfs)"
            .into(),
    ))
}

#[cfg(target_os = "windows")]
fn local_port_open(port: u16) -> bool {
    use std::net::{SocketAddr, TcpStream};
    use std::time::Duration;

    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    TcpStream::connect_timeout(&addr, Duration::from_millis(250)).is_ok()
}

/// Ensure CrateBay Runtime is running on Windows via a WSL2 distro.
///
/// Returns a docker-compatible `DOCKER_HOST` value (e.g. `tcp://127.0.0.1:2375`).
#[cfg(target_os = "windows")]
pub fn ensure_runtime_wsl_running() -> Result<String, HypervisorError> {
    let distro = runtime_vm_name();
    let port = wsl_docker_port();

    // 1) Ensure the distro exists (import if missing).
    if !wsl_distro_exists(distro)? {
        wsl_import_runtime_distro(distro)?;
    }

    // 2) Ensure dockerd is running inside WSL.
    // Start is idempotent enough: if already running, dockerd will keep the socket/port.
    let cmd = format!(
        "mkdir -p /var/lib/docker /var/run; \
         (nohup dockerd -H unix:///var/run/docker.sock -H tcp://0.0.0.0:{port} \
           > /var/log/dockerd.log 2>&1 &) || true"
    );
    let _ = wsl_exec(distro, &cmd);

    // 3) Prefer localhost forwarding (fast + stable when available).
    if local_port_open(port) {
        return Ok(format!("tcp://127.0.0.1:{port}"));
    }

    // 4) Fallback: connect directly to the WSL guest IP.
    let ip = wsl_guest_ip(distro)?;
    Ok(format!("tcp://{ip}:{port}"))
}

/// Stop CrateBay Runtime on Windows (terminates the WSL distro).
#[cfg(target_os = "windows")]
pub fn stop_runtime_wsl() -> Result<(), HypervisorError> {
    use std::process::Command;

    let distro = runtime_vm_name();
    if !wsl_distro_exists(distro)? {
        return Ok(());
    }

    let out = Command::new("wsl.exe")
        .args(["--terminate", distro])
        .output()
        .map_err(|e| HypervisorError::CreateFailed(format!("Failed to run wsl.exe: {}", e)))?;

    if !out.status.success() {
        return Err(HypervisorError::CreateFailed(format!(
            "wsl.exe --terminate failed (exit {}): {}",
            out.status,
            String::from_utf8_lossy(&out.stderr)
        )));
    }

    Ok(())
}
