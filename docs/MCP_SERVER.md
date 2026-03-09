# CrateBay MCP Server (External)

CrateBay ships an external MCP server binary (`cratebay-mcp`) that exposes **CrateBay-managed sandboxes** as MCP tools over **stdio**.

This lets MCP-capable AI clients (Claude Desktop / Cursor / etc.) create a sandbox, sync files, and execute commands inside it.

## Run

```bash
# From source
cargo run -p cratebay-mcp
```

## Install (Recommended)

Most MCP clients expect a stable executable path (not `cargo run`).

```bash
# Build a local release binary
cargo build -p cratebay-mcp --release

# Binary path:
#   target/release/cratebay-mcp
```

Or install into your Cargo bin dir:

```bash
cargo install --path crates/cratebay-mcp --locked
```

## Workspace + Safety

File sync and host workspace mounts are intentionally constrained.

Set a workspace root directory (required for file operations):

```bash
export CRATEBAY_MCP_WORKSPACE_ROOT="/absolute/path/to/your/workspace"
```

Then configure your MCP client to run `cratebay-mcp` with that env var.

**Confirmation:** write/destructive tools require `confirmed=true` to avoid silent changes.

## Tools

- `cratebay_sandbox_templates` — list built-in templates
- `cratebay_sandbox_create` — create + start a sandbox (optional `mounts`, requires `confirmed=true`)
- `cratebay_sandbox_list` / `cratebay_sandbox_inspect`
- `cratebay_sandbox_exec` — run a shell command (requires `confirmed=true`)
- `cratebay_sandbox_start` / `cratebay_sandbox_stop` / `cratebay_sandbox_delete` (requires `confirmed=true`)
- `cratebay_sandbox_cleanup_expired` (requires `confirmed=true`)
- `cratebay_sandbox_put_path` — `docker cp` local → sandbox (requires `confirmed=true` + workspace root)
- `cratebay_sandbox_get_path` — `docker cp` sandbox → local (requires `confirmed=true` + workspace root)

## Smoke Test

```bash
./scripts/cratebay-mcp-smoke.sh
```

## Client Config Notes

### Claude Desktop (macOS / Windows)

Edit Claude Desktop config and restart the app.

Config file locations:

- macOS: `~/Library/Application Support/Claude/claude_desktop_config.json`
- Windows: `%APPDATA%\\Claude\\claude_desktop_config.json`

Example:

```json
{
  "mcpServers": {
    "cratebay": {
      "command": "cratebay-mcp",
      "args": [],
      "env": {
        "CRATEBAY_MCP_WORKSPACE_ROOT": "/absolute/path/to/your/workspace"
      }
    }
  }
}
```

If `cratebay-mcp` is not on your `PATH`, set `command` to the full executable path
(for example `.../target/release/cratebay-mcp`).

### OpenAI Codex CLI / IDE extension

Add as a stdio MCP server:

```bash
codex mcp add cratebay --env CRATEBAY_MCP_WORKSPACE_ROOT=/absolute/path/to/your/workspace -- cratebay-mcp
```

Or via `~/.codex/config.toml`:

```toml
[mcp_servers.cratebay]
command = "cratebay-mcp"
args = []

[mcp_servers.cratebay.env]
CRATEBAY_MCP_WORKSPACE_ROOT = "/absolute/path/to/your/workspace"
```

### Cursor (optional)

Cursor reads MCP servers from `mcp.json` (global or project-scoped). The config
shape is similar to Claude Desktop: `command`, `args`, `env`.

### Tool confirmations

CrateBay intentionally requires `confirmed=true` for write/destructive tools. If
your AI client keeps “failing” tool calls, ask it to set `confirmed=true`.

In most MCP clients, the minimal config shape is:

- `command`: `cratebay-mcp`
- `env`: set `CRATEBAY_MCP_WORKSPACE_ROOT`
