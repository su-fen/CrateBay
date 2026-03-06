# CrateBay Pre-v1 Release Smoke Checklist

> Status as of 2026-03-06: `coming soon` / preview line `v0.7.0`.
> Use this checklist to advance from `v0.x.0` previews toward `v1.0.0` GA.

## 1. AI Core Scenario Gate

- Source of truth: `CrateBay-internal/notes/2026-03-05_ai_core_scenarios_draft.md`
- Automated gate:
  - `cargo test -p cratebay-gui ai_tests::assistant_core_scenarios_success_rate`
- Acceptance:
  - Success rate `>= 95%`
  - High-risk assistant / MCP actions require explicit confirmation

## 2. Cross-platform Installer Smoke

### macOS

- Build artifacts:
  - `cargo build --release -p cratebay-cli -p cratebay-daemon`
  - `cd crates/cratebay-gui && npm ci && npm run build && npm run tauri build`
- Clean-machine validation:
  - Fresh install works
  - App starts and tray icon is visible
  - Settings `General` / `AI` tabs switch correctly
  - Containers page, VMs page, K8s page, and AI Hub load without fatal errors
  - Uninstall + reinstall works

### Linux

- Build artifacts:
  - `cargo build --release -p cratebay-cli -p cratebay-daemon`
  - `cd crates/cratebay-gui && npm ci && npm run build && npm run tauri build`
- Clean-machine validation:
  - Fresh install works
  - App starts, no missing runtime dependency popup
  - Settings `General` / `AI` tabs switch correctly
  - Container, K8s, and AI read actions succeed
  - Uninstall + reinstall works

### Windows

- Build artifacts:
  - `bash scripts/build-release-windows.sh`
- Clean-machine validation:
  - MSI / NSIS installer works
  - App starts and can open Settings / Assistant / AI Hub pages
  - Settings `General` / `AI` tabs switch correctly
  - VM backend reports status in Hyper-V environments
  - Uninstall + reinstall works

## 3. Upgrade Path Validation

- Upgrade matrix:
  - `v0.4.x -> v0.7.0`
  - `v0.7.x -> v0.8.x`
  - `v0.8.x -> v0.9.x`
  - `v0.9.x -> v1.0.0`
- Verify:
  - Existing config is preserved
  - Existing VM / container metadata is readable
  - AI settings migrate without data loss
  - No plaintext API keys appear in config / log / crash artifacts

## 4. AI Hub Completion Gate

- **Models**
  - Ollama pull / delete actions work
  - Local model storage state is visible and understandable
- **Sandboxes**
  - TTL cleanup works
  - Assistant / Skills sandbox executor works when enabled
  - Optional OpenSandbox path can be configured and verified
- **MCP**
  - Registry CRUD works
  - Start / stop / status / logs work
  - Client config export works for supported presets
- **Assistant**
  - AI Hub workflows can bridge into Models / Sandboxes / MCP surfaces cleanly

## 5. Final Documentation & Website Guard

- Wording guard:
  - Keep external wording as `coming soon` / `即将发布`
  - Avoid GA-style wording before `v1.0.0`
- Required checks:
  - `./scripts/release-readiness.sh`
  - `npm run check:i18n`
  - Manual website spot-check
