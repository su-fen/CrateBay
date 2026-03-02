use bollard::container::{
    Config, CreateContainerOptions, ListContainersOptions, LogsOptions, RemoveContainerOptions,
    StartContainerOptions, StatsOptions, StopContainerOptions,
};
use bollard::exec::{CreateExecOptions, StartExecResults};
use bollard::image::CreateImageOptions;
use bollard::service::HostConfig;
#[cfg_attr(mobile, tauri::mobile_entry_point)]
use bollard::Docker;
use futures_util::stream::TryStreamExt;
use reqwest::header::WWW_AUTHENTICATE;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;
use std::time::Duration;
use tauri::menu::{MenuBuilder, MenuItemBuilder, PredefinedMenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{Manager, State, WindowEvent};
use tonic::transport::Channel;
use tracing::{error, info, warn};

use cargobay_core::proto;
use cargobay_core::proto::vm_service_client::VmServiceClient;

pub struct AppState {
    hv: Box<dyn cargobay_core::hypervisor::Hypervisor>,
    grpc_addr: String,
    daemon: Mutex<Option<Child>>,
}

impl Drop for AppState {
    fn drop(&mut self) {
        let Ok(mut guard) = self.daemon.lock() else {
            return;
        };
        let Some(mut child) = guard.take() else {
            return;
        };

        let _ = child.kill();
        let _ = child.wait();
    }
}

fn detect_docker_socket() -> Option<String> {
    // Unix socket detection (macOS / Linux)
    #[cfg(unix)]
    {
        let home = std::env::var("HOME").unwrap_or_default();
        let candidates = [
            format!("{}/.colima/default/docker.sock", home),
            format!("{}/.orbstack/run/docker.sock", home),
            "/var/run/docker.sock".to_string(),
            format!("{}/.docker/run/docker.sock", home),
        ];
        if let Some(sock) = candidates.into_iter().find(|p| Path::new(p).exists()) {
            return Some(sock);
        }
    }

    None
}

fn connect_docker() -> Result<Docker, String> {
    // Check DOCKER_HOST env first
    if std::env::var("DOCKER_HOST").is_ok() {
        return Docker::connect_with_local_defaults()
            .map_err(|e| format!("Failed to connect via DOCKER_HOST: {}", e));
    }

    #[cfg(unix)]
    {
        if let Some(sock) = detect_docker_socket() {
            return Docker::connect_with_socket(&sock, 120, bollard::API_DEFAULT_VERSION)
                .map_err(|e| format!("Failed to connect to Docker at {}: {}", sock, e));
        }
        Err("No Docker socket found. Set DOCKER_HOST or install Docker/Colima/OrbStack.".into())
    }

    #[cfg(windows)]
    {
        let candidates = [
            r"//./pipe/docker_engine",
            r"//./pipe/dockerDesktopLinuxEngine",
        ];
        for pipe in &candidates {
            if let Ok(d) = Docker::connect_with_named_pipe(pipe, 120, bollard::API_DEFAULT_VERSION)
            {
                return Ok(d);
            }
        }
        Err("No Docker named pipe found. Set DOCKER_HOST or install Docker Desktop.".into())
    }

    #[cfg(not(any(unix, windows)))]
    {
        Docker::connect_with_local_defaults()
            .map_err(|e| format!("Failed to connect to Docker: {}", e))
    }
}

fn docker_host_for_cli() -> Option<String> {
    if let Ok(v) = std::env::var("DOCKER_HOST") {
        return Some(v);
    }
    #[cfg(unix)]
    {
        detect_docker_socket().map(|sock| format!("unix://{}", sock))
    }
    #[cfg(windows)]
    {
        None
    }
    #[cfg(not(any(unix, windows)))]
    {
        None
    }
}

fn grpc_addr() -> String {
    std::env::var("CARGOBAY_GRPC_ADDR").unwrap_or_else(|_| "127.0.0.1:50051".into())
}

fn grpc_endpoint(addr: &str) -> String {
    if addr.starts_with("http://") || addr.starts_with("https://") {
        addr.to_string()
    } else {
        format!("http://{}", addr)
    }
}

async fn connect_vm_service(addr: &str) -> Result<VmServiceClient<Channel>, String> {
    let endpoint = grpc_endpoint(addr);
    let connect_fut = VmServiceClient::connect(endpoint.clone());
    tokio::time::timeout(Duration::from_secs(1), connect_fut)
        .await
        .map_err(|_| format!("Timed out connecting to daemon at {}", endpoint))?
        .map_err(|e| format!("Failed to connect to daemon at {}: {}", endpoint, e))
}

fn daemon_bin_name() -> &'static str {
    if cfg!(windows) {
        "cargobay-daemon.exe"
    } else {
        "cargobay-daemon"
    }
}

fn spawn_daemon(grpc_addr: &str) -> Result<Child, String> {
    let mut tried: Vec<String> = Vec::new();

    if let Ok(path) = std::env::var("CARGOBAY_DAEMON_PATH") {
        let mut cmd = Command::new(&path);
        cmd.env("CARGOBAY_GRPC_ADDR", grpc_addr);
        cmd.stdin(Stdio::null());
        if cfg!(debug_assertions) {
            cmd.stdout(Stdio::inherit());
            cmd.stderr(Stdio::inherit());
        } else {
            cmd.stdout(Stdio::null());
            cmd.stderr(Stdio::null());
        }
        return cmd.spawn().map_err(|e| {
            format!(
                "Failed to spawn daemon from CARGOBAY_DAEMON_PATH ({}): {}",
                path, e
            )
        });
    }

    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let candidate = dir.join(daemon_bin_name());
            if candidate.is_file() {
                let mut cmd = Command::new(&candidate);
                cmd.env("CARGOBAY_GRPC_ADDR", grpc_addr);
                cmd.stdin(Stdio::null());
                if cfg!(debug_assertions) {
                    cmd.stdout(Stdio::inherit());
                    cmd.stderr(Stdio::inherit());
                } else {
                    cmd.stdout(Stdio::null());
                    cmd.stderr(Stdio::null());
                }
                return cmd.spawn().map_err(|e| {
                    format!(
                        "Failed to spawn daemon next to GUI binary ({}): {}",
                        candidate.display(),
                        e
                    )
                });
            }
            tried.push(candidate.display().to_string());
        }
    }

    tried.push("cargobay-daemon (PATH)".into());
    let mut cmd = Command::new("cargobay-daemon");
    cmd.env("CARGOBAY_GRPC_ADDR", grpc_addr);
    cmd.stdin(Stdio::null());
    if cfg!(debug_assertions) {
        cmd.stdout(Stdio::inherit());
        cmd.stderr(Stdio::inherit());
    } else {
        cmd.stdout(Stdio::null());
        cmd.stderr(Stdio::null());
    }

    cmd.spawn().map_err(|e| {
        format!(
            "Failed to spawn daemon (tried: {}): {}",
            tried.join(", "),
            e
        )
    })
}

