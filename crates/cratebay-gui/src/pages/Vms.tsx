import { useState, useEffect, useRef, useCallback } from "react"
import { invoke } from "@tauri-apps/api/core"
import { I } from "../icons"
import { ErrorInline } from "../components/ErrorDisplay"
import { MiniStats } from "../components/StatsBar"
import type { VmInfoDto, OsImageDto, OsImageDownloadProgressDto, VmStats } from "../types"

function formatBytes(bytes: number): string {
  if (bytes >= 1024 * 1024 * 1024) return `${(bytes / (1024 * 1024 * 1024)).toFixed(1)} GB`
  if (bytes >= 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)} MB`
  if (bytes >= 1024) return `${(bytes / 1024).toFixed(1)} KB`
  return `${bytes} B`
}

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
  pfVmId: string
  setPfVmId: (v: string) => void
  pfHostPort: number | ""
  setPfHostPort: (v: number | "") => void
  pfGuestPort: number | ""
  setPfGuestPort: (v: number | "") => void
  pfProtocol: string
  setPfProtocol: (v: string) => void
  onFetchVms: () => void
  onVmAction: (cmd: string, id: string) => void
  onCreateVm: () => void
  onLoginCmd: (vm: VmInfoDto) => void
  onAddMount: () => void
  onRemoveMount: (vmId: string, tag: string) => void
  // OS image props
  osImages: OsImageDto[]
  selectedOsImage: string
  setSelectedOsImage: (v: string) => void
  downloadingImage: string
  downloadProgress: OsImageDownloadProgressDto | null
  onDownloadOsImage: (imageId: string) => void
  onDeleteOsImage: (imageId: string) => void
  onAddPortForward: () => void
  onRemovePortForward: (vmId: string, hostPort: number) => void
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
  setPfVmId,
  pfHostPort, setPfHostPort,
  pfGuestPort, setPfGuestPort,
  pfProtocol, setPfProtocol,
  onFetchVms, onVmAction, onCreateVm,
  onLoginCmd, onAddMount, onRemoveMount,
  osImages, selectedOsImage, setSelectedOsImage,
  downloadingImage, downloadProgress,
  onDownloadOsImage, onDeleteOsImage,
  onAddPortForward, onRemovePortForward, t,
}: VmsProps) {
  const [showCreateModal, setShowCreateModal] = useState(false)
  const [expandedVmId, setExpandedVmId] = useState<string | null>(null)
  const [activeTab, setActiveTab] = useState<"info" | "ssh" | "mounts" | "console" | "ports">("info")

  // Console modal state
  const [consoleVmId, setConsoleVmId] = useState<string | null>(null)
  const [consoleVmName, setConsoleVmName] = useState("")
  const [consoleData, setConsoleData] = useState("")
  const [autoScroll, setAutoScroll] = useState(true)
  const consoleEndRef = useRef<HTMLDivElement>(null)
  const consoleContainerRef = useRef<HTMLDivElement>(null)

  // Poll console data when modal is open
  useEffect(() => {
    if (!consoleVmId) return
    // Initial full load
    // eslint-disable-next-line react-hooks/set-state-in-effect
    setConsoleData("")
    let currentOffset = 0
    let cancelled = false

    const poll = async () => {
      if (cancelled) return
      try {
        const [data, newOffset] = await invoke<[string, number]>("vm_console", {
          id: consoleVmId,
          offset: currentOffset,
        })
        if (cancelled) return
        if (data.length > 0) {
          setConsoleData(prev => prev + data)
          currentOffset = newOffset
        }
      } catch {
        // ignore
      }
    }

    poll()
    const iv = setInterval(poll, 1500)
    return () => { cancelled = true; clearInterval(iv) }
  }, [consoleVmId])

  // Auto-scroll when new data arrives
  useEffect(() => {
    if (autoScroll && consoleEndRef.current) {
      consoleEndRef.current.scrollIntoView({ behavior: "smooth" })
    }
  }, [consoleData, autoScroll])

  const openConsole = (vm: VmInfoDto) => {
    setConsoleVmId(vm.id)
    setConsoleVmName(vm.name || vm.id)
    setConsoleData("")
    setAutoScroll(true)
  }

  const closeConsole = () => {
    setConsoleVmId(null)
    setConsoleVmName("")
    setConsoleData("")
  }

  const copyConsole = async () => {
    try {
      await navigator.clipboard.writeText(consoleData)
    } catch {
      // ignore
    }
  }

  // VM stats state
  const [vmStatsMap, setVmStatsMap] = useState<Record<string, VmStats>>({})

  const fetchVmStats = useCallback(async () => {
    const runningVms = vms.filter(v => v.state === "running")
    if (runningVms.length === 0) {
      setVmStatsMap({})
      return
    }
    const results: Record<string, VmStats> = {}
    await Promise.allSettled(
      runningVms.map(async (vm) => {
        try {
          const stats = await invoke<VmStats>("vm_stats", { id: vm.id })
          results[vm.id] = stats
        } catch {
          // silently ignore stats errors
        }
      })
    )
    setVmStatsMap(results)
  }, [vms])

  useEffect(() => {
    // eslint-disable-next-line react-hooks/set-state-in-effect
    fetchVmStats()
    const iv = setInterval(fetchVmStats, 5000)
    return () => clearInterval(iv)
  }, [fetchVmStats])

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
          <p>{t("createFirstVm")}</p>
          <button className="btn primary" style={{ marginTop: 16 }} onClick={() => setShowCreateModal(true)}>
            <span className="icon">{I.plus}</span>{t("createVm")}
          </button>
        </div>
      ) : (
        <div className="vm-list">
          {vms.map(vm => {
            const isRunning = vm.state === "running"
            const isExpanded = expandedVmId === vm.id
            const stats = isRunning ? vmStatsMap[vm.id] : undefined
            return (
              <div key={vm.id} className={`vm-item ${isExpanded ? "vm-item-expanded" : ""} ${isRunning ? "vm-item-running" : ""}`}>
                <div className="vm-item-row" onClick={() => setExpandedVmId(isExpanded ? null : vm.id)}>
                  <div className={`vm-icon ${isRunning ? "" : "stopped"}`}>{I.server}</div>
                  <div className="vm-item-info">
                    <div className="vm-item-name">{vm.name}</div>
                    <div className="vm-item-meta">
                      {vm.cpus} vCPU · {vm.memory_mb} MB · {vm.disk_gb} GB
                      {vm.rosetta_enabled && " · Rosetta"}
                      {vm.os_image && ` · ${vm.os_image}`}
                    </div>
                    {isRunning && stats && (
                      <div className="vm-item-stats">
                        <MiniStats items={[
                          { label: t("cpuUsage"), value: stats.cpu_percent, max: 100, suffix: "%" },
                          { label: t("memoryUsage"), value: stats.memory_usage_mb, max: vm.memory_mb, suffix: " MB" },
                          { label: t("diskUsage"), value: stats.disk_usage_gb, max: vm.disk_gb, suffix: " GB" },
                        ]} />
                      </div>
                    )}
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
                    <button className="action-btn" onClick={e => { e.stopPropagation(); openConsole(vm) }} title={t("console")}>{I.fileText}</button>
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
                      <button className={`vm-tab ${activeTab === "console" ? "active" : ""}`} onClick={() => setActiveTab("console")}>{t("console")}</button>
                      <button className={`vm-tab ${activeTab === "ports" ? "active" : ""}`} onClick={() => setActiveTab("ports")}>{t("portForwarding")}</button>
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
                          {vm.os_image && (
                            <div className="vm-stat-card">
                              <div className="vm-stat-label">{t("osImage")}</div>
                              <div className="vm-stat-value">{vm.os_image}</div>
                            </div>
                          )}
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
                        {isRunning && (
                          <div className="hint" style={{ marginBottom: 8, color: "var(--yellow, #f0c040)" }}>
                            {t("virtiofsRestartNotice")}
                          </div>
                        )}
                        {vm.mounts?.length > 0 && (
                          <div className="vm-mount-list">
                            {vm.mounts.map(m => (
                              <div className="vm-mount-item" key={`${vm.id}-${m.tag}`}>
                                <span className="vm-mount-tag">{m.tag}</span>
                                <span className="vm-mount-path">{m.host_path} &rarr; {m.guest_path}</span>
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
                            <input className="input" value={mountHostPath} onChange={e => { setMountVmId(vm.id); setMountHostPath(e.target.value) }} placeholder="/Users/me/code" />
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
                          <div className="hint">{t("virtiofsHint")}</div>
                          <div className="hint" style={{ marginTop: 4, fontFamily: "monospace", fontSize: 11 }}>{t("virtiofsGuestHint")}</div>
                        </div>
                      </div>
                    )}

                    {activeTab === "console" && (
                      <div className="vm-detail-content">
                        <div style={{ marginBottom: 8 }}>
                          <button className="btn sm" onClick={() => openConsole(vm)}>
                            <span className="icon">{I.terminal}</span>{t("vmConsole")}
                          </button>
                        </div>
                        <div className="hint">{t("vmHint")}</div>
                      </div>
                    )}

                    {activeTab === "ports" && (
                      <div className="vm-detail-content">
                        {vm.port_forwards?.length > 0 && (
                          <div className="vm-mount-list">
                            {vm.port_forwards.map(pf => (
                              <div className="vm-mount-item" key={`${vm.id}-pf-${pf.host_port}`}>
                                <span className="vm-mount-tag">{pf.host_port}</span>
                                <span className="vm-mount-path">{pf.host_port} → {pf.guest_port}</span>
                                <span className="vm-mount-mode">{pf.protocol.toUpperCase()}</span>
                                <button className="btn tiny" onClick={() => onRemovePortForward(vm.id, pf.host_port)}>{t("remove")}</button>
                              </div>
                            ))}
                          </div>
                        )}
                        <div className="vm-detail-form" style={{ marginTop: vm.port_forwards?.length > 0 ? 12 : 0 }}>
                          <div className="vm-form-row">
                            <label>{t("hostPort")}</label>
                            <input className="input" type="number" min={1} max={65535} value={pfHostPort} onChange={e => { setPfVmId(vm.id); setPfHostPort(e.target.value === "" ? "" : Number(e.target.value)) }} placeholder="8080" />
                          </div>
                          <div className="vm-form-row">
                            <label>{t("guestPort")}</label>
                            <input className="input" type="number" min={1} max={65535} value={pfGuestPort} onChange={e => { setPfVmId(vm.id); setPfGuestPort(e.target.value === "" ? "" : Number(e.target.value)) }} placeholder="80" />
                          </div>
                          <div className="vm-form-row">
                            <label>{t("protocol")}</label>
                            <select className="input" value={pfProtocol} onChange={e => { setPfVmId(vm.id); setPfProtocol(e.target.value) }}>
                              <option value="tcp">TCP</option>
                              <option value="udp">UDP</option>
                            </select>
                          </div>
                          <button className="btn" onClick={() => { setPfVmId(vm.id); onAddPortForward() }} disabled={pfHostPort === "" || pfGuestPort === ""}>
                            <span className="icon">{I.plus}</span>{t("addPortForward")}
                          </button>
                          <div className="hint">{t("portForwardHint")}</div>
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
          <div className="modal" onClick={e => e.stopPropagation()} style={{ maxWidth: 520 }}>
            <div className="modal-head">
              <div className="modal-title">{t("createVm")}</div>
              <div className="modal-actions">
                <button className="icon-btn" onClick={() => setShowCreateModal(false)} title={t("close")}>&times;</button>
              </div>
            </div>
            <div className="modal-body">
              <div className="form">
                <div className="row">
                  <label>{t("name")}</label>
                  <input className="input" value={vmName} onChange={e => setVmName(e.target.value)} placeholder="myvm" autoFocus />
                </div>

                {/* OS Image selector */}
                <div className="row">
                  <label>{t("osImage")}</label>
                  <select
                    className="input"
                    value={selectedOsImage}
                    onChange={e => setSelectedOsImage(e.target.value)}
                  >
                    <option value="">{t("osImageNone")}</option>
                    {osImages.map(img => (
                      <option key={img.id} value={img.id} disabled={img.status !== "ready"}>
                        {img.name} ({formatBytes(img.size_bytes)})
                        {img.status === "ready" ? ` - ${t("osImageReady")}` : ""}
                        {img.status === "downloading" ? ` - ${t("osImageDownloading")}` : ""}
                        {img.status === "not_downloaded" ? ` - ${t("osImageNotDownloaded")}` : ""}
                      </option>
                    ))}
                  </select>
                </div>

                {/* OS Image list with download/delete actions */}
                <div className="os-image-list" style={{ margin: "8px 0" }}>
                  {osImages.map(img => {
                    const isDownloading = downloadingImage === img.id
                    const progressPct = downloadProgress && downloadProgress.image_id === img.id && downloadProgress.bytes_total > 0
                      ? Math.min(100, (downloadProgress.bytes_downloaded / downloadProgress.bytes_total) * 100)
                      : 0
                    return (
                      <div key={img.id} className="os-image-item" style={{
                        display: "flex", alignItems: "center", gap: 8,
                        padding: "6px 0", borderBottom: "1px solid var(--border, #333)"
                      }}>
                        <div style={{ flex: 1, minWidth: 0 }}>
                          <div style={{ fontWeight: 500, fontSize: 13 }}>{img.name}</div>
                          <div style={{ fontSize: 11, opacity: 0.7 }}>
                            {img.arch} &middot; {t("osImageVersion")} {img.version} &middot; {formatBytes(img.size_bytes)}
                          </div>
                          {isDownloading && (
                            <div style={{ marginTop: 4 }}>
                              <div style={{
                                height: 4, borderRadius: 2,
                                background: "var(--border, #444)", overflow: "hidden"
                              }}>
                                <div style={{
                                  height: "100%", width: `${progressPct}%`,
                                  background: "var(--accent, #4ea6f5)",
                                  transition: "width 0.3s ease"
                                }} />
                              </div>
                              <div style={{ fontSize: 10, opacity: 0.6, marginTop: 2 }}>
                                {t("osImageProgress")}: {progressPct.toFixed(1)}%
                              </div>
                            </div>
                          )}
                        </div>
                        <div style={{ display: "flex", gap: 4, flexShrink: 0 }}>
                          {img.status === "not_downloaded" && (
                            <button
                              className="btn tiny"
                              disabled={!!downloadingImage}
                              onClick={() => onDownloadOsImage(img.id)}
                            >
                              {t("osImageDownload")}
                            </button>
                          )}
                          {img.status === "downloading" && (
                            <span style={{ fontSize: 11, opacity: 0.7 }}>{t("osImageDownloading")}</span>
                          )}
                          {img.status === "ready" && (
                            <>
                              <span style={{ fontSize: 11, color: "var(--green, #4caf50)", fontWeight: 500 }}>
                                {t("osImageReady")}
                              </span>
                              <button
                                className="btn tiny"
                                onClick={() => onDeleteOsImage(img.id)}
                              >
                                {t("osImageDelete")}
                              </button>
                            </>
                          )}
                        </div>
                      </div>
                    )
                  })}
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

      {/* Console Modal */}
      {consoleVmId && (
        <div className="modal-backdrop" onClick={closeConsole}>
          <div className="modal" onClick={e => e.stopPropagation()} style={{ maxWidth: 800, width: "96vw" }}>
            <div className="modal-head">
              <div className="modal-title">{t("vmConsole")} — {consoleVmName}</div>
              <div className="modal-actions">
                <button className="icon-btn" onClick={closeConsole} title={t("close")}>×</button>
              </div>
            </div>
            <div className="console-toolbar">
              <label className="console-auto-scroll">
                <input type="checkbox" checked={autoScroll} onChange={e => setAutoScroll(e.target.checked)} />
                {t("autoScroll")}
              </label>
              <div className="console-toolbar-right">
                <button className="btn xs" onClick={() => { setConsoleData("") }}>
                  {t("clear")}
                </button>
                <button className="btn xs" onClick={copyConsole}>
                  <span className="icon">{I.copy}</span>{t("copy")}
                </button>
              </div>
            </div>
            <div className="console-viewer" ref={consoleContainerRef}>
              {consoleData ? (
                <pre className="console-content">{consoleData}<div ref={consoleEndRef} /></pre>
              ) : (
                <div className="console-empty">{t("noConsoleOutput")}</div>
              )}
            </div>
            <div className="modal-footer">
              <button className="btn" onClick={closeConsole}>{t("close")}</button>
            </div>
          </div>
        </div>
      )}
    </div>
  )
}
