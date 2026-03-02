//! K3s (lightweight Kubernetes) management module.
//!
//! K3s is downloaded on-demand from GitHub releases, not bundled with CargoBay.
//! On macOS, K3s requires a Linux VM to run (it is a Linux-only binary).
//! This module provides a stub implementation for macOS/Windows that notes the
//! VM requirement; on Linux it can manage K3s directly.

use crate::store;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use thiserror::Error;
use tracing::{info, warn};

/// Default K3s version to download when none is specified.
#[allow(dead_code)]
const DEFAULT_K3S_VERSION: &str = "v1.31.4+k3s1";

/// GitHub release download URL pattern.
/// Linux amd64: https://github.com/k3s-io/k3s/releases/download/{version}/k3s
/// Linux arm64: https://github.com/k3s-io/k3s/releases/download/{version}/k3s-arm64
#[allow(dead_code)]
fn download_url(version: &str, arch: &str) -> String {
    let binary = match arch {
        "aarch64" | "arm64" => "k3s-arm64",
        _ => "k3s",
    };
    format!(
        "https://github.com/k3s-io/k3s/releases/download/{}/{}",
        version, binary
    )
}

#[derive(Error, Debug)]
pub enum K3sError {
    #[error("K3s is not installed")]
    NotInstalled,
    #[error("K3s is already running")]
    AlreadyRunning,
    #[error("K3s is not running")]
    NotRunning,
    #[error("K3s download failed: {0}")]
    DownloadFailed(String),
    #[error("K3s start failed: {0}")]
    StartFailed(String),
    #[error("K3s stop failed: {0}")]
    StopFailed(String),
    #[error("unsupported platform: {0}")]
    UnsupportedPlatform(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

/// Configuration for starting a K3s cluster.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct K3sConfig {
    /// Directory for K3s data storage.
    pub data_dir: PathBuf,
    /// Disable the built-in Traefik ingress controller.
    pub disable_traefik: bool,
    /// Flannel CNI backend (e.g. "vxlan", "host-gw", "wireguard-native").
    pub flannel_backend: String,
}

impl Default for K3sConfig {
    fn default() -> Self {
        Self {
            data_dir: k3s_data_dir(),
            disable_traefik: false,
            flannel_backend: "vxlan".to_string(),
        }
    }
}

/// Status information for a K3s cluster.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct K3sStatus {
    /// Whether the k3s binary is present on disk.
    pub installed: bool,
    /// Whether the k3s server process is currently running.
    pub running: bool,
    /// K3s version string (empty if not installed).
    pub version: String,
    /// Number of nodes in the cluster (0 if not running).
    pub node_count: u32,
}

/// Root directory for K3s binaries and data.
fn k3s_base_dir() -> PathBuf {
    store::data_dir().join("k3s")
}

/// Directory containing the k3s binary.
fn k3s_bin_dir() -> PathBuf {
    k3s_base_dir().join("bin")
}

/// K3s data directory (server state, etcd, etc.).
fn k3s_data_dir() -> PathBuf {
    k3s_base_dir().join("data")
}

/// Path to the k3s binary.
fn k3s_binary_path() -> PathBuf {
    k3s_bin_dir().join("k3s")
}

/// Manager for K3s lifecycle operations.
pub struct K3sManager;

impl K3sManager {
    /// Check whether the k3s binary exists in the CargoBay data directory.
    pub fn is_installed() -> bool {
        k3s_binary_path().exists()
    }

