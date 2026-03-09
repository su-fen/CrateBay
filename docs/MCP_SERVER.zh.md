# CrateBay MCP Server（对外）

CrateBay 提供一个对外 MCP server 可执行程序（`cratebay-mcp`），通过 **stdio** 把 **CrateBay 托管沙箱** 暴露为 MCP tools。

这样支持 MCP 的 AI 客户端（Claude Desktop / Cursor 等）就可以：创建沙箱、同步文件，并在沙箱里执行命令完成工作流。

## 运行

```bash
# 从源码运行
cargo run -p cratebay-mcp
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

## 客户端配置说明

多数 MCP 客户端都支持这种配置形态：

- `command`: `cratebay-mcp`
- `env`: 设置 `CRATEBAY_MCP_WORKSPACE_ROOT`

