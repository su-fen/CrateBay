//! Linux OS image catalog, download, and management.
//!
//! Provides a built-in catalog of downloadable Linux distributions
//! (kernel + initrd + rootfs) for VM booting via Virtualization.framework.

use crate::store;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use thiserror::Error;

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

#[derive(Error, Debug)]
pub enum ImageError {
    #[error("image not found: {0}")]
    NotFound(String),
    #[error("image already exists: {0}")]
    AlreadyExists(String),
    #[error("download failed: {0}")]
    DownloadFailed(String),
    #[error("checksum mismatch: expected {expected}, got {actual}")]
    ChecksumMismatch { expected: String, actual: String },
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Status of an OS image on disk.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImageStatus {
    /// Not yet downloaded.
    NotDownloaded,
    /// Currently downloading.
    Downloading,
    /// Downloaded and ready to use.
    Ready,
}

/// A single downloadable Linux OS image entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OsImageEntry {
    /// Short identifier, e.g. "alpine-3.19".
    pub id: String,
    /// Human-readable name, e.g. "Alpine Linux 3.19".
    pub name: String,
    /// Distribution version string.
    pub version: String,
    /// CPU architecture (aarch64 / x86_64).
    pub arch: String,
    /// URL to download the kernel (vmlinuz).
    pub kernel_url: String,
    /// URL to download the initrd / initramfs.
    pub initrd_url: String,
    /// URL to download the root filesystem image (optional).
    pub rootfs_url: String,
    /// Approximate total download size in bytes.
    pub size_bytes: u64,
    /// SHA-256 checksum of the kernel file (hex).
    pub kernel_sha256: String,
    /// SHA-256 checksum of the initrd file (hex).
    pub initrd_sha256: String,
    /// SHA-256 checksum of the rootfs file (hex).
    pub rootfs_sha256: String,
    /// Default kernel command line.
    pub default_cmdline: String,
    /// Current status on disk.
    #[serde(default = "default_status")]
    pub status: ImageStatus,
}

fn default_status() -> ImageStatus {
    ImageStatus::NotDownloaded
}

/// Progress information for an ongoing download.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadProgress {
    pub image_id: String,
    /// Which file is being downloaded: "kernel", "initrd", or "rootfs".
    pub current_file: String,
    /// Bytes downloaded so far (across all files).
    pub bytes_downloaded: u64,
    /// Total bytes to download (across all files).
    pub bytes_total: u64,
    /// true when the download is complete.
    pub done: bool,
    /// Error message if something went wrong.
    pub error: Option<String>,
}

/// Paths to the downloaded image files on disk.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImagePaths {
    pub kernel_path: PathBuf,
    pub initrd_path: PathBuf,
    pub rootfs_path: PathBuf,
}

// ---------------------------------------------------------------------------
// Built-in catalog
// ---------------------------------------------------------------------------

/// Return the built-in catalog of available Linux images.
pub fn builtin_catalog() -> Vec<OsImageEntry> {
    vec![
        OsImageEntry {
            id: "alpine-3.19".into(),
            name: "Alpine Linux 3.19".into(),
            version: "3.19".into(),
            arch: "aarch64".into(),
            kernel_url: "https://dl-cdn.alpinelinux.org/alpine/v3.19/releases/aarch64/netboot/vmlinuz-lts".into(),
            initrd_url: "https://dl-cdn.alpinelinux.org/alpine/v3.19/releases/aarch64/netboot/initramfs-lts".into(),
            rootfs_url: "https://dl-cdn.alpinelinux.org/alpine/v3.19/releases/aarch64/alpine-minirootfs-3.19.0-aarch64.tar.gz".into(),
            size_bytes: 50_000_000,
            kernel_sha256: "".into(),
            initrd_sha256: "".into(),
            rootfs_sha256: "".into(),
            default_cmdline: "console=hvc0 root=/dev/vda rw".into(),
            status: ImageStatus::NotDownloaded,
        },
        OsImageEntry {
            id: "ubuntu-24.04".into(),
            name: "Ubuntu Server 24.04 LTS".into(),
            version: "24.04".into(),
            arch: "aarch64".into(),
            kernel_url: "https://cloud-images.ubuntu.com/releases/24.04/release/unpacked/ubuntu-24.04-server-cloudimg-arm64-vmlinuz-generic".into(),
            initrd_url: "https://cloud-images.ubuntu.com/releases/24.04/release/unpacked/ubuntu-24.04-server-cloudimg-arm64-initrd-generic".into(),
            rootfs_url: "https://cloud-images.ubuntu.com/releases/24.04/release/ubuntu-24.04-server-cloudimg-arm64.img".into(),
            size_bytes: 300_000_000,
            kernel_sha256: "".into(),
            initrd_sha256: "".into(),
            rootfs_sha256: "".into(),
            default_cmdline: "console=hvc0 root=/dev/vda1 rw".into(),
            status: ImageStatus::NotDownloaded,
        },
        OsImageEntry {
            id: "debian-12".into(),
            name: "Debian 12 (Bookworm)".into(),
            version: "12".into(),
            arch: "aarch64".into(),
            kernel_url: "https://cloud.debian.org/images/cloud/bookworm/latest/debian-12-nocloud-arm64-vmlinuz".into(),
            initrd_url: "https://cloud.debian.org/images/cloud/bookworm/latest/debian-12-nocloud-arm64-initrd".into(),
            rootfs_url: "https://cloud.debian.org/images/cloud/bookworm/latest/debian-12-nocloud-arm64.raw".into(),
            size_bytes: 250_000_000,
            kernel_sha256: "".into(),
            initrd_sha256: "".into(),
            rootfs_sha256: "".into(),
            default_cmdline: "console=hvc0 root=/dev/vda1 rw".into(),
            status: ImageStatus::NotDownloaded,
        },
    ]
}

