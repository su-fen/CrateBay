use bollard::container::{
    Config, CreateContainerOptions, InspectContainerOptions, ListContainersOptions, LogsOptions,
    RemoveContainerOptions, StartContainerOptions, StopContainerOptions,
};
use bollard::image::{CreateImageOptions, ListImagesOptions, RemoveImageOptions, TagImageOptions};
use bollard::service::HostConfig;
use bollard::volume::{CreateVolumeOptions, ListVolumesOptions};
use bollard::Docker;
use clap::{Parser, Subcommand};
use futures_util::stream::TryStreamExt;
use reqwest::header::WWW_AUTHENTICATE;
use serde::Deserialize;
use std::collections::HashMap;
use std::path::Path;
use std::time::{Duration, Instant};
use tonic::transport::Channel;

use cargobay_core::proto;
use cargobay_core::proto::vm_service_client::VmServiceClient;

#[derive(Parser)]
#[command(
    name = "cargobay",
    version = "0.1.0",
    about = "Free, open-source desktop for containers and Linux VMs"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// VM management commands
    Vm {
        #[command(subcommand)]
        command: VmCommands,
    },
    /// Docker container management
    Docker {
        #[command(subcommand)]
        command: DockerCommands,
    },
    /// Image management commands
    Image {
        #[command(subcommand)]
        command: ImageCommands,
    },
    /// File sharing management (VirtioFS)
    Mount {
        #[command(subcommand)]
        command: MountCommands,
    },
    /// Docker volume management
    Volume {
        #[command(subcommand)]
        command: VolumeCommands,
    },
    /// K3s (lightweight Kubernetes) management
    K3s {
        #[command(subcommand)]
        command: K3sCommands,
    },
    /// Show system status and platform info
    Status,
}

#[derive(Subcommand)]
enum VmCommands {
    /// Create a new VM
    Create {
        name: String,
        #[arg(long, default_value = "2")]
        cpus: u32,
        #[arg(long, default_value = "2048")]
        memory: u64,
        #[arg(long, default_value = "20")]
        disk: u64,
        /// Enable Rosetta x86_64 translation (macOS Apple Silicon only)
        #[arg(long)]
        rosetta: bool,
        /// OS image to use (e.g. "alpine-3.19"). See `cargobay image list-os`
        #[arg(long)]
        os_image: Option<String>,
    },
    /// Start a VM
    Start { name: String },
    /// Stop a VM
    Stop { name: String },
    /// Delete a VM
    Delete { name: String },
    /// List all VMs
    List,
    /// Print an SSH login command for a VM (requires an SSH endpoint)
    LoginCmd {
        name: String,
        #[arg(long, default_value = "root")]
        user: String,
        #[arg(long, default_value = "127.0.0.1")]
        host: String,
        /// SSH port (required until VM networking/port-forwarding is implemented)
        #[arg(long)]
        port: Option<u16>,
    },
    /// Port forwarding management
    Port {
        #[command(subcommand)]
        command: PortCommands,
    },
}

#[derive(Subcommand)]
enum PortCommands {
    /// Add a port forward from host to VM guest
    Add {
        /// VM name or ID
        #[arg(long)]
        vm: String,
        /// Host port to listen on
        #[arg(long)]
        host: u16,
        /// Guest port to forward to
        #[arg(long)]
        guest: u16,
        /// Protocol: tcp or udp
        #[arg(long, default_value = "tcp")]
        protocol: String,
    },
    /// List port forwards for a VM
    List {
        /// VM name or ID
        #[arg(long)]
        vm: String,
    },
    /// Remove a port forward
    Remove {
        /// VM name or ID
        #[arg(long)]
        vm: String,
        /// Host port to stop forwarding
        #[arg(long)]
        host: u16,
    },
}

#[derive(Subcommand)]
enum DockerCommands {
    /// List containers
    Ps,
    /// Start a container
    Start { id: String },
    /// Stop a container
    Stop { id: String },
    /// Remove a container
    Rm { id: String },
    /// Run a new container from an image
    Run {
        image: String,
        /// Optional container name
        #[arg(long)]
        name: Option<String>,
        /// Limit CPU cores (e.g. 2)
        #[arg(long)]
        cpus: Option<u32>,
        /// Limit memory in MB (e.g. 2048)
        #[arg(long)]
        memory: Option<u64>,
        /// Pull image before creating the container
        #[arg(long)]
        pull: bool,
        /// Set environment variables (can be repeated, e.g. --env KEY=VALUE)
        #[arg(long = "env", short = 'e')]
        env: Vec<String>,
    },
    /// Print a shell login command for a container
    LoginCmd {
        container: String,
        #[arg(long, default_value = "/bin/sh")]
        shell: String,
    },
    /// Show logs for a container
    Logs {
        /// Container name or ID
        container: String,
        /// Number of lines to show from the end of the logs (or "all")
        #[arg(long, default_value = "200")]
        tail: String,
        /// Show timestamps
        #[arg(long)]
        timestamps: bool,
    },
    /// Show environment variables of a container
    Env {
        /// Container name or ID
        id: String,
    },
}

#[derive(Subcommand)]
enum ImageCommands {
    /// Search images (Docker Hub / Quay) or list tags for a registry reference
    Search {
        query: String,
        /// dockerhub | quay | all
        #[arg(long, default_value = "all")]
        source: String,
        #[arg(long, default_value_t = 20)]
        limit: usize,
    },
    /// List tags for an OCI image reference (e.g. ghcr.io/org/image)
    Tags {
        reference: String,
        #[arg(long, default_value_t = 50)]
        limit: usize,
    },
    /// List local Docker images
    List,
    /// Remove a local Docker image
    Remove {
        /// Image ID or reference (e.g. nginx:latest)
        reference: String,
    },
    /// Tag a local Docker image with a new name
    Tag {
        /// Source image reference (e.g. nginx:latest)
        source: String,
        /// Target tag in repo:tag format (e.g. myrepo/nginx:v1)
        target: String,
    },
    /// Inspect a local Docker image (show details as JSON)
    Inspect {
        /// Image ID or reference
        reference: String,
    },
    /// Load a local image archive into Docker (same as `docker load -i`)
    Load { path: String },
    /// Push an image to a registry (same as `docker push`)
    Push { reference: String },
    /// Package an image from an existing container (same as `docker commit`)
    PackContainer { container: String, tag: String },
    /// List available Linux OS images for VM booting
    ListOs,
    /// Download a Linux OS image (kernel + initrd + rootfs) for VM booting
    DownloadOs {
        /// Image id, e.g. "alpine-3.19", "ubuntu-24.04", "debian-12"
        name: String,
    },
    /// Delete a downloaded Linux OS image
    DeleteOs {
        /// Image id, e.g. "alpine-3.19"
        name: String,
    },
}

