import { useState } from "react"
import { invoke } from "@tauri-apps/api/core"
import { I } from "../icons"
import { ErrorBanner } from "../components/ErrorDisplay"
import { EmptyState } from "../components/EmptyState"
import type { ContainerInfo, ContainerGroup, RunContainerResult } from "../types"

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
    return (
      <div className={`container-card${childClass}`} key={c.id}>
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
    </div>
  )
}
