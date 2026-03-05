# CrateBay Usage Tutorial

> **English** · [中文](TUTORIAL.zh.md)
>
> CrateBay is a free, open-source desktop app for Docker containers and lightweight Linux VMs, with a native GUI (Tauri + React) and a Rust-powered CLI.

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
| **Node.js** | 22+ (22/24 LTS recommended) | GUI frontend (React + Vite) |
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
brew install node@22

# or via nvm
nvm install 24
nvm use 24
```

### Docker Runtime

CrateBay works with any Docker-compatible runtime:

- **Colima** (recommended, free) — `brew install colima && colima start`
- **Docker Desktop** — the standard Docker experience
- **OrbStack** — CrateBay auto-detects its socket too

---

## 2. Installation

### Build from Source

```bash
git clone https://github.com/coder-hhx/CrateBay.git
cd CrateBay

# Install frontend dependencies
cd crates/cratebay-gui && npm install && cd ../..

# Build everything
cargo build --release
```

### Run the GUI (Development)

```bash
cd crates/cratebay-gui
npm run tauri dev
```

Hot-reload enabled: `.tsx` changes reload instantly, Rust changes trigger recompile.

### Production Build

```bash
cd crates/cratebay-gui
npm run tauri build
```

Output: `crates/cratebay-gui/src-tauri/target/release/bundle/`

- macOS: `.dmg` and `.app`
- Windows: `.msi` and `.exe`
- Linux: `.deb`, `.rpm`, `.AppImage`

### CLI Only

```bash
cargo build --release --bin cratebay
# Binary at: target/release/cratebay
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

> Note: K3s is Linux-only. On macOS/Windows, K3s will run inside a CrateBay Linux VM in a future release.

### Volumes

Docker volume management:

- **List volumes** — See all Docker volumes with driver and mountpoint
- **Create volume** — Create a new named volume
- **Inspect volume** — View volume details (labels, options, scope)
- **Delete volume** — Remove unused volumes

### Virtual Machines (虚拟机)

Available in the current v1.0.0 preview (GA coming soon):

- **Create / start / stop / delete / list** with full lifecycle management
- **CPU / Memory / Disk** parameters on creation
- **ACPI graceful shutdown** (3-phase: requestStop → poll → force stop)
- **Rosetta toggle** (macOS Apple Silicon only; availability depends on macOS 13+)
- **VirtioFS file sharing** (real mount with tag validation and guest hints)
- **Port forwarding** (TCP proxy for VM services)
- **Resource monitoring** (CPU / memory / disk / network stats)
- **OS image download** (Alpine, Ubuntu — auto-download kernel/initrd)
- **Login command**: generates an `ssh user@host -p <port>` string

> Note: VM metadata is persisted to `vms.json` under the Config directory. The VM runtime backend uses Virtualization.framework (macOS), KVM (Linux), or Hyper-V (Windows).

### Images (镜像)

Also available in the current v1.0.0 preview:

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
cratebay status
```

Output:
```
CrateBay v1.0.0
Platform: macOS aarch64 (Virtualization.framework available)
Rosetta x86_64: available
Docker: connected (~/.colima/default/docker.sock)
```

### Docker Commands

```bash
# List all containers
cratebay docker ps

# Run a new container (optional CPU/memory limits, optional pull)
cratebay docker run nginx:latest --name web --cpus 2 --memory 512 --pull

# Start a container
cratebay docker start <container_id>

# Stop a container
cratebay docker stop <container_id>

# Remove a container (force)
cratebay docker rm <container_id>

# Print a shell login command for a container
cratebay docker login-cmd web
```

### VM Commands

> Optional: run the daemon for VM management:
>
> ```bash
> cargo run -p cratebay-daemon
> ```
>
> The CLI/GUI will use the daemon automatically when it's reachable (via `CRATEBAY_GRPC_ADDR`) and fall back to local mode if not.
>
> macOS VZ PoC: set `CRATEBAY_VZ_KERNEL` (and optionally `CRATEBAY_VZ_INITRD`) before starting a VM.

```bash
# Create a VM
cratebay vm create myvm --cpus 4 --memory 4096 --disk 20

# Create with Rosetta x86 translation (Apple Silicon)
cratebay vm create myvm --cpus 4 --memory 4096 --rosetta

# Start / Stop / Delete
cratebay vm start myvm
cratebay vm stop myvm
cratebay vm delete myvm

# List all VMs
cratebay vm list

# Print an SSH login command (requires an SSH endpoint)
cratebay vm login-cmd myvm --user root --host 127.0.0.1 --port 2222
```

### Image Commands

```bash
# Search images (Docker Hub / Quay)
cratebay image search nginx --source all --limit 20