async fn wait_for_daemon(grpc_addr: &str, timeout: Duration) -> bool {
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        if connect_vm_service(grpc_addr).await.is_ok() {
            return true;
        }

        if tokio::time::Instant::now() >= deadline {
            return false;
        }

        tokio::time::sleep(Duration::from_millis(200)).await;
    }
}

#[derive(Serialize)]
pub struct ContainerInfo {
    id: String,
    name: String,
    image: String,
    state: String,
    status: String,
    ports: String,
}

#[tauri::command]
async fn list_containers() -> Result<Vec<ContainerInfo>, String> {
    let docker = connect_docker()?;

    let opts = ListContainersOptions::<String> {
        all: true,
        ..Default::default()
    };

    let containers = docker
        .list_containers(Some(opts))
        .await
        .map_err(|e| e.to_string())?;

    Ok(containers
        .into_iter()
        .map(|c| {
            let ports = c
                .ports
                .unwrap_or_default()
                .iter()
                .filter_map(|p| {
                    p.public_port
                        .map(|pub_p| format!("{}:{}", pub_p, p.private_port))
                })
                .collect::<Vec<_>>()
                .join(", ");

            let full_id = c.id.unwrap_or_default();
            let id = full_id.chars().take(12).collect::<String>();

            ContainerInfo {
                id,
                name: c
                    .names
                    .unwrap_or_default()
                    .first()
                    .unwrap_or(&String::new())
                    .trim_start_matches('/')
                    .to_string(),
                image: c.image.unwrap_or_default(),
                state: c.state.unwrap_or_default(),
                status: c.status.unwrap_or_default(),
                ports,
            }
        })
        .collect())
}

