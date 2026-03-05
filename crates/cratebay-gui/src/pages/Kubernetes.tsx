import { useCallback, useEffect, useMemo, useState } from "react"
import { invoke } from "@tauri-apps/api/core"
import { I } from "../icons"
import { EmptyState } from "../components/EmptyState"
import { ErrorInline } from "../components/ErrorDisplay"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Card, CardAction, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card"
import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import { ScrollArea } from "@/components/ui/scroll-area"
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select"
import { Separator } from "@/components/ui/separator"
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table"
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs"
import { cn } from "@/lib/utils"
import type { K3sStatusDto, K8sDeployment, K8sPod, K8sService } from "../types"

interface KubernetesProps {
  t: (key: string) => string
}

type K8sTab = "overview" | "pods" | "services" | "deployments"

const iconStroke =
  "[&_svg]:fill-none [&_svg]:stroke-current [&_svg]:[stroke-width:2] [&_svg]:[stroke-linecap:round] [&_svg]:[stroke-linejoin:round]"
const cardActionOutline =
  "hover:border-primary/45 hover:bg-primary/12 hover:text-primary motion-safe:hover:-translate-y-px motion-safe:hover:shadow-sm motion-safe:active:translate-y-0"
const cardActionDanger =
  "border-destructive/30 text-destructive hover:border-destructive/45 hover:bg-destructive/10 hover:text-destructive motion-safe:hover:-translate-y-px motion-safe:hover:shadow-sm motion-safe:active:translate-y-0"
const panelCard = "border border-border/50 bg-muted/25"

const ALL_NAMESPACES = "__all__"

function toneForPodStatus(status: string): "ok" | "warn" | "bad" {
  const s = status.toLowerCase()
  if (s.includes("run") || s.includes("complete") || s.includes("succeed")) return "ok"
  if (s.includes("pend") || s.includes("init") || s.includes("containercreating")) return "warn"
  return "bad"
}

function badgeClassForTone(tone: "ok" | "warn" | "bad") {
  if (tone === "ok") return "border-brand-green/20 bg-brand-green/10 text-brand-green"
  if (tone === "warn") return "border-brand-cyan/20 bg-brand-cyan/10 text-brand-cyan"
  return "border-destructive/20 bg-destructive/10 text-destructive"
}

