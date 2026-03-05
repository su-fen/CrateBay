import { useMemo, useState } from "react"
import { I } from "../icons"
import { ErrorBanner } from "../components/ErrorDisplay"
import { EmptyState } from "../components/EmptyState"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
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
import { Button } from "@/components/ui/button"
import { Card, CardContent } from "@/components/ui/card"
import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import { Input } from "@/components/ui/input"
import { ScrollArea } from "@/components/ui/scroll-area"
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select"
import { cn } from "@/lib/utils"
import { iconStroke, cardActionGhost } from "@/lib/styles"
import type { VolumeInfo } from "../types"

interface VolumesProps {
  volumes: VolumeInfo[]
  loading: boolean
  error: string
  onFetch: () => void
  onCreate: (name: string, driver: string) => Promise<VolumeInfo>
  onInspect: (name: string) => Promise<VolumeInfo>
  onRemove: (name: string) => Promise<void>
  onToast: (msg: string) => void
  t: (key: string) => string
}


export function Volumes({
  volumes,
  loading,
  error,
  onFetch,
  onCreate,
  onInspect,
  onRemove,
  onToast,
  t,
}: VolumesProps) {
  const [showCreateModal, setShowCreateModal] = useState(false)
  const [createName, setCreateName] = useState("")
  const [createDriver, setCreateDriver] = useState("local")
  const [createLoading, setCreateLoading] = useState(false)
  const [createError, setCreateError] = useState("")

  const [inspectVolume, setInspectVolume] = useState<VolumeInfo | null>(null)
  const [inspectLoading, setInspectLoading] = useState(false)

  const [confirmDelete, setConfirmDelete] = useState("")
  const [search, setSearch] = useState("")

  const filtered = useMemo(() => {
    if (!search.trim()) return volumes
    const q = search.toLowerCase()
    return volumes.filter(
      (v) =>
        v.name.toLowerCase().includes(q) ||
        v.driver.toLowerCase().includes(q) ||
        v.mountpoint?.toLowerCase().includes(q)
    )
  }, [volumes, search])

  const openCreateModal = () => {
    setCreateName("")
    setCreateDriver("local")
    setCreateError("")
    setShowCreateModal(true)
  }

  const handleCreate = async () => {
    if (!createName.trim()) return
    setCreateLoading(true)
    setCreateError("")
    try {
      await onCreate(createName.trim(), createDriver)
      onToast(t("volumeCreated"))
      setShowCreateModal(false)
    } catch (e) {
      setCreateError(String(e))
    } finally {
      setCreateLoading(false)
    }
  }

  const handleInspect = async (name: string) => {
    setInspectLoading(true)
    try {
      const vol = await onInspect(name)
      setInspectVolume(vol)
    } catch (e) {
      onToast(String(e))
    } finally {
      setInspectLoading(false)
    }
  }

  const handleDelete = async (name: string) => {
    try {
      await onRemove(name)
      onToast(t("volumeDeleted"))
    } catch (e) {
      onToast(String(e))
    } finally {
      setConfirmDelete("")
    }
  }

  const formatDate = (dateStr: string) => {
    if (!dateStr) return "-"
    try {
      const d = new Date(dateStr)
      return d.toLocaleString()
    } catch {
      return dateStr
    }
  }

  const labelsCount = (v: VolumeInfo) => {
    if (!v.labels) return 0
    return Object.keys(v.labels).length
  }

  if (loading) {
    return (
      <div className="flex items-center justify-center gap-2 py-20 text-muted-foreground">
        <div className="size-4 rounded-full border-2 border-border border-t-primary animate-spin" />
        {t("loading")}
      </div>
    )
  }

  if (error) {
    return (
      <ErrorBanner
        title={t("connectionError")}
        message={error}
        actionLabel={t("refresh")}
        onAction={onFetch}
      />
    )
  }

  return (
    <div className="space-y-4">
      <div className="flex items-center gap-2">
        <div className="w-[320px] max-w-[50vw]">
          <Input
            placeholder={t("searchVolumes")}
            value={search}
            onChange={(e) => setSearch(e.target.value)}
          />
        </div>
        <div className="flex-1" />
        <Button type="button" variant="outline" onClick={onFetch}>
          <span className={cn("mr-1", iconStroke, "[&_svg]:size-4")}>
            {I.refresh}
          </span>
          {t("refresh")}
        </Button>
        <Button
          type="button"
          onClick={openCreateModal}
          data-testid="volumes-create"
        >
          <span className={cn("mr-1", iconStroke, "[&_svg]:size-4")}>
            {I.plus}
          </span>
          {t("createVolume")}
        </Button>
      </div>

      {volumes.length === 0 ? (
        <EmptyState
          icon={I.hardDrive}
          title={t("noVolumes")}
          description={t("createFirstVolume")}
          code="docker volume create my-volume"
        />
      ) : filtered.length === 0 ? (
        <EmptyState
          icon={I.hardDrive}
          title={t("noResults")}
          description={search}
        />
      ) : (
        <div className="space-y-3">
          {filtered.map((v) => {
            const lc = labelsCount(v)
            return (
              <Card key={v.name} className="py-0">
                <CardContent className="py-4">
                  <div className="flex items-start gap-4">
                    <div
                      className={cn(
                        "size-10 shrink-0 rounded-lg bg-yellow-500/10 text-yellow-500 dark:text-yellow-400 flex items-center justify-center",
                        iconStroke,
                        "[&_svg]:size-[18px]"
                      )}
                    >
                      {I.hardDrive}
                    </div>

                    <div className="min-w-0 flex-1">
                      <div className="text-sm font-semibold text-foreground truncate">
                        {v.name}
                      </div>
                      <div className="mt-1 flex flex-wrap items-center gap-x-2 gap-y-1 text-xs text-muted-foreground">
                        <span>{v.driver}</span>
                        <span className="text-muted-foreground/60">·</span>
                        <span>{v.scope || "local"}</span>
                        {lc > 0 && (
                          <>
                            <span className="text-muted-foreground/60">·</span>
                            <span>
                              {lc} {lc === 1 ? "label" : "labels"}
                            </span>
                          </>
                        )}
                        {v.created_at && (
                          <>
                            <span className="text-muted-foreground/60">·</span>
                            <span>{formatDate(v.created_at)}</span>
                          </>
                        )}
                      </div>
                    </div>

                    <div className="shrink-0 flex flex-wrap items-center justify-end gap-1">
                      <Button
                        type="button"
                        variant="ghost"
                        size="sm"
                        className={cardActionGhost}
                        onClick={() => handleInspect(v.name)}
                        title={t("inspectVolume")}
                        disabled={inspectLoading}
                      >
                        <span className={cn(iconStroke, "[&_svg]:size-4")}>
                          {I.fileText}
                        </span>
                        {t("inspect")}
                      </Button>
                      <Button
                        type="button"
                        variant="ghost"
                        size="sm"
                        className={cardActionGhost}
                        onClick={() => {
                          navigator.clipboard.writeText(v.name)
                          onToast(t("copied"))
                        }}
                        title={t("copyName")}
                      >
                        <span className={cn(iconStroke, "[&_svg]:size-4")}>
                          {I.copy}
                        </span>
                        {t("copy")}
                      </Button>
                      <Button
                        type="button"
                        variant="ghost"
                        size="sm"
                        className={cn(
                          cardActionGhost,
                          "hover:border-destructive/45 hover:bg-destructive/10 hover:text-destructive"
                        )}
                        onClick={() => setConfirmDelete(v.name)}
                        title={t("deleteVolume")}
                      >
                        <span className={cn(iconStroke, "[&_svg]:size-4")}>
                          {I.trash}
                        </span>
                        {t("delete")}
                      </Button>
                    </div>
                  </div>
                </CardContent>
              </Card>
            )
          })}
        </div>
      )}

      {/* Create Volume Dialog */}
      <Dialog open={showCreateModal} onOpenChange={setShowCreateModal}>
        <DialogContent className="sm:max-w-md">
          <DialogHeader>
            <DialogTitle>{t("createVolume")}</DialogTitle>
          </DialogHeader>

          <div className="grid gap-4">
            <div className="grid gap-2">
              <label className="text-sm font-medium text-foreground">
                {t("volumeName")}
              </label>
              <Input
                value={createName}
                onChange={(e) => setCreateName(e.target.value)}
                placeholder="my-volume"
                autoFocus
                onKeyDown={(e) => {
                  if (e.key === "Enter") handleCreate()
                }}
              />
            </div>

            <div className="grid gap-2">
              <label className="text-sm font-medium text-foreground">
                {t("driver")}
              </label>
              <Select value={createDriver} onValueChange={setCreateDriver}>
                <SelectTrigger className="w-full justify-between">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent align="end">
                  <SelectItem value="local">local</SelectItem>
                </SelectContent>
              </Select>
            </div>

            {createError && (
              <Alert variant="destructive">
                <div className="text-destructive [&_svg]:size-4 [&_svg]:fill-none [&_svg]:stroke-current [&_svg]:[stroke-width:2] [&_svg]:[stroke-linecap:round] [&_svg]:[stroke-linejoin:round]">
                  {I.alertCircle}
                </div>
                <AlertTitle>{t("createVolume")}</AlertTitle>
                <AlertDescription>
                  <p className="whitespace-pre-wrap">{createError}</p>
                </AlertDescription>
              </Alert>
            )}
          </div>

          <DialogFooter>
            <Button
              type="button"
              variant="outline"
              onClick={() => setShowCreateModal(false)}
            >
              {t("close")}
            </Button>
            <Button
              type="button"
              onClick={handleCreate}
              disabled={createLoading || !createName.trim()}
            >
              {createLoading ? t("creating") : t("create")}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      {/* Inspect Volume Dialog */}
      <Dialog
        open={Boolean(inspectVolume)}
        onOpenChange={(open) => {
          if (!open) setInspectVolume(null)
        }}
      >
        <DialogContent
          className="sm:max-w-3xl"
          data-testid="volumes-inspect-dialog"
        >
          <DialogHeader>
            <DialogTitle>
              {inspectVolume
                ? `${t("volumeDetails")} — ${inspectVolume.name}`
                : t("volumeDetails")}
            </DialogTitle>
          </DialogHeader>

          {inspectVolume && (
            <ScrollArea className="max-h-[65vh] pr-4">
              <div className="space-y-6">
                <div className="flex items-center justify-end">
                  <Button
                    type="button"
                    variant="outline"
                    size="sm"
                    onClick={() => {
                      navigator.clipboard.writeText(
                        JSON.stringify(inspectVolume, null, 2)
                      )
                      onToast(t("copied"))
                    }}
                    title={t("copyJson")}
                  >
                    <span className={cn("mr-1", iconStroke, "[&_svg]:size-4")}>
                      {I.copy}
                    </span>
                    {t("copy")}
                  </Button>
                </div>

                <div className="grid grid-cols-1 gap-4 md:grid-cols-2">
                  <div className="space-y-1">
                    <div className="text-xs font-medium text-muted-foreground">
                      {t("volumeName")}
                    </div>
                    <div className="text-sm font-semibold text-foreground">
                      {inspectVolume.name}
                    </div>
                  </div>
                  <div className="space-y-1">
                    <div className="text-xs font-medium text-muted-foreground">
                      {t("driver")}
                    </div>
                    <div className="text-sm text-foreground">{inspectVolume.driver}</div>
                  </div>
                  <div className="space-y-1">
                    <div className="text-xs font-medium text-muted-foreground">
                      Scope
                    </div>
                    <div className="text-sm text-foreground">
                      {inspectVolume.scope || "local"}
                    </div>
                  </div>
                  <div className="space-y-1">
                    <div className="text-xs font-medium text-muted-foreground">
                      {t("created")}
                    </div>
                    <div className="text-sm text-foreground">
                      {formatDate(inspectVolume.created_at)}
                    </div>
                  </div>
                </div>

                <div className="space-y-2">
                  <div className="text-xs font-medium text-muted-foreground">
                    {t("mountpoint")}
                  </div>
                  <code className="block w-full overflow-auto rounded-md border bg-muted px-3 py-2 text-xs font-mono text-foreground">
                    {inspectVolume.mountpoint || "-"}
                  </code>
                </div>

                {inspectVolume.labels && Object.keys(inspectVolume.labels).length > 0 && (
                  <div className="space-y-2">
                    <div className="text-xs font-medium text-muted-foreground">
                      Labels
                    </div>
                    <div className="flex flex-col gap-1">
                      {Object.entries(inspectVolume.labels).map(([k, val]) => (
                        <code
                          key={k}
                          className="rounded-md border bg-muted px-2 py-1 text-xs font-mono text-foreground"
                        >
                          <span className="text-brand-cyan">{k}</span>
                          <span className="text-muted-foreground"> = </span>
                          <span>{val}</span>
                        </code>
                      ))}
                    </div>
                  </div>
                )}

                {inspectVolume.options && Object.keys(inspectVolume.options).length > 0 && (
                  <div className="space-y-2">
                    <div className="text-xs font-medium text-muted-foreground">
                      Options
                    </div>
                    <div className="flex flex-col gap-1">
                      {Object.entries(inspectVolume.options).map(([k, val]) => (
                        <code
                          key={k}
                          className="rounded-md border bg-muted px-2 py-1 text-xs font-mono text-foreground"
                        >
                          <span className="text-brand-cyan">{k}</span>
                          <span className="text-muted-foreground"> = </span>
                          <span>{val}</span>
                        </code>
                      ))}
                    </div>
                  </div>
                )}

                <div className="space-y-2">
                  <div className="text-xs font-medium tracking-widest text-muted-foreground">
                    RAW JSON
                  </div>
                  <pre className="overflow-auto rounded-md border bg-muted p-3 text-xs font-mono text-foreground">
                    {JSON.stringify(inspectVolume, null, 2)}
                  </pre>
                </div>
              </div>
            </ScrollArea>
          )}

          <DialogFooter>
            <Button
              type="button"
              variant="outline"
              onClick={() => setInspectVolume(null)}
            >
              {t("close")}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      {/* Confirm Delete Dialog */}
      <AlertDialog
        open={Boolean(confirmDelete)}
        onOpenChange={(open) => {
          if (!open) setConfirmDelete("")
        }}
      >
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>{t("deleteVolume")}</AlertDialogTitle>
            <AlertDialogDescription className="space-y-2">
              <span className="block">{t("confirmDeleteVolume")}</span>
              <span className="block font-mono text-sm text-foreground">
                {confirmDelete}
              </span>
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel onClick={() => setConfirmDelete("")}>
              {t("close")}
            </AlertDialogCancel>
            <AlertDialogAction
              variant="destructive"
              onClick={() => handleDelete(confirmDelete)}
            >
              <span className={cn("mr-1", iconStroke, "[&_svg]:size-4")}>
                {I.trash}
              </span>
              {t("delete")}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </div>
  )
}
