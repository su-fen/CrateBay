# 更新记录

> [English](CHANGELOG.md) · **中文**

本文件记录本项目的所有重要变更。

格式遵循 [Keep a Changelog](https://keepachangelog.com/en/1.1.0/)，版本号遵循 [Semantic Versioning](https://semver.org/spec/v2.0.0.html)。

## [未发布]

### 新增

- CLI：镜像搜索（`cargobay image search`）与 tag 列表（`cargobay image tags`）。
- CLI：镜像导入/上传（`cargobay image load` / `cargobay image push`）与基于容器打包镜像（`cargobay image pack-container`）。
- CLI：创建并运行容器时支持可选 CPU/内存限制（`cargobay docker run --cpus/--memory`）与可选拉取镜像（`--pull`）。
- CLI：输出容器登录命令（`cargobay docker login-cmd`）。
- GUI：镜像页（搜索 Docker Hub/Quay、对带域名的镜像列出 tags、支持 CPU/内存创建容器、导入/上传镜像）。
- GUI：虚拟机页（预览版 VM 生命周期 UI、VirtioFS 挂载记录、登录命令生成）。
- GUI：显示容器登录命令，并支持将容器打包为镜像（docker commit）。

### 修复

- CI：修复 Clippy/rustfmt 失败（VZ explicit auto-deref、测试里的异步环境锁、Docker 端口类型格式化）。
- GUI：容器列表支持按命名前缀折叠；并提升 `tauri dev` 对 `localhost` 解析问题 / 重复初始化日志的兼容性。

### 变更

- GUI：将 1164 行的 App.tsx 重构为 17 个模块化文件 — types、icons、5 个自定义 Hook、3 个共享组件、5 个页面组件。
- GUI：重新设计 Settings 页面 — 响应式宽度、分组图标、自定义 Toggle Switch。
- GUI：重新设计错误展示 — 结构化 ErrorBanner（图标/标题/操作按钮）、可关闭的 ErrorInline。
- GUI：改进 Panel 组件 — 图标标题、hover 效果、视觉层次优化。
- GUI：修复 Images 搜索结果表格溢出 — 使用弹性 `minmax()` 列宽。
- GUI：优化 VMs 页面信息架构 — VM 列表移至创建表单之上。
- GUI：用 CSS 变量（`--purple-hover`、`--red-dim`）替换所有硬编码主题色值。

## [0.1.0] - 2026-02-28

### 新增

- 用于容器管理的 Tauri + Rust + React GUI 应用。
- Docker 容器生命周期管理（列表、启动、停止、删除）。
- Docker socket 路径自动识别（Colima、OrbStack、默认 `/var/run/docker.sock`）。
- CLI 工具：VM 命令（`list`/`start`/`stop`/`status`）与 Docker 命令（`ps`/`start`/`stop`/`rm`）。
- 深色/浅色主题支持（基于 CSS 自定义属性）。
- 多语言支持（English、中文）。
- 响应式布局：小窗口自动折叠侧边栏。
- CargoBay Logo 与基础品牌元素。
- VM 引擎抽象：`Hypervisor` trait（macOS Virtualization.framework、Linux KVM）。
- VM 管理的 gRPC 协议定义。
- 守护进程（daemon）脚手架，用于后台服务。
- Rust workspace（4 个 crate）：`cargobay-core`、`cargobay-cli`、`cargobay-daemon`、`cargobay-gui`。
- 跨平台设计：条件编译（`#[cfg(target_os)]`）。
- 通过 Bollard 使用 Docker API。

[0.1.0]: https://github.com/coder-hhx/CargoBay/releases/tag/v0.1.0
