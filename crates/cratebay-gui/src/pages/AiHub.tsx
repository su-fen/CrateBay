import { useCallback, useEffect, useMemo, useState } from "react"
import { invoke } from "@tauri-apps/api/core"
import { I } from "../icons"
import { EmptyState } from "../components/EmptyState"
import { ErrorInline } from "../components/ErrorDisplay"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Card, CardContent } from "@/components/ui/card"
import { Input } from "@/components/ui/input"
import { ScrollArea } from "@/components/ui/scroll-area"
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select"
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table"
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs"
import { commandNeedsConfirmation } from "@/lib/aiSkills"
import { formatSandboxError } from "@/lib/sandboxErrors"
import { cn } from "@/lib/utils"
import { iconStroke, cardActionDanger, cardActionOutline, cardActionSecondary } from "@/lib/styles"
import { Assistant } from "./Assistant"
import type {
  AiHubActionResultDto,
  GpuStatusDto,
  AiSettings,
  McpServerEntry,
  McpServerStatusDto,
  OllamaModelDto,
  OllamaStatusDto,
  OllamaStorageInfoDto,
  SandboxAuditEventDto,
  SandboxCleanupResultDto,
  SandboxCreateRequest,
  SandboxCreateResultDto,
  SandboxExecResultDto,
  SandboxInfoDto,
  SandboxInspectDto,
  SandboxRuntimeUsageDto,
  SandboxTemplateDto,
} from "../types"

export type AiHubTab = "sandboxes" | "models" | "mcp" | "assistant"

interface AiHubProps {
  t: (key: string) => string
  initialTab?: AiHubTab
}

function formatSandboxTime(value: string) {
  if (!value) return "-"
  const parsed = new Date(value)
  if (Number.isNaN(parsed.getTime())) {
    return value
  }
  return parsed.toLocaleString()
}

