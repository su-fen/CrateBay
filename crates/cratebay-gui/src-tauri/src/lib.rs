use bollard::container::{
    Config, CreateContainerOptions, InspectContainerOptions, ListContainersOptions, LogOutput,
    LogsOptions, RemoveContainerOptions, StartContainerOptions, StatsOptions, StopContainerOptions,
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
use std::collections::{HashMap, HashSet, VecDeque};
use std::fs::OpenOptions;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::async_runtime::JoinHandle;
use tauri::menu::{MenuBuilder, MenuItemBuilder, PredefinedMenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{Emitter, Manager, RunEvent, State, WindowEvent};
use tonic::transport::Channel;
use tracing::{error, info, warn};

use cratebay_core::proto;
use cratebay_core::proto::vm_service_client::VmServiceClient;
use cratebay_core::validation;

mod sandbox;
use sandbox::*;

struct McpServerRuntime {
    child: Option<Child>,
    logs: Arc<Mutex<VecDeque<String>>>,
    started_at: Option<String>,
    exit_code: Option<i32>,
}

pub struct AppState {
    hv: Box<dyn cratebay_core::hypervisor::Hypervisor>,
    grpc_addr: String,
    daemon: Mutex<Option<Child>>,
    daemon_ready: Mutex<bool>,
    log_stream_handles: Mutex<HashMap<String, JoinHandle<()>>>,
    mcp_runtimes: Mutex<HashMap<String, McpServerRuntime>>,
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
        if let Ok(mut runtimes) = self.mcp_runtimes.lock() {
            for runtime in runtimes.values_mut() {
                if let Some(mut child) = runtime.child.take() {
                    let _ = child.kill();
                    let _ = child.wait();
                }
            }
        }

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
    use super::{detect_vm_ssh_port, format_published_ports, vm_login_cmd, PortForwardDto};

    #[test]
    fn format_published_ports_sorts_by_public_then_private() {
        let out = format_published_ports(vec![(443, 443), (80, 8080), (80, 80), (8080, 80)]);
        assert_eq!(out, "80:80, 80:8080, 443:443, 8080:80");
    }

    #[test]
    fn format_published_ports_empty_is_empty() {
        assert_eq!(format_published_ports(vec![]), "");
    }

    #[test]
    fn detect_vm_ssh_port_prefers_guest_port_22_over_tcp() {
        let port = detect_vm_ssh_port(&[
            PortForwardDto {
                host_port: 9090,
                guest_port: 90,
                protocol: "tcp".into(),
            },
            PortForwardDto {
                host_port: 2228,
                guest_port: 22,
                protocol: "tcp".into(),
            },
        ]);
        assert_eq!(port, Some(2228));
    }

    #[test]
    fn vm_login_cmd_falls_back_to_detected_ssh_forward() {
        let cmd = vm_login_cmd(
            "vm-1".into(),
            "root".into(),
            "127.0.0.1".into(),
            None,
            Some(vec![PortForwardDto {
                host_port: 2228,
                guest_port: 22,
                protocol: "tcp".into(),
            }]),
        )
        .expect("login command");
        assert!(cmd.contains("ssh root@127.0.0.1 -p 2228"));
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
        .user_agent(concat!(
            "CrateBay/",
            env!("CARGO_PKG_VERSION"),
            " (+https://github.com/coder-hhx/CrateBay)"
        ))
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

fn detect_vm_ssh_port(port_forwards: &[PortForwardDto]) -> Option<u16> {
    port_forwards
        .iter()
        .find(|pf| pf.guest_port == 22 && pf.protocol.eq_ignore_ascii_case("tcp"))
        .map(|pf| pf.host_port)
}

#[tauri::command]
fn vm_login_cmd(
    name: String,
    user: String,
    host: String,
    port: Option<u16>,
    port_forwards: Option<Vec<PortForwardDto>>,
) -> Result<String, String> {
    let detected_port = port_forwards.as_deref().and_then(detect_vm_ssh_port);
    let Some(port) = port.or(detected_port) else {
        return Err(
            "VM login is not available yet. Add a guest port 22 forward or specify an SSH port."
                .into(),
        );
    };
    Ok(format!(
        "ssh {}@{} -p {}
# VM: {}",
        user, host, port, name
    ))
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

fn ollama_base_url() -> String {
    std::env::var("CRATEBAY_OLLAMA_BASE_URL")
        .ok()
        .map(|value| value.trim().trim_end_matches('/').to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| OLLAMA_BASE_URL.to_string())
}

fn ollama_http_client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .timeout(Duration::from_millis(900))
        .user_agent(concat!(
            "CrateBay/",
            env!("CARGO_PKG_VERSION"),
            " (+https://github.com/coder-hhx/CrateBay)"
        ))
        .build()
        .map_err(|e| e.to_string())
}

#[derive(Debug, Serialize)]
pub struct GpuDeviceDto {
    index: u32,
    name: String,
    utilization_percent: Option<f64>,
    memory_used_bytes: Option<u64>,
    memory_total_bytes: Option<u64>,
    memory_used_human: Option<String>,
    memory_total_human: Option<String>,
    temperature_celsius: Option<f64>,
    power_watts: Option<f64>,
}

#[derive(Debug, Serialize)]
pub struct GpuStatusDto {
    available: bool,
    utilization_supported: bool,
    backend: String,
    message: String,
    devices: Vec<GpuDeviceDto>,
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

fn parse_optional_metric_f64(value: &str) -> Option<f64> {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("n/a") || trimmed == "-" {
        None
    } else {
        trimmed.parse::<f64>().ok()
    }
}

fn parse_optional_metric_u64(value: &str) -> Option<u64> {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("n/a") || trimmed == "-" {
        None
    } else {
        trimmed
            .parse::<u64>()
            .ok()
            .or_else(|| trimmed.parse::<f64>().ok().map(|item| item.round() as u64))
    }
}

fn gpu_status_unavailable(message: impl Into<String>) -> GpuStatusDto {
    GpuStatusDto {
        available: false,
        utilization_supported: false,
        backend: String::new(),
        message: message.into(),
        devices: Vec::new(),
    }
}

fn query_nvidia_gpu_status() -> Result<GpuStatusDto, String> {
    let output = runtime_setup_run(
        "nvidia-smi",
        &[
            "--query-gpu=index,name,utilization.gpu,memory.used,memory.total,temperature.gpu,power.draw",
            "--format=csv,noheader,nounits",
        ],
    )?;
    if !output.status.success() {
        let detail = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(if detail.is_empty() {
            format!("nvidia-smi exited with status {}", output.status)
        } else {
            detail
        });
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut devices = Vec::new();
    for (line_index, line) in stdout.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let cols = trimmed
            .split(',')
            .map(|item| item.trim())
            .collect::<Vec<_>>();
        if cols.len() < 7 {
            continue;
        }
        let index = cols[0].parse::<u32>().unwrap_or(line_index as u32);
        let memory_used_bytes = parse_optional_metric_u64(cols[3]).map(|mb| mb * 1024 * 1024);
        let memory_total_bytes = parse_optional_metric_u64(cols[4]).map(|mb| mb * 1024 * 1024);
        devices.push(GpuDeviceDto {
            index,
            name: cols[1].to_string(),
            utilization_percent: parse_optional_metric_f64(cols[2]),
            memory_used_bytes,
            memory_total_bytes,
            memory_used_human: memory_used_bytes.map(format_bytes_human),
            memory_total_human: memory_total_bytes.map(format_bytes_human),
            temperature_celsius: parse_optional_metric_f64(cols[5]),
            power_watts: parse_optional_metric_f64(cols[6]),
        });
    }

    if devices.is_empty() {
        return Ok(gpu_status_unavailable(
            "nvidia-smi is installed, but no GPU devices were reported.",
        ));
    }

    Ok(GpuStatusDto {
        available: true,
        utilization_supported: true,
        backend: "nvidia-smi".to_string(),
        message: format!(
            "Live GPU telemetry is available for {} device(s).",
            devices.len()
        ),
        devices,
    })
}

#[derive(Debug)]
struct NvidiaGpuInventory {
    index: u32,
    uuid: String,
    name: String,
}

#[derive(Debug)]
struct NvidiaGpuProcessSample {
    gpu_uuid: String,
    pid: u32,
    process_name: String,
    memory_used_bytes: Option<u64>,
}

fn query_nvidia_gpu_inventory() -> Result<Vec<NvidiaGpuInventory>, String> {
    let output = runtime_setup_run(
        "nvidia-smi",
        &[
            "--query-gpu=index,uuid,name",
            "--format=csv,noheader,nounits",
        ],
    )?;
    if !output.status.success() {
        let detail = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(if detail.is_empty() {
            format!("nvidia-smi exited with status {}", output.status)
        } else {
            detail
        });
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut devices = Vec::new();
    for (line_index, line) in stdout.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let cols = trimmed
            .split(',')
            .map(|item| item.trim())
            .collect::<Vec<_>>();
        if cols.len() < 3 {
            continue;
        }
        devices.push(NvidiaGpuInventory {
            index: cols[0].parse::<u32>().unwrap_or(line_index as u32),
            uuid: cols[1].to_string(),
            name: cols[2].to_string(),
        });
    }
    Ok(devices)
}

fn query_nvidia_compute_processes() -> Result<Vec<NvidiaGpuProcessSample>, String> {
    let output = runtime_setup_run(
        "nvidia-smi",
        &[
            "--query-compute-apps=gpu_uuid,pid,process_name,used_gpu_memory",
            "--format=csv,noheader,nounits",
        ],
    )?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!(
        "{}
{}",
        stdout.trim(),
        stderr.trim()
    )
    .to_lowercase();
    if combined.contains("no running compute processes found") {
        return Ok(Vec::new());
    }

    if !output.status.success() {
        let detail = stderr.trim();
        return Err(if detail.is_empty() {
            format!("nvidia-smi exited with status {}", output.status)
        } else {
            detail.to_string()
        });
    }

    let mut processes = Vec::new();
    for line in stdout.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let cols = trimmed
            .split(',')
            .map(|item| item.trim())
            .collect::<Vec<_>>();
        if cols.len() < 4 {
            continue;
        }
        let Ok(pid) = cols[1].parse::<u32>() else {
            continue;
        };
        processes.push(NvidiaGpuProcessSample {
            gpu_uuid: cols[0].to_string(),
            pid,
            process_name: cols[2].to_string(),
            memory_used_bytes: parse_optional_metric_u64(cols[3]).map(|mb| mb * 1024 * 1024),
        });
    }

    Ok(processes)
}

#[cfg(target_os = "linux")]
fn linux_process_belongs_to_container(pid: u32, container_id: &str, short_id: &str) -> bool {
    let path = PathBuf::from("/proc").join(pid.to_string()).join("cgroup");
    let Ok(raw) = std::fs::read_to_string(path) else {
        return false;
    };
    raw.lines()
        .any(|line| line.contains(container_id) || line.contains(short_id))
}

#[cfg(not(target_os = "linux"))]
fn linux_process_belongs_to_container(_pid: u32, _container_id: &str, _short_id: &str) -> bool {
    false
}

#[cfg(target_os = "macos")]
fn query_macos_gpu_status() -> Result<GpuStatusDto, String> {
    let output = runtime_setup_run("system_profiler", &["SPDisplaysDataType", "-json"])?;
    if !output.status.success() {
        let detail = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(if detail.is_empty() {
            format!("system_profiler exited with status {}", output.status)
        } else {
            detail
        });
    }

    let value: serde_json::Value =
        serde_json::from_slice(&output.stdout).map_err(|e| format!("Invalid GPU JSON: {}", e))?;
    let items = value
        .get("SPDisplaysDataType")
        .and_then(|entry| entry.as_array())
        .cloned()
        .unwrap_or_default();

    let mut devices = Vec::new();
    for (index, item) in items.iter().enumerate() {
        let Some(obj) = item.as_object() else {
            continue;
        };
        let name = obj
            .get("sppci_model")
            .and_then(|entry| entry.as_str())
            .or_else(|| obj.get("_name").and_then(|entry| entry.as_str()))
            .or_else(|| {
                obj.get("spdisplays_vendor")
                    .and_then(|entry| entry.as_str())
            })
            .unwrap_or("GPU")
            .trim()
            .to_string();
        let memory_total_human = obj
            .get("spdisplays_vram")
            .and_then(|entry| entry.as_str())
            .or_else(|| {
                obj.get("spdisplays_vram_shared")
                    .and_then(|entry| entry.as_str())
            })
            .map(|entry| entry.trim().to_string());
        devices.push(GpuDeviceDto {
            index: index as u32,
            name,
            utilization_percent: None,
            memory_used_bytes: None,
            memory_total_bytes: None,
            memory_used_human: None,
            memory_total_human,
            temperature_celsius: None,
            power_watts: None,
        });
    }

    if devices.is_empty() {
        return Ok(gpu_status_unavailable(
            "No GPU devices were detected by system_profiler.",
        ));
    }

    Ok(GpuStatusDto {
        available: true,
        utilization_supported: false,
        backend: "system_profiler".to_string(),
        message: "GPU devices are detected, but live utilization is unavailable on this platform."
            .to_string(),
        devices,
    })
}

#[tauri::command]
async fn gpu_status() -> Result<GpuStatusDto, String> {
    tokio::task::spawn_blocking(|| {
        if runtime_setup_command_exists("nvidia-smi") {
            return query_nvidia_gpu_status().or_else(|error| {
                Ok(gpu_status_unavailable(format!(
                    "GPU telemetry probe failed: {}",
                    error
                )))
            });
        }

        #[cfg(target_os = "macos")]
        {
            if runtime_setup_command_exists("system_profiler") {
                return query_macos_gpu_status().or_else(|error| {
                    Ok(gpu_status_unavailable(format!(
                        "GPU telemetry probe failed: {}",
                        error
                    )))
                });
            }
        }

        Ok(gpu_status_unavailable(
            "GPU telemetry is unavailable on this machine. Install NVIDIA tooling or use a supported local runtime.",
        ))
    })
    .await
    .map_err(|e| format!("gpu_status task failed: {}", e))?
}

async fn ollama_check_installed() -> bool {
    tokio::task::spawn_blocking(|| Command::new("ollama").arg("--version").output().is_ok())
        .await
        .unwrap_or(false)
}

async fn ollama_check_running() -> Result<String, String> {
    let client = ollama_http_client()?;
    let base_url = ollama_base_url();
    let url = format!("{}/api/version", base_url.trim_end_matches('/'));
    let resp = client.get(&url).send().await.map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(format!(
            "ollama version endpoint returned {}",
            resp.status()
        ));
    }
    let body: OllamaVersionResponse = resp.json().await.map_err(|e| e.to_string())?;
    Ok(body.version.unwrap_or_default())
}

#[tauri::command]
async fn ollama_status() -> Result<OllamaStatusDto, String> {
    let installed = ollama_check_installed().await;
    let base_url = ollama_base_url();
    match ollama_check_running().await {
        Ok(version) => Ok(OllamaStatusDto {
            installed,
            running: true,
            version,
            base_url,
        }),
        Err(_) => Ok(OllamaStatusDto {
            installed,
            running: false,
            version: String::new(),
            base_url,
        }),
    }
}

#[tauri::command]
async fn ollama_list_models() -> Result<Vec<OllamaModelDto>, String> {
    let client = ollama_http_client()?;
    let base_url = ollama_base_url();
    let url = format!("{}/api/tags", base_url.trim_end_matches('/'));
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

#[derive(Debug, Serialize)]
pub struct AiHubActionResultDto {
    ok: bool,
    message: String,
}

#[derive(Debug, Serialize)]
pub struct OllamaStorageInfoDto {
    path: String,
    exists: bool,
    model_count: usize,
    total_size_bytes: u64,
    total_size_human: String,
}

fn ollama_models_path() -> PathBuf {
    if let Ok(value) = std::env::var("OLLAMA_MODELS") {
        let trimmed = value.trim();
        if !trimmed.is_empty() {
            return PathBuf::from(trimmed);
        }
    }

    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_default();
    PathBuf::from(home).join(".ollama").join("models")
}

async fn run_local_cli(command: &str, args: Vec<String>) -> Result<String, String> {
    let cmd = command.to_string();
    tokio::task::spawn_blocking(move || {
        let output = Command::new(&cmd)
            .args(&args)
            .env("PATH", runtime_setup_path())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .map_err(|e| format!("Failed to run {}: {}", cmd, e))?;
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        if output.status.success() {
            Ok(if stdout.is_empty() { stderr } else { stdout })
        } else {
            let detail = if stderr.is_empty() { stdout } else { stderr };
            Err(if detail.is_empty() {
                format!("{} exited with status {}", cmd, output.status)
            } else {
                detail
            })
        }
    })
    .await
    .map_err(|e| format!("{} task failed: {}", command, e))?
}

#[tauri::command]
async fn ollama_storage_info() -> Result<OllamaStorageInfoDto, String> {
    let models = ollama_list_models().await.unwrap_or_default();
    let path = ollama_models_path();
    let total_size_bytes = models.iter().map(|item| item.size_bytes).sum::<u64>();
    Ok(OllamaStorageInfoDto {
        path: path.display().to_string(),
        exists: path.exists(),
        model_count: models.len(),
        total_size_bytes,
        total_size_human: format_bytes_human(total_size_bytes),
    })
}

#[tauri::command]
async fn ollama_pull_model(name: String) -> Result<AiHubActionResultDto, String> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err("Model name is required".to_string());
    }
    let output = run_local_cli("ollama", vec!["pull".to_string(), trimmed.to_string()]).await?;
    Ok(AiHubActionResultDto {
        ok: true,
        message: if output.is_empty() {
            format!("Pulled model {}", trimmed)
        } else {
            output
        },
    })
}

#[tauri::command]
async fn ollama_delete_model(name: String) -> Result<AiHubActionResultDto, String> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err("Model name is required".to_string());
    }
    let output = run_local_cli("ollama", vec!["rm".to_string(), trimmed.to_string()]).await?;
    Ok(AiHubActionResultDto {
        ok: true,
        message: if output.is_empty() {
            format!("Removed model {}", trimmed)
        } else {
            output
        },
    })
}

