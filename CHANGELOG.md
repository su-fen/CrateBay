# Changelog

> **English** · [中文](CHANGELOG.zh.md)

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [1.0.0] - 2026-03-02

### Added

#### VM Lifecycle & Platform Backends
- Virtualization.framework FFI (Swift bridge) for real VM start/stop on macOS.
- Linux KVM VM backend (kvm-ioctls, vCPU, memory, kernel loading, serial console).
- Windows Hyper-V VM backend scaffolding.
- ACPI graceful shutdown (VZ bridge 3-phase: requestStop -> poll -> force stop).
- Real VM execution end-to-end (OS image download -> kernel/initrd -> boot).
- VM console (serial output) for all platforms.
- VM port forwarding (TCP proxy).
- VM/container resource monitoring (CPU / memory / disk / network).
- OS image catalog and download management.

#### VirtioFS & File Sharing
- Real VirtioFS mount implementation (tag validation, mount tracking, guest hints).
- VirtioFS mount management (UI + daemon).

#### Kubernetes
- K3s integration (on-demand download, install, start, stop, uninstall).
- Kubernetes dashboard (pods, services, deployments, namespace selector, pod logs).

#### Container & Image Management
- Container log streaming (real-time follow).
- Container exec / terminal integration.
- Container environment variable viewer.
- Local image management (list, remove, tag, inspect).
- Docker volume management (list, create, inspect, remove).
- CLI: image search (`cratebay image search`) and tag listing (`cratebay image tags`).
- CLI: image import/push (`cratebay image load` / `cratebay image push`) and container snapshot packaging (`cratebay image pack-container`).
- CLI: run containers with optional CPU/memory limits (`cratebay docker run --cpus/--memory`) and optional pull (`--pull`).
- CLI: print container login command (`cratebay docker login-cmd`).
- GUI: Images page (search Docker Hub/Quay, list tags for registry references, run container with CPU/memory, import/push images).
- GUI: VM page (VM lifecycle UI, VirtioFS mount tracking, login command generator).
- GUI: show container login commands and package a container as an image (docker commit).
- Images import modal: native file picker dialog for selecting `.tar` archives (via `tauri-plugin-dialog`).

#### GUI Enhancements
- UI Design System specification (`docs/DESIGN_SYSTEM.md`) — unified design tokens, button/input/modal specs.
- Containers page: "Run Container" button with run modal for directly creating containers by image name.
- Navigation reordered: Images now appears before VMs.
- GUI: redesign Settings page with responsive width, section icons, and custom toggle switches.
- GUI: redesign error displays — structured ErrorBanner with icon/title/action, ErrorInline with dismiss button.
- GUI: improve panel components with icon titles, hover effects, and visual hierarchy.
- GUI: replace all hardcoded theme colors with CSS custom properties (`--purple-hover`, `--red-dim`).
- Auto-update checker (GitHub releases).

#### Developer Experience & Infrastructure
- Shell completion (bash, zsh, fish, elvish, powershell).
- Roadmap document (`docs/ROADMAP.md`).
- CI/CD pipeline (GitHub Actions: CI + release builds).
- Comprehensive test suite (177+ tests across core, CLI, daemon, integration).
- Security audit & hardening (input validation, path traversal prevention, log sanitization).

### Fixed

- CI: fix Clippy/rustfmt failures (VZ explicit auto-deref, async env lock in tests, Docker port type formatting).
- GUI: group containers by name prefix (collapsible), and make `tauri dev` resilient to `localhost` DNS issues / double logger initialization.
- GUI: fix Images page search results table overflow with flexible `minmax()` columns.

### Changed

- GUI: unified button heights (32px default, 28px small), input heights (32px), icon stroke-width (2).
- GUI: Images toolbar simplified — removed limit input and Clear button.
- GUI: `.btn.small` -> `.btn.sm`, `.btn.tiny` -> `.btn.xs`, removed `.input.small`.
- GUI: refactor monolithic App.tsx (1164 lines) into 17 modular files — types, icons, 5 custom hooks, 3 shared components, 5 page components.
- GUI: optimize VMs page information architecture — VM list moved above create form.

## [0.1.0] - 2026-02-28

### Added

- Tauri + Rust + React GUI application for container management.
- Docker container lifecycle management (list, start, stop, remove).
- Auto-detection of Docker socket paths (Colima, OrbStack, default `/var/run/docker.sock`).
- CLI tool with VM commands (`list`, `start`, `stop`, `status`) and Docker commands (`ps`, `start`, `stop`, `rm`).
- Dark and Light theme support with CSS custom properties.
- Multi-language support (English, 中文).
- Responsive layout with sidebar collapse on small windows.
- Custom CrateBay logo and branding.
- VM engine abstraction with `Hypervisor` trait (macOS Virtualization.framework, Linux KVM).
- gRPC service definitions for VM management.
- Daemon scaffolding for background services.
- Rust workspace with 4 crates: `cratebay-core`, `cratebay-cli`, `cratebay-daemon`, `cratebay-gui`.
- Cross-platform design with conditional compilation (`#[cfg(target_os)]`).
- Bollard crate for Docker API communication.

[Unreleased]: https://github.com/coder-hhx/CrateBay/compare/v1.0.0...HEAD
[1.0.0]: https://github.com/coder-hhx/CrateBay/compare/v0.1.0...v1.0.0
[0.1.0]: https://github.com/coder-hhx/CrateBay/releases/tag/v0.1.0
