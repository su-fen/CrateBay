use bollard::container::{
    Config, CreateContainerOptions, InspectContainerOptions, ListContainersOptions, LogsOptions,
    RemoveContainerOptions, StartContainerOptions, StatsOptions, StopContainerOptions,
};
use bollard::exec::{CreateExecOptions, StartExecResults};
use bollard::image::{CreateImageOptions, ListImagesOptions, RemoveImageOptions, TagImageOptions};
use bollard::service::HostConfig;
use bollard::volume::{CreateVolumeOptions, ListVolumesOptions};
#[cfg_attr(mobile, tauri::mobile_entry_point)]
use bollard::Docker;
use futures_util::stream::TryStreamExt;
use keyring::Entry;
use reqwest::header::WWW_AUTHENTICATE;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::Duration;
use tauri::async_runtime::JoinHandle;
use tauri::menu::{MenuBuilder, MenuItemBuilder, PredefinedMenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{Emitter, Manager, State, WindowEvent};
use tonic::transport::Channel;
use tracing::{error, info, warn};

use cratebay_core::proto;
use cratebay_core::proto::vm_service_client::VmServiceClient;
use cratebay_core::validation;

pub struct AppState {
    hv: Box<dyn cratebay_core::hypervisor::Hypervisor>,
    grpc_addr: String,
    daemon: Mutex<Option<Child>>,
    daemon_ready: Mutex<bool>,
    log_stream_handles: Mutex<HashMap<String, JoinHandle<()>>>,
}

impl AppState {
    /// Ensure the daemon process is running (lazy start on first VM operation).
    /// Subsequent calls are no-ops once the daemon is confirmed ready.
    async fn ensure_daemon(&self) {
        // Fast-path: already initialised.
        {
            let guard = self.daemon_ready.lock().unwrap_or_else(|e| e.into_inner());
            if *guard {
                return;
            }
        }

        // Check if daemon is already running externally.
        if connect_vm_service(&self.grpc_addr).await.is_ok() {
            info!("CrateBay daemon already running at {}", self.grpc_addr);
            let mut guard = self.daemon_ready.lock().unwrap_or_else(|e| e.into_inner());
            *guard = true;
            return;
        }

        // Spawn it.
        info!(
            "CrateBay daemon not detected at {}, starting it",
            self.grpc_addr
        );
        match spawn_daemon(&self.grpc_addr) {
            Ok(child) => {
                if let Ok(mut dg) = self.daemon.lock() {
                    *dg = Some(child);
                }

                let ready = wait_for_daemon(&self.grpc_addr, Duration::from_secs(5)).await;
                if ready {
                    info!("CrateBay daemon is ready at {}", self.grpc_addr);
                    let mut guard = self.daemon_ready.lock().unwrap_or_else(|e| e.into_inner());
                    *guard = true;
                } else {
                    warn!(
                        "CrateBay daemon did not become ready in time ({}), \
                         falling back to local hypervisor",
                        self.grpc_addr
                    );
                }
            }
            Err(e) => {
                error!("Failed to start CrateBay daemon: {}", e);
            }
        }
    }
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
        Err("No Docker socket found. Set DOCKER_HOST or start a Docker-compatible runtime.".into())
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
        Err(
            "No Docker named pipe found. Set DOCKER_HOST or start a Docker-compatible runtime."
                .into(),
        )
    }

    #[cfg(not(any(unix, windows)))]
    {
        Docker::connect_with_local_defaults()
            .map_err(|e| format!("Failed to connect to Docker: {}", e))
    }
}

fn runtime_setup_path() -> String {
    let mut items = std::env::var("PATH")
        .unwrap_or_default()
        .split(':')
        .map(|s| s.to_string())
        .collect::<Vec<_>>();
    for extra in ["/opt/homebrew/bin", "/usr/local/bin"] {
        if !items.iter().any(|item| item == extra) {
            items.push(extra.to_string());
        }
    }
    items.join(":")
}

fn runtime_setup_command_exists(command: &str) -> bool {
    let output = Command::new("which")
        .arg(command)
        .env("PATH", runtime_setup_path())
        .output();
    matches!(output, Ok(out) if out.status.success())
}

fn runtime_setup_run(command: &str, args: &[&str]) -> Result<std::process::Output, String> {
    Command::new(command)
        .args(args)
        .env("PATH", runtime_setup_path())
        .output()
        .map_err(|e| format!("Failed to run command '{}': {}", command, e))
}

fn truncate_message(text: &str, max_chars: usize) -> String {
    let mut out = String::new();
    let mut count = 0usize;
    for ch in text.chars() {
        if count >= max_chars {
            out.push_str("...");
            break;
        }
        out.push(ch);
        count += 1;
    }
    out
}

fn runtime_setup_output_summary(output: &std::process::Output) -> String {
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let base = if !stderr.is_empty() {
        stderr
    } else if !stdout.is_empty() {
        stdout
    } else {
        "no command output".to_string()
    };
    truncate_message(&base.replace('\n', " "), 320)
}

#[cfg(target_os = "macos")]
fn docker_runtime_quick_setup_macos() -> Result<String, String> {
    if connect_docker().is_ok() {
        return Ok("Docker runtime is already available.".to_string());
    }

    if !runtime_setup_command_exists("docker") {
        if !runtime_setup_command_exists("brew") {
            return Err(
                "Docker CLI is missing and Homebrew is not installed. Install Homebrew from https://brew.sh, then run `brew install docker`."
                    .to_string(),
            );
        }
        let install = runtime_setup_run("brew", &["install", "docker"])?;
        if !install.status.success() {
            return Err(format!(
                "Failed to install Docker CLI via Homebrew: {}",
                runtime_setup_output_summary(&install)
            ));
        }
    } else {
        let version = runtime_setup_run("docker", &["version"]);
        if let Ok(output) = version {
            if !output.status.success() {
                warn!(
                    "docker version check failed during runtime setup: {}",
                    runtime_setup_output_summary(&output)
                );
            }
        }
    }

    if connect_docker().is_ok() {
        Ok("Docker runtime is reachable. Containers and Volumes are ready.".to_string())
    } else {
        Err(
            "Docker CLI is installed, but no Docker-compatible daemon socket is reachable. Start your runtime or set DOCKER_HOST, then refresh."
                .to_string(),
        )
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
    std::env::var("CRATEBAY_GRPC_ADDR").unwrap_or_else(|_| "127.0.0.1:50051".into())
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
        "cratebay-daemon.exe"
    } else {
        "cratebay-daemon"
    }
}

