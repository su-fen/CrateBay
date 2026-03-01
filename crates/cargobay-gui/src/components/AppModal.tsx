import { invoke } from "@tauri-apps/api/core"
import { I } from "../icons"
import type { ModalKind } from "../types"

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
  if (!modalTitle && !modalBody) return null

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

  return (
    <div className="modal-backdrop" onClick={onClose}>
      <div className="modal" onClick={e => e.stopPropagation()}>
        <div className="modal-head">
          <div className="modal-title">{modalTitle}</div>
          <div className="modal-actions">
            <button
              className="icon-btn"
              onClick={() => onCopy(modalKind === "package" ? packageTag : modalCopyText)}
              title={t("copy")}
            >
              {I.copy}
            </button>
            <button className="icon-btn" onClick={onClose} title={t("close")}>×</button>
          </div>
        </div>
        {modalKind === "package" ? (
          <div className="modal-body">
            <div className="hint">{modalBody}</div>
            <div className="form" style={{ marginTop: 10 }}>
              <div className="row">
                <label>{t("newImageTag")}</label>
                <input
                  className="input"
                  value={packageTag}
                  onChange={e => setPackageTag(e.target.value)}
                  placeholder="myimage:latest"
                />
              </div>
            </div>
          </div>
        ) : (
          <pre className="modal-pre">{modalBody}</pre>
        )}
        {modalKind === "package" && (
          <div className="modal-footer">
            <button
              className="btn primary"
              disabled={packageLoading || !packageContainer || !packageTag.trim()}
              onClick={handlePackage}
            >
              {packageLoading ? t("working") : t("package")}
            </button>
          </div>
        )}
      </div>
    </div>
  )
}