# List tags for an OCI registry reference (works for ghcr.io/quay.io/private registries)
cratebay image tags ghcr.io/owner/image --limit 50

# Import an image archive (.tar)
cratebay image load ./image.tar

# Push an image to a registry
cratebay image push ghcr.io/owner/image:tag

# Package an image from an existing container
cratebay image pack-container web myorg/web:snapshot
```

### File Sharing (VirtioFS)

```bash
# Mount a host directory into a VM
cratebay mount add \
  --vm myvm \
  --tag code \
  --host-path ~/code \
  --guest-path /mnt/code

# Mount as read-only
cratebay mount add \
  --vm myvm \
  --tag data \
  --host-path ~/data \
  --guest-path /mnt/data \
  --readonly

# List mounts
cratebay mount list --vm myvm

# Remove a mount
cratebay mount remove --vm myvm --tag code
```

### Volume Commands

```bash
# List all Docker volumes
cratebay volume list

# Create a volume
cratebay volume create mydata

# Inspect a volume
cratebay volume inspect mydata

# Remove a volume
cratebay volume remove mydata
```

### K3s Commands

```bash
# Check K3s cluster status
cratebay k3s status

# Install K3s (Linux only; downloads from GitHub releases)
cratebay k3s install

# Start the K3s cluster
cratebay k3s start

# Stop the K3s cluster
cratebay k3s stop

# Uninstall K3s (removes binary and data)
cratebay k3s uninstall
```

> Note: K3s is Linux-only. On macOS/Windows, K3s will run inside a CrateBay Linux VM in a future release.

### Shell Completions

Generate shell completion scripts for your preferred shell:

```bash
# Bash
cratebay completions bash >> ~/.bashrc

# Zsh
cratebay completions zsh >> ~/.zshrc

# Fish
cratebay completions fish > ~/.config/fish/completions/cratebay.fish
```

After adding completions, restart your shell or source the config file for changes to take effect.

---

## 5. Docker Socket Detection

CrateBay auto-detects Docker sockets in this order:

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
cratebay docker ps
```

---

## 6. Configuration

### Environment Variables

| Variable | Description |
|----------|-------------|
| `DOCKER_HOST` | Override Docker socket path |
| `RUST_LOG` | Set log level (`info`, `debug`, `trace`) |
| `CRATEBAY_GRPC_ADDR` | Daemon gRPC address (default: `127.0.0.1:50051`) |
| `CRATEBAY_DAEMON_PATH` | Override daemon executable path (GUI auto-start) |
| `CRATEBAY_CONFIG_DIR` | Override config directory (stores `vms.json`) |
| `CRATEBAY_DATA_DIR` | Override data directory |
| `CRATEBAY_LOG_DIR` | Override log directory |
| `CRATEBAY_LOG_RETENTION_DAYS` | Keep error logs for N days (default: 7) |
| `CRATEBAY_VZ_RUNNER_PATH` | Override `cratebay-vz` path (macOS VZ PoC) |
| `CRATEBAY_VZ_KERNEL` | Linux kernel path (macOS VZ PoC) |
| `CRATEBAY_VZ_INITRD` | Linux initrd path (optional, macOS VZ PoC) |
| `CRATEBAY_VZ_CMDLINE` | Linux kernel cmdline (default: `console=hvc0`, macOS VZ PoC) |

### Data Locations

| Platform | Config | Data | Logs |
|----------|--------|------|------|
| macOS | `~/Library/Application Support/com.cratebay.app/` | Same | Same |
| Linux | `~/.config/cratebay/` | `~/.local/share/cratebay/` | Same |
| Windows | `%APPDATA%\cratebay\` | Same | Same |

VM metadata is stored at `<config>/vms.json`.

Error logs are written to the Logs directory as `cratebay-error.log.YYYY-MM-DD` and automatically cleaned up (keeps the most recent 7 days by default).

---

## 7. Roadmap

| Version | Focus | Key Features |
|---------|-------|-------------|
| **v0.1** | Foundation | Docker management, GUI, CLI, i18n |
| **v0.2** | VM & Networking | VM lifecycle, VirtioFS, port forwarding, resource monitoring |
| **v0.3** | Developer Experience | Container logs/terminal, image management, volumes, env vars |
| **v0.4** | Kubernetes | K3s integration, K8s dashboard, auto-update |
| **v1.0.0-rc** (GA coming soon) | GA Readiness | Real VM execution, cross-platform, testing, security audit |

---

## License

Apache License 2.0 — free for personal and commercial use.
