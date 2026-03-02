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

## v1.0.0 — Production Ready (In Progress)

- ✅ ACPI graceful shutdown (VZ bridge 3-phase: requestStop → poll → force stop)
- ✅ CI/CD pipeline (GitHub Actions: CI + release builds)
- ✅ Comprehensive test suite (121 tests across core, CLI, daemon, integration)
- ✅ Shell completion (bash, zsh, fish, elvish, powershell)
- ⬜ Real VM execution end-to-end (kernel image download → boot → console)
- ⬜ Real VirtioFS mount implementation (guest-side mount)
- ⬜ Linux (KVM) VM backend
- ⬜ Windows (Hyper-V) VM backend
- ⬜ Plugin system
- ⬜ Security audit & hardening
- ⬜ Performance optimization (<20MB install, <200MB idle RAM, <3s startup)
