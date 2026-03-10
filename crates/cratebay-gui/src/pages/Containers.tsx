import { useState, useRef, useEffect, useCallback } from "react"
import { invoke } from "@tauri-apps/api/core"
import { listen, type UnlistenFn } from "@tauri-apps/api/event"
import { I } from "../icons"
import { ErrorBanner } from "../components/ErrorDisplay"
import { EmptyState } from "../components/EmptyState"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Card, CardContent } from "@/components/ui/card"
import { Checkbox } from "@/components/ui/checkbox"
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from "@/components/ui/collapsible"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog"
import { Input } from "@/components/ui/input"
import { ScrollArea } from "@/components/ui/scroll-area"
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select"
import { Separator } from "@/components/ui/separator"
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table"
import { cn } from "@/lib/utils"
import { iconStroke, cardActionSecondary, cardActionOutline } from "@/lib/styles"
import type { ContainerInfo, ContainerGroup, RunContainerResult, ContainerStats, EnvVar, LocalImageInfo } from "../types"

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
  runtimeMissing?: boolean
  settingUpRuntime?: boolean
  acting: string
  expandedGroups: Record<string, boolean>
  onContainerAction: (cmd: string, id: string) => void
  onToggleGroup: (key: string) => void
  onOpenTextModal: (title: string, body: string, copyText?: string) => void
  onOpenPackageModal: (container: string, defaultTag: string) => void
  onFetch: () => void
  onSetupRuntime?: () => void
  onRun: (image: string, name: string, cpus: number | "", mem: number | "", pull: boolean, env?: string[]) => Promise<RunContainerResult | null>
  t: (key: string) => string
}


function Spinner({ className }: { className?: string }) {
  return (
    <div
      className={cn(
        "size-4 rounded-full border-2 border-border border-t-primary animate-spin",
        className
      )}
      aria-hidden="true"
    />
  )
}