#[tauri::command]
async fn stop_container(id: String) -> Result<(), String> {
    let docker = connect_docker()?;
    docker
        .stop_container(&id, Some(StopContainerOptions { t: 10 }))
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn start_container(id: String) -> Result<(), String> {
    let docker = connect_docker()?;
    docker
        .start_container(&id, None::<StartContainerOptions<String>>)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn remove_container(id: String) -> Result<(), String> {
    let docker = connect_docker()?;
    let _ = docker
        .stop_container(&id, Some(StopContainerOptions { t: 10 }))
        .await;
    docker
        .remove_container(
            &id,
            Some(RemoveContainerOptions {
                force: true,
                ..Default::default()
            }),
        )
        .await
        .map_err(|e| e.to_string())
}

#[derive(Serialize)]
pub struct RunContainerResult {
    id: String,
    name: String,
    image: String,
    login_cmd: String,
}

#[tauri::command]
async fn docker_run(
    image: String,
    name: Option<String>,
    cpus: Option<u32>,
    memory_mb: Option<u64>,
    pull: bool,
) -> Result<RunContainerResult, String> {
    let docker = connect_docker()?;

    if pull {
        docker_pull_image(&docker, &image).await?;
    }

    let mut host_config = HostConfig::default();
    if let Some(c) = cpus {
        host_config.nano_cpus = Some((c as i64) * 1_000_000_000);
    }
    if let Some(mb) = memory_mb {
        let bytes = (mb as i64).saturating_mul(1024).saturating_mul(1024);
        host_config.memory = Some(bytes);
    }

    let config = Config::<String> {
        image: Some(image.clone()),
        host_config: Some(host_config),
        ..Default::default()
    };

    let create_opts = name.as_deref().map(|n| CreateContainerOptions::<String> {
        name: n.to_string(),
        platform: None,
    });

    let result = docker
        .create_container(create_opts, config)
        .await
        .map_err(|e| e.to_string())?;

    docker
        .start_container(&result.id, None::<StartContainerOptions<String>>)
        .await
        .map_err(|e| e.to_string())?;

    let id = result.id.chars().take(12).collect::<String>();
    let display = name.clone().unwrap_or_else(|| id.clone());
    let login_cmd = format!("docker exec -it {} /bin/sh", display);

    Ok(RunContainerResult {
        id,
        name: display,
        image,
        login_cmd,
    })
}

#[tauri::command]
fn container_login_cmd(container: String, shell: String) -> String {
    format!("docker exec -it {} {}", container, shell)
}

#[tauri::command]
async fn container_logs(
    id: String,
    tail: Option<String>,
    timestamps: bool,
) -> Result<String, String> {
    let docker = connect_docker()?;

    let tail_value = tail.unwrap_or_else(|| "200".to_string());

    let opts = LogsOptions::<String> {
        follow: false,
        stdout: true,
        stderr: true,
        timestamps,
        tail: tail_value,
        ..Default::default()
    };

    let mut stream = docker.logs(&id, Some(opts));
    let mut output = String::new();
    while let Some(chunk) = stream.try_next().await.map_err(|e| e.to_string())? {
        output.push_str(&chunk.to_string());
    }

    Ok(output)
}

#[tauri::command]
async fn container_exec(container_id: String, command: String) -> Result<String, String> {
    let docker = connect_docker()?;

    let cmd_parts: Vec<&str> = command.split_whitespace().collect();
    if cmd_parts.is_empty() {
        return Err("Empty command".into());
    }

    let exec = docker
        .create_exec(
            &container_id,
            CreateExecOptions {
                attach_stdout: Some(true),
                attach_stderr: Some(true),
                cmd: Some(cmd_parts.into_iter().map(String::from).collect()),
                ..Default::default()
            },
        )
        .await
        .map_err(|e| format!("Failed to create exec: {}", e))?;

    let output = docker
        .start_exec(&exec.id, None)
        .await
        .map_err(|e| format!("Failed to start exec: {}", e))?;

    let mut result = String::new();
    if let StartExecResults::Attached { mut output, .. } = output {
        while let Some(chunk) = output.try_next().await.map_err(|e| e.to_string())? {
            result.push_str(&chunk.to_string());
        }
    }

    Ok(result)
}

#[tauri::command]
fn container_exec_interactive_cmd(container_id: String) -> String {
    let docker_host = docker_host_for_cli();
    if let Some(host) = docker_host {
        format!("DOCKER_HOST={} docker exec -it {} /bin/sh", host, container_id)
    } else {
        format!("docker exec -it {} /bin/sh", container_id)
    }
}

#[derive(Debug, Serialize)]
pub struct ContainerStats {
    cpu_percent: f64,
    memory_usage_mb: f64,
    memory_limit_mb: f64,
    memory_percent: f64,
    network_rx_bytes: u64,
    network_tx_bytes: u64,
}

#[tauri::command]
async fn container_stats(id: String) -> Result<ContainerStats, String> {
    let docker = connect_docker()?;

    let opts = StatsOptions {
        stream: false,
        one_shot: true,
    };

    let mut stream = docker.stats(&id, Some(opts));
    let stats = stream
        .try_next()
        .await
        .map_err(|e| format!("Failed to get stats for {}: {}", id, e))?
        .ok_or_else(|| format!("No stats returned for container {}", id))?;

    // Calculate CPU percent
    let cpu_percent = {
        let cpu_delta = stats.cpu_stats.cpu_usage.total_usage as f64
            - stats.precpu_stats.cpu_usage.total_usage as f64;
        let system_delta = stats.cpu_stats.system_cpu_usage.unwrap_or(0) as f64
            - stats.precpu_stats.system_cpu_usage.unwrap_or(0) as f64;
        let num_cpus = stats
            .cpu_stats
            .online_cpus
            .unwrap_or(1) as f64;

        if system_delta > 0.0 && cpu_delta >= 0.0 {
            (cpu_delta / system_delta) * num_cpus * 100.0
        } else {
            0.0
        }
    };

    // Memory usage
    let memory_usage = stats.memory_stats.usage.unwrap_or(0);
    let memory_limit = stats.memory_stats.limit.unwrap_or(0);
    let memory_usage_mb = memory_usage as f64 / 1024.0 / 1024.0;
    let memory_limit_mb = memory_limit as f64 / 1024.0 / 1024.0;
    let memory_percent = if memory_limit > 0 {
        (memory_usage as f64 / memory_limit as f64) * 100.0
    } else {
        0.0
    };

    // Network stats
    let (network_rx_bytes, network_tx_bytes) = stats
        .networks
        .as_ref()
        .map(|nets| {
            nets.values().fold((0u64, 0u64), |(rx, tx), net| {
                (rx + net.rx_bytes, tx + net.tx_bytes)
            })
        })
        .unwrap_or((0, 0));

    Ok(ContainerStats {
        cpu_percent,
        memory_usage_mb,
        memory_limit_mb,
        memory_percent,
        network_rx_bytes,
        network_tx_bytes,
    })
}

#[derive(Debug, Serialize)]
pub struct ImageSearchResult {
    source: String,
    reference: String,
    description: String,
    stars: Option<u64>,
    pulls: Option<u64>,
    official: bool,
}

#[derive(Deserialize)]
struct DockerHubSearchResponse {
    results: Vec<DockerHubRepo>,
}

#[derive(Deserialize)]
struct DockerHubRepo {
    name: String,
    namespace: Option<String>,
    description: Option<String>,
    star_count: Option<u64>,
    pull_count: Option<u64>,
    is_official: Option<bool>,
}

#[derive(Deserialize)]
struct RegistryTagsResponse {
    tags: Option<Vec<String>>,
}

#[derive(Deserialize)]
struct RegistryTokenResponse {
    token: Option<String>,
    access_token: Option<String>,
}

fn http_client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .user_agent("CargoBay/0.1.0 (+https://github.com/coder-hhx/CargoBay)")
        .build()
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn image_search(
    query: String,
    source: String,
    limit: usize,
) -> Result<Vec<ImageSearchResult>, String> {
    let client = http_client()?;
    let src = source.to_ascii_lowercase();
    let mut items: Vec<ImageSearchResult> = Vec::new();
    let mut did_any = false;

    if matches!(src.as_str(), "all" | "dockerhub" | "hub" | "docker") {
        did_any = true;
        items.extend(search_dockerhub(&client, &query, limit).await?);
    }
    if matches!(src.as_str(), "all" | "quay") {
        did_any = true;
        items.extend(search_quay(&client, &query, limit).await?);
    }

    if !did_any {
        return Err(format!("Unknown source: {}", source));
    }

    Ok(items)
}

#[tauri::command]
async fn image_tags(reference: String, limit: usize) -> Result<Vec<String>, String> {
    let client = http_client()?;
    let Some((registry, repo)) = parse_registry_reference(&reference) else {
        return Err("Invalid reference. Expected e.g. ghcr.io/org/image".into());
    };
    list_registry_tags(&client, &registry, &repo, limit).await
}

#[tauri::command]
async fn image_load(path: String) -> Result<String, String> {
    let docker_host = docker_host_for_cli();
    tokio::task::spawn_blocking(move || {
        let mut cmd = std::process::Command::new("docker");
        cmd.arg("load").arg("-i").arg(&path);
        if let Some(host) = docker_host {
            cmd.env("DOCKER_HOST", host);
        }
        let out = cmd
            .output()
            .map_err(|e| format!("Failed to run docker: {}", e))?;
        if !out.status.success() {
            return Err(format!(
                "docker load failed (exit {}): {}",
                out.status.code().unwrap_or(-1),
                String::from_utf8_lossy(&out.stderr).trim()
            ));
        }
        Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn image_push(reference: String) -> Result<String, String> {
    let docker_host = docker_host_for_cli();
    tokio::task::spawn_blocking(move || {
        let mut cmd = std::process::Command::new("docker");
        cmd.arg("push").arg(&reference);
        if let Some(host) = docker_host {
            cmd.env("DOCKER_HOST", host);
        }
        let out = cmd
            .output()
            .map_err(|e| format!("Failed to run docker: {}", e))?;
        if !out.status.success() {
            return Err(format!(
                "docker push failed (exit {}): {}",
                out.status.code().unwrap_or(-1),
                String::from_utf8_lossy(&out.stderr).trim()
            ));
        }
        Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn image_pack_container(container: String, tag: String) -> Result<String, String> {
    let docker_host = docker_host_for_cli();
    tokio::task::spawn_blocking(move || {
        let mut cmd = std::process::Command::new("docker");
        cmd.arg("commit").arg(&container).arg(&tag);
        if let Some(host) = docker_host {
            cmd.env("DOCKER_HOST", host);
        }
        let out = cmd
            .output()
            .map_err(|e| format!("Failed to run docker: {}", e))?;
        if !out.status.success() {
            return Err(format!(
                "docker commit failed (exit {}): {}",
                out.status.code().unwrap_or(-1),
                String::from_utf8_lossy(&out.stderr).trim()
            ));
        }
        Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

// ---------------------------------------------------------------------------
// OS Image management commands
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct OsImageDto {
    id: String,
    name: String,
    version: String,
    arch: String,
    size_bytes: u64,
    status: String,
    default_cmdline: String,
}

#[derive(Debug, Serialize)]
pub struct OsImageDownloadProgressDto {
    image_id: String,
    current_file: String,
    bytes_downloaded: u64,
    bytes_total: u64,
    done: bool,
    error: Option<String>,
}

#[tauri::command]
fn image_catalog() -> Vec<OsImageDto> {
    cargobay_core::images::list_available_images()
        .into_iter()
        .map(|e| OsImageDto {
            id: e.id,
            name: e.name,
            version: e.version,
            arch: e.arch,
            size_bytes: e.size_bytes,
            status: match e.status {
                cargobay_core::images::ImageStatus::NotDownloaded => "not_downloaded".into(),
                cargobay_core::images::ImageStatus::Downloading => "downloading".into(),
                cargobay_core::images::ImageStatus::Ready => "ready".into(),
            },
            default_cmdline: e.default_cmdline,
        })
        .collect()
}

#[tauri::command]
async fn image_download_os(image_id: String) -> Result<(), String> {
    cargobay_core::images::download_image(&image_id, |_file, _downloaded, _total| {})
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
fn image_download_status(image_id: String) -> OsImageDownloadProgressDto {
    let p = cargobay_core::images::read_download_progress(&image_id);
    OsImageDownloadProgressDto {
        image_id: p.image_id,
        current_file: p.current_file,
        bytes_downloaded: p.bytes_downloaded,
        bytes_total: p.bytes_total,
        done: p.done,
        error: p.error,
    }
}

#[tauri::command]
fn image_delete_os(image_id: String) -> Result<(), String> {
    cargobay_core::images::delete_image(&image_id).map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------
// VM DTOs and commands
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct VmInfoDto {
    id: String,
    name: String,
    state: String,
    cpus: u32,
    memory_mb: u64,
    disk_gb: u64,
    rosetta_enabled: bool,
    mounts: Vec<SharedDirectoryDto>,
    port_forwards: Vec<PortForwardDto>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PortForwardDto {
    host_port: u16,
    guest_port: u16,
    protocol: String,
}

#[derive(Debug, Serialize)]
pub struct SharedDirectoryDto {
    tag: String,
    host_path: String,
    guest_path: String,
    read_only: bool,
}

impl From<cargobay_core::hypervisor::SharedDirectory> for SharedDirectoryDto {
    fn from(value: cargobay_core::hypervisor::SharedDirectory) -> Self {
        Self {
            tag: value.tag,
            host_path: value.host_path,
            guest_path: value.guest_path,
            read_only: value.read_only,
        }
    }
}

impl From<proto::SharedDirectory> for SharedDirectoryDto {
    fn from(value: proto::SharedDirectory) -> Self {
        Self {
            tag: value.tag,
            host_path: value.host_path,
            guest_path: value.guest_path,
            read_only: value.read_only,
        }
    }
}

fn vm_state_to_string(state: cargobay_core::hypervisor::VmState) -> String {
    match state {
        cargobay_core::hypervisor::VmState::Running => "running".into(),
        cargobay_core::hypervisor::VmState::Stopped => "stopped".into(),
        cargobay_core::hypervisor::VmState::Creating => "creating".into(),
    }
}

#[tauri::command]
async fn vm_list(state: State<'_, AppState>) -> Result<Vec<VmInfoDto>, String> {
    if let Ok(mut client) = connect_vm_service(&state.grpc_addr).await {
        let resp = client
            .list_v_ms(proto::ListVMsRequest {})
            .await
            .map_err(|e| e.to_string())?
            .into_inner();

        return Ok(resp
            .vms
            .into_iter()
            .map(|vm| VmInfoDto {
                id: vm.vm_id,
                name: vm.name,
                state: vm.status,
                cpus: vm.cpus,
                memory_mb: vm.memory_mb,
                disk_gb: vm.disk_gb,
                rosetta_enabled: vm.rosetta_enabled,
                mounts: vm
                    .shared_dirs
                    .into_iter()
                    .map(SharedDirectoryDto::from)
                    .collect(),
                port_forwards: vm
                    .port_forwards
                    .into_iter()
                    .map(|pf| PortForwardDto {
                        host_port: pf.host_port as u16,
                        guest_port: pf.guest_port as u16,
                        protocol: pf.protocol,
                    })
                    .collect(),
            })
            .collect());
    }

    let vms = state.hv.list_vms().map_err(|e| e.to_string())?;
    Ok(vms
        .into_iter()
        .map(|vm| VmInfoDto {
            id: vm.id,
            name: vm.name,
            state: vm_state_to_string(vm.state),
            cpus: vm.cpus,
            memory_mb: vm.memory_mb,
            disk_gb: vm.disk_gb,
            rosetta_enabled: vm.rosetta_enabled,
            mounts: vm
                .shared_dirs
                .into_iter()
                .map(SharedDirectoryDto::from)
                .collect(),
            port_forwards: vm
                .port_forwards
                .into_iter()
                .map(|pf| PortForwardDto {
                    host_port: pf.host_port,
                    guest_port: pf.guest_port,
                    protocol: pf.protocol,
                })
                .collect(),
        })
        .collect())
}

#[tauri::command]
async fn vm_create(
    state: State<'_, AppState>,
    name: String,
    cpus: u32,
    memory_mb: u64,
    disk_gb: u64,
    rosetta: bool,
    os_image: Option<String>,
) -> Result<String, String> {
    // Resolve image paths from the selected OS image.
    let (kernel_path, initrd_path, disk_path) = if let Some(ref img_id) = os_image {
        if !cargobay_core::images::is_image_ready(img_id) {
            return Err(format!("OS image '{}' is not downloaded yet", img_id));
        }
        let paths = cargobay_core::images::image_paths(img_id);
        (
            Some(paths.kernel_path.to_string_lossy().into_owned()),
            Some(paths.initrd_path.to_string_lossy().into_owned()),
            Some(paths.rootfs_path.to_string_lossy().into_owned()),
        )
    } else {
        (None, None, None)
    };

    if let Ok(mut client) = connect_vm_service(&state.grpc_addr).await {
        let resp = client
            .create_vm(proto::CreateVmRequest {
                name,
                cpus,
                memory_mb,
                disk_gb,
                rosetta,
                shared_dirs: vec![],
            })
            .await
            .map_err(|e| e.to_string())?
            .into_inner();
        return Ok(resp.vm_id);
    }

    use cargobay_core::hypervisor::VmConfig;
    let config = VmConfig {
        name,
        cpus,
        memory_mb,
        disk_gb,
        rosetta,
        shared_dirs: vec![],
        os_image,
        kernel_path,
        initrd_path,
        disk_path,
        port_forwards: vec![],
    };
    state.hv.create_vm(config).map_err(|e| e.to_string())
}

#[tauri::command]
async fn vm_start(state: State<'_, AppState>, id: String) -> Result<(), String> {
    if let Ok(mut client) = connect_vm_service(&state.grpc_addr).await {
        client
            .start_vm(proto::StartVmRequest { vm_id: id })
            .await
            .map_err(|e| e.to_string())?;
        return Ok(());
    }

    state.hv.start_vm(&id).map_err(|e| e.to_string())
}

#[tauri::command]
async fn vm_stop(state: State<'_, AppState>, id: String) -> Result<(), String> {
    if let Ok(mut client) = connect_vm_service(&state.grpc_addr).await {
        client
            .stop_vm(proto::StopVmRequest { vm_id: id })
            .await
            .map_err(|e| e.to_string())?;
        return Ok(());
    }

    state.hv.stop_vm(&id).map_err(|e| e.to_string())
}

#[tauri::command]
async fn vm_delete(state: State<'_, AppState>, id: String) -> Result<(), String> {
    if let Ok(mut client) = connect_vm_service(&state.grpc_addr).await {
        client
            .delete_vm(proto::DeleteVmRequest { vm_id: id })
            .await
            .map_err(|e| e.to_string())?;
        return Ok(());
    }

    state.hv.delete_vm(&id).map_err(|e| e.to_string())
}

#[tauri::command]
fn vm_login_cmd(
    name: String,
    user: String,
    host: String,
    port: Option<u16>,
) -> Result<String, String> {
    let Some(port) = port else {
        return Err("VM login is not available yet. Specify an SSH port.".into());
    };
    Ok(format!("ssh {}@{} -p {}\n# VM: {}", user, host, port, name))
}

#[tauri::command]
async fn vm_console(
    state: State<'_, AppState>,
    id: String,
    offset: Option<u64>,
) -> Result<(String, u64), String> {
    let off = offset.unwrap_or(0);

    if let Ok(mut client) = connect_vm_service(&state.grpc_addr).await {
        let resp = client
            .get_vm_console(proto::GetVmConsoleRequest {
                vm_id: id.clone(),
                offset: off,
            })
            .await
            .map_err(|e| e.to_string())?
            .into_inner();
        return Ok((resp.data, resp.new_offset));
    }

    state
        .hv
        .read_vm_console(&id, off)
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn vm_mount_add(
    state: State<'_, AppState>,
    vm: String,
    tag: String,
    host_path: String,
    guest_path: String,
    readonly: bool,
) -> Result<(), String> {
    if let Ok(mut client) = connect_vm_service(&state.grpc_addr).await {
        client
            .mount_virtio_fs(proto::MountVirtioFsRequest {
                vm_id: vm,
                share: Some(proto::SharedDirectory {
                    tag,
                    host_path,
                    guest_path,
                    read_only: readonly,
                }),
            })
            .await
            .map_err(|e| e.to_string())?;
        return Ok(());
    }

    use cargobay_core::hypervisor::SharedDirectory;
    let share = SharedDirectory {
        tag,
        host_path,
        guest_path,
        read_only: readonly,
    };
    state
        .hv
        .mount_virtiofs(&vm, &share)
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn vm_mount_remove(
    state: State<'_, AppState>,
    vm: String,
    tag: String,
) -> Result<(), String> {
    if let Ok(mut client) = connect_vm_service(&state.grpc_addr).await {
        client
            .unmount_virtio_fs(proto::UnmountVirtioFsRequest { vm_id: vm, tag })
            .await
            .map_err(|e| e.to_string())?;
        return Ok(());
    }

    state
        .hv
        .unmount_virtiofs(&vm, &tag)
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn vm_mount_list(
    state: State<'_, AppState>,
    vm: String,
) -> Result<Vec<SharedDirectoryDto>, String> {
    if let Ok(mut client) = connect_vm_service(&state.grpc_addr).await {
        let resp = client
            .list_virtio_fs_mounts(proto::ListVirtioFsMountsRequest { vm_id: vm })
            .await
            .map_err(|e| e.to_string())?
            .into_inner();
        return Ok(resp
            .mounts
            .into_iter()
            .map(SharedDirectoryDto::from)
            .collect());
    }

    let mounts = state
        .hv
        .list_virtiofs_mounts(&vm)
        .map_err(|e| e.to_string())?;
    Ok(mounts.into_iter().map(SharedDirectoryDto::from).collect())
}

#[tauri::command]
async fn vm_port_forward_add(
    state: State<'_, AppState>,
    vm_id: String,
    host_port: u16,
    guest_port: u16,
    protocol: String,
) -> Result<(), String> {
    let proto_str = if protocol.is_empty() {
        "tcp".to_string()
    } else {
        protocol.clone()
    };

    if let Ok(mut client) = connect_vm_service(&state.grpc_addr).await {
        client
            .add_port_forward(proto::AddPortForwardRequest {
                vm_id,
                host_port: host_port as u32,
                guest_port: guest_port as u32,
                protocol: proto_str,
            })
            .await
            .map_err(|e| e.to_string())?;
        return Ok(());
    }

    let pf = cargobay_core::hypervisor::PortForward {
        host_port,
        guest_port,
        protocol: proto_str,
    };
    state
        .hv
        .add_port_forward(&vm_id, &pf)
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn vm_port_forward_remove(
    state: State<'_, AppState>,
    vm_id: String,
    host_port: u16,
) -> Result<(), String> {
    if let Ok(mut client) = connect_vm_service(&state.grpc_addr).await {
        client
            .remove_port_forward(proto::RemovePortForwardRequest {
                vm_id,
                host_port: host_port as u32,
            })
            .await
            .map_err(|e| e.to_string())?;
        return Ok(());
    }

    state
        .hv
        .remove_port_forward(&vm_id, host_port)
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn vm_port_forward_list(
    state: State<'_, AppState>,
    vm_id: String,
) -> Result<Vec<PortForwardDto>, String> {
    if let Ok(mut client) = connect_vm_service(&state.grpc_addr).await {
        let resp = client
            .list_port_forwards(proto::ListPortForwardsRequest { vm_id })
            .await
            .map_err(|e| e.to_string())?
            .into_inner();
        return Ok(resp
            .forwards
            .into_iter()
            .map(|pf| PortForwardDto {
                host_port: pf.host_port as u16,
                guest_port: pf.guest_port as u16,
                protocol: pf.protocol,
            })
            .collect());
    }

    let forwards = state
        .hv
        .list_port_forwards(&vm_id)
        .map_err(|e| e.to_string())?;
    Ok(forwards
        .into_iter()
        .map(|pf| PortForwardDto {
            host_port: pf.host_port,
            guest_port: pf.guest_port,
            protocol: pf.protocol,
        })
        .collect())
}

#[derive(Debug, Serialize)]
pub struct VmStatsDto {
    cpu_percent: f64,
    memory_usage_mb: u64,
    disk_usage_gb: u64,
}

#[tauri::command]
async fn vm_stats(state: State<'_, AppState>, id: String) -> Result<VmStatsDto, String> {
    if let Ok(mut client) = connect_vm_service(&state.grpc_addr).await {
        let resp = client
            .get_vm_stats(proto::GetVmStatsRequest { vm_id: id })
            .await
            .map_err(|e| e.to_string())?
            .into_inner();

        return Ok(VmStatsDto {
            cpu_percent: resp.cpu_percent,
            memory_usage_mb: resp.memory_usage_mb,
            disk_usage_gb: resp.disk_usage_gb,
        });
    }

    // Fallback: stub stats for local hypervisor
    let vms = state.hv.list_vms().map_err(|e| e.to_string())?;
    let vm = vms
        .into_iter()
        .find(|v| v.id == id || v.name == id)
        .ok_or_else(|| format!("VM not found: {}", id))?;

    Ok(VmStatsDto {
        cpu_percent: 0.0,
        memory_usage_mb: 0,
        disk_usage_gb: vm.disk_gb,
    })
}

async fn docker_pull_image(docker: &Docker, reference: &str) -> Result<(), String> {
    let (from_image, tag) = split_image_reference(reference);
    let opts = CreateImageOptions {
        from_image,
        tag,
        ..Default::default()
    };

    let mut stream = docker.create_image(Some(opts), None, None);
    while let Some(_progress) = stream.try_next().await.map_err(|e| e.to_string())? {}
    Ok(())
}

fn split_image_reference(reference: &str) -> (String, String) {
    let no_digest = reference.split('@').next().unwrap_or(reference);
    let last_slash = no_digest.rfind('/').unwrap_or(0);
    let last_colon = no_digest.rfind(':');

    if let Some(colon_idx) = last_colon {
        if colon_idx > last_slash {
            let image = &no_digest[..colon_idx];
            let tag = &no_digest[(colon_idx + 1)..];
            if !image.is_empty() && !tag.is_empty() {
                return (image.to_string(), tag.to_string());
            }
        }
    }

    (no_digest.to_string(), "latest".to_string())
}

async fn search_dockerhub(
    client: &reqwest::Client,
    query: &str,
    limit: usize,
) -> Result<Vec<ImageSearchResult>, String> {
    let mut url = reqwest::Url::parse("https://hub.docker.com/v2/search/repositories/")
        .map_err(|e| e.to_string())?;
    url.query_pairs_mut()
        .append_pair("query", query)
        .append_pair("page_size", &limit.to_string());

    let resp = client.get(url).send().await.map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(format!("Docker Hub search failed: HTTP {}", resp.status()));
    }

    let data: DockerHubSearchResponse = resp.json().await.map_err(|e| e.to_string())?;
    let mut out = Vec::new();

    for r in data.results.into_iter().take(limit) {
        let ns = r.namespace.unwrap_or_else(|| "library".to_string());
        let name = if ns == "library" {
            r.name.clone()
        } else {
            format!("{}/{}", ns, r.name)
        };

        out.push(ImageSearchResult {
            source: "dockerhub".into(),
            reference: name,
            description: r.description.unwrap_or_default(),
            stars: r.star_count,
            pulls: r.pull_count,
            official: r.is_official.unwrap_or(false),
        });
    }

    Ok(out)
}

async fn search_quay(
    client: &reqwest::Client,
    query: &str,
    limit: usize,
) -> Result<Vec<ImageSearchResult>, String> {
    let mut url = reqwest::Url::parse("https://quay.io/api/v1/find/repositories")
        .map_err(|e| e.to_string())?;
    url.query_pairs_mut().append_pair("query", query);

    let resp = client.get(url).send().await.map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(format!("Quay search failed: HTTP {}", resp.status()));
    }

    let json: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
    let results = json
        .get("results")
        .and_then(|v| v.as_array())
        .or_else(|| json.get("repositories").and_then(|v| v.as_array()))
        .cloned()
        .unwrap_or_default();

    let mut out = Vec::new();
    for item in results.into_iter().take(limit) {
        let full_name = item
            .get("name")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .or_else(|| {
                let ns = item
                    .get("namespace")
                    .or_else(|| item.get("namespace_name"))
                    .and_then(|v| v.as_str())?;
                let name = item
                    .get("repo_name")
                    .or_else(|| item.get("name"))
                    .and_then(|v| v.as_str())?;
                Some(format!("{}/{}", ns, name))
            })
            .unwrap_or_else(|| "<unknown>".to_string());

        let desc = item
            .get("description")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let stars = item
            .get("stars")
            .or_else(|| item.get("star_count"))
            .and_then(|v| v.as_u64());

        out.push(ImageSearchResult {
            source: "quay".into(),
            reference: format!("quay.io/{}", full_name),
            description: desc,
            stars,
            pulls: None,
            official: false,
        });
    }

    Ok(out)
}

fn parse_registry_reference(reference: &str) -> Option<(String, String)> {
    let no_digest = reference.split('@').next().unwrap_or(reference);
    let no_tag = {
        let last_slash = no_digest.rfind('/').unwrap_or(0);
        if let Some(colon_idx) = no_digest.rfind(':') {
            if colon_idx > last_slash {
                &no_digest[..colon_idx]
            } else {
                no_digest
            }
        } else {
            no_digest
        }
    };

    let (first, rest) = no_tag.split_once('/')?;
    if !(first.contains('.') || first.contains(':') || first == "localhost") {
        return None;
    }
    if rest.is_empty() {
        return None;
    }
    Some((first.to_string(), rest.to_string()))
}

async fn list_registry_tags(
    client: &reqwest::Client,
    registry: &str,
    repository: &str,
    limit: usize,
) -> Result<Vec<String>, String> {
    let url = format!("https://{}/v2/{}/tags/list", registry, repository);
    let mut resp = client.get(&url).send().await.map_err(|e| e.to_string())?;

    if resp.status() == reqwest::StatusCode::UNAUTHORIZED {
        let auth = resp
            .headers()
            .get(WWW_AUTHENTICATE)
            .and_then(|v| v.to_str().ok())
            .ok_or_else(|| "Registry requires auth (missing WWW-Authenticate)".to_string())?;

        let fallback_scope = format!("repository:{}:pull", repository);
        let token = fetch_bearer_token(client, auth, Some(&fallback_scope)).await?;

        resp = client
            .get(&url)
            .bearer_auth(token)
            .send()
            .await
            .map_err(|e| e.to_string())?;
    }

    if !resp.status().is_success() {
        return Err(format!(
            "Failed to list tags for {}/{}: HTTP {}",
            registry,
            repository,
            resp.status()
        ));
    }

    let data: RegistryTagsResponse = resp.json().await.map_err(|e| e.to_string())?;
    let mut tags = data.tags.unwrap_or_default();
    tags.sort();
    tags.truncate(limit);
    Ok(tags)
}

async fn fetch_bearer_token(
    client: &reqwest::Client,
    auth_header: &str,
    fallback_scope: Option<&str>,
) -> Result<String, String> {
    let params = parse_bearer_auth_params(auth_header)
        .ok_or_else(|| format!("Unsupported WWW-Authenticate header: {}", auth_header))?;

    let realm = params
        .get("realm")
        .ok_or_else(|| "WWW-Authenticate missing realm".to_string())?;

    let service = params.get("service").map(String::as_str);
    let scope = params.get("scope").map(String::as_str).or(fallback_scope);

    let mut url = reqwest::Url::parse(realm).map_err(|e| e.to_string())?;
    {
        let mut qp = url.query_pairs_mut();
        if let Some(s) = service {
            qp.append_pair("service", s);
        }
        if let Some(s) = scope {
            qp.append_pair("scope", s);
        }
    }

    let resp = client.get(url).send().await.map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(format!("Token request failed: HTTP {}", resp.status()));
    }

    let token: RegistryTokenResponse = resp.json().await.map_err(|e| e.to_string())?;
    token
        .token
        .or(token.access_token)
        .ok_or_else(|| "Token response missing token".to_string())
}

fn parse_bearer_auth_params(header_value: &str) -> Option<HashMap<String, String>> {
    let header_value = header_value.trim();
    let mut parts = header_value.splitn(2, ' ');
    let scheme = parts.next()?.trim();
    if !scheme.eq_ignore_ascii_case("bearer") {
        return None;
    }
    let rest = parts.next()?.trim();

    let mut out = HashMap::new();
    for part in rest.split(',') {
        let part = part.trim();
        let mut kv = part.splitn(2, '=');
        let key = kv.next()?.trim();
        let val = kv.next()?.trim().trim_matches('"');
        out.insert(key.to_string(), val.to_string());
    }
    Some(out)
}

/// Build and return the system tray menu.
///
/// Menu layout:
///   CargoBay            (disabled title)
///   ─────────────────
///   Dashboard           → focus/show the main window
///   Containers (N running)
///   VMs (N running)
///   ─────────────────
///   Quit CargoBay       → exit the application
fn build_tray_menu(
    app: &tauri::AppHandle,
    running_containers: usize,
    running_vms: usize,
) -> Result<tauri::menu::Menu<tauri::Wry>, Box<dyn std::error::Error>> {
    let title_item = MenuItemBuilder::with_id("title", "CargoBay")
        .enabled(false)
        .build(app)?;

    let sep1 = PredefinedMenuItem::separator(app)?;

    let dashboard_item = MenuItemBuilder::with_id("dashboard", "Dashboard").build(app)?;

    let containers_label = format!("Containers ({} running)", running_containers);
    let containers_item =
        MenuItemBuilder::with_id("containers", containers_label)
            .enabled(false)
            .build(app)?;

    let vms_label = format!("VMs ({} running)", running_vms);
    let vms_item = MenuItemBuilder::with_id("vms", vms_label)
        .enabled(false)
        .build(app)?;

    let sep2 = PredefinedMenuItem::separator(app)?;

    let quit_item = MenuItemBuilder::with_id("quit", "Quit CargoBay").build(app)?;

    let menu = MenuBuilder::new(app)
        .item(&title_item)
        .item(&sep1)
        .item(&dashboard_item)
        .item(&containers_item)
        .item(&vms_item)
        .item(&sep2)
        .item(&quit_item)
        .build()?;

    Ok(menu)
}

/// Count containers with state == "running" via the Docker API.
async fn count_running_containers() -> usize {
    let Ok(docker) = connect_docker() else {
        return 0;
    };
    let opts = ListContainersOptions::<String> {
        all: true,
        ..Default::default()
    };
    let Ok(containers) = docker.list_containers(Some(opts)).await else {
        return 0;
    };
    containers
        .iter()
        .filter(|c| {
            c.state
                .as_deref()
                .map(|s| s == "running")
                .unwrap_or(false)
        })
        .count()
}

/// Count VMs with state == "running" via the gRPC daemon (or local hypervisor).
async fn count_running_vms(app_state: &AppState) -> usize {
    if let Ok(mut client) = connect_vm_service(&app_state.grpc_addr).await {
        if let Ok(resp) = client
            .list_v_ms(proto::ListVMsRequest {})
            .await
        {
            return resp
                .into_inner()
                .vms
                .iter()
                .filter(|vm| vm.status == "running")
                .count();
        }
    }
    // Fallback to local hypervisor
    if let Ok(vms) = app_state.hv.list_vms() {
        return vms
            .iter()
            .filter(|vm| matches!(vm.state, cargobay_core::hypervisor::VmState::Running))
            .count();
    }
    0
}

/// Refresh the tray menu with up-to-date container/VM counts.
fn refresh_tray_menu(app: &tauri::AppHandle) {
    let app_handle = app.clone();
    tauri::async_runtime::spawn(async move {
        let running_containers = count_running_containers().await;
        let running_vms = {
            let state = app_handle.state::<AppState>();
            count_running_vms(&state).await
        };

        if let Some(tray) = app_handle.tray_by_id("main-tray") {
            match build_tray_menu(&app_handle, running_containers, running_vms) {
                Ok(menu) => {
                    if let Err(e) = tray.set_menu(Some(menu)) {
                        error!("Failed to update tray menu: {}", e);
                    }
                }
                Err(e) => {
                    error!("Failed to build tray menu: {}", e);
                }
            }
        }
    });
}

pub fn run() {
    cargobay_core::logging::init();
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState {
            hv: cargobay_core::create_hypervisor(),
            grpc_addr: grpc_addr(),
            daemon: Mutex::new(None),
        })
        .setup(|app| {
            // ── Daemon startup ──────────────────────────────────────────
            let state = app.state::<AppState>();
            let grpc_addr = state.grpc_addr.clone();
            let daemon_up = tauri::async_runtime::block_on(async {
                connect_vm_service(&grpc_addr).await.is_ok()
            });

            if daemon_up {
                info!("CargoBay daemon already running at {}", grpc_addr);
            } else {
                info!("CargoBay daemon not detected at {}, starting it", grpc_addr);
                match spawn_daemon(&grpc_addr) {
                    Ok(child) => {
                        if let Ok(mut guard) = state.daemon.lock() {
                            *guard = Some(child);
                        }

                        let ready = tauri::async_runtime::block_on(async {
                            wait_for_daemon(&grpc_addr, Duration::from_secs(5)).await
                        });
                        if ready {
                            info!("CargoBay daemon is ready at {}", grpc_addr);
                        } else {
                            warn!(
                                "CargoBay daemon did not become ready in time ({}), falling back to local hypervisor",
                                grpc_addr
                            );
                        }
                    }
                    Err(e) => {
                        error!("Failed to start CargoBay daemon: {}", e);
                    }
                }
            }

            // ── System tray ─────────────────────────────────────────────
            let app_handle = app.handle().clone();
            let menu = build_tray_menu(&app_handle, 0, 0)?;

            TrayIconBuilder::with_id("main-tray")
                .icon(app.default_window_icon().cloned().unwrap())
                .icon_as_template(true)
                .tooltip("CargoBay")
                .menu(&menu)
                .show_menu_on_left_click(true)
                .on_menu_event(move |app, event| match event.id().as_ref() {
                    "dashboard" => {
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.unminimize();
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                    }
                    "quit" => {
                        app.exit(0);
                    }
                    _ => {}
                })
                .build(app)?;

            // Initial tray menu refresh (to get real counts)
            refresh_tray_menu(&app_handle);

            Ok(())
        })
        // ── Hide window on close instead of quitting ────────────────
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();

                // Refresh the tray menu so counts are up-to-date when the
                // user re-opens via the tray.
                refresh_tray_menu(window.app_handle());
            }
        })
        .invoke_handler(tauri::generate_handler![
            list_containers,
            stop_container,
            start_container,
            remove_container,
            docker_run,
            container_login_cmd,
            container_logs,
            container_exec,
            container_exec_interactive_cmd,
            container_stats,
            image_search,
            image_tags,
            image_load,
            image_push,
            image_pack_container,
            image_catalog,
            image_download_os,
            image_download_status,
            image_delete_os,
            vm_list,
            vm_create,
            vm_start,
            vm_stop,
            vm_delete,
            vm_login_cmd,
            vm_console,
            vm_mount_add,
            vm_mount_remove,
            vm_mount_list,
            vm_port_forward_add,
            vm_port_forward_remove,
            vm_port_forward_list,
            vm_stats
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
