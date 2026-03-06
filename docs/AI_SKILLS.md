# AI Skills Scaffold (Preview)

> This document describes the current public skills scaffold only. Detailed product planning is maintained privately.

## Goal

Provide a stable skill registry model that can be shared by:

- Assistant step execution (`assistant_step`)
- MCP action execution (`mcp_action`)
- Agent/CLI bridge presets (`agent_cli_preset`)

This is a **foundation layer** for later runtime orchestration, marketplace sync, and permission hardening.

## Data Model

`AiSettings` now includes a `skills` array. Each item has:

- `id`: stable unique skill id
- `display_name`: user-facing name
- `description`: short behavior summary
- `tags`: category hints for filtering/routing
- `executor`: adapter type (`assistant_step`, `mcp_action`, `agent_cli_preset`)
- `target`: command/action/preset target
- `input_schema`: JSON schema-like object for future validation
- `enabled`: toggle flag

## Current Defaults

- `assistant-container-diagnose`
- `mcp-k8s-pods-read`
- `agent-cli-openclaw-plan`

## Current UI Surface

Settings > AI Settings now shows a **Skills Registry (Preview)** block:

- Display skill metadata
- Enable/disable each skill
- Persist toggles with `save_ai_settings`

Runtime orchestration (chain execution, retries, guardrails, dependency graph) is not shipped yet.

Settings UX is now separated into two tabs:

- `General`: theme, language, updates
- `AI`: provider profiles, MCP policy, skills registry, Agent/CLI bridge

## Next Implementation Steps

1. Add executor adapters for direct OpenClaw Gateway tools (in addition to CLI preset mode).
2. Add schema validation before execution (`input_schema` strict mode).
3. Add audit events per skill execution (`skill_id`, `executor`, `target`, `request_id`).
4. Add import/export format for user skill packs.
5. Add a sandbox executor (`sandbox_action`) so skills can run inside isolated Agent Sandboxes.
