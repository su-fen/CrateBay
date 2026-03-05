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

## Current Stage — Pre-v1 Development (In Progress)

- ⚠️ CrateBay is still in development preview and **not GA yet**.
- ⚠️ `v1.0.0` will be declared only after all pre-v1 milestones are complete and validated on macOS/Linux/Windows.

## v0.5.0 — Core Hardening (Done)

- ✅ ACPI graceful shutdown (VZ bridge 3-phase: requestStop → poll → force stop)
- ✅ CI/CD pipeline (GitHub Actions: CI + release builds)
- ✅ Comprehensive test suite (177 tests across core, CLI, daemon, integration)
- ✅ Shell completion (bash, zsh, fish, elvish, powershell)
- ✅ Real VM execution end-to-end (OS image download → kernel/initrd → boot)
- ✅ Real VirtioFS mount implementation (tag validation, mount tracking, guest hints)
- ✅ Linux (KVM) VM backend (kvm-ioctls, vCPU, memory, kernel loading, serial console)
- ✅ Security audit & hardening (input validation, path traversal prevention, log sanitization)
- ✅ Windows (Hyper-V) VM backend
- ✅ Plugin system base
- ✅ Performance optimization (<20MB install, <200MB idle RAM, <3s startup)

## v0.6.0 — AI Infra Foundation (Done)

- ✅ AI hub navigation scaffold: `Overview / Models / Sandboxes / MCP / Assistant`
- ✅ Ollama phase 1: runtime status check + local model listing in GUI
- ✅ AI bootstrap tooling: `scripts/setup-ai.sh` + OpenSandbox local config scaffold
- ✅ AI release-readiness gate script (core scenarios, UI smoke, wording guard)
- ✅ AI skills scaffold (registry model + settings toggle + OpenClaw preset binding)
- ✅ Settings UX split (General vs AI tabs) and assistant AI icon refresh

## v0.7.0 — Agent Sandboxes MVP (In Progress)

- ✅ Sandbox templates (Node/Python/Rust dev images)
- ✅ Sandbox lifecycle UI + backend (`create/start/stop/delete/inspect`)
- ✅ Resource limit + TTL metadata + local JSONL audit trail
- ⬜ TTL auto-cleanup worker (periodic reclaim)
- ⬜ Assistant/Skills sandbox executor (opt-in)
- ⬜ OpenSandbox runtime adapter (optional backend path)

## v0.8.0 — MCP Manager MVP (Planned)

- ⬜ MCP server registry (name, command/container, env, secret refs)
- ⬜ Start/stop/status/logs for multiple MCP servers
- ⬜ Export client configuration (Codex/Claude/Cursor presets)
- ⬜ Policy/audit integration with existing AI security settings

## v0.9.0 — Release Candidate & GA Gate (Planned)

- ⬜ Final cross-platform release validation (macOS/Linux/Windows installers)
- ⬜ Final onboarding and upgrade path verification
- ⬜ Final docs + website consistency pass before GA announcement
- ⬜ Run full `docs/RELEASE_SMOKE_CHECKLIST.md` and sign-off

## v1.0.0 — Official GA (Target)

- ⬜ Complete all `v0.7.0`–`v0.9.0` milestones
- ⬜ Freeze release notes/changelog and publish signed binaries
- ⬜ Announce GA only after release smoke checks pass on all target platforms

## Post-v1 — Ecosystem & Polish (Planned)

- ⬜ Plugin system enhancements (marketplace, versioning, sandboxed execution)
- ⬜ Expanded OS image catalog (Fedora, Arch Linux, NixOS, FreeBSD)
- ⬜ Container compose / multi-container orchestration (docker-compose support)
- ⬜ Network management UI (custom bridges, DNS, firewall rules)
- ⬜ Snapshot & restore for VMs
- ⬜ Remote Docker host connections
- ⬜ GPU passthrough (macOS Metal, Linux VFIO)
- ⬜ Telemetry-free analytics dashboard (local-only usage stats)
- ⬜ Model runtime profiles + richer multi-runtime support
