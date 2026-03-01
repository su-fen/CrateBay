import { useState } from "react"
import { I } from "../icons"
import { ErrorInline } from "../components/ErrorDisplay"
import type { VmInfoDto } from "../types"

interface VmsProps {
  vms: VmInfoDto[]
  vmLoading: boolean
  vmError: string
  setVmError: (v: string) => void
  vmName: string
  setVmName: (v: string) => void
  vmCpus: number
  setVmCpus: (v: number) => void
  vmMem: number
  setVmMem: (v: number) => void
  vmDisk: number
  setVmDisk: (v: number) => void
  vmRosetta: boolean
  setVmRosetta: (v: boolean) => void
  vmActing: string
  vmLoginUser: string
  setVmLoginUser: (v: string) => void
  vmLoginHost: string
  setVmLoginHost: (v: string) => void
  vmLoginPort: number | ""
  setVmLoginPort: (v: number | "") => void
  mountVmId: string
  setMountVmId: (v: string) => void
  mountTag: string
  setMountTag: (v: string) => void
  mountHostPath: string
  setMountHostPath: (v: string) => void
  mountGuestPath: string
  setMountGuestPath: (v: string) => void
  mountReadonly: boolean
  setMountReadonly: (v: boolean) => void
  onFetchVms: () => void
  onVmAction: (cmd: string, id: string) => void
  onCreateVm: () => void
  onLoginCmd: (vm: VmInfoDto) => void
  onAddMount: () => void
  onRemoveMount: (vmId: string, tag: string) => void
  t: (key: string) => string
}

