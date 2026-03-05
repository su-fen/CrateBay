import { useCallback, useEffect, useMemo, useState } from "react"
import { invoke } from "@tauri-apps/api/core"
import { I } from "../icons"
import { EmptyState } from "../components/EmptyState"
import { ErrorInline } from "../components/ErrorDisplay"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Card, CardContent } from "@/components/ui/card"
import { ScrollArea } from "@/components/ui/scroll-area"
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table"
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs"
import { cn } from "@/lib/utils"
import { iconStroke, cardActionOutline } from "@/lib/styles"
import { Assistant } from "./Assistant"
import type { OllamaModelDto, OllamaStatusDto } from "../types"

type AiHubTab = "overview" | "models" | "sandboxes" | "mcp" | "assistant"

interface AiHubProps {
  t: (key: string) => string
}

function HubCard({
  title,
  desc,
  icon,
  toneClass,
  right,
  onClick,
}: {
  title: string
  desc: string
  icon: React.ReactNode
  toneClass: string
  right?: React.ReactNode
  onClick: () => void
}) {
  return (
    <button
      type="button"
      className={cn(
        "rounded-xl border border-border/50 bg-card/95 px-5 py-4 text-left shadow-sm transition-colors hover:bg-muted/35 hover:border-primary/30",
        "focus-visible:outline-hidden focus-visible:ring-[3px] focus-visible:ring-ring/50"
      )}
      onClick={onClick}
    >
      <div className="flex items-start justify-between gap-3">
        <div className="min-w-0 flex-1">
          <div className="flex items-center gap-2">
            <span className={cn("text-sm font-semibold text-foreground")}>{title}</span>
          </div>
          <div className="mt-1 text-xs text-muted-foreground">{desc}</div>
        </div>
        <div
          className={cn(
            "size-10 shrink-0 rounded-lg flex items-center justify-center",
            iconStroke,
            "[&_svg]:size-[18px]",
            toneClass
          )}
        >
          {icon}
        </div>
      </div>
      {right && <div className="mt-3">{right}</div>}
    </button>
  )
}