fn spawn_daemon(grpc_addr: &str) -> Result<Child, String> {
    let mut tried: Vec<String> = Vec::new();

    if let Ok(path) = std::env::var("CRATEBAY_DAEMON_PATH") {
        let mut cmd = Command::new(&path);
        cmd.env("CRATEBAY_GRPC_ADDR", grpc_addr);
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
                "Failed to spawn daemon from CRATEBAY_DAEMON_PATH ({}): {}",
                path, e
            )
        });
    }

    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let candidate = dir.join(daemon_bin_name());
            if candidate.is_file() {
                let mut cmd = Command::new(&candidate);
                cmd.env("CRATEBAY_GRPC_ADDR", grpc_addr);
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

    tried.push("cratebay-daemon (PATH)".into());
    let mut cmd = Command::new("cratebay-daemon");
    cmd.env("CRATEBAY_GRPC_ADDR", grpc_addr);
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

fn format_published_ports(mut pairs: Vec<(u16, u16)>) -> String {
    pairs.sort_unstable();
    pairs.dedup();
    pairs
        .into_iter()
        .map(|(public, private)| format!("{}:{}", public, private))
        .collect::<Vec<_>>()
        .join(", ")
}

#[cfg(test)]
mod tests {
    use super::format_published_ports;

    #[test]
    fn format_published_ports_sorts_by_public_then_private() {
        let out = format_published_ports(vec![(443, 443), (80, 8080), (80, 80), (8080, 80)]);
        assert_eq!(out, "80:80, 80:8080, 443:443, 8080:80");
    }

    #[test]
    fn format_published_ports_empty_is_empty() {
        assert_eq!(format_published_ports(vec![]), "");
    }
}

#[derive(Serialize)]
pub struct VolumeInfo {
    name: String,
    driver: String,
    mountpoint: String,
    created_at: String,
    labels: HashMap<String, String>,
    options: HashMap<String, String>,
    scope: String,
}

#[derive(Serialize)]
pub struct LocalImageInfo {
    id: String,
    repo_tags: Vec<String>,
    size_bytes: u64,
    size_human: String,
    created: i64,
}

#[derive(Serialize)]
pub struct ImageInspectInfo {
    id: String,
    repo_tags: Vec<String>,
    size_bytes: u64,
    created: String,
    architecture: String,
    os: String,
    docker_version: String,
    layers: usize,
}

fn format_bytes_human(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = 1024 * KB;
    const GB: u64 = 1024 * MB;
    if bytes >= GB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
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
            let published = c
                .ports
                .unwrap_or_default()
                .into_iter()
                .filter_map(|p| p.public_port.map(|public| (public, p.private_port)))
                .collect::<Vec<_>>();
            let ports = format_published_ports(published);

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
    env: Option<Vec<String>>,
) -> Result<RunContainerResult, String> {
    validation::validate_image_reference(&image)
        .map_err(|e| format!("Invalid image reference '{}': {}", image, e))?;
    if let Some(ref n) = name {
        validation::validate_container_name(n)
            .map_err(|e| format!("Invalid container name '{}': {}", n, e))?;
    }

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
        env: env.filter(|v| !v.is_empty()),
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
async fn container_logs_stream(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    id: String,
    timestamps: bool,
) -> Result<(), String> {
    // Stop any existing stream for this container
    if let Ok(mut handles) = state.log_stream_handles.lock() {
        if let Some(handle) = handles.remove(&id) {
            handle.abort();
        }
    }

    let docker = connect_docker()?;
    let container_id = id.clone();

    let opts = LogsOptions::<String> {
        follow: true,
        stdout: true,
        stderr: true,
        timestamps,
        tail: "100".to_string(),
        ..Default::default()
    };

    let mut stream = docker.logs(&container_id, Some(opts));
    let emit_id = id.clone();
    let handle = tauri::async_runtime::spawn(async move {
        while let Some(chunk) = stream.try_next().await.unwrap_or(None) {
            let payload = serde_json::json!({
                "container_id": emit_id,
                "data": chunk.to_string()
            });
            if app.emit("container-log", payload).is_err() {
                break;
            }
        }
        let _ = app.emit("container-log-end", &emit_id);
    });

    if let Ok(mut handles) = state.log_stream_handles.lock() {
        handles.insert(id, handle);
    }

    Ok(())
}

#[tauri::command]
async fn container_logs_stream_stop(state: State<'_, AppState>, id: String) -> Result<(), String> {
    if let Ok(mut handles) = state.log_stream_handles.lock() {
        if let Some(handle) = handles.remove(&id) {
            handle.abort();
        }
    }
    Ok(())
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
        format!(
            "DOCKER_HOST={} docker exec -it {} /bin/sh",
            host, container_id
        )
    } else {
        format!("docker exec -it {} /bin/sh", container_id)
    }
}

#[derive(Debug, Serialize)]
pub struct EnvVar {
    key: String,
    value: String,
}

#[tauri::command]
async fn container_env(id: String) -> Result<Vec<EnvVar>, String> {
    let docker = connect_docker()?;

    let inspect = docker
        .inspect_container(&id, None::<InspectContainerOptions>)
        .await
        .map_err(|e| format!("Failed to inspect container {}: {}", id, e))?;

    let env_list = inspect.config.and_then(|c| c.env).unwrap_or_default();

    Ok(env_list
        .into_iter()
        .map(|entry| {
            if let Some((k, v)) = entry.split_once('=') {
                EnvVar {
                    key: k.to_string(),
                    value: v.to_string(),
                }
            } else {
                EnvVar {
                    key: entry,
                    value: String::new(),
                }
            }
        })
        .collect())
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
        let num_cpus = stats.cpu_stats.online_cpus.unwrap_or(1) as f64;

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
    #[serde(alias = "repo_name")]
    name: String,
    #[serde(alias = "repo_owner")]
    namespace: Option<String>,
    #[serde(alias = "short_description")]
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
        .user_agent("CrateBay/1.0.0 (+https://github.com/coder-hhx/CrateBay)")
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
    cratebay_core::images::list_available_images()
        .into_iter()
        .map(|e| OsImageDto {
            id: e.id,
            name: e.name,
            version: e.version,
            arch: e.arch,
            size_bytes: e.size_bytes,
            status: match e.status {
                cratebay_core::images::ImageStatus::NotDownloaded => "not_downloaded".into(),
                cratebay_core::images::ImageStatus::Downloading => "downloading".into(),
                cratebay_core::images::ImageStatus::Ready => "ready".into(),
            },
            default_cmdline: e.default_cmdline,
        })
        .collect()
}

#[tauri::command]
async fn image_download_os(image_id: String) -> Result<(), String> {
    cratebay_core::images::download_image(&image_id, |_file, _downloaded, _total| {})
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
fn image_download_status(image_id: String) -> OsImageDownloadProgressDto {
    let p = cratebay_core::images::read_download_progress(&image_id);
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
    cratebay_core::images::delete_image(&image_id).map_err(|e| e.to_string())
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
    os_image: Option<String>,
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

impl From<cratebay_core::hypervisor::SharedDirectory> for SharedDirectoryDto {
    fn from(value: cratebay_core::hypervisor::SharedDirectory) -> Self {
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

fn vm_state_to_string(state: cratebay_core::hypervisor::VmState) -> String {
    match state {
        cratebay_core::hypervisor::VmState::Running => "running".into(),
        cratebay_core::hypervisor::VmState::Stopped => "stopped".into(),
        cratebay_core::hypervisor::VmState::Creating => "creating".into(),
    }
}

async fn vm_list_inner(state: &AppState) -> Result<Vec<VmInfoDto>, String> {
    state.ensure_daemon().await;
    if let Ok(mut client) = connect_vm_service(&state.grpc_addr).await {
        if let Ok(resp) = client.list_v_ms(proto::ListVMsRequest {}).await {
            let resp = resp.into_inner();
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
                    os_image: None, // gRPC path does not expose os_image yet
                })
                .collect());
        }
    }

    let vms = state.hv.list_vms().map_err(|e| e.to_string())?;
    Ok(vms
        .into_iter()
        .map(|vm| {
            let os_img = vm.os_image.clone();
            VmInfoDto {
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
                os_image: os_img,
            }
        })
        .collect())
}

async fn vm_start_inner(state: &AppState, id: String) -> Result<(), String> {
    state.ensure_daemon().await;
    if let Ok(mut client) = connect_vm_service(&state.grpc_addr).await {
        client
            .start_vm(proto::StartVmRequest { vm_id: id })
            .await
            .map_err(|e| e.to_string())?;
        return Ok(());
    }

    state.hv.start_vm(&id).map_err(|e| e.to_string())
}

async fn vm_stop_inner(state: &AppState, id: String) -> Result<(), String> {
    state.ensure_daemon().await;
    if let Ok(mut client) = connect_vm_service(&state.grpc_addr).await {
        client
            .stop_vm(proto::StopVmRequest { vm_id: id })
            .await
            .map_err(|e| e.to_string())?;
        return Ok(());
    }

    state.hv.stop_vm(&id).map_err(|e| e.to_string())
}

async fn vm_delete_inner(state: &AppState, id: String) -> Result<(), String> {
    state.ensure_daemon().await;
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
async fn vm_list(state: State<'_, AppState>) -> Result<Vec<VmInfoDto>, String> {
    vm_list_inner(state.inner()).await
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
    state.ensure_daemon().await;
    validation::validate_vm_name(&name)
        .map_err(|e| format!("Invalid VM name '{}': {}", name, e))?;

    // Resolve image paths from the selected OS image.
    let (kernel_path, initrd_path, disk_path) = if let Some(ref img_id) = os_image {
        if !cratebay_core::images::is_image_ready(img_id) {
            return Err(format!("OS image '{}' is not downloaded yet", img_id));
        }
        let paths = cratebay_core::images::image_paths(img_id);
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

    use cratebay_core::hypervisor::VmConfig;
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
    vm_start_inner(state.inner(), id).await
}

#[tauri::command]
async fn vm_stop(state: State<'_, AppState>, id: String) -> Result<(), String> {
    vm_stop_inner(state.inner(), id).await
}

#[tauri::command]
async fn vm_delete(state: State<'_, AppState>, id: String) -> Result<(), String> {
    vm_delete_inner(state.inner(), id).await
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
    state.ensure_daemon().await;
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
    state.ensure_daemon().await;
    validation::validate_mount_path(&host_path)
        .map_err(|e| format!("Invalid host path '{}': {}", host_path, e))?;
    validation::validate_mount_path(&guest_path)
        .map_err(|e| format!("Invalid guest path '{}': {}", guest_path, e))?;

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

    use cratebay_core::hypervisor::SharedDirectory;
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
    state.ensure_daemon().await;
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
    state.ensure_daemon().await;
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
    state.ensure_daemon().await;
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

    let pf = cratebay_core::hypervisor::PortForward {
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
    state.ensure_daemon().await;
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
    state.ensure_daemon().await;
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

#[tauri::command]
async fn volume_list() -> Result<Vec<VolumeInfo>, String> {
    let docker = connect_docker()?;
    let opts = ListVolumesOptions::<String> {
        ..Default::default()
    };
    let resp = docker
        .list_volumes(Some(opts))
        .await
        .map_err(|e| e.to_string())?;

    let volumes = resp.volumes.unwrap_or_default();
    let mut out: Vec<VolumeInfo> = volumes
        .into_iter()
        .map(|v| VolumeInfo {
            name: v.name,
            driver: v.driver,
            mountpoint: v.mountpoint,
            created_at: v.created_at.unwrap_or_default(),
            labels: v.labels,
            options: v.options,
            scope: v.scope.map(|s| format!("{:?}", s)).unwrap_or_default(),
        })
        .collect();
    // Docker doesn't guarantee ordering; keep it stable to avoid UI jitter on refresh.
    out.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(out)
}

#[derive(Debug, Serialize)]
pub struct VmStatsDto {
    cpu_percent: f64,
    memory_usage_mb: u64,
    disk_usage_gb: u64,
}

#[tauri::command]
async fn vm_stats(state: State<'_, AppState>, id: String) -> Result<VmStatsDto, String> {
    state.ensure_daemon().await;
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

#[tauri::command]
async fn volume_create(name: String, driver: Option<String>) -> Result<VolumeInfo, String> {
    let docker = connect_docker()?;
    let opts = CreateVolumeOptions {
        name: name.as_str(),
        driver: driver.as_deref().unwrap_or("local"),
        ..Default::default()
    };
    let v = docker
        .create_volume(opts)
        .await
        .map_err(|e| e.to_string())?;
    Ok(VolumeInfo {
        name: v.name,
        driver: v.driver,
        mountpoint: v.mountpoint,
        created_at: v.created_at.unwrap_or_default(),
        labels: v.labels,
        options: v.options,
        scope: v.scope.map(|s| format!("{:?}", s)).unwrap_or_default(),
    })
}

#[tauri::command]
async fn volume_inspect(name: String) -> Result<VolumeInfo, String> {
    let docker = connect_docker()?;
    let v = docker
        .inspect_volume(&name)
        .await
        .map_err(|e| e.to_string())?;
    Ok(VolumeInfo {
        name: v.name,
        driver: v.driver,
        mountpoint: v.mountpoint,
        created_at: v.created_at.unwrap_or_default(),
        labels: v.labels,
        options: v.options,
        scope: v.scope.map(|s| format!("{:?}", s)).unwrap_or_default(),
    })
}

#[tauri::command]
async fn volume_remove(name: String) -> Result<(), String> {
    let docker = connect_docker()?;
    docker
        .remove_volume(&name, None)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn image_list() -> Result<Vec<LocalImageInfo>, String> {
    let docker = connect_docker()?;
    let opts = ListImagesOptions::<String> {
        all: false,
        ..Default::default()
    };
    let images = docker
        .list_images(Some(opts))
        .await
        .map_err(|e| e.to_string())?;

    Ok(images
        .into_iter()
        .map(|img| {
            let full_id = img.id.clone();
            let short_id = if let Some(stripped) = full_id.strip_prefix("sha256:") {
                stripped.chars().take(12).collect::<String>()
            } else {
                full_id.chars().take(12).collect::<String>()
            };
            let size = img.size.max(0) as u64;
            LocalImageInfo {
                id: short_id,
                repo_tags: img.repo_tags,
                size_bytes: size,
                size_human: format_bytes_human(size),
                created: img.created,
            }
        })
        .collect())
}

#[tauri::command]
async fn image_remove(id: String) -> Result<(), String> {
    let docker = connect_docker()?;
    let opts = RemoveImageOptions {
        force: false,
        noprune: false,
    };
    docker
        .remove_image(&id, Some(opts), None)
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
async fn image_tag(source: String, repo: String, tag: String) -> Result<(), String> {
    let docker = connect_docker()?;
    let opts = TagImageOptions {
        repo: &repo,
        tag: &tag,
    };
    docker
        .tag_image(&source, Some(opts))
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn image_inspect(id: String) -> Result<ImageInspectInfo, String> {
    let docker = connect_docker()?;
    let detail = docker.inspect_image(&id).await.map_err(|e| e.to_string())?;

    let full_id = detail.id.clone().unwrap_or_default();
    let short_id = if let Some(stripped) = full_id.strip_prefix("sha256:") {
        stripped.chars().take(12).collect::<String>()
    } else {
        full_id.chars().take(12).collect::<String>()
    };
    let repo_tags = detail.repo_tags.clone().unwrap_or_default();
    let size = detail.size.unwrap_or(0).max(0) as u64;
    let created = detail.created.clone().unwrap_or_default();
    let architecture = detail.architecture.clone().unwrap_or_default();
    let os = detail.os.clone().unwrap_or_default();
    let docker_version = detail.docker_version.clone().unwrap_or_default();
    let layers = detail
        .root_fs
        .as_ref()
        .and_then(|r| r.layers.as_ref())
        .map(|l| l.len())
        .unwrap_or(0);

    Ok(ImageInspectInfo {
        id: short_id,
        repo_tags,
        size_bytes: size,
        created,
        architecture,
        os,
        docker_version,
        layers,
    })
}

// ── K3s cluster management commands ──────────────────────────────────

#[derive(Serialize)]
pub struct K3sStatusDto {
    installed: bool,
    running: bool,
    version: String,
    node_count: u32,
    kubeconfig_path: String,
}

#[tauri::command]
async fn k3s_status() -> Result<K3sStatusDto, String> {
    let status = cratebay_core::k3s::K3sManager::cluster_status().map_err(|e| e.to_string())?;
    let kubeconfig = cratebay_core::k3s::K3sManager::kubeconfig_path()
        .to_string_lossy()
        .to_string();
    Ok(K3sStatusDto {
        installed: status.installed,
        running: status.running,
        version: status.version,
        node_count: status.node_count,
        kubeconfig_path: kubeconfig,
    })
}

#[tauri::command]
async fn k3s_install() -> Result<(), String> {
    cratebay_core::k3s::K3sManager::install(None)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn k3s_start() -> Result<(), String> {
    let config = cratebay_core::k3s::K3sConfig::default();
    cratebay_core::k3s::K3sManager::start_cluster(&config).map_err(|e| e.to_string())
}

#[tauri::command]
async fn k3s_stop() -> Result<(), String> {
    cratebay_core::k3s::K3sManager::stop_cluster().map_err(|e| e.to_string())
}

#[tauri::command]
async fn k3s_uninstall() -> Result<(), String> {
    cratebay_core::k3s::K3sManager::uninstall().map_err(|e| e.to_string())
}

// ── Kubernetes dashboard commands ────────────────────────────────────

#[derive(Serialize)]
pub struct K8sPod {
    name: String,
    namespace: String,
    status: String,
    ready: String,
    restarts: u32,
    age: String,
}

#[derive(Serialize)]
pub struct K8sService {
    name: String,
    namespace: String,
    service_type: String,
    cluster_ip: String,
    ports: String,
}

#[derive(Serialize)]
pub struct K8sDeployment {
    name: String,
    namespace: String,
    ready: String,
    up_to_date: u32,
    available: u32,
    age: String,
}

fn k3s_kubeconfig_path() -> String {
    if let Ok(p) = std::env::var("KUBECONFIG") {
        return p;
    }
    let home = std::env::var("HOME").unwrap_or_default();
    // K3s default kubeconfig location
    let k3s_path = format!("{}/.kube/k3s.yaml", home);
    if Path::new(&k3s_path).exists() {
        return k3s_path;
    }
    // Fallback to default kubeconfig
    format!("{}/.kube/config", home)
}

fn run_kubectl(args: &[&str]) -> Result<String, String> {
    let kubeconfig = k3s_kubeconfig_path();
    let mut cmd = Command::new("kubectl");
    cmd.arg("--kubeconfig").arg(&kubeconfig);
    for arg in args {
        cmd.arg(arg);
    }
    cmd.arg("-o").arg("json");
    let out = cmd.output().map_err(|e| format!("kubectl failed: {}", e))?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).to_string());
    }
    Ok(String::from_utf8_lossy(&out.stdout).to_string())
}

fn k8s_age_from_timestamp(ts: &str) -> String {
    let Ok(created) = chrono::DateTime::parse_from_rfc3339(ts) else {
        return ts.to_string();
    };
    let now = chrono::Utc::now();
    let dur = now.signed_duration_since(created);
    if dur.num_days() > 0 {
        format!("{}d", dur.num_days())
    } else if dur.num_hours() > 0 {
        format!("{}h", dur.num_hours())
    } else if dur.num_minutes() > 0 {
        format!("{}m", dur.num_minutes())
    } else {
        format!("{}s", dur.num_seconds().max(0))
    }
}

#[tauri::command]
async fn k8s_list_namespaces() -> Result<Vec<String>, String> {
    let raw = run_kubectl(&["get", "namespaces"])?;
    let json: serde_json::Value =
        serde_json::from_str(&raw).map_err(|e| format!("JSON parse error: {}", e))?;
    let items = json["items"].as_array().ok_or("No items in response")?;
    let mut ns: Vec<String> = items
        .iter()
        .filter_map(|item| item["metadata"]["name"].as_str().map(|s| s.to_string()))
        .collect();
    ns.sort();
    Ok(ns)
}

#[tauri::command]
async fn k8s_list_pods(namespace: Option<String>) -> Result<Vec<K8sPod>, String> {
    let mut args = vec!["get", "pods"];
    let ns_flag;
    match &namespace {
        Some(ns) if !ns.is_empty() => {
            args.push("-n");
            ns_flag = ns.clone();
            args.push(&ns_flag);
        }
        _ => {
            args.push("--all-namespaces");
        }
    }
    let raw = run_kubectl(&args)?;
    let json: serde_json::Value =
        serde_json::from_str(&raw).map_err(|e| format!("JSON parse error: {}", e))?;
    let items = json["items"].as_array().ok_or("No items in response")?;

    let pods: Vec<K8sPod> = items
        .iter()
        .map(|item| {
            let name = item["metadata"]["name"].as_str().unwrap_or("").to_string();
            let namespace = item["metadata"]["namespace"]
                .as_str()
                .unwrap_or("")
                .to_string();
            let phase = item["status"]["phase"]
                .as_str()
                .unwrap_or("Unknown")
                .to_string();
            let containers = item["status"]["containerStatuses"]
                .as_array()
                .cloned()
                .unwrap_or_default();
            let total = containers.len();
            let ready_count = containers
                .iter()
                .filter(|c| c["ready"].as_bool().unwrap_or(false))
                .count();
            let ready = format!("{}/{}", ready_count, total);
            let restarts: u32 = containers
                .iter()
                .map(|c| c["restartCount"].as_u64().unwrap_or(0) as u32)
                .sum();
            let creation = item["metadata"]["creationTimestamp"].as_str().unwrap_or("");
            let age = k8s_age_from_timestamp(creation);

            K8sPod {
                name,
                namespace,
                status: phase,
                ready,
                restarts,
                age,
            }
        })
        .collect();
    Ok(pods)
}

#[tauri::command]
async fn k8s_list_services(namespace: Option<String>) -> Result<Vec<K8sService>, String> {
    let mut args = vec!["get", "services"];
    let ns_flag;
    match &namespace {
        Some(ns) if !ns.is_empty() => {
            args.push("-n");
            ns_flag = ns.clone();
            args.push(&ns_flag);
        }
        _ => {
            args.push("--all-namespaces");
        }
    }
    let raw = run_kubectl(&args)?;
    let json: serde_json::Value =
        serde_json::from_str(&raw).map_err(|e| format!("JSON parse error: {}", e))?;
    let items = json["items"].as_array().ok_or("No items in response")?;

    let services: Vec<K8sService> = items
        .iter()
        .map(|item| {
            let name = item["metadata"]["name"].as_str().unwrap_or("").to_string();
            let namespace = item["metadata"]["namespace"]
                .as_str()
                .unwrap_or("")
                .to_string();
            let service_type = item["spec"]["type"]
                .as_str()
                .unwrap_or("ClusterIP")
                .to_string();
            let cluster_ip = item["spec"]["clusterIP"]
                .as_str()
                .unwrap_or("None")
                .to_string();
            let ports_arr = item["spec"]["ports"]
                .as_array()
                .cloned()
                .unwrap_or_default();
            let ports = ports_arr
                .iter()
                .map(|p| {
                    let port = p["port"].as_u64().unwrap_or(0);
                    let proto = p["protocol"].as_str().unwrap_or("TCP");
                    let node_port = p["nodePort"].as_u64();
                    if let Some(np) = node_port {
                        format!("{}:{}/{}", port, np, proto)
                    } else {
                        format!("{}/{}", port, proto)
                    }
                })
                .collect::<Vec<_>>()
                .join(", ");

            K8sService {
                name,
                namespace,
                service_type,
                cluster_ip,
                ports,
            }
        })
        .collect();
    Ok(services)
}

#[tauri::command]
async fn k8s_list_deployments(namespace: Option<String>) -> Result<Vec<K8sDeployment>, String> {
    let mut args = vec!["get", "deployments"];
    let ns_flag;
    match &namespace {
        Some(ns) if !ns.is_empty() => {
            args.push("-n");
            ns_flag = ns.clone();
            args.push(&ns_flag);
        }
        _ => {
            args.push("--all-namespaces");
        }
    }
    let raw = run_kubectl(&args)?;
    let json: serde_json::Value =
        serde_json::from_str(&raw).map_err(|e| format!("JSON parse error: {}", e))?;
    let items = json["items"].as_array().ok_or("No items in response")?;

    let deployments: Vec<K8sDeployment> = items
        .iter()
        .map(|item| {
            let name = item["metadata"]["name"].as_str().unwrap_or("").to_string();
            let namespace = item["metadata"]["namespace"]
                .as_str()
                .unwrap_or("")
                .to_string();
            let replicas = item["status"]["replicas"].as_u64().unwrap_or(0);
            let ready_replicas = item["status"]["readyReplicas"].as_u64().unwrap_or(0);
            let ready = format!("{}/{}", ready_replicas, replicas);
            let up_to_date = item["status"]["updatedReplicas"].as_u64().unwrap_or(0) as u32;
            let available = item["status"]["availableReplicas"].as_u64().unwrap_or(0) as u32;
            let creation = item["metadata"]["creationTimestamp"].as_str().unwrap_or("");
            let age = k8s_age_from_timestamp(creation);

            K8sDeployment {
                name,
                namespace,
                ready,
                up_to_date,
                available,
                age,
            }
        })
        .collect();
    Ok(deployments)
}

#[tauri::command]
async fn k8s_pod_logs(
    name: String,
    namespace: String,
    tail: Option<u32>,
) -> Result<String, String> {
    let tail_str = tail.unwrap_or(200).to_string();
    let kubeconfig = k3s_kubeconfig_path();
    let mut cmd = Command::new("kubectl");
    cmd.arg("--kubeconfig")
        .arg(&kubeconfig)
        .arg("logs")
        .arg(&name)
        .arg("-n")
        .arg(&namespace)
        .arg("--tail")
        .arg(&tail_str);
    let out = cmd
        .output()
        .map_err(|e| format!("kubectl logs failed: {}", e))?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).to_string());
    }
    Ok(String::from_utf8_lossy(&out.stdout).to_string())
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
        let ns = r
            .namespace
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "library".to_string());
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
///   CrateBay            (disabled title)
///   ─────────────────
///   Dashboard           → focus/show the main window
///   Containers (N running)
///   VMs (N running)
///   ─────────────────
///   Quit CrateBay       → exit the application
fn build_tray_menu(
    app: &tauri::AppHandle,
    running_containers: usize,
    running_vms: usize,
) -> Result<tauri::menu::Menu<tauri::Wry>, Box<dyn std::error::Error>> {
    let title_item = MenuItemBuilder::with_id("title", "CrateBay")
        .enabled(false)
        .build(app)?;

    let sep1 = PredefinedMenuItem::separator(app)?;

    let dashboard_item = MenuItemBuilder::with_id("dashboard", "Dashboard").build(app)?;

    let containers_label = format!("Containers ({} running)", running_containers);
    let containers_item = MenuItemBuilder::with_id("containers", containers_label)
        .enabled(false)
        .build(app)?;

    let vms_label = format!("VMs ({} running)", running_vms);
    let vms_item = MenuItemBuilder::with_id("vms", vms_label)
        .enabled(false)
        .build(app)?;

    let sep2 = PredefinedMenuItem::separator(app)?;

    let quit_item = MenuItemBuilder::with_id("quit", "Quit CrateBay").build(app)?;

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
        .filter(|c| c.state.as_deref().map(|s| s == "running").unwrap_or(false))
        .count()
}

/// Count VMs with state == "running" via the gRPC daemon (or local hypervisor).
async fn count_running_vms(app_state: &AppState) -> usize {
    if let Ok(mut client) = connect_vm_service(&app_state.grpc_addr).await {
        if let Ok(resp) = client.list_v_ms(proto::ListVMsRequest {}).await {
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
            .filter(|vm| matches!(vm.state, cratebay_core::hypervisor::VmState::Running))
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

// ── Local model runtime (Ollama) ─────────────────────────────────────

const OLLAMA_BASE_URL: &str = "http://127.0.0.1:11434";

fn ollama_http_client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .timeout(Duration::from_millis(900))
        .user_agent("CrateBay/1.0.0 (+https://github.com/coder-hhx/CrateBay)")
        .build()
        .map_err(|e| e.to_string())
}

#[derive(Debug, Serialize)]
pub struct OllamaStatusDto {
    installed: bool,
    running: bool,
    version: String,
    base_url: String,
}

#[derive(Debug, Serialize)]
pub struct OllamaModelDto {
    name: String,
    size_bytes: u64,
    size_human: String,
    modified_at: String,
    digest: String,
    family: String,
    parameter_size: String,
    quantization_level: String,
}

#[derive(Debug, Deserialize)]
struct OllamaVersionResponse {
    version: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OllamaTagsResponse {
    models: Vec<OllamaTagModel>,
}

#[derive(Debug, Deserialize)]
struct OllamaTagModel {
    name: String,
    #[serde(default)]
    modified_at: String,
    #[serde(default)]
    size: u64,
    #[serde(default)]
    digest: String,
    #[serde(default)]
    details: OllamaModelDetails,
}

#[derive(Debug, Default, Deserialize)]
struct OllamaModelDetails {
    #[serde(default)]
    family: String,
    #[serde(default)]
    parameter_size: String,
    #[serde(default)]
    quantization_level: String,
}

async fn ollama_check_installed() -> bool {
    tokio::task::spawn_blocking(|| Command::new("ollama").arg("--version").output().is_ok())
        .await
        .unwrap_or(false)
}

async fn ollama_check_running() -> Result<String, String> {
    let client = ollama_http_client()?;
    let url = format!("{}/api/version", OLLAMA_BASE_URL.trim_end_matches('/'));
    let resp = client.get(&url).send().await.map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(format!("ollama version endpoint returned {}", resp.status()));
    }
    let body: OllamaVersionResponse = resp.json().await.map_err(|e| e.to_string())?;
    Ok(body.version.unwrap_or_default())
}

#[tauri::command]
async fn ollama_status() -> Result<OllamaStatusDto, String> {
    let installed = ollama_check_installed().await;
    match ollama_check_running().await {
        Ok(version) => Ok(OllamaStatusDto {
            installed,
            running: true,
            version,
            base_url: OLLAMA_BASE_URL.to_string(),
        }),
        Err(_) => Ok(OllamaStatusDto {
            installed,
            running: false,
            version: String::new(),
            base_url: OLLAMA_BASE_URL.to_string(),
        }),
    }
}

#[tauri::command]
async fn ollama_list_models() -> Result<Vec<OllamaModelDto>, String> {
    let client = ollama_http_client()?;
    let url = format!("{}/api/tags", OLLAMA_BASE_URL.trim_end_matches('/'));
    let resp = client.get(&url).send().await.map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(format!("ollama tags endpoint returned {}", resp.status()));
    }
    let body: OllamaTagsResponse = resp.json().await.map_err(|e| e.to_string())?;
    let mut out = body
        .models
        .into_iter()
        .map(|m| {
            let size = m.size;
            OllamaModelDto {
                name: m.name,
                size_bytes: size,
                size_human: format_bytes_human(size),
                modified_at: m.modified_at,
                digest: m.digest,
                family: m.details.family,
                parameter_size: m.details.parameter_size,
                quantization_level: m.details.quantization_level,
            }
        })
        .collect::<Vec<_>>();
    out.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(out)
}

// ── AI settings commands ───────────────────────────────────────────

const AI_SECRET_SERVICE: &str = "com.cratebay.app.ai";
static AI_REQUEST_SEQ: AtomicU64 = AtomicU64::new(1);

fn default_true() -> bool {
    true
}

fn default_skill_input_schema() -> serde_json::Value {
    serde_json::json!({})
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiProviderProfile {
    id: String,
    provider_id: String,
    display_name: String,
    model: String,
    base_url: String,
    api_key_ref: String,
    #[serde(default)]
    headers: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiSecurityPolicy {
    destructive_action_confirmation: bool,
    mcp_remote_enabled: bool,
    #[serde(default)]
    mcp_allowed_actions: Vec<String>,
    #[serde(default)]
    mcp_auth_token_ref: String,
    #[serde(default = "default_true")]
    mcp_audit_enabled: bool,
    #[serde(default)]
    cli_command_allowlist: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiSkillDefinition {
    id: String,
    display_name: String,
    description: String,
    #[serde(default)]
    tags: Vec<String>,
    executor: String,
    target: String,
    #[serde(default = "default_skill_input_schema")]
    input_schema: serde_json::Value,
    #[serde(default = "default_true")]
    enabled: bool,
}

impl Default for AiSecurityPolicy {
    fn default() -> Self {
        Self {
            destructive_action_confirmation: true,
            mcp_remote_enabled: false,
            mcp_allowed_actions: vec![
                "list_containers".to_string(),
                "vm_list".to_string(),
                "k8s_list_pods".to_string(),
            ],
            mcp_auth_token_ref: "MCP_AUTH_TOKEN".to_string(),
            mcp_audit_enabled: true,
            cli_command_allowlist: vec![
                "codex".to_string(),
                "claude".to_string(),
                "openclaw".to_string(),
                "gemini".to_string(),
                "qwen".to_string(),
                "aider".to_string(),
            ],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiSettings {
    profiles: Vec<AiProviderProfile>,
    active_profile_id: String,
    #[serde(default = "default_ai_skills")]
    skills: Vec<AiSkillDefinition>,
    #[serde(default)]
    security_policy: AiSecurityPolicy,
}

impl Default for AiSettings {
    fn default() -> Self {
        default_ai_settings()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiProfileValidationResult {
    ok: bool,
    message: String,
}

fn ai_profile(
    id: &str,
    provider_id: &str,
    display_name: &str,
    model: &str,
    base_url: &str,
    api_key_ref: &str,
) -> AiProviderProfile {
    AiProviderProfile {
        id: id.to_string(),
        provider_id: provider_id.to_string(),
        display_name: display_name.to_string(),
        model: model.to_string(),
        base_url: base_url.to_string(),
        api_key_ref: api_key_ref.to_string(),
        headers: HashMap::new(),
    }
}

fn default_ai_profiles() -> Vec<AiProviderProfile> {
    vec![
        ai_profile(
            "openai-default",
            "openai",
            "OpenAI (GPT-4.1-mini)",
            "gpt-4.1-mini",
            "https://api.openai.com/v1",
            "OPENAI_API_KEY",
        ),
        ai_profile(
            "anthropic-default",
            "anthropic",
            "Anthropic (Claude 3.7 Sonnet)",
            "claude-3-7-sonnet-latest",
            "https://api.anthropic.com/v1",
            "ANTHROPIC_API_KEY",
        ),
        ai_profile(
            "gemini-default",
            "gemini",
            "Gemini (2.5 Pro)",
            "gemini-2.5-pro",
            "https://generativelanguage.googleapis.com/v1beta/openai",
            "GEMINI_API_KEY",
        ),
        ai_profile(
            "openrouter-default",
            "openrouter",
            "OpenRouter (GPT-4.1-mini)",
            "openai/gpt-4.1-mini",
            "https://openrouter.ai/api/v1",
            "OPENROUTER_API_KEY",
        ),
        ai_profile(
            "deepseek-default",
            "deepseek",
            "DeepSeek (chat)",
            "deepseek-chat",
            "https://api.deepseek.com/v1",
            "DEEPSEEK_API_KEY",
        ),
        ai_profile(
            "minimax-default",
            "minimax",
            "MiniMax (Text-01)",
            "MiniMax-Text-01",
            "https://api.minimax.chat/v1",
            "MINIMAX_API_KEY",
        ),
        ai_profile(
            "kimi-default",
            "kimi",
            "Kimi (Moonshot)",
            "moonshot-v1-8k",
            "https://api.moonshot.cn/v1",
            "KIMI_API_KEY",
        ),
        ai_profile(
            "glm-default",
            "glm",
            "GLM (4 Plus)",
            "glm-4-plus",
            "https://open.bigmodel.cn/api/paas/v4",
            "GLM_API_KEY",
        ),
        ai_profile(
            "ollama-default",
            "ollama",
            "Ollama Local",
            "qwen2.5:7b",
            "http://127.0.0.1:11434/v1",
            "",
        ),
        ai_profile(
            "custom-default",
            "custom",
            "Custom Provider",
            "model-name",
            "https://api.example.com/v1",
            "CUSTOM_LLM_API_KEY",
        ),
    ]
}

fn ai_skill(
    id: &str,
    display_name: &str,
    description: &str,
    tags: &[&str],
    executor: &str,
    target: &str,
    input_schema: serde_json::Value,
) -> AiSkillDefinition {
    AiSkillDefinition {
        id: id.to_string(),
        display_name: display_name.to_string(),
        description: description.to_string(),
        tags: tags.iter().map(|item| item.to_string()).collect(),
        executor: executor.to_string(),
        target: target.to_string(),
        input_schema,
        enabled: true,
    }
}

fn default_ai_skills() -> Vec<AiSkillDefinition> {
    vec![
        ai_skill(
            "assistant-container-diagnose",
            "Container Diagnose",
            "Run safe assistant read flow for container status and baseline diagnosis.",
            &["assistant", "containers", "read"],
            "assistant_step",
            "list_containers",
            serde_json::json!({
                "type": "object",
                "properties": {},
                "additionalProperties": false
            }),
        ),
        ai_skill(
            "mcp-k8s-pods-read",
            "Kubernetes Pods Read",
            "Use MCP allowlisted action to fetch pod status for troubleshooting.",
            &["mcp", "k8s", "read"],
            "mcp_action",
            "k8s_list_pods",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "namespace": { "type": "string" }
                },
                "additionalProperties": false
            }),
        ),
        ai_skill(
            "agent-cli-openclaw-plan",
            "OpenClaw CLI Plan",
            "Invoke OpenClaw CLI preset to generate multi-step task plans.",
            &["agent-cli", "openclaw", "planning"],
            "agent_cli_preset",
            "openclaw",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "prompt": { "type": "string" }
                },
                "required": ["prompt"],
                "additionalProperties": false
            }),
        ),
    ]
}

fn default_ai_settings() -> AiSettings {
    let profiles = default_ai_profiles();
    let active_profile_id = profiles
        .first()
        .map(|p| p.id.clone())
        .unwrap_or_else(|| "openai-default".to_string());
    AiSettings {
        profiles,
        active_profile_id,
        skills: default_ai_skills(),
        security_policy: AiSecurityPolicy::default(),
    }
}

fn ai_settings_path() -> PathBuf {
    cratebay_core::config_dir().join("ai-settings.json")
}

fn validate_ai_profile_inner(profile: &AiProviderProfile) -> AiProfileValidationResult {
    if profile.id.trim().is_empty() {
        return AiProfileValidationResult {
            ok: false,
            message: "Profile id is required".to_string(),
        };
    }
    if profile.provider_id.trim().is_empty() {
        return AiProfileValidationResult {
            ok: false,
            message: "Provider id is required".to_string(),
        };
    }
    if profile.display_name.trim().is_empty() {
        return AiProfileValidationResult {
            ok: false,
            message: "Display name is required".to_string(),
        };
    }
    if profile.model.trim().is_empty() {
        return AiProfileValidationResult {
            ok: false,
            message: "Model is required".to_string(),
        };
    }

    let base_url = profile.base_url.trim();
    if base_url.is_empty() {
        return AiProfileValidationResult {
            ok: false,
            message: "Base URL is required".to_string(),
        };
    }
    if !(base_url.starts_with("https://") || base_url.starts_with("http://")) {
        return AiProfileValidationResult {
            ok: false,
            message: "Base URL must start with http:// or https://".to_string(),
        };
    }

    for (key, value) in &profile.headers {
        if key.trim().is_empty() {
            return AiProfileValidationResult {
                ok: false,
                message: "Header key cannot be empty".to_string(),
            };
        }
        if value.trim().is_empty() {
            return AiProfileValidationResult {
                ok: false,
                message: format!("Header value for '{}' cannot be empty", key),
            };
        }
    }

    let base_lower = base_url.to_ascii_lowercase();
    let is_local_endpoint = base_lower.contains("127.0.0.1") || base_lower.contains("localhost");
    if !is_local_endpoint && profile.api_key_ref.trim().is_empty() {
        return AiProfileValidationResult {
            ok: false,
            message: "API key reference is required for non-local endpoints".to_string(),
        };
    }

    AiProfileValidationResult {
        ok: true,
        message: "Profile is valid".to_string(),
    }
}

fn normalize_ai_settings(mut settings: AiSettings) -> AiSettings {
    if settings.profiles.is_empty() {
        return default_ai_settings();
    }

    let mut seen = std::collections::HashSet::new();
    settings.profiles.retain(|profile| {
        let id = profile.id.trim();
        !id.is_empty() && seen.insert(id.to_string())
    });

    if settings.profiles.is_empty() {
        return default_ai_settings();
    }

    if !settings
        .profiles
        .iter()
        .any(|p| p.id == settings.active_profile_id)
    {
        if let Some(profile) = settings.profiles.first() {
            settings.active_profile_id = profile.id.clone();
        }
    }

    let mut skill_seen = std::collections::HashSet::new();
    settings.skills.retain(|skill| {
        let id = skill.id.trim();
        !id.is_empty() && skill_seen.insert(id.to_string())
    });
    for skill in &mut settings.skills {
        skill.id = skill.id.trim().to_string();
        skill.display_name = skill.display_name.trim().to_string();
        skill.description = skill.description.trim().to_string();
        skill.executor = skill.executor.trim().to_string();
        skill.target = skill.target.trim().to_string();
        skill.tags.retain(|tag| !tag.trim().is_empty());
        if skill.display_name.is_empty() {
            skill.display_name = skill.id.clone();
        }
        if skill.description.is_empty() {
            skill.description = "Skill scaffold entry".to_string();
        }
        if skill.executor.is_empty() {
            skill.executor = "assistant_step".to_string();
        }
    }
    if settings.skills.is_empty() {
        settings.skills = default_ai_skills();
    }

    settings
}

fn persist_ai_settings(path: &Path, settings: &AiSettings) -> Result<(), String> {
    let Some(parent) = path.parent() else {
        return Err(format!("Invalid settings path: {}", path.display()));
    };
    std::fs::create_dir_all(parent).map_err(|e| {
        format!(
            "Failed to create config directory {}: {}",
            parent.display(),
            e
        )
    })?;

    let body = serde_json::to_vec_pretty(settings)
        .map_err(|e| format!("Failed to encode settings: {}", e))?;
    std::fs::write(path, body)
        .map_err(|e| format!("Failed to write settings file {}: {}", path.display(), e))
}

#[tauri::command]
fn load_ai_settings() -> Result<AiSettings, String> {
    let path = ai_settings_path();
    if !path.exists() {
        let defaults = default_ai_settings();
        persist_ai_settings(&path, &defaults)?;
        return Ok(defaults);
    }

    let raw = std::fs::read_to_string(&path)
        .map_err(|e| format!("Failed to read settings file {}: {}", path.display(), e))?;
    let parsed: AiSettings = serde_json::from_str(&raw)
        .map_err(|e| format!("Failed to parse AI settings JSON: {}", e))?;
    Ok(normalize_ai_settings(parsed))
}

#[tauri::command]
fn save_ai_settings(settings: AiSettings) -> Result<AiSettings, String> {
    let normalized = normalize_ai_settings(settings);
    for profile in &normalized.profiles {
        let result = validate_ai_profile_inner(profile);
        if !result.ok {
            return Err(format!(
                "Profile '{}' validation failed: {}",
                profile.display_name, result.message
            ));
        }
    }

    let path = ai_settings_path();
    persist_ai_settings(&path, &normalized)?;
    Ok(normalized)
}

#[tauri::command]
fn validate_ai_profile(profile: AiProviderProfile) -> AiProfileValidationResult {
    validate_ai_profile_inner(&profile)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiChatMessage {
    role: String,
    content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiUsage {
    prompt_tokens: Option<u64>,
    completion_tokens: Option<u64>,
    total_tokens: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiToolCall {
    name: String,
    arguments: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiChatResponse {
    request_id: String,
    provider_id: String,
    model: String,
    text: String,
    #[serde(default)]
    usage: Option<AiUsage>,
    #[serde(default)]
    tool_calls: Vec<AiToolCall>,
    #[serde(default)]
    error_type: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiConnectionTestResult {
    ok: bool,
    request_id: String,
    message: String,
    #[serde(default)]
    error_type: Option<String>,
    latency_ms: u128,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssistantPlanStep {
    id: String,
    title: String,
    command: String,
    args: serde_json::Value,
    risk_level: String,
    requires_confirmation: bool,
    explain: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssistantPlanResult {
    request_id: String,
    strategy: String,
    notes: String,
    fallback_used: bool,
    steps: Vec<AssistantPlanStep>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpAccessCheckResult {
    allowed: bool,
    request_id: String,
    message: String,
    risk_level: String,
    requires_confirmation: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DockerRuntimeSetupResult {
    ok: bool,
    request_id: String,
    message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentCliPreset {
    id: String,
    name: String,
    description: String,
    command: String,
    args_template: Vec<String>,
    timeout_sec: u64,
    dangerous: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentCliRunResult {
    ok: bool,
    request_id: String,
    command_line: String,
    exit_code: i32,
    stdout: String,
    stderr: String,
    duration_ms: u128,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssistantStepExecutionResult {
    ok: bool,
    request_id: String,
    command: String,
    risk_level: String,
    output: serde_json::Value,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct AssistantCommandPolicy {
    risk_level: &'static str,
    always_confirm: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct McpActionPolicy {
    risk_level: &'static str,
    requires_confirmation: bool,
}

fn assistant_command_policy(command: &str) -> Option<AssistantCommandPolicy> {
    match command {
        "list_containers" | "vm_list" | "k8s_list_pods" => Some(AssistantCommandPolicy {
            risk_level: "read",
            always_confirm: false,
        }),
        "start_container"
        | "stop_container"
        | "vm_start"
        | "vm_stop"
        | "docker_runtime_quick_setup" => Some(AssistantCommandPolicy {
            risk_level: "write",
            always_confirm: false,
        }),
        "remove_container" | "vm_delete" => Some(AssistantCommandPolicy {
            risk_level: "destructive",
            always_confirm: true,
        }),
        _ => None,
    }
}

fn mcp_action_has_keyword(action_lower: &str, keyword: &str) -> bool {
    action_lower == keyword
        || action_lower
            .split(|c: char| !c.is_ascii_alphanumeric())
            .any(|segment| segment == keyword)
        || action_lower.contains(keyword)
}

fn mcp_action_policy(action: &str) -> McpActionPolicy {
    let action_lower = action.trim().to_ascii_lowercase();
    const DESTRUCTIVE_KEYWORDS: &[&str] = &[
        "delete",
        "remove",
        "destroy",
        "drop",
        "wipe",
        "prune",
        "terminate",
        "kill",
        "uninstall",
        "purge",
    ];
    const WRITE_KEYWORDS: &[&str] = &[
        "create", "apply", "update", "patch", "set", "start", "stop", "restart", "scale", "run",
        "exec", "install",
    ];

    if DESTRUCTIVE_KEYWORDS
        .iter()
        .any(|kw| mcp_action_has_keyword(&action_lower, kw))
    {
        return McpActionPolicy {
            risk_level: "destructive",
            requires_confirmation: true,
        };
    }

    if WRITE_KEYWORDS
        .iter()
        .any(|kw| mcp_action_has_keyword(&action_lower, kw))
    {
        return McpActionPolicy {
            risk_level: "write",
            requires_confirmation: false,
        };
    }

    McpActionPolicy {
        risk_level: "read",
        requires_confirmation: false,
    }
}

fn mcp_confirmation_satisfied(
    action_policy: McpActionPolicy,
    destructive_action_confirmation: bool,
    requires_confirmation: Option<bool>,
    confirmed: Option<bool>,
) -> bool {
    if !destructive_action_confirmation || !action_policy.requires_confirmation {
        return true;
    }

    requires_confirmation.unwrap_or(false) && confirmed.unwrap_or(false)
}

fn assistant_arg_map(
    args: &serde_json::Value,
) -> Result<&serde_json::Map<String, serde_json::Value>, String> {
    args.as_object()
        .ok_or_else(|| "assistant step args must be a JSON object".to_string())
}

fn assistant_arg_string(
    args: &serde_json::Map<String, serde_json::Value>,
    key: &str,
) -> Result<String, String> {
    args.get(key)
        .and_then(|v| v.as_str())
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .ok_or_else(|| format!("assistant step missing required string arg '{}'", key))
}

fn assistant_arg_optional_string(
    args: &serde_json::Map<String, serde_json::Value>,
    key: &str,
) -> Result<Option<String>, String> {
    match args.get(key) {
        Some(v) if v.is_null() => Ok(None),
        Some(v) => v
            .as_str()
            .map(|s| Some(s.trim().to_string()))
            .ok_or_else(|| format!("assistant step arg '{}' must be a string or null", key)),
        None => Ok(None),
    }
}

fn next_ai_request_id() -> String {
    let seq = AI_REQUEST_SEQ.fetch_add(1, Ordering::Relaxed);
    format!("ai-{}-{}", chrono::Utc::now().timestamp_millis(), seq)
}

fn redact_sensitive(mut text: String) -> String {
    let lower = text.to_ascii_lowercase();
    if lower.contains("authorization") {
        text = text.replace("Authorization", "Authorization[redacted]");
        text = text.replace("authorization", "authorization[redacted]");
    }
    if lower.contains("bearer ") {
        text = text.replace("Bearer ", "Bearer [redacted]");
        text = text.replace("bearer ", "bearer [redacted]");
    }
    if lower.contains("api_key") {
        text = text.replace("api_key", "api_key[redacted]");
    }
    text
}

fn ai_audit_log(action: &str, level: &str, request_id: &str, details: &str) {
    let path = cratebay_core::config_dir()
        .join("audit")
        .join("ai-actions.log");
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
        let sanitized = cratebay_core::validation::sanitize_log_string(details);
        let redacted = redact_sensitive(sanitized);
        let _ = writeln!(
            file,
            "{} request_id={} action={} level={} {}",
            chrono::Utc::now().to_rfc3339(),
            request_id,
            action,
            level,
            redacted
        );
    }
}

fn secret_entry(key_ref: &str) -> Result<Entry, String> {
    Entry::new(AI_SECRET_SERVICE, key_ref)
        .map_err(|e| format!("Failed to create secret entry: {e}"))
}

fn secret_set(key_ref: &str, value: &str) -> Result<(), String> {
    let entry = secret_entry(key_ref)?;
    entry
        .set_password(value)
        .map_err(|e| format!("Failed to save secret '{}': {}", key_ref, e))
}

fn secret_get(key_ref: &str) -> Result<Option<String>, String> {
    let entry = secret_entry(key_ref)?;
    match entry.get_password() {
        Ok(v) => {
            if v.trim().is_empty() {
                Ok(None)
            } else {
                Ok(Some(v))
            }
        }
        Err(e) => {
            let msg = e.to_string();
            if msg.contains("NoEntry") || msg.contains("No entry") {
                Ok(None)
            } else {
                Err(format!("Failed to read secret '{}': {}", key_ref, e))
            }
        }
    }
}

fn secret_delete(key_ref: &str) -> Result<(), String> {
    let entry = secret_entry(key_ref)?;
    match entry.delete_password() {
        Ok(_) => Ok(()),
        Err(e) => {
            let msg = e.to_string();
            if msg.contains("NoEntry") || msg.contains("No entry") {
                Ok(())
            } else {
                Err(format!("Failed to delete secret '{}': {}", key_ref, e))
            }
        }
    }
}

#[tauri::command]
fn ai_secret_set(api_key_ref: String, api_key: String) -> Result<(), String> {
    if api_key_ref.trim().is_empty() {
        return Err("api_key_ref is required".to_string());
    }
    if api_key.trim().is_empty() {
        return Err("api_key is required".to_string());
    }
    let request_id = next_ai_request_id();
    let out = secret_set(api_key_ref.trim(), api_key.trim());
    let status = if out.is_ok() { "ok" } else { "error" };
    ai_audit_log(
        "ai_secret_set",
        "write",
        &request_id,
        &format!("key_ref={} status={}", api_key_ref.trim(), status),
    );
    out
}

#[tauri::command]
fn ai_secret_delete(api_key_ref: String) -> Result<(), String> {
    if api_key_ref.trim().is_empty() {
        return Err("api_key_ref is required".to_string());
    }
    let request_id = next_ai_request_id();
    let out = secret_delete(api_key_ref.trim());
    let status = if out.is_ok() { "ok" } else { "error" };
    ai_audit_log(
        "ai_secret_delete",
        "write",
        &request_id,
        &format!("key_ref={} status={}", api_key_ref.trim(), status),
    );
    out
}

#[tauri::command]
fn ai_secret_exists(api_key_ref: String) -> Result<bool, String> {
    if api_key_ref.trim().is_empty() {
        return Err("api_key_ref is required".to_string());
    }
    let exists = secret_get(api_key_ref.trim())?.is_some();
    Ok(exists)
}

fn resolve_ai_profile(
    settings: &AiSettings,
    profile_id: Option<&str>,
) -> Result<AiProviderProfile, String> {
    if let Some(pid) = profile_id {
        return settings
            .profiles
            .iter()
            .find(|p| p.id == pid)
            .cloned()
            .ok_or_else(|| format!("Profile not found: {}", pid));
    }
    settings
        .profiles
        .iter()
        .find(|p| p.id == settings.active_profile_id)
        .cloned()
        .ok_or_else(|| "Active AI profile not found".to_string())
}

fn classify_provider_error(status: reqwest::StatusCode) -> String {
    if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
        "auth_error".to_string()
    } else if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
        "rate_limit".to_string()
    } else if status.is_client_error() {
        "invalid_request".to_string()
    } else if status.is_server_error() {
        "provider_unavailable".to_string()
    } else {
        "unknown_error".to_string()
    }
}

fn normalized_base_url(base_url: &str) -> String {
    base_url.trim_end_matches('/').to_string()
}

fn join_endpoint(base_url: &str, suffix: &str) -> String {
    format!(
        "{}/{}",
        normalized_base_url(base_url),
        suffix.trim_start_matches('/')
    )
}

fn parse_openai_text(content: &serde_json::Value) -> String {
    if let Some(s) = content.as_str() {
        return s.to_string();
    }
    if let Some(parts) = content.as_array() {
        let mut out = String::new();
        for item in parts {
            if let Some(text) = item.get("text").and_then(|v| v.as_str()) {
                out.push_str(text);
            }
        }
        return out;
    }
    String::new()
}

fn parse_openai_tool_calls(value: &serde_json::Value) -> Vec<AiToolCall> {
    value
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|item| {
                    let function = item.get("function")?;
                    let name = function.get("name")?.as_str()?.to_string();
                    let args_value = function
                        .get("arguments")
                        .and_then(|v| v.as_str())
                        .and_then(|s| serde_json::from_str::<serde_json::Value>(s).ok())
                        .unwrap_or_else(|| serde_json::json!({}));
                    Some(AiToolCall {
                        name,
                        arguments: args_value,
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

fn parse_usage(value: &serde_json::Value) -> Option<AiUsage> {
    let prompt_tokens = value.get("prompt_tokens").and_then(|v| v.as_u64());
    let completion_tokens = value.get("completion_tokens").and_then(|v| v.as_u64());
    let total_tokens = value.get("total_tokens").and_then(|v| v.as_u64());
    if prompt_tokens.is_none() && completion_tokens.is_none() && total_tokens.is_none() {
        None
    } else {
        Some(AiUsage {
            prompt_tokens,
            completion_tokens,
            total_tokens,
        })
    }
}

async fn call_anthropic(
    client: &reqwest::Client,
    profile: &AiProviderProfile,
    messages: &[AiChatMessage],
    timeout_ms: u64,
    api_key: &str,
    request_id: &str,
) -> Result<AiChatResponse, String> {
    let url = join_endpoint(&profile.base_url, "/messages");
    let body = serde_json::json!({
        "model": profile.model,
        "max_tokens": 512u32,
        "messages": messages.iter().map(|m| {
            serde_json::json!({
                "role": if m.role == "assistant" { "assistant" } else { "user" },
                "content": m.content
            })
        }).collect::<Vec<_>>()
    });

    let mut req = client
        .post(url)
        .timeout(Duration::from_millis(timeout_ms))
        .header("x-api-key", api_key)
        .header("anthropic-version", "2023-06-01")
        .json(&body);

    for (k, v) in &profile.headers {
        req = req.header(k, v);
    }

    let resp = req.send().await.map_err(|e| {
        if e.is_timeout() {
            "network_error: request timeout".to_string()
        } else {
            format!("network_error: {}", e)
        }
    })?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(format!(
            "{}: HTTP {} {}",
            classify_provider_error(status),
            status,
            text
        ));
    }

    let json: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| format!("provider_unavailable: invalid JSON response: {}", e))?;

    let text = json
        .get("content")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|item| item.get("text").and_then(|v| v.as_str()))
                .collect::<Vec<_>>()
                .join("")
        })
        .unwrap_or_default();

    Ok(AiChatResponse {
        request_id: request_id.to_string(),
        provider_id: profile.provider_id.clone(),
        model: profile.model.clone(),
        text,
        usage: json.get("usage").and_then(parse_usage),
        tool_calls: vec![],
        error_type: None,
    })
}

async fn call_openai_compatible(
    client: &reqwest::Client,
    profile: &AiProviderProfile,
    messages: &[AiChatMessage],
    timeout_ms: u64,
    api_key: Option<&str>,
    request_id: &str,
) -> Result<AiChatResponse, String> {
    let url = join_endpoint(&profile.base_url, "/chat/completions");
    let body = serde_json::json!({
        "model": profile.model,
        "stream": false,
        "messages": messages.iter().map(|m| {
            serde_json::json!({
                "role": m.role,
                "content": m.content
            })
        }).collect::<Vec<_>>()
    });

    let mut req = client
        .post(url)
        .timeout(Duration::from_millis(timeout_ms))
        .json(&body);

    if let Some(key) = api_key {
        if !key.trim().is_empty() {
            req = req.bearer_auth(key);
        }
    }
    for (k, v) in &profile.headers {
        req = req.header(k, v);
    }

    let resp = req.send().await.map_err(|e| {
        if e.is_timeout() {
            "network_error: request timeout".to_string()
        } else {
            format!("network_error: {}", e)
        }
    })?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(format!(
            "{}: HTTP {} {}",
            classify_provider_error(status),
            status,
            text
        ));
    }

    let json: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| format!("provider_unavailable: invalid JSON response: {}", e))?;

    let choice = json
        .get("choices")
        .and_then(|v| v.as_array())
        .and_then(|arr| arr.first())
        .cloned()
        .unwrap_or_else(|| serde_json::json!({}));

    let message = choice
        .get("message")
        .cloned()
        .unwrap_or_else(|| serde_json::json!({}));
    let text = message
        .get("content")
        .map(parse_openai_text)
        .unwrap_or_default();
    let tool_calls = message
        .get("tool_calls")
        .map(parse_openai_tool_calls)
        .unwrap_or_default();

    Ok(AiChatResponse {
        request_id: request_id.to_string(),
        provider_id: profile.provider_id.clone(),
        model: profile.model.clone(),
        text,
        usage: json.get("usage").and_then(parse_usage),
        tool_calls,
        error_type: None,
    })
}

async fn ai_chat_inner(
    profile: &AiProviderProfile,
    messages: &[AiChatMessage],
    timeout_ms: u64,
    request_id: &str,
) -> Result<AiChatResponse, String> {
    let client = reqwest::Client::builder()
        .user_agent("CrateBay-AI/1.0.0")
        .build()
        .map_err(|e| format!("Failed to build HTTP client: {}", e))?;

    let api_key = if profile.api_key_ref.trim().is_empty() {
        None
    } else {
        secret_get(profile.api_key_ref.trim())?
    };

    if profile.provider_id != "ollama" && profile.provider_id != "custom" {
        let local_endpoint =
            profile.base_url.contains("127.0.0.1") || profile.base_url.contains("localhost");
        if !local_endpoint && api_key.as_deref().unwrap_or("").trim().is_empty() {
            return Err(format!(
                "auth_error: API key not found in secure store for key ref '{}'",
                profile.api_key_ref
            ));
        }
    }

    if profile.provider_id == "anthropic" {
        let key = api_key.unwrap_or_default();
        if key.trim().is_empty() {
            return Err("auth_error: anthropic profile requires API key".to_string());
        }
        call_anthropic(&client, profile, messages, timeout_ms, &key, request_id).await
    } else {
        call_openai_compatible(
            &client,
            profile,
            messages,
            timeout_ms,
            api_key.as_deref(),
            request_id,
        )
        .await
    }
}

#[tauri::command]
async fn ai_chat(
    profile_id: Option<String>,
    messages: Vec<AiChatMessage>,
    timeout_ms: Option<u64>,
) -> Result<AiChatResponse, String> {
    if messages.is_empty() {
        return Err("messages cannot be empty".to_string());
    }
    let settings = load_ai_settings()?;
    let profile = resolve_ai_profile(&settings, profile_id.as_deref())?;
    let request_id = next_ai_request_id();
    let out = ai_chat_inner(
        &profile,
        &messages,
        timeout_ms.unwrap_or(30_000),
        &request_id,
    )
    .await;
    let level = if out.is_ok() { "read" } else { "error" };
    ai_audit_log(
        "ai_chat",
        level,
        &request_id,
        &format!(
            "provider={} profile={} messages={}",
            profile.provider_id,
            profile.id,
            messages.len()
        ),
    );
    out
}

#[tauri::command]
async fn ai_test_connection(
    profile_id: Option<String>,
    timeout_ms: Option<u64>,
) -> Result<AiConnectionTestResult, String> {
    let settings = load_ai_settings()?;
    let profile = resolve_ai_profile(&settings, profile_id.as_deref())?;
    let request_id = next_ai_request_id();
    let started = std::time::Instant::now();
    let messages = vec![AiChatMessage {
        role: "user".to_string(),
        content: "Reply with the single word PONG.".to_string(),
    }];

    let out = ai_chat_inner(
        &profile,
        &messages,
        timeout_ms.unwrap_or(20_000),
        &request_id,
    )
    .await;
    let latency_ms = started.elapsed().as_millis();

    match out {
        Ok(resp) => {
            ai_audit_log(
                "ai_test_connection",
                "read",
                &request_id,
                &format!(
                    "provider={} profile={} ok=true",
                    profile.provider_id, profile.id
                ),
            );
            Ok(AiConnectionTestResult {
                ok: true,
                request_id: resp.request_id,
                message: if resp.text.trim().is_empty() {
                    "Connection succeeded but provider returned empty text".to_string()
                } else {
                    format!("Connection succeeded: {}", resp.text.trim())
                },
                error_type: None,
                latency_ms,
            })
        }
        Err(err) => {
            let error_type = err.split(':').next().map(|s| s.trim().to_string());
            ai_audit_log(
                "ai_test_connection",
                "error",
                &request_id,
                &format!(
                    "provider={} profile={} ok=false error={}",
                    profile.provider_id, profile.id, err
                ),
            );
            Ok(AiConnectionTestResult {
                ok: false,
                request_id,
                message: err,
                error_type,
                latency_ms,
            })
        }
    }
}

#[cfg(test)]
fn infer_assistant_steps(prompt: &str, require_confirm: bool) -> Vec<AssistantPlanStep> {
    infer_assistant_steps_with_runtime(prompt, require_confirm, true)
}

fn command_needs_docker_runtime(command: &str) -> bool {
    matches!(
        command,
        "list_containers" | "start_container" | "stop_container" | "remove_container"
    )
}

fn infer_assistant_steps_with_runtime(
    prompt: &str,
    require_confirm: bool,
    docker_runtime_available: bool,
) -> Vec<AssistantPlanStep> {
    let mut steps: Vec<AssistantPlanStep> = Vec::new();
    let lower = prompt.to_ascii_lowercase();

    if lower.contains("container") || prompt.contains("容器") {
        if lower.contains("stop") || prompt.contains("停止") {
            steps.push(AssistantPlanStep {
                id: "step-1".to_string(),
                title: "Stop container".to_string(),
                command: "stop_container".to_string(),
                args: serde_json::json!({ "id": "<container-id>" }),
                risk_level: "write".to_string(),
                requires_confirmation: require_confirm,
                explain: "Stops a running container.".to_string(),
            });
        } else if lower.contains("delete")
            || lower.contains("remove")
            || prompt.contains("删除")
            || prompt.contains("移除")
        {
            steps.push(AssistantPlanStep {
                id: "step-1".to_string(),
                title: "Remove container".to_string(),
                command: "remove_container".to_string(),
                args: serde_json::json!({ "id": "<container-id>" }),
                risk_level: "destructive".to_string(),
                requires_confirmation: true,
                explain: "Removes a container after stopping it.".to_string(),
            });
        } else if lower.contains("start") || prompt.contains("启动") {
            steps.push(AssistantPlanStep {
                id: "step-1".to_string(),
                title: "Start container".to_string(),
                command: "start_container".to_string(),
                args: serde_json::json!({ "id": "<container-id>" }),
                risk_level: "write".to_string(),
                requires_confirmation: require_confirm,
                explain: "Starts a stopped container.".to_string(),
            });
        } else {
            steps.push(AssistantPlanStep {
                id: "step-1".to_string(),
                title: "List containers".to_string(),
                command: "list_containers".to_string(),
                args: serde_json::json!({}),
                risk_level: "read".to_string(),
                requires_confirmation: false,
                explain: "Lists all containers as context before any action.".to_string(),
            });
        }
    }

    if lower.contains("vm") || prompt.contains("虚拟机") {
        if lower.contains("stop") || prompt.contains("停止") {
            steps.push(AssistantPlanStep {
                id: format!("step-{}", steps.len() + 1),
                title: "Stop VM".to_string(),
                command: "vm_stop".to_string(),
                args: serde_json::json!({ "id": "<vm-id>" }),
                risk_level: "write".to_string(),
                requires_confirmation: require_confirm,
                explain: "Stops a running VM.".to_string(),
            });
        } else if lower.contains("delete")
            || lower.contains("remove")
            || prompt.contains("删除")
            || prompt.contains("移除")
        {
            steps.push(AssistantPlanStep {
                id: format!("step-{}", steps.len() + 1),
                title: "Delete VM".to_string(),
                command: "vm_delete".to_string(),
                args: serde_json::json!({ "id": "<vm-id>" }),
                risk_level: "destructive".to_string(),
                requires_confirmation: true,
                explain: "Deletes VM metadata and local state.".to_string(),
            });
        } else if lower.contains("start") || prompt.contains("启动") {
            steps.push(AssistantPlanStep {
                id: format!("step-{}", steps.len() + 1),
                title: "Start VM".to_string(),
                command: "vm_start".to_string(),
                args: serde_json::json!({ "id": "<vm-id>" }),
                risk_level: "write".to_string(),
                requires_confirmation: require_confirm,
                explain: "Starts a VM.".to_string(),
            });
        } else {
            steps.push(AssistantPlanStep {
                id: format!("step-{}", steps.len() + 1),
                title: "List VMs".to_string(),
                command: "vm_list".to_string(),
                args: serde_json::json!({}),
                risk_level: "read".to_string(),
                requires_confirmation: false,
                explain: "Lists VMs and current state.".to_string(),
            });
        }
    }

    if lower.contains("k8s")
        || lower.contains("kubernetes")
        || prompt.contains("集群")
        || prompt.contains("pod")
    {
        steps.push(AssistantPlanStep {
            id: format!("step-{}", steps.len() + 1),
            title: "List Kubernetes pods".to_string(),
            command: "k8s_list_pods".to_string(),
            args: serde_json::json!({ "namespace": serde_json::Value::Null }),
            risk_level: "read".to_string(),
            requires_confirmation: false,
            explain: "Queries pod status for diagnosis.".to_string(),
        });
    }

    if steps.is_empty() {
        steps.push(AssistantPlanStep {
            id: "step-1".to_string(),
            title: "List containers".to_string(),
            command: "list_containers".to_string(),
            args: serde_json::json!({}),
            risk_level: "read".to_string(),
            requires_confirmation: false,
            explain: "Fallback baseline step when intent is ambiguous.".to_string(),
        });
        steps.push(AssistantPlanStep {
            id: "step-2".to_string(),
            title: "List VMs".to_string(),
            command: "vm_list".to_string(),
            args: serde_json::json!({}),
            risk_level: "read".to_string(),
            requires_confirmation: false,
            explain: "Collects VM context before deciding next operation.".to_string(),
        });
    }

    if !docker_runtime_available
        && steps
            .iter()
            .any(|step| command_needs_docker_runtime(step.command.as_str()))
    {
        steps.insert(
            0,
            AssistantPlanStep {
                id: "step-1".to_string(),
                title: "Repair container runtime".to_string(),
                command: "docker_runtime_quick_setup".to_string(),
                args: serde_json::json!({}),
                risk_level: "write".to_string(),
                requires_confirmation: false,
                explain:
                    "Auto-analyzes Docker runtime prerequisites and attempts a local repair flow."
                        .to_string(),
            },
        );
    }

    for (index, step) in steps.iter_mut().enumerate() {
        step.id = format!("step-{}", index + 1);
    }

    steps
}

#[tauri::command]
async fn ai_generate_plan(
    prompt: String,
    profile_id: Option<String>,
    prefer_model: Option<bool>,
) -> Result<AssistantPlanResult, String> {
    if prompt.trim().is_empty() {
        return Err("Prompt cannot be empty".to_string());
    }

    let request_id = next_ai_request_id();
    let settings = load_ai_settings()?;
    let require_confirm = settings.security_policy.destructive_action_confirmation;
    let docker_runtime_available = connect_docker().is_ok();
    let steps = infer_assistant_steps_with_runtime(
        prompt.trim(),
        require_confirm,
        docker_runtime_available,
    );

    let mut notes = "Plan generated from built-in action rules.".to_string();
    let mut strategy = "heuristic".to_string();
    let mut fallback_used = true;

    if prefer_model.unwrap_or(true) {
        if let Ok(profile) = resolve_ai_profile(&settings, profile_id.as_deref()) {
            let hint_messages = vec![
                AiChatMessage {
                    role: "system".to_string(),
                    content: "You are an infra assistant. Summarize action intent in one sentence."
                        .to_string(),
                },
                AiChatMessage {
                    role: "user".to_string(),
                    content: prompt.clone(),
                },
            ];
            if let Ok(summary) = ai_chat_inner(&profile, &hint_messages, 12_000, &request_id).await
            {
                if !summary.text.trim().is_empty() {
                    notes = summary.text.trim().to_string();
                    strategy = "llm+heuristic".to_string();
                    fallback_used = false;
                }
            }
        }
    }

    ai_audit_log(
        "ai_generate_plan",
        "read",
        &request_id,
        &format!("prompt_len={} steps={}", prompt.len(), steps.len()),
    );

    Ok(AssistantPlanResult {
        request_id,
        strategy,
        notes,
        fallback_used,
        steps,
    })
}

#[tauri::command]
async fn assistant_execute_step(
    state: State<'_, AppState>,
    command: String,
    args: serde_json::Value,
    risk_level: Option<String>,
    requires_confirmation: Option<bool>,
    confirmed: Option<bool>,
) -> Result<AssistantStepExecutionResult, String> {
    let settings = load_ai_settings()?;
    let request_id = next_ai_request_id();
    let command = command.trim().to_string();
    let Some(policy) = assistant_command_policy(&command) else {
        let msg = format!("Assistant command '{}' is not allowed", command);
        ai_audit_log("assistant_execute_step", "deny", &request_id, &msg);
        return Err(msg);
    };

    if let Some(client_risk) = risk_level.as_deref() {
        if client_risk != policy.risk_level {
            let msg = format!(
                "Assistant risk level mismatch for '{}': client='{}' server='{}'",
                command, client_risk, policy.risk_level
            );
            ai_audit_log("assistant_execute_step", "deny", &request_id, &msg);
            return Err(msg);
        }
    }

    let needs_confirmation = policy.always_confirm
        || (settings.security_policy.destructive_action_confirmation
            && requires_confirmation.unwrap_or(false));
    if needs_confirmation && !confirmed.unwrap_or(false) {
        let msg = format!(
            "Assistant command '{}' requires explicit confirmation",
            command
        );
        ai_audit_log("assistant_execute_step", "deny", &request_id, &msg);
        return Err(msg);
    }

    let arg_keys = args
        .as_object()
        .map(|obj| obj.keys().cloned().collect::<Vec<_>>().join(","))
        .unwrap_or_default();
    let args_map = assistant_arg_map(&args)?;

    let output = match command.as_str() {
        "list_containers" => {
            let items = list_containers().await?;
            serde_json::to_value(items).map_err(|e| e.to_string())?
        }
        "start_container" => {
            let id = assistant_arg_string(args_map, "id")?;
            start_container(id).await?;
            serde_json::json!({ "ok": true })
        }
        "stop_container" => {
            let id = assistant_arg_string(args_map, "id")?;
            stop_container(id).await?;
            serde_json::json!({ "ok": true })
        }
        "remove_container" => {
            let id = assistant_arg_string(args_map, "id")?;
            remove_container(id).await?;
            serde_json::json!({ "ok": true })
        }
        "vm_list" => {
            let items = vm_list_inner(state.inner()).await?;
            serde_json::to_value(items).map_err(|e| e.to_string())?
        }
        "vm_start" => {
            let id = assistant_arg_string(args_map, "id")?;
            vm_start_inner(state.inner(), id).await?;
            serde_json::json!({ "ok": true })
        }
        "vm_stop" => {
            let id = assistant_arg_string(args_map, "id")?;
            vm_stop_inner(state.inner(), id).await?;
            serde_json::json!({ "ok": true })
        }
        "vm_delete" => {
            let id = assistant_arg_string(args_map, "id")?;
            vm_delete_inner(state.inner(), id).await?;
            serde_json::json!({ "ok": true })
        }
        "k8s_list_pods" => {
            let namespace = assistant_arg_optional_string(args_map, "namespace")?
                .map(|v| v.trim().to_string())
                .filter(|v| !v.is_empty());
            let pods = k8s_list_pods(namespace).await?;
            serde_json::to_value(pods).map_err(|e| e.to_string())?
        }
        "docker_runtime_quick_setup" => {
            let result = docker_runtime_quick_setup().await?;
            serde_json::to_value(result).map_err(|e| e.to_string())?
        }
        _ => unreachable!("assistant command policy should reject unknown commands"),
    };

    ai_audit_log(
        "assistant_execute_step",
        "write",
        &request_id,
        &format!(
            "command={} risk={} args_keys=[{}] confirm={}",
            command,
            policy.risk_level,
            arg_keys,
            confirmed.unwrap_or(false)
        ),
    );

    Ok(AssistantStepExecutionResult {
        ok: true,
        request_id,
        command,
        risk_level: policy.risk_level.to_string(),
        output,
    })
}

#[tauri::command]
fn mcp_check_access(
    action: String,
    token: Option<String>,
    requires_confirmation: Option<bool>,
    confirmed: Option<bool>,
) -> Result<McpAccessCheckResult, String> {
    let settings = load_ai_settings()?;
    let policy = settings.security_policy;
    let request_id = next_ai_request_id();
    let action_policy = mcp_action_policy(&action);
    let confirmation_required =
        action_policy.requires_confirmation && policy.destructive_action_confirmation;

    if !policy.mcp_remote_enabled {
        let msg = "MCP remote is disabled by policy".to_string();
        if policy.mcp_audit_enabled {
            ai_audit_log("mcp_check_access", "deny", &request_id, &msg);
        }
        return Ok(McpAccessCheckResult {
            allowed: false,
            request_id,
            message: msg,
            risk_level: action_policy.risk_level.to_string(),
            requires_confirmation: confirmation_required,
        });
    }

    if !policy.mcp_allowed_actions.is_empty()
        && !policy
            .mcp_allowed_actions
            .iter()
            .any(|item| item == &action)
    {
        let msg = format!("Action '{}' is not in MCP whitelist", action);
        if policy.mcp_audit_enabled {
            ai_audit_log("mcp_check_access", "deny", &request_id, &msg);
        }
        return Ok(McpAccessCheckResult {
            allowed: false,
            request_id,
            message: msg,
            risk_level: action_policy.risk_level.to_string(),
            requires_confirmation: confirmation_required,
        });
    }

    if !policy.mcp_auth_token_ref.trim().is_empty() {
        let expected = secret_get(policy.mcp_auth_token_ref.trim())?;
        if let Some(expected_token) = expected {
            if token.as_deref().unwrap_or("") != expected_token {
                let msg = "MCP token verification failed".to_string();
                if policy.mcp_audit_enabled {
                    ai_audit_log("mcp_check_access", "deny", &request_id, &msg);
                }
                return Ok(McpAccessCheckResult {
                    allowed: false,
                    request_id,
                    message: msg,
                    risk_level: action_policy.risk_level.to_string(),
                    requires_confirmation: confirmation_required,
                });
            }
        }
    }

    if !mcp_confirmation_satisfied(
        action_policy,
        policy.destructive_action_confirmation,
        requires_confirmation,
        confirmed,
    ) {
        let msg = format!(
            "MCP action '{}' (risk={}) requires explicit confirmation",
            action, action_policy.risk_level
        );
        if policy.mcp_audit_enabled {
            ai_audit_log("mcp_check_access", "deny", &request_id, &msg);
        }
        return Ok(McpAccessCheckResult {
            allowed: false,
            request_id,
            message: msg,
            risk_level: action_policy.risk_level.to_string(),
            requires_confirmation: confirmation_required,
        });
    }

    if policy.mcp_audit_enabled {
        ai_audit_log(
            "mcp_check_access",
            "allow",
            &request_id,
            &format!(
                "action={} allowed=true risk={} confirm_required={} confirmed={}",
                action,
                action_policy.risk_level,
                confirmation_required,
                confirmed.unwrap_or(false)
            ),
        );
    }

    Ok(McpAccessCheckResult {
        allowed: true,
        request_id,
        message: "MCP access granted".to_string(),
        risk_level: action_policy.risk_level.to_string(),
        requires_confirmation: confirmation_required,
    })
}

#[tauri::command]
async fn docker_runtime_quick_setup() -> Result<DockerRuntimeSetupResult, String> {
    let request_id = next_ai_request_id();
    if connect_docker().is_ok() {
        return Ok(DockerRuntimeSetupResult {
            ok: true,
            request_id,
            message: "Docker runtime is already available.".to_string(),
        });
    }

    #[cfg(target_os = "macos")]
    let setup_result = tokio::task::spawn_blocking(docker_runtime_quick_setup_macos)
        .await
        .map_err(|e| format!("Runtime setup task failed: {}", e))?;

    #[cfg(not(target_os = "macos"))]
    let setup_result: Result<String, String> = Err(
        "One-click runtime setup currently supports macOS Docker CLI bootstrap only. Install Docker CLI and expose a Docker-compatible socket."
            .to_string(),
    );

    let (ok, message) = match setup_result {
        Ok(msg) => (true, msg),
        Err(msg) => (false, msg),
    };

    ai_audit_log(
        "docker_runtime_quick_setup",
        if ok { "write" } else { "error" },
        &request_id,
        &format!("ok={} message={}", ok, message),
    );

    Ok(DockerRuntimeSetupResult {
        ok,
        request_id,
        message,
    })
}

fn default_agent_cli_presets() -> Vec<AgentCliPreset> {
    vec![
        AgentCliPreset {
            id: "codex".to_string(),
            name: "OpenAI Codex CLI".to_string(),
            description: "Run codex in non-interactive mode".to_string(),
            command: "codex".to_string(),
            args_template: vec!["exec".to_string(), "{{prompt}}".to_string()],
            timeout_sec: 180,
            dangerous: false,
        },
        AgentCliPreset {
            id: "claude".to_string(),
            name: "Claude Code CLI".to_string(),
            description: "Invoke claude with prompt text".to_string(),
            command: "claude".to_string(),
            args_template: vec!["--print".to_string(), "{{prompt}}".to_string()],
            timeout_sec: 180,
            dangerous: false,
        },
        AgentCliPreset {
            id: "openclaw".to_string(),
            name: "OpenClaw CLI".to_string(),
            description: "Invoke openclaw cli prompt mode".to_string(),
            command: "openclaw".to_string(),
            args_template: vec![
                "run".to_string(),
                "--prompt".to_string(),
                "{{prompt}}".to_string(),
            ],
            timeout_sec: 180,
            dangerous: false,
        },
        AgentCliPreset {
            id: "gemini".to_string(),
            name: "Gemini CLI".to_string(),
            description: "Invoke gemini cli prompt mode".to_string(),
            command: "gemini".to_string(),
            args_template: vec!["--prompt".to_string(), "{{prompt}}".to_string()],
            timeout_sec: 180,
            dangerous: false,
        },
        AgentCliPreset {
            id: "qwen".to_string(),
            name: "Qwen CLI".to_string(),
            description: "Invoke qwen command line client".to_string(),
            command: "qwen".to_string(),
            args_template: vec!["--prompt".to_string(), "{{prompt}}".to_string()],
            timeout_sec: 180,
            dangerous: false,
        },
    ]
}

#[tauri::command]
fn agent_cli_list_presets() -> Vec<AgentCliPreset> {
    default_agent_cli_presets()
}

fn build_command_line(command: &str, args: &[String]) -> String {
    if args.is_empty() {
        command.to_string()
    } else {
        format!("{} {}", command, args.join(" "))
    }
}

fn is_command_allowed(allowlist: &[String], command: &str) -> bool {
    if allowlist.is_empty() {
        return true;
    }
    let command_name = std::path::Path::new(command)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(command);
    allowlist.iter().any(|item| item == command_name)
}

#[tauri::command]
async fn agent_cli_run(
    preset_id: Option<String>,
    command: Option<String>,
    args: Option<Vec<String>>,
    prompt: Option<String>,
    dry_run: bool,
    timeout_sec: Option<u64>,
) -> Result<AgentCliRunResult, String> {
    let settings = load_ai_settings()?;
    let request_id = next_ai_request_id();

    let presets = default_agent_cli_presets();
    let (resolved_command, resolved_args, preset_timeout, dangerous) =
        if let Some(preset_id) = preset_id {
            let preset = presets
                .into_iter()
                .find(|item| item.id == preset_id)
                .ok_or_else(|| format!("Unknown preset: {}", preset_id))?;
            let prompt_text = prompt.unwrap_or_default();
            let args = preset
                .args_template
                .iter()
                .map(|arg| arg.replace("{{prompt}}", &prompt_text))
                .collect::<Vec<_>>();
            (preset.command, args, preset.timeout_sec, preset.dangerous)
        } else {
            let cmd =
                command.ok_or_else(|| "command is required when preset_id is empty".to_string())?;
            (cmd, args.unwrap_or_default(), 180, false)
        };

    if !is_command_allowed(
        &settings.security_policy.cli_command_allowlist,
        &resolved_command,
    ) {
        let detail = format!("command '{}' blocked by CLI allowlist", resolved_command);
        ai_audit_log("agent_cli_run", "deny", &request_id, &detail);
        return Err(detail);
    }

    if dangerous && settings.security_policy.destructive_action_confirmation {
        ai_audit_log(
            "agent_cli_run",
            "deny",
            &request_id,
            "dangerous preset blocked by destructive_action_confirmation",
        );
        return Err(
            "Dangerous preset blocked by policy (disable confirmation policy to proceed)"
                .to_string(),
        );
    }

    let command_line = build_command_line(&resolved_command, &resolved_args);
    if dry_run {
        ai_audit_log(
            "agent_cli_run",
            "read",
            &request_id,
            &format!("dry_run=true command={}", command_line),
        );
        return Ok(AgentCliRunResult {
            ok: true,
            request_id,
            command_line,
            exit_code: 0,
            stdout: String::new(),
            stderr: String::new(),
            duration_ms: 0,
        });
    }

    let timeout = timeout_sec.unwrap_or(preset_timeout).max(1);
    let start = std::time::Instant::now();

    let mut command_builder = tokio::process::Command::new(&resolved_command);
    command_builder
        .args(&resolved_args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    let child = command_builder
        .spawn()
        .map_err(|e| format!("Failed to spawn command '{}': {}", resolved_command, e))?;

    let output = match tokio::time::timeout(Duration::from_secs(timeout), child.wait_with_output())
        .await
    {
        Ok(v) => v.map_err(|e| format!("Failed to wait command '{}': {}", resolved_command, e))?,
        Err(_) => {
            ai_audit_log(
                "agent_cli_run",
                "error",
                &request_id,
                &format!("command timeout={}s command={}", timeout, command_line),
            );
            return Err(format!("Command timed out after {} seconds", timeout));
        }
    };

    let duration_ms = start.elapsed().as_millis();
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let exit_code = output.status.code().unwrap_or(-1);
    let ok = output.status.success();
    ai_audit_log(
        "agent_cli_run",
        if ok { "write" } else { "error" },
        &request_id,
        &format!(
            "command={} exit_code={} duration_ms={}",
            command_line, exit_code, duration_ms
        ),
    );

    Ok(AgentCliRunResult {
        ok,
        request_id,
        command_line,
        exit_code,
        stdout,
        stderr,
        duration_ms,
    })
}

#[cfg(test)]
mod ai_tests {
    use super::{
        assistant_arg_optional_string, assistant_arg_string, assistant_command_policy,
        default_ai_settings, infer_assistant_steps, infer_assistant_steps_with_runtime,
        is_command_allowed, mcp_action_policy, mcp_confirmation_satisfied, normalize_ai_settings,
        redact_sensitive,
    };

    #[test]
    fn infer_plan_marks_destructive_actions() {
        let steps = infer_assistant_steps("delete container web", true);
        assert!(!steps.is_empty());
        assert_eq!(steps[0].command, "remove_container");
        assert_eq!(steps[0].risk_level, "destructive");
        assert!(steps[0].requires_confirmation);
    }

    #[test]
    fn infer_plan_fallback_has_two_read_steps() {
        let steps = infer_assistant_steps("show me infra context", true);
        assert_eq!(steps.len(), 2);
        assert_eq!(steps[0].risk_level, "read");
        assert_eq!(steps[1].risk_level, "read");
    }

    #[test]
    fn redact_sensitive_rewrites_auth_markers() {
        let input = "Authorization: Bearer sk-test-abc";
        let output = redact_sensitive(input.to_string());
        assert!(output.contains("Authorization[redacted]"));
        assert!(output.contains("Bearer [redacted]"));
    }

    #[test]
    fn assistant_policy_limits_command_surface() {
        let destructive = assistant_command_policy("vm_delete").expect("policy should exist");
        assert_eq!(destructive.risk_level, "destructive");
        assert!(destructive.always_confirm);

        let repair =
            assistant_command_policy("docker_runtime_quick_setup").expect("policy should exist");
        assert_eq!(repair.risk_level, "write");
        assert!(!repair.always_confirm);

        let read = assistant_command_policy("list_containers").expect("policy should exist");
        assert_eq!(read.risk_level, "read");
        assert!(!read.always_confirm);

        assert!(assistant_command_policy("docker_run").is_none());
    }

    #[test]
    fn assistant_arg_helpers_parse_required_and_optional_values() {
        let args = serde_json::json!({
            "id": "vm-1",
            "namespace": "default",
            "nullable": null
        });
        let map = args.as_object().expect("object");
        assert_eq!(
            assistant_arg_string(map, "id").expect("id"),
            "vm-1".to_string()
        );
        assert_eq!(
            assistant_arg_optional_string(map, "namespace").expect("namespace"),
            Some("default".to_string())
        );
        assert_eq!(
            assistant_arg_optional_string(map, "nullable").expect("nullable"),
            None
        );
    }

    #[test]
    fn cli_allowlist_matches_binary_file_name() {
        let allowlist = vec!["openclaw".to_string(), "codex".to_string()];
        assert!(is_command_allowed(&allowlist, "/usr/local/bin/openclaw"));
        assert!(!is_command_allowed(&allowlist, "/usr/local/bin/bash"));
    }

    #[test]
    fn default_ai_settings_exposes_skill_scaffold_entries() {
        let settings = default_ai_settings();
        assert!(!settings.skills.is_empty());
        assert!(settings
            .skills
            .iter()
            .any(|skill| skill.id == "agent-cli-openclaw-plan"));
    }

    #[test]
    fn normalize_ai_settings_rebuilds_skills_when_empty() {
        let mut settings = default_ai_settings();
        settings.skills.clear();
        let normalized = normalize_ai_settings(settings);
        assert!(!normalized.skills.is_empty());
    }

    #[derive(Clone, Copy)]
    struct CoreScenario {
        name: &'static str,
        prompt: &'static str,
        expected_command: &'static str,
        expected_risk: &'static str,
        expected_confirm: bool,
    }

    #[test]
    fn assistant_core_scenarios_success_rate() {
        let scenarios = vec![
            CoreScenario {
                name: "container_delete",
                prompt: "delete container web",
                expected_command: "remove_container",
                expected_risk: "destructive",
                expected_confirm: true,
            },
            CoreScenario {
                name: "container_remove",
                prompt: "remove container api",
                expected_command: "remove_container",
                expected_risk: "destructive",
                expected_confirm: true,
            },
            CoreScenario {
                name: "container_stop",
                prompt: "stop container gateway",
                expected_command: "stop_container",
                expected_risk: "write",
                expected_confirm: true,
            },
            CoreScenario {
                name: "container_start",
                prompt: "start container worker",
                expected_command: "start_container",
                expected_risk: "write",
                expected_confirm: true,
            },
            CoreScenario {
                name: "container_list",
                prompt: "show container status overview",
                expected_command: "list_containers",
                expected_risk: "read",
                expected_confirm: false,
            },
            CoreScenario {
                name: "container_k8s_dual",
                prompt: "container and kubernetes pod diagnosis",
                expected_command: "k8s_list_pods",
                expected_risk: "read",
                expected_confirm: false,
            },
            CoreScenario {
                name: "vm_delete",
                prompt: "delete vm dev",
                expected_command: "vm_delete",
                expected_risk: "destructive",
                expected_confirm: true,
            },
            CoreScenario {
                name: "vm_remove",
                prompt: "remove vm qa",
                expected_command: "vm_delete",
                expected_risk: "destructive",
                expected_confirm: true,
            },
            CoreScenario {
                name: "vm_stop",
                prompt: "stop vm alpha",
                expected_command: "vm_stop",
                expected_risk: "write",
                expected_confirm: true,
            },
            CoreScenario {
                name: "vm_start",
                prompt: "start vm alpha",
                expected_command: "vm_start",
                expected_risk: "write",
                expected_confirm: true,
            },
            CoreScenario {
                name: "vm_list",
                prompt: "show vm status overview",
                expected_command: "vm_list",
                expected_risk: "read",
                expected_confirm: false,
            },
            CoreScenario {
                name: "vm_k8s_dual",
                prompt: "vm and pod health check",
                expected_command: "k8s_list_pods",
                expected_risk: "read",
                expected_confirm: false,
            },
            CoreScenario {
                name: "k8s_pods_status",
                prompt: "kubernetes pods status",
                expected_command: "k8s_list_pods",
                expected_risk: "read",
                expected_confirm: false,
            },
            CoreScenario {
                name: "k8s_pods_crashloop",
                prompt: "k8s pod crashloop check",
                expected_command: "k8s_list_pods",
                expected_risk: "read",
                expected_confirm: false,
            },
            CoreScenario {
                name: "fallback_infra_context",
                prompt: "show me infra context",
                expected_command: "list_containers",
                expected_risk: "read",
                expected_confirm: false,
            },
            CoreScenario {
                name: "fallback_general_diagnostics",
                prompt: "need full diagnostics",
                expected_command: "list_containers",
                expected_risk: "read",
                expected_confirm: false,
            },
            CoreScenario {
                name: "delete_container_and_vm",
                prompt: "delete container and vm now",
                expected_command: "vm_delete",
                expected_risk: "destructive",
                expected_confirm: true,
            },
            CoreScenario {
                name: "start_container_and_vm",
                prompt: "start container and vm together",
                expected_command: "vm_start",
                expected_risk: "write",
                expected_confirm: true,
            },
            CoreScenario {
                name: "container_logs_and_pod",
                prompt: "container logs and pod status",
                expected_command: "list_containers",
                expected_risk: "read",
                expected_confirm: false,
            },
            CoreScenario {
                name: "pod_investigation",
                prompt: "pod issue investigation",
                expected_command: "k8s_list_pods",
                expected_risk: "read",
                expected_confirm: false,
            },
        ];

        let mut passed = 0usize;
        for scenario in &scenarios {
            let steps = infer_assistant_steps(scenario.prompt, true);
            let matched = steps
                .iter()
                .find(|s| s.command == scenario.expected_command)
                .map(|s| {
                    s.risk_level == scenario.expected_risk
                        && s.requires_confirmation == scenario.expected_confirm
                })
                .unwrap_or(false);
            if matched {
                passed += 1;
            } else {
                eprintln!(
                    "scenario failed: {} prompt='{}' expected command={} risk={} confirm={} actual={:?}",
                    scenario.name,
                    scenario.prompt,
                    scenario.expected_command,
                    scenario.expected_risk,
                    scenario.expected_confirm,
                    steps
                );
            }
        }

        let total = scenarios.len();
        let success_rate = passed as f64 / total as f64;
        assert!(
            success_rate >= 0.95,
            "assistant core scenarios below threshold: passed={} total={} rate={:.2}",
            passed,
            total,
            success_rate
        );
    }

    #[test]
    fn destructive_steps_always_require_confirmation() {
        let prompts = vec![
            "delete container web",
            "remove container api",
            "delete vm dev",
            "remove vm qa",
            "delete container and vm now",
        ];
        for prompt in prompts {
            let steps = infer_assistant_steps(prompt, true);
            for step in steps {
                if step.risk_level == "destructive" {
                    assert!(
                        step.requires_confirmation,
                        "destructive step without confirmation: prompt='{}' command='{}'",
                        prompt, step.command
                    );
                }
            }
        }
    }

    #[test]
    fn infer_plan_injects_runtime_repair_when_docker_is_unavailable() {
        let steps = infer_assistant_steps_with_runtime("show container status", true, false);
        assert!(!steps.is_empty());
        assert_eq!(steps[0].command, "docker_runtime_quick_setup");
        assert_eq!(steps[0].risk_level, "write");
        assert!(!steps[0].requires_confirmation);
        assert!(
            steps.iter().any(|step| step.command == "list_containers"),
            "container read step should be preserved after prepending repair step"
        );
    }

    #[test]
    fn infer_plan_skips_runtime_repair_when_runtime_is_available() {
        let steps = infer_assistant_steps_with_runtime("show container status", true, true);
        assert!(!steps.is_empty());
        assert_ne!(steps[0].command, "docker_runtime_quick_setup");
    }

    #[test]
    fn mcp_policy_classifies_risk_levels() {
        let destructive = mcp_action_policy("k8s.delete_pod");
        assert_eq!(destructive.risk_level, "destructive");
        assert!(destructive.requires_confirmation);

        let write = mcp_action_policy("vm.restart");
        assert_eq!(write.risk_level, "write");
        assert!(!write.requires_confirmation);

        let read = mcp_action_policy("k8s.list_pods");
        assert_eq!(read.risk_level, "read");
        assert!(!read.requires_confirmation);
    }

    #[test]
    fn mcp_destructive_confirmation_needs_explicit_ack() {
        let destructive = mcp_action_policy("container.remove");
        assert!(!mcp_confirmation_satisfied(destructive, true, None, None));
        assert!(!mcp_confirmation_satisfied(
            destructive,
            true,
            Some(true),
            Some(false)
        ));
        assert!(mcp_confirmation_satisfied(
            destructive,
            true,
            Some(true),
            Some(true)
        ));

        let read = mcp_action_policy("k8s.list_pods");
        assert!(mcp_confirmation_satisfied(read, true, None, None));
    }
}

// ── Auto-update ────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize, Clone)]
struct UpdateInfo {
    available: bool,
    current_version: String,
    latest_version: String,
    release_notes: String,
    download_url: String,
}

#[tauri::command]
async fn check_update() -> Result<UpdateInfo, String> {
    let current_version = env!("CARGO_PKG_VERSION");

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|e| format!("Failed to create HTTP client: {}", e))?;

    // Use redirect-based approach first (no rate limit), fall back to API
    let tag = match client
        .head("https://github.com/coder-hhx/CrateBay/releases/latest")
        .header("User-Agent", format!("CrateBay/{}", current_version))
        .send()
        .await
    {
        Ok(resp) if resp.status().is_redirection() => {
            // Location header: https://github.com/.../releases/tag/v1.2.3
            resp.headers()
                .get("location")
                .and_then(|v| v.to_str().ok())
                .filter(|url| url.contains("/releases/tag/"))
                .and_then(|url| url.rsplit('/').next())
                .map(|t| t.trim_start_matches('v').to_string())
                .unwrap_or_default()
        }
        _ => String::new(),
    };

    // If redirect approach failed, try API as fallback
    let (tag, release_notes, html_url) = if tag.is_empty() {
        let resp = client
            .get("https://api.github.com/repos/coder-hhx/CrateBay/releases/latest")
            .header("User-Agent", format!("CrateBay/{}", current_version))
            .header("Accept", "application/vnd.github.v3+json")
            .send()
            .await
            .map_err(|e| format!("Failed to fetch release info: {}", e))?;

        if resp.status() == reqwest::StatusCode::NOT_FOUND {
            // No releases published yet — treat as up-to-date
            return Ok(UpdateInfo {
                available: false,
                current_version: current_version.to_string(),
                latest_version: current_version.to_string(),
                release_notes: String::new(),
                download_url: String::new(),
            });
        }

        if !resp.status().is_success() {
            return Err(format!("GitHub API returned status {}", resp.status()));
        }

        let body: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| format!("Failed to parse release info: {}", e))?;

        let api_tag = body["tag_name"]
            .as_str()
            .unwrap_or("")
            .trim_start_matches('v')
            .to_string();
        let notes = body["body"].as_str().unwrap_or("").to_string();
        let url = body["html_url"].as_str().unwrap_or("").to_string();
        (api_tag, notes, url)
    } else {
        let url = format!(
            "https://github.com/coder-hhx/CrateBay/releases/tag/v{}",
            tag
        );
        (tag, String::new(), url)
    };

    let available = !tag.is_empty() && tag != current_version;

    Ok(UpdateInfo {
        available,
        current_version: current_version.to_string(),
        latest_version: if tag.is_empty() {
            current_version.to_string()
        } else {
            tag
        },
        release_notes,
        download_url: html_url,
    })
}

#[tauri::command]
async fn open_release_page(url: String) -> Result<(), String> {
    open::that(&url).map_err(|e| format!("Failed to open URL: {}", e))
}

#[tauri::command]
async fn set_window_theme(window: tauri::WebviewWindow, theme: String) -> Result<(), String> {
    let t = match theme.as_str() {
        "light" => Some(tauri::Theme::Light),
        "dark" => Some(tauri::Theme::Dark),
        _ => None,
    };
    window.set_theme(t).map_err(|e| e.to_string())
}

pub fn run() {
    cratebay_core::logging::init();
    #[allow(unused_mut)]
    let mut builder = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_shell::init());

    // Enable MCP debug plugin only in debug builds
    #[cfg(debug_assertions)]
    {
        let mcp_bind = std::env::var("CRATEBAY_MCP_BIND").unwrap_or_else(|_| "127.0.0.1".into());
        let mcp_port = std::env::var("CRATEBAY_MCP_PORT")
            .ok()
            .and_then(|v| v.parse::<u16>().ok())
            .unwrap_or(9223);

        info!(
            "Debug build detected: enabling MCP bridge at {}:{}",
            mcp_bind, mcp_port
        );
        builder = builder.plugin(
            tauri_plugin_mcp_bridge::Builder::new()
                .bind_address(&mcp_bind)
                .base_port(mcp_port)
                .build(),
        );
    }

    builder
        .manage(AppState {
            hv: cratebay_core::create_hypervisor(),
            grpc_addr: grpc_addr(),
            daemon: Mutex::new(None),
            daemon_ready: Mutex::new(false),
            log_stream_handles: Mutex::new(HashMap::new()),
        })
        .setup(|app| {
            // ── System tray ─────────────────────────────────────────────
            let app_handle = app.handle().clone();
            let menu = build_tray_menu(&app_handle, 0, 0)?;

            TrayIconBuilder::with_id("main-tray")
                .icon(tauri::include_image!("icons/tray-icon.png"))
                .icon_as_template(true)
                .tooltip("CrateBay")
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

            // Set webview background color to match dark theme (prevents white flash on resize)
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.set_background_color(Some(tauri::window::Color(15, 17, 26, 255)));
                // Hide native title bar text (macOS/Linux with decorations: true)
                let _ = window.set_title("");
                // Enforce minimum window size (decorations:false may not honor config minWidth/minHeight)
                let _ = window.set_min_size(Some(tauri::LogicalSize::new(1100.0, 650.0)));
            }

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
            container_logs_stream,
            container_logs_stream_stop,
            container_exec,
            container_exec_interactive_cmd,
            container_env,
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
            vm_stats,
            volume_list,
            volume_create,
            volume_inspect,
            volume_remove,
            image_list,
            image_remove,
            image_tag,
            image_inspect,
            k3s_status,
            k3s_install,
            k3s_start,
            k3s_stop,
            k3s_uninstall,
            k8s_list_namespaces,
            k8s_list_pods,
            k8s_list_services,
            k8s_list_deployments,
            k8s_pod_logs,
            ollama_status,
            ollama_list_models,
            load_ai_settings,
            save_ai_settings,
            validate_ai_profile,
            ai_secret_set,
            ai_secret_delete,
            ai_secret_exists,
            ai_chat,
            ai_test_connection,
            ai_generate_plan,
            assistant_execute_step,
            mcp_check_access,
            docker_runtime_quick_setup,
            agent_cli_list_presets,
            agent_cli_run,
            check_update,
            open_release_page,
            set_window_theme
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