export function Containers({
  groups, loading, error, runtimeMissing, settingUpRuntime, acting, expandedGroups,
  onContainerAction, onToggleGroup,
  onOpenTextModal, onOpenPackageModal, onFetch, onSetupRuntime, onRun, t,
}: ContainersProps) {
  const [showRunModal, setShowRunModal] = useState(false)
  const [runImage, setRunImage] = useState("")
  const [runName, setRunName] = useState("")
  const [runCpus, setRunCpus] = useState<number | "">("")
  const [runMem, setRunMem] = useState<number | "">("")
  const [runPull, setRunPull] = useState(true)
  const [, setRunResult] = useState<RunContainerResult | null>(null)
  const [, setRunError] = useState("")
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

  // Refresh animation state
  const [refreshing, setRefreshing] = useState(false)

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

  // Creating container inline status
  const [creating, setCreating] = useState(false)
  const [createStatus, setCreateStatus] = useState("")
  const [createImageName, setCreateImageName] = useState("")
  const [createFailed, setCreateFailed] = useState("")

  // Local images for autocomplete
  const [localImages, setLocalImages] = useState<string[]>([])
  const [showImageDropdown, setShowImageDropdown] = useState(false)
  const imageInputRef = useRef<HTMLInputElement>(null)
  const imageDropdownRef = useRef<HTMLDivElement>(null)

  useEffect(() => {
    const fetchImages = async () => {
      try {
        const result = await invoke<LocalImageInfo[]>("image_list")
        const tags = result.flatMap(img => img.repo_tags).filter(t => t && t !== "<none>:<none>")
        setLocalImages(tags)
      } catch { /* ignore */ }
    }
    fetchImages()
  }, [])

  // Close image dropdown on click outside
  useEffect(() => {
    const handleClick = (e: MouseEvent) => {
      if (imageDropdownRef.current && !imageDropdownRef.current.contains(e.target as Node) &&
          imageInputRef.current && !imageInputRef.current.contains(e.target as Node)) {
        setShowImageDropdown(false)
      }
    }
    document.addEventListener("mousedown", handleClick)
    return () => document.removeEventListener("mousedown", handleClick)
  }, [])

  const filteredImages = localImages.filter(img =>
    img.toLowerCase().includes(runImage.toLowerCase())
  )

  const handleRun = async () => {
    if (!runImage.trim()) return
    const image = runImage
    const name = runName
    const cpus = runCpus
    const mem = runMem
    const pull = runPull
    const envStrings = runEnvVars
      .filter(e => e.key.trim())
      .map(e => `${e.key}=${e.value}`)
    const env = envStrings.length > 0 ? envStrings : undefined

    // Close modal, show inline creating card
    setShowRunModal(false)
    setCreating(true)
    setCreateFailed("")
    setCreateImageName(name || image)
    setCreateStatus(pull ? t("pullingImage") : t("creatingContainer"))

    try {
      if (pull) setCreateStatus(t("pullingImage"))
      await onRun(image, name, cpus, mem, pull, env)
      // Container created — fetchContainers already called by onRun, list will update
      setCreating(false)
    } catch (e) {
      setCreateFailed(String(e))
      setCreateStatus(String(e))
      setTimeout(() => {
        setCreating(false)
        setCreateFailed("")
      }, 6000)
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
    return (
      <div className="flex items-center justify-center gap-2 py-20 text-muted-foreground">
        <Spinner />
        {t("loadingContainers")}
      </div>
    )
  }
  if (error) {
    const setupMode = Boolean(runtimeMissing && onSetupRuntime)
    return (
      <ErrorBanner
        title={t("connectionError")}
        message={error}
        actionLabel={setupMode ? t("dockerOneClickSetup") : t("refresh")}
        actionDisabled={Boolean(setupMode && settingUpRuntime)}
        onAction={setupMode ? onSetupRuntime : onFetch}
      />
    )
  }

  const renderCard = (c: ContainerInfo, opts?: { child?: boolean }) => {
    const isRunning = c.state === "running"
    const name = c.name || c.id
    const testKey = name.replace(/[^a-zA-Z0-9_-]/g, "-")
    const stats = isRunning ? containerStats[c.id] : undefined

    const disabled = acting === c.id
    return (
      <Card
        key={c.id}
        data-testid={`container-card-${testKey}`}
        data-container-id={c.id}
        data-container-name={name}
        className={cn("py-0", opts?.child && "ml-6")}
      >
        <CardContent className="py-4">
          <div className="flex items-start justify-between gap-4">
            <div className="flex min-w-0 items-start gap-3">
              <div
                className={cn(
                  "size-10 shrink-0 rounded-lg flex items-center justify-center",
                  iconStroke,
                  "[&_svg]:size-[18px]",
                  isRunning ? "bg-primary/10 text-primary" : "bg-muted text-muted-foreground"
                )}
              >
                {I.box}
              </div>

              <div className="min-w-0">
                <div className="flex items-center gap-2">
                  <div className="font-semibold text-foreground truncate">{name}</div>
                  <Badge
                    variant="secondary"
                    data-testid={`container-status-${testKey}`}
                    className={cn(
                      "rounded-md px-2 py-0.5 text-[11px] border",
                      isRunning
                        ? "border-brand-green/15 bg-brand-green/10 text-brand-green"
                        : "border-border/60 bg-popover/40 text-muted-foreground"
                    )}
                  >
                    {isRunning ? t("running") : t("stopped")}
                  </Badge>
                </div>

                <div className="mt-1 flex flex-wrap items-center gap-2 text-xs text-muted-foreground">
                  <span className="truncate">{c.image}</span>
                  {isRunning && c.ports ? (
                    <>
                      <span className="text-muted-foreground/50">•</span>
                      <span className="font-mono text-brand-cyan">{c.ports}</span>
                    </>
                  ) : (
                    <>
                      <span className="text-muted-foreground/50">•</span>
                      <span className="font-mono">{c.id.slice(0, 12)}</span>
                    </>
                  )}
                </div>
              </div>
            </div>

            <div className="flex flex-col items-end gap-2">
              {isRunning && stats && (
                <div className="flex items-center gap-3 text-xs text-muted-foreground">
                  <span className="inline-flex items-center gap-1">
                    <span className={cn(iconStroke, "[&_svg]:size-3")}>{I.cpu}</span>
                    <span className="font-mono">{stats.cpu_percent.toFixed(1)}%</span>
                  </span>
                  <span className="inline-flex items-center gap-1">
                    <span className={cn(iconStroke, "[&_svg]:size-3")}>{I.memory}</span>
                    <span className="font-mono">{stats.memory_usage_mb.toFixed(0)} MB</span>
                  </span>
                </div>
              )}

              <div className="inline-flex items-center gap-2 text-xs text-muted-foreground">
                <span
                  className={cn(
                    "size-1.5 rounded-full",
                    isRunning
                      ? "bg-brand-green shadow-[0_0_10px_hsl(var(--brand-green)/0.6)]"
                      : "bg-destructive"
                  )}
                />
                <span>{c.status}</span>
              </div>
            </div>
          </div>

          <Separator className="my-3" />

          <div className="flex flex-wrap items-center justify-between gap-2">
            <div className="flex flex-wrap gap-2">
              <Button
                type="button"
                size="xs"
                variant="secondary"
                data-testid={`container-login-${testKey}`}
                className={cardActionSecondary}
                disabled={disabled}
                title={t("loginCommand")}
                onClick={async () => {
                  const target = c.name || c.id
                  const cmd = await invoke<string>("container_login_cmd", { container: target, shell: "/bin/sh" })
                  onOpenTextModal(t("loginCommand"), cmd, cmd)
                }}
              >
                <span className={cn(iconStroke, "[&_svg]:size-3")}>{I.key}</span>
                {t("loginCommand")}
              </Button>

              <Button
                type="button"
                size="xs"
                variant="secondary"
                className={cardActionSecondary}
                disabled={disabled}
                title={t("viewLogs")}
                onClick={() => openLogModal(c)}
              >
                <span className={cn(iconStroke, "[&_svg]:size-3")}>{I.fileText}</span>
                {t("logs")}
              </Button>

              <Button
                type="button"
                size="xs"
                variant="secondary"
                data-testid={`container-env-${testKey}`}
                className={cardActionSecondary}
                disabled={disabled}
                title={t("viewEnvVars")}
                onClick={() => openEnvModal(c)}
              >
                <span className={cn(iconStroke, "[&_svg]:size-3")}>{I.settings}</span>
                {t("envVars")}
              </Button>

              {isRunning && (
                <Button
                  type="button"
                  size="xs"
                  variant="secondary"
                  className={cardActionSecondary}
                  disabled={disabled}
                  title={t("execCommand")}
                  onClick={() => openExecModal(c)}
                >
                  <span className={cn(iconStroke, "[&_svg]:size-3")}>{I.terminal}</span>
                  {t("execCommand")}
                </Button>
              )}

              <Button
                type="button"
                size="xs"
                variant="secondary"
                className={cardActionSecondary}
                disabled={disabled}
                title={t("packageImage")}
                onClick={() => {
                  const target = c.name || c.id
                  const defaultTag = `${(c.image || "image").split(":")[0]}-snapshot:latest`
                  onOpenPackageModal(target, defaultTag)
                }}
              >
                <span className={cn(iconStroke, "[&_svg]:size-3")}>{I.layers}</span>
                {t("package")}
              </Button>
            </div>

            <div className="flex flex-wrap gap-2">
              {isRunning ? (
                <Button
                  type="button"
                  size="xs"
                  variant="outline"
                  data-testid={`container-stop-${testKey}`}
                  className={cardActionOutline}
                  disabled={disabled}
                  title={t("stop")}
                  onClick={() => onContainerAction("stop_container", c.id)}
                >
                  <span className={cn(iconStroke, "[&_svg]:size-3")}>{I.stop}</span>
                  {t("stop")}
                </Button>
              ) : (
                <Button
                  type="button"
                  size="xs"
                  variant="outline"
                  data-testid={`container-start-${testKey}`}
                  className={cardActionOutline}
                  disabled={disabled}
                  title={t("start")}
                  onClick={() => onContainerAction("start_container", c.id)}
                >
                  <span className={cn(iconStroke, "[&_svg]:size-3")}>{I.play}</span>
                  {t("start")}
                </Button>
              )}

              <Button
                type="button"
                size="xs"
                variant="destructive"
                data-testid={`container-delete-${testKey}`}
                disabled={disabled}
                title={t("delete")}
                onClick={() => {
                  setConfirmRemove(c.id)
                  setContainerToRemoveName(c.name || c.id)
                }}
              >
                <span className={cn(iconStroke, "[&_svg]:size-3")}>{I.trash}</span>
                {t("delete")}
              </Button>
            </div>
          </div>
        </CardContent>
      </Card>
    )
  }

  return (
    <div className="space-y-4">
      <div className="flex items-center justify-end gap-2">
        <Button type="button" variant="outline" size="sm" disabled={refreshing} onClick={() => {
          setRefreshing(true)
          onFetch()
          setTimeout(() => setRefreshing(false), 600)
        }}>
          <span className={cn(iconStroke, "[&_svg]:size-4", refreshing && "animate-spin")}>{I.refresh}</span>
          {t("refresh")}
        </Button>
        <Button type="button" onClick={openRunModal} data-testid="containers-run">
          <span className={cn(iconStroke, "[&_svg]:size-4")}>{I.plus}</span>
          {t("runNewContainer")}
        </Button>
      </div>

      <div className="space-y-3">
        {/* Inline creating card */}
        {creating && (
          <Card className={cn("py-0", createFailed && "border-destructive/30")}>
            <CardContent className="py-3 flex items-start justify-between gap-3">
              <div className="flex items-start gap-3">
                <div
                  className={cn(
                    "mt-0.5 size-9 rounded-lg flex items-center justify-center",
                    createFailed ? "bg-destructive/10 text-destructive" : "bg-primary/10 text-primary"
                  )}
                >
                  {createFailed ? (
                    <span className={cn(iconStroke, "[&_svg]:size-4")}>{I.alertCircle}</span>
                  ) : (
                    <Spinner className="size-4 border-primary/30 border-t-primary" />
                  )}
                </div>

                <div className="min-w-0">
                  <div className="text-sm font-semibold text-foreground truncate">
                    {createImageName}
                  </div>
                  <div
                    className={cn(
                      "mt-0.5 text-xs whitespace-pre-wrap",
                      createFailed ? "text-destructive/90" : "text-muted-foreground"
                    )}
                  >
                    {createStatus}
                  </div>
                  {createFailed && (
                    <div className="mt-2 text-xs text-muted-foreground whitespace-pre-wrap">
                      {createFailed}
                    </div>
                  )}
                </div>
              </div>

              {createFailed && (
                <Button
                  type="button"
                  variant="ghost"
                  size="icon-xs"
                  className="hover:bg-destructive/10 hover:text-destructive"
                  onClick={() => {
                    setCreating(false)
                    setCreateFailed("")
                  }}
                  aria-label={t("close")}
                >
                  ×
                </Button>
              )}
            </CardContent>
          </Card>
        )}
      {groups.length === 0 && !creating ? (
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
            return (
              <Collapsible key={g.key} open={expanded} onOpenChange={() => onToggleGroup(g.key)}>
                <Card className="py-0 gap-0">
                  <CardContent className="px-0">
                    <CollapsibleTrigger asChild>
                      <button
                        type="button"
                        title={expanded ? "Collapse" : "Expand"}
                        className="w-full px-4 py-3 flex items-center justify-between gap-3 text-left hover:bg-accent/30 transition-colors rounded-xl"
                      >
                        <div className="flex items-start gap-3 min-w-0">
                          <div className="mt-0.5 size-9 rounded-lg bg-primary/10 text-primary flex items-center justify-center">
                            <span className={cn(iconStroke, "[&_svg]:size-4")}>{I.layers}</span>
                          </div>
                          <div className="min-w-0">
                            <div className="text-sm font-semibold text-foreground truncate">
                              {g.key}
                            </div>
                            <div className="mt-0.5 text-xs text-muted-foreground">
                              {g.containers.length} {t("containers")}
                            </div>
                          </div>
                        </div>

                        <div className="flex items-center gap-3">
                          <Badge
                            variant="secondary"
                            className={cn(
                              "rounded-full gap-2 px-3 py-1 text-xs font-medium border border-border/60 bg-popover/40",
                              g.runningCount > 0 ? "text-brand-green" : "text-muted-foreground"
                            )}
                          >
                            <span
                              className={cn(
                                "size-1.5 rounded-full",
                                g.runningCount > 0 ? "bg-brand-green" : "bg-muted-foreground/70"
                              )}
                            />
                            {g.runningCount} {t("running")}
                          </Badge>

                          {g.stoppedCount > 0 && (
                            <Badge
                              variant="secondary"
                              className="rounded-full gap-2 px-3 py-1 text-xs font-medium border border-border/60 bg-popover/40 text-muted-foreground"
                            >
                              <span className="size-1.5 rounded-full bg-muted-foreground/70" />
                              {g.stoppedCount} {t("stopped")}
                            </Badge>
                          )}

                          <span className={cn(iconStroke, "text-muted-foreground [&_svg]:size-4")}>
                            {expanded ? I.chevronDown : I.chevronRight}
                          </span>
                        </div>
                      </button>
                    </CollapsibleTrigger>
                  </CardContent>
                </Card>

                <CollapsibleContent className="mt-2 space-y-3">
                  {g.containers.map(c => renderCard(c, { child: true }))}
                </CollapsibleContent>
              </Collapsible>
            )
          })
        )}
      </div>

      {/* Run Container Modal */}
      <Dialog open={showRunModal} onOpenChange={setShowRunModal}>
        <DialogContent className="sm:max-w-[760px]" data-testid="containers-dialog-run">
          <DialogHeader>
            <DialogTitle className="flex items-center gap-2">
              <span className={cn(iconStroke, "text-primary [&_svg]:size-4")}>{I.play}</span>
              {t("runContainer")}
            </DialogTitle>
            <DialogDescription>{t("runContainerTip")}</DialogDescription>
          </DialogHeader>

          <div className="space-y-6">
            <div className="grid gap-4 sm:grid-cols-2">
              <div className="sm:col-span-1">
                <label className="text-xs font-semibold text-muted-foreground">
                  {t("image")} <span className="text-destructive">*</span>
                </label>
                <div className="relative mt-1">
                  <Input
                    ref={imageInputRef}
                    value={runImage}
                    onChange={(e) => {
                      setRunImage(e.target.value)
                      setShowImageDropdown(true)
                    }}
                    onFocus={() => setShowImageDropdown(true)}
                    placeholder="nginx:latest"
                    autoFocus
                  />

                  {showImageDropdown && filteredImages.length > 0 && (
                    <div
                      ref={imageDropdownRef}
                      className="absolute z-50 mt-1 w-full overflow-hidden rounded-md border bg-popover text-popover-foreground shadow-md"
                    >
                      <ScrollArea className="max-h-56">
                        <div className="p-1">
                          {filteredImages.slice(0, 8).map((img) => (
                            <button
                              key={img}
                              type="button"
                              className="w-full text-left rounded-sm px-2 py-1.5 text-sm hover:bg-accent hover:text-accent-foreground"
                              onClick={() => {
                                setRunImage(img)
                                setShowImageDropdown(false)
                              }}
                            >
                              {img}
                            </button>
                          ))}
                        </div>
                      </ScrollArea>
                    </div>
                  )}
                </div>
              </div>

              <div className="sm:col-span-1">
                <label className="text-xs font-semibold text-muted-foreground">
                  {t("nameOptional")}
                </label>
                <Input
                  value={runName}
                  onChange={(e) => setRunName(e.target.value)}
                  placeholder="my-container"
                  className="mt-1"
                />
              </div>
            </div>

            <Separator />

            <div className="grid gap-4 sm:grid-cols-3">
              <div>
                <label className="text-xs font-semibold text-muted-foreground">{t("cpus")}</label>
                <Input
                  type="number"
                  min={1}
                  value={runCpus}
                  onChange={(e) =>
                    setRunCpus(e.target.value === "" ? "" : Number(e.target.value))
                  }
                  placeholder="—"
                  className="mt-1"
                />
              </div>
              <div>
                <label className="text-xs font-semibold text-muted-foreground">{t("memoryMb")}</label>
                <Input
                  type="number"
                  min={64}
                  value={runMem}
                  onChange={(e) =>
                    setRunMem(e.target.value === "" ? "" : Number(e.target.value))
                  }
                  placeholder="—"
                  className="mt-1"
                />
              </div>
              <div className="flex items-end">
                <label className="flex items-center gap-2 text-sm text-muted-foreground">
                  <Checkbox checked={runPull} onCheckedChange={(v) => setRunPull(v === true)} />
                  <span>{t("pullBeforeRun")}</span>
                </label>
              </div>
            </div>

            <Separator />

            <div className="space-y-3">
              <div className="flex items-start justify-between gap-3">
                <div>
                  <div className="text-sm font-semibold flex items-center gap-2">
                    <span className={cn(iconStroke, "[&_svg]:size-4")}>{I.settings}</span>
                    {t("envVars")}
                  </div>
                  <div className="text-xs text-muted-foreground">{t("envVarsHint")}</div>
                </div>
                <Button
                  type="button"
                  variant="outline"
                  size="xs"
                  data-testid="containers-run-add-env"
                  onClick={() => setRunEnvVars([...runEnvVars, { key: "", value: "" }])}
                >
                  <span className={cn(iconStroke, "[&_svg]:size-3")}>{I.plus}</span>
                  {t("addEnvVar")}
                </Button>
              </div>

              {runEnvVars.length > 0 ? (
                <div className="space-y-2">
                  {runEnvVars.map((env, i) => (
                    <div
                      key={i}
                      className="grid grid-cols-[1fr_auto_1fr_auto] items-center gap-2"
                    >
                      <Input
                        data-testid={`containers-run-env-key-${i}`}
                        value={env.key}
                        onChange={(e) => {
                          const updated = [...runEnvVars]
                          updated[i] = { ...updated[i], key: e.target.value }
                          setRunEnvVars(updated)
                        }}
                        placeholder={t("envKey")}
                      />
                      <span className="text-xs text-muted-foreground">=</span>
                      <Input
                        data-testid={`containers-run-env-value-${i}`}
                        value={env.value}
                        onChange={(e) => {
                          const updated = [...runEnvVars]
                          updated[i] = { ...updated[i], value: e.target.value }
                          setRunEnvVars(updated)
                        }}
                        placeholder={t("envValue")}
                      />
                      <Button
                        type="button"
                        variant="ghost"
                        size="icon-xs"
                        className="hover:bg-destructive/10 hover:text-destructive"
                        onClick={() => setRunEnvVars(runEnvVars.filter((_, j) => j !== i))}
                        title={t("removeEnvVar")}
                        aria-label={t("removeEnvVar")}
                      >
                        <span className={cn(iconStroke, "[&_svg]:size-3")}>{I.trash}</span>
                      </Button>
                    </div>
                  ))}
                </div>
              ) : (
                <div className="rounded-md border bg-muted/40 p-3 text-xs text-muted-foreground">
                  {t("noEnvVars")}
                </div>
              )}
            </div>
          </div>

          <DialogFooter>
            <Button type="button" variant="outline" data-testid="containers-run-close" onClick={() => setShowRunModal(false)}>
              {t("close")}
            </Button>
            <Button type="button" data-testid="containers-run-submit" disabled={!runImage.trim()} onClick={handleRun}>
              <span className={cn(iconStroke, "[&_svg]:size-4")}>{I.play}</span>
              {t("create")}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      {/* Env Viewer Modal */}
      <Dialog open={showEnvModal} onOpenChange={setShowEnvModal}>
        <DialogContent className="sm:max-w-[720px]" data-testid="containers-dialog-env">
          <DialogHeader>
            <DialogTitle className="flex items-center gap-2">
              <span className={cn(iconStroke, "text-primary [&_svg]:size-4")}>{I.settings}</span>
              {t("envVars")} — {envContainerName}
            </DialogTitle>
          </DialogHeader>

          {envError && (
            <div className="rounded-md border border-destructive/30 bg-destructive/5 p-3 text-sm text-destructive whitespace-pre-wrap">
              {envError}
            </div>
          )}

          {envLoading ? (
            <div className="flex items-center justify-center gap-2 py-16 text-muted-foreground">
              <Spinner />
              {t("loading")}
            </div>
          ) : envVars.length === 0 ? (
            <div className="py-16 text-center text-sm text-muted-foreground">
              {t("noEnvVars")}
            </div>
          ) : (
            <div className="rounded-md border">
              <ScrollArea className="max-h-[360px]">
                <Table>
                  <TableHeader>
                    <TableRow>
                      <TableHead className="w-[220px]">{t("envKey")}</TableHead>
                      <TableHead>{t("envValue")}</TableHead>
                    </TableRow>
                  </TableHeader>
                  <TableBody>
                    {envVars.map((ev, i) => (
                      <TableRow key={i}>
                        <TableCell className="font-mono text-xs font-semibold break-all">
                          {ev.key}
                        </TableCell>
                        <TableCell className="font-mono text-xs break-all">
                          {ev.value}
                        </TableCell>
                      </TableRow>
                    ))}
                  </TableBody>
                </Table>
              </ScrollArea>
            </div>
          )}

          <DialogFooter>
            <Button type="button" variant="outline" data-testid="containers-env-close" onClick={() => setShowEnvModal(false)}>
              {t("close")}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      {/* Log Viewer Modal */}
      <Dialog open={showLogModal} onOpenChange={setShowLogModal}>
        <DialogContent className="sm:max-w-[820px]" data-testid="containers-dialog-logs">
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            <span className={cn(iconStroke, "text-primary [&_svg]:size-4")}>{I.fileText}</span>
            {t("logs")} — {logContainerName}
            {logFollowing && (
              <Badge
                variant="secondary"
                className="ml-2 rounded-full border border-brand-green/20 bg-brand-green/10 text-brand-green"
              >
                {t("live")}
              </Badge>
            )}
          </DialogTitle>
          <DialogDescription className="sr-only">{t("viewLogs")}</DialogDescription>
        </DialogHeader>

          <div className="flex flex-wrap items-center gap-3">
            <div className="flex items-center gap-2">
              <span className="text-xs font-semibold text-muted-foreground">{t("tailLines")}</span>
              <Select
                value={logTail}
                onValueChange={(value) => {
                  setLogTail(value)
                  fetchLogs(logContainerId, value, logTimestamps)
                }}
                disabled={logFollowing}
              >
                <SelectTrigger size="sm" className="w-[110px]">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="100">100</SelectItem>
                  <SelectItem value="200">200</SelectItem>
                  <SelectItem value="500">500</SelectItem>
                  <SelectItem value="all">All</SelectItem>
                </SelectContent>
              </Select>
            </div>

            <label className="flex items-center gap-2 text-sm text-muted-foreground">
              <Checkbox
                checked={logTimestamps}
                disabled={logFollowing}
                onCheckedChange={(v) => {
                  const next = v === true
                  setLogTimestamps(next)
                  fetchLogs(logContainerId, logTail, next)
                }}
              />
              <span>{t("showTimestamps")}</span>
            </label>

            <div className="flex-1" />

            <Button
              type="button"
              size="sm"
              variant={logFollowing ? "default" : "outline"}
              onClick={() => {
                if (logFollowing) {
                  stopLogStream()
                } else {
                  startLogStream(logContainerId, logTimestamps)
                }
              }}
            >
              {logFollowing ? t("stopFollowing") : t("follow")}
            </Button>

            <Button
              type="button"
              size="sm"
              variant="outline"
              disabled={logLoading || logFollowing}
              onClick={() => fetchLogs(logContainerId, logTail, logTimestamps)}
            >
              <span className={cn(iconStroke, "[&_svg]:size-4")}>{I.refresh}</span>
              {t("refreshLogs")}
            </Button>
          </div>

          {logFollowing && (
            <div className="flex items-center gap-2 rounded-md border bg-muted/40 px-3 py-2 text-xs text-muted-foreground">
              <Spinner className="size-3" />
              {t("streamingLogs")}
            </div>
          )}

          {logStreamEnded && !logFollowing && (
            <div className="rounded-md border bg-muted/40 px-3 py-2 text-xs text-muted-foreground">
              {t("logStreamEnded")}
            </div>
          )}

          {logError && (
            <div className="rounded-md border border-destructive/30 bg-destructive/5 px-3 py-2 text-xs text-destructive whitespace-pre-wrap">
              {logError}
            </div>
          )}

          <div className="rounded-md border bg-muted/20">
            <ScrollArea className="h-[420px]">
              {logLoading && !logContent ? (
                <div className="flex items-center justify-center gap-2 py-20 text-muted-foreground">
                  <Spinner />
                  {t("loading")}
                </div>
              ) : logContent ? (
                <div className="p-3">
                  <pre className="text-xs font-mono whitespace-pre break-words">{logContent}</pre>
                  <div ref={logEndRef} />
                </div>
              ) : (
                <div className="py-20 text-center text-sm text-muted-foreground">{t("noLogs")}</div>
              )}
            </ScrollArea>
          </div>

          <DialogFooter>
            <Button type="button" variant="outline" onClick={() => setShowLogModal(false)}>
              {t("close")}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      {/* Exec Terminal Modal */}
      <Dialog open={!!execContainer} onOpenChange={(open) => !open && setExecContainer(null)}>
        <DialogContent className="sm:max-w-[860px]" data-testid="containers-dialog-exec">
          <DialogHeader>
            <DialogTitle className="flex items-center gap-2">
              <span className={cn(iconStroke, "text-primary [&_svg]:size-4")}>{I.terminal}</span>
              {t("terminal")} — {execContainer?.name || execContainer?.id}
            </DialogTitle>
          </DialogHeader>

          <div className="flex flex-wrap items-center gap-2">
            <Input
              value={execCmd}
              onChange={(e) => setExecCmd(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter") handleExec()
              }}
              placeholder={t("commandPlaceholder")}
              disabled={execRunning}
              autoFocus
              className="min-w-[240px] flex-1"
            />
            <Button type="button" disabled={execRunning || !execCmd.trim()} onClick={handleExec}>
              {execRunning ? t("working") : t("runCommand")}
            </Button>
            <Button
              type="button"
              variant="outline"
              onClick={() => setExecHistory([])}
              disabled={execRunning}
              title={t("clear")}
            >
              {t("clear")}
            </Button>
          </div>

          <div className="rounded-md border bg-muted/20">
            <div ref={execOutputRef} className="h-[360px] overflow-auto p-3">
              {execHistory.length === 0 ? (
                <div className="text-sm text-muted-foreground">{t("commandPlaceholder")}</div>
              ) : (
                <div className="space-y-4">
                  {execHistory.map((entry, i) => (
                    <div key={i} className="space-y-2">
                      <div className="text-xs font-mono text-muted-foreground">
                        <span className="text-foreground">$</span> {entry.command}
                      </div>
                      <pre
                        className={cn(
                          "text-xs font-mono whitespace-pre-wrap break-words",
                          entry.isError ? "text-destructive" : "text-foreground"
                        )}
                      >
                        {entry.output}
                      </pre>
                    </div>
                  ))}
                  {execRunning && (
                    <div className="text-xs font-mono text-muted-foreground">...</div>
                  )}
                </div>
              )}
            </div>
          </div>

          <div className="flex items-center justify-between gap-3 rounded-md border bg-muted/30 px-3 py-2">
            <code className="text-xs font-mono text-muted-foreground break-all">
              {execInteractiveCmd}
            </code>
            <Button
              type="button"
              variant="outline"
              size="xs"
              onClick={() => navigator.clipboard.writeText(execInteractiveCmd)}
              title={t("copyExecCmd")}
            >
              <span className={cn(iconStroke, "[&_svg]:size-3")}>{I.copy}</span>
              {t("copy")}
            </Button>
          </div>

          <DialogFooter>
            <Button type="button" variant="outline" onClick={() => setExecContainer(null)}>
              {t("close")}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      {/* Confirm Remove Container Modal */}
      <AlertDialog open={!!confirmRemove} onOpenChange={(open) => !open && setConfirmRemove("")}>
        <AlertDialogContent size="sm" data-testid="containers-dialog-remove">
          <AlertDialogHeader>
            <AlertDialogTitle>{t("removeContainer")}</AlertDialogTitle>
            <AlertDialogDescription className="space-y-2">
              <span className="block">{t("confirmRemoveContainer")}</span>
              <span className="block font-semibold text-foreground break-all">{containerToRemoveName}</span>
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>{t("close")}</AlertDialogCancel>
            <AlertDialogAction
              variant="destructive"
              data-testid="containers-remove-confirm"
              onClick={() => {
                onContainerAction("remove_container", confirmRemove)
                setConfirmRemove("")
              }}
            >
              {t("remove")}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </div>
  )
}