export function Kubernetes({ t }: KubernetesProps) {
  // K3s cluster state
  const [status, setStatus] = useState<K3sStatusDto | null>(null)
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState("")
  const [acting, setActing] = useState("")

  // K8s dashboard state
  const [tab, setTab] = useState<K8sTab>("overview")
  const [namespace, setNamespace] = useState("")
  const [namespaces, setNamespaces] = useState<string[]>([])
  const [pods, setPods] = useState<K8sPod[]>([])
  const [services, setServices] = useState<K8sService[]>([])
  const [deployments, setDeployments] = useState<K8sDeployment[]>([])
  const [k8sLoading, setK8sLoading] = useState(false)
  const [k8sError, setK8sError] = useState("")

  // Pod logs modal
  const [logPod, setLogPod] = useState<K8sPod | null>(null)
  const [podLogs, setPodLogs] = useState("")
  const [logsLoading, setLogsLoading] = useState(false)

  const isInstalled = status?.installed ?? false
  const isRunning = status?.running ?? false

  // ── K3s status polling ──────────────────────────────────────────────
  const fetchStatus = useCallback(async () => {
    try {
      const s = await invoke<K3sStatusDto>("k3s_status")
      setStatus(s)
      setError("")
    } catch (e: unknown) {
      setError(String(e))
    } finally {
      setLoading(false)
    }
  }, [])

  useEffect(() => {
    fetchStatus()
    const iv = setInterval(fetchStatus, 5000)
    return () => clearInterval(iv)
  }, [fetchStatus])

  const doAction = async (action: string, label: string) => {
    setActing(label)
    setError("")
    try {
      await invoke(action)
      await fetchStatus()
    } catch (e: unknown) {
      setError(String(e))
    } finally {
      setActing("")
    }
  }

  // ── K8s dashboard data fetching ─────────────────────────────────────
  const fetchK8sData = useCallback(async () => {
    if (!status?.running) return
    setK8sLoading(true)
    setK8sError("")
    try {
      const [ns, p, s, d] = await Promise.all([
        invoke<string[]>("k8s_list_namespaces"),
        invoke<K8sPod[]>("k8s_list_pods", { namespace: namespace || null }),
        invoke<K8sService[]>("k8s_list_services", { namespace: namespace || null }),
        invoke<K8sDeployment[]>("k8s_list_deployments", { namespace: namespace || null }),
      ])
      setNamespaces(ns)
      setPods(p)
      setServices(s)
      setDeployments(d)
    } catch (e: unknown) {
      setK8sError(String(e))
    } finally {
      setK8sLoading(false)
    }
  }, [namespace, status?.running])

  useEffect(() => {
    if (status?.running) {
      fetchK8sData()
      const iv = setInterval(fetchK8sData, 10000)
      return () => clearInterval(iv)
    }
  }, [fetchK8sData, status?.running])

  const fetchPodLogs = async (pod: K8sPod) => {
    setLogPod(pod)
    setLogsLoading(true)
    setPodLogs("")
    try {
      const logs = await invoke<string>("k8s_pod_logs", {
        name: pod.name,
        namespace: pod.namespace,
        tail: 200,
      })
      setPodLogs(logs)
    } catch (e: unknown) {
      setPodLogs(`Error: ${String(e)}`)
    } finally {
      setLogsLoading(false)
    }
  }

  const nsSelectValue = namespace ? namespace : ALL_NAMESPACES
  const kubeconfigPath = status?.kubeconfig_path || ""

  const podRunningCount = useMemo(
    () => pods.filter((p) => toneForPodStatus(p.status) === "ok").length,
    [pods]
  )
  const servicesClusterIpCount = useMemo(
    () => services.filter((s) => s.service_type === "ClusterIP").length,
    [services]
  )
  const deploymentsAvailableCount = useMemo(
    () => deployments.filter((d) => d.available > 0).length,
    [deployments]
  )

  const tabCounts: Record<K8sTab, number> = {
    overview: 0,
    pods: pods.length,
    services: services.length,
    deployments: deployments.length,
  }

  return (
    <div className="space-y-4">
      {/* Toolbar */}
      <div className="flex items-center gap-2">
        <Button type="button" variant="outline" onClick={fetchStatus} disabled={loading}>
          <span className={cn("mr-1", iconStroke, "[&_svg]:size-4", loading && "animate-spin")}>
            {I.refresh}
          </span>
          {loading ? t("loading") : t("refresh")}
        </Button>

        {isRunning && (
          <>
            <Separator orientation="vertical" className="mx-1 h-6" />
            <Select
              value={nsSelectValue}
              onValueChange={(v) => setNamespace(v === ALL_NAMESPACES ? "" : v)}
            >
              <SelectTrigger size="sm" className="w-[220px] justify-between" title={t("namespace")}>
                <SelectValue placeholder={t("namespace")} />
              </SelectTrigger>
              <SelectContent align="start">
                <SelectItem value={ALL_NAMESPACES}>{t("allNamespaces")}</SelectItem>
                {namespaces.map((ns) => (
                  <SelectItem key={ns} value={ns}>
                    {ns}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>

            {k8sLoading && (
              <Badge
                variant="secondary"
                className="rounded-full gap-2 px-3 py-1 text-xs font-medium text-muted-foreground border border-border/60 bg-popover/40"
              >
                <span className="size-1.5 rounded-full bg-primary animate-pulse" />
                {t("loading")}
              </Badge>
            )}
          </>
        )}

        <div className="flex-1" />
      </div>

      {error && <ErrorInline message={error} onDismiss={() => setError("")} />}
      {k8sError && <ErrorInline message={k8sError} onDismiss={() => setK8sError("")} />}

      {/* K3s Cluster Status */}
      <Card className="py-0">
        <CardHeader className="border-b border-border/50 py-4">
          <div className="flex items-center gap-3">
            <div
              className={cn(
                "size-10 shrink-0 rounded-xl flex items-center justify-center border bg-muted/30 text-muted-foreground",
                iconStroke,
                "[&_svg]:size-5",
                isRunning && "border-brand-green/20 bg-brand-green/10 text-brand-green"
              )}
            >
              {I.kubernetes}
            </div>
            <div className="min-w-0">
              <CardTitle className="text-base">{t("k3sCluster")}</CardTitle>
              <CardDescription className="text-xs">
                {status?.version ? `v${status.version}` : t("notInstalled")}
              </CardDescription>
            </div>
          </div>

          <CardAction className="flex items-center gap-2">
            {!isInstalled && (
              <Button
                type="button"
                onClick={() => doAction("k3s_install", "install")}
                disabled={!!acting}
              >
                <span className={cn("mr-1", iconStroke, "[&_svg]:size-4")}>{I.plus}</span>
                {acting === "install" ? t("working") : t("installK3s")}
              </Button>
            )}

            {isInstalled && !isRunning && (
              <Button
                type="button"
                onClick={() => doAction("k3s_start", "start")}
                disabled={!!acting}
              >
                <span className={cn("mr-1", iconStroke, "[&_svg]:size-4")}>{I.play}</span>
                {acting === "start" ? t("working") : t("startCluster")}
              </Button>
            )}

            {isInstalled && isRunning && (
              <Button
                type="button"
                variant="outline"
                onClick={() => doAction("k3s_stop", "stop")}
                disabled={!!acting}
              >
                <span className={cn("mr-1", iconStroke, "[&_svg]:size-4")}>{I.stop}</span>
                {acting === "stop" ? t("working") : t("stopCluster")}
              </Button>
            )}

            <Button
              type="button"
              variant="outline"
              onClick={() => doAction("k3s_uninstall", "uninstall")}
              disabled={!!acting || isRunning}
              title={isRunning ? t("stopCluster") : ""}
              className={cardActionDanger}
            >
              <span className={cn("mr-1", iconStroke, "[&_svg]:size-4")}>{I.trash}</span>
              {acting === "uninstall" ? t("working") : t("uninstallK3s")}
            </Button>
          </CardAction>
        </CardHeader>

        <CardContent className="py-4 space-y-4">
          <div className="grid grid-cols-1 gap-3 md:grid-cols-3">
            <div className={cn("rounded-lg p-3", panelCard)}>
              <div className="text-xs font-medium text-muted-foreground">{t("clusterStatus")}</div>
              <div className="mt-1">
                <Badge
                  variant="secondary"
                  className={cn(
                    "rounded-md",
                    isInstalled
                      ? "border-primary/15 bg-primary/10 text-primary"
                      : "border-border/70 bg-popover/40 text-muted-foreground"
                  )}
                >
                  {isInstalled ? t("installed") : t("notInstalled")}
                </Badge>
              </div>
            </div>

            <div className={cn("rounded-lg p-3", panelCard)}>
              <div className="text-xs font-medium text-muted-foreground">{t("status")}</div>
              <div className="mt-1">
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
              </div>
            </div>

            <div className={cn("rounded-lg p-3", panelCard)}>
              <div className="text-xs font-medium text-muted-foreground">{t("nodeCount")}</div>
              <div className="mt-1 text-sm font-semibold text-foreground">
                {status?.node_count ?? 0}
              </div>
            </div>
          </div>

          <div className={cn("rounded-lg p-3", panelCard)}>
            <div className="flex items-end gap-3">
              <div className="min-w-0 flex-1 space-y-1">
                <div className="text-xs font-medium text-muted-foreground">{t("kubeconfig")}</div>
                <code className="block w-full overflow-auto rounded-md border border-border/60 bg-muted/70 px-3 py-2 text-xs font-mono text-foreground">
                  {kubeconfigPath || "-"}
                </code>
              </div>
              <Button
                type="button"
                variant="outline"
                size="xs"
                className={cn("self-end", cardActionOutline)}
                disabled={!kubeconfigPath}
                onClick={() => navigator.clipboard.writeText(kubeconfigPath)}
              >
                <span className={cn("mr-1", iconStroke, "[&_svg]:size-3")}>{I.copy}</span>
                {t("copy")}
              </Button>
            </div>
          </div>
        </CardContent>
      </Card>

      {/* K8s Dashboard */}
      {isRunning && (
        <Tabs value={tab} onValueChange={(v) => setTab(v as K8sTab)}>
          <TabsList variant="line" className="w-full justify-start">
            <TabsTrigger value="overview" data-testid="k8s-tab-overview">
              {t("overview")}
            </TabsTrigger>
            <TabsTrigger value="pods" data-testid="k8s-tab-pods">
              {t("pods")}
              <Badge variant="secondary" className="rounded-md px-1.5 py-0 text-[10px]">
                {tabCounts.pods}
              </Badge>
            </TabsTrigger>
            <TabsTrigger value="services" data-testid="k8s-tab-services">
              {t("services")}
              <Badge variant="secondary" className="rounded-md px-1.5 py-0 text-[10px]">
                {tabCounts.services}
              </Badge>
            </TabsTrigger>
            <TabsTrigger value="deployments" data-testid="k8s-tab-deployments">
              {t("deployments")}
              <Badge variant="secondary" className="rounded-md px-1.5 py-0 text-[10px]">
                {tabCounts.deployments}
              </Badge>
            </TabsTrigger>
          </TabsList>

          <TabsContent value="overview" className="space-y-4">
            <div className="grid grid-cols-1 gap-3 md:grid-cols-2 lg:grid-cols-4">
              <button
                type="button"
                className="rounded-xl border border-border/50 bg-card/95 px-5 py-4 text-left shadow-sm transition-colors hover:bg-muted/35 hover:border-primary/30"
                onClick={() => setTab("pods")}
              >
                <div className="flex items-center justify-between gap-3">
                  <div className="text-xs font-medium text-muted-foreground">{t("pods")}</div>
                  <div className={cn("text-muted-foreground", iconStroke, "[&_svg]:size-4")}>{I.box}</div>
                </div>
                <div className="mt-2 text-2xl font-semibold text-foreground">{pods.length}</div>
                <div className="mt-1 text-xs text-muted-foreground">
                  <span className="text-brand-green font-medium">{podRunningCount}</span>{" "}
                  {t("running").toLowerCase()}
                </div>
              </button>

              <button
                type="button"
                className="rounded-xl border border-border/50 bg-card/95 px-5 py-4 text-left shadow-sm transition-colors hover:bg-muted/35 hover:border-primary/30"
                onClick={() => setTab("services")}
              >
                <div className="flex items-center justify-between gap-3">
                  <div className="text-xs font-medium text-muted-foreground">{t("services")}</div>
                  <div className={cn("text-muted-foreground", iconStroke, "[&_svg]:size-4")}>{I.globe}</div>
                </div>
                <div className="mt-2 text-2xl font-semibold text-foreground">{services.length}</div>
                <div className="mt-1 text-xs text-muted-foreground">
                  <span className="text-brand-cyan font-medium">{servicesClusterIpCount}</span> ClusterIP
                </div>
              </button>

              <button
                type="button"
                className="rounded-xl border border-border/50 bg-card/95 px-5 py-4 text-left shadow-sm transition-colors hover:bg-muted/35 hover:border-primary/30"
                onClick={() => setTab("deployments")}
              >
                <div className="flex items-center justify-between gap-3">
                  <div className="text-xs font-medium text-muted-foreground">{t("deployments")}</div>
                  <div className={cn("text-muted-foreground", iconStroke, "[&_svg]:size-4")}>{I.layers}</div>
                </div>
                <div className="mt-2 text-2xl font-semibold text-foreground">{deployments.length}</div>
                <div className="mt-1 text-xs text-muted-foreground">
                  <span className="text-brand-green font-medium">{deploymentsAvailableCount}</span>{" "}
                  {t("available").toLowerCase()}
                </div>
              </button>

              <div className="rounded-xl border border-border/50 bg-card/95 px-5 py-4 shadow-sm">
                <div className="flex items-center justify-between gap-3">
                  <div className="text-xs font-medium text-muted-foreground">{t("namespace")}</div>
                  <div className={cn("text-muted-foreground", iconStroke, "[&_svg]:size-4")}>{I.server}</div>
                </div>
                <div className="mt-2 text-sm font-semibold text-foreground truncate">
                  {namespace || t("allNamespaces").toLowerCase()}
                </div>
                <div className="mt-1 text-xs text-muted-foreground">{namespaces.length} namespaces</div>
              </div>
            </div>
          </TabsContent>

          <TabsContent value="pods" className="space-y-3">
            {pods.length === 0 ? (
              <EmptyState
                icon={I.box}
                title={t("noPods")}
                description="No pod resources found in the current namespace"
              />
            ) : (
              <Card className="py-0">
                <CardHeader className="border-b border-border/50 py-4">
                  <div className="flex flex-wrap items-center gap-2 text-xs text-muted-foreground">
                    <span className="font-medium text-foreground">{pods.length}</span> {t("pods")}
                    <span className="text-muted-foreground/40">•</span>
                    <span>
                      <span className="text-brand-green font-medium">{podRunningCount}</span>{" "}
                      {t("running").toLowerCase()}
                    </span>
                  </div>
                </CardHeader>
                <CardContent className="py-0">
                  <div className="max-h-[520px] overflow-auto">
                    <Table>
                      <TableHeader>
                        <TableRow>
                          <TableHead>{t("name")}</TableHead>
                          <TableHead>{t("namespace")}</TableHead>
                          <TableHead>{t("status")}</TableHead>
                          <TableHead>{t("ready")}</TableHead>
                          <TableHead>{t("restarts")}</TableHead>
                          <TableHead>{t("age")}</TableHead>
                          <TableHead className="text-right">{t("actions")}</TableHead>
                        </TableRow>
                      </TableHeader>
                      <TableBody>
                        {pods.map((pod) => {
                          const tone = toneForPodStatus(pod.status)
                          return (
                            <TableRow key={`${pod.namespace}/${pod.name}`}>
                              <TableCell className="font-mono text-xs max-w-[420px] truncate">
                                {pod.name}
                              </TableCell>
                              <TableCell>
                                <Badge
                                  variant="secondary"
                                  className="rounded-md border border-brand-cyan/15 bg-brand-cyan/10 px-1.5 py-0 text-[11px] text-brand-cyan"
                                >
                                  {pod.namespace}
                                </Badge>
                              </TableCell>
                              <TableCell>
                                <Badge
                                  variant="secondary"
                                  className={cn("rounded-md", badgeClassForTone(tone))}
                                >
                                  {pod.status}
                                </Badge>
                              </TableCell>
                              <TableCell className="font-mono text-xs">{pod.ready}</TableCell>
                              <TableCell className="font-mono text-xs">{pod.restarts}</TableCell>
                              <TableCell className="text-xs">{pod.age}</TableCell>
                              <TableCell className="text-right">
                                <Button
                                  type="button"
                                  variant="outline"
                                  size="xs"
                                  className={cardActionOutline}
                                  onClick={() => fetchPodLogs(pod)}
                                  title={t("podLogs")}
                                >
                                  <span className={cn("mr-1", iconStroke, "[&_svg]:size-3")}>
                                    {I.terminal}
                                  </span>
                                  {t("logs")}
                                </Button>
                              </TableCell>
                            </TableRow>
                          )
                        })}
                      </TableBody>
                    </Table>
                  </div>
                </CardContent>
              </Card>
            )}
          </TabsContent>

          <TabsContent value="services" className="space-y-3">
            {services.length === 0 ? (
              <EmptyState
                icon={I.globe}
                title={t("noServices")}
                description="No service resources found in the current namespace"
              />
            ) : (
              <Card className="py-0">
                <CardHeader className="border-b border-border/50 py-4">
                  <div className="flex flex-wrap items-center gap-2 text-xs text-muted-foreground">
                    <span className="font-medium text-foreground">{services.length}</span> {t("services")}
                    <span className="text-muted-foreground/40">•</span>
                    <span>
                      <span className="text-brand-cyan font-medium">{servicesClusterIpCount}</span> ClusterIP
                    </span>
                  </div>
                </CardHeader>
                <CardContent className="py-0">
                  <div className="max-h-[520px] overflow-auto">
                    <Table>
                      <TableHeader>
                        <TableRow>
                          <TableHead>{t("name")}</TableHead>
                          <TableHead>{t("namespace")}</TableHead>
                          <TableHead>{t("type")}</TableHead>
                          <TableHead>{t("clusterIp")}</TableHead>
                          <TableHead>{t("ports")}</TableHead>
                        </TableRow>
                      </TableHeader>
                      <TableBody>
                        {services.map((svc) => (
                          <TableRow key={`${svc.namespace}/${svc.name}`}>
                            <TableCell className="font-mono text-xs max-w-[420px] truncate">
                              {svc.name}
                            </TableCell>
                            <TableCell>
                              <Badge
                                variant="secondary"
                                className="rounded-md border border-brand-cyan/15 bg-brand-cyan/10 px-1.5 py-0 text-[11px] text-brand-cyan"
                              >
                                {svc.namespace}
                              </Badge>
                            </TableCell>
                            <TableCell>
                        <Badge
                          variant="secondary"
                          className={cn(
                            "rounded-md border border-border/50",
                            svc.service_type === "ClusterIP"
                              ? "border-brand-cyan/25 bg-brand-cyan/10 text-brand-cyan"
                              : "border-primary/20 bg-primary/10 text-primary"
                          )}
                        >
                                {svc.service_type}
                              </Badge>
                            </TableCell>
                            <TableCell className="font-mono text-xs">{svc.cluster_ip}</TableCell>
                            <TableCell className="font-mono text-xs">{svc.ports}</TableCell>
                          </TableRow>
                        ))}
                      </TableBody>
                    </Table>
                  </div>
                </CardContent>
              </Card>
            )}
          </TabsContent>

          <TabsContent value="deployments" className="space-y-3">
            {deployments.length === 0 ? (
              <EmptyState
                icon={I.layers}
                title={t("noDeployments")}
                description="No deployment resources found in the current namespace"
              />
            ) : (
              <Card className="py-0">
                <CardHeader className="border-b border-border/50 py-4">
                  <div className="flex flex-wrap items-center gap-2 text-xs text-muted-foreground">
                    <span className="font-medium text-foreground">{deployments.length}</span>{" "}
                    {t("deployments")}
                    <span className="text-muted-foreground/40">•</span>
                    <span>
                      <span className="text-brand-green font-medium">{deploymentsAvailableCount}</span>{" "}
                      {t("available").toLowerCase()}
                    </span>
                  </div>
                </CardHeader>
                <CardContent className="py-0">
                  <div className="max-h-[520px] overflow-auto">
                    <Table>
                      <TableHeader>
                        <TableRow>
                          <TableHead>{t("name")}</TableHead>
                          <TableHead>{t("namespace")}</TableHead>
                          <TableHead>{t("ready")}</TableHead>
                          <TableHead>{t("upToDate")}</TableHead>
                          <TableHead>{t("available")}</TableHead>
                          <TableHead>{t("age")}</TableHead>
                        </TableRow>
                      </TableHeader>
                      <TableBody>
                        {deployments.map((dep) => (
                          <TableRow key={`${dep.namespace}/${dep.name}`}>
                            <TableCell className="font-mono text-xs max-w-[420px] truncate">
                              {dep.name}
                            </TableCell>
                            <TableCell>
                              <Badge
                                variant="secondary"
                                className="rounded-md border border-brand-cyan/15 bg-brand-cyan/10 px-1.5 py-0 text-[11px] text-brand-cyan"
                              >
                                {dep.namespace}
                              </Badge>
                            </TableCell>
                            <TableCell>
                              <Badge
                                variant="secondary"
                                className={cn(
                                  "rounded-md",
                                  dep.available > 0
                                    ? "border-brand-green/20 bg-brand-green/10 text-brand-green"
                                    : "border-brand-cyan/20 bg-brand-cyan/10 text-brand-cyan"
                                )}
                              >
                                {dep.ready}
                              </Badge>
                            </TableCell>
                            <TableCell className="font-mono text-xs">{dep.up_to_date}</TableCell>
                            <TableCell className="font-mono text-xs">{dep.available}</TableCell>
                            <TableCell className="text-xs">{dep.age}</TableCell>
                          </TableRow>
                        ))}
                      </TableBody>
                    </Table>
                  </div>
                </CardContent>
              </Card>
            )}
          </TabsContent>
        </Tabs>
      )}

      {/* Pod Logs Modal */}
      <Dialog
        open={Boolean(logPod)}
        onOpenChange={(open) => {
          if (!open) setLogPod(null)
        }}
      >
        <DialogContent className="sm:max-w-3xl" data-testid="k8s-dialog-pod-logs">
          <DialogHeader>
            <DialogTitle>{t("podLogs")}</DialogTitle>
          </DialogHeader>

          {logPod && (
            <div className="space-y-3">
              <div className="flex flex-wrap items-center justify-between gap-2">
                <div className="min-w-0">
                  <div className="text-xs text-muted-foreground">{t("name")}</div>
                  <div className="font-mono text-sm font-semibold text-foreground truncate">
                    {logPod.name}
                  </div>
                </div>

                <div className="flex items-center gap-2">
                  <Badge
                    variant="secondary"
                    className={cn("rounded-md", badgeClassForTone(toneForPodStatus(logPod.status)))}
                  >
                    {logPod.status}
                  </Badge>
                  <Button
                    type="button"
                    variant="outline"
                    size="xs"
                    className={cardActionOutline}
                    onClick={() => navigator.clipboard.writeText(podLogs)}
                    disabled={!podLogs}
                    title={t("copy")}
                  >
                    <span className={cn("mr-1", iconStroke, "[&_svg]:size-3")}>{I.copy}</span>
                    {t("copy")}
                  </Button>
                  <Button
                    type="button"
                    variant="outline"
                    size="xs"
                    className={cardActionOutline}
                    onClick={() => fetchPodLogs(logPod)}
                    disabled={logsLoading}
                    title={t("refresh")}
                  >
                    <span className={cn("mr-1", iconStroke, "[&_svg]:size-3")}>{I.refresh}</span>
                    {t("refresh")}
                  </Button>
                </div>
              </div>

              <div className="rounded-lg border bg-muted/30">
                <ScrollArea className="h-[420px]">
                  <div className="p-4">
                    {logsLoading ? (
                      <div className="flex items-center gap-2 text-muted-foreground">
                        <div className="size-4 rounded-full border-2 border-border border-t-primary animate-spin" />
                        {t("loading")}
                      </div>
                    ) : (
                      <pre className="whitespace-pre-wrap break-words text-xs font-mono text-foreground">
                        {podLogs || t("noLogs")}
                      </pre>
                    )}
                  </div>
                </ScrollArea>
              </div>
            </div>
          )}

          <DialogFooter>
            <Button type="button" variant="outline" onClick={() => setLogPod(null)}>
              {t("close")}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  )
}
