import { useState, useRef, useEffect, useCallback } from "react"
import { invoke } from "@tauri-apps/api/core"
import { listen, type UnlistenFn } from "@tauri-apps/api/event"
import { I } from "../icons"
import { ErrorBanner } from "../components/ErrorDisplay"
import { EmptyState } from "../components/EmptyState"
import type { ContainerInfo, ContainerGroup, RunContainerResult, ContainerStats, EnvVar } from "../types"

interface ExecEntry {
  command: string
  output: string
  isError: boolean
}

interface ContainersProps {
  containers: ContainerInfo[]
  groups: ContainerGroup[]
  loading: boolean
  error: string
  acting: string
  expandedGroups: Record<string, boolean>
  onContainerAction: (cmd: string, id: string) => void
  onToggleGroup: (key: string) => void
  onOpenTextModal: (title: string, body: string, copyText?: string) => void
  onOpenPackageModal: (container: string, defaultTag: string) => void
  onFetch: () => void
  onRun: (image: string, name: string, cpus: number | "", mem: number | "", pull: boolean, env?: string[]) => Promise<RunContainerResult | null>
  t: (key: string) => string
}

export function Containers({
  groups, loading, error, acting, expandedGroups,
  onContainerAction, onToggleGroup,
  onOpenTextModal, onOpenPackageModal, onFetch, onRun, t,
}: ContainersProps) {
  const [showRunModal, setShowRunModal] = useState(false)
  const [runImage, setRunImage] = useState("")
  const [runName, setRunName] = useState("")
  const [runCpus, setRunCpus] = useState<number | "">("")
  const [runMem, setRunMem] = useState<number | "">("")
  const [runPull, setRunPull] = useState(true)
  const [runLoading, setRunLoading] = useState(false)
  const [runResult, setRunResult] = useState<RunContainerResult | null>(null)
  const [runError, setRunError] = useState("")
  const [runEnvVars, setRunEnvVars] = useState<{ key: string; value: string }[]>([])

  // Log viewer state
  const [showLogModal, setShowLogModal] = useState(false)
  const [logContainerId, setLogContainerId] = useState("")
  const [logContainerName, setLogContainerName] = useState("")
  const [logContent, setLogContent] = useState("")
  const [logLoading, setLogLoading] = useState(false)
  const [logError, setLogError] = useState("")
  const [logTail, setLogTail] = useState("200")
  const [logTimestamps, setLogTimestamps] = useState(false)
  const logEndRef = useRef<HTMLDivElement>(null)

  // Log streaming (follow) state
  const [logFollowing, setLogFollowing] = useState(false)
  const [logStreamEnded, setLogStreamEnded] = useState(false)
  const logUnlistenRef = useRef<UnlistenFn | null>(null)
  const logEndUnlistenRef = useRef<UnlistenFn | null>(null)

  // Exec terminal state
  const [execContainer, setExecContainer] = useState<ContainerInfo | null>(null)
  const [execCmd, setExecCmd] = useState("")
  const [execHistory, setExecHistory] = useState<ExecEntry[]>([])
  const [execRunning, setExecRunning] = useState(false)
  const [execInteractiveCmd, setExecInteractiveCmd] = useState("")
  const execOutputRef = useRef<HTMLDivElement>(null)

  // Container stats state
  const [containerStats, setContainerStats] = useState<Record<string, ContainerStats>>({})

  // Confirm remove state
  const [confirmRemove, setConfirmRemove] = useState("")
  const [containerToRemoveName, setContainerToRemoveName] = useState("")

  // Env viewer state
  const [showEnvModal, setShowEnvModal] = useState(false)
  const [envContainerName, setEnvContainerName] = useState("")
  const [envVars, setEnvVars] = useState<EnvVar[]>([])
  const [envLoading, setEnvLoading] = useState(false)
  const [envError, setEnvError] = useState("")

  const fetchStatsForRunning = useCallback(async () => {
    // Collect running container IDs from all groups
    const runningIds: string[] = []
    for (const g of groups) {
      for (const c of g.containers) {
        if (c.state === "running") {
          runningIds.push(c.id)
        }
      }
    }
    if (runningIds.length === 0) {
      setContainerStats({})
      return
    }
    const results: Record<string, ContainerStats> = {}
    await Promise.allSettled(
      runningIds.map(async (id) => {
        try {
          const stats = await invoke<ContainerStats>("container_stats", { id })
          results[id] = stats
        } catch {
          // silently ignore stats errors for individual containers
        }
      })
    )
    setContainerStats(results)
  }, [groups])

  useEffect(() => {
    fetchStatsForRunning()
    const iv = setInterval(fetchStatsForRunning, 5000)
    return () => clearInterval(iv)
  }, [fetchStatsForRunning])

  useEffect(() => {
    if (execOutputRef.current) {
      execOutputRef.current.scrollTop = execOutputRef.current.scrollHeight
    }
  }, [execHistory])

  const openExecModal = async (c: ContainerInfo) => {
    setExecContainer(c)
    setExecCmd("")
    setExecHistory([])
    setExecRunning(false)
    try {
      const target = c.name || c.id
      const cmd = await invoke<string>("container_exec_interactive_cmd", { containerId: target })
      setExecInteractiveCmd(cmd)
    } catch {
      setExecInteractiveCmd(`docker exec -it ${c.name || c.id} /bin/sh`)
    }
  }

  const handleExec = async () => {
    if (!execContainer || !execCmd.trim() || execRunning) return
    const command = execCmd.trim()
    setExecCmd("")
    setExecRunning(true)
    try {
      const target = execContainer.name || execContainer.id
      const output = await invoke<string>("container_exec", { containerId: target, command })
      setExecHistory(prev => [...prev, { command, output, isError: false }])
    } catch (e) {
      setExecHistory(prev => [...prev, { command, output: String(e), isError: true }])
    } finally {
      setExecRunning(false)
    }
  }

  const handleRun = async () => {
    if (!runImage.trim()) return
    setRunLoading(true)
    setRunError("")
    try {
      const envStrings = runEnvVars
        .filter(e => e.key.trim())
        .map(e => `${e.key}=${e.value}`)
      const result = await onRun(runImage, runName, runCpus, runMem, runPull, envStrings.length > 0 ? envStrings : undefined)
      if (result) setRunResult(result)
    } catch (e) {
      setRunError(String(e))
    } finally {
      setRunLoading(false)
    }
  }

  const openRunModal = () => {
    setRunImage("")
    setRunName("")
    setRunCpus("")
    setRunMem("")
    setRunPull(true)
    setRunResult(null)
    setRunError("")
    setRunEnvVars([])
    setShowRunModal(true)
  }

  const openEnvModal = async (c: ContainerInfo) => {
    setEnvContainerName(c.name || c.id)
    setEnvVars([])
    setEnvError("")
    setEnvLoading(true)
    setShowEnvModal(true)
    try {
      const target = c.name || c.id
      const vars = await invoke<EnvVar[]>("container_env", { id: target })
      setEnvVars(vars)
    } catch (e) {
      setEnvError(String(e))
    } finally {
      setEnvLoading(false)
    }
  }

  const fetchLogs = async (containerId: string, tail: string, timestamps: boolean) => {
    setLogLoading(true)
    setLogError("")
    try {
      const tailParam = tail === "all" ? "all" : tail
      const result = await invoke<string>("container_logs", {
        id: containerId,
        tail: tailParam,
        timestamps,
      })
      setLogContent(result)
    } catch (e) {
      setLogError(String(e))
    } finally {
      setLogLoading(false)
    }
  }

  const stopLogStream = async (containerId?: string) => {
    if (logUnlistenRef.current) {
      logUnlistenRef.current()
      logUnlistenRef.current = null
    }
    if (logEndUnlistenRef.current) {
      logEndUnlistenRef.current()
      logEndUnlistenRef.current = null
    }
    const id = containerId || logContainerId
    if (id) {
      try {
        await invoke("container_logs_stream_stop", { id })
      } catch {
        // ignore stop errors
      }
    }
    setLogFollowing(false)
  }

  const startLogStream = async (containerId: string, timestamps: boolean) => {
    await stopLogStream(containerId)
    setLogStreamEnded(false)
    setLogFollowing(true)

    const unlistenLog = await listen<{ container_id: string; data: string }>("container-log", (event) => {
      if (event.payload.container_id === containerId) {
        setLogContent(prev => prev + event.payload.data)
      }
    })
    logUnlistenRef.current = unlistenLog

    const unlistenEnd = await listen<string>("container-log-end", (event) => {
      if (event.payload === containerId) {
        setLogStreamEnded(true)
        setLogFollowing(false)
      }
    })
    logEndUnlistenRef.current = unlistenEnd

    try {
      await invoke("container_logs_stream", { id: containerId, timestamps })
    } catch (e) {
      setLogError(String(e))
      setLogFollowing(false)
    }
  }

  const openLogModal = (c: ContainerInfo) => {
    setLogContainerId(c.id)
    setLogContainerName(c.name || c.id)
    setLogContent("")
    setLogError("")
    setLogTail("200")
    setLogTimestamps(false)
    setLogFollowing(false)
    setLogStreamEnded(false)
    setShowLogModal(true)
    fetchLogs(c.id, "200", false)
  }

  useEffect(() => {
    if (showLogModal && logEndRef.current) {
      logEndRef.current.scrollIntoView({ behavior: "smooth" })
    }
  }, [logContent, showLogModal])

  // Cleanup log stream when modal closes or component unmounts
  useEffect(() => {
    if (!showLogModal) {
      stopLogStream()
    }
    return () => {
      stopLogStream()
    }
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [showLogModal])

  if (loading) {
    return <div className="loading"><div className="spinner" />{t("loadingContainers")}</div>
  }
  if (error) {
    return (
      <ErrorBanner
        title={t("connectionError")}
        message={error}
        actionLabel={t("refresh")}
        onAction={onFetch}
      />
    )
  }

  const renderCard = (c: ContainerInfo, opts?: { child?: boolean }) => {
    const isRunning = c.state === "running"
    const name = c.name || c.id
    const meta = isRunning ? (c.ports || c.id) : c.id
    const childClass = opts?.child ? " container-child" : ""
    const stats = isRunning ? containerStats[c.id] : undefined
    return (
      <div className={`container-card${childClass}${isRunning ? "" : " stopped"}`} key={c.id}>
        <div className="card-main">
          <div className={`card-icon${isRunning ? "" : " stopped"}`}>{I.box}</div>
          <div className="card-body">
            <div className="card-name">{name}</div>
            <div className="card-meta">{c.image} · {meta}</div>
          </div>
          {isRunning && stats && (
            <div className="card-stats">
              <div className="stat-item">
                <span className="stat-icon">{I.cpu}</span>
                <span className="stat-value">{stats.cpu_percent.toFixed(1)}%</span>
              </div>
              <div className="stat-item">
                <span className="stat-icon">{I.memory}</span>
                <span className="stat-value">{stats.memory_usage_mb.toFixed(0)} MB</span>
              </div>
            </div>
          )}
          <div className="card-status">
            <span className={`dot ${isRunning ? "running" : "stopped"}`} />
            <span>{c.status}</span>
          </div>
        </div>
        <div className="card-actions">
          <div className="card-actions-group">
            <button
              className="action-btn"
              disabled={acting === c.id}
              onClick={async (e) => {
                e.stopPropagation()
                const target = c.name || c.id
                const cmd = await invoke<string>("container_login_cmd", { container: target, shell: "/bin/sh" })
                onOpenTextModal(t("loginCommand"), cmd, cmd)
              }}
              title={t("loginCommand")}
            >
              {I.terminal}<span className="action-label">{t("loginCommand")}</span>
            </button>
            <button
              className="action-btn"
              disabled={acting === c.id}
              onClick={(e) => { e.stopPropagation(); openLogModal(c) }}
              title={t("viewLogs")}
            >
              {I.fileText}<span className="action-label">{t("logs")}</span>
            </button>
            <button
              className="action-btn"
              disabled={acting === c.id}
              onClick={(e) => { e.stopPropagation(); openEnvModal(c) }}
              title={t("viewEnvVars")}
            >
              {I.settings}<span className="action-label">{t("envVars")}</span>
            </button>
            {isRunning && (
              <button
                className="action-btn"
                disabled={acting === c.id}
                onClick={(e) => { e.stopPropagation(); openExecModal(c) }}
                title={t("execCommand")}
              >
                {I.command}<span className="action-label">{t("execCommand")}</span>
              </button>
            )}
            <button
              className="action-btn"
              disabled={acting === c.id}
              onClick={(e) => {
                e.stopPropagation()
                const target = c.name || c.id
                const defaultTag = `${(c.image || "image").split(":")[0]}-snapshot:latest`
                onOpenPackageModal(target, defaultTag)
              }}
              title={t("packageImage")}
            >
              {I.layers}<span className="action-label">{t("package")}</span>
            </button>
          </div>
          <div className="card-actions-sep" />
          <div className="card-actions-group">
            {isRunning ? (
              <button className="action-btn warn" disabled={acting === c.id} onClick={(e) => { e.stopPropagation(); onContainerAction("stop_container", c.id) }} title={t("stop")}>{I.stop}<span className="action-label">{t("stop")}</span></button>
            ) : (
              <button className="action-btn success" disabled={acting === c.id} onClick={(e) => { e.stopPropagation(); onContainerAction("start_container", c.id) }} title={t("start")}>{I.play}<span className="action-label">{t("start")}</span></button>
            )}
            <button className="action-btn danger" disabled={acting === c.id} onClick={(e) => { e.stopPropagation(); setConfirmRemove(c.id); setContainerToRemoveName(c.name || c.id) }} title={t("delete")}>{I.trash}</button>
          </div>
        </div>
      </div>
    )
  }

  return (
    <div className="page">
      <div className="toolbar">
        <div style={{ flex: 1 }} />
        <button className="btn primary" onClick={openRunModal}>
          <span className="icon">{I.plus}</span>{t("runNewContainer")}
        </button>
      </div>

      {groups.length === 0 ? (
        <EmptyState
          icon={I.box}
          title={t("noContainers")}
          description={t("runContainerTip")}
          code="docker run -it -p 80:80 docker/getting-started"
        />
      ) : (
        groups.map(g => {
          if (g.containers.length <= 1) {
            return renderCard(g.containers[0])
          }

          const expanded = !!expandedGroups[g.key]
          const hasRunning = g.runningCount > 0
          return (
            <div className="container-group" key={g.key}>
              <div
                className={`container-card container-group-header${expanded ? " expanded" : ""}`}
                role="button"
                tabIndex={0}
                onClick={() => onToggleGroup(g.key)}
                onKeyDown={e => {
                  if (e.key === "Enter" || e.key === " ") {
                    e.preventDefault()
                    onToggleGroup(g.key)
                  }
                }}
                title={expanded ? "Collapse" : "Expand"}
              >
                <div className="card-main">
                  <div className="card-icon">{I.box}</div>
                  <div className="card-body">
                    <div className="card-name">{g.key}</div>
                    <div className="card-meta">
                      {t("running")}: {g.runningCount} · {t("stopped")}: {g.stoppedCount}
                    </div>
                  </div>
                  <div className="card-status">
                    <span className={`dot ${hasRunning ? "running" : "stopped"}`} />
                    <span>{hasRunning ? t("running") : t("stopped")}</span>
                  </div>
                  <div className="group-chevron" aria-hidden="true">
                    {expanded ? I.chevronDown : I.chevronRight}
                  </div>
                </div>
              </div>
              {expanded && (
                <div className="container-group-children">
                  {g.containers.map(c => renderCard(c, { child: true }))}
                </div>
              )}
            </div>
          )
        })
      )}

      {/* Run Container Modal */}
      {showRunModal && (
        <div className="modal-backdrop run-modal-backdrop" onClick={() => setShowRunModal(false)}>
          <div className="modal run-modal" onClick={e => e.stopPropagation()}>
            <div className="modal-head">
              <div className="modal-title">
                <span className="modal-title-icon">{I.play}</span>
                {t("runContainer")}
              </div>
              <div className="modal-actions">
                <button className="icon-btn" onClick={() => setShowRunModal(false)} title={t("close")}>&times;</button>
              </div>
            </div>
            <div className="modal-body run-modal-body">
              <div className="form">
                {/* Image & Name Section */}
                <div className="run-section">
                  <div className="run-section-title">
                    <span className="run-section-icon">{I.box}</span>
                    {t("image")}
                  </div>
                  <div className="run-field">
                    <label>{t("image")}<span className="run-required">*</span></label>
                    <input className="input" value={runImage} onChange={e => setRunImage(e.target.value)} placeholder="nginx:latest" autoFocus />
                  </div>
                  <div className="run-field">
                    <label>{t("nameOptional")}</label>
                    <input className="input" value={runName} onChange={e => setRunName(e.target.value)} placeholder="my-container" />
                  </div>
                </div>

                {/* Resources Section */}
                <div className="run-section">
                  <div className="run-section-title">
                    <span className="run-section-icon">{I.cpu}</span>
                    {t("cpus")} & {t("memoryMb")}
                  </div>
                  <div className="run-resources-grid">
                    <div className="run-field">
                      <label>{t("cpus")}</label>
                      <input className="input" type="number" min={1} value={runCpus} onChange={e => setRunCpus(e.target.value === "" ? "" : Number(e.target.value))} placeholder="—" />
                    </div>
                    <div className="run-field">
                      <label>{t("memoryMb")}</label>
                      <input className="input" type="number" min={64} value={runMem} onChange={e => setRunMem(e.target.value === "" ? "" : Number(e.target.value))} placeholder="—" />
                    </div>
                  </div>
                  <div className="run-toggle-row">
                    <input type="checkbox" checked={runPull} onChange={e => setRunPull(e.target.checked)} id="run-pull-toggle" />
                    <label htmlFor="run-pull-toggle">{t("pullBeforeRun")}</label>
                  </div>
                </div>

                {/* Environment Variables Section */}
                <div className="run-section">
                  <div className="run-section-title">
                    <span className="run-section-icon">{I.settings}</span>
                    {t("envVars")}
                  </div>
                  <div className="run-env-hint">{t("envVarsHint")}</div>
                  {runEnvVars.length > 0 && (
                    <div className="run-env-list">
                      {runEnvVars.map((env, i) => (
                        <div className="run-env-row" key={i}>
                          <input
                            className="input"
                            value={env.key}
                            onChange={e => {
                              const updated = [...runEnvVars]
                              updated[i] = { ...updated[i], key: e.target.value }
                              setRunEnvVars(updated)
                            }}
                            placeholder={t("envKey")}
                          />
                          <span className="run-env-eq">=</span>
                          <input
                            className="input"
                            value={env.value}
                            onChange={e => {
                              const updated = [...runEnvVars]
                              updated[i] = { ...updated[i], value: e.target.value }
                              setRunEnvVars(updated)
                            }}
                            placeholder={t("envValue")}
                          />
                          <button
                            className="run-env-delete"
                            onClick={() => setRunEnvVars(runEnvVars.filter((_, j) => j !== i))}
                            title={t("removeEnvVar")}
                          >
                            {I.trash}
                          </button>
                        </div>
                      ))}
                    </div>
                  )}
                  <button
                    className="btn xs run-env-add"
                    onClick={() => setRunEnvVars([...runEnvVars, { key: "", value: "" }])}
                  >
                    <span className="icon">{I.plus}</span>{t("addEnvVar")}
                  </button>
                </div>
              </div>

              {/* Error & Result */}
              {runError && (
                <div className="run-error">
                  <span className="run-error-icon">{I.alertCircle}</span>
                  {runError}
                </div>
              )}
              {runResult && (
                <div className="run-result">
                  <div className="run-result-title">{I.terminal} {t("loginCommand")}</div>
                  <div className="run-result-code">
                    <code>{runResult.login_cmd}</code>
                    <button className="icon-btn" onClick={() => {
                      navigator.clipboard.writeText(runResult.login_cmd)
                    }} title={t("copy")}>{I.copy}</button>
                  </div>
                </div>
              )}
            </div>
            <div className="modal-footer">
              <button className="btn" onClick={() => setShowRunModal(false)}>{t("close")}</button>
              <button className="btn primary" disabled={runLoading || !runImage.trim()} onClick={handleRun}>
                {runLoading ? (
                  <><div className="spinner spinner-sm" />{t("creating")}</>
                ) : (
                  <><span className="icon">{I.play}</span>{t("create")}</>
                )}
              </button>
            </div>
          </div>
        </div>
      )}

      {/* Env Viewer Modal */}
      {showEnvModal && (
        <div className="modal-backdrop" onClick={() => setShowEnvModal(false)}>
          <div className="modal" onClick={e => e.stopPropagation()} style={{ maxWidth: 640 }}>
            <div className="modal-head">
              <div className="modal-title">{t("envVars")} — {envContainerName}</div>
              <div className="modal-actions">
                <button className="icon-btn" onClick={() => setShowEnvModal(false)} title={t("close")}>&times;</button>
              </div>
            </div>
            <div className="modal-body" style={{ padding: "10px 14px" }}>
              {envError && <div className="hint" style={{ color: "var(--red)", marginBottom: 8 }}>{envError}</div>}
              {envLoading ? (
                <div style={{ display: "flex", alignItems: "center", gap: 8, padding: 20, justifyContent: "center", color: "var(--text2)" }}>
                  <div className="spinner" />{t("loading")}
                </div>
              ) : envVars.length === 0 ? (
                <div style={{ padding: 20, textAlign: "center", color: "var(--text3)" }}>{t("noEnvVars")}</div>
              ) : (
                <div style={{ overflowX: "auto" }}>
                  <table style={{ width: "100%", borderCollapse: "collapse", fontSize: 12 }}>
                    <thead>
                      <tr style={{ borderBottom: "1px solid var(--border)" }}>
                        <th style={{ textAlign: "left", padding: "6px 10px", color: "var(--text2)", fontWeight: 700 }}>{t("envKey")}</th>
                        <th style={{ textAlign: "left", padding: "6px 10px", color: "var(--text2)", fontWeight: 700 }}>{t("envValue")}</th>
                      </tr>
                    </thead>
                    <tbody>
                      {envVars.map((ev, i) => (
                        <tr key={i} style={{ borderBottom: "1px solid var(--border)" }}>
                          <td style={{ padding: "5px 10px", fontFamily: "'Geist Mono', ui-monospace, monospace", fontWeight: 600, wordBreak: "break-all" }}>{ev.key}</td>
                          <td style={{ padding: "5px 10px", fontFamily: "'Geist Mono', ui-monospace, monospace", wordBreak: "break-all" }}>{ev.value}</td>
                        </tr>
                      ))}
                    </tbody>
                  </table>
                </div>
              )}
            </div>
            <div className="modal-footer">
              <button className="btn" onClick={() => setShowEnvModal(false)}>{t("close")}</button>
            </div>
          </div>
        </div>
      )}

      {/* Log Viewer Modal */}
      {showLogModal && (
        <div className="modal-backdrop" onClick={() => setShowLogModal(false)}>
          <div className="modal" onClick={e => e.stopPropagation()} style={{ maxWidth: 720 }}>
            <div className="modal-head">
              <div className="modal-title">
                {t("logs")} — {logContainerName}
                {logFollowing && (
                  <span style={{
                    display: "inline-flex",
                    alignItems: "center",
                    gap: 4,
                    marginLeft: 10,
                    fontSize: 11,
                    fontWeight: 600,
                    color: "var(--green, #22c55e)",
                    background: "rgba(34, 197, 94, 0.1)",
                    padding: "2px 8px",
                    borderRadius: 10,
                  }}>
                    <span style={{
                      width: 6,
                      height: 6,
                      borderRadius: "50%",
                      background: "var(--green, #22c55e)",
                      animation: "pulse 1.5s ease-in-out infinite",
                    }} />
                    {t("live")}
                  </span>
                )}
              </div>
              <div className="modal-actions">
                <button className="icon-btn" onClick={() => setShowLogModal(false)} title={t("close")}>&times;</button>
              </div>
            </div>
            <div className="modal-body" style={{ padding: "10px 14px" }}>
              <div style={{ display: "flex", alignItems: "center", gap: 10, flexWrap: "wrap", marginBottom: 10 }}>
                <label style={{ fontSize: 11, fontWeight: 700, color: "var(--text2)" }}>{t("tailLines")}</label>
                <select
                  className="select"
                  value={logTail}
                  disabled={logFollowing}
                  onChange={e => {
                    setLogTail(e.target.value)
                    fetchLogs(logContainerId, e.target.value, logTimestamps)
                  }}
                >
                  <option value="100">100</option>
                  <option value="200">200</option>
                  <option value="500">500</option>
                  <option value="all">All</option>
                </select>
                <label style={{ display: "flex", alignItems: "center", gap: 6, fontSize: 12, color: "var(--text2)", cursor: "pointer" }}>
                  <input
                    type="checkbox"
                    checked={logTimestamps}
                    disabled={logFollowing}
                    onChange={e => {
                      setLogTimestamps(e.target.checked)
                      fetchLogs(logContainerId, logTail, e.target.checked)
                    }}
                    style={{ width: 14, height: 14 }}
                  />
                  {t("showTimestamps")}
                </label>
                <div style={{ flex: 1 }} />
                <button
                  className={`btn sm${logFollowing ? " primary" : ""}`}
                  onClick={() => {
                    if (logFollowing) {
                      stopLogStream()
                    } else {
                      startLogStream(logContainerId, logTimestamps)
                    }
                  }}
                >
                  {logFollowing ? t("stopFollowing") : t("follow")}
                </button>
                <button
                  className="btn sm"
                  disabled={logLoading || logFollowing}
                  onClick={() => fetchLogs(logContainerId, logTail, logTimestamps)}
                >
                  <span className="icon">{I.refresh}</span>
                  {t("refreshLogs")}
                </button>
              </div>
              {logFollowing && (
                <div style={{
                  display: "flex",
                  alignItems: "center",
                  gap: 6,
                  padding: "4px 8px",
                  marginBottom: 8,
                  fontSize: 11,
                  color: "var(--text2)",
                  background: "var(--bg2, rgba(0,0,0,0.05))",
                  borderRadius: 4,
                }}>
                  <div className="spinner" style={{ width: 12, height: 12 }} />
                  {t("streamingLogs")}
                </div>
              )}
              {logStreamEnded && !logFollowing && (
                <div style={{
                  padding: "4px 8px",
                  marginBottom: 8,
                  fontSize: 11,
                  color: "var(--text3)",
                  background: "var(--bg2, rgba(0,0,0,0.05))",
                  borderRadius: 4,
                }}>
                  {t("logStreamEnded")}
                </div>
              )}
              {logError && <div className="hint" style={{ color: "var(--red)", marginBottom: 8 }}>{logError}</div>}
              <div className="log-viewer">
                {logLoading && !logContent ? (
                  <div style={{ display: "flex", alignItems: "center", gap: 8, padding: 20, justifyContent: "center", color: "var(--text2)" }}>
                    <div className="spinner" />{t("loading")}
                  </div>
                ) : logContent ? (
                  <>
                    <pre className="log-content">{logContent}</pre>
                    <div ref={logEndRef} />
                  </>
                ) : (
                  <div style={{ padding: 20, textAlign: "center", color: "var(--text3)" }}>{t("noLogs")}</div>
                )}
              </div>
            </div>
            <div className="modal-footer">
              <button className="btn" onClick={() => setShowLogModal(false)}>{t("close")}</button>
            </div>
          </div>
        </div>
      )}

      {/* Exec Terminal Modal */}
      {execContainer && (
        <div className="modal-backdrop" onClick={() => setExecContainer(null)}>
          <div className="modal" onClick={e => e.stopPropagation()} style={{ maxWidth: 640 }}>
            <div className="modal-head">
              <div className="modal-title">{t("terminal")} — {execContainer.name || execContainer.id}</div>
              <div className="modal-actions">
                <button className="btn xs" onClick={() => { setExecHistory([]); }} title={t("clear")}>{t("clear")}</button>
                <button className="icon-btn" onClick={() => setExecContainer(null)} title={t("close")}>&times;</button>
              </div>
            </div>
            <div className="exec-modal-body">
              <div className="exec-toolbar">
                <input
                  className="input"
                  value={execCmd}
                  onChange={e => setExecCmd(e.target.value)}
                  onKeyDown={e => { if (e.key === "Enter") handleExec() }}
                  placeholder={t("commandPlaceholder")}
                  disabled={execRunning}
                  autoFocus
                />
                <button className="btn primary sm" disabled={execRunning || !execCmd.trim()} onClick={handleExec}>
                  {execRunning ? t("working") : t("runCommand")}
                </button>
              </div>
              <div className="exec-output" ref={execOutputRef}>
                {execHistory.length === 0 && (
                  <span style={{ color: "var(--text3)" }}>{t("commandPlaceholder")}</span>
                )}
                {execHistory.map((entry, i) => (
                  <div className="exec-entry" key={i}>
                    <div><span className="exec-prompt">$ </span>{entry.command}</div>
                    {entry.isError ? (
                      <div className="exec-error-text">{entry.output}</div>
                    ) : (
                      <div className="exec-result">{entry.output}</div>
                    )}
                  </div>
                ))}
                {execRunning && <div className="exec-prompt" style={{ opacity: 0.5 }}>...</div>}
              </div>
              <div className="exec-copy-bar">
                <code>{execInteractiveCmd}</code>
                <button
                  className="btn xs"
                  onClick={() => navigator.clipboard.writeText(execInteractiveCmd)}
                  title={t("copyExecCmd")}
                >
                  {I.copy} {t("copy")}
                </button>
              </div>
            </div>
          </div>
        </div>
      )}

      {/* Confirm Remove Container Modal */}
      {confirmRemove && (
        <div className="modal-backdrop" onClick={() => setConfirmRemove("")}>
          <div className="modal" onClick={e => e.stopPropagation()} style={{ maxWidth: 420 }}>
            <div className="modal-head">
              <div className="modal-title">{t("removeContainer")}</div>
              <div className="modal-actions">
                <button className="icon-btn" onClick={() => setConfirmRemove("")} title={t("close")}>&times;</button>
              </div>
            </div>
            <div className="modal-body">
              <p style={{ margin: 0 }}>{t("confirmRemoveContainer")}</p>
              <p style={{ margin: "8px 0 0", fontWeight: 600, color: "var(--text)" }}>{containerToRemoveName}</p>
            </div>
            <div className="modal-footer">
              <button className="btn" onClick={() => setConfirmRemove("")}>{t("close")}</button>
              <button className="btn primary" onClick={() => { onContainerAction("remove_container", confirmRemove); setConfirmRemove("") }} style={{ marginLeft: 8, background: "var(--red)", borderColor: "var(--red)" }}>
                {t("remove")}
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  )
}
