# CrateBay Roadmap

> **English** · [中文](../README.zh.md)
>
> Vision: [AI Infra Proposal](VISION.md) · [中文](VISION.zh.md)

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

## v1.0.0 — GA Readiness (In Progress, Coming Soon)

- ⚠️ Core feature scope is mostly complete, but GA release is not announced yet.

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
- ✅ AI release-readiness gate script (core scenarios, UI smoke, wording guard)
- ✅ AI skills scaffold (registry model + settings toggle + OpenClaw preset binding)
- ✅ Settings UX split (General vs AI tabs) and assistant AI icon refresh
- ⬜ Final cross-platform release validation (macOS/Linux/Windows installers)
- ⬜ Final onboarding and upgrade path verification
- ⬜ Final docs + website consistency pass before GA announcement

## v1.1.0 — Ecosystem & Polish (Planned)

- ⬜ Plugin system enhancements (marketplace, versioning, sandboxed execution)
- ⬜ Expanded OS image catalog (Fedora, Arch Linux, NixOS, FreeBSD)
- ⬜ Container compose / multi-container orchestration (docker-compose support)
- ⬜ Network management UI (custom bridges, DNS, firewall rules)
- ⬜ Snapshot & restore for VMs
- ⬜ Remote Docker host connections
- ⬜ GPU passthrough (macOS Metal, Linux VFIO)
- ⬜ Telemetry-free analytics dashboard (local-only usage stats)

## v1.2.0 — AI Infrastructure MVP (In Progress)

- ✅ AI hub navigation scaffold: `Overview / Models / Sandboxes / MCP / Assistant`
- ✅ Ollama phase 1: runtime status check + local model listing in GUI
- ✅ AI bootstrap tooling: `scripts/setup-ai.sh` + OpenSandbox local config scaffold
- ⬜ Managed Services abstraction: install/configure/start/stop/status/logs
- ⬜ Ollama phase 2: pull/delete models and richer storage management
- ⬜ MCP Server Manager MVP (registry, start/stop/logs, export client config)

## v1.3.0 — Agent Sandbox v1 (Planned)

- ⬜ Sandbox templates (dev env images) + lifecycle UI (create/start/stop/inspect)
- ⬜ OpenSandbox integration path (optional local runtime backend)
- ⬜ Network policy presets, TTL cleanup, secrets refs, workspace mounts
- ⬜ Assistant/Skills: sandbox executor (opt-in) + audit events

## v1.4.0 — Local Models & GPU Observability (Planned)

- ⬜ Model runtime profiles (model sets, per-workflow presets)
- ⬜ GPU visibility (best-effort per OS) and basic VRAM usage snapshots
- ⬜ Multi-runtime support (Ollama first; extend later)
