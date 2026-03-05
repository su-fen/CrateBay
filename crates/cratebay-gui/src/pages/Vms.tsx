import { useEffect, useMemo, useRef, useState } from "react"
import { invoke } from "@tauri-apps/api/core"
import { I } from "../icons"
import { ErrorInline } from "../components/ErrorDisplay"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card"
import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import { Input } from "@/components/ui/input"
import { Progress } from "@/components/ui/progress"
import { ScrollArea } from "@/components/ui/scroll-area"
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select"
import { Separator } from "@/components/ui/separator"
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table"
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs"
import { Checkbox } from "@/components/ui/checkbox"
import { cn } from "@/lib/utils"
import { iconStroke, cardActionOutline, cardActionGhost, cardActionDanger } from "@/lib/styles"
import type {
  OsImageDownloadProgressDto,
  OsImageDto,
  VmInfoDto,
  VmStats,
} from "../types"

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
  onCreateVm: () => Promise<boolean>
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


const NONE_OS_IMAGE = "__none__"

function formatBytes(bytes: number): string {
  if (bytes >= 1024 * 1024 * 1024) return `${(bytes / (1024 * 1024 * 1024)).toFixed(1)} GB`
  if (bytes >= 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)} MB`
  if (bytes >= 1024) return `${(bytes / 1024).toFixed(1)} KB`
  return `${bytes} B`
}

export function Vms({
  vms,
  vmLoading,
  vmError,
  setVmError,
  vmName,
  setVmName,
  vmCpus,
  setVmCpus,
  vmMem,
  setVmMem,
  vmDisk,
  setVmDisk,
  vmRosetta,
  setVmRosetta,
  vmActing,
  vmLoginUser,
  setVmLoginUser,
  vmLoginHost,
  setVmLoginHost,
  vmLoginPort,
  setVmLoginPort,
  mountVmId,
  setMountVmId,
  mountTag,
  setMountTag,
  mountHostPath,
  setMountHostPath,
  mountGuestPath,
  setMountGuestPath,
  mountReadonly,
  setMountReadonly,
  pfVmId,
  setPfVmId,
  pfHostPort,
  setPfHostPort,
  pfGuestPort,
  setPfGuestPort,
  pfProtocol,
  setPfProtocol,
  onFetchVms,
  onVmAction,
  onCreateVm,
  onLoginCmd,
  onAddMount,
  onRemoveMount,
  osImages,
  selectedOsImage,
  setSelectedOsImage,
  downloadingImage,
  downloadProgress,
  onDownloadOsImage,
  onDeleteOsImage,
  onAddPortForward,
  onRemovePortForward,
  t,
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

  // VM stats state
  const [vmStatsMap, setVmStatsMap] = useState<Record<string, VmStats>>({})

  const runningVms = useMemo(() => vms.filter((v) => v.state === "running"), [vms])
  const selectedOsValue = selectedOsImage ? selectedOsImage : NONE_OS_IMAGE
  const readyOsImages = useMemo(() => osImages.filter((img) => img.status === "ready"), [osImages])
  const sortedVms = useMemo(() => {
    const list = [...vms]
    list.sort((a, b) => (a.name || a.id).localeCompare(b.name || b.id))
    return list
  }, [vms])

  const activeDownloadPct = useMemo(() => {
    if (!downloadProgress?.bytes_total) return 0
    return Math.round((downloadProgress.bytes_downloaded / downloadProgress.bytes_total) * 100)
  }, [downloadProgress])

  const openCreate = () => {
    setVmError("")
    setShowCreateModal(true)
  }

  const closeCreate = () => {
    setShowCreateModal(false)
  }

  const toggleExpanded = (vm: VmInfoDto) => {
    setExpandedVmId((prev) => (prev === vm.id ? null : vm.id))
    setActiveTab("info")
    setMountVmId(vm.id)
    setPfVmId(vm.id)
  }

  const handleTabChange = (vmId: string, value: string) => {
    const next = value as typeof activeTab
    setActiveTab(next)
    if (next === "mounts") setMountVmId(vmId)
    if (next === "ports") setPfVmId(vmId)
  }

  const handleAddMount = async (vmId: string) => {
    setMountVmId(vmId)
    await onAddMount()
  }

  const handleAddPortForward = async (vmId: string) => {
    setPfVmId(vmId)
    await onAddPortForward()
  }

  const handleCreate = async () => {
    if (!vmName.trim()) return
    setVmError("")
    const ok = await onCreateVm()
    if (ok) closeCreate()
  }

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

  // Poll console data when modal is open
  useEffect(() => {
    if (!consoleVmId) return
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
          setConsoleData((prev) => prev + data)
          currentOffset = newOffset
        }
      } catch {
        // ignore
      }
    }

    poll()
    const iv = setInterval(poll, 1500)
    return () => {
      cancelled = true
      clearInterval(iv)
    }
  }, [consoleVmId])

  // Auto-scroll when new data arrives
  useEffect(() => {
    if (autoScroll && consoleEndRef.current) {
      consoleEndRef.current.scrollIntoView({ behavior: "smooth" })
    }
  }, [consoleData, autoScroll])

  // Poll VM stats for running VMs
  useEffect(() => {
    if (runningVms.length === 0) return

    let cancelled = false
    const poll = async () => {
      const results: Record<string, VmStats> = {}
      await Promise.all(
        runningVms.map(async (vm) => {
          try {
            const stats = await invoke<VmStats>("vm_stats", { id: vm.id })
            results[vm.id] = stats
          } catch {
            // ignore stats errors
          }
        })
      )
      if (!cancelled) setVmStatsMap(results)
    }

    poll()
    const iv = setInterval(poll, 2000)
    return () => {
      cancelled = true
      clearInterval(iv)
    }
  }, [runningVms])

  return (
    <div className="space-y-4">
      <div className="flex items-center gap-2">
        <Button type="button" onClick={openCreate} data-testid="vms-create">
          <span className={cn("mr-1", iconStroke, "[&_svg]:size-4")}>{I.plus}</span>
          {t("createVm")}
        </Button>
        <Button type="button" variant="outline" onClick={onFetchVms}>
          <span className={cn("mr-1", iconStroke, "[&_svg]:size-4")}>{I.refresh}</span>
          {t("refresh")}
        </Button>
        <div className="flex-1" />
      </div>

      {vmError && <ErrorInline message={vmError} onDismiss={() => setVmError("")} />}

      {vmLoading ? (
        <div className="flex items-center justify-center gap-2 py-20 text-muted-foreground">
          <div className="size-4 rounded-full border-2 border-border border-t-primary animate-spin" />
          {t("loading")}
        </div>
      ) : vms.length === 0 ? (
        <div className="flex items-center justify-center py-28">
          <div className="flex flex-col items-center text-center gap-3">
            <div className={cn(
              "size-14 rounded-2xl bg-primary/10 text-primary flex items-center justify-center",
              iconStroke,
              "[&_svg]:size-7"
            )}>
              {I.server}
            </div>
            <div className="text-lg font-semibold text-foreground">{t("noVms")}</div>
            <div className="text-sm text-muted-foreground">{t("createFirstVm")}</div>
            <Button type="button" onClick={openCreate}>
              <span className={cn("mr-1", iconStroke, "[&_svg]:size-4")}>{I.plus}</span>
              {t("createVm")}
            </Button>
          </div>
        </div>
      ) : (
        <div className="rounded-lg border bg-muted/30 p-3 text-xs text-muted-foreground">
          {t("vmHint")}
        </div>
      )}

      {/* VM list */}
      {!vmLoading && vms.length > 0 && (
        <div className="space-y-3">
          {sortedVms.map((vm) => {
            const isRunning = vm.state === "running"
            const isExpanded = expandedVmId === vm.id
            const isActing = vmActing === vm.id
            const stats = vmStatsMap[vm.id]

            const memoryPct =
              stats && vm.memory_mb > 0
                ? Math.min((stats.memory_usage_mb / vm.memory_mb) * 100, 100)
                : 0
            const diskPct =
              stats && vm.disk_gb > 0
                ? Math.min((stats.disk_usage_gb / vm.disk_gb) * 100, 100)
                : 0

            return (
              <Card key={vm.id} className="py-0">
                <CardHeader className="border-b py-4">
                  <div className="flex items-start justify-between gap-3">
                    <div className="min-w-0">
                      <div className="flex items-center gap-2">
                        <div className="font-mono text-sm font-semibold text-foreground truncate max-w-[520px]">
                          {vm.name || vm.id}
                        </div>
                        <Badge
                          variant="secondary"
                          className={cn(
                            "rounded-md",
                            isRunning
                              ? "border-brand-green/20 bg-brand-green/10 text-brand-green"
                              : "border-border/70 bg-popover/40 text-muted-foreground"
                          )}
                        >
                          {isRunning ? t("running") : t("stopped")}
                        </Badge>
                        {vm.rosetta_enabled ? (
                          <Badge
                            variant="secondary"
                            className="rounded-md border border-primary/15 bg-primary/10 text-primary"
                          >
                            {t("rosettaOn")}
                          </Badge>
                        ) : (
                          <Badge variant="secondary" className="rounded-md px-1.5 py-0 text-[10px]">
                            {t("rosettaOff")}
                          </Badge>
                        )}
                      </div>

                      <div className="mt-1 flex flex-wrap items-center gap-2 text-xs text-muted-foreground">
                        <span className="font-mono">
                          {vm.cpus} {t("cpus")}
                        </span>
                        <span className="text-muted-foreground/40">•</span>
                        <span className="font-mono">
                          {vm.memory_mb} {t("memoryMb")}
                        </span>
                        <span className="text-muted-foreground/40">•</span>
                        <span className="font-mono">
                          {vm.disk_gb} {t("diskGb")}
                        </span>
                        {vm.os_image && (
                          <>
                            <span className="text-muted-foreground/40">•</span>
                            <span className="truncate max-w-[360px]">
                              {t("osImage")}:{" "}
                              <span className="font-mono text-foreground">{vm.os_image}</span>
                            </span>
                          </>
                        )}
                      </div>
                    </div>

                    <div className="flex items-center gap-1">
                      <Button
                        type="button"
                        variant="outline"
                        size="icon-xs"
                        disabled={isActing}
                        onClick={() => onVmAction(isRunning ? "vm_stop" : "vm_start", vm.id)}
                        title={isRunning ? t("stop") : t("start")}
                        className={cn(iconStroke, "[&_svg]:size-3", cardActionOutline)}
                      >
                        {isRunning ? I.stop : I.play}
                      </Button>
                      <Button
                        type="button"
                        variant="outline"
                        size="icon-xs"
                        onClick={() => onLoginCmd(vm)}
                        title={t("vmLogin")}
                        className={cn(iconStroke, "[&_svg]:size-3", cardActionOutline)}
                      >
                        {I.key}
                      </Button>
                      <Button
                        type="button"
                        variant="outline"
                        size="icon-xs"
                        onClick={() => openConsole(vm)}
                        title={t("console")}
                        className={cn(iconStroke, "[&_svg]:size-3", cardActionOutline)}
                      >
                        {I.terminal}
                      </Button>
                      <Button
                        type="button"
                        variant="outline"
                        size="icon-xs"
                        disabled={isActing}
                        onClick={() => onVmAction("vm_delete", vm.id)}
                        title={t("delete")}
                        className={cn(iconStroke, "[&_svg]:size-3", cardActionDanger)}
                      >
                        {I.trash}
                      </Button>
                      <Button
                        type="button"
                        variant="ghost"
                        size="icon-xs"
                        onClick={() => toggleExpanded(vm)}
                        title={isExpanded ? t("close") : "Details"}
                        className={cn(iconStroke, "[&_svg]:size-3", cardActionGhost)}
                      >
                        <span className={cn("transition-transform", isExpanded && "rotate-180")}>
                          {I.chevronDown}
                        </span>
                      </Button>
                    </div>
                  </div>
                </CardHeader>

                <CardContent className="py-4 space-y-4">
                  {isRunning && stats && (
                    <div className="grid grid-cols-1 gap-3 md:grid-cols-3">
                      <div className="space-y-1">
                        <div className="flex items-center justify-between text-xs text-muted-foreground">
                          <span>{t("cpuUsage")}</span>
                          <span className="font-mono text-foreground">
                            {stats.cpu_percent.toFixed(1)}%
                          </span>
                        </div>
                        <Progress value={Math.min(stats.cpu_percent, 100)} />
                      </div>
                      <div className="space-y-1">
                        <div className="flex items-center justify-between text-xs text-muted-foreground">
                          <span>{t("memoryUsage")}</span>
                          <span className="font-mono text-foreground">
                            {Math.round(stats.memory_usage_mb)} MB
                          </span>
                        </div>
                        <Progress value={memoryPct} />
                      </div>
                      <div className="space-y-1">
                        <div className="flex items-center justify-between text-xs text-muted-foreground">
                          <span>{t("diskUsage")}</span>
                          <span className="font-mono text-foreground">
                            {stats.disk_usage_gb.toFixed(1)} GB
                          </span>
                        </div>
                        <Progress value={diskPct} />
                      </div>
                    </div>
                  )}

                  {isExpanded && (
                    <Tabs value={activeTab} onValueChange={(v) => handleTabChange(vm.id, v)}>
                      <TabsList variant="line" className="w-full justify-start">
                        <TabsTrigger value="info">Info</TabsTrigger>
                        <TabsTrigger value="ssh">{t("vmLogin")}</TabsTrigger>
                        <TabsTrigger value="mounts">{t("mounts")}</TabsTrigger>
                        <TabsTrigger value="console">{t("console")}</TabsTrigger>
                        <TabsTrigger value="ports">{t("portForwarding")}</TabsTrigger>
                      </TabsList>

                      <TabsContent value="info" className="pt-3 space-y-3">
                        <div className="grid grid-cols-1 gap-3 md:grid-cols-2">
                          <div className="rounded-lg border bg-muted/30 p-3 space-y-1">
                            <div className="text-xs font-medium text-muted-foreground">{t("state")}</div>
                            <div className="text-sm font-semibold text-foreground">{vm.state}</div>
                          </div>
                          <div className="rounded-lg border bg-muted/30 p-3 space-y-1">
                            <div className="text-xs font-medium text-muted-foreground">{t("osImage")}</div>
                            <div className="text-sm font-mono text-foreground truncate">
                              {vm.os_image || t("osImageNone")}
                            </div>
                          </div>
                        </div>
                        <div className="grid grid-cols-1 gap-3 md:grid-cols-3">
                          <div className="rounded-lg border bg-muted/30 p-3 space-y-1">
                            <div className="text-xs font-medium text-muted-foreground">{t("cpus")}</div>
                            <div className="text-sm font-semibold text-foreground">{vm.cpus}</div>
                          </div>
                          <div className="rounded-lg border bg-muted/30 p-3 space-y-1">
                            <div className="text-xs font-medium text-muted-foreground">{t("memoryMb")}</div>
                            <div className="text-sm font-semibold text-foreground">{vm.memory_mb}</div>
                          </div>
                          <div className="rounded-lg border bg-muted/30 p-3 space-y-1">
                            <div className="text-xs font-medium text-muted-foreground">{t("diskGb")}</div>
                            <div className="text-sm font-semibold text-foreground">{vm.disk_gb}</div>
                          </div>
                        </div>
                      </TabsContent>

                      <TabsContent value="ssh" className="pt-3 space-y-3">
                        <div className="rounded-lg border bg-muted/30 p-3 text-xs text-muted-foreground">
                          {t("vmLoginHint")}
                        </div>
                        <div className="grid grid-cols-1 gap-3 md:grid-cols-3">
                          <div className="space-y-2">
                            <div className="text-xs font-medium text-muted-foreground">{t("user")}</div>
                            <Input value={vmLoginUser} onChange={(e) => setVmLoginUser(e.target.value)} />
                          </div>
                          <div className="space-y-2">
                            <div className="text-xs font-medium text-muted-foreground">{t("host")}</div>
                            <Input value={vmLoginHost} onChange={(e) => setVmLoginHost(e.target.value)} />
                          </div>
                          <div className="space-y-2">
                            <div className="text-xs font-medium text-muted-foreground">{t("port")}</div>
                            <Input
                              type="number"
                              value={vmLoginPort}
                              onChange={(e) =>
                                setVmLoginPort(e.target.value === "" ? "" : Number(e.target.value))
                              }
                            />
                          </div>
                        </div>
                        <div className="flex items-center gap-2">
                          <Button type="button" variant="outline" onClick={() => onLoginCmd(vm)}>
                            <span className={cn("mr-1", iconStroke, "[&_svg]:size-4")}>{I.key}</span>
                            {t("loginCommand")}
                          </Button>
                        </div>
                      </TabsContent>

                      <TabsContent value="mounts" className="pt-3 space-y-4">
                        <div className="rounded-lg border bg-muted/30 p-3 space-y-1">
                          <div className="text-xs font-semibold tracking-widest text-muted-foreground">
                            {t("virtiofs")}
                          </div>
                          <div className="text-xs text-muted-foreground">{t("virtiofsHint")}</div>
                          {isRunning && (
                            <div className="text-xs text-brand-cyan">{t("virtiofsRestartNotice")}</div>
                          )}
                        </div>

                        <div className="grid grid-cols-1 gap-3 md:grid-cols-2">
                          <div className="space-y-2">
                            <div className="text-xs font-medium text-muted-foreground">{t("tag")}</div>
                            <Input value={mountTag} onChange={(e) => setMountTag(e.target.value)} />
                          </div>
                          <div className="space-y-2">
                            <div className="text-xs font-medium text-muted-foreground">{t("hostPath")}</div>
                            <Input
                              value={mountHostPath}
                              onChange={(e) => setMountHostPath(e.target.value)}
                              placeholder="C:\\path\\to\\dir"
                            />
                          </div>
                          <div className="space-y-2 md:col-span-2">
                            <div className="text-xs font-medium text-muted-foreground">{t("guestPath")}</div>
                            <Input
                              value={mountGuestPath}
                              onChange={(e) => setMountGuestPath(e.target.value)}
                              placeholder="/mnt/host"
                            />
                          </div>
                          <div className="flex items-center gap-2 md:col-span-2">
                            <Checkbox
                              id={`mount-ro-${vm.id}`}
                              checked={mountReadonly}
                              onCheckedChange={(v) => setMountReadonly(Boolean(v))}
                            />
                            <label htmlFor={`mount-ro-${vm.id}`} className="text-sm text-foreground">
                              {t("readOnly")}
                            </label>
                            <div className="flex-1" />
                            <Badge variant="secondary" className="rounded-md px-1.5 py-0 text-[10px]">
                              {t("vm")}: {mountVmId || vm.id}
                            </Badge>
                          </div>
                          <div className="md:col-span-2">
                            <Button
                              type="button"
                              onClick={() => handleAddMount(vm.id)}
                              disabled={!mountTag.trim() || !mountHostPath.trim()}
                            >
                              <span className={cn("mr-1", iconStroke, "[&_svg]:size-4")}>{I.plus}</span>
                              {t("addMount")}
                            </Button>
                          </div>
                        </div>

                        <div className="rounded-lg border bg-muted/30 p-3 text-xs text-muted-foreground">
                          <span className="font-mono">{t("virtiofsGuestHint")}</span>
                        </div>

                        {vm.mounts.length > 0 && (
                          <Card className="py-0">
                            <CardHeader className="border-b py-3">
                              <CardTitle className="text-sm">{t("mounts")}</CardTitle>
                            </CardHeader>
                            <CardContent className="py-0">
                              <Table>
                                <TableHeader>
                                  <TableRow>
                                    <TableHead>{t("tag")}</TableHead>
                                    <TableHead>{t("hostPath")}</TableHead>
                                    <TableHead>{t("guestPath")}</TableHead>
                                    <TableHead>{t("readOnly")}</TableHead>
                                    <TableHead className="text-right">{t("actions")}</TableHead>
                                  </TableRow>
                                </TableHeader>
                                <TableBody>
                                  {vm.mounts.map((m) => (
                                    <TableRow key={m.tag}>
                                      <TableCell className="font-mono text-xs">{m.tag}</TableCell>
                                      <TableCell className="font-mono text-xs max-w-[360px] truncate">
                                        {m.host_path}
                                      </TableCell>
                                      <TableCell className="font-mono text-xs max-w-[260px] truncate">
                                        {m.guest_path}
                                      </TableCell>
                                      <TableCell className="text-xs">
                                        {m.read_only ? t("yes") : t("no")}
                                      </TableCell>
                                      <TableCell className="text-right">
                                        <Button
                                          type="button"
                                          variant="outline"
                                          size="xs"
                                          className={cardActionDanger}
                                          onClick={() => onRemoveMount(vm.id, m.tag)}
                                        >
                                          <span className={cn("mr-1", iconStroke, "[&_svg]:size-3")}>
                                            {I.trash}
                                          </span>
                                          {t("remove")}
                                        </Button>
                                      </TableCell>
                                    </TableRow>
                                  ))}
                                </TableBody>
                              </Table>
                            </CardContent>
                          </Card>
                        )}
                      </TabsContent>

                      <TabsContent value="console" className="pt-3 space-y-3">
                        <div className="rounded-lg border bg-muted/30 p-3 text-xs text-muted-foreground">
                          {t("vmConsole")}
                        </div>
                        <Button type="button" variant="outline" onClick={() => openConsole(vm)}>
                          <span className={cn("mr-1", iconStroke, "[&_svg]:size-4")}>{I.terminal}</span>
                          {t("console")}
                        </Button>
                      </TabsContent>

                      <TabsContent value="ports" className="pt-3 space-y-4">
                        <div className="rounded-lg border bg-muted/30 p-3 space-y-1">
                          <div className="text-xs font-semibold tracking-widest text-muted-foreground">
                            {t("portForwarding")}
                          </div>
                          <div className="text-xs text-muted-foreground">{t("portForwardHint")}</div>
                        </div>

                        <div className="grid grid-cols-1 gap-3 md:grid-cols-3">
                          <div className="space-y-2">
                            <div className="text-xs font-medium text-muted-foreground">{t("hostPort")}</div>
                            <Input
                              type="number"
                              value={pfHostPort}
                              onChange={(e) =>
                                setPfHostPort(e.target.value === "" ? "" : Number(e.target.value))
                              }
                            />
                          </div>
                          <div className="space-y-2">
                            <div className="text-xs font-medium text-muted-foreground">{t("guestPort")}</div>
                            <Input
                              type="number"
                              value={pfGuestPort}
                              onChange={(e) =>
                                setPfGuestPort(e.target.value === "" ? "" : Number(e.target.value))
                              }
                            />
                          </div>
                          <div className="space-y-2">
                            <div className="text-xs font-medium text-muted-foreground">{t("protocol")}</div>
                            <Select value={pfProtocol} onValueChange={(v) => setPfProtocol(v)}>
                              <SelectTrigger className="w-full justify-between">
                                <SelectValue />
                              </SelectTrigger>
                              <SelectContent>
                                <SelectItem value="tcp">tcp</SelectItem>
                                <SelectItem value="udp">udp</SelectItem>
                              </SelectContent>
                            </Select>
                          </div>

                          <div className="md:col-span-3 flex items-center gap-2">
                            <Badge variant="secondary" className="rounded-md px-1.5 py-0 text-[10px]">
                              {t("vm")}: {pfVmId || vm.id}
                            </Badge>
                            <div className="flex-1" />
                            <Button
                              type="button"
                              onClick={() => handleAddPortForward(vm.id)}
                              disabled={pfHostPort === "" || pfGuestPort === ""}
                            >
                              <span className={cn("mr-1", iconStroke, "[&_svg]:size-4")}>{I.plus}</span>
                              {t("addPortForward")}
                            </Button>
                          </div>
                        </div>

                        {vm.port_forwards.length > 0 && (
                          <Card className="py-0">
                            <CardHeader className="border-b py-3">
                              <CardTitle className="text-sm">{t("portForwarding")}</CardTitle>
                            </CardHeader>
                            <CardContent className="py-0">
                              <Table>
                                <TableHeader>
                                  <TableRow>
                                    <TableHead>{t("hostPort")}</TableHead>
                                    <TableHead>{t("guestPort")}</TableHead>
                                    <TableHead>{t("protocol")}</TableHead>
                                    <TableHead className="text-right">{t("actions")}</TableHead>
                                  </TableRow>
                                </TableHeader>
                                <TableBody>
                                  {vm.port_forwards.map((pf) => (
                                    <TableRow key={`${pf.host_port}/${pf.protocol}`}>
                                      <TableCell className="font-mono text-xs">{pf.host_port}</TableCell>
                                      <TableCell className="font-mono text-xs">{pf.guest_port}</TableCell>
                                      <TableCell className="font-mono text-xs">{pf.protocol}</TableCell>
                                      <TableCell className="text-right">
                                        <Button
                                          type="button"
                                          variant="outline"
                                          size="xs"
                                          className={cardActionDanger}
                                          onClick={() => onRemovePortForward(vm.id, pf.host_port)}
                                        >
                                          <span className={cn("mr-1", iconStroke, "[&_svg]:size-3")}>
                                            {I.trash}
                                          </span>
                                          {t("remove")}
                                        </Button>
                                      </TableCell>
                                    </TableRow>
                                  ))}
                                </TableBody>
                              </Table>
                            </CardContent>
                          </Card>
                        )}
                      </TabsContent>
                    </Tabs>
                  )}
                </CardContent>
              </Card>
            )
          })}
        </div>
      )}

      {/* Create VM dialog, Console dialog */}
      <Dialog
        open={showCreateModal}
        onOpenChange={(open) => {
          setShowCreateModal(open)
        }}
      >
        <DialogContent className="sm:max-w-3xl" data-testid="vms-dialog-create">
          <DialogHeader>
            <DialogTitle className="flex items-center gap-2">
              <span className={cn("text-muted-foreground", iconStroke, "[&_svg]:size-5")}>
                {I.server}
              </span>
              {t("createVm")}
            </DialogTitle>
          </DialogHeader>

          {vmError && <ErrorInline message={vmError} onDismiss={() => setVmError("")} />}

          <ScrollArea className="max-h-[620px] -mx-6 px-6">
            <div className="space-y-6 pb-2">
              <div className="space-y-2">
                <div className="text-xs font-medium text-muted-foreground">{t("name")}</div>
                <Input
                  value={vmName}
                  onChange={(e) => setVmName(e.target.value)}
                  placeholder="myvm"
                />
              </div>

              <div className="space-y-2">
                <div className="text-xs font-medium text-muted-foreground">{t("osImage")}</div>
                <Select
                  value={selectedOsValue}
                  onValueChange={(v) => setSelectedOsImage(v === NONE_OS_IMAGE ? "" : v)}
                >
                  <SelectTrigger className="w-full justify-between">
                    <SelectValue placeholder={t("osImageSelect")} />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectItem value={NONE_OS_IMAGE}>{t("osImageNone")}</SelectItem>
                    {readyOsImages.map((img) => (
                      <SelectItem key={img.id} value={img.id}>
                        <span className="font-medium">{img.name}</span>
                        <span className="text-muted-foreground"> · {img.arch}</span>
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>
              </div>

              <Card className="py-0">
                <CardHeader className="border-b py-3">
                  <CardTitle className="text-xs font-semibold tracking-widest text-muted-foreground">
                    AVAILABLE IMAGES
                  </CardTitle>
                </CardHeader>
                <CardContent className="py-3 space-y-2">
                  {osImages.length === 0 ? (
                    <div className="text-sm text-muted-foreground">{t("loading")}</div>
                  ) : (
                    osImages.map((img) => {
                      const showProgress = downloadProgress?.image_id === img.id
                      return (
                        <div key={img.id} className="space-y-2">
                          <div className="flex items-center justify-between gap-3 rounded-lg border bg-muted/30 px-4 py-3">
                            <div className="flex items-center gap-3 min-w-0">
                              <div
                                className={cn(
                                  "size-9 shrink-0 rounded-lg border bg-background/40 flex items-center justify-center text-muted-foreground",
                                  iconStroke,
                                  "[&_svg]:size-4"
                                )}
                              >
                                {I.hardDrive}
                              </div>
                              <div className="min-w-0">
                                <div className="text-sm font-semibold text-foreground truncate">
                                  {img.name}
                                </div>
                                <div className="text-xs text-muted-foreground font-mono truncate">
                                  {img.arch} · v{img.version} · {formatBytes(img.size_bytes)}
                                </div>
                              </div>
                            </div>

                            <div className="flex items-center gap-2">
                              {img.status === "ready" ? (
                                <>
                                  <Badge
                                    variant="secondary"
                                    className="rounded-md border border-brand-green/20 bg-brand-green/10 text-brand-green"
                                  >
                                    {t("osImageReady")}
                                  </Badge>
                                  <Button
                                    type="button"
                                    variant="outline"
                                    size="xs"
                                    className={cardActionDanger}
                                    title={t("osImageDelete")}
                                    onClick={() => onDeleteOsImage(img.id)}
                                  >
                                    <span className={cn(iconStroke, "[&_svg]:size-3")}>{I.trash}</span>
                                  </Button>
                                </>
                              ) : img.status === "downloading" || showProgress ? (
                                <Badge
                                  variant="secondary"
                                  className="rounded-md border border-brand-cyan/20 bg-brand-cyan/10 text-brand-cyan"
                                >
                                  {t("osImageDownloading")}
                                </Badge>
                              ) : (
                                <Button
                                  type="button"
                                  variant="outline"
                                  size="xs"
                                  className={cardActionOutline}
                                  disabled={!!downloadingImage || vmActing === "create"}
                                  onClick={() => onDownloadOsImage(img.id)}
                                >
                                  {t("osImageDownload")}
                                </Button>
                              )}
                            </div>
                          </div>

                          {showProgress && (
                            <div className="rounded-lg border bg-muted/30 px-4 py-3 space-y-2">
                              <div className="flex items-center justify-between text-xs text-muted-foreground">
                                <span className="font-mono truncate max-w-[60%]">
                                  {downloadProgress?.current_file || img.name}
                                </span>
                                <span className="font-mono">{activeDownloadPct}%</span>
                              </div>
                              <Progress value={activeDownloadPct} />
                            </div>
                          )}
                        </div>
                      )
                    })
                  )}
                </CardContent>
              </Card>

              <Separator className="opacity-60" />

              <div className="text-xs font-semibold tracking-widest text-muted-foreground">
                HARDWARE CONFIGURATION
              </div>

              <div className="grid grid-cols-1 gap-3 md:grid-cols-2">
                <div className="space-y-2">
                  <div className="text-xs font-medium text-muted-foreground">{t("cpus")}</div>
                  <Input
                    type="number"
                    min={1}
                    value={vmCpus}
                    onChange={(e) => setVmCpus(Number(e.target.value) || 2)}
                  />
                </div>
                <div className="space-y-2">
                  <div className="text-xs font-medium text-muted-foreground">{t("memoryMb")}</div>
                  <Input
                    type="number"
                    min={256}
                    value={vmMem}
                    onChange={(e) => setVmMem(Number(e.target.value) || 2048)}
                  />
                </div>
              </div>

              <div className="space-y-2">
                <div className="text-xs font-medium text-muted-foreground">{t("diskGb")}</div>
                <Input
                  type="number"
                  min={10}
                  value={vmDisk}
                  onChange={(e) => setVmDisk(Number(e.target.value) || 20)}
                />
              </div>

              <div className="flex items-start gap-2">
                <Checkbox
                  id="vm-create-rosetta"
                  checked={vmRosetta}
                  onCheckedChange={(v) => setVmRosetta(Boolean(v))}
                />
                <label htmlFor="vm-create-rosetta" className="text-sm text-foreground leading-5">
                  {t("enableRosetta")}
                </label>
              </div>
            </div>
          </ScrollArea>

          <DialogFooter>
            <Button type="button" variant="outline" onClick={closeCreate}>
              {t("close")}
            </Button>
            <Button
              type="button"
              onClick={handleCreate}
              disabled={vmActing === "create" || !vmName.trim()}
            >
              {vmActing === "create" ? (
                <>
                  <div className="size-4 rounded-full border-2 border-primary-foreground/50 border-t-primary-foreground animate-spin" />
                  {t("creating")}
                </>
              ) : (
                <>
                  <span className={cn(iconStroke, "[&_svg]:size-4")}>{I.plus}</span>
                  {t("create")}
                </>
              )}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      <Dialog
        open={Boolean(consoleVmId)}
        onOpenChange={(open) => {
          if (!open) closeConsole()
        }}
      >
        <DialogContent className="sm:max-w-4xl" data-testid="vm-console-dialog">
          <DialogHeader>
            <DialogTitle className="flex items-center gap-2">
              <span className={cn("text-muted-foreground", iconStroke, "[&_svg]:size-5")}>
                {I.terminal}
              </span>
              {t("vmConsole")}
              {consoleVmName && (
                <span className="text-muted-foreground font-mono text-sm">— {consoleVmName}</span>
              )}
            </DialogTitle>
          </DialogHeader>

          <div className="flex flex-wrap items-center justify-between gap-3">
            <div className="flex items-center gap-2">
              <Checkbox
                id="vm-console-autoscroll"
                checked={autoScroll}
                onCheckedChange={(v) => setAutoScroll(Boolean(v))}
              />
              <label htmlFor="vm-console-autoscroll" className="text-sm text-foreground">
                {t("autoScroll")}
              </label>
            </div>

            <div className="flex items-center gap-2">
              <Button type="button" variant="outline" size="sm" onClick={() => setConsoleData("")}>
                {t("clear")}
              </Button>
              <Button
                type="button"
                variant="outline"
                size="sm"
                onClick={copyConsole}
                disabled={!consoleData}
              >
                <span className={cn("mr-1", iconStroke, "[&_svg]:size-4")}>{I.copy}</span>
                {t("copy")}
              </Button>
            </div>
          </div>

          <div className="rounded-lg border bg-muted/30">
            <ScrollArea className="h-[520px]">
              <div className="p-4">
                {consoleData ? (
                  <pre className="whitespace-pre-wrap break-words text-xs font-mono text-foreground">
                    {consoleData}
                    <div ref={consoleEndRef} />
                  </pre>
                ) : (
                  <div className="flex items-center justify-center py-20 text-muted-foreground">
                    <span className={cn(iconStroke, "[&_svg]:size-5")}>{I.terminal}</span>
                    <span className="ml-2">{t("noConsoleOutput")}</span>
                  </div>
                )}
              </div>
            </ScrollArea>
          </div>

          <DialogFooter>
            <Button type="button" variant="outline" onClick={closeConsole}>
              {t("close")}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  )
}
