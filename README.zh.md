<p align="center">
  <img src="https://raw.githubusercontent.com/coder-hhx/CargoBay/master/assets/logo.png" alt="CargoBay" width="128" />
</p>

<h1 align="center">CargoBay</h1>

<p align="center">
  <strong>免费开源的容器与 Linux 虚拟机桌面工具。</strong><br>
  轻量级 Linux 虚拟机、Docker 容器、Kubernetes —— 集成在一个应用里。
</p>

<p align="center">
  <a href="README.md">English</a> ·
  <strong>中文</strong>
</p>

<p align="center">
  <a href="https://github.com/coder-hhx/CargoBay/releases">下载</a> ·
  <a href="https://github.com/coder-hhx/CargoBay/issues">问题反馈</a> ·
  <a href="docs/ARCHITECTURE.md">架构</a> ·
  <a href="docs/TUTORIAL.zh.md">教程</a> ·
  <a href="CHANGELOG.zh.md">更新记录</a>
</p>

<p align="center">
  <img src="https://img.shields.io/badge/license-Apache%202.0-blue" />
  <img src="https://img.shields.io/badge/rust-1.75+-orange" />
  <img src="https://img.shields.io/badge/platform-macOS%20|%20Linux%20|%20Windows-lightgrey" />
</p>

---

## 为什么是 CargoBay？

OrbStack 很优秀，但它**闭源且仅支持 macOS**。Docker Desktop **较重且存在商业订阅限制**。Podman Desktop、Rancher Desktop 基于 **Electron（300-500MB RAM）**。我们希望给开发者一个更轻、更自由的选择：

- **名字含义**：*CargoBay* = `cargo`（容器，也致敬 Rust 的 `cargo`）+ `bay`（停泊虚拟机与开发环境的港湾）
- **100% 免费开源** — Apache 2.0，无授权费、无遥测
- **Rust + Tauri 原生** — 非 Electron，空闲内存目标 <200MB
- **VM + 容器统一** — 一套工具管理全部
- **跨平台** — macOS、Linux、Windows

## 平台兼容性

- **macOS**：兼容 Apple Silicon（M 系列）与 Intel（x86_64）。Rosetta x86_64 仅在 Apple Silicon + macOS 13+ 可用。
- **Windows**：目标兼容 Windows 10 与 Windows 11。VM 后端依赖 Hyper-V（通常需要 Pro/Enterprise/Education + 启用 Hyper-V）。
- **Linux**：VM 后端依赖 KVM（需要 `/dev/kvm` 及权限）。

## 对比

| | CargoBay | OrbStack | Docker Desktop | Podman Desktop | Colima |
|---|:---:|:---:|:---:|:---:|:---:|
| **开源** | ✅ | ❌ | 部分 | ✅ | ✅ |
| **商业可免费使用** | ✅ | ❌ | ❌（>250 人） | ✅ | ✅ |
| **GUI** | Tauri（原生） | Swift（原生） | Electron | Electron | 无 |
| **空闲内存** | <200 MB | <1 GB | 3-6 GB | 300-500 MB | ~400 MB |
| **macOS** | ✅ | ✅ | ✅ | ✅ | ✅ |
| **Windows** | 计划中 | ❌ | ✅ | ✅ | ❌ |
| **Linux** | 计划中 | ❌ | ✅ | ✅ | ✅ |
| **Docker 管理** | ✅ | ✅ | ✅ | ✅ | ✅ |
| **Linux VM** | ✅ | ✅ | ❌ | ❌ | 间接 |
| **Kubernetes** | ✅（K3s + 仪表盘） | ✅ | ✅ | ✅ | ✅（K3s） |
| **自动端口转发** | 计划中 | ✅ | ✅ | ❌ | ✅ |
| **VirtioFS 共享** | ✅ | ✅ | ✅ | ❌ | ✅ |
| **技术栈** | Rust | Swift | Go + Electron | Electron + TS | Go |

## 功能