#[derive(Subcommand)]
enum MountCommands {
    /// Mount a host directory into a VM via VirtioFS
    Add {
        /// VM name or ID
        #[arg(long)]
        vm: String,
        /// Tag for the mount
        #[arg(long)]
        tag: String,
        /// Host path to share
        #[arg(long)]
        host_path: String,
        /// Guest mount point
        #[arg(long, default_value = "/mnt/host")]
        guest_path: String,
        /// Mount as read-only
        #[arg(long)]
        readonly: bool,
    },
    /// Unmount a VirtioFS share from a VM
    Remove {
        /// VM name or ID
        #[arg(long)]
        vm: String,
        /// Tag of the mount to remove
        #[arg(long)]
        tag: String,
    },
    /// List VirtioFS mounts for a VM
    List {
        /// VM name or ID
        #[arg(long)]
        vm: String,
    },
}

#[derive(Subcommand)]
enum VolumeCommands {
    /// List all Docker volumes
    List,
    /// Create a Docker volume
    Create {
        /// Volume name
        name: String,
        /// Volume driver (default: local)
        #[arg(long, default_value = "local")]
        driver: String,
    },
    /// Inspect a Docker volume (show details as JSON)
    Inspect {
        /// Volume name
        name: String,
    },
    /// Remove a Docker volume
    Remove {
        /// Volume name
        name: String,
    },
}

#[derive(Subcommand)]
enum K3sCommands {
    /// Show K3s cluster status
    Status,
    /// Download the K3s binary
    Install,
    /// Start the K3s cluster
    Start,
    /// Stop the K3s cluster
    Stop,
    /// Remove K3s binary and data
    Uninstall,
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

