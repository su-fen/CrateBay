# CrateBay 愿景 — 自托管 AI 基础设施桌面（Self‑hosted AI Infra Desktop）

CrateBay 已经是一套 **容器 / Linux 虚拟机 / Kubernetes 的桌面 GUI**（Rust + Tauri，轻量且跨平台）。最自然的延伸，是把这套底座演进成 **自托管 AI 基础设施桌面**：

- **Agent Sandboxes**：一键创建、强隔离的 AI Agent 执行环境（本地化 / 开源化）。
- **本地模型运行底座**：一键管理本地模型运行时（启动/停止、模型生命周期、GPU 可见性）。
- **MCP Server 管理器**：把 MCP 配置和多 Server 管理从“手写 JSON”变成可视化和可复用。

本文是产品与架构提案（不代表已发布能力）。

---

## 1）为什么这个方向最贴合 CrateBay

CrateBay 现有能力天然对应 “AI Infra” 的基本积木：

- **编排能力**：已经统一管理 Docker + VM + K3s。
- **Local-first 体验**：桌面 GUI，低学习成本，反馈快。
- **安全护栏**：已有 AI 设置、审计思路与 MCP 策略脚手架。

AI Agent 工具的趋势很明确：**隔离执行**（安全、可复现、可控成本）+ **标准化工具协议**（MCP）。一个能把“自托管 AI 基建”做成可视化的一体化桌面工具，有清晰叙事与传播路径。

---

## 2）三大支柱（以及 MVP 的边界）

### 支柱 A — Agent Sandboxes（“Portainer for AI”的切入点）

**目标：** 用户可以在秒级创建/启动/停止/查看一个 “AI Agent Sandbox”，并且具备可预期的隔离与资源控制。

**MVP（桌面优先）建议：**

- 模板化一键创建（Node/Python/Rust 等常见开发镜像）。
- CPU/RAM/磁盘限制，TTL/自动清理。
- Volume 挂载（workspace 输入、产物输出）。
- 网络策略预设（离线 / allowlist / 全通）。
- 统一的日志、状态、审计轨迹。

**后端策略：** 把 sandbox runtime 做成可插拔：

- **Docker-only（最快落地）**：容器 + 强化默认配置。
- **OpenSandbox 兼容（推荐路线）**：把 OpenSandbox 作为可选本地服务，由 CrateBay 管理并通过 API 调用。
- 未来：VM 级 sandbox（极致隔离场景）。

### 支柱 B — 本地模型运行底座（Ollama-first）

**目标：** CrateBay 成为“本地模型运行管理中心”，解决模型下载、切换、资源可视化等痛点。

**MVP：**

- 自动发现本地运行时（优先支持 Ollama）。
- 模型列表 / 下载（pull）/ 删除、存储位置与占用。
- “模型组合（model set）/ 工作流预设”一键切换。
- GPU 可见性 + 显存快照（按平台 best-effort）。

**MVP 非目标：** 复杂的跨应用 GPU 调度、深度驱动安装自动化。

### 支柱 C — MCP Server 管理器（把配置复杂度降到最低）

**目标：** 像管理容器一样管理 MCP Servers：可发现、可复用、可控风险。

**MVP：**

- MCP Server 注册表（名称、命令/容器、env、secret 引用）。
- 一键启动/停止 + 健康检查 + 日志。
- 从同一份配置导出 Claude Code / Cursor 等客户端配置。
- 策略：按 Server 的权限、网络模式、workspace 挂载规则。

---

## 3）架构改造建议（尽量不打断 Core）

### Core 稳定，AI 叠加

不要把现有容器/VM 产品变成“科学实验”。AI 能力应当是 **叠加层**：

- 保持现有页面（Dashboard / Containers / Images / Volumes / VMs / Kubernetes）。
- 新增一个 **AI Hub**：Models / Sandboxes / MCP / Assistant。

### 引入 “Managed Services” 统一抽象

AI Infra 少不了后台服务。建议抽象成统一的内部能力：

- `Service`：install / configure / start / stop / status / logs
- 典型服务：**Ollama**、**OpenSandbox server**、各类 **MCP servers**