// ── Agent sandboxes (MVP) ──────────────────────────────────────────

// ── AI settings commands ───────────────────────────────────────────

const AI_SECRET_SERVICE: &str = "com.cratebay.app.ai";
static AI_REQUEST_SEQ: AtomicU64 = AtomicU64::new(1);
static SANDBOX_SEQ: AtomicU64 = AtomicU64::new(1);

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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerEntry {
    id: String,
    name: String,
    command: String,
    #[serde(default)]
    args: Vec<String>,
    #[serde(default)]
    env: Vec<String>,
    #[serde(default)]
    working_dir: String,
    #[serde(default = "default_true")]
    enabled: bool,
    #[serde(default)]
    notes: String,
}

fn default_opensandbox_base_url() -> String {
    "http://127.0.0.1:8080".to_string()
}

fn default_opensandbox_config_path() -> String {
    let home = std::env::var("HOME").unwrap_or_else(|_| "~".to_string());
    format!("{}/.cratebay/opensandbox.toml", home)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenSandboxConfig {
    #[serde(default)]
    enabled: bool,
    #[serde(default = "default_opensandbox_base_url")]
    base_url: String,
    #[serde(default = "default_opensandbox_config_path")]
    config_path: String,
}

impl Default for OpenSandboxConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            base_url: default_opensandbox_base_url(),
            config_path: default_opensandbox_config_path(),
        }
    }
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
    #[serde(default)]
    mcp_servers: Vec<McpServerEntry>,
    #[serde(default)]
    opensandbox: OpenSandboxConfig,
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
            "managed-sandbox-list",
            "Managed Sandbox List",
            "List CrateBay-managed sandboxes and their current lifecycle state.",
            &["sandbox", "managed", "read"],
            "assistant_step",
            "sandbox_list",
            serde_json::json!({
                "type": "object",
                "properties": {},
                "additionalProperties": false
            }),
        ),
        ai_skill(
            "managed-sandbox-command",
            "Managed Sandbox Command",
            "Run a command inside a CrateBay-managed sandbox.",
            &["sandbox", "managed", "command"],
            "sandbox_action",
            "sandbox_exec",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "id": { "type": "string", "minLength": 1 },
                    "command": { "type": "string", "minLength": 1 }
                },
                "required": ["id", "command"],
                "additionalProperties": false
            }),
        ),
        ai_skill(
            "agent-cli-codex-prompt",
            "Codex CLI Prompt",
            "Invoke the Codex CLI preset directly from the skills runtime.",
            &["agent-cli", "codex", "prompting"],
            "agent_cli_preset",
            "codex",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "prompt": { "type": "string", "minLength": 1 }
                },
                "required": ["prompt"],
                "additionalProperties": false
            }),
        ),
        ai_skill(
            "agent-cli-claude-prompt",
            "Claude Code Prompt",
            "Invoke the Claude Code CLI preset directly from the skills runtime.",
            &["agent-cli", "claude", "prompting"],
            "agent_cli_preset",
            "claude",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "prompt": { "type": "string", "minLength": 1 }
                },
                "required": ["prompt"],
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
                    "prompt": { "type": "string", "minLength": 1 }
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
        mcp_servers: Vec::new(),
        opensandbox: OpenSandboxConfig::default(),
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
            skill.description = "Skill runtime entry".to_string();
        }
        if skill.executor.is_empty() {
            skill.executor = "assistant_step".to_string();
        }
        if skill.input_schema.is_null() {
            skill.input_schema = default_skill_input_schema();
        }
    }
    if settings.skills.is_empty() {
        settings.skills = default_ai_skills();
    } else {
        let mut existing_ids = settings
            .skills
            .iter()
            .map(|skill| skill.id.clone())
            .collect::<std::collections::HashSet<_>>();
        for default_skill in default_ai_skills() {
            if existing_ids.insert(default_skill.id.clone()) {
                settings.skills.push(default_skill);
            }
        }
    }

    let mut mcp_seen = std::collections::HashSet::new();
    settings.mcp_servers.retain(|server| {
        let id = server.id.trim();
        !id.is_empty() && mcp_seen.insert(id.to_string())
    });
    for server in &mut settings.mcp_servers {
        server.id = server.id.trim().to_string();
        server.name = server.name.trim().to_string();
        server.command = server.command.trim().to_string();
        server.working_dir = server.working_dir.trim().to_string();
        server.notes = server.notes.trim().to_string();
        server.args.retain(|arg| !arg.trim().is_empty());
        if server.name.is_empty() {
            server.name = server.id.clone();
        }
    }

    settings.opensandbox.base_url = settings.opensandbox.base_url.trim().to_string();
    if settings.opensandbox.base_url.is_empty() {
        settings.opensandbox.base_url = default_opensandbox_base_url();
    }
    settings.opensandbox.config_path = settings.opensandbox.config_path.trim().to_string();
    if settings.opensandbox.config_path.is_empty() {
        settings.opensandbox.config_path = default_opensandbox_config_path();
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

    for server in &normalized.mcp_servers {
        if server.id.trim().is_empty() {
            return Err("MCP server id is required".to_string());
        }
        if server.command.trim().is_empty() {
            return Err(format!("MCP server '{}' command is required", server.id));
        }
        sandbox_normalize_env(Some(server.env.clone()))?;
    }

    let path = ai_settings_path();
    persist_ai_settings(&path, &normalized)?;
    Ok(normalized)
}

