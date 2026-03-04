import { useState, useEffect, useCallback } from "react"
import { invoke } from "@tauri-apps/api/core"
import { I } from "../icons"
import type { ContainerInfo, ContainerStats, VmStats, VmInfoDto } from "../types"

interface DashboardProps {
  containers: ContainerInfo[]
  running: ContainerInfo[]
  vmsCount: number
  vmsRunningCount: number
  runningVms: VmInfoDto[]
  imgResultsCount: number
  installedImagesCount: number
  volumesCount: number
  onNavigate: (page: "containers" | "vms" | "images" | "volumes") => void
  t: (key: string) => string
}

interface TotalResources {
  totalCpuPercent: number
  totalMemoryUsageMb: number
  totalMemoryLimitMb: number
}

export function Dashboard({
  containers, running, vmsCount, vmsRunningCount, runningVms,
  imgResultsCount, installedImagesCount, volumesCount, onNavigate, t,
}: DashboardProps) {
  const [totals, setTotals] = useState<TotalResources>({
    totalCpuPercent: 0,
    totalMemoryUsageMb: 0,
    totalMemoryLimitMb: 0,
  })

  const fetchTotals = useCallback(async () => {
    let totalCpu = 0
    let totalMemUsage = 0
    let totalMemLimit = 0

    // Fetch container stats
    const containerPromises = running.map(async (c) => {
      try {
        const stats = await invoke<ContainerStats>("container_stats", { id: c.id })
        return stats
      } catch {
        return null
      }
    })

    // Fetch VM stats
    const vmPromises = runningVms.map(async (vm) => {
      try {
        const stats = await invoke<VmStats>("vm_stats", { id: vm.id })
        return stats
      } catch {
        return null
      }
    })

    const [containerResults, vmResults] = await Promise.all([
      Promise.allSettled(containerPromises),
      Promise.allSettled(vmPromises),
    ])

    for (const result of containerResults) {
      if (result.status === "fulfilled" && result.value) {
        totalCpu += result.value.cpu_percent
        totalMemUsage += result.value.memory_usage_mb
        totalMemLimit += result.value.memory_limit_mb
      }
    }

    for (const result of vmResults) {
      if (result.status === "fulfilled" && result.value) {
        totalCpu += result.value.cpu_percent
        totalMemUsage += result.value.memory_usage_mb
      }
    }

    // Add VM memory limits from VM configs
    for (const vm of runningVms) {
      totalMemLimit += vm.memory_mb
    }

    setTotals({
      totalCpuPercent: totalCpu,
      totalMemoryUsageMb: totalMemUsage,
      totalMemoryLimitMb: totalMemLimit,
    })
  }, [running, runningVms])

  useEffect(() => {
    if (running.length > 0 || runningVms.length > 0) {
      // eslint-disable-next-line react-hooks/set-state-in-effect
      fetchTotals()
      const iv = setInterval(fetchTotals, 5000)
      return () => clearInterval(iv)
    } else {
      setTotals({ totalCpuPercent: 0, totalMemoryUsageMb: 0, totalMemoryLimitMb: 0 })
    }
  }, [fetchTotals, running.length, runningVms.length])

  const hasRunning = running.length > 0 || runningVms.length > 0

  const memPercent = totals.totalMemoryLimitMb > 0
    ? (totals.totalMemoryUsageMb / totals.totalMemoryLimitMb) * 100
    : 0
  const cpuClamped = Math.min(totals.totalCpuPercent, 100)

  return (
    <div className="dashboard">
      {/* Navigation overview cards */}
      <div className="dash-cards">
        <div className="dash-card" onClick={() => onNavigate("containers")}>
          <div className="dash-card-top">
            <div className="dash-card-icon">{I.box}</div>
            <div className="dash-card-sub">
              {running.length > 0
                ? <span className="dash-running"><span className="dot running" />{running.length} {t("runningCount")}</span>
                : <span className="dash-idle">{t("noRunning") || "Idle"}</span>}
            </div>
          </div>
          <div className="dash-card-bottom">
            <div className="dash-card-value">{containers.length}</div>
            <div className="dash-card-label">{t("containers")}</div>
          </div>
        </div>

        <div className="dash-card" onClick={() => onNavigate("vms")}>
          <div className="dash-card-top">
            <div className="dash-card-icon icon-cyan">{I.server}</div>
            <div className="dash-card-sub">
              {vmsRunningCount > 0
                ? <span className="dash-running"><span className="dot running" />{vmsRunningCount} {t("runningCount")}</span>
                : <span className="dash-idle">{t("noRunning") || "Idle"}</span>}
            </div>
          </div>
          <div className="dash-card-bottom">
            <div className="dash-card-value">{vmsCount}</div>
            <div className="dash-card-label">{t("vms")}</div>
          </div>
        </div>

        <div className="dash-card" onClick={() => onNavigate("images")}>
          <div className="dash-card-top">
            <div className="dash-card-icon icon-green">{I.layers}</div>
            <div className="dash-card-sub">
              {imgResultsCount > 0 && <span className="dash-badge">{imgResultsCount} {t("searchResults")}</span>}
            </div>
          </div>
          <div className="dash-card-bottom">
            <div className="dash-card-value">{installedImagesCount}</div>
            <div className="dash-card-label">{t("images")}</div>
          </div>
        </div>

        <div className="dash-card" onClick={() => onNavigate("volumes")}>
          <div className="dash-card-top">
            <div className="dash-card-icon icon-yellow">{I.hardDrive}</div>
            <div className="dash-card-sub" />
          </div>
          <div className="dash-card-bottom">
            <div className="dash-card-value">{volumesCount}</div>
            <div className="dash-card-label">{t("volumes")}</div>
          </div>
        </div>
      </div>

      {/* Resource monitoring strip */}
      {hasRunning && (
        <div className="dash-resources">
          <div className="dash-res-card">
            <div className="dash-res-icon purple">{I.cpu}</div>
            <div className="dash-res-body">
              <div className="dash-res-header">
                <span className="dash-res-title">{t("cpuUsage")}</span>
                <span className="dash-res-value">{totals.totalCpuPercent.toFixed(1)}%</span>
              </div>
              <div className="dash-res-bar">
                <div className="dash-res-bar-fill purple" style={{ width: `${cpuClamped}%` }} />
              </div>
            </div>
          </div>
          <div className="dash-res-card">
            <div className="dash-res-icon cyan">{I.memory}</div>
            <div className="dash-res-body">
              <div className="dash-res-header">
                <span className="dash-res-title">{t("memoryUsage")}</span>
                <span className="dash-res-value">{totals.totalMemoryUsageMb.toFixed(0)} / {totals.totalMemoryLimitMb.toFixed(0)} MB</span>
              </div>
              <div className="dash-res-bar">
                <div className="dash-res-bar-fill cyan" style={{ width: `${Math.min(memPercent, 100)}%` }} />
              </div>
            </div>
          </div>
        </div>
      )}

      {running.length > 0 && (
        <div className="dash-running-section">
          <div className="dash-section-header">
            <div className="dash-section-left">
              <div className="dash-section-icon">{I.play}</div>
              <span className="dash-section-title">{t("running")}</span>
              <span className="dash-section-count">{running.length}</span>
            </div>
            {running.length > 5 && (
              <div className="dash-section-action" onClick={() => onNavigate("containers")}>
                {t("viewAll")} {I.chevronRight}
              </div>
            )}
          </div>
          <div className="dash-running-list">
            {running.slice(0, 5).map((c, idx) => (
              <div className="dash-running-item" key={c.id}>
                <div className="dash-running-index">{idx + 1}</div>
                <div className="dash-running-icon">{I.box}</div>
                <div className="dash-running-body">
                  <div className="dash-running-name">{c.name}</div>
                  <div className="dash-running-meta">
                    <span className="dash-running-image">{c.image}</span>
                    {c.ports && <span className="dash-running-ports">{c.ports}</span>}
                  </div>
                </div>
                <div className="dash-running-pill">
                  <span className="dot running" />
                  <span>{c.status}</span>
                </div>
              </div>
            ))}
          </div>
        </div>
      )}
    </div>
  )
}
