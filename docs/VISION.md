# CrateBay AI Hub Vision

> Product direction note for the public repo.
> Current public preview line: `v0.7.0`.

## 1. Positioning

CrateBay is building an open-source desktop infrastructure app for:

- Docker containers
- Linux VMs
- Local Kubernetes
- An upcoming AI Hub that connects models, sandboxes, MCP, and assistant workflows

The AI Hub is not a separate product. It is the next layer on top of CrateBay's existing container / VM / K8s foundation.

## 2. Why the AI Hub matters

The current codebase already has strong infrastructure primitives:

- native desktop UX via Tauri
- cross-platform VM backends
- Docker API integration
- local K3s management
- policy, audit, and secure settings primitives

The AI Hub turns those primitives into a higher-level operator workflow:

- **Models** for local and remote model runtimes
- **Sandboxes** for isolated execution environments
- **MCP** for managed tool connectivity
- **Assistant** for plan / execute / explain flows over existing product capabilities

## 3. Current snapshot (`v0.7.0` preview)

Shipped in the current preview:

- Top-level **AI Hub** with `Overview / Models / Sandboxes / MCP / Assistant`
- **Assistant** plan generation and step execution with policy checks and audit logs
- **AI settings** for provider profiles, secret references, MCP allowlists, and CLI presets
- **Ollama phase 1** integration: runtime status and local model listing
- **Sandboxes MVP**: template-based lifecycle, resource limits, TTL metadata, and local audit log
- Local bootstrap assets: `scripts/setup-ai.sh` and `tools/opensandbox/`

## 4. What is still missing before `v1.0.0`

The public release line stays in `v0.x.0` until all of the following are complete and validated:

- **Models**: pull / delete actions and clearer local storage controls
- **Sandboxes**: TTL auto-cleanup, assistant/skills executor, optional OpenSandbox backend path
- **MCP**: real server registry, lifecycle management, logs, and client config export
- **Assistant**: full integration across the AI Hub surfaces instead of partial wiring only
- **Release gates**: cross-platform installer smoke, upgrade path validation, privacy audit, final doc/site consistency

## 5. Versioning policy

- `v0.7.0` = current public preview line
- `v0.8.0` = AI Hub completion work
- `v0.9.0` = pre-v1 hardening and validation
- `v1.0.0` = reserved for the first GA-ready build only

In short: CrateBay does **not** use `v1.0.0` as a preview label.

## 6. Roadmap (pre-v1 → GA)

- **v0.7.0 — AI Hub Preview**
  - Current preview with assistant, provider settings, Ollama phase 1, sandbox MVP
- **v0.8.0 — AI Hub Completion**
  - Models phase 2
  - Sandboxes completion
  - MCP Manager MVP
- **v0.9.0 — Pre-v1 Hardening**
  - Smoke tests, upgrade validation, privacy audit, wording consistency
- **v1.0.0 — Official GA**
  - Only after AI Hub scope is complete and all release gates pass

## 7. Tooling bootstrap

CrateBay already includes local bootstrap assets for the preview line:

- `scripts/setup-ai.sh` for prerequisite checks and optional best-effort installs
- `tools/opensandbox/` for local optional sandbox runtime scaffolding

## 8. Public messaging

Until `v1.0.0` is truly ready:

- keep the README and website in `Coming Soon` posture
- use `preview` language for `v0.x.0`
- avoid implying GA or full product completion
