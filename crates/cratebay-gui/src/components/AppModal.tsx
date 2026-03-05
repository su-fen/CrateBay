import { invoke } from "@tauri-apps/api/core"
import { I } from "../icons"
import type { ModalKind } from "../types"
import { Button } from "@/components/ui/button"
import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import { Input } from "@/components/ui/input"
import { ScrollArea } from "@/components/ui/scroll-area"
import { cn } from "@/lib/utils"
import { iconStroke } from "@/lib/styles"

interface AppModalProps {
  modalKind: ModalKind
  modalTitle: string
  modalBody: string
  modalCopyText: string
  packageContainer: string
  packageTag: string
  setPackageTag: (v: string) => void
  packageLoading: boolean
  setPackageLoading: (v: boolean) => void
  onClose: () => void
  onCopy: (text: string) => void
  onOpenTextModal: (title: string, body: string, copyText?: string) => void
  onError: (msg: string) => void
  onToast: (msg: string) => void
  t: (key: string) => string
}

export function AppModal({
  modalKind, modalTitle, modalBody, modalCopyText,
  packageContainer, packageTag, setPackageTag,
  packageLoading, setPackageLoading,
  onClose, onCopy, onOpenTextModal, onError, onToast, t,
}: AppModalProps) {
  const open = Boolean(modalTitle || modalBody)
  if (!open) return null

  const handlePackage = async () => {
    if (!packageContainer || !packageTag.trim()) return
    setPackageLoading(true)
    try {
      const out = await invoke<string>("image_pack_container", {
        container: packageContainer, tag: packageTag.trim(),
      })
      onClose()
      onOpenTextModal(t("imagePacked"), out || t("done"), out || t("done"))
      onToast(t("done"))
    } catch (e) {
      onError(String(e))
    } finally {
      setPackageLoading(false)
    }
  }

  const copyValue = modalKind === "package" ? packageTag : modalCopyText

  return (
    <Dialog
      open={open}
      onOpenChange={(next) => {
        if (!next) onClose()
      }}
    >
      <DialogContent
        className={cn(modalKind === "package" ? "sm:max-w-2xl" : "sm:max-w-4xl")}
        data-testid={modalKind === "package" ? "app-modal-package" : "app-modal-text"}
      >
        <DialogHeader className="sm:text-left">
          <div className="flex items-start justify-between gap-3">
            <DialogTitle className="text-base">{modalTitle}</DialogTitle>
            <Button
              type="button"
              variant="outline"
              size="icon-xs"
              onClick={() => onCopy(copyValue)}
              title={t("copy")}
              disabled={!copyValue}
              className={cn(iconStroke, "[&_svg]:size-3")}
            >
              {I.copy}
            </Button>
          </div>
        </DialogHeader>

        {modalKind === "package" ? (
          <div className="space-y-4">
            <div className="text-sm text-muted-foreground whitespace-pre-wrap">{modalBody}</div>
            <div className="space-y-2">
              <div className="text-xs font-medium text-muted-foreground">{t("newImageTag")}</div>
              <Input
                value={packageTag}
                onChange={(e) => setPackageTag(e.target.value)}
                placeholder="myimage:latest"
              />
            </div>
          </div>
        ) : (
          <ScrollArea className="max-h-[560px] rounded-lg border bg-muted/30">
            <pre className="p-4 text-xs font-mono text-foreground whitespace-pre-wrap break-words">
              {modalBody}
            </pre>
          </ScrollArea>
        )}

        <DialogFooter>
          <Button type="button" variant="outline" onClick={onClose}>
            {t("close")}
          </Button>
          {modalKind === "package" && (
            <Button
              type="button"
              disabled={packageLoading || !packageContainer || !packageTag.trim()}
              onClick={handlePackage}
            >
              {packageLoading ? (
                <>
                  <div className="size-4 rounded-full border-2 border-primary-foreground/50 border-t-primary-foreground animate-spin" />
                  {t("working")}
                </>
              ) : (
                <>
                  <span className={cn(iconStroke, "[&_svg]:size-4")}>{I.box}</span>
                  {t("package")}
                </>
              )}
            </Button>
          )}
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}
