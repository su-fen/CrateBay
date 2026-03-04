import { type JSX, useState, useEffect, useRef, useCallback } from "react"
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

  // Tab config for cleaner rendering
  const tabConfig: { key: typeof activeTab; icon: JSX.Element; label: string }[] = [
    { key: "info", icon: I.cpu, label: t("status") },
    { key: "ssh", icon: I.terminal, label: t("loginCommand") },
    { key: "mounts", icon: I.hardDrive, label: t("virtiofs") },
    { key: "ports", icon: I.globe, label: t("portForwarding") },
    { key: "console", icon: I.fileText, label: t("console") },
  ]

  return (
    <div className="page">
      {/* Toolbar */}
      <div className="toolbar">
        <button type="button" className="btn primary" onClick={() => setShowCreateModal(true)}>
          <span className="icon">{I.plus}</span>{t("createVm")}
        </button>
        <button type="button" className="btn" onClick={onFetchVms} disabled={vmLoading}>
          <span className="icon">{I.refresh}</span>{vmLoading ? t("loading") : t("refresh")}
        </button>
        <div className="toolbar-spacer" />
      </div>

      {vmError && <ErrorInline message={vmError} onDismiss={() => setVmError("")} />}

      {/* VM List */}
      {vms.length === 0 ? (
        <div className="empty-state empty-state-lg">
          <div className="empty-icon">{I.server}</div>
          <h3>{t("noVms")}</h3>
          <p>{t("createFirstVm")}</p>
          <button type="button" className="btn primary" onClick={() => setShowCreateModal(true)}>
            <span className="icon">{I.plus}</span>{t("createVm")}
          </button>
        </div>
      ) : (
        <div className="vm-list">
          {vms.map(vm => {
            const isRunning = vm.state === "running"
            const isExpanded = expandedVmId === vm.id
            const stats = isRunning ? vmStatsMap[vm.id] : undefined
            const isActing = vmActing === vm.id
            return (
              <div key={vm.id} className={`vm-item${isExpanded ? " vm-item-expanded" : ""}${isRunning ? " vm-item-running" : ""}${isActing ? " vm-item-acting" : ""}`}>
                {/* Main row */}
                <div className="vm-item-row" onClick={() => setExpandedVmId(isExpanded ? null : vm.id)}>
                  <div className={`vm-icon${isRunning ? "" : " stopped"}`}>{I.server}</div>
                  <div className="vm-item-info">
                    <div className="vm-item-name">{vm.name}</div>
                    <div className="vm-item-meta vm-item-meta-rich">
                      <span className="vm-meta-spec">
                        <span className="vm-meta-icon">{I.cpu}</span>
                        {vm.cpus} vCPU
                      </span>
                      <span className="vm-meta-sep">&middot;</span>
                      <span className="vm-meta-spec">
                        <span className="vm-meta-icon">{I.memory}</span>
                        {vm.memory_mb} MB
                      </span>
                      <span className="vm-meta-sep">&middot;</span>
                      <span className="vm-meta-spec">
                        <span className="vm-meta-icon">{I.hardDrive}</span>
                        {vm.disk_gb} GB
                      </span>
                      {vm.rosetta_enabled && (
                        <>
                          <span className="vm-meta-sep">&middot;</span>
                          <span className="vm-meta-rosetta">Rosetta</span>
                        </>
                      )}
                      {vm.os_image && (
                        <>
                          <span className="vm-meta-sep">&middot;</span>
                          <span className="muted">{vm.os_image}</span>
                        </>
                      )}
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

                  {/* Status badge */}
                  <div className={`vm-status-pill${isRunning ? " running" : " stopped"}`}>
                    <span className={`dot${isRunning ? " running" : " stopped"}`} />
                    <span className={`vm-status-label${isRunning ? " running" : " stopped"}`}>{vm.state}</span>
                  </div>

                  {/* Actions */}
                  <div className="vm-item-actions">
                    {isRunning ? (
                      <button type="button" className="action-btn action-stop" disabled={isActing}
                        onClick={e => { e.stopPropagation(); onVmAction("vm_stop", vm.id) }}
                        title={t("stop")}>
                        {I.stop}
                      </button>
                    ) : (
                      <button type="button" className="action-btn action-start" disabled={isActing}
                        onClick={e => { e.stopPropagation(); onVmAction("vm_start", vm.id) }}
                        title={t("start")}>
                        {I.play}
                      </button>
                    )}
                    <button type="button" className="action-btn" disabled={isActing}
                      onClick={e => { e.stopPropagation(); onLoginCmd(vm) }}
                      title={t("loginCommand")}>{I.terminal}</button>
                    <button type="button" className="action-btn"
                      onClick={e => { e.stopPropagation(); openConsole(vm) }}
                      title={t("console")}>{I.fileText}</button>
                    <div className="vm-actions-sep" />
                    <button type="button" className="action-btn danger" disabled={isActing}
                      onClick={e => { e.stopPropagation(); onVmAction("vm_delete", vm.id) }}
                      title={t("delete")}>{I.trash}</button>
                  </div>

                  <div className="vm-chevron">{isExpanded ? I.chevronDown : I.chevronRight}</div>
                </div>

                {/* Expanded detail panel */}
                {isExpanded && (
                  <div className="vm-detail vm-detail-animated">
                    {/* Tab navigation with icons */}
                    <div className="vm-detail-tabs vm-detail-tabs-enhanced">
                      {tabConfig.map(tab => (
                        <button type="button" key={tab.key}
                          className={`vm-tab vm-tab-icon${activeTab === tab.key ? " active" : ""}`}
                          onClick={() => setActiveTab(tab.key)}>
                          <span className="vm-tab-icon-svg">{tab.icon}</span>
                          {tab.label}
                        </button>
                      ))}
                    </div>

                    {/* === Info Tab === */}
                    {activeTab === "info" && (
                      <div className="vm-detail-content vm-detail-content-padded">
                        <div className="vm-stats-grid vm-stats-grid-enhanced">
                          <div className="vm-stat-card vm-stat-card-enhanced">
                            <div className="vm-stat-label vm-stat-label-icon">
                              <span className="vm-stat-label-icon-svg">{I.server}</span>
                              ID
                            </div>
                            <div className="vm-stat-value mono">{vm.id}</div>
                          </div>
                          <div className="vm-stat-card vm-stat-card-enhanced">
                            <div className="vm-stat-label vm-stat-label-icon">
                              <span className="vm-stat-label-icon-svg">{I.cpu}</span>
                              {t("cpus")}
                            </div>
                            <div className="vm-stat-value vm-stat-value-lg">{vm.cpus} <span className="vm-stat-value-unit">vCPU</span></div>
                          </div>
                          <div className="vm-stat-card vm-stat-card-enhanced">
                            <div className="vm-stat-label vm-stat-label-icon">
                              <span className="vm-stat-label-icon-svg">{I.memory}</span>
                              {t("memoryMb")}
                            </div>
                            <div className="vm-stat-value vm-stat-value-lg">{vm.memory_mb} <span className="vm-stat-value-unit">MB</span></div>
                          </div>
                          <div className="vm-stat-card vm-stat-card-enhanced">
                            <div className="vm-stat-label vm-stat-label-icon">
                              <span className="vm-stat-label-icon-svg">{I.hardDrive}</span>
                              {t("diskGb")}
                            </div>
                            <div className="vm-stat-value vm-stat-value-lg">{vm.disk_gb} <span className="vm-stat-value-unit">GB</span></div>
                          </div>
                          <div className="vm-stat-card vm-stat-card-enhanced">
                            <div className="vm-stat-label">Rosetta</div>
                            <div className="vm-stat-value">
                              <span className={`vm-inline-pill${vm.rosetta_enabled ? " on" : " off"}`}>
                                <span className={`dot${vm.rosetta_enabled ? " running" : " stopped"}`} />
                                {vm.rosetta_enabled ? "ON" : "OFF"}
                              </span>
                            </div>
                          </div>
                          {vm.os_image && (
                            <div className="vm-stat-card vm-stat-card-enhanced">
                              <div className="vm-stat-label">{t("osImage")}</div>
                              <div className="vm-stat-value">{vm.os_image}</div>
                            </div>
                          )}
                          <div className="vm-stat-card vm-stat-card-enhanced">
                            <div className="vm-stat-label">{t("state")}</div>
                            <div className="vm-stat-value">
                              <span className={`vm-state-pill${isRunning ? " running" : " stopped"}`}>
                                <span className={`dot${isRunning ? " running" : " stopped"}`} />
                                {vm.state}
                              </span>
                            </div>
                          </div>
                        </div>
                      </div>
                    )}

                    {/* === SSH Tab === */}
                    {activeTab === "ssh" && (
                      <div className="vm-detail-content vm-detail-content-padded">
                        <div className="vm-section-card">
                          <div className="vm-section-card-header">
                            <span className="vm-section-card-icon">{I.terminal}</span>
                            <span className="vm-section-card-title">SSH {t("loginCommand")}</span>
                          </div>
                          <div className="vm-detail-form vm-detail-form-spaced">
                            <div className="vm-form-row vm-form-row-spaced">
                              <label htmlFor={`ssh-user-${vm.id}`}>{t("user")}</label>
                              <input id={`ssh-user-${vm.id}`} className="input" value={vmLoginUser} onChange={e => setVmLoginUser(e.target.value)} placeholder="root" />
                            </div>
                            <div className="vm-form-grid-2-narrow">
                              <div className="vm-form-row vm-form-row-spaced">
                                <label htmlFor={`ssh-host-${vm.id}`}>{t("host")}</label>
                                <input id={`ssh-host-${vm.id}`} className="input input-full" value={vmLoginHost} onChange={e => setVmLoginHost(e.target.value)} placeholder="localhost" />
                              </div>
                              <div className="vm-form-row vm-form-row-spaced">
                                <label htmlFor={`ssh-port-${vm.id}`}>{t("port")}</label>
                                <input id={`ssh-port-${vm.id}`} className="input input-full" type="number" min={1} value={vmLoginPort}
                                  onChange={e => setVmLoginPort(e.target.value === "" ? "" : Number(e.target.value))}
                                  placeholder="22" />
                              </div>
                            </div>
                            <div className="vm-form-actions">
                              <button type="button" className="btn primary" onClick={() => onLoginCmd(vm)}>
                                <span className="icon">{I.terminal}</span>{t("loginCommand")}
                              </button>
                            </div>
                            <div className="hint">{t("vmLoginHint")}</div>
                          </div>
                        </div>
                      </div>
                    )}

                    {/* === Mounts Tab === */}
                    {activeTab === "mounts" && (
                      <div className="vm-detail-content vm-detail-content-padded">
                        {isRunning && (
                          <div className="vm-warning-banner">
                            <span className="vm-warning-banner-icon">{I.alertCircle}</span>
                            <span className="vm-warning-banner-text">{t("virtiofsRestartNotice")}</span>
                          </div>
                        )}

                        {/* Existing mounts */}
                        {vm.mounts?.length > 0 && (
                          <div className="vm-list-section">
                            <div className="vm-section-label">
                              {t("virtiofs")} ({vm.mounts.length})
                            </div>
                            <div className="vm-mount-list">
                              {vm.mounts.map(m => (
                                <div className="vm-mount-item vm-mount-item-grid" key={`${vm.id}-${m.tag}`}>
                                  <span className="vm-mount-tag-pill">{m.tag}</span>
                                  <span className="vm-mount-path vm-mount-path-arrow">
                                    <span className="path-text">{m.host_path}</span>
                                    <span className="path-arrow">&rarr;</span>
                                    <span className="path-text">{m.guest_path}</span>
                                  </span>
                                  <span className={`vm-mount-mode-badge${m.read_only ? " ro" : " rw"}`}>{m.read_only ? "RO" : "RW"}</span>
                                  <button type="button" className="btn xs btn-remove" onClick={() => onRemoveMount(vm.id, m.tag)}>
                                    <span className="btn-remove-icon">{I.trash}</span>
                                    {t("remove")}
                                  </button>
                                </div>
                              ))}
                            </div>
                          </div>
                        )}

                        {/* Add mount form */}
                        <div className="vm-section-card wide">
                          <div className="vm-section-card-header">
                            <span className="vm-section-card-icon">{I.plus}</span>
                            <span className="vm-section-card-title">{t("addMount")}</span>
                          </div>
                          <div className="vm-detail-form vm-detail-form-spaced">
                            <div className="vm-form-row vm-form-row-spaced">
                              <label htmlFor={`mount-tag-${vm.id}`}>{t("tag")}</label>
                              <input id={`mount-tag-${vm.id}`} className="input" value={mountTag}
                                onChange={e => { setMountVmId(vm.id); setMountTag(e.target.value) }}
                                placeholder="code" />
                            </div>
                            <div className="vm-form-grid-2">
                              <div className="vm-form-row vm-form-row-spaced">
                                <label htmlFor={`mount-host-${vm.id}`}>{t("hostPath")}</label>
                                <input id={`mount-host-${vm.id}`} className="input input-full" value={mountHostPath}
                                  onChange={e => { setMountVmId(vm.id); setMountHostPath(e.target.value) }}
                                  placeholder="/Users/me/code" />
                              </div>
                              <div className="vm-form-row vm-form-row-spaced">
                                <label htmlFor={`mount-guest-${vm.id}`}>{t("guestPath")}</label>
                                <input id={`mount-guest-${vm.id}`} className="input input-full" value={mountGuestPath}
                                  onChange={e => { setMountVmId(vm.id); setMountGuestPath(e.target.value) }}
                                  placeholder="/mnt/code" />
                              </div>
                            </div>
                            <div className="vm-form-check">
                              <input type="checkbox" id={`mount-ro-${vm.id}`} checked={mountReadonly} onChange={e => setMountReadonly(e.target.checked)} />
                              <label htmlFor={`mount-ro-${vm.id}`}>{t("readOnly")}</label>
                            </div>
                            <div className="vm-form-actions">
                              <button type="button" className="btn primary"
                                onClick={() => { setMountVmId(vm.id); onAddMount() }}
                                disabled={!mountTag.trim() || !mountHostPath.trim()}>
                                <span className="icon">{I.plus}</span>{t("addMount")}
                              </button>
                            </div>
                            <div className="hint">{t("virtiofsHint")}</div>
                            <div className="hint vm-code-hint">{t("virtiofsGuestHint")}</div>
                          </div>
                        </div>
                      </div>
                    )}

                    {/* === Console Tab === */}
                    {activeTab === "console" && (
                      <div className="vm-detail-content vm-detail-content-padded">
                        <div className="vm-console-card">
                          <div className="vm-console-card-icon">
                            <span className="vm-console-card-icon-svg">{I.terminal}</span>
                          </div>
                          <div className="vm-console-card-title">{t("vmConsole")}</div>
                          <button type="button" className="btn primary" onClick={() => openConsole(vm)}>
                            <span className="icon">{I.terminal}</span>{t("vmConsole")}
                          </button>
                        </div>
                      </div>
                    )}

                    {/* === Port Forwarding Tab === */}
                    {activeTab === "ports" && (
                      <div className="vm-detail-content vm-detail-content-padded">
                        {/* Existing port forwards */}
                        {vm.port_forwards?.length > 0 && (
                          <div className="vm-list-section">
                            <div className="vm-section-label">
                              {t("portForwarding")} ({vm.port_forwards.length})
                            </div>
                            <div className="vm-mount-list">
                              {vm.port_forwards.map(pf => (
                                <div className="vm-mount-item vm-mount-item-grid" key={`${vm.id}-pf-${pf.host_port}`}>
                                  <span className="vm-pf-host">
                                    <span className="vm-pf-icon">{I.globe}</span>
                                    :{pf.host_port}
                                  </span>
                                  <span className="vm-pf-mapping">
                                    <span className="pf-arrow">&rarr;</span>
                                    <span className="pf-guest">:{pf.guest_port}</span>
                                  </span>
                                  <span className="vm-pf-protocol">{pf.protocol.toUpperCase()}</span>
                                  <button type="button" className="btn xs btn-remove" onClick={() => onRemovePortForward(vm.id, pf.host_port)}>
                                    <span className="btn-remove-icon">{I.trash}</span>
                                    {t("remove")}
                                  </button>
                                </div>
                              ))}
                            </div>
                          </div>
                        )}

                        {/* Add port forward form */}
                        <div className="vm-section-card">
                          <div className="vm-section-card-header">
                            <span className="vm-section-card-icon">{I.plus}</span>
                            <span className="vm-section-card-title">{t("addPortForward")}</span>
                          </div>
                          <div className="vm-detail-form vm-detail-form-spaced">
                            <div className="vm-form-grid-3">
                              <div className="vm-form-row vm-form-row-spaced">
                                <label htmlFor={`pf-host-${vm.id}`}>{t("hostPort")}</label>
                                <input id={`pf-host-${vm.id}`} className="input input-full" type="number" min={1} max={65535}
                                  value={pfHostPort}
                                  onChange={e => { setPfVmId(vm.id); setPfHostPort(e.target.value === "" ? "" : Number(e.target.value)) }}
                                  placeholder="8080" />
                              </div>
                              <div className="vm-form-row vm-form-row-spaced">
                                <label htmlFor={`pf-guest-${vm.id}`}>{t("guestPort")}</label>
                                <input id={`pf-guest-${vm.id}`} className="input input-full" type="number" min={1} max={65535}
                                  value={pfGuestPort}
                                  onChange={e => { setPfVmId(vm.id); setPfGuestPort(e.target.value === "" ? "" : Number(e.target.value)) }}
                                  placeholder="80" />
                              </div>
                              <div className="vm-form-row vm-form-row-spaced">
                                <label htmlFor={`pf-proto-${vm.id}`}>{t("protocol")}</label>
                                <select id={`pf-proto-${vm.id}`} className="input input-full" title={t("protocol")}
                                  value={pfProtocol}
                                  onChange={e => { setPfVmId(vm.id); setPfProtocol(e.target.value) }}>
                                  <option value="tcp">TCP</option>
                                  <option value="udp">UDP</option>
                                </select>
                              </div>
                            </div>
                            <div className="vm-form-actions">
                              <button type="button" className="btn primary"
                                onClick={() => { setPfVmId(vm.id); onAddPortForward() }}
                                disabled={pfHostPort === "" || pfGuestPort === ""}>
                                <span className="icon">{I.plus}</span>{t("addPortForward")}
                              </button>
                            </div>
                            <div className="hint">{t("portForwardHint")}</div>
                          </div>
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

      {/* ============ Create VM Modal ============ */}
      {showCreateModal && (
        <div className="modal-backdrop" onClick={() => setShowCreateModal(false)}>
          <div className="modal modal-create-vm" onClick={e => e.stopPropagation()}>
            <div className="modal-head modal-head-enhanced">
              <div className="modal-head-left">
                <div className="modal-head-icon purple">
                  <span className="modal-head-icon-svg">{I.server}</span>
                </div>
                <div className="modal-title modal-title-lg">{t("createVm")}</div>
              </div>
              <div className="modal-actions">
                <button type="button" className="icon-btn" onClick={() => setShowCreateModal(false)} title={t("close")}>&times;</button>
              </div>
            </div>
            <div className="modal-body modal-body-padded">
              <div className="form form-spaced">
                {/* VM Name */}
                <div className="row">
                  <label htmlFor="vm-create-name">{t("name")}</label>
                  <input id="vm-create-name" className="input input-lg" value={vmName} onChange={e => setVmName(e.target.value)}
                    placeholder="myvm" autoFocus />
                </div>

                {/* OS Image selector */}
                <div className="row">
                  <label htmlFor="vm-create-os">{t("osImage")}</label>
                  <select id="vm-create-os" className="input input-lg" title={t("osImage")} value={selectedOsImage}
                    onChange={e => setSelectedOsImage(e.target.value)}>
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
                {osImages.length > 0 && (
                  <div className="os-image-list-container">
                    <div className="os-image-list-header">Available Images</div>
                    {osImages.map(img => {
                      const isDownloading = downloadingImage === img.id
                      const progressPct = downloadProgress && downloadProgress.image_id === img.id && downloadProgress.bytes_total > 0
                        ? Math.min(100, (downloadProgress.bytes_downloaded / downloadProgress.bytes_total) * 100)
                        : 0
                      return (
                        <div key={img.id} className="os-image-list-item">
                          {/* Icon */}
                          <div className={`os-image-list-icon${img.status === "ready" ? " ready" : ""}`}>
                            <span className="os-image-list-icon-svg">{I.hardDrive}</span>
                          </div>

                          {/* Info */}
                          <div className="os-image-info">
                            <div className="os-image-name">{img.name}</div>
                            <div className="os-image-meta">
                              <span>{img.arch}</span>
                              <span className="vm-meta-sep">&middot;</span>
                              <span>v{img.version}</span>
                              <span className="vm-meta-sep">&middot;</span>
                              <span>{formatBytes(img.size_bytes)}</span>
                            </div>
                            {isDownloading && (
                              <div className="os-image-progress">
                                <div className="os-image-progress-track">
                                  <div className="os-image-progress-fill" style={{ width: `${progressPct}%` }} />
                                </div>
                                <div className="os-image-progress-text">
                                  <span>{t("osImageProgress")}</span>
                                  <span className="os-image-progress-pct">{progressPct.toFixed(1)}%</span>
                                </div>
                              </div>
                            )}
                          </div>

                          {/* Status / Actions */}
                          <div className="os-image-actions">
                            {img.status === "not_downloaded" && (
                              <button type="button" className="btn xs btn-download"
                                disabled={!!downloadingImage}
                                onClick={() => onDownloadOsImage(img.id)}>
                                {t("osImageDownload")}
                              </button>
                            )}
                            {img.status === "downloading" && (
                              <span className="os-image-downloading">
                                <span className="spinner" />
                                {t("osImageDownloading")}
                              </span>
                            )}
                            {img.status === "ready" && (
                              <>
                                <span className="os-image-ready-badge">
                                  <span className="dot running" />
                                  {t("osImageReady")}
                                </span>
                                <button type="button" className="btn xs" onClick={() => onDeleteOsImage(img.id)}>
                                  <span className="btn-remove-icon">{I.trash}</span>
                                </button>
                              </>
                            )}
                          </div>
                        </div>
                      )
                    })}
                  </div>
                )}

                {/* Hardware specs */}
                <div className="form-section-divider">Hardware Configuration</div>
                <div className="row two">
                  <div>
                    <label htmlFor="vm-create-cpus" className="form-label-icon">
                      <span className="vm-stat-label-icon-svg">{I.cpu}</span>
                      {t("cpus")}
                    </label>
                    <input id="vm-create-cpus" className="input input-lg" type="number" min={1} value={vmCpus}
                      onChange={e => setVmCpus(Number(e.target.value) || 2)} />
                  </div>
                  <div>
                    <label htmlFor="vm-create-mem" className="form-label-icon">
                      <span className="vm-stat-label-icon-svg">{I.memory}</span>
                      {t("memoryMb")}
                    </label>
                    <input id="vm-create-mem" className="input input-lg" type="number" min={256} value={vmMem}
                      onChange={e => setVmMem(Number(e.target.value) || 2048)} />
                  </div>
                </div>
                <div className="row">
                  <label htmlFor="vm-create-disk" className="form-label-icon">
                    <span className="vm-stat-label-icon-svg">{I.hardDrive}</span>
                    {t("diskGb")}
                  </label>
                  <input id="vm-create-disk" className="input input-lg" type="number" min={10} value={vmDisk}
                    onChange={e => setVmDisk(Number(e.target.value) || 20)} />
                </div>
                <div className="row inline">
                  <input type="checkbox" id="vm-create-rosetta" checked={vmRosetta} onChange={e => setVmRosetta(e.target.checked)} />
                  <label htmlFor="vm-create-rosetta">{t("enableRosetta")}</label>
                </div>
              </div>
            </div>
            <div className="modal-footer modal-footer-padded">
              <button type="button" className="btn" onClick={() => setShowCreateModal(false)}>{t("close")}</button>
              <button type="button" className="btn primary"
                disabled={vmActing === "create" || !vmName.trim()}
                onClick={handleCreate}>
                {vmActing === "create" ? (
                  <><span className="spinner btn-spinner" />{t("creating")}</>
                ) : (
                  <><span className="icon">{I.plus}</span>{t("create")}</>
                )}
              </button>
            </div>
          </div>
        </div>
      )}

      {/* ============ Console Modal ============ */}
      {consoleVmId && (
        <div className="modal-backdrop" onClick={closeConsole}>
          <div className="modal modal-console" onClick={e => e.stopPropagation()}>
            <div className="modal-head modal-head-enhanced">
              <div className="modal-head-left">
                <div className="modal-head-icon green">
                  <span className="modal-head-icon-svg">{I.terminal}</span>
                </div>
                <div className="modal-title modal-title-lg">
                  {t("vmConsole")}
                  <span className="modal-title-sub">— {consoleVmName}</span>
                </div>
              </div>
              <div className="modal-actions">
                <button type="button" className="icon-btn" onClick={closeConsole} title={t("close")}>×</button>
              </div>
            </div>
            <div className="console-toolbar">
              <label className="console-auto-scroll" htmlFor="console-auto-scroll-cb">
                <input type="checkbox" id="console-auto-scroll-cb" checked={autoScroll} onChange={e => setAutoScroll(e.target.checked)} />
                {t("autoScroll")}
              </label>
              <div className="console-toolbar-right">
                <button type="button" className="btn xs" onClick={() => { setConsoleData("") }}>
                  {t("clear")}
                </button>
                <button type="button" className="btn xs" onClick={copyConsole}>
                  <span className="icon">{I.copy}</span>{t("copy")}
                </button>
              </div>
            </div>
            <div className="console-viewer-padded">
              <div className="console-viewer console-viewer-lg" ref={consoleContainerRef}>
                {consoleData ? (
                  <pre className="console-content">{consoleData}<div ref={consoleEndRef} /></pre>
                ) : (
                  <div className="console-empty console-empty-enhanced">
                    <span className="console-empty-icon">{I.terminal}</span>
                    <span>{t("noConsoleOutput")}</span>
                  </div>
                )}
              </div>
            </div>
            <div className="modal-footer modal-footer-padded">
              <button type="button" className="btn" onClick={closeConsole}>{t("close")}</button>
            </div>
          </div>
        </div>
      )}
    </div>
  )
}
