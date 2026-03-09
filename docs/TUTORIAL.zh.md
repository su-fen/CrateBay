# CrateBay 使用教程（中文）

> [English](TUTORIAL.md) · **中文**
>
> CrateBay 是一款免费开源的 Docker 容器与轻量 Linux 虚拟机桌面工具，提供 Tauri + React 原生桌面 GUI 与 Rust 命令行。

---

## 目录

1. [环境准备](#1-环境准备)
2. [安装与构建](#2-安装与构建)
3. [GUI 使用指南](#3-gui-使用指南)
4. [CLI 命令参考](#4-cli-命令参考)
5. [Docker Socket 自动识别](#5-docker-socket-自动识别)
6. [配置与数据目录](#6-配置与数据目录)
7. [补充说明](#7-补充说明)

---

## 1. 环境准备

| 工具 | 版本 | 用途 |
|------|---------|---------|
| **Rust** | 1.75+ | 后端、CLI、Tauri 后端 |
| **Node.js** | 22+（建议 22/24 LTS） | GUI 前端（React + Vite） |
| **npm** | 9+ | JavaScript 依赖 |
| **Docker** | 任意 | 容器运行时 |

### 平台兼容性

- **macOS**：兼容 Apple Silicon（M 系列）与 Intel（x86_64）。Rosetta x86_64 仅在 Apple Silicon + macOS 13+ 可用。
- **Windows**：目标兼容 Windows 10 与 Windows 11。VM 后端依赖 Hyper-V（通常需要 Pro/Enterprise/Education + 启用 Hyper-V）。
- **Linux**：VM 后端依赖 KVM（需要 `/dev/kvm` 及权限）。

### 安装 Rust

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source ~/.cargo/env
```

### 安装 Node.js

```bash
# macOS
brew install node@24

# or via nvm
nvm install 24
nvm use 24
```

### Docker 运行时

CrateBay 支持任意 Docker 兼容运行时：

- **Colima**（推荐，免费）— `brew install colima && colima start`
- **Docker Desktop** — 常见 Docker 体验
- **OrbStack** — CrateBay 也会自动识别其 socket

---

## 2. 安装与构建

### 从源码构建

```bash
git clone https://github.com/coder-hhx/CrateBay.git
cd CrateBay

# 安装前端依赖
cd crates/cratebay-gui && npm install && cd ../..

# 构建
cargo build --release
```

### 运行 GUI（开发模式）

```bash
cd crates/cratebay-gui
npm run tauri dev
```

支持热更新：`.tsx` 改动会即时刷新；Rust 改动会触发重新编译。

### 生产构建

```bash
cd crates/cratebay-gui
npm run tauri build
```

如仅需本地安装 macOS `.app`（不打包 `.dmg`）：

```bash
./scripts/install-local-macos-app.sh --open
```

输出目录：`crates/cratebay-gui/src-tauri/target/release/bundle/`

- macOS：`.dmg` / `.app`
- Windows：`.msi` / `.exe`
- Linux：`.deb` / `.rpm` / `.AppImage`

### GA 前发布门禁

```bash
./scripts/release-readiness.sh
```

检查清单：`docs/RELEASE_SMOKE_CHECKLIST.md`

现在这套 GA 前门禁已经包含 Rust 检查、前端 lint/build/i18n 校验、单元测试、基于 Playwright 的 GUI 端到端验证，以及 AI runtime smoke 与真实 Docker runtime smoke（本机有 daemon 时自动执行）。

### GUI 自动化验证

```bash
cd crates/cratebay-gui
npm ci
npm run test:unit
npm run test:e2e:install
npx playwright test
```

这套 Playwright 用例会运行真实构建后的前端，并注入 Tauri bridge mock，覆盖应用壳导航、容器/镜像/卷、VM/Kubernetes、AI Hub、Assistant 与安全护栏回归场景。

如果要补齐运行时级别信心，可以额外执行：

```bash
./scripts/docker-runtime-smoke.sh
./scripts/ai-runtime-smoke.sh
```

详细覆盖矩阵：`docs/VALIDATION_MATRIX.md`

### 仅构建 CLI

```bash
cargo build --release --bin cratebay
# 二进制：target/release/cratebay
```

---

## 3. GUI 使用指南

### Dashboard（仪表盘）

默认首页，会展示卡片式概览：

| 卡片 | 说明 |
|------|-------------|
| **Containers** | 容器总数，点击进入容器管理 |
| **Virtual Machines** | 虚拟机数量（预览） |
| **Images** | 镜像搜索结果数量（最近一次搜索） |
| **System** | Docker 连接状态 |

运行中的容器会在下方预览（最多 5 个）。

### Containers（容器管理）

完整容器管理页面：

- **运行中** — 绿色状态点
- **已停止** — 灰色状态点

**每个容器支持的操作：**

| 操作 | 说明 |
|--------|-------------|
| **Start** | 启动已停止容器 |
| **Stop** | 优雅停止（10 秒超时） |
| **Delete** | 强制停止并删除容器 |
| **Logs** | 查看容器日志（支持实时跟踪/流式） |
| **Env** | 查看容器环境变量 |
| **Login command** | 显示该容器的 `docker exec -it ...` 登录命令 |
| **Package as image** | 基于容器生成新镜像（`docker commit`） |

容器列表每 3 秒自动刷新；右上角会显示连接状态。

### Kubernetes

Kubernetes 页面提供：

- **K3s 集群管理** — 安装、启动、停止、卸载 K3s
- **集群状态** — 版本、节点数、kubeconfig 路径
- **Pods 标签页** — 查看所有命名空间的 Pod 及其状态、就绪数、重启次数、运行时间
- **Services 标签页** — 查看集群 Service（类型、集群 IP、端口）
- **Deployments 标签页** — 查看 Deployment 副本状态
- **命名空间选择器** — 按命名空间筛选或查看全部
- **Pod 日志** — 点击查看任意 Pod 的日志

> 注意：K3s 仅支持 Linux。macOS/Windows 上将在后续版本中通过 CrateBay Linux VM 运行。

### Volumes（存储卷）

Docker 存储卷管理：

- **列出存储卷** — 查看所有 Docker 存储卷（驱动、挂载点）
- **创建存储卷** — 创建命名存储卷
- **查看存储卷** — 查看详细信息（标签、选项、范围）
- **删除存储卷** — 删除未使用的存储卷

### Virtual Machines（虚拟机）

当前 pre-v1 预览构建已支持：

- **创建 / 启动 / 停止 / 删除 / 列表**，完整生命周期管理
- 创建时可设置 **CPU / 内存 / 磁盘**
- **ACPI 优雅关机**（三阶段：requestStop → 轮询 → 强制停止）
- **Rosetta 开关**（仅 macOS Apple Silicon；是否可用取决于 macOS 13+）
- **VirtioFS 文件共享**（真实挂载，标签校验 + guest hints）
- **端口转发**（TCP 代理，暴露 VM 服务）
- **资源监控**（CPU / 内存 / 磁盘 / 网络统计）
- **OS 镜像下载**（Alpine、Ubuntu — 自动下载 kernel/initrd）
- **登录命令**：生成 `ssh user@host -p <port>`

> 注意：VM 元数据会持久化到”配置”目录下的 `vms.json`。VM 运行时后端使用 Virtualization.framework（macOS）、KVM（Linux）或 Hyper-V（Windows）。

### Images（镜像）

当前 pre-v1 预览构建同样支持：

- **镜像搜索**：Docker Hub、Quay
- **标签列表**：对带域名的镜像引用列出 tags（如 `quay.io/org/image`、`ghcr.io/org/image`）
- **基于镜像创建容器**：可选 **CPU 核数 / 内存(MB)**，可选 **创建前拉取**
- **导入自定义镜像**：从本地 `.tar` 归档导入（`docker load -i`）
- **上传镜像到仓库**：`docker push`

> 提示：Docker Hub 的镜像一般使用 `docker run` 风格引用（如 `nginx:latest`）。tags 列表目前需要带域名的引用。

### Settings（设置）

设置页拆分为两个标签页：

- **常规**：主题、语言、更新检查
- **AI**：模型配置、密钥引用、MCP 策略、可执行的 Skills 注册表、Assistant 快捷 skills，以及面向 Codex / Claude 等 CLI 的 Agent/CLI 桥接

其中 GUI 偏好保存在 `localStorage`，AI 配置持久化在 `ai-settings.json`。

当前 AI 开发能力（pre-v1）：

- **AI Hub** 页面标签：`Overview / Models / Sandboxes / MCP / Assistant`
- **Models** 标签已支持 Ollama 运行状态、本地模型列表、拉取 / 删除与存储可见性
- **Sandboxes** 标签已支持模板生命周期、资源限制、TTL 清理、命令执行与本地审计日志
- **MCP** 标签已支持注册表、本地进程启停、日志与客户端配置导出
- **对外 MCP server**：`cratebay-mcp` 通过 stdio 把 CrateBay 沙箱暴露为 MCP tools（见 `docs/MCP_SERVER.zh.md`）
- **Assistant** 已可覆盖容器 / VM / K8s / 模型 / 沙箱 / MCP 的计划与执行，并支持直接运行已启用的快捷 skills
- **直连集成** 当前聚焦于本地 provider profiles，以及 Codex、Claude Code 等 Agent/CLI bridge 预设

> 说明：详细产品规划、版本排期与内部验收门槛均在私有环境维护，不在公开仓库披露。

---

## 4. CLI 命令参考

### 系统状态

```bash
cratebay status
```

示例输出：
```
CrateBay v0.x
Platform: macOS aarch64 (Virtualization.framework available)
Rosetta x86_64: available
Docker: connected (~/.colima/default/docker.sock)
```

### Docker 命令

```bash
# 列出容器
cratebay docker ps

# 运行一个新容器（可选 CPU/内存限制，可选拉取镜像）
cratebay docker run nginx:latest --name web --cpus 2 --memory 512 --pull

# 启动容器
cratebay docker start <container_id>

# 停止容器
cratebay docker stop <container_id>

# 删除容器（强制）
cratebay docker rm <container_id>

# 输出容器登录命令（shell）
cratebay docker login-cmd web
```

### VM 命令

> 可选：先启动 daemon 来管理 VM：
>
> ```bash
> cargo run -p cratebay-daemon
> ```
>
> CLI/GUI 在可连接到 daemon 时会自动通过 gRPC 调用（可用 `CRATEBAY_GRPC_ADDR` 配置地址）；不可用时会自动回退到本地模式。
>
> macOS VZ PoC：启动 VM 前需要设置 `CRATEBAY_VZ_KERNEL`（可选 `CRATEBAY_VZ_INITRD`）。

```bash
# 创建 VM（可自定义 CPU 核数与内存）
cratebay vm create myvm --cpus 4 --memory 4096 --disk 20

# Apple Silicon 上启用 Rosetta x86 翻译
cratebay vm create myvm --cpus 4 --memory 4096 --rosetta

# 启动 / 停止 / 删除
cratebay vm start myvm
cratebay vm stop myvm
cratebay vm delete myvm

# 列出全部 VM
cratebay vm list

# 输出 VM 登录命令（SSH，需要你提供端口）
cratebay vm login-cmd myvm --user root --host 127.0.0.1 --port 2222
```

### 镜像命令

```bash
# 搜索镜像（Docker Hub / Quay）
cratebay image search nginx --source all --limit 20

# 列出某个 OCI 镜像仓库的 tags（支持 ghcr.io/quay.io/私有仓库等）
cratebay image tags ghcr.io/owner/image --limit 50

# 导入镜像归档（.tar）
cratebay image load ./image.tar

# 上传镜像到仓库
cratebay image push ghcr.io/owner/image:tag

# 基于已有容器打包镜像
cratebay image pack-container web myorg/web:snapshot
```

### 文件共享（VirtioFS）

```bash
# 把宿主机目录挂载到 VM 内
cratebay mount add \
  --vm myvm \
  --tag code \
  --host-path ~/code \
  --guest-path /mnt/code

# 只读挂载
cratebay mount add \
  --vm myvm \
  --tag data \
  --host-path ~/data \
  --guest-path /mnt/data \
  --readonly

# 查看挂载
cratebay mount list --vm myvm

# 移除挂载
cratebay mount remove --vm myvm --tag code
```

### 存储卷命令

```bash
# 列出所有 Docker 存储卷
cratebay volume list

# 创建存储卷
cratebay volume create mydata

# 查看存储卷详情
cratebay volume inspect mydata

# 删除存储卷
cratebay volume remove mydata
```

### K3s 命令

```bash
# 查看 K3s 集群状态
cratebay k3s status

# 安装 K3s（仅 Linux；从 GitHub releases 下载）
cratebay k3s install

# 启动 K3s 集群
cratebay k3s start

# 停止 K3s 集群
cratebay k3s stop

# 卸载 K3s（删除二进制和数据）
cratebay k3s uninstall
```

> 注意：K3s 仅支持 Linux。macOS/Windows 上将在后续版本中通过 CrateBay Linux VM 运行。

### Shell 自动补全

为你的 Shell 生成自动补全脚本：

```bash
# Bash
cratebay completions bash >> ~/.bashrc

# Zsh
cratebay completions zsh >> ~/.zshrc

# Fish
cratebay completions fish > ~/.config/fish/completions/cratebay.fish
```

添加补全后，重新启动 Shell 或 source 配置文件即可生效。

---

## 5. Docker Socket 自动识别

CrateBay 会按以下顺序自动识别 Docker socket：

| 优先级 | 路径 | 运行时 |
|----------|------|---------|
| 1 | `~/.colima/default/docker.sock` | Colima |
| 2 | `~/.orbstack/run/docker.sock` | OrbStack |
| 3 | `/var/run/docker.sock` | Docker Desktop / 原生 |
| 4 | `~/.docker/run/docker.sock` | Docker Desktop（备用） |

**Windows：** 也会尝试 `//./pipe/docker_engine` 与 `//./pipe/dockerDesktopLinuxEngine`。

### 覆盖默认识别顺序

```bash
export DOCKER_HOST=unix:///path/to/custom/docker.sock
cratebay docker ps
```

---

## 6. 配置与数据目录

### 环境变量

| 变量 | 说明 |
|----------|-------------|
| `DOCKER_HOST` | 覆盖 Docker socket 路径 |
| `RUST_LOG` | 日志级别（`info` / `debug` / `trace`） |
| `CRATEBAY_GRPC_ADDR` | Daemon gRPC 地址（默认：`127.0.0.1:50051`） |
| `CRATEBAY_DAEMON_PATH` | 覆盖 daemon 可执行文件路径（GUI 自动拉起） |
| `CRATEBAY_CONFIG_DIR` | 覆盖配置目录（保存 `vms.json`） |
| `CRATEBAY_DATA_DIR` | 覆盖数据目录 |
| `CRATEBAY_LOG_DIR` | 覆盖日志目录 |
| `CRATEBAY_OLLAMA_BASE_URL` | 覆盖 AI Hub Models 使用的本地 Ollama 兼容 HTTP 地址 |
| `CRATEBAY_LOG_RETENTION_DAYS` | 错误日志保留天数（默认：7） |
| `CRATEBAY_VZ_RUNNER_PATH` | 覆盖 `cratebay-vz` 路径（macOS VZ PoC） |
| `CRATEBAY_VZ_KERNEL` | Linux kernel 路径（macOS VZ PoC） |
| `CRATEBAY_VZ_INITRD` | Linux initrd 路径（可选，macOS VZ PoC） |
| `CRATEBAY_VZ_CMDLINE` | Linux kernel 启动参数（默认：`console=hvc0`，macOS VZ PoC） |

### 数据目录

| 平台 | 配置 | 数据 | 日志 |
|----------|--------|------|------|
| macOS | `~/Library/Application Support/com.cratebay.app/` | 同上 | 同上 |
| Linux | `~/.config/cratebay/` | `~/.local/share/cratebay/` | 同上 |
| Windows | `%APPDATA%\cratebay\` | 同上 | 同上 |

VM 元数据文件位于 `<config>/vms.json`。

错误日志会写入“日志”目录，文件名为 `cratebay-error.log.YYYY-MM-DD`，并会自动清理（默认仅保留近 7 天）。

---

## 7. 补充说明

详细路线图、版本规划与节奏均在私有环境维护。

---

## License

Apache License 2.0 — 可免费用于个人与商业用途。