    // Windows named pipe detection
    #[cfg(windows)]
    {
        // Docker Desktop for Windows uses named pipes
        let candidates = [
            r"//./pipe/docker_engine",
            r"//./pipe/dockerDesktopLinuxEngine",
        ];
        for pipe in &candidates {
            if Path::new(pipe).exists() {
                return Some(pipe.to_string());
            }
        }
        // WSL2 Docker socket
        let userprofile = std::env::var("USERPROFILE").unwrap_or_default();
        let wsl_sock = format!(r"{}\\.docker\run\docker.sock", userprofile);
        if Path::new(&wsl_sock).exists() {
            return Some(wsl_sock);
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

#[tokio::main]
async fn main() {
    cargobay_core::logging::init();
    let cli = Cli::parse();
    match cli.command {
        Commands::Vm { command } => handle_vm(command).await,
        Commands::Docker { command } => {
            if let Err(e) = handle_docker(command).await {
                eprintln!("Error: {}", e);
                std::process::exit(1);
            }
        }
        Commands::Image { command } => {
            if let Err(e) = handle_image(command).await {
                eprintln!("Error: {}", e);
                std::process::exit(1);
            }
        }
        Commands::Mount { command } => handle_mount(command).await,
        Commands::Volume { command } => {
            if let Err(e) = handle_volume(command).await {
                eprintln!("Error: {}", e);
                std::process::exit(1);
            }
        }
        Commands::K3s { command } => {
            if let Err(e) = handle_k3s(command).await {
                eprintln!("Error: {}", e);
                std::process::exit(1);
            }
        }
        Commands::Status => {
            println!("CargoBay v0.1.0");
            println!("Platform: {}", cargobay_core::platform_info());
            let hv = cargobay_core::create_hypervisor();
            println!(
                "Rosetta x86_64: {}",
                if hv.rosetta_available() {
                    "available"
                } else {
                    "not available"
                }
            );
            match detect_docker_socket() {
                Some(sock) => println!("Docker: connected ({})", sock),
                None => println!("Docker: not found"),
            }

            let addr = grpc_addr();
            match connect_vm_service(&addr).await {
                Ok(_) => println!("Daemon gRPC: connected ({})", addr),
                Err(_) => println!("Daemon gRPC: not running ({})", addr),
            }
        }
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

async fn connect_vm_service_timeout(
    addr: &str,
    timeout: Duration,
) -> Result<VmServiceClient<Channel>, String> {
    let endpoint = grpc_endpoint(addr);
    let connect_fut = VmServiceClient::connect(endpoint.clone());
    let client = tokio::time::timeout(timeout, connect_fut)
        .await
        .map_err(|_| format!("Timed out connecting to daemon at {}", endpoint))?
        .map_err(|e| format!("Failed to connect to daemon at {}: {}", endpoint, e))?;
    Ok(client)
}

async fn connect_vm_service(addr: &str) -> Result<VmServiceClient<Channel>, String> {
    connect_vm_service_timeout(addr, Duration::from_secs(1)).await
}

fn daemon_file_name() -> &'static str {
    #[cfg(windows)]
    {
        "cargobay-daemon.exe"
    }
    #[cfg(not(windows))]
    {
        "cargobay-daemon"
    }
}

fn daemon_path() -> std::path::PathBuf {
    if let Ok(path) = std::env::var("CARGOBAY_DAEMON_PATH") {
        return path.into();
    }

    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let candidate = dir.join(daemon_file_name());
            if candidate.is_file() {
                return candidate;
            }
        }
    }

    daemon_file_name().into()
}

fn spawn_daemon_detached() -> Result<u32, String> {
    use std::process::{Command as ProcessCommand, Stdio};

    let daemon = daemon_path();
    let mut cmd = ProcessCommand::new(&daemon);
    cmd.stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());

    let child = cmd
        .spawn()
        .map_err(|e| format!("Failed to spawn {}: {}", daemon.display(), e))?;
    Ok(child.id())
}

async fn wait_for_vm_service(
    addr: &str,
    timeout: Duration,
) -> Result<VmServiceClient<Channel>, String> {
    let deadline = Instant::now() + timeout;
    loop {
        if let Ok(client) = connect_vm_service_timeout(addr, Duration::from_millis(200)).await {
            return Ok(client);
        }

        if Instant::now() >= deadline {
            return Err(format!(
                "Timed out waiting for daemon to become ready at {}",
                grpc_endpoint(addr)
            ));
        }

        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

async fn connect_vm_service_autostart(addr: &str) -> Option<VmServiceClient<Channel>> {
    if let Ok(client) = connect_vm_service(addr).await {
        return Some(client);
    }

    if spawn_daemon_detached().is_err() {
        return None;
    }

    wait_for_vm_service(addr, Duration::from_secs(5)).await.ok()
}

async fn resolve_vm_id_grpc(
    client: &mut VmServiceClient<Channel>,
    selector: &str,
) -> Result<String, String> {
    let resp = client
        .list_v_ms(proto::ListVMsRequest {})
        .await
        .map_err(|e| format!("Failed to list VMs: {}", e))?
        .into_inner();

    if resp.vms.iter().any(|vm| vm.vm_id == selector) {
        return Ok(selector.to_string());
    }
    if let Some(vm) = resp.vms.iter().find(|vm| vm.name == selector) {
        return Ok(vm.vm_id.clone());
    }
    Err(format!("VM not found: {}", selector))
}

fn resolve_vm_id_local(
    hv: &dyn cargobay_core::hypervisor::Hypervisor,
    selector: &str,
) -> Result<String, cargobay_core::hypervisor::HypervisorError> {
    let vms = hv.list_vms()?;
    if vms.iter().any(|vm| vm.id == selector) {
        return Ok(selector.to_string());
    }
    if let Some(vm) = vms.into_iter().find(|vm| vm.name == selector) {
        return Ok(vm.id);
    }
    Err(cargobay_core::hypervisor::HypervisorError::NotFound(
        selector.into(),
    ))
}

async fn handle_vm(cmd: VmCommands) {
    let addr = grpc_addr();
    let mut client = connect_vm_service_autostart(&addr).await;

    let hv = if client.is_none() {
        Some(cargobay_core::create_hypervisor())
    } else {
        None
    };

    match cmd {
        VmCommands::Create {
            name,
            cpus,
            memory,
            disk,
            rosetta,
            os_image,
        } => {
            // Resolve image paths if an OS image was specified.
            let (kernel_path, initrd_path, disk_path) = if let Some(ref img_id) = os_image {
                if !cargobay_core::images::is_image_ready(img_id) {
                    eprintln!("Error: OS image '{}' is not downloaded yet. Run: cargobay image download-os {}", img_id, img_id);
                    std::process::exit(1);
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

            if let Some(client) = client.as_mut() {
                let resp = client
                    .create_vm(proto::CreateVmRequest {
                        name: name.clone(),
                        cpus,
                        memory_mb: memory,
                        disk_gb: disk,
                        rosetta,
                        shared_dirs: vec![],
                    })
                    .await;
                match resp {
                    Ok(r) => {
                        let id = r.into_inner().vm_id;
                        println!("Created VM '{}' (id: {})", name, id);
                        if rosetta {
                            println!("  Rosetta x86_64 translation: enabled");
                        }
                        if let Some(ref img) = os_image {
                            println!("  OS image: {}", img);
                        }
                    }
                    Err(e) => {
                        eprintln!("Error: {}", e);
                        std::process::exit(1);
                    }
                }
            } else {
                use cargobay_core::hypervisor::VmConfig;
                let hv = hv.as_ref().unwrap();
                let config = VmConfig {
                    name: name.clone(),
                    cpus,
                    memory_mb: memory,
                    disk_gb: disk,
                    rosetta,
                    shared_dirs: vec![],
                    os_image: os_image.clone(),
                    kernel_path,
                    initrd_path,
                    disk_path,
                    port_forwards: vec![],
                };
                match hv.create_vm(config) {
                    Ok(id) => {
                        println!("Created VM '{}' (id: {})", name, id);
                        if rosetta {
                            println!("  Rosetta x86_64 translation: enabled");
                        }
                        if let Some(ref img) = os_image {
                            println!("  OS image: {}", img);
                        }
                    }
                    Err(e) => {
                        eprintln!("Error: {}", e);
                        std::process::exit(1);
                    }
                }
            }
        }
        VmCommands::Start { name } => {
            if let Some(client) = client.as_mut() {
                let id = match resolve_vm_id_grpc(client, &name).await {
                    Ok(id) => id,
                    Err(e) => {
                        eprintln!("Error: {}", e);
                        std::process::exit(1);
                    }
                };
                if let Err(e) = client
                    .start_vm(proto::StartVmRequest { vm_id: id.clone() })
                    .await
                {
                    eprintln!("Error: {}", e);
                    std::process::exit(1);
                }
                println!("Started VM '{}'", name);
            } else {
                let hv = hv.as_ref().unwrap();
                let id = match resolve_vm_id_local(hv.as_ref(), &name) {
                    Ok(id) => id,
                    Err(e) => {
                        eprintln!("Error: {}", e);
                        std::process::exit(1);
                    }
                };
                match hv.start_vm(&id) {
                    Ok(()) => println!("Started VM '{}'", name),
                    Err(e) => {
                        eprintln!("Error: {}", e);
                        std::process::exit(1);
                    }
                }
            }
        }
        VmCommands::Stop { name } => {
            if let Some(client) = client.as_mut() {
                let id = match resolve_vm_id_grpc(client, &name).await {
                    Ok(id) => id,
                    Err(e) => {
                        eprintln!("Error: {}", e);
                        std::process::exit(1);
                    }
                };
                if let Err(e) = client
                    .stop_vm(proto::StopVmRequest { vm_id: id.clone() })
                    .await
                {
                    eprintln!("Error: {}", e);
                    std::process::exit(1);
                }
                println!("Stopped VM '{}'", name);
            } else {
                let hv = hv.as_ref().unwrap();
                let id = match resolve_vm_id_local(hv.as_ref(), &name) {
                    Ok(id) => id,
                    Err(e) => {
                        eprintln!("Error: {}", e);
                        std::process::exit(1);
                    }
                };
                match hv.stop_vm(&id) {
                    Ok(()) => println!("Stopped VM '{}'", name),
                    Err(e) => {
                        eprintln!("Error: {}", e);
                        std::process::exit(1);
                    }
                }
            }
        }
        VmCommands::Delete { name } => {
            if let Some(client) = client.as_mut() {
                let id = match resolve_vm_id_grpc(client, &name).await {
                    Ok(id) => id,
                    Err(e) => {
                        eprintln!("Error: {}", e);
                        std::process::exit(1);
                    }
                };
                if let Err(e) = client
                    .delete_vm(proto::DeleteVmRequest { vm_id: id.clone() })
                    .await
                {
                    eprintln!("Error: {}", e);
                    std::process::exit(1);
                }
                println!("Deleted VM '{}'", name);
            } else {
                let hv = hv.as_ref().unwrap();
                let id = match resolve_vm_id_local(hv.as_ref(), &name) {
                    Ok(id) => id,
                    Err(e) => {
                        eprintln!("Error: {}", e);
                        std::process::exit(1);
                    }
                };
                match hv.delete_vm(&id) {
                    Ok(()) => println!("Deleted VM '{}'", name),
                    Err(e) => {
                        eprintln!("Error: {}", e);
                        std::process::exit(1);
                    }
                }
            }
        }
        VmCommands::List => {
            if let Some(client) = client.as_mut() {
                let resp = client.list_v_ms(proto::ListVMsRequest {}).await;
                match resp {
                    Ok(r) => {
                        let vms = r.into_inner().vms;
                        if vms.is_empty() {
                            println!("No VMs found.");
                            return;
                        }
                        println!(
                            "{:<12} {:<20} {:<10} {:<6} {:<8} {:<8} MOUNTS",
                            "ID", "NAME", "STATE", "CPUS", "MEMORY", "ROSETTA"
                        );
                        for vm in vms {
                            println!(
                                "{:<12} {:<20} {:<10} {:<6} {:<8} {:<8} {}",
                                vm.vm_id,
                                vm.name,
                                vm.status,
                                vm.cpus,
                                format!("{}MB", vm.memory_mb),
                                if vm.rosetta_enabled { "yes" } else { "no" },
                                vm.shared_dirs.len(),
                            );
                        }
                    }
                    Err(e) => {
                        eprintln!("Error: {}", e);
                        std::process::exit(1);
                    }
                }
            } else {
                let hv = hv.as_ref().unwrap();
                match hv.list_vms() {
                    Ok(vms) => {
                        if vms.is_empty() {
                            println!("No VMs found.");
                            return;
                        }
                        println!(
                            "{:<12} {:<20} {:<10} {:<6} {:<8} {:<8} MOUNTS",
                            "ID", "NAME", "STATE", "CPUS", "MEMORY", "ROSETTA"
                        );
                        for vm in vms {
                            println!(
                                "{:<12} {:<20} {:<10} {:<6} {:<8} {:<8} {}",
                                vm.id,
                                vm.name,
                                format!("{:?}", vm.state),
                                vm.cpus,
                                format!("{}MB", vm.memory_mb),
                                if vm.rosetta_enabled { "yes" } else { "no" },
                                vm.shared_dirs.len(),
                            );
                        }
                    }
                    Err(e) => {
                        eprintln!("Error: {}", e);
                        std::process::exit(1);
                    }
                }
            }
        }
        VmCommands::LoginCmd {
            name,
            user,
            host,
            port,
        } => {
            let Some(port) = port else {
                eprintln!("Error: VM login is not available yet. Specify an SSH port via --port.");
                std::process::exit(1);
            };
            println!("ssh {}@{} -p {}", user, host, port);
            println!("# VM: {}", name);
        }
        VmCommands::Port { command } => {
            handle_port(command, client.as_mut(), hv.as_deref()).await;
        }
    }
}

async fn handle_port(
    cmd: PortCommands,
    client: Option<&mut VmServiceClient<Channel>>,
    hv: Option<&dyn cargobay_core::hypervisor::Hypervisor>,
) {
    match cmd {
        PortCommands::Add {
            vm,
            host,
            guest,
            protocol,
        } => {
            if let Some(client) = client {
                let vm_id = match resolve_vm_id_grpc(client, &vm).await {
                    Ok(id) => id,
                    Err(e) => {
                        eprintln!("Error: {}", e);
                        std::process::exit(1);
                    }
                };
                if let Err(e) = client
                    .add_port_forward(proto::AddPortForwardRequest {
                        vm_id,
                        host_port: host as u32,
                        guest_port: guest as u32,
                        protocol: protocol.clone(),
                    })
                    .await
                {
                    eprintln!("Error: {}", e);
                    std::process::exit(1);
                }
                println!(
                    "Added port forward: host {} -> guest {} ({})",
                    host, guest, protocol
                );
            } else {
                let hv = hv.unwrap();
                let vm_id = match resolve_vm_id_local(hv, &vm) {
                    Ok(id) => id,
                    Err(e) => {
                        eprintln!("Error: {}", e);
                        std::process::exit(1);
                    }
                };
                let pf = cargobay_core::hypervisor::PortForward {
                    host_port: host,
                    guest_port: guest,
                    protocol: protocol.clone(),
                };
                match hv.add_port_forward(&vm_id, &pf) {
                    Ok(()) => {
                        println!(
                            "Added port forward: host {} -> guest {} ({})",
                            host, guest, protocol
                        );
                    }
                    Err(e) => {
                        eprintln!("Error: {}", e);
                        std::process::exit(1);
                    }
                }
            }
        }
        PortCommands::List { vm } => {
            if let Some(client) = client {
                let vm_id = match resolve_vm_id_grpc(client, &vm).await {
                    Ok(id) => id,
                    Err(e) => {
                        eprintln!("Error: {}", e);
                        std::process::exit(1);
                    }
                };
                match client
                    .list_port_forwards(proto::ListPortForwardsRequest { vm_id })
                    .await
                {
                    Ok(resp) => {
                        let forwards = resp.into_inner().forwards;
                        if forwards.is_empty() {
                            println!("No port forwards configured.");
                            return;
                        }
                        println!("{:<12} {:<12} PROTOCOL", "HOST PORT", "GUEST PORT");
                        for pf in forwards {
                            println!("{:<12} {:<12} {}", pf.host_port, pf.guest_port, pf.protocol);
                        }
                    }
                    Err(e) => {
                        eprintln!("Error: {}", e);
                        std::process::exit(1);
                    }
                }
            } else {
                let hv = hv.unwrap();
                let vm_id = match resolve_vm_id_local(hv, &vm) {
                    Ok(id) => id,
                    Err(e) => {
                        eprintln!("Error: {}", e);
                        std::process::exit(1);
                    }
                };
                match hv.list_port_forwards(&vm_id) {
                    Ok(forwards) => {
                        if forwards.is_empty() {
                            println!("No port forwards configured.");
                            return;
                        }
                        println!("{:<12} {:<12} PROTOCOL", "HOST PORT", "GUEST PORT");
                        for pf in forwards {
                            println!("{:<12} {:<12} {}", pf.host_port, pf.guest_port, pf.protocol);
                        }
                    }
                    Err(e) => {
                        eprintln!("Error: {}", e);
                        std::process::exit(1);
                    }
                }
            }
        }
        PortCommands::Remove { vm, host } => {
            if let Some(client) = client {
                let vm_id = match resolve_vm_id_grpc(client, &vm).await {
                    Ok(id) => id,
                    Err(e) => {
                        eprintln!("Error: {}", e);
                        std::process::exit(1);
                    }
                };
                if let Err(e) = client
                    .remove_port_forward(proto::RemovePortForwardRequest {
                        vm_id,
                        host_port: host as u32,
                    })
                    .await
                {
                    eprintln!("Error: {}", e);
                    std::process::exit(1);
                }
                println!("Removed port forward on host port {}", host);
            } else {
                let hv = hv.unwrap();
                let vm_id = match resolve_vm_id_local(hv, &vm) {
                    Ok(id) => id,
                    Err(e) => {
                        eprintln!("Error: {}", e);
                        std::process::exit(1);
                    }
                };
                match hv.remove_port_forward(&vm_id, host) {
                    Ok(()) => println!("Removed port forward on host port {}", host),
                    Err(e) => {
                        eprintln!("Error: {}", e);
                        std::process::exit(1);
                    }
                }
            }
        }
    }
}

async fn handle_mount(cmd: MountCommands) {
    let addr = grpc_addr();
    let mut client = connect_vm_service_autostart(&addr).await;

    let hv = if client.is_none() {
        Some(cargobay_core::create_hypervisor())
    } else {
        None
    };

    match cmd {
        MountCommands::Add {
            vm,
            tag,
            host_path,
            guest_path,
            readonly,
        } => {
            if let Some(client) = client.as_mut() {
                let vm_id = match resolve_vm_id_grpc(client, &vm).await {
                    Ok(id) => id,
                    Err(e) => {
                        eprintln!("Error: {}", e);
                        std::process::exit(1);
                    }
                };
                let req = proto::MountVirtioFsRequest {
                    vm_id,
                    share: Some(proto::SharedDirectory {
                        tag: tag.clone(),
                        host_path: host_path.clone(),
                        guest_path: guest_path.clone(),
                        read_only: readonly,
                    }),
                };
                if let Err(e) = client.mount_virtio_fs(req).await {
                    eprintln!("Error: {}", e);
                    std::process::exit(1);
                }
                println!(
                    "Mounted '{}' → {} (tag: {}{})",
                    host_path,
                    guest_path,
                    tag,
                    if readonly { ", read-only" } else { "" }
                );
            } else {
                use cargobay_core::hypervisor::SharedDirectory;
                let hv = hv.as_ref().unwrap();
                let vm_id = match resolve_vm_id_local(hv.as_ref(), &vm) {
                    Ok(id) => id,
                    Err(e) => {
                        eprintln!("Error: {}", e);
                        std::process::exit(1);
                    }
                };
                let share = SharedDirectory {
                    tag: tag.clone(),
                    host_path: host_path.clone(),
                    guest_path: guest_path.clone(),
                    read_only: readonly,
                };
                match hv.mount_virtiofs(&vm_id, &share) {
                    Ok(()) => {
                        println!(
                            "Mounted '{}' → {} (tag: {}{})",
                            host_path,
                            guest_path,
                            tag,
                            if readonly { ", read-only" } else { "" }
                        );
                    }
                    Err(e) => {
                        eprintln!("Error: {}", e);
                        std::process::exit(1);
                    }
                }
            }
        }
        MountCommands::Remove { vm, tag } => {
            if let Some(client) = client.as_mut() {
                let vm_id = match resolve_vm_id_grpc(client, &vm).await {
                    Ok(id) => id,
                    Err(e) => {
                        eprintln!("Error: {}", e);
                        std::process::exit(1);
                    }
                };
                if let Err(e) = client
                    .unmount_virtio_fs(proto::UnmountVirtioFsRequest {
                        vm_id,
                        tag: tag.clone(),
                    })
                    .await
                {
                    eprintln!("Error: {}", e);
                    std::process::exit(1);
                }
                println!("Unmounted tag '{}'", tag);
            } else {
                let hv = hv.as_ref().unwrap();
                let vm_id = match resolve_vm_id_local(hv.as_ref(), &vm) {
                    Ok(id) => id,
                    Err(e) => {
                        eprintln!("Error: {}", e);
                        std::process::exit(1);
                    }
                };
                match hv.unmount_virtiofs(&vm_id, &tag) {
                    Ok(()) => println!("Unmounted tag '{}'", tag),
                    Err(e) => {
                        eprintln!("Error: {}", e);
                        std::process::exit(1);
                    }
                }
            }
        }
        MountCommands::List { vm } => {
            if let Some(client) = client.as_mut() {
                let vm_id = match resolve_vm_id_grpc(client, &vm).await {
                    Ok(id) => id,
                    Err(e) => {
                        eprintln!("Error: {}", e);
                        std::process::exit(1);
                    }
                };
                let resp = client
                    .list_virtio_fs_mounts(proto::ListVirtioFsMountsRequest { vm_id })
                    .await;
                match resp {
                    Ok(r) => {
                        let mounts = r.into_inner().mounts;
                        if mounts.is_empty() {
                            println!("No VirtioFS mounts for VM '{}'.", vm);
                            return;
                        }
                        println!(
                            "{:<16} {:<30} {:<20} MODE",
                            "TAG", "HOST PATH", "GUEST PATH"
                        );
                        for m in mounts {
                            println!(
                                "{:<16} {:<30} {:<20} {}",
                                m.tag,
                                m.host_path,
                                m.guest_path,
                                if m.read_only { "ro" } else { "rw" }
                            );
                        }
                    }
                    Err(e) => {
                        eprintln!("Error: {}", e);
                        std::process::exit(1);
                    }
                }
            } else {
                let hv = hv.as_ref().unwrap();
                let vm_id = match resolve_vm_id_local(hv.as_ref(), &vm) {
                    Ok(id) => id,
                    Err(e) => {
                        eprintln!("Error: {}", e);
                        std::process::exit(1);
                    }
                };
                match hv.list_virtiofs_mounts(&vm_id) {
                    Ok(mounts) => {
                        if mounts.is_empty() {
                            println!("No VirtioFS mounts for VM '{}'.", vm);
                            return;
                        }
                        println!(
                            "{:<16} {:<30} {:<20} MODE",
                            "TAG", "HOST PATH", "GUEST PATH"
                        );
                        for m in mounts {
                            println!(
                                "{:<16} {:<30} {:<20} {}",
                                m.tag,
                                m.host_path,
                                m.guest_path,
                                if m.read_only { "ro" } else { "rw" }
                            );
                        }
                    }
                    Err(e) => {
                        eprintln!("Error: {}", e);
                        std::process::exit(1);
                    }
                }
            }
        }
    }
}

#[derive(Debug)]
struct ImageSearchItem {
    source: &'static str,
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

async fn handle_image(cmd: ImageCommands) -> Result<(), String> {
    let client = reqwest::Client::builder()
        .user_agent("CargoBay/0.1.0 (+https://github.com/coder-hhx/CargoBay)")
        .build()
        .map_err(|e| e.to_string())?;

    match cmd {
        ImageCommands::Search {
            query,
            source,
            limit,
        } => {
            if let Some((registry, repo)) = parse_registry_reference(&query) {
                let tags = list_registry_tags(&client, &registry, &repo, limit).await?;
                if tags.is_empty() {
                    println!("No tags found for {}/{}.", registry, repo);
                    return Ok(());
                }
                println!("Tags for {}/{}:", registry, repo);
                for tag in tags {
                    println!("{}/{}:{}", registry, repo, tag);
                }
                return Ok(());
            }

            let src = source.to_ascii_lowercase();
            let mut items: Vec<ImageSearchItem> = Vec::new();
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

            if items.is_empty() {
                println!("No results.");
                return Ok(());
            }

            print_image_search_results(&items);
            Ok(())
        }
        ImageCommands::Tags { reference, limit } => {
            let Some((registry, repo)) = parse_registry_reference(&reference) else {
                return Err("Invalid reference. Expected e.g. ghcr.io/org/image".into());
            };

            let tags = list_registry_tags(&client, &registry, &repo, limit).await?;
            if tags.is_empty() {
                println!("No tags found for {}/{}.", registry, repo);
                return Ok(());
            }
            println!("Tags for {}/{}:", registry, repo);
            for tag in tags {
                println!("{}/{}:{}", registry, repo, tag);
            }
            Ok(())
        }
        ImageCommands::List => {
            let docker = connect_docker()?;
            let opts = ListImagesOptions::<String> {
                all: false,
                ..Default::default()
            };
            let images = docker
                .list_images(Some(opts))
                .await
                .map_err(|e| e.to_string())?;

            if images.is_empty() {
                println!("No local images found.");
                return Ok(());
            }

            println!(
                "{:<40} {:<14} {:<12} CREATED",
                "REPOSITORY:TAG", "IMAGE ID", "SIZE"
            );
            for img in images {
                let full_id = img.id.clone();
                let short_id = if let Some(stripped) = full_id.strip_prefix("sha256:") {
                    stripped.chars().take(12).collect::<String>()
                } else {
                    full_id.chars().take(12).collect::<String>()
                };
                let size = img.size.max(0) as u64;
                let size_str = format_bytes(size);
                let created = {
                    let ts = img.created;
                    if ts > 0 {
                        // Simple UTC timestamp formatting without chrono
                        let secs_per_min = 60i64;
                        let secs_per_hour = 3600i64;
                        let secs_per_day = 86400i64;
                        let days_since_epoch = ts / secs_per_day;
                        let time_of_day = ts % secs_per_day;
                        let hours = time_of_day / secs_per_hour;
                        let minutes = (time_of_day % secs_per_hour) / secs_per_min;

                        // Simple days-since-epoch to Y-M-D (good enough for display)
                        let mut y = 1970i64;
                        let mut remaining = days_since_epoch;
                        loop {
                            let days_in_year = if (y % 4 == 0 && y % 100 != 0) || y % 400 == 0 {
                                366
                            } else {
                                365
                            };
                            if remaining < days_in_year {
                                break;
                            }
                            remaining -= days_in_year;
                            y += 1;
                        }
                        let leap = (y % 4 == 0 && y % 100 != 0) || y % 400 == 0;
                        let month_days = [
                            31,
                            if leap { 29 } else { 28 },
                            31,
                            30,
                            31,
                            30,
                            31,
                            31,
                            30,
                            31,
                            30,
                            31,
                        ];
                        let mut m = 0usize;
                        for &md in &month_days {
                            if remaining < md {
                                break;
                            }
                            remaining -= md;
                            m += 1;
                        }
                        format!(
                            "{:04}-{:02}-{:02} {:02}:{:02}",
                            y,
                            m + 1,
                            remaining + 1,
                            hours,
                            minutes
                        )
                    } else {
                        "-".to_string()
                    }
                };

                if img.repo_tags.is_empty() {
                    println!(
                        "{:<40} {:<14} {:<12} {}",
                        "<none>:<none>", short_id, size_str, created
                    );
                } else {
                    for tag in &img.repo_tags {
                        println!("{:<40} {:<14} {:<12} {}", tag, short_id, size_str, created);
                    }
                }
            }
            Ok(())
        }
        ImageCommands::Remove { reference } => {
            let docker = connect_docker()?;
            let opts = RemoveImageOptions {
                force: false,
                noprune: false,
            };
            let results = docker
                .remove_image(&reference, Some(opts), None)
                .await
                .map_err(|e| e.to_string())?;
            for info in results {
                if let Some(deleted) = info.deleted {
                    println!("Deleted: {}", deleted);
                }
                if let Some(untagged) = info.untagged {
                    println!("Untagged: {}", untagged);
                }
            }
            Ok(())
        }
        ImageCommands::Tag { source, target } => {
            let docker = connect_docker()?;
            let (repo, tag) = if let Some(pos) = target.rfind(':') {
                (&target[..pos], &target[pos + 1..])
            } else {
                (target.as_str(), "latest")
            };
            let opts = TagImageOptions { repo, tag };
            docker
                .tag_image(&source, Some(opts))
                .await
                .map_err(|e| e.to_string())?;
            println!("Tagged {} as {}", source, target);
            Ok(())
        }
        ImageCommands::Inspect { reference } => {
            let docker = connect_docker()?;
            let detail = docker
                .inspect_image(&reference)
                .await
                .map_err(|e| e.to_string())?;
            let json = serde_json::to_string_pretty(&detail).map_err(|e| e.to_string())?;
            println!("{}", json);
            Ok(())
        }
        ImageCommands::Load { path } => {
            let out = run_docker_cli(&["load", "-i", &path])?;
            if out.is_empty() {
                println!("Done.");
            } else {
                println!("{}", out);
            }
            Ok(())
        }
        ImageCommands::Push { reference } => {
            let out = run_docker_cli(&["push", &reference])?;
            if out.is_empty() {
                println!("Done.");
            } else {
                println!("{}", out);
            }
            Ok(())
        }
        ImageCommands::PackContainer { container, tag } => {
            let out = run_docker_cli(&["commit", &container, &tag])?;
            if out.is_empty() {
                println!("Done.");
            } else {
                println!("{}", out);
            }
            Ok(())
        }
        ImageCommands::ListOs => {
            let images = cargobay_core::images::list_available_images();
            if images.is_empty() {
                println!("No OS images in catalog.");
                return Ok(());
            }
            println!(
                "{:<16} {:<28} {:<10} {:<10} STATUS",
                "ID", "NAME", "VERSION", "SIZE"
            );
            for img in images {
                let size_str = format_bytes(img.size_bytes);
                let status = match img.status {
                    cargobay_core::images::ImageStatus::NotDownloaded => "not downloaded",
                    cargobay_core::images::ImageStatus::Downloading => "downloading...",
                    cargobay_core::images::ImageStatus::Ready => "ready",
                };
                println!(
                    "{:<16} {:<28} {:<10} {:<10} {}",
                    img.id, img.name, img.version, size_str, status
                );
            }
            Ok(())
        }
        ImageCommands::DownloadOs { name } => {
            let entry = cargobay_core::images::find_image(&name);
            if entry.is_none() {
                return Err(format!(
                    "Unknown OS image: '{}'. Run 'cargobay image list-os' to see available images.",
                    name
                ));
            }

            println!("Downloading OS image '{}'...", name);
            cargobay_core::images::download_image(&name, move |file, downloaded, total| {
                if total > 0 {
                    let pct = (downloaded as f64 / total as f64 * 100.0).min(100.0);
                    eprint!(
                        "\r  [{}] {}/{} ({:.1}%)    ",
                        file,
                        format_bytes(downloaded),
                        format_bytes(total),
                        pct
                    );
                }
            })
            .await
            .map_err(|e| e.to_string())?;

            eprintln!();
            println!("OS image '{}' downloaded successfully.", name);
            let paths = cargobay_core::images::image_paths(&name);
            println!("  Kernel:  {}", paths.kernel_path.display());
            println!("  Initrd:  {}", paths.initrd_path.display());
            println!("  Rootfs:  {}", paths.rootfs_path.display());
            Ok(())
        }
        ImageCommands::DeleteOs { name } => {
            cargobay_core::images::delete_image(&name).map_err(|e| e.to_string())?;
            println!("Deleted OS image '{}'.", name);
            Ok(())
        }
    }
}

async fn search_dockerhub(
    client: &reqwest::Client,
    query: &str,
    limit: usize,
) -> Result<Vec<ImageSearchItem>, String> {
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

        out.push(ImageSearchItem {
            source: "dockerhub",
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
) -> Result<Vec<ImageSearchItem>, String> {
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

        out.push(ImageSearchItem {
            source: "quay",
            reference: format!("quay.io/{}", full_name),
            description: desc,
            stars,
            pulls: None,
            official: false,
        });
    }

    Ok(out)
}

fn print_image_search_results(items: &[ImageSearchItem]) {
    println!(
        "{:<10} {:<48} {:>7} {:>12}  DESCRIPTION",
        "SOURCE", "IMAGE", "STARS", "PULLS"
    );
    for i in items {
        let stars = i.stars.map(|v| v.to_string()).unwrap_or_else(|| "-".into());
        let pulls = i.pulls.map(|v| v.to_string()).unwrap_or_else(|| "-".into());
        let mut image = i.reference.clone();
        if i.official {
            image = format!("{} (official)", image);
        }
        println!(
            "{:<10} {:<48} {:>7} {:>12}  {}",
            i.source,
            truncate_str(&image, 48),
            stars,
            pulls,
            truncate_str(&i.description, 80)
        );
    }
}

fn truncate_str(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let mut out = String::new();
    for (idx, ch) in s.chars().enumerate() {
        if idx + 1 >= max {
            break;
        }
        out.push(ch);
    }
    out.push('\u{2026}');
    out
}

fn format_bytes(bytes: u64) -> String {
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
        if let Some(service) = service {
            qp.append_pair("service", service);
        }
        if let Some(scope) = scope {
            qp.append_pair("scope", scope);
        }
        qp.append_pair("client_id", "cargobay");
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

async fn handle_docker(cmd: DockerCommands) -> Result<(), String> {
    let docker = connect_docker()?;
    match cmd {
        DockerCommands::Ps => {
            let mut filters = HashMap::new();
            filters.insert(
                "status",
                vec![
                    "running",
                    "exited",
                    "paused",
                    "created",
                    "restarting",
                    "dead",
                ],
            );
            let opts = ListContainersOptions {
                all: true,
                filters,
                ..Default::default()
            };
            let containers = docker
                .list_containers(Some(opts))
                .await
                .map_err(|e| e.to_string())?;

            println!(
                "{:<16} {:<24} {:<24} {:<16} PORTS",
                "CONTAINER ID", "NAME", "IMAGE", "STATUS"
            );
            for c in containers {
                let id =
                    c.id.as_deref()
                        .unwrap_or("")
                        .chars()
                        .take(12)
                        .collect::<String>();
                let name = c
                    .names
                    .as_ref()
                    .and_then(|n| n.first())
                    .map(|n| n.trim_start_matches('/'))
                    .unwrap_or("")
                    .to_string();
                let image = c.image.as_deref().unwrap_or("");
                let status = c.status.as_deref().unwrap_or("");
                let ports = c
                    .ports
                    .as_ref()
                    .map(|ps| {
                        ps.iter()
                            .map(|p| {
                                let private = p.private_port;
                                let public = p.public_port;
                                let typ = p.typ.map(|t| t.to_string()).unwrap_or_default();
                                match public {
                                    Some(pub_port) => format!(
                                        "{}:{}->{}/{}",
                                        p.ip.as_deref().unwrap_or("0.0.0.0"),
                                        pub_port,
                                        private,
                                        typ
                                    ),
                                    None => format!("{}/{}", private, typ),
                                }
                            })
                            .collect::<Vec<_>>()
                            .join(", ")
                    })
                    .unwrap_or_default();
                println!(
                    "{:<16} {:<24} {:<24} {:<16} {}",
                    id, name, image, status, ports
                );
            }
        }
        DockerCommands::Start { id } => {
            docker
                .start_container(&id, None::<StartContainerOptions<String>>)
                .await
                .map_err(|e| e.to_string())?;
            println!("Started container {}", id);
        }
        DockerCommands::Stop { id } => {
            docker
                .stop_container(&id, Some(StopContainerOptions { t: 10 }))
                .await
                .map_err(|e| e.to_string())?;
            println!("Stopped container {}", id);
        }
        DockerCommands::Rm { id } => {
            docker
                .remove_container(
                    &id,
                    Some(RemoveContainerOptions {
                        force: true,
                        ..Default::default()
                    }),
                )
                .await
                .map_err(|e| e.to_string())?;
            println!("Removed container {}", id);
        }
        DockerCommands::Run {
            image,
            name,
            cpus,
            memory,
            pull,
            env,
        } => {
            if pull {
                docker_pull_image(&docker, &image).await?;
            }

            let mut host_config = HostConfig::default();
            if let Some(c) = cpus {
                host_config.nano_cpus = Some((c as i64) * 1_000_000_000);
            }
            if let Some(mb) = memory {
                let bytes = (mb as i64).saturating_mul(1024).saturating_mul(1024);
                host_config.memory = Some(bytes);
            }

            let config = Config::<String> {
                image: Some(image.clone()),
                host_config: Some(host_config),
                env: if env.is_empty() { None } else { Some(env) },
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

            let display = name
                .clone()
                .unwrap_or_else(|| result.id.chars().take(12).collect());
            println!("Created and started container: {}", display);
            println!("Login command:");
            println!("  docker exec -it {} /bin/sh", display);
        }
        DockerCommands::LoginCmd { container, shell } => {
            println!("docker exec -it {} {}", container, shell);
        }
        DockerCommands::Logs {
            container,
            tail,
            timestamps,
        } => {
            let opts = LogsOptions::<String> {
                follow: false,
                stdout: true,
                stderr: true,
                timestamps,
                tail: tail.clone(),
                ..Default::default()
            };

            let mut stream = docker.logs(&container, Some(opts));
            while let Some(chunk) = stream.try_next().await.map_err(|e| e.to_string())? {
                print!("{}", chunk);
            }
        }
        DockerCommands::Env { id } => {
            let inspect = docker
                .inspect_container(&id, None::<InspectContainerOptions>)
                .await
                .map_err(|e| format!("Failed to inspect container {}: {}", id, e))?;

            let env_list = inspect.config.and_then(|c| c.env).unwrap_or_default();

            if env_list.is_empty() {
                println!("No environment variables set.");
            } else {
                println!("{:<32} VALUE", "KEY");
                for entry in env_list {
                    if let Some((k, v)) = entry.split_once('=') {
                        println!("{:<32} {}", k, v);
                    } else {
                        println!("{:<32}", entry);
                    }
                }
            }
        }
    }
    Ok(())
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

fn docker_host_for_docker_cli() -> Option<String> {
    if let Ok(v) = std::env::var("DOCKER_HOST") {
        return Some(v);
    }
    #[cfg(unix)]
    {
        detect_docker_socket().map(|sock| format!("unix://{}", sock))
    }
    #[cfg(not(unix))]
    {
        None
    }
}

fn run_docker_cli(args: &[&str]) -> Result<String, String> {
    let mut cmd = std::process::Command::new("docker");
    cmd.args(args);
    if let Some(host) = docker_host_for_docker_cli() {
        cmd.env("DOCKER_HOST", host);
    }

    let out = cmd
        .output()
        .map_err(|e| format!("Failed to run docker: {}", e))?;
    if !out.status.success() {
        return Err(format!(
            "docker {} failed (exit {}): {}",
            args.join(" "),
            out.status.code().unwrap_or(-1),
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

async fn handle_volume(cmd: VolumeCommands) -> Result<(), String> {
    let docker = connect_docker()?;
    match cmd {
        VolumeCommands::List => {
            let opts = ListVolumesOptions::<String> {
                ..Default::default()
            };
            let resp = docker
                .list_volumes(Some(opts))
                .await
                .map_err(|e| e.to_string())?;
            let volumes = resp.volumes.unwrap_or_default();
            println!("{:<32} {:<12} MOUNTPOINT", "VOLUME NAME", "DRIVER");
            for v in volumes {
                println!("{:<32} {:<12} {}", v.name, v.driver, v.mountpoint);
            }
        }
        VolumeCommands::Create { name, driver } => {
            let opts = CreateVolumeOptions {
                name: name.as_str(),
                driver: driver.as_str(),
                ..Default::default()
            };
            let v = docker
                .create_volume(opts)
                .await
                .map_err(|e| e.to_string())?;
            println!("Created volume '{}'", v.name);
        }
        VolumeCommands::Inspect { name } => {
            let v = docker
                .inspect_volume(&name)
                .await
                .map_err(|e| e.to_string())?;
            let json = serde_json::to_string_pretty(&v).map_err(|e| e.to_string())?;
            println!("{}", json);
        }
        VolumeCommands::Remove { name } => {
            docker
                .remove_volume(&name, None)
                .await
                .map_err(|e| e.to_string())?;
            println!("Removed volume '{}'", name);
        }
    }
    Ok(())
}

async fn handle_k3s(cmd: K3sCommands) -> Result<(), String> {
    match cmd {
        K3sCommands::Status => {
            let status =
                cargobay_core::k3s::K3sManager::cluster_status().map_err(|e| e.to_string())?;
            println!("K3s Status");
            println!(
                "  Installed: {}",
                if status.installed { "yes" } else { "no" }
            );
            println!("  Running:   {}", if status.running { "yes" } else { "no" });
            if !status.version.is_empty() {
                println!("  Version:   {}", status.version);
            }
            if status.running {
                println!("  Nodes:     {}", status.node_count);
            }
            println!(
                "  Kubeconfig: {}",
                cargobay_core::k3s::K3sManager::kubeconfig_path().display()
            );
        }
        K3sCommands::Install => {
            println!("Downloading K3s...");
            cargobay_core::k3s::K3sManager::install(None)
                .await
                .map_err(|e| e.to_string())?;
            println!("K3s installed successfully.");
        }
        K3sCommands::Start => {
            let config = cargobay_core::k3s::K3sConfig::default();
            cargobay_core::k3s::K3sManager::start_cluster(&config).map_err(|e| e.to_string())?;
            println!("K3s cluster started.");
        }
        K3sCommands::Stop => {
            cargobay_core::k3s::K3sManager::stop_cluster().map_err(|e| e.to_string())?;
            println!("K3s cluster stopped.");
        }
        K3sCommands::Uninstall => {
            cargobay_core::k3s::K3sManager::uninstall().map_err(|e| e.to_string())?;
            println!("K3s uninstalled.");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsString;
    use std::path::Path;
    use std::sync::{Mutex, OnceLock};
    use std::time::Duration;

    static ENV_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

    struct EnvVarGuard {
        key: &'static str,
        prev: Option<OsString>,
    }

    impl EnvVarGuard {
        fn set_path(key: &'static str, value: &Path) -> Self {
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
            let nanos = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
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

    #[test]
    fn daemon_path_prefers_cargobay_daemon_path_env() {
        let _env_guard = ENV_LOCK
            .get_or_init(|| Mutex::new(()))
            .lock()
            .expect("env lock");

        let temp = TempDirGuard::new("cargobay-cli-test");
        let fake = temp.path.join("fake-daemon");
        std::fs::write(&fake, b"").expect("write");

        let _daemon_path = EnvVarGuard::set_path("CARGOBAY_DAEMON_PATH", &fake);
        let resolved = daemon_path();
        assert_eq!(resolved, fake);
    }

    #[test]
    #[cfg(unix)]
    fn spawn_daemon_detached_runs_executable_from_env() {
        use std::os::unix::fs::PermissionsExt;

        let _env_guard = ENV_LOCK
            .get_or_init(|| Mutex::new(()))
            .lock()
            .expect("env lock");

        let temp = TempDirGuard::new("cargobay-cli-test");
        let marker = temp.path.join("started");
        let script = temp.path.join("fake-daemon");

        let script_body = format!(
            "#!/bin/sh\nset -eu\nprintf 'ok\\n' > '{}'\n",
            marker.display()
        );
        std::fs::write(&script, script_body).expect("write script");
        let mut perms = std::fs::metadata(&script).expect("meta").permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&script, perms).expect("chmod");

        let _daemon_path = EnvVarGuard::set_path("CARGOBAY_DAEMON_PATH", &script);
        let _pid = spawn_daemon_detached().expect("spawn");

        let deadline = std::time::Instant::now() + Duration::from_secs(2);
        while std::time::Instant::now() < deadline {
            if marker.exists() {
                return;
            }
            std::thread::sleep(Duration::from_millis(20));
        }

        panic!("expected marker file to be created");
    }
}