这与现有 CrateBay 风格一致（例如 K3s 的按需下载/启动/停止）。

### 复用现有 AI 脚手架

CrateBay 已有 AI 设置与 Skills Registry 的基础模型，不要推倒重来：

- 规划新增 skill executor：例如 `sandbox_action`，把步骤执行迁移到 sandbox 里。
- Assistant 默认（可开关）在 sandbox 内执行，主机只做编排与审计。

---

## 4）命名与定位建议

### 建议保留 “CrateBay”

优点：

- 与当前底座高度一致（容器/VM），也延续 Rust “crate” 的品牌心智。
- 避免重命名带来的成本（仓库、域名、应用 ID、历史版本、传播素材）。

建议调整的点：

- Tagline 从 “容器/VM GUI” 扩展为 “容器 + VM + 自托管 AI 基础设施”。
- 导航与文档明确新增 AI 模块。

### 可选：AI 层做子品牌

如果需要更强的 AI 记忆点，但又不想整体改名：

- “CrateBay AI”（功能集合）
- “CrateBay Sandbox”（Agent 运行时）
- “CrateBay MCP Manager”（MCP 工具链）

---

## 5）页面布局：建议演进式改造，不必整体重构

在 AI 三大模块至少落地一个迭代前，不建议“整体重构”。更稳妥的做法：

- 顶部新增 **AI** 大类（4 个子页）：
  - **Models**：运行时 + 模型管理
  - **Sandboxes**：模板 + 运行中实例
  - **MCP**：Servers + 导出配置
  - **Assistant**：自然语言 → 计划 → sandbox 执行
- Dashboard 增加 AI 相关小组件（运行中 sandboxes、模型运行时状态、MCP servers）。

---

## 6）后续规划（GA 之后）

- **v1.0 GA**：收尾跨平台安装包 + 文档 + 官网一致性。
- **v1.2 “AI Infra MVP”**：
  - Managed Services（Ollama/OpenSandbox/MCP 作为服务）
  - Ollama 集成（发现 + list/pull/delete）
  - MCP Server Manager MVP（启停/日志 + 导出配置）
- **v1.3 “Agent Sandbox v1”**：
  - sandbox 模板 + 生命周期 + 审计
  - OpenSandbox 集成（可选 runtime）
  - Assistant → sandbox 执行（可选）
- **v1.4 “GPU + Scale”**：
  - GPU 可观测性增强、多运行时支持
  - 远程主机 / 多机 sandbox backend（可选扩展）

---

## 7）工具与依赖自动化

建议新增脚本用于 **检查** 和可选的 **自动安装**：

- Docker（含 `docker compose`）
- Ollama
- 可选：OpenSandbox server（docker compose）

见 `scripts/setup-ai.sh`。

---

## 8）当前落地快照（截至 2026-03-06）

当前预览版已落地：

- 顶层 **AI Hub** 页面，含 `Overview / Models / Sandboxes / MCP / Assistant`。
- **Ollama 第一阶段** 集成：GUI 可查看运行状态与本地模型列表。
- **Agent Sandboxes MVP**：模板化沙箱生命周期（`create/start/stop/delete/inspect`），含资源限制、TTL 元数据与本地审计日志。
- 本地引导资产：`scripts/setup-ai.sh` 与 `tools/opensandbox/`。

短期执行重点：

- MCP Manager MVP：Server 注册表 + 启停/日志 + 客户端配置导出。
- Ollama 第二阶段：模型拉取/删除动作与更完整的存储管理。
- 沙箱 TTL 自动清理与 Assistant/Skills 沙箱执行器集成。

发布状态：

- CrateBay 仍处于 **pre-v1 开发阶段**。
- 仅在 Models / Sandboxes / MCP 全部范围完成并通过发布验证后，才会进入 `v1.0.0` 正式发布。

---

## 参考阅读（源头资料）

- OpenSandbox：`https://github.com/alibaba/OpenSandbox`
- OpenSandbox 文档：`https://docs.open-sandbox.ai/`
- MCP：`https://modelcontextprotocol.io/`