export function AiHub({ t, initialTab = "sandboxes" }: AiHubProps) {
  const [tab, setTab] = useState<AiHubTab>(initialTab)
  const [ollamaStatus, setOllamaStatus] = useState<OllamaStatusDto | null>(null)
  const [ollamaModels, setOllamaModels] = useState<OllamaModelDto[]>([])
  const [ollamaStorage, setOllamaStorage] = useState<OllamaStorageInfoDto | null>(null)
  const [gpuStatus, setGpuStatus] = useState<GpuStatusDto | null>(null)
  const [ollamaLoading, setOllamaLoading] = useState(false)
  const [ollamaError, setOllamaError] = useState("")
  const [ollamaNotice, setOllamaNotice] = useState("")
  const [ollamaPullName, setOllamaPullName] = useState("")
  const [ollamaActingName, setOllamaActingName] = useState("")
  const [sandboxTemplates, setSandboxTemplates] = useState<SandboxTemplateDto[]>([])
  const [sandboxes, setSandboxes] = useState<SandboxInfoDto[]>([])
  const [sandboxAudit, setSandboxAudit] = useState<SandboxAuditEventDto[]>([])
  const [sandboxInspect, setSandboxInspect] = useState<SandboxInspectDto | null>(null)
  const [sandboxRuntimeUsage, setSandboxRuntimeUsage] = useState<SandboxRuntimeUsageDto | null>(null)
  const [sandboxRuntimeLoading, setSandboxRuntimeLoading] = useState(false)
  const [sandboxRuntimeError, setSandboxRuntimeError] = useState("")
  const [sandboxLoading, setSandboxLoading] = useState(false)
  const [sandboxCreating, setSandboxCreating] = useState(false)
  const [sandboxActingId, setSandboxActingId] = useState("")
  const [sandboxError, setSandboxError] = useState("")
  const [sandboxNotice, setSandboxNotice] = useState("")
  const [sandboxInspectError, setSandboxInspectError] = useState("")
  const [sandboxSelectedTemplate, setSandboxSelectedTemplate] = useState("")
  const [sandboxName, setSandboxName] = useState("")
  const [sandboxOwner, setSandboxOwner] = useState("")
  const [sandboxCpu, setSandboxCpu] = useState<number | "">("")
  const [sandboxMemoryMb, setSandboxMemoryMb] = useState<number | "">("")
  const [sandboxTtlHours, setSandboxTtlHours] = useState<number | "">("")
  const [sandboxCommand, setSandboxCommand] = useState("")
  const [sandboxEnvLines, setSandboxEnvLines] = useState("")
  const [sandboxExecCommand, setSandboxExecCommand] = useState("")
  const [sandboxExecResult, setSandboxExecResult] = useState<SandboxExecResultDto | null>(null)
  const [mcpServers, setMcpServers] = useState<McpServerStatusDto[]>([])
  const [mcpDrafts, setMcpDrafts] = useState<McpServerEntry[]>([])
  const [mcpSelectedId, setMcpSelectedId] = useState("")
  const [mcpLogs, setMcpLogs] = useState<string[]>([])
  const [mcpExportClient, setMcpExportClient] = useState("codex")
  const [mcpExportValue, setMcpExportValue] = useState("")
  const [mcpLoading, setMcpLoading] = useState(false)
  const [mcpError, setMcpError] = useState("")
  const [mcpActingId, setMcpActingId] = useState("")

  const sandboxErrorMessage = useCallback((error: unknown) => formatSandboxError(String(error), t), [t])

  const refreshOllama = useCallback(async () => {
    setOllamaLoading(true)
    setOllamaError("")
    setOllamaNotice("")
    try {
      const [status, gpu] = await Promise.all([
        invoke<OllamaStatusDto>("ollama_status"),
        invoke<GpuStatusDto>("gpu_status").catch(() => null),
      ])
      setOllamaStatus(status)
      setGpuStatus(gpu)
      if (status.installed) {
        const storage = await invoke<OllamaStorageInfoDto>("ollama_storage_info")
        setOllamaStorage(storage)
      } else {
        setOllamaStorage(null)
      }
      if (status.running) {
        const models = await invoke<OllamaModelDto[]>("ollama_list_models")
        setOllamaModels(models)
      } else {
        setOllamaModels([])
      }
    } catch (e) {
      setOllamaError(String(e))
    } finally {
      setOllamaLoading(false)
    }
  }, [])

  const refreshSandboxes = useCallback(async () => {
    setSandboxLoading(true)
    setSandboxError("")
    try {
      const [templates, instances, auditEvents] = await Promise.all([
        invoke<SandboxTemplateDto[]>("sandbox_templates"),
        invoke<SandboxInfoDto[]>("sandbox_list"),
        invoke<SandboxAuditEventDto[]>("sandbox_audit_list", { limit: 40 }),
      ])
      setSandboxTemplates(templates)
      setSandboxes(instances)
      setSandboxAudit(auditEvents)
      setSandboxSelectedTemplate((prev) =>
        prev && templates.some((it) => it.id === prev) ? prev : (templates[0]?.id ?? "")
      )
    } catch (e) {
      setSandboxError(sandboxErrorMessage(e))
    } finally {
      setSandboxLoading(false)
    }
  }, [])

  const refreshMcp = useCallback(async () => {
    setMcpLoading(true)
    setMcpError("")
    try {
      const [settings, servers] = await Promise.all([
        invoke<AiSettings>("load_ai_settings"),
        invoke<McpServerStatusDto[]>("mcp_list_servers"),
      ])
      setMcpDrafts(settings.mcp_servers ?? [])
      setMcpServers(servers)
      setMcpSelectedId((prev) =>
        prev && (settings.mcp_servers ?? []).some((item) => item.id === prev)
          ? prev
          : (settings.mcp_servers?.[0]?.id ?? "")
      )
    } catch (e) {
      setMcpError(String(e))
    } finally {
      setMcpLoading(false)
    }
  }, [])

  useEffect(() => {
    refreshOllama()
  }, [refreshOllama])

  useEffect(() => {
    refreshSandboxes()
  }, [refreshSandboxes])

  useEffect(() => {
    refreshMcp()
  }, [refreshMcp])

  useEffect(() => {
    setTab(initialTab)
  }, [initialTab])

  useEffect(() => {
    if (!mcpSelectedId) {
      setMcpLogs([])
      return
    }
    invoke<string[]>("mcp_server_logs", { id: mcpSelectedId, limit: 120 })
      .then((logs) => setMcpLogs(logs))
      .catch(() => setMcpLogs([]))
  }, [mcpSelectedId, mcpServers])

  const formatGpuMetric = useCallback((value: number | null | undefined, suffix: string) => {
    if (value == null || Number.isNaN(value)) return "-"
    const rounded = suffix === "%" ? Math.round(value) : Math.round(value * 10) / 10
    return `${rounded}${suffix}`
  }, [])

  const formatGpuMemory = useCallback((used?: string | null, total?: string | null) => {
    if (used && total) return `${used} / ${total}`
    return total || used || "-"
  }, [])

  const formatUsagePercent = useCallback((value: number | null | undefined) => {
    if (value == null || Number.isNaN(value)) return "-"
    return `${Math.round(value * 10) / 10}%`
  }, [])

  const formatUsageMegabytes = useCallback((value: number | null | undefined) => {
    if (value == null || Number.isNaN(value)) return "-"
    return `${Math.round(value * 10) / 10} MB`
  }, [])

  const totalModelBytes = useMemo(
    () => ollamaModels.reduce((sum, m) => sum + (m.size_bytes || 0), 0),
    [ollamaModels]
  )

  const selectedSandboxTemplate = useMemo(
    () => sandboxTemplates.find((it) => it.id === sandboxSelectedTemplate) ?? null,
    [sandboxSelectedTemplate, sandboxTemplates]
  )

  const selectedMcpDraft = useMemo(
    () => mcpDrafts.find((item) => item.id === mcpSelectedId) ?? null,
    [mcpDrafts, mcpSelectedId]
  )

  const selectedMcpStatus = useMemo(
    () => mcpServers.find((item) => item.id === mcpSelectedId) ?? null,
    [mcpServers, mcpSelectedId]
  )

  const runningSandboxCount = useMemo(
    () => sandboxes.filter((s) => s.state === "running").length,
    [sandboxes]
  )

  const expiredSandboxCount = useMemo(
    () => sandboxes.filter((s) => s.is_expired).length,
    [sandboxes]
  )

  const loadSandboxInspect = useCallback(async (id: string) => {
    setSandboxRuntimeLoading(true)
    setSandboxRuntimeError("")
    setSandboxRuntimeUsage(null)
    try {
      const inspect = await invoke<SandboxInspectDto>("sandbox_inspect", { id })
      setSandboxInspect(inspect)
      try {
        const runtimeUsage = await invoke<SandboxRuntimeUsageDto>("sandbox_runtime_usage", { id })
        setSandboxRuntimeUsage(runtimeUsage)
      } catch (e) {
        setSandboxRuntimeError(sandboxErrorMessage(e))
      }
      return inspect
    } finally {
      setSandboxRuntimeLoading(false)
    }
  }, [])

  useEffect(() => {
    if (!selectedSandboxTemplate) return
    if (sandboxCpu === "" && sandboxMemoryMb === "" && sandboxTtlHours === "" && !sandboxCommand.trim()) {
      setSandboxCpu(selectedSandboxTemplate.cpu_default)
      setSandboxMemoryMb(selectedSandboxTemplate.memory_mb_default)
      setSandboxTtlHours(selectedSandboxTemplate.ttl_hours_default)
      setSandboxCommand(selectedSandboxTemplate.default_command)
    }
  }, [
    selectedSandboxTemplate,
    sandboxCommand,
    sandboxCpu,
    sandboxMemoryMb,
    sandboxTtlHours,
  ])

  const tabLabel = useMemo(() => {
    switch (tab) {
      case "models":
        return t("models")
      case "sandboxes":
        return t("sandboxes")
      case "mcp":
        return t("mcp")
      case "assistant":
        return t("assistant")
      default:
        return t("ai")
    }
  }, [tab, t])

  const applyTemplateDefaults = (template: SandboxTemplateDto) => {
    setSandboxCpu(template.cpu_default)
    setSandboxMemoryMb(template.memory_mb_default)
    setSandboxTtlHours(template.ttl_hours_default)
    setSandboxCommand(template.default_command)
  }

  const handleSelectSandboxTemplate = (templateId: string) => {
    setSandboxSelectedTemplate(templateId)
    const template = sandboxTemplates.find((it) => it.id === templateId)
    if (template) {
      applyTemplateDefaults(template)
    }
  }

  const handleCreateSandbox = async () => {
    if (!sandboxSelectedTemplate) {
      setSandboxError(t("sandboxTemplateRequired"))
      return
    }

    setSandboxCreating(true)
    setSandboxError("")
    try {
      const env = sandboxEnvLines
        .split("\n")
        .map((line) => line.trim())
        .filter(Boolean)

      const request: SandboxCreateRequest = {
        template_id: sandboxSelectedTemplate,
        name: sandboxName.trim() ? sandboxName.trim() : null,
        owner: sandboxOwner.trim() ? sandboxOwner.trim() : null,
        cpu_cores: sandboxCpu === "" ? null : sandboxCpu,
        memory_mb: sandboxMemoryMb === "" ? null : sandboxMemoryMb,
        ttl_hours: sandboxTtlHours === "" ? null : sandboxTtlHours,
        command: sandboxCommand.trim() ? sandboxCommand.trim() : null,
        env: env.length > 0 ? env : null,
      }

      const created = await invoke<SandboxCreateResultDto>("sandbox_create", { request })
      await loadSandboxInspect(created.id)
      setSandboxInspectError("")
      setSandboxName("")
      setSandboxEnvLines("")
      await refreshSandboxes()
    } catch (e) {
      setSandboxError(sandboxErrorMessage(e))
    } finally {
      setSandboxCreating(false)
    }
  }

  const handleSandboxAction = async (
    action: "start" | "stop" | "delete" | "inspect",
    item: SandboxInfoDto
  ) => {
    setSandboxError("")
    setSandboxInspectError("")

    if (action === "delete" && !window.confirm(t("confirmDeleteSandbox"))) {
      return
    }

    const actionKey = `${action}:${item.id}`
    setSandboxActingId(actionKey)
    try {
      if (action === "inspect") {
        await loadSandboxInspect(item.id)
      } else if (action === "start") {
        await invoke("sandbox_start", { id: item.id })
        await refreshSandboxes()
        if (sandboxInspect?.id === item.id) {
          await loadSandboxInspect(item.id)
        }
      } else if (action === "stop") {
        await invoke("sandbox_stop", { id: item.id })
        await refreshSandboxes()
        if (sandboxInspect?.id === item.id) {
          await loadSandboxInspect(item.id)
        }
      } else if (action === "delete") {
        await invoke("sandbox_delete", { id: item.id })
        if (sandboxInspect?.id === item.id) {
          setSandboxInspect(null)
          setSandboxRuntimeUsage(null)
          setSandboxRuntimeError("")
          setSandboxExecResult(null)
        }
        await refreshSandboxes()
      }
    } catch (e) {
      if (action === "inspect") {
        setSandboxInspectError(sandboxErrorMessage(e))
      } else {
        setSandboxError(sandboxErrorMessage(e))
      }
    } finally {
      setSandboxActingId("")
    }
  }

  const handleCleanupExpiredSandboxes = async () => {
    if (commandNeedsConfirmation("sandbox_cleanup_expired")) {
      const confirmed = window.confirm(
        `${t("assistantConfirmAction")}\n${t("sandboxCleanupExpired")}\nsandbox_cleanup_expired`
      )
      if (!confirmed) return
    }

    setSandboxError("")
    setSandboxNotice("")
    setSandboxActingId("cleanup")
    try {
      const result = await invoke<SandboxCleanupResultDto>("sandbox_cleanup_expired")
      if (result.message) {
        setSandboxNotice(result.message)
      }
      await refreshSandboxes()
    } catch (e) {
      setSandboxError(sandboxErrorMessage(e))
    } finally {
      setSandboxActingId("")
    }
  }

  const handleSandboxExec = async () => {
    const targetId = sandboxInspect?.id ?? sandboxes.find((item) => item.state === "running")?.id
    if (!targetId) {
      setSandboxInspectError(t("sandboxExecTitle"))
      return
    }
    const execCommand = sandboxExecCommand.trim()
    if (!execCommand) {
      return
    }
    if (commandNeedsConfirmation("sandbox_exec")) {
      const confirmed = window.confirm(
        `${t("assistantConfirmAction")}\n${t("sandboxExecTitle")}\n${execCommand}`
      )
      if (!confirmed) return
    }
    setSandboxActingId(`exec:${targetId}`)
    setSandboxInspectError("")
    setSandboxExecResult(null)
    try {
      const result = await invoke<SandboxExecResultDto>("sandbox_exec", {
        id: targetId,
        command: execCommand,
      })
      setSandboxExecResult(result)
    } catch (e) {
      setSandboxInspectError(sandboxErrorMessage(e))
    } finally {
      setSandboxActingId("")
    }
  }

  const handlePullModel = async () => {
    if (!ollamaPullName.trim()) {
      setOllamaError(t("ollamaModelRequired"))
      return
    }
    setOllamaActingName(`pull:${ollamaPullName.trim()}`)
    setOllamaError("")
    setOllamaNotice("")
    try {
      const result = await invoke<AiHubActionResultDto>("ollama_pull_model", { name: ollamaPullName.trim() })
      setOllamaPullName("")
      if (!result.ok && result.message) {
        setOllamaError(result.message)
      } else if (result.message) {
        setOllamaNotice(result.message)
      }
      await refreshOllama()
    } catch (e) {
      setOllamaError(String(e))
    } finally {
      setOllamaActingName("")
    }
  }

  const handleDeleteModel = async (name: string) => {
    if (!window.confirm(t("confirmDeleteModel"))) {
      return
    }
    setOllamaActingName(`delete:${name}`)
    setOllamaError("")
    setOllamaNotice("")
    try {
      const result = await invoke<AiHubActionResultDto>("ollama_delete_model", { name })
      if (!result.ok && result.message) {
        setOllamaError(result.message)
      } else if (result.message) {
        setOllamaNotice(result.message)
      }
      await refreshOllama()
    } catch (e) {
      setOllamaError(String(e))
    } finally {
      setOllamaActingName("")
    }
  }

  const updateSelectedMcpDraft = (updater: (draft: McpServerEntry) => McpServerEntry) => {
    let nextSelectedId = mcpSelectedId
    setMcpDrafts((prev) =>
      prev.map((item) => {
        if (item.id !== mcpSelectedId) return item
        const next = updater(item)
        nextSelectedId = next.id
        return next
      })
    )
    if (nextSelectedId !== mcpSelectedId) {
      setMcpSelectedId(nextSelectedId)
    }
  }

  const handleAddMcpServer = () => {
    let nextIndex = mcpDrafts.length + 1
    let id = `local-mcp-${nextIndex}`
    while (mcpDrafts.some((item) => item.id === id)) {
      nextIndex += 1
      id = `local-mcp-${nextIndex}`
    }
    const next: McpServerEntry = {
      id,
      name: `Local MCP ${nextIndex}`,
      command: "",
      args: [],
      env: [],
      working_dir: "",
      enabled: true,
      notes: "",
    }
    setMcpDrafts((prev) => [...prev, next])
    setMcpSelectedId(id)
    setMcpExportValue("")
  }

  const handleDeleteMcpServer = (id: string) => {
    if (!window.confirm(`${t("mcpDeleteServer")} ${id}?`)) {
      return
    }
    const nextDrafts = mcpDrafts.filter((item) => item.id !== id)
    setMcpDrafts(nextDrafts)
    if (mcpSelectedId === id) {
      setMcpSelectedId(nextDrafts[0]?.id ?? "")
    }
    setMcpExportValue("")
  }

  const handleSaveMcpRegistry = async () => {
    if (mcpDrafts.some((item) => !item.id.trim())) {
      setMcpError(t("mcpServerIdRequired"))
      return
    }
    if (mcpDrafts.some((item) => !item.command.trim())) {
      setMcpError(t("mcpServerCommandRequired"))
      return
    }
    setMcpActingId("save")
    setMcpError("")
    try {
      await invoke<McpServerEntry[]>("mcp_save_servers", {
        servers: mcpDrafts.map((item) => ({
          ...item,
          id: item.id.trim(),
          name: item.name.trim(),
          command: item.command.trim(),
          working_dir: item.working_dir.trim(),
          notes: item.notes.trim(),
          args: item.args.map((arg) => arg.trim()).filter(Boolean),
          env: item.env.map((entry) => entry.trim()).filter(Boolean),
        })),
      })
      await refreshMcp()
    } catch (e) {
      setMcpError(String(e))
    } finally {
      setMcpActingId("")
    }
  }

  const handleMcpAction = async (action: "start" | "stop" | "export", id?: string) => {
    const targetId = id ?? mcpSelectedId
    if (!targetId) return
    setMcpActingId(`${action}:${targetId}`)
    setMcpError("")
    try {
      if (action === "start") {
        await invoke<AiHubActionResultDto>("mcp_start_server", { id: targetId })
        await refreshMcp()
      } else if (action === "stop") {
        await invoke<AiHubActionResultDto>("mcp_stop_server", { id: targetId })
        await refreshMcp()
      } else {
        const content = await invoke<string>("mcp_export_client_config", { client: mcpExportClient })
        setMcpExportValue(content)
      }
    } catch (e) {
      setMcpError(String(e))
    } finally {
      setMcpActingId("")
    }
  }

  return (
    <div className="space-y-4">
      <Card className="py-0">
        <CardContent className="py-4 space-y-3">
          <div className="flex items-start gap-3">
            <div className="size-10 shrink-0 rounded-lg bg-primary/10 text-primary flex items-center justify-center [&_svg]:size-5 [&_svg]:fill-none [&_svg]:stroke-current [&_svg]:stroke-2 [&_svg]:stroke-linecap-round [&_svg]:stroke-linejoin-round">
              {I.aiAssistant}
            </div>
            <div className="min-w-0 flex-1">
              <div className="flex flex-wrap items-center gap-2">
                <div className="text-sm font-semibold text-foreground">{t("ai")}</div>
                <Badge
                  variant="secondary"
                  className="rounded-md border border-brand-cyan/15 bg-brand-cyan/10 px-1.5 py-0 text-[11px] text-brand-cyan"
                >
                  {t("aiInfra")}
                </Badge>
                <span className="text-muted-foreground/40">•</span>
                <span className="text-xs text-muted-foreground">
                  {t("aiHubActiveTab")}: <span className="text-foreground/90 font-medium">{tabLabel}</span>
                </span>
              </div>
            <div className="mt-1 text-xs text-muted-foreground">
              {t("aiHubDesc")}
            </div>
          </div>
        </div>
      </CardContent>
    </Card>

    <Tabs value={tab} onValueChange={(v) => setTab(v as AiHubTab)}>
      <TabsList variant="line" className="w-full justify-start">
        <TabsTrigger value="sandboxes">{t("sandboxes")}</TabsTrigger>
        <TabsTrigger value="models">{t("models")}</TabsTrigger>
        <TabsTrigger value="mcp" data-testid="aihub-tab-mcp">{t("mcp")}</TabsTrigger>
        <TabsTrigger value="assistant">{t("assistant")}</TabsTrigger>
      </TabsList>

      <TabsContent value="models" className="space-y-3">
        <Card className="py-0">
          <CardContent className="py-4 space-y-3">
              <div className="flex flex-wrap items-start justify-between gap-3">
                <div className="min-w-0 flex-1">
                  <div className="flex flex-wrap items-center gap-2">
                    <div className={cn("text-sm font-semibold text-foreground")}>{t("ollamaRuntime")}</div>
                    {ollamaStatus?.running ? (
                      <Badge className="rounded-full gap-2 px-3 py-1 text-xs font-medium border border-brand-green/20 bg-brand-green/10 text-brand-green">
                        <span className="size-1.5 rounded-full bg-brand-green shadow-[0_0_10px_hsl(var(--brand-green)/0.6)]" />
                        {t("running")}
                      </Badge>
                    ) : (
                      <Badge
                        variant="secondary"
                        className="rounded-full gap-2 px-3 py-1 text-xs font-medium border border-border/60 bg-popover/40 text-muted-foreground"
                      >
                        <span className="size-1.5 rounded-full bg-destructive" />
                        {t("stopped")}
                      </Badge>
                    )}
                    {ollamaStatus?.version && (
                      <Badge variant="secondary" className="rounded-md px-1.5 py-0 text-[11px]">
                        v{ollamaStatus.version}
                      </Badge>
                    )}
                  </div>
                  <div className="mt-1 text-xs text-muted-foreground">
                    {t("aiBaseUrl")}: <code className="rounded-md border border-border/60 bg-muted/60 px-1.5 py-0.5 font-mono text-[11px] text-foreground">{ollamaStatus?.base_url ?? "-"}</code>
                  </div>
                </div>

                <div className="flex items-center gap-2">
                  <Button
                    type="button"
                    variant="outline"
                    size="xs"
                    className={cn(cardActionOutline)}
                    onClick={refreshOllama}
                    disabled={ollamaLoading}
                  >
                    <span className={cn("mr-1", iconStroke, "[&_svg]:size-3")}>{I.refresh}</span>
                    {ollamaLoading ? t("working") : t("refresh")}
                  </Button>
                </div>
              </div>

              {ollamaError && (
                <ErrorInline message={ollamaError} onDismiss={() => setOllamaError("")} />
              )}
              {ollamaNotice && <div className="text-xs text-muted-foreground">{ollamaNotice}</div>}

              <div className="grid gap-3 lg:grid-cols-3">
                <div className="rounded-xl border border-border/50 bg-muted/20 p-3">
                  <div className="text-xs font-semibold text-muted-foreground">{t("aiModelStorage")}</div>
                  <div className="mt-2 text-sm font-semibold text-foreground">{ollamaStorage?.total_size_human ?? `${Math.round((totalModelBytes / (1024 * 1024 * 1024)) * 10) / 10} GB`}</div>
                  <div className="mt-1 text-xs text-muted-foreground">
                    {(ollamaStorage?.model_count ?? ollamaModels.length)} {t("models").toLowerCase()}
                  </div>
                  <div className="mt-3 text-xs text-muted-foreground">{t("ollamaStoragePath")}</div>
                  <code className="mt-1 block break-all rounded-lg border border-border/50 bg-popover/40 px-2 py-2 text-[11px] text-foreground">
                    {ollamaStorage?.path || t("ollamaStorageMissing")}
                  </code>
                </div>

                <div data-testid="ollama-gpu-card" className="rounded-xl border border-border/50 bg-muted/20 p-3">
                  <div className="flex items-center gap-2">
                    <div className="text-xs font-semibold text-muted-foreground">{t("gpuRuntime")}</div>
                    {gpuStatus?.backend && (
                      <Badge variant="secondary" className="rounded-md px-1.5 py-0 text-[11px]">
                        {gpuStatus.backend}
                      </Badge>
                    )}
                  </div>
                  <div className="mt-2 text-sm font-semibold text-foreground">
                    {gpuStatus?.available ? `${gpuStatus.devices.length} ${t("gpuDevices").toLowerCase()}` : t("gpuTelemetryUnavailable")}
                  </div>
                  <div className="mt-1 text-xs text-muted-foreground">
                    {gpuStatus?.message || t("gpuTelemetryUnavailable")}
                  </div>
                  {gpuStatus?.devices.length ? (
                    <div className="mt-3 space-y-2">
                      {gpuStatus.devices.map((device) => (
                        <div
                          key={`${device.index}-${device.name}`}
                          data-testid={`ollama-gpu-device-${device.index}`}
                          className="rounded-lg border border-border/50 bg-popover/40 px-2.5 py-2"
                        >
                          <div className="truncate text-xs font-semibold text-foreground">{device.name}</div>
                          <div className="mt-1 grid grid-cols-2 gap-2 text-[11px] text-muted-foreground">
                            <div>{t("gpuUtilization")}: <span className="text-foreground">{formatGpuMetric(device.utilization_percent, "%")}</span></div>
                            <div>{t("gpuMemory")}: <span className="text-foreground">{formatGpuMemory(device.memory_used_human, device.memory_total_human)}</span></div>
                            <div>{t("gpuTemperature")}: <span className="text-foreground">{formatGpuMetric(device.temperature_celsius, "°C")}</span></div>
                            <div>{t("gpuPower")}: <span className="text-foreground">{formatGpuMetric(device.power_watts, "W")}</span></div>
                          </div>
                        </div>
                      ))}
                    </div>
                  ) : null}
                </div>

                <div className="rounded-xl border border-border/50 bg-muted/20 p-3 space-y-2">
                  <div className="text-xs font-semibold text-muted-foreground">{t("ollamaPullLabel")}</div>
                  <Input
                    value={ollamaPullName}
                    onChange={(e) => setOllamaPullName(e.target.value)}
                    placeholder={t("ollamaPullPlaceholder")}
                    className="h-8 rounded-lg border-border/60 bg-popover/40 text-xs font-mono"
                  />
                  <Button
                    type="button"
                    size="xs"
                    className={cn(cardActionSecondary)}
                    disabled={!ollamaPullName.trim() || !!ollamaActingName}
                    onClick={handlePullModel}
                  >
                    <span className={cn("mr-1", iconStroke, "[&_svg]:size-3")}>{I.play}</span>
                    {ollamaActingName.startsWith("pull:") ? t("working") : t("ollamaPullAction")}
                  </Button>
                </div>
              </div>

              {ollamaStatus?.installed === false && (
                <EmptyState
                  icon={I.layers}
                  title={t("ollamaNotInstalledTitle")}
                  description={t("ollamaInstallHint")}
                  code={"bash scripts/setup-ai.sh --install"}
                />
              )}

              {ollamaStatus?.installed && !ollamaStatus.running && (
                <EmptyState
                  icon={I.layers}
                  title={t("ollamaNotRunningTitle")}
                  description={t("ollamaStartHint")}
                  code={"ollama serve"}
                />
              )}
            </CardContent>
          </Card>

          {ollamaStatus?.running && (
            <Card className="py-0">
              <CardContent className="py-0">
                {ollamaModels.length === 0 ? (
                  <div className="px-4 py-6">
                    <EmptyState
                      icon={I.layers}
                      title={t("ollamaModelsEmptyTitle")}
                      description={t("ollamaModelsEmptyDesc")}
                      code={"ollama pull qwen2.5:7b"}
                    />
                  </div>
                ) : (
                  <ScrollArea className="max-h-[520px]">
                    <Table>
                      <TableHeader>
                        <TableRow>
                          <TableHead>{t("name")}</TableHead>
                          <TableHead>{t("size")}</TableHead>
                          <TableHead>{t("description")}</TableHead>
                          <TableHead>{t("modifiedAt")}</TableHead>
                          <TableHead className="text-right">{t("actions")}</TableHead>
                        </TableRow>
                      </TableHeader>
                      <TableBody>
                        {ollamaModels.map((m) => {
                          const details = [m.family, m.parameter_size, m.quantization_level]
                            .filter(Boolean)
                            .join(" · ")
                          const actingDelete = ollamaActingName === `delete:${m.name}`
                          return (
                            <TableRow key={m.name}>
                              <TableCell className="font-mono text-xs max-w-[420px] truncate">{m.name}</TableCell>
                              <TableCell className="text-xs text-muted-foreground">{m.size_human}</TableCell>
                              <TableCell className="text-xs text-muted-foreground max-w-[420px] truncate">{details || "-"}</TableCell>
                              <TableCell className="text-xs text-muted-foreground max-w-[260px] truncate">{m.modified_at || "-"}</TableCell>
                              <TableCell className="text-right">
                                <div className="flex justify-end gap-2">
                                  <Button
                                    type="button"
                                    variant="outline"
                                    size="xs"
                                    className={cn(cardActionOutline)}
                                    onClick={() => navigator.clipboard.writeText(`ollama run ${m.name}`)}
                                  >
                                    <span className={cn("mr-1", iconStroke, "[&_svg]:size-3")}>{I.copy}</span>
                                    {t("copy")}
                                  </Button>
                                  <Button
                                    type="button"
                                    variant="outline"
                                    size="xs"
                                    className={cn(cardActionDanger)}
                                    disabled={!!ollamaActingName}
                                    onClick={() => handleDeleteModel(m.name)}
                                  >
                                    <span className={cn("mr-1", iconStroke, "[&_svg]:size-3")}>{I.trash}</span>
                                    {actingDelete ? t("working") : t("ollamaDeleteAction")}
                                  </Button>
                                </div>
                              </TableCell>
                            </TableRow>
                          )
                        })}
                      </TableBody>
                    </Table>
                  </ScrollArea>
                )}
              </CardContent>
            </Card>
          )}
        </TabsContent>

        <TabsContent value="sandboxes" className="space-y-3">
          <Card className="py-0">
            <CardContent className="py-4 space-y-3">
              <div className="flex flex-wrap items-start justify-between gap-3">
                <div className="min-w-0 flex-1">
                  <div className="flex flex-wrap items-center gap-2">
                    <div className={cn("text-sm font-semibold text-foreground")}>{t("aiSandboxesTitle")}</div>
                    <Badge className="rounded-md border border-brand-green/20 bg-brand-green/10 px-1.5 py-0 text-[11px] text-brand-green">
                      {t("mvp")}
                    </Badge>
                    <Badge variant="secondary" className="rounded-md px-1.5 py-0 text-[11px]">
                      {runningSandboxCount} {t("running")}
                    </Badge>
                    <Badge variant="secondary" className="rounded-md px-1.5 py-0 text-[11px]">
                      {sandboxes.length} {t("sandboxInstances")}
                    </Badge>
                    {expiredSandboxCount > 0 && (
                      <Badge variant="secondary" className="rounded-md px-1.5 py-0 text-[11px]">
                        {expiredSandboxCount} {t("sandboxExpired")}
                      </Badge>
                    )}
                  </div>
                  <div className="mt-1 text-xs text-muted-foreground">{t("aiSandboxesDesc")}</div>
                </div>
                <div className="flex items-center gap-2">
                  <Button
                    type="button"
                    variant="outline"
                    size="xs"
                    className={cn(cardActionOutline)}
                    disabled={sandboxActingId === "cleanup" || sandboxLoading || sandboxCreating}
                    onClick={handleCleanupExpiredSandboxes}
                  >
                    <span className={cn("mr-1", iconStroke, "[&_svg]:size-3")}>{I.trash}</span>
                    {sandboxActingId === "cleanup" ? t("working") : t("sandboxCleanupExpired")}
                  </Button>
                  <Button
                    type="button"
                    variant="outline"
                    size="xs"
                    className={cn(cardActionOutline)}
                    onClick={refreshSandboxes}
                    disabled={sandboxLoading || sandboxCreating}
                  >
                    <span className={cn("mr-1", iconStroke, "[&_svg]:size-3")}>{I.refresh}</span>
                    {sandboxLoading ? t("working") : t("refresh")}
                  </Button>
                </div>
              </div>

              {sandboxError && <ErrorInline message={sandboxError} onDismiss={() => setSandboxError("")} />}
              {sandboxNotice && <div className="text-xs text-muted-foreground">{sandboxNotice}</div>}
            </CardContent>
          </Card>

          <Card className="py-0">
            <CardContent className="py-4 space-y-4">
              <div className="grid grid-cols-1 gap-3 lg:grid-cols-2">
                <div className="space-y-2">
                  <label className="text-xs font-semibold text-muted-foreground">{t("sandboxTemplate")}</label>
                  <Select
                    value={sandboxSelectedTemplate}
                    onValueChange={handleSelectSandboxTemplate}
                  >
                    <SelectTrigger className="h-8 rounded-lg border-border/60 bg-popover/40 text-xs">
                      <SelectValue placeholder={t("sandboxTemplate")} />
                    </SelectTrigger>
                    <SelectContent>
                      {sandboxTemplates.map((item) => (
                        <SelectItem key={item.id} value={item.id}>
                          {item.name}
                        </SelectItem>
                      ))}
                    </SelectContent>
                  </Select>
                </div>
                <div className="space-y-2">
                  <label className="text-xs font-semibold text-muted-foreground">{t("nameOptional")}</label>
                  <Input
                    value={sandboxName}
                    onChange={(e) => setSandboxName(e.target.value)}
                    placeholder="cbx-node-dev-..."
                    className="h-8 rounded-lg border-border/60 bg-popover/40 text-xs"
                  />
                </div>
                <div className="space-y-2">
                  <label className="text-xs font-semibold text-muted-foreground">{t("sandboxOwner")}</label>
                  <Input
                    value={sandboxOwner}
                    onChange={(e) => setSandboxOwner(e.target.value)}
                    placeholder={t("sandboxOwnerHint")}
                    className="h-8 rounded-lg border-border/60 bg-popover/40 text-xs"
                  />
                </div>
                <div className="space-y-2">
                  <label className="text-xs font-semibold text-muted-foreground">{t("sandboxCommand")}</label>
                  <Input
                    value={sandboxCommand}
                    onChange={(e) => setSandboxCommand(e.target.value)}
                    placeholder={selectedSandboxTemplate?.default_command ?? "sleep infinity"}
                    className="h-8 rounded-lg border-border/60 bg-popover/40 text-xs font-mono"
                  />
                </div>
                <div className="space-y-2">
                  <label className="text-xs font-semibold text-muted-foreground">{t("cpus")}</label>
                  <Input
                    type="number"
                    min={1}
                    max={16}
                    value={sandboxCpu}
                    onChange={(e) => {
                      const value = e.target.value
                      setSandboxCpu(value === "" ? "" : Number(value))
                    }}
                    className="h-8 rounded-lg border-border/60 bg-popover/40 text-xs"
                  />
                </div>
                <div className="space-y-2">
                  <label className="text-xs font-semibold text-muted-foreground">{t("memoryMb")}</label>
                  <Input
                    type="number"
                    min={256}
                    max={65536}
                    value={sandboxMemoryMb}
                    onChange={(e) => {
                      const value = e.target.value
                      setSandboxMemoryMb(value === "" ? "" : Number(value))
                    }}
                    className="h-8 rounded-lg border-border/60 bg-popover/40 text-xs"
                  />
                </div>
                <div className="space-y-2">
                  <label className="text-xs font-semibold text-muted-foreground">{t("sandboxTtlHours")}</label>
                  <Input
                    type="number"
                    min={1}
                    max={168}
                    value={sandboxTtlHours}
                    onChange={(e) => {
                      const value = e.target.value
                      setSandboxTtlHours(value === "" ? "" : Number(value))
                    }}
                    className="h-8 rounded-lg border-border/60 bg-popover/40 text-xs"
                  />
                </div>
              </div>

              <div className="space-y-2">
                <label className="text-xs font-semibold text-muted-foreground">{t("sandboxEnvLines")}</label>
                <textarea
                  value={sandboxEnvLines}
                  onChange={(e) => setSandboxEnvLines(e.target.value)}
                  placeholder={t("sandboxEnvHint")}
                  className="min-h-[72px] w-full rounded-lg border border-border/60 bg-popover/40 px-2.5 py-2 text-xs text-foreground outline-hidden ring-ring/40 transition focus:ring-2"
                />
              </div>

              {selectedSandboxTemplate && (
                <div className="rounded-lg border border-border/50 bg-muted/25 px-3 py-2 text-xs text-muted-foreground">
                  <div className="flex flex-wrap items-center gap-2">
                    <span className="font-medium text-foreground">{selectedSandboxTemplate.name}</span>
                    <Badge variant="secondary" className="rounded-md px-1.5 py-0 text-[11px]">
                      {selectedSandboxTemplate.image}
                    </Badge>
                    <span>{selectedSandboxTemplate.description}</span>
                  </div>
                </div>
              )}

              <div className="flex justify-end">
                <Button
                  type="button"
                  size="xs"
                  className={cn(cardActionSecondary)}
                  disabled={sandboxCreating || !sandboxSelectedTemplate}
                  onClick={handleCreateSandbox}
                >
                  <span className={cn("mr-1", iconStroke, "[&_svg]:size-3")}>{I.plus}</span>
                  {sandboxCreating ? t("working") : t("sandboxCreate")}
                </Button>
              </div>
            </CardContent>
          </Card>

          <Card className="py-0">
            <CardContent className="py-0">
              <div className="border-b border-border/50 px-4 py-3 text-xs text-muted-foreground">
                {t("sandboxListDesc")}
              </div>
              {sandboxes.length === 0 ? (
                <div className="px-4 py-6">
                  <EmptyState
                    icon={I.server}
                    title={t("sandboxListEmptyTitle")}
                    description={t("sandboxListEmptyDesc")}
                    code={"bash scripts/setup-ai.sh --install"}
                  />
                </div>
              ) : (
                <ScrollArea className="max-h-[380px]">
                  <Table>
                    <TableHeader>
                      <TableRow>
                        <TableHead>{t("name")}</TableHead>
                        <TableHead>{t("sandboxTemplate")}</TableHead>
                        <TableHead>{t("status")}</TableHead>
                        <TableHead>{t("sandboxExpiresAt")}</TableHead>
                        <TableHead>{t("cpus")}</TableHead>
                        <TableHead>{t("memoryMb")}</TableHead>
                        <TableHead className="text-right">{t("actions")}</TableHead>
                      </TableRow>
                    </TableHeader>
                    <TableBody>
                      {sandboxes.map((item) => (
                        <TableRow key={item.id}>
                          <TableCell className="max-w-[280px]">
                            <div className="truncate text-xs font-medium text-foreground">{item.name}</div>
                            <div className="truncate font-mono text-[11px] text-muted-foreground">{item.short_id}</div>
                          </TableCell>
                          <TableCell className="text-xs text-muted-foreground max-w-[220px] truncate">
                            {item.template_id}
                          </TableCell>
                          <TableCell>
                            {item.state === "running" ? (
                              <Badge className="rounded-full gap-2 px-3 py-1 text-[11px] font-medium border border-brand-green/20 bg-brand-green/10 text-brand-green">
                                <span className="size-1.5 rounded-full bg-brand-green shadow-[0_0_10px_hsl(var(--brand-green)/0.6)]" />
                                {t("running")}
                              </Badge>
                            ) : (
                              <Badge
                                variant="secondary"
                                className="rounded-full gap-2 px-3 py-1 text-[11px] font-medium border border-border/60 bg-popover/40 text-muted-foreground"
                              >
                                <span className="size-1.5 rounded-full bg-destructive" />
                                {t("stopped")}
                              </Badge>
                            )}
                          </TableCell>
                          <TableCell className="text-xs text-muted-foreground max-w-[220px] truncate">
                            {formatSandboxTime(item.expires_at)}
                            {item.is_expired && (
                              <span className="ml-1 text-destructive">{t("sandboxExpired")}</span>
                            )}
                          </TableCell>
                          <TableCell className="text-xs text-muted-foreground">{item.cpu_cores}</TableCell>
                          <TableCell className="text-xs text-muted-foreground">{item.memory_mb}</TableCell>
                          <TableCell className="text-right">
                            <div className="inline-flex items-center gap-1">
                              <Button
                                type="button"
                                variant="outline"
                                size="xs"
                                className={cn(cardActionOutline)}
                                disabled={!!sandboxActingId}
                                onClick={() => handleSandboxAction("inspect", item)}
                              >
                                {t("inspect")}
                              </Button>
                              {item.state === "running" ? (
                                <Button
                                  type="button"
                                  variant="outline"
                                  size="xs"
                                  className={cn(cardActionOutline)}
                                  disabled={!!sandboxActingId}
                                  onClick={() => handleSandboxAction("stop", item)}
                                >
                                  {sandboxActingId === `stop:${item.id}` ? t("working") : t("stop")}
                                </Button>
                              ) : (
                                <Button
                                  type="button"
                                  variant="outline"
                                  size="xs"
                                  className={cn(cardActionOutline)}
                                  disabled={!!sandboxActingId}
                                  onClick={() => handleSandboxAction("start", item)}
                                >
                                  {sandboxActingId === `start:${item.id}` ? t("working") : t("start")}
                                </Button>
                              )}
                              <Button
                                type="button"
                                variant="outline"
                                size="xs"
                                className={cn(cardActionDanger)}
                                disabled={!!sandboxActingId}
                                onClick={() => handleSandboxAction("delete", item)}
                              >
                                {sandboxActingId === `delete:${item.id}` ? t("working") : t("delete")}
                              </Button>
                            </div>
                          </TableCell>
                        </TableRow>
                      ))}
                    </TableBody>
                  </Table>
                </ScrollArea>
              )}
            </CardContent>
          </Card>

          {(sandboxInspect || sandboxInspectError) && (
            <Card className="py-0">
              <CardContent className="py-4 space-y-3">
                <div className="flex flex-wrap items-center gap-2">
                  <div className="text-sm font-semibold text-foreground">{t("sandboxInspectTitle")}</div>
                  {sandboxInspect?.running ? (
                    <Badge className="rounded-md border border-brand-green/20 bg-brand-green/10 px-1.5 py-0 text-[11px] text-brand-green">
                      {t("running")}
                    </Badge>
                  ) : (
                    <Badge variant="secondary" className="rounded-md px-1.5 py-0 text-[11px]">
                      {t("stopped")}
                    </Badge>
                  )}
                </div>
                {sandboxInspectError && (
                  <ErrorInline message={sandboxInspectError} onDismiss={() => setSandboxInspectError("")} />
                )}
                {sandboxInspect && (
                  <div className="grid grid-cols-1 gap-2 text-xs text-muted-foreground lg:grid-cols-2">
                    <div>{t("name")}: <span className="text-foreground">{sandboxInspect.name}</span></div>
                    <div>ID: <span className="font-mono text-foreground">{sandboxInspect.short_id}</span></div>
                    <div>{t("image")}: <span className="font-mono text-foreground">{sandboxInspect.image}</span></div>
                    <div>{t("sandboxTemplate")}: <span className="text-foreground">{sandboxInspect.template_id}</span></div>
                    <div>{t("sandboxOwner")}: <span className="text-foreground">{sandboxInspect.owner}</span></div>
                    <div>{t("sandboxTtlHours")}: <span className="text-foreground">{sandboxInspect.ttl_hours}</span></div>
                    <div>{t("sandboxCreatedAt")}: <span className="text-foreground">{formatSandboxTime(sandboxInspect.created_at)}</span></div>
                    <div>{t("sandboxExpiresAt")}: <span className="text-foreground">{formatSandboxTime(sandboxInspect.expires_at)}</span></div>
                    <div>{t("cpus")}: <span className="text-foreground">{sandboxInspect.cpu_cores}</span></div>
                    <div>{t("memoryMb")}: <span className="text-foreground">{sandboxInspect.memory_mb}</span></div>
                  </div>
                )}
                {sandboxInspect && (
                  <div data-testid="sandbox-runtime-card" className="rounded-lg border border-border/50 bg-muted/25 px-3 py-2 text-xs">
                    <div className="mb-2 text-muted-foreground">{t("sandboxRuntimeUsage")}</div>
                    {sandboxRuntimeLoading ? (
                      <div className="text-muted-foreground">{t("working")}</div>
                    ) : sandboxRuntimeError ? (
                      <ErrorInline message={sandboxRuntimeError} onDismiss={() => setSandboxRuntimeError("")} />
                    ) : sandboxRuntimeUsage ? (
                      <>
                        <div className="grid grid-cols-1 gap-2 text-muted-foreground lg:grid-cols-2">
                          <div>{t("cpuUsage")}: <span className="text-foreground">{formatUsagePercent(sandboxRuntimeUsage.cpu_percent)}</span></div>
                          <div>
                            {t("memoryUsage")}: <span className="text-foreground">{formatUsageMegabytes(sandboxRuntimeUsage.memory_usage_mb)} / {formatUsageMegabytes(sandboxRuntimeUsage.memory_limit_mb)} ({formatUsagePercent(sandboxRuntimeUsage.memory_percent)})</span>
                          </div>
                          <div>{t("gpuMemory")}: <span className="text-foreground">{sandboxRuntimeUsage.gpu_processes.length > 0 ? sandboxRuntimeUsage.gpu_memory_used_human : "-"}</span></div>
                          <div>{t("gpuProcesses")}: <span className="text-foreground">{sandboxRuntimeUsage.gpu_processes.length}</span></div>
                        </div>
                        <div className="mt-2 text-muted-foreground">
                          {sandboxRuntimeUsage.running
                            ? sandboxRuntimeUsage.gpu_attribution_supported
                              ? sandboxRuntimeUsage.gpu_processes.length > 0
                                ? `${sandboxRuntimeUsage.gpu_processes.length} ${t("gpuProcesses").toLowerCase()} · ${new Set(sandboxRuntimeUsage.gpu_processes.map((process) => process.gpu_index)).size} ${t("gpuDevices").toLowerCase()}`
                                : t("sandboxGpuIdle")
                              : sandboxRuntimeUsage.gpu_message || t("gpuTelemetryUnavailable")
                            : t("stopped")}
                        </div>
                        {sandboxRuntimeUsage.gpu_processes.length > 0 && (
                          <div className="mt-2 flex flex-wrap gap-2">
                            {sandboxRuntimeUsage.gpu_processes.map((process) => (
                              <Badge
                                key={`${process.pid}-${process.gpu_index}-${process.process_name}`}
                                variant="secondary"
                                className="rounded-md px-1.5 py-0 text-[11px]"
                              >
                                {process.process_name} · GPU {process.gpu_index}
                                {process.memory_used_human ? ` · ${process.memory_used_human}` : ""}
                              </Badge>
                            ))}
                          </div>
                        )}
                      </>
                    ) : (
                      <div className="text-muted-foreground">-</div>
                    )}
                  </div>
                )}
                {sandboxInspect?.command && (
                  <div className="rounded-lg border border-border/50 bg-muted/25 px-3 py-2 text-xs">
                    <div className="mb-1 text-muted-foreground">{t("sandboxCommand")}</div>
                    <code className="font-mono text-foreground">{sandboxInspect.command}</code>
                  </div>
                )}
                {sandboxInspect?.env && sandboxInspect.env.length > 0 && (
                  <div className="rounded-lg border border-border/50 bg-muted/25 px-3 py-2 text-xs">
                    <div className="mb-1 text-muted-foreground">ENV</div>
                    <div className="font-mono text-foreground break-all">{sandboxInspect.env.join(" · ")}</div>
                  </div>
                )}
              </CardContent>
            </Card>
          )}

          <Card className="py-0">
            <CardContent className="py-4 space-y-3">
              <div className="text-sm font-semibold text-foreground">{t("sandboxExecTitle")}</div>
              <div className="flex flex-col gap-2 lg:flex-row">
                <Input
                  value={sandboxExecCommand}
                  onChange={(e) => setSandboxExecCommand(e.target.value)}
                  placeholder={t("sandboxExecPlaceholder")}
                  className="h-8 rounded-lg border-border/60 bg-popover/40 text-xs font-mono"
                />
                <Button type="button" size="xs" className={cn(cardActionSecondary)} onClick={handleSandboxExec} disabled={!sandboxExecCommand.trim() || !!sandboxActingId}>
                  <span className={cn("mr-1", iconStroke, "[&_svg]:size-3")}>{I.terminal}</span>
                  {sandboxActingId.startsWith("exec:") ? t("working") : t("sandboxExecRun")}
                </Button>
              </div>
              {sandboxExecResult && (
                <div className="space-y-2 rounded-lg border border-border/50 bg-muted/25 px-3 py-2 text-xs">
                  <div className="flex flex-wrap items-center gap-2 text-muted-foreground">
                    <span>{t("sandboxExecExitCode")}: {sandboxExecResult.exit_code ?? "-"}</span>
                    <Badge variant={sandboxExecResult.ok ? "secondary" : "outline"}>
                      {sandboxExecResult.ok ? t("running") : t("error")}
                    </Badge>
                  </div>
                  {!!sandboxExecResult.stdout && (
                    <div>
                      <div className="mb-1 text-muted-foreground">{t("sandboxExecStdout")}</div>
                      <pre className="whitespace-pre-wrap break-all font-mono text-foreground">{sandboxExecResult.stdout}</pre>
                    </div>
                  )}
                  {!!sandboxExecResult.stderr && (
                    <div>
                      <div className="mb-1 text-muted-foreground">{t("sandboxExecStderr")}</div>
                      <pre className="whitespace-pre-wrap break-all font-mono text-foreground">{sandboxExecResult.stderr}</pre>
                    </div>
                  )}
                  {!sandboxExecResult.stdout && !sandboxExecResult.stderr && !!sandboxExecResult.output && (
                    <div>
                      <div className="mb-1 text-muted-foreground">{t("sandboxExecOutput")}</div>
                      <pre className="whitespace-pre-wrap break-all font-mono text-foreground">{sandboxExecResult.output}</pre>
                    </div>
                  )}
                </div>
              )}
            </CardContent>
          </Card>

          <Card className="py-0">
            <CardContent className="py-0">
              <div className="border-b border-border/50 px-4 py-3 text-xs text-muted-foreground">
                {t("sandboxAuditDesc")}
              </div>
              {sandboxAudit.length === 0 ? (
                <div className="px-4 py-6">
                  <EmptyState
                    icon={I.fileText}
                    title={t("sandboxAuditEmptyTitle")}
                    description={t("sandboxAuditEmptyDesc")}
                  />
                </div>
              ) : (
                <ScrollArea className="max-h-[260px]">
                  <Table>
                    <TableHeader>
                      <TableRow>
                        <TableHead>{t("modifiedAt")}</TableHead>
                        <TableHead>{t("action")}</TableHead>
                        <TableHead>{t("name")}</TableHead>
                        <TableHead>{t("description")}</TableHead>
                      </TableRow>
                    </TableHeader>
                    <TableBody>
                      {sandboxAudit.map((event, index) => (
                        <TableRow key={`${event.timestamp}-${event.action}-${event.sandbox_id}-${index}`}>
                          <TableCell className="text-xs text-muted-foreground max-w-[220px] truncate">
                            {formatSandboxTime(event.timestamp)}
                          </TableCell>
                          <TableCell className="text-xs text-foreground">{event.action}</TableCell>
                          <TableCell className="text-xs text-muted-foreground max-w-[180px] truncate">
                            {event.sandbox_name || event.sandbox_id}
                          </TableCell>
                          <TableCell className="text-xs text-muted-foreground max-w-[480px] truncate">
                            {event.detail}
                          </TableCell>
                        </TableRow>
                      ))}
                    </TableBody>
                  </Table>
                </ScrollArea>
              )}
            </CardContent>
          </Card>
        </TabsContent>

        <TabsContent value="mcp" className="space-y-3">
          <Card className="py-0">
            <CardContent className="py-4 space-y-3">
              <div className="flex flex-wrap items-start justify-between gap-3">
                <div className="min-w-0 flex-1">
                  <div className="text-sm font-semibold text-foreground">{t("mcpRegistryTitle")}</div>
                  <div className="mt-1 text-xs text-muted-foreground">{t("mcpRegistryDesc")}</div>
                </div>
                <div className="flex flex-wrap items-center gap-2">
                  <Button
                    type="button"
                    variant="outline"
                    size="xs"
                    className={cn(cardActionOutline)}
                    onClick={refreshMcp}
                    disabled={mcpLoading}
                  >
                    <span className={cn("mr-1", iconStroke, "[&_svg]:size-3")}>{I.refresh}</span>
                    {mcpLoading ? t("working") : t("refresh")}
                  </Button>
                  <Button type="button" size="xs" data-testid="mcp-add-server" className={cn(cardActionSecondary)} onClick={handleAddMcpServer}>
                    <span className={cn("mr-1", iconStroke, "[&_svg]:size-3")}>{I.play}</span>
                    {t("mcpAddServer")}
                  </Button>
                  <Button
                    type="button"
                    size="xs"
                    data-testid="mcp-save-registry"
                    className={cn(cardActionSecondary)}
                    onClick={handleSaveMcpRegistry}
                    disabled={mcpActingId === "save"}
                  >
                    {mcpActingId === "save" ? t("working") : t("mcpSaveRegistry")}
                  </Button>
                </div>
              </div>
              {mcpError && <ErrorInline message={mcpError} onDismiss={() => setMcpError("")} />}
            </CardContent>
          </Card>

          <div className="grid gap-3 xl:grid-cols-[0.95fr_1.05fr]">
            <Card className="py-0">
              <CardContent className="py-0">
                <div className="border-b border-border/50 px-4 py-3 text-xs text-muted-foreground">{t("mcp")}</div>
                {mcpDrafts.length === 0 ? (
                  <div className="px-4 py-6">
                    <EmptyState icon={I.globe} title={t("mcpNoServersTitle")} description={t("mcpNoServersDesc")} />
                  </div>
                ) : (
                  <ScrollArea className="max-h-[520px]">
                    <Table>
                      <TableHeader>
                        <TableRow>
                          <TableHead>{t("name")}</TableHead>
                          <TableHead>{t("mcpStatus")}</TableHead>
                          <TableHead className="text-right">{t("actions")}</TableHead>
                        </TableRow>
                      </TableHeader>
                      <TableBody>
                        {mcpDrafts.map((server) => {
                          const runtime = mcpServers.find((item) => item.id === server.id)
                          const selected = mcpSelectedId === server.id
                          return (
                            <TableRow key={server.id} data-testid={`mcp-row-${server.id}`} className={selected ? "bg-primary/5" : ""}>
                              <TableCell>
                                <button type="button" data-testid={`mcp-select-${server.id}`} className="text-left" onClick={() => setMcpSelectedId(server.id)}>
                                  <div className="text-sm font-medium text-foreground">{server.name || server.id}</div>
                                  <div className="text-xs font-mono text-muted-foreground">{server.id}</div>
                                </button>
                              </TableCell>
                              <TableCell data-testid={`mcp-status-${server.id}`} className="text-xs text-muted-foreground">{runtime?.running ? t("mcpRunning") : (runtime?.status === "exited" ? t("mcpExited") : t("mcpStopped"))}</TableCell>
                              <TableCell className="text-right">
                                <div className="flex justify-end gap-2">
                                  <Button
                                    type="button"
                                    variant="outline"
                                    size="xs"
                                    data-testid={`mcp-toggle-${server.id}`}
                                    className={runtime?.running ? cn(cardActionDanger) : cn(cardActionSecondary)}
                                    onClick={() => handleMcpAction(runtime?.running ? "stop" : "start", server.id)}
                                    disabled={!!mcpActingId && mcpActingId !== `${runtime?.running ? "stop" : "start"}:${server.id}`}
                                  >
                                    <span className={cn("mr-1", iconStroke, "[&_svg]:size-3")}>{runtime?.running ? I.stop : I.play}</span>
                                    {mcpActingId === `${runtime?.running ? "stop" : "start"}:${server.id}` ? t("working") : runtime?.running ? t("stop") : t("start")}
                                  </Button>
                                  <Button type="button" variant="outline" size="xs" className={cn(cardActionDanger)} onClick={() => handleDeleteMcpServer(server.id)}>
                                    <span className={cn("mr-1", iconStroke, "[&_svg]:size-3")}>{I.trash}</span>
                                    {t("mcpDeleteServer")}
                                  </Button>
                                </div>
                              </TableCell>
                            </TableRow>
                          )
                        })}
                      </TableBody>
                    </Table>
                  </ScrollArea>
                )}
              </CardContent>
            </Card>

            <div className="space-y-3">
              <Card className="py-0">
                <CardContent className="py-4 space-y-3">
                  <div className="grid gap-3 md:grid-cols-2">
                    <div className="space-y-2">
                      <label className="text-xs font-semibold text-muted-foreground">{t("mcpServerId")}</label>
                      <Input
                        data-testid="mcp-input-id"
                        value={selectedMcpDraft?.id ?? ""}
                        onChange={(e) => updateSelectedMcpDraft((draft) => ({ ...draft, id: e.target.value }))}
                        className="h-8 rounded-lg border-border/60 bg-popover/40 text-xs font-mono"
                        disabled={!selectedMcpDraft}
                      />
                    </div>
                    <div className="space-y-2">
                      <label className="text-xs font-semibold text-muted-foreground">{t("name")}</label>
                      <Input
                        data-testid="mcp-input-name"
                        value={selectedMcpDraft?.name ?? ""}
                        onChange={(e) => updateSelectedMcpDraft((draft) => ({ ...draft, name: e.target.value }))}
                        className="h-8 rounded-lg border-border/60 bg-popover/40 text-xs"
                        disabled={!selectedMcpDraft}
                      />
                    </div>
                    <div className="space-y-2 md:col-span-2">
                      <label className="text-xs font-semibold text-muted-foreground">{t("mcpCommand")}</label>
                      <Input
                        data-testid="mcp-input-command"
                        value={selectedMcpDraft?.command ?? ""}
                        onChange={(e) => updateSelectedMcpDraft((draft) => ({ ...draft, command: e.target.value }))}
                        className="h-8 rounded-lg border-border/60 bg-popover/40 text-xs font-mono"
                        disabled={!selectedMcpDraft}
                      />
                    </div>
                    <div className="space-y-2 md:col-span-2">
                      <label className="text-xs font-semibold text-muted-foreground">{t("mcpArgs")}</label>
                      <textarea
                        data-testid="mcp-input-args"
                        value={(selectedMcpDraft?.args ?? []).join("\n")}
                        onChange={(e) => updateSelectedMcpDraft((draft) => ({ ...draft, args: e.target.value.split("\n").map((item) => item.trim()).filter(Boolean) }))}
                        placeholder={t("mcpArgsHint")}
                        className="min-h-[72px] w-full rounded-lg border border-border/60 bg-popover/40 px-2.5 py-2 text-xs text-foreground outline-hidden ring-ring/40 transition focus:ring-2"
                        disabled={!selectedMcpDraft}
                      />
                    </div>
                    <div className="space-y-2 md:col-span-2">
                      <label className="text-xs font-semibold text-muted-foreground">ENV</label>
                      <textarea
                        data-testid="mcp-input-env"
                        value={(selectedMcpDraft?.env ?? []).join("\n")}
                        onChange={(e) => updateSelectedMcpDraft((draft) => ({ ...draft, env: e.target.value.split("\n").map((item) => item.trim()).filter(Boolean) }))}
                        placeholder={t("mcpEnvHint")}
                        className="min-h-[72px] w-full rounded-lg border border-border/60 bg-popover/40 px-2.5 py-2 text-xs text-foreground outline-hidden ring-ring/40 transition focus:ring-2"
                        disabled={!selectedMcpDraft}
                      />
                    </div>
                    <div className="space-y-2 md:col-span-2">
                      <label className="text-xs font-semibold text-muted-foreground">{t("mcpWorkingDir")}</label>
                      <Input
                        data-testid="mcp-input-working-dir"
                        value={selectedMcpDraft?.working_dir ?? ""}
                        onChange={(e) => updateSelectedMcpDraft((draft) => ({ ...draft, working_dir: e.target.value }))}
                        className="h-8 rounded-lg border-border/60 bg-popover/40 text-xs font-mono"
                        disabled={!selectedMcpDraft}
                      />
                    </div>
                    <div className="space-y-2 md:col-span-2">
                      <label className="text-xs font-semibold text-muted-foreground">{t("mcpNotes")}</label>
                      <textarea
                        data-testid="mcp-input-notes"
                        value={selectedMcpDraft?.notes ?? ""}
                        onChange={(e) => updateSelectedMcpDraft((draft) => ({ ...draft, notes: e.target.value }))}
                        className="min-h-[72px] w-full rounded-lg border border-border/60 bg-popover/40 px-2.5 py-2 text-xs text-foreground outline-hidden ring-ring/40 transition focus:ring-2"
                        disabled={!selectedMcpDraft}
                      />
                    </div>
                  </div>
                  {selectedMcpStatus && (
                    <div data-testid="mcp-selected-status" className="rounded-lg border border-border/50 bg-muted/25 px-3 py-2 text-xs text-muted-foreground">
                      <div>{t("mcpStatus")}: <span className="text-foreground">{selectedMcpStatus.running ? t("mcpRunning") : (selectedMcpStatus.status === "exited" ? t("mcpExited") : t("mcpStopped"))}</span></div>
                      {selectedMcpStatus.pid && <div>PID: <span className="text-foreground">{selectedMcpStatus.pid}</span></div>}
                      {selectedMcpStatus.started_at && <div>{t("sandboxCreatedAt")}: <span className="text-foreground">{formatSandboxTime(selectedMcpStatus.started_at)}</span></div>}
                    </div>
                  )}
                </CardContent>
              </Card>

              <Card className="py-0">
                <CardContent className="py-4 space-y-3">
                  <div className="flex flex-wrap items-center gap-2">
                    <div className="text-sm font-semibold text-foreground">{t("mcpExportConfig")}</div>
                    <Select value={mcpExportClient} onValueChange={setMcpExportClient}>
                      <SelectTrigger className="h-8 w-[160px] rounded-lg border-border/60 bg-popover/40 text-xs">
                        <SelectValue placeholder={t("mcpExportClient")} />
                      </SelectTrigger>
                      <SelectContent>
                        <SelectItem value="codex">Codex</SelectItem>
                        <SelectItem value="claude">Claude</SelectItem>
                        <SelectItem value="cursor">Cursor</SelectItem>
                      </SelectContent>
                    </Select>
                    <Button type="button" size="xs" className={cn(cardActionOutline)} onClick={() => handleMcpAction("export")}>
                      {t("mcpExportConfig")}
                    </Button>
                    {mcpExportValue && (
                      <Button type="button" size="xs" className={cn(cardActionOutline)} onClick={() => navigator.clipboard.writeText(mcpExportValue)}>
                        <span className={cn("mr-1", iconStroke, "[&_svg]:size-3")}>{I.copy}</span>
                        {t("mcpCopyConfig")}
                      </Button>
                    )}
                  </div>
                  <textarea
                    value={mcpExportValue}
                    readOnly
                    className="min-h-[180px] w-full rounded-lg border border-border/60 bg-popover/40 px-2.5 py-2 text-xs text-foreground outline-hidden"
                  />
                </CardContent>
              </Card>

              <Card className="py-0">
                <CardContent className="py-4 space-y-3">
                  <div className="text-sm font-semibold text-foreground">{t("mcpLogs")}</div>
                  <div data-testid="mcp-logs-output" className="rounded-lg border border-border/50 bg-muted/25 px-3 py-2 font-mono text-[11px] text-muted-foreground min-h-[180px] whitespace-pre-wrap break-all">
                    {mcpLogs.length > 0 ? mcpLogs.join("\n") : "-"}
                  </div>
                </CardContent>
              </Card>
            </div>
          </div>
        </TabsContent>

        <TabsContent value="assistant" className="space-y-3">
          <Assistant t={t} />
        </TabsContent>
      </Tabs>
    </div>
  )
}
