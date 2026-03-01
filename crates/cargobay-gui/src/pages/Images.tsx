import { useState } from "react"
import { open } from "@tauri-apps/plugin-dialog"
import { I } from "../icons"
import { ErrorInline } from "../components/ErrorDisplay"
import type { ImageSearchResult, RunContainerResult } from "../types"

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
  const canTags = (ref: string) => ref.includes(".") || ref.includes(":") || ref.startsWith("localhost/")

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
    <div className="page">
      {/* Search bar */}
      <div className="toolbar">
        <input
          className="input toolbar-search"
          placeholder={t("searchImages")}
          value={imgQuery}
          onChange={e => setImgQuery(e.target.value)}
          onKeyDown={e => e.key === "Enter" && onSearch()}
        />
        <select className="select" value={imgSource} onChange={e => setImgSource(e.target.value)}>
          <option value="all">{t("sourceAll")}</option>
          <option value="dockerhub">{t("sourceDockerHub")}</option>
          <option value="quay">{t("sourceQuay")}</option>
        </select>
        <button className="btn primary" disabled={imgSearching || !imgQuery.trim()} onClick={onSearch}>
          {imgSearching ? t("searching") : t("search")}
        </button>
        <div style={{ flex: 1 }} />
        <button className="btn" onClick={() => setShowImportModal(true)}>
          <span className="icon">{I.plus}</span>{t("importImage")}
        </button>
      </div>

      {imgError && <ErrorInline message={imgError} onDismiss={() => setImgError("")} />}

      {/* Tags bar */}
      {imgTags.length > 0 && (
        <div className="img-tags-bar">
          <span className="img-tags-label">{t("tags")} ({imgTagsRef}):</span>
          <div className="tags">
            {imgTags.map(tag => (
              <div className="tag" key={tag} onClick={() => openRunWithImage(`${imgTagsRef}:${tag}`)}>
                {tag}
              </div>
            ))}
          </div>
        </div>
      )}
      {imgTagsLoading && <div className="hint">{t("loading")}</div>}

      {/* Search results - card list */}
      {imgResults.length === 0 ? (
        <div className="empty-state">
          <div className="empty-icon">{I.layers}</div>
          <h3>{t("searchHint")}</h3>
          <p>Docker Hub · Quay.io · GitHub Container Registry</p>
        </div>
      ) : (
        <div className="img-grid">
          {imgResults.map((r, idx) => (
            <div className="img-card" key={`${r.source}-${r.reference}-${idx}`}>
              <div className="img-card-top">
                <div className="img-card-header">
                  <span className="badge">{r.source}</span>
                  {r.official && <span className="img-official">{t("official")}</span>}
                </div>
                <div className="img-card-name">{r.reference}</div>
                <div className="img-card-desc">{r.description || "-"}</div>
              </div>
              <div className="img-card-bottom">
                <div className="img-card-stats">
                  <span className="img-stat">
                    <svg viewBox="0 0 24 24" className="img-stat-icon"><polygon points="12 2 15.09 8.26 22 9.27 17 14.14 18.18 21.02 12 17.77 5.82 21.02 7 14.14 2 9.27 8.91 8.26 12 2"/></svg>
                    {r.stars ?? "-"}
                  </span>
                  <span className="img-stat">
                    <svg viewBox="0 0 24 24" className="img-stat-icon"><path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"/><polyline points="7 10 12 15 17 10"/><line x1="12" y1="15" x2="12" y2="3"/></svg>
                    {formatPulls(r.pulls)}
                  </span>
                </div>
                <div className="img-card-actions">
                  <button className="btn sm" onClick={() => openRunWithImage(r.reference)}>{t("run")}</button>
                  <button className="btn sm" disabled={!canTags(r.reference)} onClick={() => onTags(r.reference)}>{t("tags")}</button>
                </div>
              </div>
            </div>
          ))}
        </div>
      )}

      {/* Run Container Modal */}
      {showRunModal && (
        <div className="modal-backdrop" onClick={() => setShowRunModal(false)}>
          <div className="modal" onClick={e => e.stopPropagation()} style={{ maxWidth: 480 }}>
            <div className="modal-head">
              <div className="modal-title">{t("runContainer")}</div>
              <div className="modal-actions">
                <button className="icon-btn" onClick={() => setShowRunModal(false)} title={t("close")}>×</button>
              </div>
            </div>
            <div className="modal-body">
              <div className="form">
                <div className="row">
                  <label>{t("image")}</label>
                  <input className="input" value={runImage} onChange={e => setRunImage(e.target.value)} placeholder="nginx:latest" autoFocus />
                </div>
                <div className="row">
                  <label>{t("nameOptional")}</label>
                  <input className="input" value={runName} onChange={e => setRunName(e.target.value)} placeholder="web" />
                </div>
                <div className="row two">
                  <div>
                    <label>{t("cpus")}</label>
                    <input className="input" type="number" min={1} value={runCpus} onChange={e => setRunCpus(e.target.value === "" ? "" : Number(e.target.value))} />
                  </div>
                  <div>
                    <label>{t("memoryMb")}</label>
                    <input className="input" type="number" min={64} value={runMem} onChange={e => setRunMem(e.target.value === "" ? "" : Number(e.target.value))} />
                  </div>
                </div>
                <div className="row inline">
                  <input type="checkbox" checked={runPull} onChange={e => setRunPull(e.target.checked)} />
                  <span>{t("pullBeforeRun")}</span>
                </div>
              </div>
              {runResult && (
                <div className="result" style={{ marginTop: 14 }}>
                  <div className="result-title">{t("loginCommand")}</div>
                  <div className="result-code">
                    <code>{runResult.login_cmd}</code>
                    <button className="icon-btn" onClick={() => onCopy(runResult.login_cmd)} title={t("copy")}>{I.copy}</button>
                  </div>
                </div>
              )}
            </div>
            <div className="modal-footer">
              <button className="btn" onClick={() => setShowRunModal(false)}>{t("close")}</button>
              <button className="btn primary" disabled={runLoading || !runImage.trim()} onClick={onRun} style={{ marginLeft: 8 }}>
                {runLoading ? t("creating") : t("create")}
              </button>
            </div>
          </div>
        </div>
      )}

      {/* Import / Push Modal */}
      {showImportModal && (
        <div className="modal-backdrop" onClick={() => setShowImportModal(false)}>
          <div className="modal" onClick={e => e.stopPropagation()} style={{ maxWidth: 480 }}>
            <div className="modal-head">
              <div className="modal-title">{t("importImage")} / {t("pushImage")}</div>
              <div className="modal-actions">
                <button className="icon-btn" onClick={() => setShowImportModal(false)} title={t("close")}>×</button>
              </div>
            </div>
            <div className="modal-body">
              <div className="form">
                <div className="row">
                  <label>{t("imageArchivePath")}</label>
                  <div style={{ display: "flex", gap: 8 }}>
                    <input className="input" style={{ flex: 1 }} value={loadPath} onChange={e => setLoadPath(e.target.value)} placeholder="/path/to/image.tar" autoFocus />
                    <button className="btn" onClick={browseFile}>{t("browse")}</button>
                  </div>
                </div>
                <div className="row">
                  <button className="btn primary" disabled={loadLoading || !loadPath.trim()} onClick={onLoad}>
                    {loadLoading ? t("working") : t("load")}
                  </button>
                </div>
                <div className="hint">{t("importHint")}</div>
                <div style={{ borderTop: "1px solid var(--border)", margin: "8px 0" }} />
                <div className="row">
                  <label>{t("imageRef")}</label>
                  <input className="input" value={pushRef} onChange={e => setPushRef(e.target.value)} placeholder="ghcr.io/org/image:tag" />
                </div>
                <div className="row">
                  <button className="btn" disabled={pushLoading || !pushRef.trim()} onClick={onPush}>
                    {pushLoading ? t("working") : t("push")}
                  </button>
                </div>
                <div className="hint">{t("pushHint")}</div>
              </div>
            </div>
          </div>
        </div>
      )}
    </div>
  )
}
