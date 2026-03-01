import { I } from "../icons"
import type { ContainerInfo } from "../types"

interface DashboardProps {
  containers: ContainerInfo[]
  running: ContainerInfo[]
  vmsCount: number
  vmsRunningCount: number
  imgResultsCount: number
  connected: boolean
  onNavigate: (page: "containers" | "vms" | "images") => void
  t: (key: string) => string
}

export function Dashboard({
  containers, running, vmsCount, vmsRunningCount,
  imgResultsCount, connected, onNavigate, t,
}: DashboardProps) {
  return (
    <div className="dashboard">
      <div className="dash-cards">
        <div className="dash-card" onClick={() => onNavigate("containers")}>
          <div className="dash-card-icon">{I.box}</div>
          <div className="dash-card-info">
            <div className="dash-card-value">{containers.length}</div>
            <div className="dash-card-label">{t("containers")}</div>
          </div>
          <div className="dash-card-sub">
            {running.length > 0 && <span className="dash-running">{running.length} {t("runningCount")}</span>}
          </div>
        </div>
        <div className="dash-card" onClick={() => onNavigate("vms")}>
          <div className="dash-card-icon">{I.server}</div>
          <div className="dash-card-info">
            <div className="dash-card-value">{vmsCount}</div>
            <div className="dash-card-label">{t("vms")}</div>
          </div>
          <div className="dash-card-sub">
            {vmsCount > 0 && <span className="dash-running">{vmsRunningCount} {t("runningCount")}</span>}
          </div>
        </div>
        <div className="dash-card" onClick={() => onNavigate("images")}>
          <div className="dash-card-icon">{I.layers}</div>
          <div className="dash-card-info">
            <div className="dash-card-value">{imgResultsCount}</div>
            <div className="dash-card-label">{t("images")}</div>
          </div>
          <div className="dash-card-sub">
            {imgResultsCount > 0 && <span className="dash-badge">{t("searchResults")}</span>}
          </div>
        </div>
        <div className="dash-card">
          <div className="dash-card-icon">{I.cpu}</div>
          <div className="dash-card-info">
            <div className="dash-card-value">{connected ? "OK" : "--"}</div>
            <div className="dash-card-label">{t("system")}</div>
          </div>
          <div className="dash-card-sub">
            <span className={`dot ${connected ? "on" : "off"}`} />
            <span>{connected ? "Docker " + t("connected") : t("disconnected")}</span>
          </div>
        </div>
      </div>

      {running.length > 0 && <>
        <div className="section-title">{t("running")} ({running.length})</div>
        {running.slice(0, 5).map(c => (
          <div className="container-card" key={c.id}>
            <div className="card-icon">{I.box}</div>
            <div className="card-body">
              <div className="card-name">{c.name}</div>
              <div className="card-meta">{c.image} · {c.ports || c.id}</div>
            </div>
            <div className="card-status">
              <span className="dot running" />
              <span>{c.status}</span>
            </div>
          </div>
        ))}
        {running.length > 5 && (
          <div className="view-all" onClick={() => onNavigate("containers")}>
            {t("viewAll")} ({running.length})
          </div>
        )}
      </>}
    </div>
  )
}
