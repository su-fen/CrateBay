import { useState } from "react"
import { I } from "../icons"
import { ErrorBanner } from "../components/ErrorDisplay"
import { EmptyState } from "../components/EmptyState"
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
  volumes, loading, error,
  onFetch, onCreate, onInspect, onRemove, onToast, t,
}: VolumesProps) {
  const [showCreateModal, setShowCreateModal] = useState(false)
  const [createName, setCreateName] = useState("")
  const [createDriver, setCreateDriver] = useState("local")
  const [createLoading, setCreateLoading] = useState(false)
  const [createError, setCreateError] = useState("")

  const [inspectVolume, setInspectVolume] = useState<VolumeInfo | null>(null)
  const [inspectLoading, setInspectLoading] = useState(false)

  const [confirmDelete, setConfirmDelete] = useState("")

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

  if (loading) {
    return <div className="loading"><div className="spinner" />{t("loading")}</div>
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
    <div className="page">
      <div className="toolbar">
        <div style={{ flex: 1 }} />
        <button className="btn" onClick={onFetch}>
          <span className="icon">{I.refresh}</span>{t("refresh")}
        </button>
        <button className="btn primary" onClick={openCreateModal}>
          <span className="icon">{I.plus}</span>{t("createVolume")}
        </button>
      </div>

      {volumes.length === 0 ? (
        <EmptyState
          icon={I.hardDrive}
          title={t("noVolumes")}
          description={t("createFirstVolume")}
          code="docker volume create my-volume"
        />
      ) : (
        volumes.map(v => (
          <div className="container-card" key={v.name}>
            <div className="card-icon" style={{ background: "var(--surface2)" }}>
              {I.hardDrive}
            </div>
            <div className="card-body">
              <div className="card-name">{v.name}</div>
              <div className="card-meta">
                {t("driver")}: {v.driver} · {t("mountpoint")}: {v.mountpoint || "-"}
                {v.created_at ? ` · ${formatDate(v.created_at)}` : ""}
              </div>
            </div>
            <div className="card-actions">
              <button
                className="action-btn"
                onClick={() => handleInspect(v.name)}
                title={t("inspectVolume")}
                disabled={inspectLoading}
              >
                {I.fileText}
              </button>
              <button
                className="action-btn danger"
                onClick={() => setConfirmDelete(v.name)}
                title={t("deleteVolume")}
              >
                {I.trash}
              </button>
            </div>
          </div>
        ))
      )}

      {/* Create Volume Modal */}
      {showCreateModal && (
        <div className="modal-backdrop" onClick={() => setShowCreateModal(false)}>
          <div className="modal" onClick={e => e.stopPropagation()} style={{ maxWidth: 480 }}>
            <div className="modal-head">
              <div className="modal-title">{t("createVolume")}</div>
              <div className="modal-actions">
                <button className="icon-btn" onClick={() => setShowCreateModal(false)} title={t("close")}>x</button>
              </div>
            </div>
            <div className="modal-body">
              <div className="form">
                <div className="row">
                  <label>{t("volumeName")}</label>
                  <input
                    className="input"
                    value={createName}
                    onChange={e => setCreateName(e.target.value)}
                    placeholder="my-volume"
                    autoFocus
                    onKeyDown={e => { if (e.key === "Enter") handleCreate() }}
                  />
                </div>
                <div className="row">
                  <label>{t("driver")}</label>
                  <select className="select" value={createDriver} onChange={e => setCreateDriver(e.target.value)}>
                    <option value="local">local</option>
                  </select>
                </div>
              </div>
              {createError && <div className="hint" style={{ color: "var(--red)", marginTop: 8 }}>{createError}</div>}
            </div>
            <div className="modal-footer">
              <button className="btn" onClick={() => setShowCreateModal(false)}>{t("close")}</button>
              <button className="btn primary" disabled={createLoading || !createName.trim()} onClick={handleCreate} style={{ marginLeft: 8 }}>
                {createLoading ? t("creating") : t("create")}
              </button>
            </div>
          </div>
        </div>
      )}

      {/* Inspect Volume Modal */}
      {inspectVolume && (
        <div className="modal-backdrop" onClick={() => setInspectVolume(null)}>
          <div className="modal" onClick={e => e.stopPropagation()} style={{ maxWidth: 640 }}>
            <div className="modal-head">
              <div className="modal-title">{t("volumeDetails")} — {inspectVolume.name}</div>
              <div className="modal-actions">
                <button className="icon-btn" onClick={() => setInspectVolume(null)} title={t("close")}>x</button>
              </div>
            </div>
            <div className="modal-body" style={{ padding: 0 }}>
              <pre className="modal-pre">{JSON.stringify(inspectVolume, null, 2)}</pre>
            </div>
            <div className="modal-footer">
              <button className="btn" onClick={() => setInspectVolume(null)}>{t("close")}</button>
            </div>
          </div>
        </div>
      )}

      {/* Confirm Delete Modal */}
      {confirmDelete && (
        <div className="modal-backdrop" onClick={() => setConfirmDelete("")}>
          <div className="modal" onClick={e => e.stopPropagation()} style={{ maxWidth: 420 }}>
            <div className="modal-head">
              <div className="modal-title">{t("deleteVolume")}</div>
              <div className="modal-actions">
                <button className="icon-btn" onClick={() => setConfirmDelete("")} title={t("close")}>x</button>
              </div>
            </div>
            <div className="modal-body">
              <p style={{ margin: 0 }}>{t("confirmDeleteVolume")}</p>
              <p style={{ margin: "8px 0 0", fontWeight: 600, color: "var(--text)" }}>{confirmDelete}</p>
            </div>
            <div className="modal-footer">
              <button className="btn" onClick={() => setConfirmDelete("")}>{t("close")}</button>
              <button className="btn primary" onClick={() => handleDelete(confirmDelete)} style={{ marginLeft: 8, background: "var(--red)", borderColor: "var(--red)" }}>
                {t("delete")}
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  )
}