#[derive(Debug, Clone, Serialize)]
pub struct McpServerStatusDto {
    id: String,
    name: String,
    command: String,
    args: Vec<String>,
    enabled: bool,
    running: bool,
    status: String,
    pid: Option<u32>,
    started_at: String,
    exit_code: Option<i32>,
    notes: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct OpenSandboxStatusDto {
    installed: bool,
    enabled: bool,
    configured: bool,
    reachable: bool,
    base_url: String,
    config_path: String,
}

fn mcp_runtime_logs() -> Arc<Mutex<VecDeque<String>>> {
    Arc::new(Mutex::new(VecDeque::new()))
}

fn mcp_push_log(logs: &Arc<Mutex<VecDeque<String>>>, line: String) {
    if let Ok(mut guard) = logs.lock() {
        guard.push_back(format!(
            "{} {}",
            chrono::Local::now().format("%H:%M:%S"),
            line
        ));
        while guard.len() > 400 {
            guard.pop_front();
        }
    }
}

fn mcp_env_map(entries: &[String]) -> HashMap<String, String> {
    entries
        .iter()
        .filter_map(|item| item.split_once('='))
        .map(|(key, value)| (key.to_string(), value.to_string()))
        .collect()
}

fn mcp_refresh_runtime(runtime: &mut McpServerRuntime) {
    if let Some(child) = runtime.child.as_mut() {
        match child.try_wait() {
            Ok(Some(status)) => {
                runtime.exit_code = status.code();
                runtime.child = None;
                mcp_push_log(
                    &runtime.logs,
                    format!("[runtime] process exited with {:?}", runtime.exit_code),
                );
            }
            Ok(None) => {}
            Err(err) => {
                mcp_push_log(
                    &runtime.logs,
                    format!("[runtime] status check failed: {}", err),
                );
            }
        }
    }
}

fn mcp_spawn_log_reader<R: std::io::Read + Send + 'static>(
    logs: Arc<Mutex<VecDeque<String>>>,
    stream_name: &'static str,
    reader: R,
) {
    std::thread::spawn(move || {
        let buf = BufReader::new(reader);
        for line in buf.lines() {
            match line {
                Ok(line) => mcp_push_log(&logs, format!("[{}] {}", stream_name, line)),
                Err(err) => {
                    mcp_push_log(&logs, format!("[{}] read error: {}", stream_name, err));
                    break;
                }
            }
        }
    });
}

fn mcp_runtime_status_from_settings(
    state: &AppState,
    settings: &AiSettings,
) -> Vec<McpServerStatusDto> {
    let mut runtimes = state.mcp_runtimes.lock().unwrap_or_else(|e| e.into_inner());
    let mut out = Vec::new();
    for server in &settings.mcp_servers {
        let runtime = runtimes
            .entry(server.id.clone())
            .or_insert_with(|| McpServerRuntime {
                child: None,
                logs: mcp_runtime_logs(),
                started_at: None,
                exit_code: None,
            });
        mcp_refresh_runtime(runtime);
        let pid = runtime.child.as_ref().map(|child| child.id());
        let running = runtime.child.is_some();
        out.push(McpServerStatusDto {
            id: server.id.clone(),
            name: server.name.clone(),
            command: server.command.clone(),
            args: server.args.clone(),
            enabled: server.enabled,
            running,
            status: if running {
                "running".to_string()
            } else if runtime.exit_code.is_some() {
                "exited".to_string()
            } else {
                "stopped".to_string()
            },
            pid,
            started_at: runtime.started_at.clone().unwrap_or_default(),
            exit_code: runtime.exit_code,
            notes: server.notes.clone(),
        });
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

fn mcp_list_servers_inner(state: &AppState) -> Result<Vec<McpServerStatusDto>, String> {
    let settings = load_ai_settings()?;
    Ok(mcp_runtime_status_from_settings(state, &settings))
}

#[tauri::command]
fn mcp_list_servers(state: State<'_, AppState>) -> Result<Vec<McpServerStatusDto>, String> {
    mcp_list_servers_inner(state.inner())
}

#[tauri::command]
fn mcp_save_servers(servers: Vec<McpServerEntry>) -> Result<Vec<McpServerEntry>, String> {
    let mut settings = load_ai_settings()?;
    settings.mcp_servers = servers;
    let saved = save_ai_settings(settings)?;
    Ok(saved.mcp_servers)
}

async fn mcp_start_server_inner(
    state: &AppState,
    id: String,
) -> Result<AiHubActionResultDto, String> {
    let settings = load_ai_settings()?;
    let server = settings
        .mcp_servers
        .into_iter()
        .find(|item| item.id == id)
        .ok_or_else(|| format!("Unknown MCP server '{}'", id))?;
    if !server.enabled {
        return Err(format!("MCP server '{}' is disabled", server.name));
    }
    sandbox_normalize_env(Some(server.env.clone()))?;

    let mut runtimes = state.mcp_runtimes.lock().unwrap_or_else(|e| e.into_inner());
    let runtime = runtimes
        .entry(server.id.clone())
        .or_insert_with(|| McpServerRuntime {
            child: None,
            logs: mcp_runtime_logs(),
            started_at: None,
            exit_code: None,
        });
    mcp_refresh_runtime(runtime);
    if runtime.child.is_some() {
        return Ok(AiHubActionResultDto {
            ok: true,
            message: format!("{} is already running", server.name),
        });
    }

    let mut command = Command::new(&server.command);
    command.args(&server.args);
    command.env("PATH", runtime_setup_path());
    for (key, value) in mcp_env_map(&server.env) {
        command.env(key, value);
    }
    if !server.working_dir.trim().is_empty() {
        command.current_dir(server.working_dir.trim());
    }
    command.stdout(Stdio::piped()).stderr(Stdio::piped());

    let mut child = command
        .spawn()
        .map_err(|e| format!("Failed to start MCP server {}: {}", server.name, e))?;
    let pid = child.id();
    if let Some(stdout) = child.stdout.take() {
        mcp_spawn_log_reader(runtime.logs.clone(), "stdout", stdout);
    }
    if let Some(stderr) = child.stderr.take() {
        mcp_spawn_log_reader(runtime.logs.clone(), "stderr", stderr);
    }
    runtime.started_at = Some(chrono::Utc::now().to_rfc3339());
    runtime.exit_code = None;
    runtime.child = Some(child);
    mcp_push_log(
        &runtime.logs,
        format!(
            "[runtime] started pid={} cmd={} {}",
            pid,
            server.command,
            server.args.join(" ")
        ),
    );
    Ok(AiHubActionResultDto {
        ok: true,
        message: format!("Started {} (pid {})", server.name, pid),
    })
}

#[tauri::command]
async fn mcp_start_server(
    state: State<'_, AppState>,
    id: String,
) -> Result<AiHubActionResultDto, String> {
    mcp_start_server_inner(state.inner(), id).await
}

async fn mcp_stop_server_inner(
    state: &AppState,
    id: String,
) -> Result<AiHubActionResultDto, String> {
    let mut runtimes = state.mcp_runtimes.lock().unwrap_or_else(|e| e.into_inner());
    let Some(runtime) = runtimes.get_mut(&id) else {
        return Ok(AiHubActionResultDto {
            ok: true,
            message: format!("MCP server {} is not running", id),
        });
    };
    mcp_refresh_runtime(runtime);
    let Some(mut child) = runtime.child.take() else {
        return Ok(AiHubActionResultDto {
            ok: true,
            message: format!("MCP server {} is already stopped", id),
        });
    };
    child
        .kill()
        .map_err(|e| format!("Failed to stop MCP server {}: {}", id, e))?;
    let status = child
        .wait()
        .map_err(|e| format!("Failed to wait for MCP server {}: {}", id, e))?;
    runtime.exit_code = status.code();
    mcp_push_log(
        &runtime.logs,
        format!("[runtime] stopped with {:?}", runtime.exit_code),
    );
    Ok(AiHubActionResultDto {
        ok: true,
        message: format!("Stopped MCP server {}", id),
    })
}

#[tauri::command]
async fn mcp_stop_server(
    state: State<'_, AppState>,
    id: String,
) -> Result<AiHubActionResultDto, String> {
    mcp_stop_server_inner(state.inner(), id).await
}

fn mcp_server_logs_inner(
    state: &AppState,
    id: String,
    limit: Option<usize>,
) -> Result<Vec<String>, String> {
    let runtimes = state.mcp_runtimes.lock().unwrap_or_else(|e| e.into_inner());
    let Some(runtime) = runtimes.get(&id) else {
        return Ok(Vec::new());
    };
    let logs = runtime.logs.lock().unwrap_or_else(|e| e.into_inner());
    let limit = limit.unwrap_or(80).clamp(1, 400);
    let len = logs.len();
    let start = len.saturating_sub(limit);
    Ok(logs.iter().skip(start).cloned().collect())
}

#[tauri::command]
fn mcp_server_logs(
    state: State<'_, AppState>,
    id: String,
    limit: Option<usize>,
) -> Result<Vec<String>, String> {
    mcp_server_logs_inner(state.inner(), id, limit)
}

#[tauri::command]
fn mcp_export_client_config(client: String) -> Result<String, String> {
    let normalized_client = client.trim().to_ascii_lowercase();
    if !matches!(normalized_client.as_str(), "codex" | "claude" | "cursor") {
        return Err(format!("Unsupported MCP client '{}'", client));
    }
    let settings = load_ai_settings()?;
    let mut servers = serde_json::Map::new();
    for server in settings.mcp_servers.into_iter().filter(|item| item.enabled) {
        let env_map = mcp_env_map(&server.env);
        servers.insert(
            server.id.clone(),
            serde_json::json!({
                "command": server.command,
                "args": server.args,
                "cwd": if server.working_dir.trim().is_empty() { serde_json::Value::Null } else { serde_json::Value::String(server.working_dir) },
                "env": env_map,
            }),
        );
    }
    serde_json::to_string_pretty(&serde_json::json!({
        "client": normalized_client,
        "mcpServers": servers,
    }))
    .map_err(|e| format!("Failed to encode MCP export config: {}", e))
}

#[tauri::command]
async fn opensandbox_status() -> Result<OpenSandboxStatusDto, String> {
    let settings = load_ai_settings()?;
    let config = settings.opensandbox;
    let installed = tokio::task::spawn_blocking(|| {
        Command::new("opensandbox-server")
            .arg("--help")
            .env("PATH", runtime_setup_path())
            .output()
            .is_ok()
    })
    .await
    .unwrap_or(false);
    let configured = Path::new(&config.config_path).exists();
    let reachable = if config.base_url.trim().is_empty() {
        false
    } else {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_millis(500))
            .build()
            .map_err(|e| format!("Failed to build OpenSandbox client: {}", e))?;
        let url = format!("{}/docs", config.base_url.trim_end_matches('/'));
        match client.get(&url).send().await {
            Ok(resp) => resp.status().is_success() || resp.status().is_redirection(),
            Err(_) => false,
        }
    };
    Ok(OpenSandboxStatusDto {
        installed,
        enabled: config.enabled,
        configured,
        reachable,
        base_url: config.base_url,
        config_path: config.config_path,
    })
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiSkillExecutionResult {
    ok: bool,
    skill_id: String,
    executor: String,
    target: String,
    request_id: String,
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
    if let Some(policy) = sandbox_action_policy(command) {
        return Some(AssistantCommandPolicy {
            risk_level: policy.risk_level,
            always_confirm: policy.requires_confirmation,
        });
    }

    match command {
        "list_containers"
        | "vm_list"
        | "k8s_list_pods"
        | "ollama_list_models"
        | "mcp_list_servers"
        | "mcp_export_client_config" => Some(AssistantCommandPolicy {
            risk_level: "read",
            always_confirm: false,
        }),
        "start_container"
        | "stop_container"
        | "vm_start"
        | "vm_stop"
        | "docker_runtime_quick_setup"
        | "ollama_pull_model"
        | "mcp_start_server"
        | "mcp_stop_server" => Some(AssistantCommandPolicy {
            risk_level: "write",
            always_confirm: false,
        }),
        "remove_container" | "vm_delete" | "ollama_delete_model" => Some(AssistantCommandPolicy {
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

fn file_secret_path(key_ref: &str) -> Option<PathBuf> {
    let base = std::env::var("CRATEBAY_TEST_SECRET_DIR").ok()?;
    let mut name = key_ref
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' || ch == '.' {
                ch
            } else {
                '_'
            }
        })
        .collect::<String>();
    if name.trim_matches('_').is_empty() {
        name = "secret".to_string();
    }
    Some(PathBuf::from(base).join(format!("{}.secret", name)))
}

fn secret_entry(key_ref: &str) -> Result<Entry, String> {
    Entry::new(AI_SECRET_SERVICE, key_ref)
        .map_err(|e| format!("Failed to create secret entry: {e}"))
}

fn secret_set(key_ref: &str, value: &str) -> Result<(), String> {
    if let Some(path) = file_secret_path(key_ref) {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                format!(
                    "Failed to create test secret directory {}: {}",
                    parent.display(),
                    e
                )
            })?;
        }
        std::fs::write(&path, value).map_err(|e| {
            format!(
                "Failed to write test secret '{}' at {}: {}",
                key_ref,
                path.display(),
                e
            )
        })?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&path)
                .map_err(|e| format!("Failed to stat test secret '{}': {}", key_ref, e))?
                .permissions();
            perms.set_mode(0o600);
            std::fs::set_permissions(&path, perms).map_err(|e| {
                format!(
                    "Failed to set permissions on test secret '{}': {}",
                    key_ref, e
                )
            })?;
        }
        return Ok(());
    }

    let entry = secret_entry(key_ref)?;
    entry
        .set_password(value)
        .map_err(|e| format!("Failed to save secret '{}': {}", key_ref, e))
}

