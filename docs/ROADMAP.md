# CrateBay Roadmap

> **English** · [中文](../README.zh.md)
>
> Vision: [AI Hub Vision](VISION.md) · [中文](VISION.zh.md)

## Current Stage — Pre-v1 Development

- Current public preview line: `v0.7.0`
- The public README and website remain `Coming Soon`
- `v1.0.0` is reserved for the point when the full AI Hub scope (`Models / Sandboxes / MCP / Assistant`) is complete and all pre-v1 release gates pass

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
- ✅ Official website foundation (GitHub Pages)

## v0.5.0 — Core Hardening (Done)

- ✅ ACPI graceful shutdown (VZ bridge 3-phase: requestStop → poll → force stop)
- ✅ CI/CD pipeline (GitHub Actions: CI + release builds)
- ✅ Comprehensive test suite (Rust + frontend + integration)
- ✅ Shell completion (bash, zsh, fish, elvish, powershell)
- ✅ Real VM execution end-to-end (OS image download → kernel/initrd → boot)
- ✅ Real VirtioFS mount implementation (tag validation, mount tracking, guest hints)
- ✅ Linux (KVM) VM backend
- ✅ Windows (Hyper-V) VM backend
- ✅ Security audit & hardening (input validation, path traversal prevention, log sanitization)
- ✅ Performance optimization (<20MB install, <200MB idle RAM, <3s startup)

## v0.6.0 — AI Foundation (Done)

- ✅ AI Hub navigation scaffold: `Overview / Models / Sandboxes / MCP / Assistant`
- ✅ Settings split into `General` / `AI`
- ✅ Multi-provider AI settings and secret storage
- ✅ Skills scaffold + Agent/CLI preset foundation
- ✅ Assistant plan → execute → explain flow with audit trail
- ✅ Docker runtime quick-repair path for container actions

## v0.7.0 — AI Hub Preview (Current)

- ✅ Ollama phase 1: runtime status + local model listing in GUI
- ✅ Sandboxes MVP: template-based `create/start/stop/delete/inspect`
- ✅ Sandbox resource limits + TTL metadata + local JSONL audit trail
- ✅ AI bootstrap tooling: `scripts/setup-ai.sh`
- ✅ OpenSandbox local scaffold: `tools/opensandbox/`
- ✅ Local macOS app install flow for preview builds
- ✅ Public docs + website reset to preview / `Coming Soon` posture

## v0.8.0 — AI Hub Completion (In Progress)

- ⬜ Models: Ollama pull/delete actions and richer local storage management
- ⬜ Sandboxes: TTL auto-cleanup worker (periodic reclaim)
- ⬜ Sandboxes: Assistant / Skills sandbox executor (opt-in)
- ⬜ Sandboxes: OpenSandbox runtime adapter (optional backend path)
- ⬜ MCP Manager: server registry (name, command/container, env, secrets)
- ⬜ MCP Manager: start/stop/status/logs for multiple MCP servers
- ⬜ MCP Manager: export client configuration (Codex / Claude / Cursor presets)
- ⬜ Assistant: deeper AI Hub integration across Models / Sandboxes / MCP surfaces

## v0.9.0 — Pre-v1 Hardening & Validation (Planned)

- ⬜ 100% explicit confirmation coverage for high-risk assistant / MCP actions
- ⬜ Secret / privacy audit pass (no plaintext leaks in logs, config, crash artifacts)
- ⬜ Cross-platform installer smoke validation (macOS / Linux / Windows)
- ⬜ Upgrade path validation across pre-v1 versions
- ⬜ Final docs + website consistency pass before GA
- ⬜ Run full `docs/RELEASE_SMOKE_CHECKLIST.md` and sign off

## v1.0.0 — Official GA (Target)

- ⬜ Complete the full AI Hub scope (`Models / Sandboxes / MCP / Assistant`)
- ⬜ Pass release-readiness gate and installer smoke checks on all target platforms
- ⬜ Freeze release notes / changelog and publish signed binaries
- ⬜ Keep `v1.0.0` reserved until the above conditions are fully satisfied

## Post-v1 — Ecosystem & Polish (Planned)

- ⬜ Plugin system enhancements (marketplace, versioning, sandboxed execution)
- ⬜ Expanded OS image catalog (Fedora, Arch Linux, NixOS, FreeBSD)
- ⬜ Docker Compose / multi-container orchestration
- ⬜ Network management UI (bridge / DNS / firewall)
- ⬜ VM snapshots and restore
- ⬜ Remote Docker host connections
- ⬜ GPU passthrough / observability improvements
- ⬜ Local-only usage insights (no telemetry)
