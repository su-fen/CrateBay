import { useState, useEffect, useCallback } from "react"
import { invoke } from "@tauri-apps/api/core"
import { I } from "../icons"
import { StatsBar } from "../components/StatsBar"
import type { ContainerInfo, ContainerStats, VmStats, VmInfoDto } from "../types"

interface DashboardProps {
  containers: ContainerInfo[]
  running: ContainerInfo[]
  vmsCount: number
  vmsRunningCount: number
  runningVms: VmInfoDto[]
  imgResultsCount: number
  connected: boolean
  onNavigate: (page: "containers" | "vms" | "images") => void
  t: (key: string) => string
}

interface TotalResources {
  totalCpuPercent: number
  totalMemoryUsageMb: number
  totalMemoryLimitMb: number
}

export function Dashboard({
  containers, running, vmsCount, vmsRunningCount, runningVms,
  imgResultsCount, connected, onNavigate, t,
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
      fetchTotals()
      const iv = setInterval(fetchTotals, 5000)
      return () => clearInterval(iv)
    } else {
      setTotals({ totalCpuPercent: 0, totalMemoryUsageMb: 0, totalMemoryLimitMb: 0 })
    }
  }, [fetchTotals, running.length, runningVms.length])

  const hasRunning = running.length > 0 || runningVms.length > 0

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

      {hasRunning && (
        <div className="dash-resources-panel">
          <div className="section-title">{t("totalResources")}</div>
          <div className="dash-resources-bars">
            <StatsBar
              label={t("cpuUsage")}
              value={totals.totalCpuPercent}
              max={100}
              suffix="%"
            />
            <StatsBar
              label={t("memoryUsage")}
              value={totals.totalMemoryUsageMb}
              max={totals.totalMemoryLimitMb || 1}
              suffix=" MB"
            />
          </div>
        </div>
      )}

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
