# CrateBay MCP Server (External)

CrateBay ships an external MCP server binary (`cratebay-mcp`) that exposes **CrateBay-managed sandboxes** as MCP tools over **stdio**.

This lets MCP-capable AI clients (Claude Desktop / Cursor / etc.) create a sandbox, sync files, and execute commands inside it.

## Run

```bash
# From source
cargo run -p cratebay-mcp
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

## Client Config Notes

In most MCP clients, the config shape is:

- `command`: `cratebay-mcp`
- `env`: set `CRATEBAY_MCP_WORKSPACE_ROOT`

