import { useState, useEffect, useCallback } from "react"
import { invoke } from "@tauri-apps/api/core"
import { open } from "@tauri-apps/plugin-dialog"
import { I } from "../icons"
import { EmptyState } from "../components/EmptyState"
import { ErrorInline } from "../components/ErrorDisplay"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Card, CardContent } from "@/components/ui/card"
import { Checkbox } from "@/components/ui/checkbox"
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
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import { Input } from "@/components/ui/input"
import { ScrollArea } from "@/components/ui/scroll-area"
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select"
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs"
import { cn } from "@/lib/utils"
import { iconStroke, cardActionSecondary, cardActionOutline, cardActionGhost } from "@/lib/styles"
import type { ImageSearchResult, RunContainerResult, LocalImageInfo, ImageInspectInfo } from "../types"

interface ImagesProps {
  imgQuery: string
  setImgQuery: (v: string) => void
  imgSource: string
  setImgSource: (v: string) => void
  imgResults: ImageSearchResult[]
  imgSearching: boolean
  imgError: string
  setImgError: (v: string) => void
  imgTags: string[]
  imgTagsRef: string
  imgTagsLoading: boolean
  runImage: string
  setRunImage: (v: string) => void
  runName: string
  setRunName: (v: string) => void
  runCpus: number | ""
  setRunCpus: (v: number | "") => void
  runMem: number | ""
  setRunMem: (v: number | "") => void
  runPull: boolean
  setRunPull: (v: boolean) => void
  runLoading: boolean
  runResult: RunContainerResult | null
  setRunResult: (v: RunContainerResult | null) => void
  loadPath: string
  setLoadPath: (v: string) => void
  loadLoading: boolean
  pushRef: string
  setPushRef: (v: string) => void
  pushLoading: boolean
  onSearch: () => void
  onTags: (ref: string) => void
  onRun: () => void
  onLoad: () => void
  onPush: () => void
  onCopy: (text: string) => void
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

export function Images({
  imgQuery, setImgQuery, imgSource, setImgSource,
  imgResults, imgSearching,
  imgError, setImgError, imgTags, imgTagsRef, imgTagsLoading,
  runImage, setRunImage, runName, setRunName,
  runCpus, setRunCpus, runMem, setRunMem,
  runPull, setRunPull, runLoading, runResult, setRunResult,
  loadPath, setLoadPath, loadLoading,
  pushRef, setPushRef, pushLoading,
  onSearch, onTags, onRun, onLoad, onPush, onCopy, t,
}: ImagesProps) {
  const [showRunModal, setShowRunModal] = useState(false)
  const [showImportModal, setShowImportModal] = useState(false)
  const [importTab, setImportTab] = useState<"import" | "push">("import")
  const [activeTab, setActiveTab] = useState<"local" | "search">("local")
  const canTags = (ref: string) => ref.includes(".") || ref.includes(":") || ref.startsWith("localhost/")

  // Local images state
  const [localImages, setLocalImages] = useState<LocalImageInfo[]>([])
  const [localLoading, setLocalLoading] = useState(true)
  const [localFilter, setLocalFilter] = useState("")
  const [inspectInfo, setInspectInfo] = useState<ImageInspectInfo | null>(null)
  const [inspectLoading, setInspectLoading] = useState(false)
  const [confirmRemove, setConfirmRemove] = useState("")
  const [refreshing, setRefreshing] = useState(false)
  const [showTagModal, setShowTagModal] = useState(false)
  const [tagSource, setTagSource] = useState("")
  const [tagRepo, setTagRepo] = useState("")
  const [tagTag, setTagTag] = useState("")
  const [tagLoading, setTagLoading] = useState(false)

  const fetchLocalImages = useCallback(async () => {
    try {
      const result = await invoke<LocalImageInfo[]>("image_list")
      setLocalImages(result)
    } catch {
      // silently ignore - Docker may not be running
    } finally {
      setLocalLoading(false)
    }
  }, [])

  useEffect(() => {
    fetchLocalImages()
    const iv = setInterval(fetchLocalImages, 10000)
    return () => clearInterval(iv)
  }, [fetchLocalImages])

  const filteredImages = localImages.filter(img => {
    if (!localFilter.trim()) return true
    const q = localFilter.toLowerCase()
    return img.id.toLowerCase().includes(q) ||
      img.repo_tags.some(tag => tag.toLowerCase().includes(q))
  })

  const handleRemoveImage = async (ref: string) => {
    try {
      await invoke<void>("image_remove", { id: ref })
      await fetchLocalImages()
      setImgError("")
    } catch (e) {
      setImgError(String(e))
    } finally {
      setConfirmRemove("")
    }
  }

  const handleInspect = async (ref: string) => {
    setInspectLoading(true)
    try {
      const info = await invoke<ImageInspectInfo>("image_inspect", { id: ref })
      setInspectInfo(info)
    } catch (e) {
      setImgError(String(e))
    } finally {
      setInspectLoading(false)
    }
  }

  const openTagModal = (source: string) => {
    setTagSource(source)
    setTagRepo("")
    setTagTag("latest")
    setShowTagModal(true)
  }

  const handleTag = async () => {
    if (!tagRepo.trim()) return
    setTagLoading(true)
    try {
      await invoke<void>("image_tag", { source: tagSource, repo: tagRepo.trim(), tag: tagTag.trim() || "latest" })
      await fetchLocalImages()
      setShowTagModal(false)
    } catch (e) {
      setImgError(String(e))
    } finally {
      setTagLoading(false)
    }
  }

  const formatCreated = (timestamp: number) => {
    if (!timestamp) return "-"
    try {
      const d = new Date(timestamp * 1000)
      return d.toLocaleString()
    } catch {
      return "-"
    }
  }

  const formatPulls = (n?: number) => {
    if (n == null) return "-"
    if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`
    if (n >= 1_000) return `${(n / 1_000).toFixed(1)}K`
    return String(n)
  }

  const openRunWithImage = (ref: string) => {
    setRunImage(ref)
    setRunName("")
    setRunResult(null)
    setShowRunModal(true)
  }

  const browseFile = async () => {
    const selected = await open({
      multiple: false,
      filters: [{ name: "Docker Image Archive", extensions: ["tar"] }],
    })
    if (selected) setLoadPath(selected as string)
  }

  return (
    <div className="space-y-4">
      <Tabs value={activeTab} onValueChange={(v) => setActiveTab(v as "local" | "search")}>
        <TabsList variant="line">
          <TabsTrigger value="local" data-testid="images-tab-local">
            <span className={cn(iconStroke, "[&_svg]:size-4")}>{I.layers}</span>
            {t("localImages")}
            <Badge
              variant="secondary"
              className="ml-1 rounded-md border border-brand-cyan/15 bg-brand-cyan/10 px-2 py-0.5 text-[11px] text-brand-cyan"
            >
              {localImages.length}
            </Badge>
          </TabsTrigger>
          <TabsTrigger value="search" data-testid="images-tab-search">
            <span className={cn(iconStroke, "[&_svg]:size-4")}>{I.globe}</span>
            {t("searchImages")}
          </TabsTrigger>
        </TabsList>

      {imgError && <ErrorInline message={imgError} onDismiss={() => setImgError("")} />}

      <TabsContent value="local" className="space-y-4">
          <div className="flex items-center gap-2">
            <div className="w-[420px] max-w-[60vw]">
              <Input
                placeholder={t("filterLocalImages")}
                value={localFilter}
                onChange={(e) => setLocalFilter(e.target.value)}
              />
            </div>
            <div className="flex-1" />
            <Button type="button" variant="outline" onClick={() => setShowImportModal(true)}>
              <span className={cn("mr-1", iconStroke, "[&_svg]:size-4")}>{I.plus}</span>
              {t("importImage")}
            </Button>
            <Button type="button" variant="outline" onClick={async () => {
              setRefreshing(true)
              await Promise.all([fetchLocalImages(), new Promise(r => setTimeout(r, 600))])
              setRefreshing(false)
            }} disabled={refreshing || localLoading}>
              <span className={cn("mr-1", iconStroke, "[&_svg]:size-4", refreshing && "animate-spin")}>{I.refresh}</span>
              {t("refresh")}
            </Button>
          </div>

          {localLoading ? (
            <div className="flex items-center justify-center gap-2 py-20 text-muted-foreground">
              <Spinner />
              {t("loading")}
            </div>
          ) : filteredImages.length === 0 ? (
            <EmptyState icon={I.layers} title={t("noLocalImages")} />
          ) : (
            <div className="space-y-3">
              {filteredImages.map((img) => {
                const primaryRef = img.repo_tags[0] || img.id
                return (
                  <Card key={img.id} className="py-0">
                    <CardContent className="py-4">
                      <div className="flex items-start justify-between gap-4">
                        <div className="flex min-w-0 items-start gap-3">
                          <div
                            className={cn(
                              "size-10 shrink-0 rounded-lg bg-primary/10 text-primary flex items-center justify-center",
                              iconStroke,
                              "[&_svg]:size-[18px]"
                            )}
                          >
                            {I.layers}
                          </div>
                          <div className="min-w-0">
                            <div className="font-semibold text-foreground truncate">
                              {img.repo_tags.length > 0 ? img.repo_tags.join(", ") : "<none>:<none>"}
                            </div>
                            <div className="mt-1 flex flex-wrap items-center gap-2 text-xs text-muted-foreground">
                              <span className="font-mono">
                                {t("imageId")}: {img.id.slice(0, 16)}
                              </span>
                              <span className="text-muted-foreground/50">•</span>
                              <span>{img.size_human}</span>
                              <span className="text-muted-foreground/50">•</span>
                              <span>{formatCreated(img.created)}</span>
                            </div>
                          </div>
                        </div>

                        <div className="flex items-center gap-1">
                          <Button
                            type="button"
                            size="xs"
                            variant="secondary"
                            className={cardActionSecondary}
                            title={t("run")}
                            onClick={() => openRunWithImage(primaryRef)}
                          >
                            <span className={cn(iconStroke, "[&_svg]:size-3")}>{I.play}</span>
                            {t("run")}
                          </Button>
                          <Button
                            type="button"
                            size="xs"
                            variant="secondary"
                            className={cardActionSecondary}
                            title={t("inspectImage")}
                            onClick={() => handleInspect(primaryRef)}
                            disabled={inspectLoading}
                          >
                            <span className={cn(iconStroke, "[&_svg]:size-3")}>{I.fileText}</span>
                            {t("inspectImage")}
                          </Button>
                          <Button
                            type="button"
                            size="xs"
                            variant="secondary"
                            className={cardActionSecondary}
                            title={t("tagImage")}
                            onClick={() => openTagModal(primaryRef)}
                          >
                            <span className={cn(iconStroke, "[&_svg]:size-3")}>{I.plus}</span>
                            {t("tagImage")}
                          </Button>
                          <Button
                            type="button"
                            size="icon-xs"
                            variant="ghost"
                            title={t("copyId")}
                            onClick={() => onCopy(img.id)}
                            className={cn(cardActionGhost, iconStroke, "[&_svg]:size-3")}
                          >
                            {I.copy}
                          </Button>
                          <Button
                            type="button"
                            size="icon-xs"
                            variant="ghost"
                            title={t("removeImage")}
                            onClick={() => setConfirmRemove(primaryRef)}
                            className={cn(
                              cardActionGhost,
                              "hover:border-destructive/45 hover:bg-destructive/10 hover:text-destructive",
                              iconStroke,
                              "[&_svg]:size-3"
                            )}
                          >
                            {I.trash}
                          </Button>
                        </div>
                      </div>
                    </CardContent>
                  </Card>
                )
              })}
            </div>
          )}
      </TabsContent>

      <TabsContent value="search" className="space-y-4">
          <div className="flex flex-wrap items-center gap-2">
            <div className="min-w-[280px] flex-1">
              <Input
                placeholder={t("searchImages")}
                value={imgQuery}
                onChange={(e) => setImgQuery(e.target.value)}
                onKeyDown={(e) => e.key === "Enter" && onSearch()}
              />
            </div>
            <div className="w-[220px]">
              <Select value={imgSource} onValueChange={setImgSource}>
                <SelectTrigger>
                  <SelectValue placeholder={t("sourceAll")} />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="all">{t("sourceAll")}</SelectItem>
                  <SelectItem value="dockerhub">{t("sourceDockerHub")}</SelectItem>
                  <SelectItem value="quay">{t("sourceQuay")}</SelectItem>
                </SelectContent>
              </Select>
            </div>
            <Button type="button" disabled={imgSearching || !imgQuery.trim()} onClick={onSearch}>
              {imgSearching ? t("searching") : t("search")}
            </Button>
          </div>

          {imgTags.length > 0 && (
            <Card className="py-0">
              <CardContent className="py-4">
                <div className="flex flex-wrap items-center gap-2">
                  <div className="text-xs text-muted-foreground">
                    {t("tags")} ({imgTagsRef}):
                  </div>
                  <div className="flex flex-wrap gap-2">
                    {imgTags.map((tag) => (
                      <Badge
                        key={tag}
                        variant="secondary"
                        className="cursor-pointer border border-border/60 bg-popover/40 hover:bg-accent/30"
                        onClick={() => openRunWithImage(`${imgTagsRef}:${tag}`)}
                      >
                        {tag}
                      </Badge>
                    ))}
                  </div>
                </div>
              </CardContent>
            </Card>
          )}
          {imgTagsLoading && (
            <div className="flex items-center gap-2 text-sm text-muted-foreground">
              <Spinner className="size-3" />
              {t("loading")}
            </div>
          )}

          {imgResults.length === 0 ? (
            <EmptyState
              icon={I.globe}
              title={t("searchHint")}
              description={"Docker Hub · Quay.io · GitHub Container Registry"}
            />
          ) : (
            <div className="grid grid-cols-1 gap-3 lg:grid-cols-2 xl:grid-cols-3">
              {imgResults.map((r, idx) => (
                <Card key={`${r.source}-${r.reference}-${idx}`} className="py-0">
                  <CardContent className="py-4">
                    <div className="flex items-start justify-between gap-3">
                      <div className="flex flex-col gap-1 min-w-0">
                        <div className="flex items-center gap-2">
                          <Badge
                            variant="secondary"
                            className="rounded-md border border-border/60 bg-popover/40 text-xs"
                          >
                            {r.source}
                          </Badge>
                          {r.official && (
                            <Badge className="rounded-md bg-primary/10 text-primary border border-primary/15 text-[11px]">
                              {t("official")}
                            </Badge>
                          )}
                        </div>
                        <div className="font-semibold text-foreground truncate">{r.reference}</div>
                        {r.description && (
                          <div className="text-xs text-muted-foreground line-clamp-2">
                            {r.description}
                          </div>
                        )}
                      </div>
                    </div>

                    <div className="mt-3 flex items-center justify-between gap-2">
                      <div className="flex items-center gap-3 text-xs text-muted-foreground">
                        <span className="inline-flex items-center gap-1">
                          <svg viewBox="0 0 24 24" className="size-3.5 fill-none stroke-current stroke-[1.5]">
                            <polygon points="12 2 15.09 8.26 22 9.27 17 14.14 18.18 21.02 12 17.77 5.82 21.02 7 14.14 2 9.27 8.91 8.26 12 2" />
                          </svg>
                          {r.stars ?? "-"}
                        </span>
                        <span className="inline-flex items-center gap-1">
                          <svg viewBox="0 0 24 24" className="size-3.5 fill-none stroke-current stroke-[1.5]">
                            <path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4" />
                            <polyline points="7 10 12 15 17 10" />
                            <line x1="12" y1="15" x2="12" y2="3" />
                          </svg>
                          {formatPulls(r.pulls)}
                        </span>
                      </div>
                      <div className="flex items-center gap-2">
                        <Button
                          type="button"
                          size="xs"
                          variant="secondary"
                          className={cardActionSecondary}
                          onClick={() => openRunWithImage(r.reference)}
                        >
                          <span className={cn(iconStroke, "[&_svg]:size-3")}>{I.play}</span>
                          {t("run")}
                        </Button>
                        <Button
                          type="button"
                          size="xs"
                          variant="outline"
                          className={cardActionOutline}
                          disabled={!canTags(r.reference)}
                          onClick={() => onTags(r.reference)}
                        >
                          {t("tags")}
                        </Button>
                      </div>
                    </div>
                  </CardContent>
                </Card>
              ))}
            </div>
          )}
      </TabsContent>

      {/* Run Container */}
      <Dialog open={showRunModal} onOpenChange={setShowRunModal}>
        <DialogContent className="sm:max-w-[520px]" data-testid="images-dialog-run">
          <DialogHeader>
            <DialogTitle>{t("runContainer")}</DialogTitle>
            <DialogDescription className="sr-only">{t("runContainer")}</DialogDescription>
          </DialogHeader>

          <div className="grid gap-4">
            <div className="grid gap-2">
              <div className="text-sm font-medium">{t("image")}</div>
              <Input value={runImage} onChange={(e) => setRunImage(e.target.value)} placeholder="nginx:latest" />
            </div>
            <div className="grid gap-2">
              <div className="text-sm font-medium">{t("nameOptional")}</div>
              <Input value={runName} onChange={(e) => setRunName(e.target.value)} placeholder="web" />
            </div>
            <div className="grid grid-cols-2 gap-3">
              <div className="grid gap-2">
                <div className="text-sm font-medium">{t("cpus")}</div>
                <Input
                  type="number"
                  min={1}
                  value={runCpus}
                  onChange={(e) => setRunCpus(e.target.value === "" ? "" : Number(e.target.value))}
                />
              </div>
              <div className="grid gap-2">
                <div className="text-sm font-medium">{t("memoryMb")}</div>
                <Input
                  type="number"
                  min={64}
                  value={runMem}
                  onChange={(e) => setRunMem(e.target.value === "" ? "" : Number(e.target.value))}
                />
              </div>
            </div>
            <label className="flex items-center gap-2 text-sm text-muted-foreground">
              <Checkbox checked={runPull} onCheckedChange={(v) => setRunPull(Boolean(v))} />
              {t("pullBeforeRun")}
            </label>

            {runResult && (
              <Card className="py-0">
                <CardContent className="py-4">
                  <div className="text-sm font-medium">{t("loginCommand")}</div>
                  <div className="mt-2 flex items-start gap-2">
                    <code className="flex-1 rounded-md border bg-muted px-2 py-1 text-xs font-mono text-foreground break-all">
                      {runResult.login_cmd}
                    </code>
                    <Button
                      type="button"
                      size="icon-xs"
                      variant="outline"
                      title={t("copy")}
                      onClick={() => onCopy(runResult.login_cmd)}
                      className={cn(iconStroke, "[&_svg]:size-3")}
                    >
                      {I.copy}
                    </Button>
                  </div>
                </CardContent>
              </Card>
            )}
          </div>

          <DialogFooter>
            <Button type="button" variant="outline" onClick={() => setShowRunModal(false)}>
              {t("close")}
            </Button>
            <Button type="button" disabled={runLoading || !runImage.trim()} onClick={onRun}>
              {runLoading ? t("creating") : t("create")}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      {/* Import / Push */}
      <Dialog open={showImportModal} onOpenChange={setShowImportModal}>
        <DialogContent className="sm:max-w-[560px]" data-testid="images-dialog-import">
          <DialogHeader>
            <DialogTitle>
              {t("importImage")} / {t("pushImage")}
            </DialogTitle>
            <DialogDescription className="sr-only">
              {t("importImage")} / {t("pushImage")}
            </DialogDescription>
          </DialogHeader>

          <Tabs value={importTab} onValueChange={(v) => setImportTab(v as "import" | "push")} className="w-full">
            <TabsList variant="line" className="w-full">
              <TabsTrigger value="import" className="flex-1">
                <span className={cn(iconStroke, "[&_svg]:size-4")}>{I.hardDrive}</span>
                {t("importImage")}
              </TabsTrigger>
              <TabsTrigger value="push" className="flex-1">
                <span className={cn(iconStroke, "[&_svg]:size-4")}>{I.globe}</span>
                {t("pushImage")}
              </TabsTrigger>
            </TabsList>

            <TabsContent value="import" className="mt-4 space-y-4">
              <div className="grid gap-2">
                <div className="text-sm font-medium">{t("imageArchivePath")}</div>
                <div className="flex gap-2">
                  <Input
                    value={loadPath}
                    onChange={(e) => setLoadPath(e.target.value)}
                    placeholder="/path/to/image.tar"
                  />
                  <Button type="button" variant="outline" onClick={browseFile}>
                    <span className={cn(iconStroke, "[&_svg]:size-3.5")}>{I.fileText}</span>
                    {t("browse")}
                  </Button>
                </div>
              </div>
              <div className="rounded-lg border border-border/60 bg-muted/30 px-3 py-2.5 text-xs text-muted-foreground leading-relaxed">
                {t("importHint")}
              </div>
            </TabsContent>

            <TabsContent value="push" className="mt-4 space-y-4">
              <div className="grid gap-2">
                <div className="text-sm font-medium">{t("imageRef")}</div>
                <Input
                  value={pushRef}
                  onChange={(e) => setPushRef(e.target.value)}
                  placeholder="ghcr.io/org/image:tag"
                />
              </div>
              <div className="rounded-lg border border-border/60 bg-muted/30 px-3 py-2.5 text-xs text-muted-foreground leading-relaxed">
                {t("pushHint")}
              </div>
            </TabsContent>
          </Tabs>

          <DialogFooter>
            <Button type="button" variant="outline" onClick={() => setShowImportModal(false)}>
              {t("close")}
            </Button>
            {importTab === "import" ? (
              <Button type="button" disabled={loadLoading || !loadPath.trim()} onClick={onLoad}>
                {loadLoading && <Spinner className="mr-1.5 size-3" />}
                {loadLoading ? t("working") : t("load")}
              </Button>
            ) : (
              <Button type="button" disabled={pushLoading || !pushRef.trim()} onClick={onPush}>
                {pushLoading && <Spinner className="mr-1.5 size-3" />}
                {pushLoading ? t("working") : t("push")}
              </Button>
            )}
          </DialogFooter>
        </DialogContent>
      </Dialog>

      {/* Inspect Image */}
      <Dialog open={!!inspectInfo} onOpenChange={(open) => !open && setInspectInfo(null)}>
        <DialogContent className="sm:max-w-[720px]" data-testid="images-dialog-inspect">
          <DialogHeader>
            <DialogTitle>
              {t("inspectImage")} — {inspectInfo?.id || ""}
            </DialogTitle>
            <DialogDescription className="sr-only">{t("inspectImage")}</DialogDescription>
          </DialogHeader>

          <ScrollArea className="max-h-[60vh] pr-2">
            <div className="grid gap-2 text-sm">
              <div className="grid grid-cols-[160px_1fr] gap-x-4 gap-y-2">
                <div className="text-muted-foreground">{t("imageId")}</div>
                <div className="font-mono break-all">{inspectInfo?.id}</div>

                <div className="text-muted-foreground">{t("repoTags")}</div>
                <div className="break-words">
                  {inspectInfo?.repo_tags?.length ? inspectInfo.repo_tags.join(", ") : "-"}
                </div>

                <div className="text-muted-foreground">{t("imageSize")}</div>
                <div>{inspectInfo ? (inspectInfo.size_bytes / (1024 * 1024)).toFixed(1) : "-"} MB</div>

                <div className="text-muted-foreground">{t("imageCreated")}</div>
                <div>{inspectInfo?.created || "-"}</div>

                <div className="text-muted-foreground">{t("architecture")}</div>
                <div>{inspectInfo?.architecture || "-"}</div>

                <div className="text-muted-foreground">OS</div>
                <div>{inspectInfo?.os || "-"}</div>

                <div className="text-muted-foreground">Docker Version</div>
                <div>{inspectInfo?.docker_version || "-"}</div>

                <div className="text-muted-foreground">{t("layers")}</div>
                <div>{inspectInfo?.layers ?? "-"}</div>
              </div>
            </div>
          </ScrollArea>

          <DialogFooter>
            <Button type="button" variant="outline" onClick={() => setInspectInfo(null)}>
              {t("close")}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      {/* Tag Image */}
      <Dialog open={showTagModal} onOpenChange={setShowTagModal}>
        <DialogContent className="sm:max-w-[560px]" data-testid="images-dialog-tag">
          <DialogHeader>
            <DialogTitle>{t("tagImage")}</DialogTitle>
            <DialogDescription className="sr-only">{t("tagImage")}</DialogDescription>
          </DialogHeader>

          <div className="grid gap-4">
            <div className="grid gap-1">
              <div className="text-sm font-medium">{t("sourceRef")}</div>
              <div className="text-xs text-muted-foreground break-all">{tagSource}</div>
            </div>

            <div className="grid gap-2">
              <div className="text-sm font-medium">{t("targetTag")} (repository)</div>
              <Input
                value={tagRepo}
                onChange={(e) => setTagRepo(e.target.value)}
                placeholder="myrepo/myimage"
                onKeyDown={(e) => {
                  if (e.key === "Enter") handleTag()
                }}
              />
            </div>

            <div className="grid gap-2">
              <div className="text-sm font-medium">{t("targetTag")} (tag)</div>
              <Input value={tagTag} onChange={(e) => setTagTag(e.target.value)} placeholder="latest" />
            </div>
          </div>

          <DialogFooter>
            <Button type="button" variant="outline" onClick={() => setShowTagModal(false)}>
              {t("close")}
            </Button>
            <Button type="button" disabled={tagLoading || !tagRepo.trim()} onClick={handleTag}>
              {tagLoading ? t("working") : t("tagImage")}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      {/* Confirm Remove */}
      <AlertDialog open={!!confirmRemove} onOpenChange={(open) => !open && setConfirmRemove("")}>
        <AlertDialogContent size="sm" data-testid="images-dialog-remove">
          <AlertDialogHeader>
            <AlertDialogTitle>{t("removeImage")}</AlertDialogTitle>
            <AlertDialogDescription>{t("confirmRemoveImage")}</AlertDialogDescription>
          </AlertDialogHeader>
          <div className="rounded-md border bg-muted px-2 py-1 text-xs font-mono text-foreground break-all">
            {confirmRemove}
          </div>
          <AlertDialogFooter>
            <AlertDialogCancel onClick={() => setConfirmRemove("")}>{t("close")}</AlertDialogCancel>
            <AlertDialogAction variant="destructive" onClick={() => handleRemoveImage(confirmRemove)}>
              {t("remove")}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </Tabs>
  </div>
  )
}
