import { useState, useEffect, useCallback } from "react"
import { invoke } from "@tauri-apps/api/core"
import { open } from "@tauri-apps/plugin-dialog"
import { I } from "../icons"
import { ErrorInline } from "../components/ErrorDisplay"
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
  const [activeTab, setActiveTab] = useState<"local" | "search">("local")
  const canTags = (ref: string) => ref.includes(".") || ref.includes(":") || ref.startsWith("localhost/")

  // Local images state
  const [localImages, setLocalImages] = useState<LocalImageInfo[]>([])
  const [localLoading, setLocalLoading] = useState(true)
  const [localFilter, setLocalFilter] = useState("")
  const [inspectInfo, setInspectInfo] = useState<ImageInspectInfo | null>(null)
  const [inspectLoading, setInspectLoading] = useState(false)
  const [confirmRemove, setConfirmRemove] = useState("")
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
    <div className="page">
      {/* Tab Navigation */}
      <div className="tabs">
        <button
          className={`tab ${activeTab === "local" ? "active" : ""}`}
          onClick={() => setActiveTab("local")}
        >
          <span className="icon">{I.layers}</span>
          {t("localImages")} ({localImages.length})
        </button>
        <button
          className={`tab ${activeTab === "search" ? "active" : ""}`}
          onClick={() => setActiveTab("search")}
        >
          <span className="icon">{I.globe}</span>
          {t("searchImages")}
        </button>
      </div>

      {imgError && <ErrorInline message={imgError} onDismiss={() => setImgError("")} />}

      {/* ---- Local Images Section ---- */}
      {activeTab === "local" && (
        <>
          <div className="toolbar">
            <input
              className="input toolbar-search"
              placeholder={t("filterLocalImages")}
              value={localFilter}
              onChange={e => setLocalFilter(e.target.value)}
            />
            <div className="toolbar-spacer" />
            <button className="btn" onClick={() => setShowImportModal(true)}>
              <span className="icon">{I.plus}</span>{t("importImage")}
            </button>
            <button className="btn" onClick={fetchLocalImages} disabled={localLoading}>
              <span className="icon">{I.refresh}</span>{t("refresh")}
            </button>
          </div>

          <div className="page-scroll">
          {localLoading ? (
            <div className="loading"><div className="spinner" />{t("loading")}</div>
          ) : filteredImages.length === 0 ? (
            <div className="empty-state">
              <div className="empty-icon">{I.layers}</div>
              <h3>{t("noLocalImages")}</h3>
            </div>
          ) : (
            <div className="image-list">
              {filteredImages.map((img, idx) => (
                <div className="image-item" key={`${img.id}-${idx}`}>
                  <div className="image-item-main">
                    <div className="image-item-icon">{I.layers}</div>
                    <div className="image-item-body">
                      <div className="image-item-name">
                        {img.repo_tags.length > 0
                          ? img.repo_tags.join(", ")
                          : "<none>:<none>"}
                      </div>
                      <div className="image-item-meta">
                        <span>{t("imageId")}: {img.id.slice(0, 16)}</span>
                        <span className="meta-sep" />
                        <span>{img.size_human}</span>
                        <span className="meta-sep" />
                        <span>{formatCreated(img.created)}</span>
                      </div>
                    </div>
                  </div>
                  <div className="image-item-actions">
                    <div className="image-item-actions-group">
                      <button
                        className="action-btn"
                        onClick={() => openRunWithImage(img.repo_tags[0] || img.id)}
                        title={t("run")}
                      >
                        {I.play}<span className="action-label">{t("run")}</span>
                      </button>
                      <button
                        className="action-btn"
                        onClick={() => handleInspect(img.repo_tags[0] || img.id)}
                        title={t("inspectImage")}
                        disabled={inspectLoading}
                      >
                        {I.fileText}<span className="action-label">{t("inspectImage")}</span>
                      </button>
                      <button
                        className="action-btn"
                        onClick={() => openTagModal(img.repo_tags[0] || img.id)}
                        title={t("tagImage")}
                      >
                        {I.plus}<span className="action-label">{t("tagImage")}</span>
                      </button>
                      <button
                        className="action-btn"
                        onClick={() => onCopy(img.id)}
                        title={t("copyId")}
                      >
                        {I.copy}
                      </button>
                    </div>
                    <div className="image-item-actions-sep" />
                    <button
                      className="action-btn danger"
                      onClick={() => setConfirmRemove(img.repo_tags[0] || img.id)}
                      title={t("removeImage")}
                    >
                      {I.trash}
                    </button>
                  </div>
                </div>
              ))}
            </div>
          )}
          </div>
        </>
      )}

      {/* ---- Registry Search Section ---- */}
      {activeTab === "search" && (
        <>
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
          </div>

          <div className="page-scroll">
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
              <div className="empty-icon">{I.globe}</div>
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
                    {r.description && <div className="img-card-desc">{r.description}</div>}
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
                      <button className="btn sm" onClick={() => openRunWithImage(r.reference)}>
                        <span className="icon">{I.play}</span>{t("run")}
                      </button>
                      <button className="btn sm" disabled={!canTags(r.reference)} onClick={() => onTags(r.reference)}>{t("tags")}</button>
                    </div>
                  </div>
                </div>
              ))}
            </div>
          )}
          </div>
        </>
      )}

      {/* Run Container Modal */}
      {showRunModal && (
        <div className="modal-backdrop" onClick={() => setShowRunModal(false)}>
          <div className="modal" onClick={e => e.stopPropagation()} style={{ maxWidth: 480 }}>
            <div className="modal-head">
              <div className="modal-title">{t("runContainer")}</div>
              <div className="modal-actions">
                <button className="icon-btn" onClick={() => setShowRunModal(false)} title={t("close")}>x</button>
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
                <button className="icon-btn" onClick={() => setShowImportModal(false)} title={t("close")}>x</button>
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

      {/* Inspect Image Modal */}
      {inspectInfo && (
        <div className="modal-backdrop" onClick={() => setInspectInfo(null)}>
          <div className="modal" onClick={e => e.stopPropagation()} style={{ maxWidth: 640 }}>
            <div className="modal-head">
              <div className="modal-title">{t("inspectImage")} -- {inspectInfo.id}</div>
              <div className="modal-actions">
                <button className="icon-btn" onClick={() => setInspectInfo(null)} title={t("close")}>x</button>
              </div>
            </div>
            <div className="modal-body">
              <div className="form">
                <div className="row">
                  <label>{t("imageId")}</label>
                  <div>{inspectInfo.id}</div>
                </div>
                <div className="row">
                  <label>{t("repoTags")}</label>
                  <div>{inspectInfo.repo_tags.length > 0 ? inspectInfo.repo_tags.join(", ") : "-"}</div>
                </div>
                <div className="row">
                  <label>{t("imageSize")}</label>
                  <div>{(inspectInfo.size_bytes / (1024 * 1024)).toFixed(1)} MB</div>
                </div>
                <div className="row">
                  <label>{t("imageCreated")}</label>
                  <div>{inspectInfo.created || "-"}</div>
                </div>
                <div className="row">
                  <label>{t("architecture")}</label>
                  <div>{inspectInfo.architecture || "-"}</div>
                </div>
                <div className="row">
                  <label>OS</label>
                  <div>{inspectInfo.os || "-"}</div>
                </div>
                <div className="row">
                  <label>Docker Version</label>
                  <div>{inspectInfo.docker_version || "-"}</div>
                </div>
                <div className="row">
                  <label>{t("layers")}</label>
                  <div>{inspectInfo.layers}</div>
                </div>
              </div>
            </div>
            <div className="modal-footer">
              <button className="btn" onClick={() => setInspectInfo(null)}>{t("close")}</button>
            </div>
          </div>
        </div>
      )}

      {/* Tag Image Modal */}
      {showTagModal && (
        <div className="modal-backdrop" onClick={() => setShowTagModal(false)}>
          <div className="modal" onClick={e => e.stopPropagation()} style={{ maxWidth: 480 }}>
            <div className="modal-head">
              <div className="modal-title">{t("tagImage")}</div>
              <div className="modal-actions">
                <button className="icon-btn" onClick={() => setShowTagModal(false)} title={t("close")}>x</button>
              </div>
            </div>
            <div className="modal-body">
              <div className="form">
                <div className="row">
                  <label>{t("sourceRef")}</label>
                  <div style={{ color: "var(--text-secondary)" }}>{tagSource}</div>
                </div>
                <div className="row">
                  <label>{t("targetTag")} (repository)</label>
                  <input
                    className="input"
                    value={tagRepo}
                    onChange={e => setTagRepo(e.target.value)}
                    placeholder="myrepo/myimage"
                    autoFocus
                    onKeyDown={e => { if (e.key === "Enter") handleTag() }}
                  />
                </div>
                <div className="row">
                  <label>{t("targetTag")} (tag)</label>
                  <input
                    className="input"
                    value={tagTag}
                    onChange={e => setTagTag(e.target.value)}
                    placeholder="latest"
                  />
                </div>
              </div>
            </div>
            <div className="modal-footer">
              <button className="btn" onClick={() => setShowTagModal(false)}>{t("close")}</button>
              <button className="btn primary" disabled={tagLoading || !tagRepo.trim()} onClick={handleTag} style={{ marginLeft: 8 }}>
                {tagLoading ? t("working") : t("tagImage")}
              </button>
            </div>
          </div>
        </div>
      )}

      {/* Confirm Remove Image Modal */}
      {confirmRemove && (
        <div className="modal-backdrop" onClick={() => setConfirmRemove("")}>
          <div className="modal" onClick={e => e.stopPropagation()} style={{ maxWidth: 420 }}>
            <div className="modal-head">
              <div className="modal-title">{t("removeImage")}</div>
              <div className="modal-actions">
                <button className="icon-btn" onClick={() => setConfirmRemove("")} title={t("close")}>x</button>
              </div>
            </div>
            <div className="modal-body">
              <p style={{ margin: 0 }}>{t("confirmRemoveImage")}</p>
              <p style={{ margin: "8px 0 0", fontWeight: 600, color: "var(--text)" }}>{confirmRemove}</p>
            </div>
            <div className="modal-footer">
              <button className="btn" onClick={() => setConfirmRemove("")}>{t("close")}</button>
              <button className="btn primary" onClick={() => handleRemoveImage(confirmRemove)} style={{ marginLeft: 8, background: "var(--red)", borderColor: "var(--red)" }}>
                {t("remove")}
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  )
}
