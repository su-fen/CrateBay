import { useState, useRef, useEffect, useCallback } from "react"
import { invoke } from "@tauri-apps/api/core"
import { I } from "../icons"
import { ErrorBanner } from "../components/ErrorDisplay"
import { EmptyState } from "../components/EmptyState"
import { MiniStats } from "../components/StatsBar"
import type { ContainerInfo, ContainerGroup, RunContainerResult, ContainerStats } from "../types"

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
  onRun: (image: string, name: string, cpus: number | "", mem: number | "", pull: boolean) => Promise<RunContainerResult | null>
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

  // Exec terminal state
  const [execContainer, setExecContainer] = useState<ContainerInfo | null>(null)
  const [execCmd, setExecCmd] = useState("")
  const [execHistory, setExecHistory] = useState<ExecEntry[]>([])
  const [execRunning, setExecRunning] = useState(false)
  const [execInteractiveCmd, setExecInteractiveCmd] = useState("")
  const execOutputRef = useRef<HTMLDivElement>(null)

  // Container stats state
  const [containerStats, setContainerStats] = useState<Record<string, ContainerStats>>({})

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
      const result = await onRun(runImage, runName, runCpus, runMem, runPull)
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
    setShowRunModal(true)
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

  const openLogModal = (c: ContainerInfo) => {
    setLogContainerId(c.id)
    setLogContainerName(c.name || c.id)
    setLogContent("")
    setLogError("")
    setLogTail("200")
    setLogTimestamps(false)
    setShowLogModal(true)
    fetchLogs(c.id, "200", false)
  }

  useEffect(() => {
    if (showLogModal && logEndRef.current) {
      logEndRef.current.scrollIntoView({ behavior: "smooth" })
    }
  }, [logContent, showLogModal])

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
      <div className={`container-card${childClass}${isRunning ? " container-card-with-stats" : ""}`} key={c.id}>
        <div className="container-card-main">
          <div className={`card-icon${isRunning ? "" : " stopped"}`}>{I.box}</div>
          <div className="card-body">
            <div className="card-name">{name}</div>
            <div className="card-meta">{c.image} · {meta}</div>
          </div>
          <div className="card-status">
            <span className={`dot ${isRunning ? "running" : "stopped"}`} />
            <span>{c.status}</span>
          </div>
          <div className="card-actions">
            <button
              className="action-btn"
              disabled={acting === c.id}
              onClick={async () => {
                const target = c.name || c.id
                const cmd = await invoke<string>("container_login_cmd", { container: target, shell: "/bin/sh" })
                onOpenTextModal(t("loginCommand"), cmd, cmd)
              }}
              title={t("loginCommand")}
            >
              {I.terminal}
            </button>
            <button
              className="action-btn"
              disabled={acting === c.id}
              onClick={() => openLogModal(c)}
              title={t("viewLogs")}
            >
              {I.fileText}
            </button>
            {isRunning && (
              <button
                className="action-btn"
                disabled={acting === c.id}
                onClick={() => openExecModal(c)}
                title={t("execCommand")}
              >
                {I.command}
              </button>
            )}
            {isRunning ? (
              <button className="action-btn" disabled={acting === c.id} onClick={() => onContainerAction("stop_container", c.id)} title={t("stop")}>{I.stop}</button>
            ) : (
              <button className="action-btn" disabled={acting === c.id} onClick={() => onContainerAction("start_container", c.id)} title={t("start")}>{I.play}</button>
            )}
            <button
              className="action-btn"
              disabled={acting === c.id}
              onClick={() => {
                const target = c.name || c.id
                const defaultTag = `${(c.image || "image").split(":")[0]}-snapshot:latest`
                onOpenPackageModal(target, defaultTag)
              }}
              title={t("packageImage")}
            >
              {I.layers}
            </button>
            <button className="action-btn danger" disabled={acting === c.id} onClick={() => onContainerAction("remove_container", c.id)} title={t("delete")}>{I.trash}</button>
          </div>
        </div>
        {isRunning && stats && (
          <div className="container-card-stats">
            <MiniStats items={[
              { label: t("cpuUsage"), value: stats.cpu_percent, max: 100, suffix: "%" },
              { label: t("memoryUsage"), value: stats.memory_usage_mb, max: stats.memory_limit_mb, suffix: " MB" },
            ]} />
          </div>
        )}
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
        <div className="modal-backdrop" onClick={() => setShowRunModal(false)}>
          <div className="modal" onClick={e => e.stopPropagation()} style={{ maxWidth: 480 }}>
            <div className="modal-head">
              <div className="modal-title">{t("runContainer")}</div>
              <div className="modal-actions">
                <button className="icon-btn" onClick={() => setShowRunModal(false)} title={t("close")}>×</button>
              </div>
            </div>
            <div className="modal-body">
              <div className="form">
                <div className="row">
                  <label>{t("image")}</label>
                  <input className="input" value={runImage} onChange={e => setRunImage(e.target.value)} placeholder="nginx:latest" autoFocus />
                </div>
                <div className="row">
                  <label>{t("nameOptional")}</label>
                  <input className="input" value={runName} onChange={e => setRunName(e.target.value)} placeholder="web" />
                </div>
                <div className="row two">
                  <div>
                    <label>{t("cpus")}</label>
                    <input className="input" type="number" min={1} value={runCpus} onChange={e => setRunCpus(e.target.value === "" ? "" : Number(e.target.value))} />
                  </div>
                  <div>
                    <label>{t("memoryMb")}</label>
                    <input className="input" type="number" min={64} value={runMem} onChange={e => setRunMem(e.target.value === "" ? "" : Number(e.target.value))} />
                  </div>
                </div>
                <div className="row inline">
                  <input type="checkbox" checked={runPull} onChange={e => setRunPull(e.target.checked)} />
                  <span>{t("pullBeforeRun")}</span>
                </div>
              </div>
              {runError && <div className="hint" style={{ color: "var(--red)", marginTop: 8 }}>{runError}</div>}
              {runResult && (
                <div className="result" style={{ marginTop: 14 }}>
                  <div className="result-title">{t("loginCommand")}</div>
                  <div className="result-code">
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
              <button className="btn primary" disabled={runLoading || !runImage.trim()} onClick={handleRun} style={{ marginLeft: 8 }}>
                {runLoading ? t("creating") : t("create")}
              </button>
            </div>
          </div>
        </div>
      )}

      {/* Log Viewer Modal */}
      {showLogModal && (
        <div className="modal-backdrop" onClick={() => setShowLogModal(false)}>
          <div className="modal" onClick={e => e.stopPropagation()} style={{ maxWidth: 720 }}>
            <div className="modal-head">
              <div className="modal-title">{t("logs")} — {logContainerName}</div>
              <div className="modal-actions">
                <button className="icon-btn" onClick={() => setShowLogModal(false)} title={t("close")}>×</button>
              </div>
            </div>
            <div className="modal-body" style={{ padding: "10px 14px" }}>
              <div style={{ display: "flex", alignItems: "center", gap: 10, flexWrap: "wrap", marginBottom: 10 }}>
                <label style={{ fontSize: 11, fontWeight: 700, color: "var(--text2)" }}>{t("tailLines")}</label>
                <select
                  className="select"
                  value={logTail}
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
                  className="btn sm"
                  disabled={logLoading}
                  onClick={() => fetchLogs(logContainerId, logTail, logTimestamps)}
                >
                  <span className="icon">{I.refresh}</span>
                  {t("refreshLogs")}
                </button>
              </div>
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
                <button className="icon-btn" onClick={() => setExecContainer(null)} title={t("close")}>×</button>
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
    </div>
  )
}
