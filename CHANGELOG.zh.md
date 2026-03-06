# 更新记录

> [English](CHANGELOG.md) · **中文**

本文件记录本项目的所有重要变更。

格式遵循 [Keep a Changelog](https://keepachangelog.com/en/1.1.0/)，版本号遵循 [Semantic Versioning](https://semver.org/spec/v2.0.0.html)。

## [未发布]

### 新增

#### AI Hub 预览能力
- 新增顶层 AI Hub 页面，包含 `Overview / Models / Sandboxes / MCP / Assistant` 标签。
- GUI 接入 Ollama 第一阶段能力（`ollama_status`、`ollama_list_models`）：运行状态检测 + 本地模型列表。
- 新增 AI 引导脚本 `scripts/setup-ai.sh`，用于前置检查与可选的 best-effort 安装流程。
- 新增 `tools/opensandbox/` 本地脚手架，提供模板配置和启动说明。
- 新增 `docs/VISION.md` 与 `docs/VISION.zh.md`，明确 AI Hub 方向与版本策略。

### 变更

- 将当前公开预览线重置为 `v0.7.0`，把 `v1.0.0` 保留给 GA。
- 刷新官网布局，并统一 README / Roadmap / Tutorial 的 preview 口径。
- 更新路线图，明确 AI Hub 补完与 pre-v1 验证阶段。

## [0.7.0] - 2026-03-02

### 新增

#### VM 生命周期与平台后端
- Virtualization.framework FFI（Swift 桥接），实现 macOS 上真正的 VM 启动/停止。
- Linux KVM VM 后端（kvm-ioctls、vCPU、内存、内核加载、串口控制台）。
- Windows Hyper-V VM 后端脚手架。
- ACPI 优雅关机（VZ 桥接 3 阶段：requestStop -> 轮询 -> 强制停止）。
- VM 端到端真实执行（OS 镜像下载 -> 内核/initrd -> 启动）。
- VM 控制台（串口输出），全平台支持。
- VM 端口转发（TCP 代理）。
- VM/容器资源监控（CPU / 内存 / 磁盘 / 网络）。
- OS 镜像目录与下载管理。

#### VirtioFS 与文件共享
- VirtioFS 真实挂载实现（tag 验证、挂载追踪、guest hints）。
- VirtioFS 挂载管理（UI + 守护进程）。

#### Kubernetes
- K3s 集成（按需下载、安装、启动、停止、卸载）。
- Kubernetes 仪表盘（Pods、Services、Deployments、命名空间选择器、Pod 日志）。

#### 容器与镜像管理
- 容器日志流式查看（实时跟踪）。
- 容器 exec / 终端集成。
- 容器环境变量查看。
- 本地镜像管理（列表、删除、标签、详情）。
- Docker 存储卷管理（列表、创建、详情、删除）。
- CLI：镜像搜索（`cratebay image search`）与 tag 列表（`cratebay image tags`）。
- CLI：镜像导入/上传（`cratebay image load` / `cratebay image push`）与基于容器打包镜像（`cratebay image pack-container`）。
- CLI：创建并运行容器时支持可选 CPU/内存限制（`cratebay docker run --cpus/--memory`）与可选拉取镜像（`--pull`）。
- CLI：输出容器登录命令（`cratebay docker login-cmd`）。
- GUI：镜像页（搜索 Docker Hub/Quay、对带域名的镜像列出 tags、支持 CPU/内存创建容器、导入/上传镜像）。
- GUI：虚拟机页（VM 生命周期 UI、VirtioFS 挂载记录、登录命令生成）。
- GUI：显示容器登录命令，并支持将容器打包为镜像（docker commit）。
- 镜像导入弹窗：原生文件选择对话框，用于选择 `.tar` 归档文件（通过 `tauri-plugin-dialog`）。

#### GUI 增强
- UI 设计系统规范（`docs/DESIGN_SYSTEM.md`）— 统一设计令牌、按钮/输入框/弹窗规范。
- 容器页新增"创建容器"按钮，支持直接输入镜像名创建容器。
- 导航顺序调整：镜像排在虚拟机之前。
- GUI：重新设计 Settings 页面 — 响应式宽度、分组图标、自定义 Toggle Switch。
- GUI：重新设计错误展示 — 结构化 ErrorBanner（图标/标题/操作按钮）、可关闭的 ErrorInline。
- GUI：改进 Panel 组件 — 图标标题、hover 效果、视觉层次优化。
- GUI：用 CSS 变量（`--purple-hover`、`--red-dim`）替换所有硬编码主题色值。
- 自动更新检查（GitHub releases）。

#### 开发者体验与基础设施
- Shell 补全（bash、zsh、fish、elvish、powershell）。
- 路线图文档（`docs/ROADMAP.md`）。
- CI/CD 流水线（GitHub Actions：CI + 发布构建）。
- 全面测试套件（177+ 测试覆盖 core、CLI、daemon、集成测试）。
- 安全审计与加固（输入验证、路径遍历防护、日志脱敏）。

### 修复

- CI：修复 Clippy/rustfmt 失败（VZ explicit auto-deref、测试里的异步环境锁、Docker 端口类型格式化）。
- GUI：容器列表支持按命名前缀折叠；并提升 `tauri dev` 对 `localhost` 解析问题 / 重复初始化日志的兼容性。
- GUI：修复 Images 搜索结果表格溢出 — 使用弹性 `minmax()` 列宽。

### 变更

- GUI：统一按钮高度（默认 32px、小号 28px）、输入框高度（32px）、图标 stroke-width（2）。
- GUI：Images 工具栏简化 — 移除 limit 输入框和清空按钮。
- GUI：`.btn.small` -> `.btn.sm`、`.btn.tiny` -> `.btn.xs`，移除 `.input.small`。
- GUI：将 1164 行的 App.tsx 重构为 17 个模块化文件 — types、icons、5 个自定义 Hook、3 个共享组件、5 个页面组件。
- GUI：优化 VMs 页面信息架构 — VM 列表移至创建表单之上。

## [0.1.0] - 2026-02-28

### 新增

- 用于容器管理的 Tauri + Rust + React GUI 应用。
- Docker 容器生命周期管理（列表、启动、停止、删除）。
- Docker socket 路径自动识别（Colima、OrbStack、默认 `/var/run/docker.sock`）。
- CLI 工具：VM 命令（`list`/`start`/`stop`/`status`）与 Docker 命令（`ps`/`start`/`stop`/`rm`）。
- 深色/浅色主题支持（基于 CSS 自定义属性）。
- 多语言支持（English、中文）。
- 响应式布局：小窗口自动折叠侧边栏。
- CrateBay Logo 与基础品牌元素。
- VM 引擎抽象：`Hypervisor` trait（macOS Virtualization.framework、Linux KVM）。
- VM 管理的 gRPC 协议定义。
- 守护进程（daemon）脚手架，用于后台服务。
- Rust workspace（4 个 crate）：`cratebay-core`、`cratebay-cli`、`cratebay-daemon`、`cratebay-gui`。
- 跨平台设计：条件编译（`#[cfg(target_os)]`）。
- 通过 Bollard 使用 Docker API。

[未发布]: https://github.com/coder-hhx/CrateBay/compare/v0.7.0...HEAD
[0.7.0]: https://github.com/coder-hhx/CrateBay/compare/v0.1.0...v0.7.0
[0.1.0]: https://github.com/coder-hhx/CrateBay/releases/tag/v0.1.0
