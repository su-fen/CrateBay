# CrateBay AI Hub 愿景

> 面向公开仓库的产品方向说明。
> 当前公开预览版本线：`v0.7.0`。

## 1）产品定位

CrateBay 正在构建一款自由开源的桌面基础设施应用，覆盖：

- Docker 容器
- Linux 虚拟机
- 本地 Kubernetes
- 正在补完的 AI Hub（Models / Sandboxes / MCP / Assistant）

AI Hub 不是独立产品，而是建立在现有容器 / VM / K8s 能力之上的下一层工作流。

## 2）为什么要做 AI Hub

当前代码库已经具备较强的基础设施底座：

- 基于 Tauri 的原生桌面体验
- 跨平台 VM 后端
- Docker API 直连
- 本地 K3s 管理
- 策略、审计与安全设置基础能力

AI Hub 的作用，是把这些底座能力提升为更高层的操作面：

- **Models**：本地与远程模型运行时
- **Sandboxes**：隔离执行环境
- **MCP**：受控的工具连接层
- **Assistant**：围绕现有产品能力的计划 / 执行 / 解释流程

## 3）当前快照（`v0.7.0` 预览版）

当前预览版已经包含：

- 顶层 **AI Hub** 页面：`Overview / Models / Sandboxes / MCP / Assistant`
- **Assistant** 计划生成与步骤执行，带策略校验与审计日志
- **AI Settings**：Provider 配置、密钥引用、MCP allowlist、CLI presets
- **Ollama 第一阶段**：运行状态探测与本地模型列表
- **Sandboxes MVP**：模板化生命周期、资源限制、TTL 元数据与本地审计日志
- 本地引导资产：`scripts/setup-ai.sh` 与 `tools/opensandbox/`

## 4）进入 `v1.0.0` 前仍缺什么

在以下范围全部完成并验证之前，对外版本线保持在 `v0.x.0`：

- **Models**：拉取 / 删除动作，以及更清晰的本地存储管理
- **Sandboxes**：TTL 自动清理、Assistant/Skills 执行器、可选 OpenSandbox 后端路径
- **MCP**：真正的 Server 注册表、生命周期管理、日志与客户端配置导出
- **Assistant**：不再只是局部接线，而是完整贯通 AI Hub 各个面板
- **发布门槛**：三平台安装验证、升级路径验证、隐私审计、最终文档/官网一致性

## 5）版本策略

- `v0.7.0` = 当前公开预览线
- `v0.8.0` = AI Hub 功能补完阶段
- `v0.9.0` = pre-v1 打磨与验证阶段
- `v1.0.0` = 仅保留给首个真正的 GA 版本

换句话说：CrateBay 不再把 `v1.0.0` 当作预览标签使用。

## 6）版本规划（pre-v1 → GA）

- **v0.7.0 — AI Hub Preview**
  - 当前预览版：Assistant、Provider Settings、Ollama phase 1、Sandbox MVP
- **v0.8.0 — AI Hub Completion**
  - Models phase 2
  - Sandboxes 补完
  - MCP Manager MVP
- **v0.9.0 — Pre-v1 Hardening**
  - Smoke tests、升级验证、隐私审计、文案一致性
- **v1.0.0 — Official GA**
  - 只有在 AI Hub 范围完整闭环、并通过所有发布门槛后才进入

## 7）工具与依赖引导

当前预览线已经包含本地引导资产：

- `scripts/setup-ai.sh`：检查依赖并执行尽力安装
- `tools/opensandbox/`：可选 sandbox runtime 的本地脚手架

## 8）公开口径

在 `v1.0.0` 真正准备好之前：

- README 与官网持续保持 `Coming Soon`
- `v0.x.0` 一律使用 `preview` 口径
- 不提前暗示 GA 或“已经完成全部产品范围”
