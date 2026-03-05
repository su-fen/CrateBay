import { useState, useEffect, useCallback } from "react"
import { invoke } from "@tauri-apps/api/core"
import { I } from "../icons"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Card, CardContent } from "@/components/ui/card"
import { Progress } from "@/components/ui/progress"
import { cn } from "@/lib/utils"
import { iconStroke, cardActionGhost } from "@/lib/styles"
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


function NavCard({
  value,
  label,
  icon,
  iconClassName,
  sub,
  onClick,
  testId,
}: {
  value: number
  label: string
  icon: React.ReactNode
  iconClassName: string
  sub?: React.ReactNode
  onClick: () => void
  testId: string
}) {
  return (
    <Card
      data-testid={testId}
      role="button"
      tabIndex={0}
      className="py-0 cursor-pointer transition-all hover:border-primary/40 hover:bg-accent/10 motion-safe:hover:-translate-y-px motion-safe:hover:shadow-sm focus-visible:outline-hidden focus-visible:ring-[3px] focus-visible:ring-ring/50"
      onClick={onClick}
      onKeyDown={(e) => {
        if (e.key === "Enter" || e.key === " ") {
          e.preventDefault()
          onClick()
        }
      }}
    >
      <CardContent className="py-4">
        <div className="flex items-start justify-between gap-3">
          <div
            className={cn(
              "size-10 shrink-0 rounded-lg flex items-center justify-center",
              iconStroke,
              "[&_svg]:size-[18px]",
              iconClassName
            )}
          >
            {icon}
          </div>
          {sub}
        </div>
        <div className="mt-4">
          <div className="text-[34px] leading-none font-bold text-foreground">
            {value}
          </div>
          <div className="mt-1 text-xs text-muted-foreground">{label}</div>
        </div>
      </CardContent>
    </Card>
  )
}