// ---------------------------------------------------------------------------
// Path helpers
// ---------------------------------------------------------------------------

/// Directory where OS images are stored: `<data_dir>/images/`
pub fn images_dir() -> PathBuf {
    store::data_dir().join("images")
}

/// Directory for a specific image: `<data_dir>/images/<image_id>/`
pub fn image_dir(image_id: &str) -> PathBuf {
    images_dir().join(image_id)
}

/// Return the on-disk paths for a given image id (whether or not the files exist).
pub fn image_paths(image_id: &str) -> ImagePaths {
    let dir = image_dir(image_id);
    ImagePaths {
        kernel_path: dir.join("vmlinuz"),
        initrd_path: dir.join("initramfs"),
        rootfs_path: dir.join("rootfs.img"),
    }
}

/// The metadata file that tracks download state.
fn metadata_path(image_id: &str) -> PathBuf {
    image_dir(image_id).join("metadata.json")
}

// ---------------------------------------------------------------------------
// Status tracking
// ---------------------------------------------------------------------------

/// Persist the status of an image to `metadata.json`.
fn save_image_status(image_id: &str, status: &ImageStatus) -> Result<(), ImageError> {
    let dir = image_dir(image_id);
    std::fs::create_dir_all(&dir)?;

    #[derive(Serialize)]
    struct Meta {
        status: ImageStatus,
    }

    let json = serde_json::to_vec_pretty(&Meta {
        status: status.clone(),
    })
    .map_err(|e| ImageError::DownloadFailed(e.to_string()))?;
    std::fs::write(metadata_path(image_id), json)?;
    Ok(())
}

