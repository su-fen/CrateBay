# CrateBay MCP Server（对外）

CrateBay 提供一个对外 MCP server 可执行程序（`cratebay-mcp`），通过 **stdio** 把 **CrateBay 托管沙箱** 暴露为 MCP tools。

这样支持 MCP 的 AI 客户端（Claude Desktop / Cursor 等）就可以：创建沙箱、同步文件，并在沙箱里执行命令完成工作流。

## 运行

```bash
# 从源码运行
cargo run -p cratebay-mcp
```

## 安装（推荐）

多数 MCP 客户端希望使用一个稳定的可执行文件路径（而不是 `cargo run`）。

```bash
# 构建本地 release 可执行文件
cargo build -p cratebay-mcp --release

# 可执行文件路径：
#   target/release/cratebay-mcp
```

或者安装到 Cargo bin 目录：

```bash
cargo install --path crates/cratebay-mcp --locked
```

## 工作区与安全策略

文件同步与宿主机目录挂载会被刻意约束。

请先设置工作区根目录（文件相关能力需要它）：

```bash
export CRATEBAY_MCP_WORKSPACE_ROOT="/你的/绝对/路径"
```

然后在 MCP 客户端配置里把该环境变量传给 `cratebay-mcp`。

**确认开关：** 所有写入/破坏性 tool 都要求显式 `confirmed=true`，避免静默修改。

## Tools 列表

- `cratebay_sandbox_templates` — 列出内置模板
- `cratebay_sandbox_create` — 创建并启动沙箱（可选 `mounts`，需要 `confirmed=true`）
- `cratebay_sandbox_list` / `cratebay_sandbox_inspect`
- `cratebay_sandbox_exec` — 在沙箱里执行命令（需要 `confirmed=true`）
- `cratebay_sandbox_start` / `cratebay_sandbox_stop` / `cratebay_sandbox_delete`（需要 `confirmed=true`）
- `cratebay_sandbox_cleanup_expired`（需要 `confirmed=true`）
- `cratebay_sandbox_put_path` — `docker cp` 本地 → 沙箱（需要 `confirmed=true` + workspace root）
- `cratebay_sandbox_get_path` — `docker cp` 沙箱 → 本地（需要 `confirmed=true` + workspace root）

## 自检（Smoke）

```bash
./scripts/cratebay-mcp-smoke.sh
```

## 客户端配置说明

### Claude Desktop（macOS / Windows）

编辑 Claude Desktop 配置并重启应用。

配置文件位置：

- macOS：`~/Library/Application Support/Claude/claude_desktop_config.json`
- Windows：`%APPDATA%\\Claude\\claude_desktop_config.json`

示例：

```json
{
  "mcpServers": {
    "cratebay": {
      "command": "cratebay-mcp",
      "args": [],
      "env": {
        "CRATEBAY_MCP_WORKSPACE_ROOT": "/你的/绝对/路径"
      }
    }
  }
}
```

如果你的 `PATH` 里找不到 `cratebay-mcp`，请把 `command` 改成可执行文件完整路径
（例如 `.../target/release/cratebay-mcp`）。

### OpenAI Codex CLI / IDE 插件

把它作为 stdio MCP server 添加进去：

```bash
codex mcp add cratebay --env CRATEBAY_MCP_WORKSPACE_ROOT=/你的/绝对/路径 -- cratebay-mcp
```

或直接写入 `~/.codex/config.toml`：

```toml
[mcp_servers.cratebay]
command = "cratebay-mcp"
args = []

[mcp_servers.cratebay.env]
CRATEBAY_MCP_WORKSPACE_ROOT = "/你的/绝对/路径"
```

### Cursor（可选）

Cursor 会从 `mcp.json`（全局或项目级）读取 MCP servers 配置；配置结构与 Claude Desktop
类似：`command`、`args`、`env`。

### confirmed=true

CrateBay 刻意要求写入/破坏性 tool 必须显式 `confirmed=true`，避免静默修改。
如果你的 AI 客户端一直 “失败”，通常是因为没传 `confirmed=true`。

多数 MCP 客户端都支持这种最小配置形态：

- `command`: `cratebay-mcp`
- `env`: 设置 `CRATEBAY_MCP_WORKSPACE_ROOT`
