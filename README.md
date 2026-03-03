<p align="center">
  <img src="https://raw.githubusercontent.com/coder-hhx/CrateBay/master/assets/logo.png" alt="CrateBay" width="128" />
</p>

<h1 align="center">CrateBay</h1>

<p align="center">
  <strong>Free, open-source desktop for containers and Linux VMs.</strong><br>
  Lightweight Linux VMs, Docker containers, and Kubernetes — all in one app.
</p>

<p align="center">
  <strong>English</strong> ·
  <a href="README.zh.md">中文</a>
</p>

<p align="center">
  <a href="https://github.com/coder-hhx/CrateBay/releases">Download</a> ·
  <a href="https://github.com/coder-hhx/CrateBay/issues">Issues</a> ·
  <a href="docs/ARCHITECTURE.md">Architecture</a> ·
  <a href="docs/TUTORIAL.md">Tutorial</a> ·
  <a href="CHANGELOG.md">Changelog</a>
</p>

<p align="center">
  <img src="https://img.shields.io/badge/license-Apache%202.0-blue" />
  <img src="https://img.shields.io/badge/rust-1.75+-orange" />
  <img src="https://img.shields.io/badge/platform-macOS%20|%20Linux%20|%20Windows-lightgrey" />
</p>

---

## Why CrateBay?

OrbStack is great, but it's **closed-source and macOS-only**. Docker Desktop is **heavy and requires paid subscriptions**. Podman Desktop and Rancher Desktop use **Electron (300-500MB RAM)**. We believe developers deserve something better:

- **Name meaning**: *CrateBay* = `cargo` (containers, and a wink to Rust `cargo`) + `bay` (a home port for your VMs and dev environments)
- **100% free & open source** — Apache 2.0, no license fees, no telemetry
- **Rust + Tauri native** — not Electron, idles at <200MB RAM
- **VM + Containers unified** — one tool for everything
- **Cross-platform** — macOS, Linux, and Windows

## Comparison

| | CrateBay | OrbStack | Docker Desktop | Podman Desktop | Colima |
|---|:---:|:---:|:---:|:---:|:---:|
| **Open source** | ✅ | ❌ | Partial | ✅ | ✅ |
| **Free for commercial use** | ✅ | ❌ | ❌ (>250 employees) | ✅ | ✅ |
| **GUI** | Tauri (native) | Swift (native) | Electron | Electron | None |
| **Idle RAM** | <200 MB | <1 GB | 3-6 GB | 300-500 MB | ~400 MB |
| **macOS** | ✅ | ✅ | ✅ | ✅ | ✅ |
| **Windows** | ✅ | ❌ | ✅ | ✅ | ❌ |
| **Linux** | ✅ | ❌ | ✅ | ✅ | ✅ |
| **Docker management** | ✅ | ✅ | ✅ | ✅ | ✅ |
| **Linux VMs** | ✅ | ✅ | ❌ | ❌ | Indirect |
| **Kubernetes** | ✅ (K3s + Dashboard) | ✅ | ✅ | ✅ | ✅ (K3s) |
| **Auto port forwarding** | ✅ | ✅ | ✅ | ❌ | ✅ |
| **VirtioFS file sharing** | ✅ | ✅ | ✅ | ❌ | ✅ |
| **Tech stack** | Rust | Swift | Go + Electron | Electron + TS | Go |

## Features

| Feature | macOS | Linux | Windows | Status |
|---------|-------|-------|---------|--------|
| Docker container management | ✅ | ✅ | ✅ | Working |
| Container log streaming | ✅ | ✅ | ✅ | Working |
| Container exec / terminal | ✅ | ✅ | ✅ | Working |
| Container env variable viewer | ✅ | ✅ | ✅ | Working |
| Dashboard & GUI | ✅ | ✅ | ✅ | Working |
| Image search (Docker Hub / Quay) | ✅ | ✅ | ✅ | Working |
| Local image management (list/remove/tag/inspect) | ✅ | ✅ | ✅ | Working |
| Import / Push images (docker load/push) | ✅ | ✅ | ✅ | Working |
| Package image from container (docker commit) | ✅ | ✅ | ✅ | Working |
| Docker volume management | ✅ | ✅ | ✅ | Working |
| Lightweight Linux VMs | ✅ Virtualization.framework | ✅ KVM | ✅ Hyper-V | Working |
| VM console (serial output) | ✅ | ✅ | ✅ | Working |
| VM port forwarding | ✅ | ✅ | ✅ | Working |
| VM resource monitoring | ✅ | ✅ | ✅ | Working |
| OS image download & management | ✅ | ✅ | ✅ | Working |
| Rosetta x86_64 translation | ✅ Apple Silicon | — | — | Working |
| VirtioFS file sharing | ✅ | ✅ virtiofsd | ✅ Plan 9/SMB | Working |
| K3s cluster management | ✅ | ✅ | 📋 | Working (Linux) |
| Kubernetes dashboard (pods/services/deployments) | ✅ | ✅ | ✅ | Working |
| Auto-update checker | ✅ | ✅ | ✅ | Working |
| CLI (VM + Docker + K3s + Mount) | ✅ | ✅ | ✅ | Working |
| Dark/Light theme + i18n | ✅ | ✅ | ✅ | Working |

## Platform Compatibility

- **macOS**: Supports Apple Silicon (M-series) and Intel (x86_64). Rosetta x86_64 is available only on Apple Silicon with macOS 13+.
- **Windows**: Targets Windows 10 and Windows 11. VM backend relies on Hyper-V (typically Pro/Enterprise/Education + Hyper-V enabled).
- **Linux**: VM backend relies on KVM (`/dev/kvm` required).

## Tech Stack

- **Core**: Rust (cross-platform workspace)
- **GUI**: Tauri v2 + React (TypeScript)
- **VM Engine**: Virtualization.framework (macOS) / KVM (Linux) / Hyper-V (Windows)
- **File Sharing**: VirtioFS (macOS/Linux) / Plan 9 (Windows)
- **x86 Emulation**: Rosetta 2 (macOS Apple Silicon)
- **Containers**: Docker API via Bollard (direct socket connection)
- **Kubernetes**: K3s (on-demand download) + kubectl
- **CLI**: Rust (clap)
- **IPC**: gRPC (tonic + prost) — VM operations only; containers use direct Docker socket

## Quick Start

> CrateBay v1.0.0 is now available. Feedback and contributions are welcome!

```bash
# Build from source
git clone https://github.com/coder-hhx/CrateBay.git
cd CrateBay
cargo build --release

# CLI usage
cratebay status                              # Show platform info
cratebay image search nginx --source all --limit 20
cratebay image load ./image.tar
cratebay image push ghcr.io/owner/image:tag
cratebay docker run nginx:latest --name web --cpus 2 --memory 512 --pull
cratebay image pack-container web myorg/web:snapshot
cratebay docker login-cmd web
cratebay docker ps                           # List containers
cratebay vm create myvm --cpus 4 --memory 4096 --rosetta  # Create VM with Rosetta
cratebay mount add --vm myvm --tag code --host-path ~/code --guest-path /mnt/code
```

See [Tutorial](docs/TUTORIAL.md) for detailed instructions.

## Architecture

See [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) for the full system design.

<p align="center">
  <img src="assets/architecture.svg" alt="CrateBay Architecture" width="900" />
</p>

**Key**: Containers talk directly to Docker (lowest latency). VMs go through the CrateBay daemon (needs privileged lifecycle management). K8s queries use kubectl directly.

## Contributing

We welcome contributions! Please open an issue or submit a pull request.

## License

Apache License 2.0 — free for personal and commercial use.
