# CLAUDE.md — Project Guide for Claude Code

## Project Overview

CrateBay is a cross-platform container & VM management tool (like Docker Desktop / OrbStack alternative), built with Rust.

## Repository Structure

```
CrateBay/
├── crates/
│   ├── cratebay-core/     # Core library: Hypervisor trait, Docker (bollard), VM backends
│   ├── cratebay-cli/      # CLI binary (clap)
│   ├── cratebay-daemon/   # gRPC daemon (tonic/tokio) for VM operations
│   ├── cratebay-gui/      # Tauri v2 + React/TypeScript desktop app
│   └── cratebay-vz/       # macOS Virtualization.framework Swift FFI bridge
├── proto/                  # Protobuf definitions for gRPC
├── scripts/                # Dev scripts (setup-dev.sh, bench-perf.sh)
├── assets/                 # Logo, architecture diagram (SVG)
├── docs/                   # Tutorial, roadmap, changelog
└── .github/workflows/      # CI + release pipelines
```

## Build & Test

```bash
cargo check --workspace           # Quick compile check
cargo test --workspace            # Run all tests
cargo build --release             # Release build
cargo bench -p cratebay-core      # Criterion benchmarks
./scripts/bench-perf.sh           # Performance validation (<20MB, <3s, <200MB RAM)
```

**Note:** `cratebay-vz` only compiles on macOS (Virtualization.framework). CI skips it on Linux/Windows.

## Architecture

- **Hypervisor trait** in `cratebay-core/src/lib.rs` — abstraction for VM backends
- **macOS backend** (`macos.rs`) — Virtualization.framework via cratebay-vz Swift bridge
- **Linux backend** (`linux.rs`) — KVM via rust-vmm/kvm-ioctls
- **Windows backend** (`windows.rs`) — Hyper-V via PowerShell cmdlets
- **Docker** — Direct socket connection via bollard (no daemon needed)
- **Kubernetes** — kubectl JSON output parsing, K3s on-demand management

## Conventions

- Commit messages: **Conventional Commits** format (`feat:`, `fix:`, `docs:`, etc.)
- Max commit subject: 72 characters
- Mutex usage: Use `lock_or_recover()` (defined in `cratebay-core/src/lib.rs`), never `.lock().unwrap()`
- Platform-specific code gated with `#[cfg(target_os = "...")]`
- Tests: `cargo test --workspace` must pass before commit (enforced by pre-commit hook)

## Git Hooks

Hooks are in `.githooks/`. New devs run `scripts/setup-dev.sh` to activate them.

- `pre-commit`: runs `cargo check --workspace --locked`
- `commit-msg`: validates Conventional Commits format

## CI/CD

- **ci.yml**: check + test + clippy + fmt + size-check + perf-bench (macOS + Linux + Windows)
- **release.yml**: triggered by `v*` tags, builds CLI/daemon/GUI for macOS/Linux/Windows, creates GitHub Release
- **pages.yml**: deploys `website/` to GitHub Pages on push (paths: `website/**`) or manual dispatch

## Website Sync (IMPORTANT)

Official website: https://coder-hhx.github.io/CrateBay/ (auto-deployed from `website/` directory)

**Every time you add features, fix bugs, update docs, or change user-facing behavior, you MUST also update the website content to reflect those changes:**

- `website/index.html` — feature list, comparison tables, performance stats, version numbers
- `website/script.js` — interactive demos, feature highlights
- `website/style.css` — styles for any new sections

The pre-commit hook will remind you if source code changed but `website/` was not updated.

## Performance Claims (README)

These are validated by `scripts/bench-perf.sh` and CI `perf-bench` job:
- Binary size: <20MB
- Startup time: <3s
- Idle RAM: <200MB