| 功能 | macOS | Linux | Windows | 状态 |
|---------|-------|-------|---------|--------|
| Docker 容器管理 | ✅ | ✅ | ✅ | 可用 |
| 容器日志流式查看 | ✅ | ✅ | ✅ | 可用 |
| 容器 exec / 终端 | ✅ | ✅ | ✅ | 可用 |
| 容器环境变量查看 | ✅ | ✅ | ✅ | 可用 |
| Dashboard & GUI | ✅ | ✅ | ✅ | 可用 |
| 镜像搜索（Docker Hub / Quay） | ✅ | ✅ | ✅ | 可用 |
| 本地镜像管理（列表/删除/标签/详情） | ✅ | ✅ | ✅ | 可用 |
| 导入/上传镜像（docker load/push） | ✅ | ✅ | ✅ | 可用 |
| 基于容器打包镜像（docker commit） | ✅ | ✅ | ✅ | 可用 |
| Docker 存储卷管理 | ✅ | ✅ | ✅ | 可用 |
| 轻量级 Linux VM | ✅ Virtualization.framework | ✅ KVM | ✅ Hyper-V | 可用 |
| VM 控制台（串口输出） | ✅ | ✅ | ✅ | 可用 |
| VM 端口转发 | ✅ | ✅ | ✅ | 可用 |
| VM 资源监控 | ✅ | ✅ | ✅ | 可用 |
| OS 镜像下载与管理 | ✅ | ✅ | ✅ | 可用 |
| Rosetta x86_64 翻译 | ✅ Apple Silicon | — | — | 可用 |
| VirtioFS 文件共享 | ✅ | ✅ virtiofsd | ✅ Plan 9/SMB | 可用 |
| K3s 集群管理 | ✅ | ✅ | 📋 | 可用（Linux） |
| Kubernetes 仪表盘（Pods/Services/Deployments） | ✅ | ✅ | ✅ | 可用 |
| 自动更新检查 | ✅ | ✅ | ✅ | 可用 |
| CLI（VM + Docker + K3s + Mount） | ✅ | ✅ | ✅ | 可用 |
| 深色/浅色主题 + i18n | ✅ | ✅ | ✅ | 可用（中/英） |

## 技术栈

- **Core**：Rust（跨平台 workspace）
- **GUI**：Tauri v2 + React（TypeScript）
- **VM Engine**：Virtualization.framework（macOS）/ KVM（Linux）/ Hyper-V（Windows）
- **文件共享**：VirtioFS（macOS/Linux）/ Plan 9（Windows）
- **x86 模拟**：Rosetta 2（macOS Apple Silicon）
- **容器**：Docker API（Bollard，直连 Docker socket）
- **Kubernetes**：K3s（按需下载）+ kubectl
- **CLI**：Rust（clap）
- **IPC**：gRPC（tonic + prost）— 仅用于 VM 操作；容器直连 Docker socket

## 快速开始

> CargoBay v1.0.0 现已发布。欢迎反馈与贡献！

```bash
# 从源码构建
git clone https://github.com/coder-hhx/CargoBay.git
cd CargoBay
cargo build --release

# CLI 示例
cargobay status                              # 平台信息
cargobay image search nginx --source all --limit 20
cargobay image load ./image.tar
cargobay image push ghcr.io/owner/image:tag
cargobay docker run nginx:latest --name web --cpus 2 --memory 512 --pull
cargobay image pack-container web myorg/web:snapshot
cargobay docker login-cmd web
cargobay docker ps                           # 容器列表
cargobay vm create myvm --cpus 4 --memory 4096 --rosetta  # 创建 VM（Rosetta）
cargobay mount add --vm myvm --tag code --host-path ~/code --guest-path /mnt/code
```

详细用法见 [教程](docs/TUTORIAL.zh.md)。

## 架构

查看 [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md)。

## 贡献

欢迎提交 Issue / PR。

## License

Apache License 2.0 — 可免费用于个人与商业用途。