export function Dashboard({
  containers,
  running,
  vmsCount,
  vmsRunningCount,
  runningVms,
  imgResultsCount,
  installedImagesCount,
  volumesCount,
  onNavigate,
  t,
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

    const containerPromises = running.map(async (c) => {
      try {
        const stats = await invoke<ContainerStats>("container_stats", { id: c.id })
        return stats
      } catch {
        return null
      }
    })

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
    }
    setTotals({ totalCpuPercent: 0, totalMemoryUsageMb: 0, totalMemoryLimitMb: 0 })
  }, [fetchTotals, running.length, runningVms.length])

  const hasRunning = running.length > 0 || runningVms.length > 0
  const memPercent =
    totals.totalMemoryLimitMb > 0
      ? (totals.totalMemoryUsageMb / totals.totalMemoryLimitMb) * 100
      : 0
  const cpuClamped = Math.min(totals.totalCpuPercent, 100)

  return (
    <div className="space-y-6">
      <div className="grid grid-cols-1 gap-4 md:grid-cols-2 xl:grid-cols-4">
        <NavCard
          testId="dashboard-card-containers"
          value={containers.length}
          label={t("containers")}
          icon={I.box}
          iconClassName="bg-primary/10 text-primary"
          sub={
            running.length > 0 ? (
              <Badge
                variant="secondary"
                className="rounded-full gap-2 border border-brand-green/15 bg-brand-green/10 px-2 py-0.5 text-[11px] text-brand-green"
              >
                <span className="size-1.5 rounded-full bg-brand-green shadow-[0_0_10px_hsl(var(--brand-green)/0.6)]" />
                {running.length} {t("runningCount")}
              </Badge>
            ) : (
              <span className="text-xs text-muted-foreground">
                {t("noRunning") || "Idle"}
              </span>
            )
          }
          onClick={() => onNavigate("containers")}
        />

        <NavCard
          testId="dashboard-card-vms"
          value={vmsCount}
          label={t("vms")}
          icon={I.server}
          iconClassName="bg-brand-cyan/10 text-brand-cyan"
          sub={
            vmsRunningCount > 0 ? (
              <Badge
                variant="secondary"
                className="rounded-full gap-2 border border-brand-green/15 bg-brand-green/10 px-2 py-0.5 text-[11px] text-brand-green"
              >
                <span className="size-1.5 rounded-full bg-brand-green shadow-[0_0_10px_hsl(var(--brand-green)/0.6)]" />
                {vmsRunningCount} {t("runningCount")}
              </Badge>
            ) : (
              <span className="text-xs text-muted-foreground">
                {t("noRunning") || "Idle"}
              </span>
            )
          }
          onClick={() => onNavigate("vms")}
        />

        <NavCard
          testId="dashboard-card-images"
          value={installedImagesCount}
          label={t("images")}
          icon={I.layers}
          iconClassName="bg-brand-green/10 text-brand-green"
          sub={
            imgResultsCount > 0 ? (
              <Badge
                variant="secondary"
                className="rounded-full border border-brand-cyan/15 bg-brand-cyan/10 px-2 py-0.5 text-[11px] text-brand-cyan"
              >
                {imgResultsCount} {t("searchResults")}
              </Badge>
            ) : (
              <span className="text-xs text-muted-foreground" />
            )
          }
          onClick={() => onNavigate("images")}
        />

        <NavCard
          testId="dashboard-card-volumes"
          value={volumesCount}
          label={t("volumes")}
          icon={I.hardDrive}
          iconClassName="bg-yellow-500/10 text-yellow-500 dark:text-yellow-400"
          onClick={() => onNavigate("volumes")}
        />
      </div>

      {hasRunning && (
        <div className="grid grid-cols-1 gap-4 xl:grid-cols-2">
          <Card className="py-0">
            <CardContent className="py-4">
              <div className="flex items-center gap-4">
                <div className={cn("size-10 shrink-0 rounded-lg bg-primary/10 text-primary flex items-center justify-center", iconStroke, "[&_svg]:size-[18px]")}>
                  {I.cpu}
                </div>
                <div className="min-w-0 flex-1">
                  <div className="flex items-center justify-between gap-3">
                    <span className="text-xs font-semibold text-muted-foreground">
                      {t("cpuUsage")}
                    </span>
                    <span className="text-xs font-semibold text-foreground">
                      {totals.totalCpuPercent.toFixed(1)}%
                    </span>
                  </div>
                  <div className="mt-2">
                    <Progress
                      value={cpuClamped}
                      style={
                        { "--progress-color": "hsl(var(--primary))" } as React.CSSProperties
                      }
                    />
                  </div>
                </div>
              </div>
            </CardContent>
          </Card>

          <Card className="py-0">
            <CardContent className="py-4">
              <div className="flex items-center gap-4">
                <div className={cn("size-10 shrink-0 rounded-lg bg-brand-cyan/10 text-brand-cyan flex items-center justify-center", iconStroke, "[&_svg]:size-[18px]")}>
                  {I.memory}
                </div>
                <div className="min-w-0 flex-1">
                  <div className="flex items-center justify-between gap-3">
                    <span className="text-xs font-semibold text-muted-foreground">
                      {t("memoryUsage")}
                    </span>
                    <span className="text-xs font-semibold text-foreground">
                      {totals.totalMemoryUsageMb.toFixed(0)}{" "}
                      {totals.totalMemoryLimitMb > 0 && (
                        <>
                          / {totals.totalMemoryLimitMb.toFixed(0)} MB
                        </>
                      )}
                    </span>
                  </div>
                  <div className="mt-2">
                    <Progress
                      className="bg-brand-cyan/20"
                      value={Math.min(memPercent, 100)}
                      style={
                        { "--progress-color": "hsl(var(--brand-cyan))" } as React.CSSProperties
                      }
                    />
                  </div>
                </div>
              </div>
            </CardContent>
          </Card>
        </div>
      )}

      {running.length > 0 && (
        <Card className="py-0">
          <div className="flex items-center justify-between gap-3 border-b border-border/70 px-6 py-4">
            <div className="flex items-center gap-3">
              <div className={cn("size-8 rounded-lg bg-brand-green/10 text-brand-green flex items-center justify-center", iconStroke, "[&_svg]:size-[14px]")}>
                {I.play}
              </div>
              <div className="text-sm font-semibold text-foreground">
                {t("running")}
              </div>
              <Badge
                variant="secondary"
                className="rounded-full border border-brand-green/15 bg-brand-green/10 px-2 py-0.5 text-[11px] text-brand-green"
              >
                {running.length}
              </Badge>
            </div>
            {running.length > 5 && (
              <Button
                type="button"
                variant="ghost"
                size="sm"
                className={cardActionGhost}
                onClick={() => onNavigate("containers")}
              >
                {t("viewAll")}
                <span className={cn(iconStroke, "[&_svg]:size-[14px]")}>
                  {I.chevronRight}
                </span>
              </Button>
            )}
          </div>

          <div className="divide-y divide-border/60">
            {running.slice(0, 5).map((c, idx) => (
              <div
                key={c.id}
                data-testid="dashboard-running-item"
                className="flex items-center gap-3 px-6 py-3 hover:bg-accent/30"
              >
                <div className="hidden sm:flex size-6 shrink-0 items-center justify-center rounded-md bg-secondary text-muted-foreground font-mono text-[11px]">
                  {idx + 1}
                </div>
                <div className={cn("size-9 shrink-0 rounded-lg bg-gradient-to-br from-primary to-primary/70 text-primary-foreground flex items-center justify-center shadow-sm", iconStroke, "[&_svg]:size-[16px]")}>
                  {I.box}
                </div>
                <div className="min-w-0 flex-1">
                  <div className="text-sm font-semibold text-foreground truncate">
                    {c.name}
                  </div>
                  <div className="mt-0.5 text-xs text-muted-foreground truncate">
                    <span className="truncate">{c.image}</span>
                    {c.ports && (
                      <>
                        <span className="mx-2 text-muted-foreground/70">·</span>
                        <span className="font-mono text-[10px] text-brand-cyan">
                          {c.ports}
                        </span>
                      </>
                    )}
                  </div>
                </div>
                <div className="shrink-0 flex items-center gap-2 rounded-lg border border-brand-green/15 bg-brand-green/10 px-3 py-1">
                  <span className="size-1.5 rounded-full bg-brand-green shadow-[0_0_10px_hsl(var(--brand-green)/0.6)]" />
                  <span className="text-xs font-medium text-brand-green">
                    {c.status}
                  </span>
                </div>
              </div>
            ))}
          </div>
        </Card>
      )}
    </div>
  )
}