fn secret_get(key_ref: &str) -> Result<Option<String>, String> {
    if let Some(path) = file_secret_path(key_ref) {
        if !path.exists() {
            return Ok(None);
        }
        let value = std::fs::read_to_string(&path).map_err(|e| {
            format!(
                "Failed to read test secret '{}' at {}: {}",
                key_ref,
                path.display(),
                e
            )
        })?;
        if value.trim().is_empty() {
            return Ok(None);
        }
        return Ok(Some(value));
    }

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
    if let Some(path) = file_secret_path(key_ref) {
        match std::fs::remove_file(&path) {
            Ok(_) => return Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(e) => {
                return Err(format!(
                    "Failed to delete test secret '{}' at {}: {}",
                    key_ref,
                    path.display(),
                    e
                ))
            }
        }
    }

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
        .user_agent(concat!("CrateBay-AI/", env!("CARGO_PKG_VERSION")))
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
        "list_containers"
            | "start_container"
            | "stop_container"
            | "remove_container"
            | "sandbox_list"
            | "sandbox_create"
            | "sandbox_start"
            | "sandbox_stop"
            | "sandbox_delete"
            | "sandbox_exec"
            | "sandbox_cleanup_expired"
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

    if lower.contains("ollama") || lower.contains("model") || prompt.contains("模型") {
        if lower.contains("pull")
            || lower.contains("download")
            || prompt.contains("拉取")
            || prompt.contains("下载")
        {
            steps.push(AssistantPlanStep {
                id: format!("step-{}", steps.len() + 1),
                title: "Pull Ollama model".to_string(),
                command: "ollama_pull_model".to_string(),
                args: serde_json::json!({ "name": "<model-name>" }),
                risk_level: "write".to_string(),
                requires_confirmation: require_confirm,
                explain: "Pulls a local model into Ollama.".to_string(),
            });
        } else if lower.contains("delete")
            || lower.contains("remove")
            || prompt.contains("删除")
            || prompt.contains("移除")
        {
            steps.push(AssistantPlanStep {
                id: format!("step-{}", steps.len() + 1),
                title: "Delete Ollama model".to_string(),
                command: "ollama_delete_model".to_string(),
                args: serde_json::json!({ "name": "<model-name>" }),
                risk_level: "destructive".to_string(),
                requires_confirmation: true,
                explain: "Removes a local Ollama model.".to_string(),
            });
        } else {
            steps.push(AssistantPlanStep {
                id: format!("step-{}", steps.len() + 1),
                title: "List Ollama models".to_string(),
                command: "ollama_list_models".to_string(),
                args: serde_json::json!({}),
                risk_level: "read".to_string(),
                requires_confirmation: false,
                explain: "Lists available local models.".to_string(),
            });
        }
    }

    if lower.contains("sandbox") || prompt.contains("沙箱") {
        if lower.contains("cleanup")
            || lower.contains("expire")
            || prompt.contains("清理")
            || prompt.contains("过期")
        {
            steps.push(AssistantPlanStep {
                id: format!("step-{}", steps.len() + 1),
                title: "Cleanup expired sandboxes".to_string(),
                command: "sandbox_cleanup_expired".to_string(),
                args: serde_json::json!({}),
                risk_level: "write".to_string(),
                requires_confirmation: require_confirm,
                explain: "Reclaims expired managed sandboxes.".to_string(),
            });
        } else if lower.contains("delete")
            || lower.contains("remove")
            || prompt.contains("删除")
            || prompt.contains("移除")
        {
            steps.push(AssistantPlanStep {
                id: format!("step-{}", steps.len() + 1),
                title: "Delete sandbox".to_string(),
                command: "sandbox_delete".to_string(),
                args: serde_json::json!({ "id": "<sandbox-id>" }),
                risk_level: "destructive".to_string(),
                requires_confirmation: true,
                explain: "Deletes a managed sandbox.".to_string(),
            });
        } else if lower.contains("stop") || prompt.contains("停止") {
            steps.push(AssistantPlanStep {
                id: format!("step-{}", steps.len() + 1),
                title: "Stop sandbox".to_string(),
                command: "sandbox_stop".to_string(),
                args: serde_json::json!({ "id": "<sandbox-id>" }),
                risk_level: "write".to_string(),
                requires_confirmation: require_confirm,
                explain: "Stops a managed sandbox.".to_string(),
            });
        } else if lower.contains("start") || prompt.contains("启动") {
            steps.push(AssistantPlanStep {
                id: format!("step-{}", steps.len() + 1),
                title: "Start sandbox".to_string(),
                command: "sandbox_start".to_string(),
                args: serde_json::json!({ "id": "<sandbox-id>" }),
                risk_level: "write".to_string(),
                requires_confirmation: require_confirm,
                explain: "Starts a managed sandbox.".to_string(),
            });
        } else if lower.contains("exec") || lower.contains("run") || prompt.contains("执行") {
            steps.push(AssistantPlanStep {
                id: format!("step-{}", steps.len() + 1),
                title: "Execute command in sandbox".to_string(),
                command: "sandbox_exec".to_string(),
                args: serde_json::json!({ "id": "<sandbox-id>", "command": "<shell-command>" }),
                risk_level: "write".to_string(),
                requires_confirmation: require_confirm,
                explain: "Runs a command inside a managed sandbox.".to_string(),
            });
        } else {
            steps.push(AssistantPlanStep {
                id: format!("step-{}", steps.len() + 1),
                title: "List sandboxes".to_string(),
                command: "sandbox_list".to_string(),
                args: serde_json::json!({}),
                risk_level: "read".to_string(),
                requires_confirmation: false,
                explain: "Lists managed sandboxes and current state.".to_string(),
            });
        }
    }

    if lower.contains("mcp") {
        if lower.contains("stop") || prompt.contains("停止") {
            steps.push(AssistantPlanStep {
                id: format!("step-{}", steps.len() + 1),
                title: "Stop MCP server".to_string(),
                command: "mcp_stop_server".to_string(),
                args: serde_json::json!({ "id": "<server-id>" }),
                risk_level: "write".to_string(),
                requires_confirmation: require_confirm,
                explain: "Stops a managed MCP server process.".to_string(),
            });
        } else if lower.contains("start") || prompt.contains("启动") {
            steps.push(AssistantPlanStep {
                id: format!("step-{}", steps.len() + 1),
                title: "Start MCP server".to_string(),
                command: "mcp_start_server".to_string(),
                args: serde_json::json!({ "id": "<server-id>" }),
                risk_level: "write".to_string(),
                requires_confirmation: require_confirm,
                explain: "Starts a managed MCP server process.".to_string(),
            });
        } else if lower.contains("export") || prompt.contains("导出") {
            steps.push(AssistantPlanStep {
                id: format!("step-{}", steps.len() + 1),
                title: "Export MCP client config".to_string(),
                command: "mcp_export_client_config".to_string(),
                args: serde_json::json!({ "client": "codex" }),
                risk_level: "read".to_string(),
                requires_confirmation: false,
                explain: "Exports MCP client configuration for the selected client.".to_string(),
            });
        } else {
            steps.push(AssistantPlanStep {
                id: format!("step-{}", steps.len() + 1),
                title: "List MCP servers".to_string(),
                command: "mcp_list_servers".to_string(),
                args: serde_json::json!({}),
                risk_level: "read".to_string(),
                requires_confirmation: false,
                explain: "Lists MCP registry entries and runtime states.".to_string(),
            });
        }
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
        "ollama_list_models" => {
            let items = ollama_list_models().await?;
            serde_json::to_value(items).map_err(|e| e.to_string())?
        }
        "mcp_list_servers" => {
            let items = mcp_list_servers(state.clone())?;
            serde_json::to_value(items).map_err(|e| e.to_string())?
        }
        "mcp_export_client_config" => {
            let client = assistant_arg_optional_string(args_map, "client")?
                .unwrap_or_else(|| "codex".to_string());
            serde_json::to_value(mcp_export_client_config(client)?).map_err(|e| e.to_string())?
        }
        "sandbox_list" => {
            let items = sandbox_list().await?;
            serde_json::to_value(items).map_err(|e| e.to_string())?
        }
        "start_container" => {
            let id = assistant_arg_string(args_map, "id")?;
            start_container(id).await?;
            serde_json::json!({ "ok": true })
        }
        "ollama_pull_model" => {
            let name = assistant_arg_string(args_map, "name")?;
            serde_json::to_value(ollama_pull_model(name).await?).map_err(|e| e.to_string())?
        }
        "mcp_start_server" => {
            let id = assistant_arg_string(args_map, "id")?;
            serde_json::to_value(mcp_start_server(state.clone(), id).await?)
                .map_err(|e| e.to_string())?
        }
        "mcp_stop_server" => {
            let id = assistant_arg_string(args_map, "id")?;
            serde_json::to_value(mcp_stop_server(state.clone(), id).await?)
                .map_err(|e| e.to_string())?
        }
        "sandbox_start" => {
            let id = assistant_arg_string(args_map, "id")?;
            sandbox_start(id).await?;
            serde_json::json!({ "ok": true })
        }
        "sandbox_stop" => {
            let id = assistant_arg_string(args_map, "id")?;
            sandbox_stop(id).await?;
            serde_json::json!({ "ok": true })
        }
        "sandbox_cleanup_expired" => {
            serde_json::to_value(sandbox_cleanup_expired().await?).map_err(|e| e.to_string())?
        }
        "sandbox_exec" => {
            let id = assistant_arg_string(args_map, "id")?;
            let command = assistant_arg_string(args_map, "command")?;
            serde_json::to_value(sandbox_exec(id, command).await?).map_err(|e| e.to_string())?
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
        "ollama_delete_model" => {
            let name = assistant_arg_string(args_map, "name")?;
            serde_json::to_value(ollama_delete_model(name).await?).map_err(|e| e.to_string())?
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
        "sandbox_delete" => {
            let id = assistant_arg_string(args_map, "id")?;
            sandbox_delete(id).await?;
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

fn resolve_ai_skill(settings: &AiSettings, skill_id: &str) -> Result<AiSkillDefinition, String> {
    settings
        .skills
        .iter()
        .find(|skill| skill.id == skill_id)
        .cloned()
        .ok_or_else(|| format!("Skill not found: {}", skill_id))
}

fn skill_prompt_input(input: &serde_json::Value) -> Option<String> {
    if let Some(prompt) = input.as_str() {
        let prompt = prompt.trim();
        if !prompt.is_empty() {
            return Some(prompt.to_string());
        }
    }

    input
        .as_object()
        .and_then(|obj| obj.get("prompt"))
        .and_then(|value| value.as_str())
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn skill_input_kind(value: &serde_json::Value) -> &'static str {
    match value {
        serde_json::Value::Null => "null",
        serde_json::Value::Bool(_) => "boolean",
        serde_json::Value::Number(_) => "number",
        serde_json::Value::String(_) => "string",
        serde_json::Value::Array(_) => "array",
        serde_json::Value::Object(_) => "object",
    }
}

fn normalize_skill_input(skill: &AiSkillDefinition, input: serde_json::Value) -> serde_json::Value {
    if skill.executor != "agent_cli_preset" {
        return input;
    }

    match input {
        serde_json::Value::String(prompt) => {
            serde_json::json!({ "prompt": prompt.trim() })
        }
        serde_json::Value::Object(mut map) => {
            if let Some(prompt) = map.get("prompt").and_then(|value| value.as_str()) {
                map.insert(
                    "prompt".to_string(),
                    serde_json::Value::String(prompt.trim().to_string()),
                );
            }
            serde_json::Value::Object(map)
        }
        other => other,
    }
}

fn validate_skill_input(
    path: &str,
    value: &serde_json::Value,
    schema: &serde_json::Value,
) -> Result<(), String> {
    let Some(schema_obj) = schema.as_object() else {
        return Ok(());
    };
    if schema_obj.is_empty() {
        return Ok(());
    }

    if let Some(expected_type) = schema_obj.get("type").and_then(|value| value.as_str()) {
        match expected_type {
            "object" => {
                let object = value.as_object().ok_or_else(|| {
                    format!(
                        "{} must be an object, got {}",
                        path,
                        skill_input_kind(value)
                    )
                })?;
                let properties = schema_obj
                    .get("properties")
                    .and_then(|value| value.as_object());

                if let Some(required) = schema_obj
                    .get("required")
                    .and_then(|value| value.as_array())
                {
                    for item in required {
                        let Some(key) = item.as_str() else {
                            continue;
                        };
                        if !object.contains_key(key) {
                            return Err(format!("{}.{} is required", path, key));
                        }
                    }
                }

                let allow_additional = schema_obj
                    .get("additionalProperties")
                    .and_then(|value| value.as_bool())
                    .unwrap_or(true);
                if !allow_additional {
                    for key in object.keys() {
                        if !properties
                            .map(|items| items.contains_key(key))
                            .unwrap_or(false)
                        {
                            return Err(format!("{}.{} is not allowed", path, key));
                        }
                    }
                }

                if let Some(properties) = properties {
                    for (key, property_schema) in properties {
                        if let Some(property_value) = object.get(key) {
                            validate_skill_input(
                                &format!("{}.{}", path, key),
                                property_value,
                                property_schema,
                            )?;
                        }
                    }
                }
            }
            "string" => {
                let text = value.as_str().ok_or_else(|| {
                    format!("{} must be a string, got {}", path, skill_input_kind(value))
                })?;
                if let Some(min_length) =
                    schema_obj.get("minLength").and_then(|value| value.as_u64())
                {
                    if text.chars().count() < min_length as usize {
                        return Err(format!(
                            "{} must be at least {} characters",
                            path, min_length
                        ));
                    }
                }
            }
            "boolean" => {
                if !value.is_boolean() {
                    return Err(format!(
                        "{} must be a boolean, got {}",
                        path,
                        skill_input_kind(value)
                    ));
                }
            }
            "number" => {
                if !value.is_number() {
                    return Err(format!(
                        "{} must be a number, got {}",
                        path,
                        skill_input_kind(value)
                    ));
                }
            }
            "integer" => match value {
                serde_json::Value::Number(number) if number.is_i64() || number.is_u64() => {}
                _ => {
                    return Err(format!(
                        "{} must be an integer, got {}",
                        path,
                        skill_input_kind(value)
                    ));
                }
            },
            "array" => {
                let items = value.as_array().ok_or_else(|| {
                    format!("{} must be an array, got {}", path, skill_input_kind(value))
                })?;
                if let Some(min_items) = schema_obj.get("minItems").and_then(|value| value.as_u64())
                {
                    if items.len() < min_items as usize {
                        return Err(format!(
                            "{} must contain at least {} items",
                            path, min_items
                        ));
                    }
                }
                if let Some(item_schema) = schema_obj.get("items") {
                    for (index, item) in items.iter().enumerate() {
                        validate_skill_input(&format!("{}[{}]", path, index), item, item_schema)?;
                    }
                }
            }
            _ => {}
        }
    }

    if let Some(allowed_values) = schema_obj.get("enum").and_then(|value| value.as_array()) {
        if !allowed_values.iter().any(|candidate| candidate == value) {
            return Err(format!("{} must match one of the allowed values", path));
        }
    }

    Ok(())
}

#[tauri::command]
async fn ai_skill_execute(
    state: State<'_, AppState>,
    skill_id: String,
    input: Option<serde_json::Value>,
    dry_run: Option<bool>,
    confirmed: Option<bool>,
) -> Result<AiSkillExecutionResult, String> {
    let settings = load_ai_settings()?;
    let skill = resolve_ai_skill(&settings, skill_id.trim())?;
    if !skill.enabled {
        return Err(format!("Skill '{}' is disabled", skill.id));
    }

    let input_value = normalize_skill_input(&skill, input.unwrap_or_else(|| serde_json::json!({})));
    validate_skill_input("input", &input_value, &skill.input_schema)?;

    let (request_id, output) = match skill.executor.as_str() {
        "assistant_step" | "sandbox_action" => {
            let confirmation_hint = assistant_command_policy(&skill.target)
                .map(|policy| policy.risk_level != "read")
                .unwrap_or(false);
            let result = assistant_execute_step(
                state.clone(),
                skill.target.clone(),
                input_value,
                None,
                Some(confirmation_hint),
                confirmed,
            )
            .await?;
            let request_id = result.request_id.clone();
            (
                request_id,
                serde_json::to_value(result).map_err(|e| e.to_string())?,
            )
        }
        "mcp_action" => {
            let access = mcp_check_access(
                skill.target.clone(),
                None,
                Some(mcp_action_policy(&skill.target).requires_confirmation),
                confirmed,
            )?;
            if !access.allowed {
                return Err(access.message);
            }
            let result = assistant_execute_step(
                state.clone(),
                skill.target.clone(),
                input_value,
                Some(access.risk_level),
                Some(access.requires_confirmation),
                confirmed,
            )
            .await?;
            let request_id = result.request_id.clone();
            (
                request_id,
                serde_json::to_value(result).map_err(|e| e.to_string())?,
            )
        }
        "agent_cli_preset" => {
            let result = agent_cli_run(
                Some(skill.target.clone()),
                None,
                None,
                skill_prompt_input(&input_value),
                dry_run.unwrap_or(false),
                None,
            )
            .await?;
            let request_id = result.request_id.clone();
            (
                request_id,
                serde_json::to_value(result).map_err(|e| e.to_string())?,
            )
        }
        other => return Err(format!("Unsupported skill executor '{}'", other)),
    };

    ai_audit_log(
        "ai_skill_execute",
        "write",
        &request_id,
        &format!(
            "skill_id={} executor={} target={} dry_run={}",
            skill.id,
            skill.executor,
            skill.target,
            dry_run.unwrap_or(false)
        ),
    );

    Ok(AiSkillExecutionResult {
        ok: true,
        skill_id: skill.id,
        executor: skill.executor,
        target: skill.target,
        request_id,
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
            &format!(
                "dry_run=true command={} args_count={}",
                resolved_command,
                resolved_args.len()
            ),
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
                &format!(
                    "command timeout={}s command={} args_count={}",
                    timeout,
                    resolved_command,
                    resolved_args.len()
                ),
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
            "command={} args_count={} exit_code={} duration_ms={}",
            resolved_command,
            resolved_args.len(),
            exit_code,
            duration_ms
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
        normalize_skill_input, redact_sensitive, resolve_ai_skill, skill_prompt_input,
        validate_skill_input,
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
            .any(|skill| skill.id == "managed-sandbox-list"));
        assert!(settings
            .skills
            .iter()
            .any(|skill| skill.id == "managed-sandbox-command"));
        assert!(settings
            .skills
            .iter()
            .any(|skill| skill.id == "agent-cli-codex-prompt"));
        assert!(settings
            .skills
            .iter()
            .any(|skill| skill.id == "agent-cli-claude-prompt"));
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

    #[test]
    fn normalize_ai_settings_appends_new_default_skills() {
        let mut settings = default_ai_settings();
        settings
            .skills
            .retain(|skill| skill.id == "assistant-container-diagnose");
        let normalized = normalize_ai_settings(settings);
        assert!(normalized
            .skills
            .iter()
            .any(|skill| skill.id == "managed-sandbox-list"));
        assert!(normalized
            .skills
            .iter()
            .any(|skill| skill.id == "managed-sandbox-command"));
        assert!(normalized
            .skills
            .iter()
            .any(|skill| skill.id == "agent-cli-codex-prompt"));
        assert!(normalized
            .skills
            .iter()
            .any(|skill| skill.id == "agent-cli-claude-prompt"));
    }

    #[test]
    fn skill_prompt_input_supports_string_and_object_forms() {
        assert_eq!(
            skill_prompt_input(&serde_json::json!("plan infra")),
            Some("plan infra".to_string())
        );
        assert_eq!(
            skill_prompt_input(&serde_json::json!({ "prompt": "run tests" })),
            Some("run tests".to_string())
        );
        assert_eq!(skill_prompt_input(&serde_json::json!({})), None);
    }

    #[test]
    fn skill_input_schema_normalizes_prompt_string_input() {
        let settings = default_ai_settings();
        let skill = resolve_ai_skill(&settings, "agent-cli-codex-prompt").expect("codex skill");
        let normalized = normalize_skill_input(&skill, serde_json::json!("  summarize repo  "));
        assert_eq!(
            normalized,
            serde_json::json!({ "prompt": "summarize repo" })
        );
        validate_skill_input("input", &normalized, &skill.input_schema).expect("prompt schema");
    }

    #[test]
    fn skill_input_schema_rejects_missing_and_unknown_fields() {
        let settings = default_ai_settings();
        let skill = resolve_ai_skill(&settings, "managed-sandbox-command").expect("sandbox skill");
        let missing = validate_skill_input(
            "input",
            &serde_json::json!({ "id": "sandbox-1" }),
            &skill.input_schema,
        )
        .expect_err("missing command should fail");
        assert!(missing.contains("input.command is required"));

        let unexpected = validate_skill_input(
            "input",
            &serde_json::json!({
                "id": "sandbox-1",
                "command": "echo hi",
                "extra": true
            }),
            &skill.input_schema,
        )
        .expect_err("unexpected field should fail");
        assert!(unexpected.contains("input.extra is not allowed"));
    }

    #[test]
    fn skill_input_schema_rejects_empty_prompt_values() {
        let settings = default_ai_settings();
        let skill = resolve_ai_skill(&settings, "agent-cli-claude-prompt").expect("claude skill");
        let normalized = normalize_skill_input(&skill, serde_json::json!("   "));
        let err = validate_skill_input("input", &normalized, &skill.input_schema)
            .expect_err("blank prompt should fail");
        assert!(err.contains("input.prompt must be at least 1 characters"));
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

#[cfg(test)]
mod ai_runtime_tests {
    use super::{
        agent_cli_run, ai_profile, ai_test_connection, default_ai_settings, load_ai_settings,
        mcp_export_client_config, mcp_list_servers_inner, mcp_server_logs_inner,
        mcp_start_server_inner, mcp_stop_server_inner, ollama_delete_model, ollama_list_models,
        ollama_pull_model, ollama_status, ollama_storage_info, sandbox_audit_list, sandbox_create,
        sandbox_delete, sandbox_exec, sandbox_inspect, sandbox_list, sandbox_start, sandbox_stop,
        save_ai_settings, secret_delete, secret_set, AppState, McpServerEntry,
        SandboxCreateRequest,
    };
    use serde_json::json;
    use std::collections::HashMap;
    use std::ffi::OsString;
    use std::io::{Read, Write};
    use std::net::{TcpListener, TcpStream};
    use std::path::{Path, PathBuf};
    use std::process::Command;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex, OnceLock};
    use std::thread;
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

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
        fn set(key: &str, value: impl AsRef<str>) -> Self {
            let prev = std::env::var_os(key);
            std::env::set_var(key, value.as_ref());
            Self {
                key: key.to_string(),
                prev,
            }
        }
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            match self.prev.take() {
                Some(value) => std::env::set_var(&self.key, value),
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

    fn canary_timeout(env_key: &str, default_secs: u64) -> u64 {
        std::env::var(env_key)
            .ok()
            .and_then(|value| value.parse::<u64>().ok())
            .unwrap_or(default_secs)
            .max(1)
    }

    fn require_env(key: &str) -> String {
        std::env::var(key).unwrap_or_else(|_| panic!("required env var missing: {}", key))
    }

    fn configure_canary_dirs(tmp: &TempDir) -> (EnvGuard, EnvGuard, EnvGuard, EnvGuard) {
        let config_dir = tmp.path.join("config");
        let data_dir = tmp.path.join("data");
        let log_dir = tmp.path.join("logs");
        let secret_dir = tmp.path.join("secrets");
        std::fs::create_dir_all(&config_dir).expect("create config dir");
        std::fs::create_dir_all(&data_dir).expect("create data dir");
        std::fs::create_dir_all(&log_dir).expect("create log dir");
        std::fs::create_dir_all(&secret_dir).expect("create secret dir");
        (
            EnvGuard::set(
                "CRATEBAY_CONFIG_DIR",
                config_dir.to_str().expect("config dir"),
            ),
            EnvGuard::set("CRATEBAY_DATA_DIR", data_dir.to_str().expect("data dir")),
            EnvGuard::set("CRATEBAY_LOG_DIR", log_dir.to_str().expect("log dir")),
            EnvGuard::set(
                "CRATEBAY_TEST_SECRET_DIR",
                secret_dir.to_str().expect("secret dir"),
            ),
        )
    }

    fn prepend_path(dir: &Path) -> String {
        let current = std::env::var("PATH").unwrap_or_default();
        if current.is_empty() {
            dir.display().to_string()
        } else {
            format!("{}:{}", dir.display(), current)
        }
    }

    fn write_forwarder_binary(bin_dir: &Path, name: &str, target_env: &str) {
        std::fs::create_dir_all(bin_dir).expect("create canary bin dir");
        let script_path = bin_dir.join(name);
        let script = format!(
            r#"#!/usr/bin/env bash
set -euo pipefail
target="${{{target_env}:-}}"
if [[ -z "$target" ]]; then
  echo "missing {target_env}" >&2
  exit 97
fi
exec "$target" "$@"
"#
        );
        std::fs::write(&script_path, script).expect("write forwarder script");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&script_path)
                .expect("forwarder metadata")
                .permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(&script_path, perms).expect("forwarder perms");
        }
    }

    struct DockerCleanup {
        container_name: Option<String>,
        image_tag: Option<String>,
    }

    impl Drop for DockerCleanup {
        fn drop(&mut self) {
            if let Some(name) = &self.container_name {
                let _ = Command::new("docker").args(["rm", "-f", name]).output();
            }
            if let Some(tag) = &self.image_tag {
                let _ = Command::new("docker")
                    .args(["image", "rm", "-f", tag])
                    .output();
            }
        }
    }

    fn docker_ready() -> bool {
        matches!(
            Command::new("docker").arg("info").output(),
            Ok(output) if output.status.success()
        )
    }

    fn test_app_state() -> AppState {
        AppState {
            hv: Box::new(cratebay_core::vm::StubHypervisor::new()),
            grpc_addr: "http://127.0.0.1:65531".to_string(),
            daemon: Mutex::new(None),
            daemon_ready: Mutex::new(false),
            log_stream_handles: Mutex::new(HashMap::new()),
            mcp_runtimes: Mutex::new(HashMap::new()),
        }
    }

    fn fake_ollama_models(state_path: &Path) -> Vec<String> {
        std::fs::read_to_string(state_path)
            .unwrap_or_default()
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(ToString::to_string)
            .collect()
    }

    fn fake_ollama_tags_payload(state_path: &Path) -> String {
        let models = fake_ollama_models(state_path)
            .into_iter()
            .enumerate()
            .map(|(index, name)| {
                let size = 64_u64 * 1024 * 1024 * (index as u64 + 1);
                json!({
                    "name": name,
                    "modified_at": format!("2026-03-07T00:00:{:02}Z", index),
                    "size": size,
                    "digest": format!("sha256:test{:02}", index),
                    "details": {
                        "family": "qwen2.5",
                        "parameter_size": "7B",
                        "quantization_level": "Q4_K_M"
                    }
                })
            })
            .collect::<Vec<_>>();
        json!({ "models": models }).to_string()
    }

    struct FakeOllamaServer {
        stop: Arc<AtomicBool>,
        handle: Option<thread::JoinHandle<()>>,
        base_url: String,
    }

    impl FakeOllamaServer {
        fn start(state_path: PathBuf) -> Self {
            let listener = TcpListener::bind("127.0.0.1:0").expect("bind fake ollama server");
            listener
                .set_nonblocking(true)
                .expect("set fake ollama listener nonblocking");
            let port = listener.local_addr().expect("listener addr").port();
            let stop = Arc::new(AtomicBool::new(false));
            let stop_flag = stop.clone();
            let handle = thread::spawn(move || {
                while !stop_flag.load(Ordering::SeqCst) {
                    match listener.accept() {
                        Ok((mut stream, _)) => {
                            let mut buffer = [0_u8; 4096];
                            let read = stream.read(&mut buffer).unwrap_or(0);
                            let request = String::from_utf8_lossy(&buffer[..read]);
                            let path = request
                                .lines()
                                .next()
                                .and_then(|line| line.split_whitespace().nth(1))
                                .unwrap_or("/");
                            let (status, body) =
                                if path == "/api/version" || path.ends_with("/api/version") {
                                    (
                                        "HTTP/1.1 200 OK",
                                        json!({ "version": "0.5.7-test" }).to_string(),
                                    )
                                } else if path == "/api/tags" || path.ends_with("/api/tags") {
                                    ("HTTP/1.1 200 OK", fake_ollama_tags_payload(&state_path))
                                } else {
                                    (
                                        "HTTP/1.1 404 Not Found",
                                        json!({ "error": "not found", "path": path }).to_string(),
                                    )
                                };
                            let response = format!(
                                "{}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                                status,
                                body.len(),
                                body
                            );
                            let _ = stream.write_all(response.as_bytes());
                            let _ = stream.flush();
                        }
                        Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                            thread::sleep(Duration::from_millis(25));
                        }
                        Err(_) => break,
                    }
                }
            });
            Self {
                stop,
                handle: Some(handle),
                base_url: format!("http://127.0.0.1:{}", port),
            }
        }
    }

    impl Drop for FakeOllamaServer {
        fn drop(&mut self) {
            self.stop.store(true, Ordering::SeqCst);
            if let Some(port) = self.base_url.rsplit(':').next() {
                let _ = TcpStream::connect(format!("127.0.0.1:{}", port));
            }
            if let Some(handle) = self.handle.take() {
                let _ = handle.join();
            }
        }
    }

    fn write_fake_ollama_binary(bin_dir: &Path, state_path: &Path) {
        let script = format!(
            "#!/usr/bin/env bash\nset -euo pipefail\nstate=\"{}\"\nmkdir -p \"$(dirname \"$state\")\"\ntouch \"$state\"\ncase \"${{1:-}}\" in\n  --version)\n    echo \"ollama version 0.5.7-test\"\n    ;;\n  pull)\n    model=\"${{2:?model required}}\"\n    if ! grep -Fxq \"$model\" \"$state\"; then\n      echo \"$model\" >> \"$state\"\n    fi\n    echo \"pulled $model\"\n    ;;\n  rm)\n    model=\"${{2:?model required}}\"\n    tmp=\"$state.tmp\"\n    grep -Fxv \"$model\" \"$state\" > \"$tmp\" || true\n    mv \"$tmp\" \"$state\"\n    echo \"removed $model\"\n    ;;\n  *)\n    echo \"unsupported fake ollama args: $*\" >&2\n    exit 1\n    ;;\nesac\n",
            state_path.display()
        );
        let script_path = bin_dir.join("ollama");
        std::fs::write(&script_path, script).expect("write fake ollama binary");
        let chmod = Command::new("chmod")
            .args(["+x", script_path.to_str().expect("script path")])
            .status()
            .expect("chmod fake ollama");
        assert!(chmod.success(), "fake ollama binary should be executable");
    }

    #[tokio::test]
    #[ignore = "requires Docker runtime"]
    async fn sandbox_runtime_smoke_lifecycle() {
        let _lock = env_lock();
        assert!(
            docker_ready(),
            "Docker daemon must be available for sandbox runtime smoke"
        );

        let tmp = TempDir::new("cratebay-ai-sandbox-smoke");
        let config_dir = tmp.path.join("config");
        let _config = EnvGuard::set(
            "CRATEBAY_CONFIG_DIR",
            config_dir.to_str().expect("config dir"),
        );

        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let sandbox_name = format!("cbx-ai-sandbox-{}-{}", std::process::id(), suffix);
        let mut cleanup = DockerCleanup {
            container_name: Some(sandbox_name.clone()),
            image_tag: None,
        };

        let created = sandbox_create(SandboxCreateRequest {
            template_id: "python-dev".to_string(),
            name: Some(sandbox_name.clone()),
            image: Some("alpine:3.20".to_string()),
            command: Some("sleep 300".to_string()),
            env: Some(vec!["CRATEBAY_E2E=1".to_string()]),
            cpu_cores: Some(1),
            memory_mb: Some(256),
            ttl_hours: Some(1),
            owner: Some("ci".to_string()),
        })
        .await
        .expect("create sandbox");

        assert_eq!(created.name, sandbox_name);
        assert!(created.login_cmd.contains(&sandbox_name));

        let list = sandbox_list().await.expect("list sandboxes");
        let item = list
            .iter()
            .find(|entry| entry.name == sandbox_name)
            .expect("created sandbox should be listed");
        assert_eq!(item.template_id, "python-dev");
        assert_eq!(item.owner, "ci");
        assert_eq!(item.cpu_cores, 1);
        assert_eq!(item.memory_mb, 256);
        assert!(!item.is_expired);

        let inspect = sandbox_inspect(created.id.clone())
            .await
            .expect("inspect sandbox");
        assert!(inspect.running);
        assert!(inspect
            .env
            .iter()
            .any(|entry| entry == "CRATEBAY_SANDBOX=1"));
        assert!(inspect.env.iter().any(|entry| entry == "CRATEBAY_E2E=1"));

        let exec = sandbox_exec(created.id.clone(), "echo CRATEBAY_SANDBOX_OK".to_string())
            .await
            .expect("exec sandbox command");
        assert!(exec.ok);
        assert_eq!(exec.exit_code, Some(0));
        assert!(exec.stdout.contains("CRATEBAY_SANDBOX_OK"));
        assert!(exec.output.contains("CRATEBAY_SANDBOX_OK"));

        sandbox_stop(created.id.clone())
            .await
            .expect("stop sandbox");
        let stopped = sandbox_inspect(created.id.clone())
            .await
            .expect("inspect stopped sandbox");
        assert!(!stopped.running);

        sandbox_start(created.id.clone())
            .await
            .expect("restart sandbox");
        let restarted = sandbox_inspect(created.id.clone())
            .await
            .expect("inspect restarted sandbox");
        assert!(restarted.running);

        sandbox_delete(created.id.clone())
            .await
            .expect("delete sandbox");
        cleanup.container_name = None;

        let after = sandbox_list().await.expect("list sandboxes after delete");
        assert!(!after.iter().any(|entry| entry.name == sandbox_name));

        let audit = sandbox_audit_list(Some(20)).expect("sandbox audit list");
        assert!(audit
            .iter()
            .any(|event| event.action == "create" && event.sandbox_name == sandbox_name));
        assert!(audit
            .iter()
            .any(|event| event.action == "delete" && event.sandbox_name == sandbox_name));
    }

    #[tokio::test]
    #[ignore = "requires local runtime canary server"]
    async fn ollama_runtime_canary_smoke() {
        let _lock = env_lock();

        let tmp = TempDir::new("cratebay-ollama-smoke");
        let config_dir = tmp.path.join("config");
        let bin_dir = tmp.path.join("bin");
        let models_dir = tmp.path.join("models");
        let state_path = tmp.path.join("fake-ollama-models.txt");
        std::fs::create_dir_all(&bin_dir).expect("create fake bin dir");
        std::fs::create_dir_all(&models_dir).expect("create fake models dir");
        std::fs::write(&state_path, "qwen2.5:7b\n").expect("seed fake models");
        write_fake_ollama_binary(&bin_dir, &state_path);
        let server = FakeOllamaServer::start(state_path.clone());

        let current_path = std::env::var("PATH").unwrap_or_default();
        let joined_path = if current_path.is_empty() {
            bin_dir.display().to_string()
        } else {
            format!("{}:{}", bin_dir.display(), current_path)
        };
        let _path = EnvGuard::set("PATH", joined_path);
        let _config = EnvGuard::set(
            "CRATEBAY_CONFIG_DIR",
            config_dir.to_str().expect("config dir"),
        );
        let _models = EnvGuard::set("OLLAMA_MODELS", models_dir.to_str().expect("models dir"));
        let _base_url = EnvGuard::set("CRATEBAY_OLLAMA_BASE_URL", &server.base_url);

        let status = ollama_status().await.expect("ollama status");
        assert!(status.installed);
        assert!(status.running);
        assert_eq!(status.version, "0.5.7-test");
        assert_eq!(status.base_url, server.base_url);

        let models = ollama_list_models().await.expect("initial ollama models");
        assert_eq!(models.len(), 1);
        assert_eq!(models[0].name, "qwen2.5:7b");

        let storage = ollama_storage_info().await.expect("ollama storage info");
        assert!(storage.exists);
        assert_eq!(storage.model_count, 1);
        assert_eq!(PathBuf::from(storage.path), models_dir);

        let pull = ollama_pull_model("smoke:test".to_string())
            .await
            .expect("pull fake model");
        assert!(pull.ok);
        assert!(pull.message.contains("pulled smoke:test"));

        let pulled_models = ollama_list_models().await.expect("models after pull");
        assert!(pulled_models.iter().any(|item| item.name == "smoke:test"));

        let storage_after_pull = ollama_storage_info().await.expect("storage after pull");
        assert_eq!(storage_after_pull.model_count, 2);

        let delete = ollama_delete_model("smoke:test".to_string())
            .await
            .expect("delete fake model");
        assert!(delete.ok);
        assert!(delete.message.contains("removed smoke:test"));

        let models_after_delete = ollama_list_models().await.expect("models after delete");
        assert_eq!(models_after_delete.len(), 1);
        assert_eq!(models_after_delete[0].name, "qwen2.5:7b");
    }

    #[tokio::test]
    #[ignore = "requires real OpenAI canary credentials"]
    async fn openai_provider_canary_real_connection() {
        let _lock = env_lock();
        let tmp = TempDir::new("cratebay-openai-provider-canary");
        let (_config, _data, _log, _secret_dir) = configure_canary_dirs(&tmp);

        let profile_id = "openai-canary";
        let api_key_ref = "OPENAI_CANARY_API_KEY";
        let api_key = require_env("CRATEBAY_CANARY_OPENAI_API_KEY");
        let base_url = std::env::var("CRATEBAY_CANARY_OPENAI_BASE_URL")
            .unwrap_or_else(|_| "https://api.openai.com/v1".to_string());
        let model = std::env::var("CRATEBAY_CANARY_OPENAI_MODEL")
            .unwrap_or_else(|_| "gpt-4.1-mini".to_string());

        let mut settings = default_ai_settings();
        settings.profiles = vec![ai_profile(
            profile_id,
            "openai",
            "OpenAI Canary",
            &model,
            &base_url,
            api_key_ref,
        )];
        settings.active_profile_id = profile_id.to_string();
        save_ai_settings(settings).expect("save OpenAI canary settings");
        secret_set(api_key_ref, &api_key).expect("store OpenAI canary API key");

        let result = ai_test_connection(
            Some(profile_id.to_string()),
            Some(canary_timeout("CRATEBAY_CANARY_OPENAI_TIMEOUT_SEC", 25) * 1000),
        )
        .await
        .expect("run OpenAI canary connection test");
        let _ = secret_delete(api_key_ref);

        assert!(result.ok, "OpenAI canary failed: {}", result.message);
        assert!(
            result.message.to_ascii_uppercase().contains("PONG"),
            "OpenAI canary response should contain PONG: {}",
            result.message
        );
    }

    #[tokio::test]
    #[ignore = "requires real Anthropic canary credentials"]
    async fn anthropic_provider_canary_real_connection() {
        let _lock = env_lock();
        let tmp = TempDir::new("cratebay-anthropic-provider-canary");
        let (_config, _data, _log, _secret_dir) = configure_canary_dirs(&tmp);

        let profile_id = "anthropic-canary";
        let api_key_ref = "ANTHROPIC_CANARY_API_KEY";
        let api_key = require_env("CRATEBAY_CANARY_ANTHROPIC_API_KEY");
        let base_url = std::env::var("CRATEBAY_CANARY_ANTHROPIC_BASE_URL")
            .unwrap_or_else(|_| "https://api.anthropic.com/v1".to_string());
        let model = std::env::var("CRATEBAY_CANARY_ANTHROPIC_MODEL")
            .unwrap_or_else(|_| "claude-3-7-sonnet-latest".to_string());

        let mut settings = default_ai_settings();
        settings.profiles = vec![ai_profile(
            profile_id,
            "anthropic",
            "Anthropic Canary",
            &model,
            &base_url,
            api_key_ref,
        )];
        settings.active_profile_id = profile_id.to_string();
        save_ai_settings(settings).expect("save Anthropic canary settings");
        secret_set(api_key_ref, &api_key).expect("store Anthropic canary API key");

        let result = ai_test_connection(
            Some(profile_id.to_string()),
            Some(canary_timeout("CRATEBAY_CANARY_ANTHROPIC_TIMEOUT_SEC", 25) * 1000),
        )
        .await
        .expect("run Anthropic canary connection test");
        let _ = secret_delete(api_key_ref);

        assert!(result.ok, "Anthropic canary failed: {}", result.message);
        assert!(
            result.message.to_ascii_uppercase().contains("PONG"),
            "Anthropic canary response should contain PONG: {}",
            result.message
        );
    }

    #[tokio::test]
    #[ignore = "requires real Codex CLI bridge"]
    async fn codex_cli_bridge_canary() {
        let _lock = env_lock();
        let _target = require_env("CRATEBAY_CANARY_CODEX_BIN");
        let tmp = TempDir::new("cratebay-codex-cli-canary");
        let (_config, _data, _log, _secret_dir) = configure_canary_dirs(&tmp);
        let bin_dir = tmp.path.join("bin");
        write_forwarder_binary(&bin_dir, "codex", "CRATEBAY_CANARY_CODEX_BIN");
        let _path = EnvGuard::set("PATH", prepend_path(&bin_dir));
        let prompt = std::env::var("CRATEBAY_CANARY_CODEX_PROMPT")
            .unwrap_or_else(|_| "Reply with PONG and exit.".to_string());

        let result = agent_cli_run(
            Some("codex".to_string()),
            None,
            None,
            Some(prompt),
            false,
            Some(canary_timeout("CRATEBAY_CANARY_CODEX_TIMEOUT_SEC", 60)),
        )
        .await
        .expect("run Codex CLI canary");

        let combined = format!(
            "{}
{}",
            result.stdout, result.stderr
        )
        .to_ascii_uppercase();
        assert!(result.ok, "Codex CLI canary failed: {:?}", result);
        assert!(
            result.command_line.starts_with("codex exec "),
            "Codex preset command should use preset form: {}",
            result.command_line
        );
        assert!(
            combined.contains("PONG"),
            "Codex CLI output should contain PONG: {}",
            combined
        );
    }

    #[tokio::test]
    #[ignore = "requires real Claude CLI bridge"]
    async fn claude_cli_bridge_canary() {
        let _lock = env_lock();
        let _target = require_env("CRATEBAY_CANARY_CLAUDE_BIN");
        let tmp = TempDir::new("cratebay-claude-cli-canary");
        let (_config, _data, _log, _secret_dir) = configure_canary_dirs(&tmp);
        let bin_dir = tmp.path.join("bin");
        write_forwarder_binary(&bin_dir, "claude", "CRATEBAY_CANARY_CLAUDE_BIN");
        let _path = EnvGuard::set("PATH", prepend_path(&bin_dir));
        let prompt = std::env::var("CRATEBAY_CANARY_CLAUDE_PROMPT")
            .unwrap_or_else(|_| "Reply with PONG and exit.".to_string());

        let result = agent_cli_run(
            Some("claude".to_string()),
            None,
            None,
            Some(prompt),
            false,
            Some(canary_timeout("CRATEBAY_CANARY_CLAUDE_TIMEOUT_SEC", 60)),
        )
        .await
        .expect("run Claude CLI canary");

        let combined = format!(
            "{}
{}",
            result.stdout, result.stderr
        )
        .to_ascii_uppercase();
        assert!(result.ok, "Claude CLI canary failed: {:?}", result);
        assert!(
            result.command_line.starts_with("claude --print "),
            "Claude preset command should use preset form: {}",
            result.command_line
        );
        assert!(
            combined.contains("PONG"),
            "Claude CLI output should contain PONG: {}",
            combined
        );
    }

    #[tokio::test]
    #[ignore = "requires real Ollama daemon"]
    async fn ollama_real_daemon_canary_smoke() {
        let _lock = env_lock();
        let tmp = TempDir::new("cratebay-ollama-daemon-canary");
        let (_config, _data, _log, _secret_dir) = configure_canary_dirs(&tmp);

        let _base = std::env::var("CRATEBAY_CANARY_OLLAMA_BASE_URL")
            .ok()
            .map(|value| EnvGuard::set("CRATEBAY_OLLAMA_BASE_URL", value));
        let _models = std::env::var("CRATEBAY_CANARY_OLLAMA_MODELS_DIR")
            .ok()
            .map(|value| EnvGuard::set("OLLAMA_MODELS", value));

        let status = ollama_status().await.expect("ollama daemon status");
        assert!(
            status.installed,
            "Ollama daemon canary expects installed=true"
        );
        assert!(status.running, "Ollama daemon canary expects running=true");

        let models = ollama_list_models().await.expect("ollama daemon models");
        if let Ok(expected_model) = std::env::var("CRATEBAY_CANARY_OLLAMA_EXPECT_MODEL") {
            assert!(
                models.iter().any(|item| item.name == expected_model),
                "expected Ollama model '{}' to be present; got {:?}",
                expected_model,
                models
                    .iter()
                    .map(|item| item.name.clone())
                    .collect::<Vec<_>>()
            );
        } else {
            assert!(
                !models.is_empty(),
                "Ollama daemon canary expects at least one model"
            );
        }

        let storage = ollama_storage_info()
            .await
            .expect("ollama daemon storage info");
        assert!(storage.exists, "Ollama storage should exist");
        assert!(
            storage.model_count >= 1,
            "Ollama storage should report at least one model"
        );

        if let Ok(model) = std::env::var("CRATEBAY_CANARY_OLLAMA_PULL_MODEL") {
            let pull = ollama_pull_model(model.clone())
                .await
                .expect("pull Ollama canary model");
            assert!(pull.ok, "Ollama pull should succeed: {}", pull.message);
            let delete = ollama_delete_model(model.clone())
                .await
                .expect("delete Ollama canary model");
            assert!(
                delete.ok,
                "Ollama delete should succeed: {}",
                delete.message
            );
        }
    }

    #[tokio::test]
    #[ignore = "requires local process runtime"]
    async fn mcp_runtime_smoke_lifecycle() {
        let _lock = env_lock();

        let tmp = TempDir::new("cratebay-mcp-smoke");
        let config_dir = tmp.path.join("config");
        let _config = EnvGuard::set(
            "CRATEBAY_CONFIG_DIR",
            config_dir.to_str().expect("config dir"),
        );

        let mut settings = default_ai_settings();
        settings.mcp_servers = vec![McpServerEntry {
            id: "local-smoke".to_string(),
            name: "Local Smoke MCP".to_string(),
            command: "/bin/sh".to_string(),
            args: vec![
                "-lc".to_string(),
                "echo MCP_READY; while true; do sleep 1; done".to_string(),
            ],
            env: vec!["CRATEBAY_MCP=1".to_string()],
            working_dir: tmp.path.display().to_string(),
            enabled: true,
            notes: "runtime smoke".to_string(),
        }];
        save_ai_settings(settings).expect("save MCP test settings");

        let state = test_app_state();
        let started = mcp_start_server_inner(&state, "local-smoke".to_string())
            .await
            .expect("start MCP runtime");
        assert!(started.ok);
        assert!(started.message.contains("Started Local Smoke MCP"));

        tokio::time::sleep(Duration::from_millis(250)).await;

        let servers = mcp_list_servers_inner(&state).expect("list MCP servers");
        let server = servers
            .iter()
            .find(|entry| entry.id == "local-smoke")
            .expect("started MCP server should be listed");
        assert!(server.running);
        assert_eq!(server.status, "running");
        assert!(server.pid.is_some());

        let logs = mcp_server_logs_inner(&state, "local-smoke".to_string(), Some(20))
            .expect("read MCP runtime logs");
        assert!(
            logs.iter().any(|line| line.contains("MCP_READY"))
                || logs.iter().any(|line| line.contains("started pid="))
        );

        let exported =
            mcp_export_client_config("codex".to_string()).expect("export codex MCP config");
        assert!(exported.contains("\"local-smoke\""));
        assert!(exported.contains("\"/bin/sh\""));

        let loaded = load_ai_settings().expect("reload AI settings");
        assert_eq!(loaded.mcp_servers.len(), 1);
        assert_eq!(loaded.mcp_servers[0].id, "local-smoke");

        let stopped = mcp_stop_server_inner(&state, "local-smoke".to_string())
            .await
            .expect("stop MCP runtime");
        assert!(stopped.ok);

        let after_stop = mcp_list_servers_inner(&state).expect("list MCP servers after stop");
        let stopped_server = after_stop
            .iter()
            .find(|entry| entry.id == "local-smoke")
            .expect("stopped MCP server should still be listed");
        assert!(!stopped_server.running);
        assert!(matches!(
            stopped_server.status.as_str(),
            "exited" | "stopped"
        ));
    }
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

    let app = builder
        .manage(AppState {
            hv: cratebay_core::create_hypervisor(),
            grpc_addr: grpc_addr(),
            daemon: Mutex::new(None),
            daemon_ready: Mutex::new(false),
            log_stream_handles: Mutex::new(HashMap::new()),
            mcp_runtimes: Mutex::new(HashMap::new()),
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

            tauri::async_runtime::spawn(async move {
                let mut interval = tokio::time::interval(Duration::from_secs(900));
                loop {
                    interval.tick().await;
                    if let Err(err) = sandbox_cleanup_expired_internal().await {
                        warn!("sandbox cleanup worker failed: {}", err);
                    }
                }
            });

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
            gpu_status,
            ollama_list_models,
            ollama_storage_info,
            ollama_pull_model,
            ollama_delete_model,
            sandbox_templates,
            sandbox_list,
            sandbox_create,
            sandbox_start,
            sandbox_stop,
            sandbox_delete,
            sandbox_inspect,
            sandbox_runtime_usage,
            sandbox_audit_list,
            sandbox_cleanup_expired,
            sandbox_exec,
            load_ai_settings,
            save_ai_settings,
            mcp_list_servers,
            mcp_save_servers,
            mcp_start_server,
            mcp_stop_server,
            mcp_server_logs,
            mcp_export_client_config,
            opensandbox_status,
            ai_skill_execute,
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
        .build(tauri::generate_context!())
        .expect("error while building tauri application");

    app.run(|app_handle, event| {
        #[cfg(target_os = "macos")]
        if let RunEvent::Reopen {
            has_visible_windows: false,
            ..
        } = event
        {
            if let Some(window) = app_handle.get_webview_window("main") {
                let _ = window.unminimize();
                let _ = window.show();
                let _ = window.set_focus();
            }
        }
    });
}
