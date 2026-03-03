# CrateBay Technical Architecture

## Technology Choices

| Layer | Technology | Rationale |
|-------|-----------|-----------|
| GUI | Tauri v2 + React (TypeScript) | Cross-platform, lightweight, the most popular desktop framework on GitHub |
| CLI | Rust (clap) | Same language as the backend; single-binary distribution |
| Daemon | Rust (tokio) | High-performance async runtime for managing VM and K3s lifecycles |
| IPC | gRPC (tonic) | High performance, type-safe, Rust-native (used for VM operations only) |
| Container Engine | Docker API (bollard) | GUI/CLI connect directly to the Docker socket — no extra middleware, lowest latency |
| Virtualization (macOS) | Rust FFI → Swift → Virtualization.framework | Native macOS performance |
| Virtualization (Linux) | Rust → KVM (rust-vmm) | Kernel-level virtualization |
| File Sharing | virtiofs | Near-native filesystem performance |
| K8s | k3s (on-demand download) + kubectl | Lightweight, low resource footprint |
| K8s Queries | kubectl JSON output | GUI/CLI parse kubectl output directly — no additional dependencies |

## System Architecture Diagram

<p align="center">
  <img src="../assets/architecture.svg" alt="CrateBay Architecture" width="900" />
</p>

## Data Flow

### Container / Image / Volume Operations
```
GUI/CLI → bollard (Rust Docker client) → Docker socket → Docker daemon
```
- **Does not go through the CrateBay Daemon**
- Connects directly to the Docker socket for minimal latency (~1-5 ms)
- Docker itself is already a daemon — there is no need for an extra intermediary
- Automatically detects the Docker socket path (Colima / OrbStack / Docker Desktop)

### VM Operations
```
GUI/CLI → gRPC → CrateBay Daemon → Hypervisor trait → Platform VM backend
```
- **Must go through the Daemon** (privileged operations and complex lifecycle management required)
- The Daemon manages VM creation / start / stop / delete / console / port forwarding / VirtioFS
- The Hypervisor trait provides a unified interface with independent implementations per platform

### K3s Cluster Management
```
GUI/CLI → cratebay-core::k3s::K3sManager → K3s binary (downloaded on demand)
```
- K3s is downloaded on demand from GitHub Releases
- Runs natively on Linux; future macOS/Windows support will run K3s inside a VM

### Kubernetes Dashboard Queries
```
GUI/CLI → kubectl --kubeconfig → K8s API Server
```
- Calls kubectl directly and parses its JSON output
- Covers pods / services / deployments / namespaces
- Read-only queries, stateless

## Crate Structure

```
CrateBay/
├── crates/
│   ├── cratebay-core/     # Core library: Hypervisor trait, K3s manager,
│   │                      # store, images, port forwarding
│   ├── cratebay-cli/      # CLI: direct Docker access (bollard) + gRPC → Daemon (VM)
│   ├── cratebay-daemon/   # Daemon: VM service only (gRPC VMService)
│   ├── cratebay-gui/      # GUI: Tauri v2 backend + React frontend
│   │   ├── src/           #   React frontend (TS)
│   │   └── src-tauri/     #   Tauri backend (Rust)
│   └── cratebay-vz/       # macOS Virtualization.framework FFI (Swift bridge)
├── proto/                 # gRPC definitions (VMService only, 14 RPCs)
└── website/               # Official website (GitHub Pages)
```

## Key Design Decisions

1. **Hybrid IPC model** — Containers connect directly to the Docker socket (performance first); VMs go through the gRPC Daemon (requires privileges and lifecycle management). No unnecessary middleware for Docker.
2. **Full-stack Rust** — GUI backend, Daemon, CLI, and VM engine all share one language, reducing overall stack complexity.
3. **Hypervisor trait abstraction** — A unified VM interface with per-platform implementations. Adding a new platform only requires a new backend.
4. **Tauri v2 for the GUI** — Uses 60-90% less memory than Electron while costing roughly one-third the development effort of a fully native GUI.
5. **On-demand K3s download** — K3s is not bundled in the installer, keeping the distribution size small.
6. **Automatic Docker socket detection** — Supports Colima / OrbStack / Docker Desktop / native Docker with no manual configuration.

## Proto Definitions (VM Only)

`proto/cratebay.proto` defines the `VMService` with 14 RPCs:

| RPC | Purpose |
|-----|---------|
| CreateVm / StartVm / StopVm / DeleteVm | VM lifecycle |
| ListVMs / GetVmStatus | VM queries |
| MountVirtioFs / UnmountVirtioFs / ListVirtioFsMounts | VirtioFS sharing |
| AddPortForward / RemovePortForward / ListPortForwards | Port forwarding |
| GetVmConsole | Serial console |
| GetVmStats | Resource monitoring |

Container, image, volume, K3s, and K8s operations do not use gRPC and have no corresponding proto definitions.
