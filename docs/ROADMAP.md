# CargoBay Roadmap

> **English** · [中文](../README.zh.md)

## v0.2.0 — VM Execution

- Virtualization.framework FFI for real VM start/stop
- OS image download and management (file picker for Linux kernel/image)
- VM console (serial output)
- Graceful shutdown (ACPI)

## v0.3.0 — Networking & Monitoring

- Automatic port forwarding (VM <-> host)
- VirtioFS real mount implementation
- VM/container resource monitoring (CPU / memory / disk)
- Container log streaming

## v0.4.0 — Developer Experience

- Container exec / terminal integration
- Local image list management
- Volume management
- Environment variable editor

## v0.5.0 — Kubernetes

- K3s integration
- Kubernetes dashboard

## v1.0.0 — Production Ready

- Linux (KVM) + Windows (Hyper-V) support
- Auto-update + plugin system
