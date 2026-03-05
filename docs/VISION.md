# CrateBay Vision — Self‑hosted AI Infrastructure Desktop

CrateBay already ships as a **containers / Linux VMs / Kubernetes GUI** with a lightweight Rust + Tauri stack. The most natural next step is to extend that foundation into a **self‑hosted AI infrastructure desktop**:

- **Agent Sandboxes**: one‑click, isolated execution environments for AI agents (local-first, self-hosted).
- **Local Model Runtime**: manage on-device model runtimes (start/stop, model lifecycle, GPU visibility).
- **MCP Server Manager**: make MCP server setup and multi-server operations boring and safe.

This document is a product + architecture proposal (not a promise of shipped features).

---

## 1) Why this direction fits CrateBay

CrateBay’s current strengths map directly to “AI infra” primitives:

- **Lifecycle orchestration**: we already manage Docker + VMs + K3s.
- **Local-first UX**: desktop GUI with fast feedback loops.
- **Safety rails**: existing AI settings, audit ideas, and MCP policy scaffolding.

AI agent tooling is rapidly moving toward **isolated execution** (for safety, reproducibility, and cost control) and **standardized tool interfaces** (MCP). A self-hosted GUI that makes these pieces approachable is a clear wedge.

---

## 2) The three pillars (and what “MVP” means)

### Pillar A — Agent Sandboxes (the “Portainer for AI” wedge)

**Goal:** a user can create / start / stop / inspect an “AI Agent Sandbox” in seconds, with predictable isolation and resource limits.

**MVP scope (desktop-first):**

- One-click sandbox creation from templates (Node/Python/Rust “dev” images).
- Resource limits (CPU / RAM / disk), TTL / auto cleanup.
- Volume mounts (workspace in, artifacts out).
- Network policy presets (offline / allowlist / full).
- Unified logs + status + audit trail.

**Backend strategy:** make sandbox execution pluggable:

- **Docker-only (fastest path)**: containers with hardened defaults.
- **OpenSandbox-compatible runtime (recommended)**: treat OpenSandbox as an optional local service we can manage and talk to via API.
- Future: VM-based sandboxes for “max isolation”.

### Pillar B — Local Model Runtime (Ollama-first)

**Goal:** CrateBay becomes the place users go to manage local model runtimes without guesswork.

**MVP scope:**

- Detect existing local runtime (Ollama first).
- Model list / pull / delete, storage location and disk usage.
- “Active model set” presets for switching workflows.
- GPU visibility and memory snapshot (best-effort by platform).

**Non-goals for MVP:** full GPU scheduler across multiple apps; deep driver setup automation.

### Pillar C — MCP Server Manager (reduce JSON hand-edit pain)

**Goal:** manage many MCP servers the same way we manage containers today: discoverable, reproducible, and safe.

**MVP scope:**

- GUI registry of MCP servers (name, command/container, env, secrets refs).
- One-click start/stop + health + logs.
- Export client configs (Claude Code / Cursor / etc.) from a single source of truth.
- Policies: per-server permissions, network mode, workspace mount rules.

---

## 3) Architecture proposal (minimal disruption)

### Keep “CrateBay Core” stable

Do not turn the existing container/VM product into a science project. AI should be an **additive layer**:

- Keep the current pages (Dashboard / Containers / Images / Volumes / VMs / Kubernetes).
- Add an **AI hub** that groups Models / Sandboxes / MCP / Assistant.

### Add “Managed Services” as a unifying concept

AI infra needs background services. Introduce a shared internal abstraction:

- `Service`: install / configure / start / stop / status / logs
- Examples: **Ollama**, **OpenSandbox server**, **MCP servers**

This also fits existing CrateBay patterns (K3s on-demand download/start/stop).

### Reuse existing AI scaffolding

CrateBay already has an AI settings surface and a skills registry scaffold. Extend (not replace) it:

- Add a new skill executor type like `sandbox_action` (planned) to run steps inside a sandbox.
- Route “assistant steps” through sandboxes by default when enabled.

---

## 4) Naming and positioning

### Keep “CrateBay” (recommended)

Pros:

- Already matches the foundation (containers/VMs) and Rust “crate” identity.
- Avoids expensive renaming churn (repo, domains, app IDs, release history).

What to change:

- Update the tagline: “containers + VMs + self-hosted AI infrastructure”.
- Introduce an “AI” section in navigation and documentation.

### Optional: sub-brand the AI layer

If you want clearer AI recall without renaming everything:

- “CrateBay AI” (feature set)
- “CrateBay Sandbox” (agent runtime)
- “CrateBay MCP Manager” (MCP tooling)

---

## 5) UI layout: evolve, don’t rewrite

Avoid a full redesign until the AI modules have shipped at least one iteration. Recommended layout changes:

- Add top-level **AI** section with 4 sub-pages:
  - **Models** (runtime + models)
  - **Sandboxes** (templates + running sandboxes)
  - **MCP** (servers + export config)
  - **Assistant** (natural language → plan → sandbox execution)
- Update Dashboard widgets to include AI runtime status (running sandboxes, active model runtime, MCP servers).

---

## 6) Proposed roadmap (post‑GA)

- **v1.0 GA**: finalize cross-platform installers + docs + website consistency.
- **v1.2 “AI Infra MVP”**:
  - Managed Services (Ollama/OpenSandbox/MCP as services)
  - Ollama integration (detect + list/pull/delete)
  - MCP Server Manager MVP (start/stop/logs + config export)
- **v1.3 “Agent Sandbox v1”**:
  - Sandbox templates + lifecycle + audit
  - OpenSandbox integration path (optional runtime)
  - Assistant → sandbox execution (opt-in)
- **v1.4 “GPU + Scale”**:
  - GPU observability improvements, multi-runtime support
  - Remote hosts / multi-machine sandbox backends (stretch)

---

## 7) Tooling bootstrap

Add a repo script that can **check** and optionally **install** prerequisites:

- Docker (and `docker compose`)
- Ollama
- Optional: OpenSandbox server (docker compose)

See `scripts/setup-ai.sh`.

---

## 8) Current implementation snapshot (as of 2026-03-06)

Shipped in current preview:

- Top-level **AI Hub** page with `Overview / Models / Sandboxes / MCP / Assistant`.
- **Ollama phase 1** integration: runtime status probe + local model list in GUI.
- **Agent Sandboxes MVP**: template-based sandbox lifecycle (`create/start/stop/delete/inspect`) with resource limits, TTL metadata, and local audit log.
- Local bootstrap assets: `scripts/setup-ai.sh` and `tools/opensandbox/`.

Next execution focus (short horizon):

- MCP Manager MVP: server registry + start/stop/logs + client config export.
- Ollama phase 2: model pull/delete actions and richer storage controls.
- Sandbox TTL auto-cleanup + Assistant/Skills sandbox executor integration.

Release posture:

- CrateBay remains in **pre-v1 development**.
- `v1.0.0` will be announced only after Models/Sandboxes/MCP scope and release validation are fully complete.

---

## References (source reading)

- OpenSandbox: `https://github.com/alibaba/OpenSandbox`
- OpenSandbox docs: `https://docs.open-sandbox.ai/`
- MCP: `https://modelcontextprotocol.io/`
