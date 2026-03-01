# Changelog

> **English** · [中文](CHANGELOG.zh.md)

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- UI Design System specification (`docs/DESIGN_SYSTEM.md`) — unified design tokens, button/input/modal specs.
- Containers page: "Run Container" button with run modal for directly creating containers by image name.
- Images import modal: native file picker dialog for selecting `.tar` archives (via `tauri-plugin-dialog`).
- Navigation reordered: Images now appears before VMs.
- VM page temporarily sealed as "Coming Soon" — code preserved but not active.
- Roadmap document (`docs/ROADMAP.md`) — future version plans.
- CLI: image search (`cargobay image search`) and tag listing (`cargobay image tags`).
- CLI: image import/push (`cargobay image load` / `cargobay image push`) and container snapshot packaging (`cargobay image pack-container`).
- CLI: run containers with optional CPU/memory limits (`cargobay docker run --cpus/--memory`) and optional pull (`--pull`).
- CLI: print container login command (`cargobay docker login-cmd`).
- GUI: Images page (search Docker Hub/Quay, list tags for registry references, run container with CPU/memory, import/push images).
- GUI: VM page (preview VM lifecycle UI, VirtioFS mount tracking, login command generator).
- GUI: show container login commands and package a container as an image (docker commit).

### Fixed

- CI: fix Clippy/rustfmt failures (VZ explicit auto-deref, async env lock in tests, Docker port type formatting).
- GUI: group containers by name prefix (collapsible), and make `tauri dev` resilient to `localhost` DNS issues / double logger initialization.

### Changed

- GUI: unified button heights (32px default, 28px small), input heights (32px), icon stroke-width (2).
- GUI: Images toolbar simplified — removed limit input and Clear button.
- GUI: `.btn.small` → `.btn.sm`, `.btn.tiny` → `.btn.xs`, removed `.input.small`.
- GUI: refactor monolithic App.tsx (1164 lines) into 17 modular files — types, icons, 5 custom hooks, 3 shared components, 5 page components.
- GUI: redesign Settings page with responsive width, section icons, and custom toggle switches.
- GUI: redesign error displays — structured ErrorBanner with icon/title/action, ErrorInline with dismiss button.
- GUI: improve panel components with icon titles, hover effects, and visual hierarchy.
- GUI: fix Images page search results table overflow with flexible `minmax()` columns.
- GUI: optimize VMs page information architecture — VM list moved above create form.
- GUI: replace all hardcoded theme colors with CSS custom properties (`--purple-hover`, `--red-dim`).

## [0.1.0] - 2026-02-28

### Added

- Tauri + Rust + React GUI application for container management.
- Docker container lifecycle management (list, start, stop, remove).
- Auto-detection of Docker socket paths (Colima, OrbStack, default `/var/run/docker.sock`).
- CLI tool with VM commands (`list`, `start`, `stop`, `status`) and Docker commands (`ps`, `start`, `stop`, `rm`).
- Dark and Light theme support with CSS custom properties.
- Multi-language support (English, 中文).
- Responsive layout with sidebar collapse on small windows.
- Custom CargoBay logo and branding.
- VM engine abstraction with `Hypervisor` trait (macOS Virtualization.framework, Linux KVM).
- gRPC service definitions for VM management.
- Daemon scaffolding for background services.
- Rust workspace with 4 crates: `cargobay-core`, `cargobay-cli`, `cargobay-daemon`, `cargobay-gui`.
- Cross-platform design with conditional compilation (`#[cfg(target_os)]`).
- Bollard crate for Docker API communication.

[0.1.0]: https://github.com/coder-hhx/CargoBay/releases/tag/v0.1.0