export function AiHub({ t }: AiHubProps) {
  const [tab, setTab] = useState<AiHubTab>("overview")
  const [ollamaStatus, setOllamaStatus] = useState<OllamaStatusDto | null>(null)
  const [ollamaModels, setOllamaModels] = useState<OllamaModelDto[]>([])
  const [ollamaLoading, setOllamaLoading] = useState(false)
  const [ollamaError, setOllamaError] = useState("")

  const refreshOllama = useCallback(async () => {
    setOllamaLoading(true)
    setOllamaError("")
    try {
      const status = await invoke<OllamaStatusDto>("ollama_status")
      setOllamaStatus(status)
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

  useEffect(() => {
    refreshOllama()
  }, [refreshOllama])

  const totalModelBytes = useMemo(
    () => ollamaModels.reduce((sum, m) => sum + (m.size_bytes || 0), 0),
    [ollamaModels]
  )

  const tabLabel = useMemo(() => {
    switch (tab) {
      case "overview":
        return t("overview")
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
            <Button
              type="button"
              variant="outline"
              size="xs"
              className={cn(cardActionOutline)}
              onClick={() => setTab("overview")}
            >
              <span className={cn("mr-1", iconStroke, "[&_svg]:size-3")}>{I.refresh}</span>
              {t("overview")}
            </Button>
          </div>
        </CardContent>
      </Card>

      <Tabs value={tab} onValueChange={(v) => setTab(v as AiHubTab)}>
        <TabsList variant="line" className="w-full justify-start">
          <TabsTrigger value="overview">{t("overview")}</TabsTrigger>
          <TabsTrigger value="models">{t("models")}</TabsTrigger>
          <TabsTrigger value="sandboxes">
            {t("sandboxes")}
            <Badge variant="secondary" className="rounded-md px-1.5 py-0 text-[10px]">
              {t("soon")}
            </Badge>
          </TabsTrigger>
          <TabsTrigger value="mcp">
            {t("mcp")}
            <Badge variant="secondary" className="rounded-md px-1.5 py-0 text-[10px]">
              {t("soon")}
            </Badge>
          </TabsTrigger>
          <TabsTrigger value="assistant">{t("assistant")}</TabsTrigger>
        </TabsList>

        <TabsContent value="overview" className="space-y-4">
          <div className="grid grid-cols-1 gap-3 md:grid-cols-2 xl:grid-cols-3">
            <HubCard
              title={t("models")}
              desc={t("aiModelsCardDesc")}
              icon={I.layers}
              toneClass="bg-brand-cyan/10 text-brand-cyan"
              right={
                <div className="flex flex-wrap items-center gap-2 text-xs text-muted-foreground">
                  <span className="rounded-md border border-border/50 bg-muted/60 px-2 py-1 font-mono">
                    ollama
                  </span>
                  <span>{t("aiModelsCardHint")}</span>
                  {ollamaStatus?.running && (
                    <Badge
                      variant="secondary"
                      className="rounded-md border border-brand-green/15 bg-brand-green/10 px-1.5 py-0 text-[11px] text-brand-green"
                    >
                      {ollamaModels.length} {t("models").toLowerCase()}
                    </Badge>
                  )}
                </div>
              }
              onClick={() => setTab("models")}
            />

            <HubCard
              title={t("sandboxes")}
              desc={t("aiSandboxesCardDesc")}
              icon={I.server}
              toneClass="bg-brand-green/10 text-brand-green"
              right={
                <div className="flex items-center gap-2 text-xs text-muted-foreground">
                  <span className="rounded-md border border-border/50 bg-muted/60 px-2 py-1 font-mono">
                    opensandbox
                  </span>
                  <span>{t("aiSandboxesCardHint")}</span>
                </div>
              }
              onClick={() => setTab("sandboxes")}
            />

            <HubCard
              title={t("mcp")}
              desc={t("aiMcpCardDesc")}
              icon={I.globe}
              toneClass="bg-primary/10 text-primary"
              right={
                <div className="flex items-center gap-2 text-xs text-muted-foreground">
                  <span className="rounded-md border border-border/50 bg-muted/60 px-2 py-1 font-mono">
                    mcp
                  </span>
                  <span>{t("aiMcpCardHint")}</span>
                </div>
              }
              onClick={() => setTab("mcp")}
            />

            <HubCard
              title={t("assistant")}
              desc={t("assistantDesc")}
              icon={I.aiAssistant}
              toneClass="bg-primary/10 text-primary"
              right={
                <div className="flex items-center gap-2 text-xs text-muted-foreground">
                  <span className="rounded-md border border-border/50 bg-muted/60 px-2 py-1 font-mono">
                    skills
                  </span>
                  <span>{t("aiAssistantCardHint")}</span>
                </div>
              }
              onClick={() => setTab("assistant")}
            />
          </div>
        </TabsContent>

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
                    {t("aiBaseUrl")}:{" "}
                    <code className="rounded-md border border-border/60 bg-muted/60 px-1.5 py-0.5 font-mono text-[11px] text-foreground">
                      {ollamaStatus?.base_url ?? "-"}
                    </code>
                    {ollamaStatus?.installed === false && (
                      <>
                        {" "}
                        <span className="text-muted-foreground/40">•</span>{" "}
                        <span>{t("notInstalled")}</span>
                      </>
                    )}
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
                <div className="border-b border-border/50 px-4 py-3">
                  <div className="flex flex-wrap items-center gap-2 text-xs text-muted-foreground">
                    <span className="font-medium text-foreground">{ollamaModels.length}</span>{" "}
                    {t("models")}
                    <span className="text-muted-foreground/40">•</span>
                    <span>
                      <span className="text-brand-cyan font-medium">
                        {Math.round((totalModelBytes / (1024 * 1024 * 1024)) * 10) / 10} GB
                      </span>{" "}
                      {t("aiModelStorage")}
                    </span>
                  </div>
                </div>

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
                          return (
                            <TableRow key={m.name}>
                              <TableCell className="font-mono text-xs max-w-[420px] truncate">
                                {m.name}
                              </TableCell>
                              <TableCell className="text-xs text-muted-foreground">
                                {m.size_human}
                              </TableCell>
                              <TableCell className="text-xs text-muted-foreground max-w-[420px] truncate">
                                {details || "-"}
                              </TableCell>
                              <TableCell className="text-xs text-muted-foreground max-w-[260px] truncate">
                                {m.modified_at || "-"}
                              </TableCell>
                              <TableCell className="text-right">
                                <Button
                                  type="button"
                                  variant="outline"
                                  size="xs"
                                  className={cn(cardActionOutline)}
                                  onClick={() =>
                                    navigator.clipboard.writeText(`ollama run ${m.name}`)
                                  }
                                >
                                  <span className={cn("mr-1", iconStroke, "[&_svg]:size-3")}>{I.copy}</span>
                                  {t("copy")}
                                </Button>
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
          <EmptyState
            icon={I.server}
            title={t("aiSandboxesTitle")}
            description={t("aiSandboxesDesc")}
            code={"cat tools/opensandbox/README.md"}
          />
        </TabsContent>

        <TabsContent value="mcp" className="space-y-3">
          <EmptyState
            icon={I.globe}
            title={t("aiMcpTitle")}
            description={t("aiMcpDesc")}
          />
        </TabsContent>

        <TabsContent value="assistant" className="space-y-3">
          <Assistant t={t} />
        </TabsContent>
      </Tabs>
    </div>
  )
}