/// Load the status of an image from disk.
fn load_image_status(image_id: &str) -> ImageStatus {
    let path = metadata_path(image_id);
    if !path.exists() {
        return ImageStatus::NotDownloaded;
    }

    #[derive(Deserialize)]
    struct Meta {
        #[serde(default = "default_status")]
        status: ImageStatus,
    }

    let Ok(bytes) = std::fs::read(&path) else {
        return ImageStatus::NotDownloaded;
    };
    let Ok(meta) = serde_json::from_slice::<Meta>(&bytes) else {
        return ImageStatus::NotDownloaded;
    };
    meta.status
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// List all available OS images, with their current download status.
pub fn list_available_images() -> Vec<OsImageEntry> {
    let mut catalog = builtin_catalog();
    for entry in &mut catalog {
        entry.status = load_image_status(&entry.id);
    }
    catalog
}

/// List only images that have been downloaded and are ready.
pub fn list_downloaded_images() -> Vec<OsImageEntry> {
    list_available_images()
        .into_iter()
        .filter(|e| e.status == ImageStatus::Ready)
        .collect()
}

/// Find a catalog entry by id.
pub fn find_image(id: &str) -> Option<OsImageEntry> {
    list_available_images().into_iter().find(|e| e.id == id)
}

/// Delete a downloaded image from disk.
pub fn delete_image(image_id: &str) -> Result<(), ImageError> {
    let dir = image_dir(image_id);
    if !dir.exists() {
        return Err(ImageError::NotFound(image_id.into()));
    }
    std::fs::remove_dir_all(&dir)?;
    Ok(())
}

/// Check if an image is downloaded and ready.
pub fn is_image_ready(image_id: &str) -> bool {
    load_image_status(image_id) == ImageStatus::Ready
}

// ---------------------------------------------------------------------------
// Download (async, uses reqwest)
// ---------------------------------------------------------------------------

/// Download an OS image (kernel + initrd + rootfs).
///
/// `progress_cb` is called periodically with current progress. The callback
/// receives `(current_file, bytes_so_far, total_bytes)`.
///
/// This function is async and requires a tokio runtime.
#[cfg(feature = "download")]
pub async fn download_image<F>(image_id: &str, progress_cb: F) -> Result<ImagePaths, ImageError>
where
    F: Fn(&str, u64, u64) + Send + 'static,
{
    use sha2::{Digest, Sha256};
    use tokio::io::AsyncWriteExt;

    let entry = builtin_catalog()
        .into_iter()
        .find(|e| e.id == image_id)
        .ok_or_else(|| ImageError::NotFound(image_id.into()))?;

    let current_status = load_image_status(image_id);
    if current_status == ImageStatus::Downloading {
        return Err(ImageError::DownloadFailed(
            "Download already in progress".into(),
        ));
    }

    // Mark as downloading.
    save_image_status(image_id, &ImageStatus::Downloading)?;

    let paths = image_paths(image_id);
    let dir = image_dir(image_id);
    std::fs::create_dir_all(&dir)?;

    let total = entry.size_bytes;

    // Files to download: (url, dest_path, sha256, label)
    let files = [
        (
            &entry.kernel_url,
            &paths.kernel_path,
            &entry.kernel_sha256,
            "kernel",
        ),
        (
            &entry.initrd_url,
            &paths.initrd_path,
            &entry.initrd_sha256,
            "initrd",
        ),
        (
            &entry.rootfs_url,
            &paths.rootfs_path,
            &entry.rootfs_sha256,
            "rootfs",
        ),
    ];

    let client = reqwest::Client::builder()
        .user_agent("CargoBay/0.1.0")
        .build()
        .map_err(|e| ImageError::DownloadFailed(e.to_string()))?;

    let mut cumulative: u64 = 0;

    for (url, dest, expected_sha256, label) in &files {
        progress_cb(label, cumulative, total);

        let resp = client
            .get(*url)
            .send()
            .await
            .map_err(|e| ImageError::DownloadFailed(format!("{}: {}", label, e)))?;

        if !resp.status().is_success() {
            save_image_status(image_id, &ImageStatus::NotDownloaded)?;
            return Err(ImageError::DownloadFailed(format!(
                "{}: HTTP {}",
                label,
                resp.status()
            )));
        }

        let mut file = tokio::fs::File::create(dest)
            .await
            .map_err(ImageError::Io)?;
        let mut hasher = Sha256::new();
        let mut stream = resp.bytes_stream();

        use futures_util::StreamExt;
        while let Some(chunk_result) = stream.next().await {
            let chunk = chunk_result
                .map_err(|e| ImageError::DownloadFailed(format!("{}: {}", label, e)))?;
            file.write_all(&chunk).await.map_err(ImageError::Io)?;
            hasher.update(&chunk);
            cumulative += chunk.len() as u64;
            progress_cb(label, cumulative, total);
        }

        file.flush().await.map_err(ImageError::Io)?;
        drop(file);

        // Verify checksum if provided (non-empty).
        if !expected_sha256.is_empty() {
            let digest = format!("{:x}", hasher.finalize());
            if digest != **expected_sha256 {
                save_image_status(image_id, &ImageStatus::NotDownloaded)?;
                return Err(ImageError::ChecksumMismatch {
                    expected: expected_sha256.to_string(),
                    actual: digest,
                });
            }
        }
    }

    // Mark as ready.
    save_image_status(image_id, &ImageStatus::Ready)?;

    progress_cb("done", total, total);
    Ok(paths)
}

/// Lightweight progress query: returns the current download progress from a
/// shared state file. This is used by the GUI to poll progress.
#[cfg(feature = "download")]
pub fn read_download_progress(image_id: &str) -> DownloadProgress {
    let status = load_image_status(image_id);
    let entry = builtin_catalog().into_iter().find(|e| e.id == image_id);
    let total = entry.as_ref().map(|e| e.size_bytes).unwrap_or(0);

    match status {
        ImageStatus::Ready => DownloadProgress {
            image_id: image_id.into(),
            current_file: "done".into(),
            bytes_downloaded: total,
            bytes_total: total,
            done: true,
            error: None,
        },
        ImageStatus::Downloading => {
            // Estimate progress based on which files exist.
            let paths = image_paths(image_id);
            let mut downloaded: u64 = 0;
            for p in [&paths.kernel_path, &paths.initrd_path, &paths.rootfs_path] {
                if let Ok(meta) = std::fs::metadata(p) {
                    downloaded += meta.len();
                }
            }
            DownloadProgress {
                image_id: image_id.into(),
                current_file: "downloading".into(),
                bytes_downloaded: downloaded,
                bytes_total: total,
                done: false,
                error: None,
            }
        }
        ImageStatus::NotDownloaded => DownloadProgress {
            image_id: image_id.into(),
            current_file: "".into(),
            bytes_downloaded: 0,
            bytes_total: total,
            done: false,
            error: None,
        },
    }
}

/// Create a VM disk image by copying the rootfs or creating a blank raw file.
///
/// If the rootfs file exists it is used as the base; otherwise a sparse
/// raw image of `size_bytes` is created.
pub fn create_disk_from_image(
    image_id: &str,
    dest: &Path,
    size_bytes: u64,
) -> Result<(), ImageError> {
    let paths = image_paths(image_id);

    if paths.rootfs_path.exists() {
        // Copy the rootfs as the disk image.
        std::fs::copy(&paths.rootfs_path, dest)?;
    } else {
        // Create a sparse raw disk image.
        let f = std::fs::File::create(dest)?;
        f.set_len(size_bytes)?;
    }

    Ok(())
}
