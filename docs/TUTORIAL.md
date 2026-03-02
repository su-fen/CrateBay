# CargoBay Usage Tutorial

> **English** · [中文](TUTORIAL.zh.md)
>
> CargoBay is a free, open-source desktop app for Docker containers and lightweight Linux VMs, with a native GUI (Tauri + React) and a Rust-powered CLI.

---

## Table of Contents

1. [Prerequisites](#1-prerequisites)
2. [Installation](#2-installation)
3. [GUI Guide](#3-gui-guide)
4. [CLI Reference](#4-cli-reference)
5. [Docker Socket Detection](#5-docker-socket-detection)
6. [Configuration](#6-configuration)
7. [Roadmap](#7-roadmap)

---

## 1. Prerequisites

| Tool | Version | Purpose |
|------|---------|---------|
| **Rust** | 1.75+ | Core backend, CLI, Tauri backend |
| **Node.js** | 18+ | GUI frontend (React + Vite) |
| **npm** | 9+ | JavaScript dependencies |
| **Docker** | Any | Container engine |

### Platform Compatibility

- **macOS**: Supports Apple Silicon (M-series) and Intel (x86_64). Rosetta x86_64 is available only on Apple Silicon with macOS 13+.
- **Windows**: Targets Windows 10 and Windows 11. VM backend relies on Hyper-V (typically Pro/Enterprise/Education + Hyper-V enabled).
- **Linux**: VM backend relies on KVM (`/dev/kvm` required).

### Install Rust

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source ~/.cargo/env
```

### Install Node.js

```bash
# macOS
brew install node

# or via nvm
nvm install 18
```

### Docker Runtime

CargoBay works with any Docker-compatible runtime:

- **Colima** (recommended, free) — `brew install colima && colima start`
- **Docker Desktop** — the standard Docker experience
- **OrbStack** — CargoBay auto-detects its socket too

---

## 2. Installation

### Build from Source

```bash
git clone https://github.com/coder-hhx/CargoBay.git
cd CargoBay

# Install frontend dependencies
cd crates/cargobay-gui && npm install && cd ../..

# Build everything
cargo build --release
```

### Run the GUI (Development)

```bash
cd crates/cargobay-gui
npm run tauri dev
```

Hot-reload enabled: `.tsx` changes reload instantly, Rust changes trigger recompile.

### Production Build

```bash
cd crates/cargobay-gui
npm run tauri build
```

Output: `crates/cargobay-gui/src-tauri/target/release/bundle/`

- macOS: `.dmg` and `.app`
- Windows: `.msi` and `.exe`
- Linux: `.deb`, `.rpm`, `.AppImage`

### CLI Only

```bash
cargo build --release --bin cargobay
# Binary at: target/release/cargobay
```

---

## 3. GUI Guide

### Dashboard (仪表盘)

The default landing page. Shows a card-based overview:

| Card | Description |
|------|-------------|
| **Containers** | Total container count, click to jump to container management |
| **Virtual Machines** | VM count (preview) |
| **Images** | Image search result count (last search) |
| **System** | Docker connection status |

Running containers are previewed below the cards (up to 5).

### Containers (容器管理)

Full container management page:

- **Running containers** — shown with a green status dot and glow effect
- **Stopped containers** — shown with a grey dot

**Actions per container:**

| Action | Description |
|--------|-------------|
| **Start** | Start a stopped container |
| **Stop** | Gracefully stop (10s timeout) |
| **Delete** | Force stop + remove container |
| **Logs** | View container logs with follow/streaming mode |
| **Env** | View container environment variables |
| **Login command** | Show a `docker exec -it ...` command for the container |
| **Package as image** | Create a new image from the container (`docker commit`) |

The container list auto-refreshes every 3 seconds. Connection status is shown in the top-right pill.

### Kubernetes

The Kubernetes page provides:

- **K3s Cluster Management** — Install, start, stop, and uninstall K3s directly from the GUI
- **Cluster Status** — Version, node count, kubeconfig path
- **Pods Tab** — View all pods across namespaces, with status, ready count, restarts, and age
- **Services Tab** — View cluster services with type, cluster IP, and ports
- **Deployments Tab** — View deployments with replica status
- **Namespace Selector** — Filter by namespace or view all
- **Pod Logs** — Click to view logs for any pod

> Note: K3s is Linux-only. On macOS/Windows, K3s will run inside a CargoBay Linux VM in a future release.

### Volumes

Docker volume management:

- **List volumes** — See all Docker volumes with driver and mountpoint
- **Create volume** — Create a new named volume
- **Inspect volume** — View volume details (labels, options, scope)
- **Delete volume** — Remove unused volumes

### Virtual Machines (虚拟机)

Preview in v0.1:

- **Create / start / stop / delete / list** (preview)
- **CPU / Memory / Disk** parameters on creation
- **Rosetta toggle** (macOS Apple Silicon only; availability depends on macOS 13+)
- **VirtioFS mount list** (tracked in UI; real mounting will be implemented later)
- **Login command**: generates an `ssh user@host -p <port>` string (you provide the port)

> Note: VM metadata is persisted to `vms.json` under the Config directory. The VM runtime backend uses Virtualization.framework (macOS), KVM (Linux), or Hyper-V (Windows).

### Images (镜像)

Available in v0.1:

- **Search images** across **Docker Hub** and **Quay**
- **List tags** for registry-domain references (e.g. `quay.io/org/image`, `ghcr.io/org/image`)
- **Create containers** from an image with optional **CPU cores / memory (MB)** and optional **pull**
- **Import custom images** from a local `.tar` archive (`docker load -i`)
- **Push images** to a registry (`docker push`)

> Tip: For Docker Hub images, use `docker run`-style references (e.g. `nginx:latest`). Tag listing currently requires a registry domain reference.

### Settings (设置)

| Setting | Options |
|---------|---------|
| **Theme** | Dark (default) / Light |
| **Language** | English, 中文 |

Preferences are saved in `localStorage` and persist across sessions.

---

## 4. CLI Reference

### System Status

```bash
cargobay status
```

Output:
```
CargoBay v1.0.0
Platform: macOS aarch64 (Virtualization.framework available)
Rosetta x86_64: available
Docker: connected (~/.colima/default/docker.sock)
```

### Docker Commands

```bash
# List all containers
cargobay docker ps

# Run a new container (optional CPU/memory limits, optional pull)
cargobay docker run nginx:latest --name web --cpus 2 --memory 512 --pull

# Start a container
cargobay docker start <container_id>

# Stop a container
cargobay docker stop <container_id>

# Remove a container (force)
cargobay docker rm <container_id>

# Print a shell login command for a container
cargobay docker login-cmd web
```

### VM Commands

> Optional: run the daemon for VM management:
>
> ```bash
> cargo run -p cargobay-daemon
> ```
>
> The CLI/GUI will use the daemon automatically when it's reachable (via `CARGOBAY_GRPC_ADDR`) and fall back to local mode if not.
>
> macOS VZ PoC: set `CARGOBAY_VZ_KERNEL` (and optionally `CARGOBAY_VZ_INITRD`) before starting a VM.

```bash
# Create a VM
cargobay vm create myvm --cpus 4 --memory 4096 --disk 20

# Create with Rosetta x86 translation (Apple Silicon)
cargobay vm create myvm --cpus 4 --memory 4096 --rosetta

# Start / Stop / Delete
cargobay vm start myvm
cargobay vm stop myvm
cargobay vm delete myvm

# List all VMs
cargobay vm list

# Print an SSH login command (requires an SSH endpoint)
cargobay vm login-cmd myvm --user root --host 127.0.0.1 --port 2222
```

### Image Commands

```bash
# Search images (Docker Hub / Quay)
cargobay image search nginx --source all --limit 20

# List tags for an OCI registry reference (works for ghcr.io/quay.io/private registries)
cargobay image tags ghcr.io/owner/image --limit 50

# Import an image archive (.tar)
cargobay image load ./image.tar

# Push an image to a registry
cargobay image push ghcr.io/owner/image:tag

# Package an image from an existing container
cargobay image pack-container web myorg/web:snapshot
```

### File Sharing (VirtioFS)

```bash
# Mount a host directory into a VM
cargobay mount add \
  --vm myvm \
  --tag code \
  --host-path ~/code \
  --guest-path /mnt/code

# Mount as read-only
cargobay mount add \
  --vm myvm \
  --tag data \
  --host-path ~/data \
  --guest-path /mnt/data \
  --readonly

# List mounts
cargobay mount list --vm myvm

# Remove a mount
cargobay mount remove --vm myvm --tag code
```

### Volume Commands

```bash
# List all Docker volumes
cargobay volume list

# Create a volume
cargobay volume create mydata

# Inspect a volume
cargobay volume inspect mydata

# Remove a volume
cargobay volume remove mydata
```

### K3s Commands

```bash
# Check K3s cluster status
cargobay k3s status

# Install K3s (Linux only; downloads from GitHub releases)
cargobay k3s install

# Start the K3s cluster
cargobay k3s start

# Stop the K3s cluster
cargobay k3s stop

# Uninstall K3s (removes binary and data)
cargobay k3s uninstall
```

> Note: K3s is Linux-only. On macOS/Windows, K3s will run inside a CargoBay Linux VM in a future release.

### Shell Completions

Generate shell completion scripts for your preferred shell:

```bash
# Bash
cargobay completions bash >> ~/.bashrc

# Zsh
cargobay completions zsh >> ~/.zshrc

# Fish
cargobay completions fish > ~/.config/fish/completions/cargobay.fish
```

After adding completions, restart your shell or source the config file for changes to take effect.

---

## 5. Docker Socket Detection

CargoBay auto-detects Docker sockets in this order:

| Priority | Path | Runtime |
|----------|------|---------|
| 1 | `~/.colima/default/docker.sock` | Colima |
| 2 | `~/.orbstack/run/docker.sock` | OrbStack |
| 3 | `/var/run/docker.sock` | Docker Desktop / native |
| 4 | `~/.docker/run/docker.sock` | Docker Desktop (alt) |

**Windows:** Also checks `//./pipe/docker_engine` and `//./pipe/dockerDesktopLinuxEngine`.

### Override

```bash
export DOCKER_HOST=unix:///path/to/custom/docker.sock
cargobay docker ps
```

---

## 6. Configuration

### Environment Variables

| Variable | Description |
|----------|-------------|
| `DOCKER_HOST` | Override Docker socket path |
| `RUST_LOG` | Set log level (`info`, `debug`, `trace`) |
| `CARGOBAY_GRPC_ADDR` | Daemon gRPC address (default: `127.0.0.1:50051`) |
| `CARGOBAY_DAEMON_PATH` | Override daemon executable path (GUI auto-start) |
| `CARGOBAY_CONFIG_DIR` | Override config directory (stores `vms.json`) |
| `CARGOBAY_DATA_DIR` | Override data directory |
| `CARGOBAY_LOG_DIR` | Override log directory |
| `CARGOBAY_LOG_RETENTION_DAYS` | Keep error logs for N days (default: 7) |
| `CARGOBAY_VZ_RUNNER_PATH` | Override `cargobay-vz` path (macOS VZ PoC) |
| `CARGOBAY_VZ_KERNEL` | Linux kernel path (macOS VZ PoC) |
| `CARGOBAY_VZ_INITRD` | Linux initrd path (optional, macOS VZ PoC) |
| `CARGOBAY_VZ_CMDLINE` | Linux kernel cmdline (default: `console=hvc0`, macOS VZ PoC) |

### Data Locations

| Platform | Config | Data | Logs |
|----------|--------|------|------|
| macOS | `~/Library/Application Support/com.cargobay.app/` | Same | Same |
| Linux | `~/.config/cargobay/` | `~/.local/share/cargobay/` | Same |
| Windows | `%APPDATA%\cargobay\` | Same | Same |

VM metadata is stored at `<config>/vms.json`.

Error logs are written to the Logs directory as `cargobay-error.log.YYYY-MM-DD` and automatically cleaned up (keeps the most recent 7 days by default).

---

## 7. Roadmap

| Version | Focus | Key Features |
|---------|-------|-------------|
| **v0.1** | Foundation | Docker management, GUI, CLI, i18n |
| **v0.2** | VM & Networking | VM lifecycle, VirtioFS, port forwarding, resource monitoring |
| **v0.3** | Developer Experience | Container logs/terminal, image management, volumes, env vars |
| **v0.4** | Kubernetes | K3s integration, K8s dashboard, auto-update |
| **v1.0** (current) | Production Ready | Real VM execution, cross-platform, testing, security audit |

---

## License

Apache License 2.0 — free for personal and commercial use.
