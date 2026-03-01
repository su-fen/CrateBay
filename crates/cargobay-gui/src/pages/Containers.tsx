import { invoke } from "@tauri-apps/api/core"
import { I } from "../icons"
import { ErrorBanner } from "../components/ErrorDisplay"
import { EmptyState } from "../components/EmptyState"
import type { ContainerInfo, ContainerGroup } from "../types"

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
  t: (key: string) => string
}

export function Containers({
  groups, loading, error, acting, expandedGroups,
  onContainerAction, onToggleGroup,
  onOpenTextModal, onOpenPackageModal, onFetch, t,
}: ContainersProps) {
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
  if (groups.length === 0) {
    return (
      <EmptyState
        icon={I.box}
        title={t("noContainers")}
        description={t("runContainerTip")}
        code="docker run -it -p 80:80 docker/getting-started"
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
    <>
      {groups.map(g => {
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
      })}
    </>
  )
}
