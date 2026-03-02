# CargoBay Roadmap

> **English** · [中文](../README.zh.md)

## v0.1.0 — Foundation (Done)

- ✅ Docker container management (list, start, stop, remove, run)
- ✅ Image search (Docker Hub, Quay), tag listing, import/push
- ✅ Package container as image (docker commit)
- ✅ GUI Dashboard with container overview
- ✅ CLI with Docker/VM/Image/Mount subcommands
- ✅ Dark/Light theme + i18n (EN/ZH)
- ✅ Docker socket auto-detection (Colima, OrbStack, Docker Desktop)

## v0.2.0 — VM & Networking (Done)

- ✅ Virtualization.framework FFI (Swift bridge) for real VM start/stop
- ✅ OS image download and management
- ✅ VM console (serial output)
- ✅ VM port forwarding (TCP proxy)
- ✅ VirtioFS mount management (UI + daemon)
- ✅ VM/container resource monitoring (CPU / memory / disk / network)

## v0.3.0 — Developer Experience (Done)

- ✅ Container log streaming (real-time follow)
- ✅ Container exec / terminal integration
- ✅ Container environment variable viewer
- ✅ Local image management (list, remove, tag, inspect)
- ✅ Docker volume management (list, create, inspect, remove)

## v0.4.0 — Kubernetes (Done)

- ✅ K3s integration (on-demand download, install, start, stop, uninstall)
- ✅ Kubernetes dashboard (pods, services, deployments, namespace selector, pod logs)
- ✅ Auto-update checker (GitHub releases)
- ✅ Official website (GitHub Pages)

## v1.0.0 — Production Ready (Done)

- ✅ ACPI graceful shutdown (VZ bridge 3-phase: requestStop → poll → force stop)
- ✅ CI/CD pipeline (GitHub Actions: CI + release builds)
- ✅ Comprehensive test suite (177 tests across core, CLI, daemon, integration)
- ✅ Shell completion (bash, zsh, fish, elvish, powershell)
- ✅ Real VM execution end-to-end (OS image download → kernel/initrd → boot)
- ✅ Real VirtioFS mount implementation (tag validation, mount tracking, guest hints)
- ✅ Linux (KVM) VM backend (kvm-ioctls, vCPU, memory, kernel loading, serial console)
- ✅ Security audit & hardening (input validation, path traversal prevention, log sanitization)
- ✅ Windows (Hyper-V) VM backend
- ✅ Plugin system
- ✅ Performance optimization (<20MB install, <200MB idle RAM, <3s startup)

## v1.1.0 — Ecosystem & Polish (Planned)

- ⬜ Plugin system enhancements (marketplace, versioning, sandboxed execution)
- ⬜ Expanded OS image catalog (Fedora, Arch Linux, NixOS, FreeBSD)
- ⬜ Container compose / multi-container orchestration (docker-compose support)
- ⬜ Network management UI (custom bridges, DNS, firewall rules)
- ⬜ Snapshot & restore for VMs
- ⬜ Remote Docker host connections
- ⬜ GPU passthrough (macOS Metal, Linux VFIO)
- ⬜ Telemetry-free analytics dashboard (local-only usage stats)