export function Vms({
  vms, vmLoading, vmError, setVmError,
  vmName, setVmName, vmCpus, setVmCpus,
  vmMem, setVmMem, vmDisk, setVmDisk,
  vmRosetta, setVmRosetta, vmActing,
  vmLoginUser, setVmLoginUser,
  vmLoginHost, setVmLoginHost,
  vmLoginPort, setVmLoginPort,
  setMountVmId,
  mountTag, setMountTag,
  mountHostPath, setMountHostPath,
  mountGuestPath, setMountGuestPath,
  mountReadonly, setMountReadonly,
  onFetchVms, onVmAction, onCreateVm,
  onLoginCmd, onAddMount, onRemoveMount, t,
}: VmsProps) {
  const [showCreateModal, setShowCreateModal] = useState(false)
  const [expandedVmId, setExpandedVmId] = useState<string | null>(null)
  const [activeTab, setActiveTab] = useState<"info" | "ssh" | "mounts">("info")

  const handleCreate = () => {
    onCreateVm()
    setShowCreateModal(false)
  }

  return (
    <div className="page">
      {/* Toolbar */}
      <div className="toolbar">
        <button className="btn primary" onClick={() => setShowCreateModal(true)}>
          <span className="icon">{I.plus}</span>{t("createVm")}
        </button>
        <button className="btn" onClick={onFetchVms} disabled={vmLoading}>
          <span className="icon">{I.refresh}</span>{vmLoading ? t("loading") : t("refresh")}
        </button>
        <div style={{ flex: 1 }} />
        <div className="hint">{t("vmHint")}</div>
      </div>

      {vmError && <ErrorInline message={vmError} onDismiss={() => setVmError("")} />}

      {/* VM List */}
      {vms.length === 0 ? (
        <div className="empty-state">
          <div className="empty-icon">{I.server}</div>
          <h3>{t("noVms")}</h3>
          <p>{t("vmHint")}</p>
          <button className="btn primary" style={{ marginTop: 16 }} onClick={() => setShowCreateModal(true)}>
            <span className="icon">{I.plus}</span>{t("createVm")}
          </button>
        </div>
      ) : (
        <div className="vm-list">
          {vms.map(vm => {
            const isRunning = vm.state === "running"
            const isExpanded = expandedVmId === vm.id
            return (
              <div key={vm.id} className={`vm-item ${isExpanded ? "vm-item-expanded" : ""} ${isRunning ? "vm-item-running" : ""}`}>
                <div className="vm-item-row" onClick={() => setExpandedVmId(isExpanded ? null : vm.id)}>
                  <div className={`vm-icon ${isRunning ? "" : "stopped"}`}>{I.server}</div>
                  <div className="vm-item-info">
                    <div className="vm-item-name">{vm.name}</div>
                    <div className="vm-item-meta">
                      {vm.cpus} vCPU · {vm.memory_mb} MB · {vm.disk_gb} GB
                      {vm.rosetta_enabled && " · Rosetta"}
                    </div>
                  </div>
                  <div className="vm-item-status">
                    <span className={`dot ${isRunning ? "running" : "stopped"}`} />
                    <span>{vm.state}</span>
                  </div>
                  <div className="vm-item-actions">
                    {isRunning ? (
                      <button className="action-btn" disabled={vmActing === vm.id} onClick={e => { e.stopPropagation(); onVmAction("vm_stop", vm.id) }} title={t("stop")}>{I.stop}</button>
                    ) : (
                      <button className="action-btn" disabled={vmActing === vm.id} onClick={e => { e.stopPropagation(); onVmAction("vm_start", vm.id) }} title={t("start")}>{I.play}</button>
                    )}
                    <button className="action-btn" disabled={vmActing === vm.id} onClick={e => { e.stopPropagation(); onLoginCmd(vm) }} title={t("loginCommand")}>{I.terminal}</button>
                    <button className="action-btn danger" disabled={vmActing === vm.id} onClick={e => { e.stopPropagation(); onVmAction("vm_delete", vm.id) }} title={t("delete")}>{I.trash}</button>
                  </div>
                  <div className="vm-chevron">{isExpanded ? I.chevronDown : I.chevronRight}</div>
                </div>

                {/* Expanded detail panel */}
                {isExpanded && (
                  <div className="vm-detail">
                    <div className="vm-detail-tabs">
                      <button className={`vm-tab ${activeTab === "info" ? "active" : ""}`} onClick={() => setActiveTab("info")}>{t("status")}</button>
                      <button className={`vm-tab ${activeTab === "ssh" ? "active" : ""}`} onClick={() => setActiveTab("ssh")}>{t("loginCommand")}</button>
                      <button className={`vm-tab ${activeTab === "mounts" ? "active" : ""}`} onClick={() => setActiveTab("mounts")}>{t("virtiofs")}</button>
                    </div>

                    {activeTab === "info" && (
                      <div className="vm-detail-content">
                        <div className="vm-stats-grid">
                          <div className="vm-stat-card">
                            <div className="vm-stat-label">ID</div>
                            <div className="vm-stat-value mono">{vm.id}</div>
                          </div>
                          <div className="vm-stat-card">
                            <div className="vm-stat-label">{t("cpus")}</div>
                            <div className="vm-stat-value">{vm.cpus} vCPU</div>
                          </div>
                          <div className="vm-stat-card">
                            <div className="vm-stat-label">{t("memoryMb")}</div>
                            <div className="vm-stat-value">{vm.memory_mb} MB</div>
                          </div>
                          <div className="vm-stat-card">
                            <div className="vm-stat-label">{t("diskGb")}</div>
                            <div className="vm-stat-value">{vm.disk_gb} GB</div>
                          </div>
                          <div className="vm-stat-card">
                            <div className="vm-stat-label">Rosetta</div>
                            <div className="vm-stat-value">{vm.rosetta_enabled ? "ON" : "OFF"}</div>
                          </div>
                          <div className="vm-stat-card">
                            <div className="vm-stat-label">{t("state")}</div>
                            <div className="vm-stat-value">
                              <span className={`dot ${isRunning ? "running" : "stopped"}`} style={{ display: "inline-block", marginRight: 6 }} />
                              {vm.state}
                            </div>
                          </div>
                        </div>
                      </div>
                    )}

                    {activeTab === "ssh" && (
                      <div className="vm-detail-content">
                        <div className="vm-detail-form">
                          <div className="vm-form-row">
                            <label>{t("user")}</label>
                            <input className="input" value={vmLoginUser} onChange={e => setVmLoginUser(e.target.value)} />
                          </div>
                          <div className="vm-form-row">
                            <label>{t("host")}</label>
                            <input className="input" value={vmLoginHost} onChange={e => setVmLoginHost(e.target.value)} />
                          </div>
                          <div className="vm-form-row">
                            <label>{t("port")}</label>
                            <input className="input" type="number" min={1} value={vmLoginPort} onChange={e => setVmLoginPort(e.target.value === "" ? "" : Number(e.target.value))} />
                          </div>
                          <button className="btn primary" onClick={() => onLoginCmd(vm)}>
                            <span className="icon">{I.terminal}</span>{t("loginCommand")}
                          </button>
                          <div className="hint">{t("vmLoginHint")}</div>
                        </div>
                      </div>
                    )}

                    {activeTab === "mounts" && (
                      <div className="vm-detail-content">
                        {vm.mounts?.length > 0 && (
                          <div className="vm-mount-list">
                            {vm.mounts.map(m => (
                              <div className="vm-mount-item" key={`${vm.id}-${m.tag}`}>
                                <span className="vm-mount-tag">{m.tag}</span>
                                <span className="vm-mount-path">{m.host_path} → {m.guest_path}</span>
                                <span className="vm-mount-mode">{m.read_only ? "RO" : "RW"}</span>
                                <button className="btn tiny" onClick={() => onRemoveMount(vm.id, m.tag)}>{t("remove")}</button>
                              </div>
                            ))}
                          </div>
                        )}
                        <div className="vm-detail-form" style={{ marginTop: vm.mounts?.length > 0 ? 12 : 0 }}>
                          <div className="vm-form-row">
                            <label>{t("tag")}</label>
                            <input className="input" value={mountTag} onChange={e => { setMountVmId(vm.id); setMountTag(e.target.value) }} placeholder="code" />
                          </div>
                          <div className="vm-form-row">
                            <label>{t("hostPath")}</label>
                            <input className="input" value={mountHostPath} onChange={e => { setMountVmId(vm.id); setMountHostPath(e.target.value) }} placeholder="~/code" />
                          </div>
                          <div className="vm-form-row">
                            <label>{t("guestPath")}</label>
                            <input className="input" value={mountGuestPath} onChange={e => { setMountVmId(vm.id); setMountGuestPath(e.target.value) }} placeholder="/mnt/code" />
                          </div>
                          <div className="vm-form-check">
                            <input type="checkbox" checked={mountReadonly} onChange={e => setMountReadonly(e.target.checked)} />
                            <span>{t("readOnly")}</span>
                          </div>
                          <button className="btn" onClick={() => { setMountVmId(vm.id); onAddMount() }} disabled={!mountTag.trim() || !mountHostPath.trim()}>
                            <span className="icon">{I.plus}</span>{t("addMount")}
                          </button>
                        </div>
                      </div>
                    )}
                  </div>
                )}
              </div>
            )
          })}
        </div>
      )}

      {/* Create VM Modal */}
      {showCreateModal && (
        <div className="modal-backdrop" onClick={() => setShowCreateModal(false)}>
          <div className="modal" onClick={e => e.stopPropagation()} style={{ maxWidth: 480 }}>
            <div className="modal-head">
              <div className="modal-title">{t("createVm")}</div>
              <div className="modal-actions">
                <button className="icon-btn" onClick={() => setShowCreateModal(false)} title={t("close")}>×</button>
              </div>
            </div>
            <div className="modal-body">
              <div className="form">
                <div className="row">
                  <label>{t("name")}</label>
                  <input className="input" value={vmName} onChange={e => setVmName(e.target.value)} placeholder="myvm" autoFocus />
                </div>
                <div className="row two">
                  <div>
                    <label>{t("cpus")}</label>
                    <input className="input" type="number" min={1} value={vmCpus} onChange={e => setVmCpus(Number(e.target.value) || 2)} />
                  </div>
                  <div>
                    <label>{t("memoryMb")}</label>
                    <input className="input" type="number" min={256} value={vmMem} onChange={e => setVmMem(Number(e.target.value) || 2048)} />
                  </div>
                </div>
                <div className="row">
                  <label>{t("diskGb")}</label>
                  <input className="input" type="number" min={10} value={vmDisk} onChange={e => setVmDisk(Number(e.target.value) || 20)} />
                </div>
                <div className="row inline">
                  <input type="checkbox" checked={vmRosetta} onChange={e => setVmRosetta(e.target.checked)} />
                  <span>{t("enableRosetta")}</span>
                </div>
              </div>
            </div>
            <div className="modal-footer">
              <button className="btn" onClick={() => setShowCreateModal(false)}>{t("close")}</button>
              <button className="btn primary" disabled={vmActing === "create" || !vmName.trim()} onClick={handleCreate} style={{ marginLeft: 8 }}>
                {vmActing === "create" ? t("creating") : t("create")}
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  )
}