    /// Download the k3s binary from GitHub releases for the current platform.
    ///
    /// On macOS / Windows this returns an error because K3s only runs on Linux.
    /// In the future, CargoBay will run K3s inside a Linux VM on non-Linux hosts.
    #[allow(unused_variables)]
    pub async fn install(version: Option<&str>) -> Result<(), K3sError> {
        let version = version.unwrap_or(DEFAULT_K3S_VERSION);

        // NOTE: K3s is Linux-only. On macOS it must run inside a Linux VM.
        // For now we stub non-Linux platforms and provide the real download
        // path for Linux.
        #[cfg(not(target_os = "linux"))]
        {
            // On macOS/Windows, K3s would run inside a CargoBay Linux VM.
            // This is not yet implemented -- return an informational error.
            warn!("K3s install requested on non-Linux platform; K3s runs inside a Linux VM (not yet implemented)");
            Err(K3sError::UnsupportedPlatform(
                "K3s requires Linux. On macOS/Windows it will run inside a CargoBay VM (coming soon).".into(),
            ))
        }

        #[cfg(target_os = "linux")]
        {
            #[cfg(not(feature = "download"))]
            {
                Err(K3sError::DownloadFailed(
                    "Build without 'download' feature; cannot fetch K3s binary.".into(),
                ))
            }

            #[cfg(feature = "download")]
            {
                let arch = std::env::consts::ARCH;
                let url = download_url(version, arch);

                info!("Downloading K3s {} for {} from {}", version, arch, url);

                let bin_dir = k3s_bin_dir();
                std::fs::create_dir_all(&bin_dir)?;

                let binary_path = k3s_binary_path();

                let resp = reqwest::get(&url)
                    .await
                    .map_err(|e| K3sError::DownloadFailed(e.to_string()))?;

                if !resp.status().is_success() {
                    return Err(K3sError::DownloadFailed(format!(
                        "HTTP {}: {}",
                        resp.status(),
                        url
                    )));
                }

                let bytes = resp
                    .bytes()
                    .await
                    .map_err(|e| K3sError::DownloadFailed(e.to_string()))?;

                std::fs::write(&binary_path, &bytes)?;

                // Make the binary executable.
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    let mut perms = std::fs::metadata(&binary_path)?.permissions();
                    perms.set_mode(0o755);
                    std::fs::set_permissions(&binary_path, perms)?;
                }

                info!("K3s installed to {}", binary_path.display());
                Ok(())
            }
        }
    }

    /// Start the K3s server process.
    ///
    /// Runs `k3s server --write-kubeconfig-mode 644` with the provided config.
    pub fn start_cluster(config: &K3sConfig) -> Result<(), K3sError> {
        if !Self::is_installed() {
            return Err(K3sError::NotInstalled);
        }

        // Check if already running.
        if Self::is_running() {
            return Err(K3sError::AlreadyRunning);
        }

        let binary = k3s_binary_path();
        let mut cmd = std::process::Command::new(&binary);
        cmd.arg("server")
            .arg("--write-kubeconfig-mode")
            .arg("644")
            .arg("--data-dir")
            .arg(&config.data_dir)
            .arg("--flannel-backend")
            .arg(&config.flannel_backend);

        if config.disable_traefik {
            cmd.arg("--disable").arg("traefik");
        }

        cmd.stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());

        cmd.spawn()
            .map_err(|e| K3sError::StartFailed(e.to_string()))?;

        info!("K3s cluster started");
        Ok(())
    }

    /// Stop the K3s server process by running `k3s-killall.sh` or sending SIGTERM.
    pub fn stop_cluster() -> Result<(), K3sError> {
        if !Self::is_running() {
            return Err(K3sError::NotRunning);
        }

        // Try to find and kill the k3s server process.
        #[cfg(unix)]
        {
            let output = std::process::Command::new("pkill")
                .arg("-f")
                .arg("k3s server")
                .output()
                .map_err(|e| K3sError::StopFailed(e.to_string()))?;

            if !output.status.success() {
                warn!("pkill k3s returned non-zero; process may already be stopped");
            }
        }

        #[cfg(not(unix))]
        {
            return Err(K3sError::UnsupportedPlatform(
                "Stopping K3s on this platform is not yet supported.".into(),
            ));
        }

        info!("K3s cluster stopped");
        Ok(())
    }

    /// Query the current status of the K3s cluster.
    pub fn cluster_status() -> Result<K3sStatus, K3sError> {
        let installed = Self::is_installed();
        let running = Self::is_running();

        let version = if installed {
            Self::get_version().unwrap_or_default()
        } else {
            String::new()
        };

        let node_count = if running {
            Self::get_node_count().unwrap_or(0)
        } else {
            0
        };

        Ok(K3sStatus {
            installed,
            running,
            version,
            node_count,
        })
    }

    /// Path to the kubeconfig file generated by K3s.
    pub fn kubeconfig_path() -> PathBuf {
        k3s_data_dir()
            .join("server")
            .join("cred")
            .join("admin.kubeconfig")
    }

    /// Remove the k3s binary and all associated data.
    pub fn uninstall() -> Result<(), K3sError> {
        // Stop first if running.
        if Self::is_running() {
            let _ = Self::stop_cluster();
        }

        let base = k3s_base_dir();
        if base.exists() {
            std::fs::remove_dir_all(&base)?;
            info!("K3s uninstalled (removed {})", base.display());
        }

        Ok(())
    }

    /// Check whether a k3s server process is currently running.
    fn is_running() -> bool {
        #[cfg(unix)]
        {
            let output = std::process::Command::new("pgrep")
                .arg("-f")
                .arg("k3s server")
                .output();
            match output {
                Ok(o) => o.status.success(),
                Err(_) => false,
            }
        }

        #[cfg(not(unix))]
        {
            false
        }
    }

    /// Get the installed k3s version by running `k3s --version`.
    fn get_version() -> Option<String> {
        let binary = k3s_binary_path();
        let output = std::process::Command::new(&binary)
            .arg("--version")
            .output()
            .ok()?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        // Output is like: "k3s version v1.31.4+k3s1 (abc1234)"
        // Extract the version token.
        stdout
            .split_whitespace()
            .find(|s| s.starts_with('v'))
            .map(|s| s.to_string())
    }

    /// Get the number of nodes in the cluster via `k3s kubectl get nodes`.
    fn get_node_count() -> Option<u32> {
        let binary = k3s_binary_path();
        let output = std::process::Command::new(&binary)
            .args(["kubectl", "get", "nodes", "--no-headers"])
            .output()
            .ok()?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        let count = stdout.lines().filter(|l| !l.trim().is_empty()).count();
        Some(count as u32)
    }
}
