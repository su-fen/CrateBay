import { I } from "../icons"
import { ErrorInline } from "../components/ErrorDisplay"
import type { ImageSearchResult, RunContainerResult } from "../types"

interface ImagesProps {
  imgQuery: string
  setImgQuery: (v: string) => void
  imgSource: string
  setImgSource: (v: string) => void
  imgLimit: number
  setImgLimit: (v: number) => void
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
  onClear: () => void
  onCopy: (text: string) => void
  t: (key: string) => string
}

export function Images({
  imgQuery, setImgQuery, imgSource, setImgSource,
  imgLimit, setImgLimit, imgResults, imgSearching,
  imgError, setImgError, imgTags, imgTagsRef, imgTagsLoading,
  runImage, setRunImage, runName, setRunName,
  runCpus, setRunCpus, runMem, setRunMem,
  runPull, setRunPull, runLoading, runResult, setRunResult,
  loadPath, setLoadPath, loadLoading,
  pushRef, setPushRef, pushLoading,
  onSearch, onTags, onRun, onLoad, onPush, onClear, onCopy, t,
}: ImagesProps) {
  const canTags = (ref: string) => ref.includes(".") || ref.includes(":") || ref.startsWith("localhost/")

  return (
    <div className="page">
      <div className="toolbar">
        <input
          className="input"
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
        <input
          className="input small"
          type="number"
          min={1}
          max={100}
          value={imgLimit}
          onChange={e => setImgLimit(Number(e.target.value) || 20)}
        />
        <button className="btn primary" disabled={imgSearching || !imgQuery.trim()} onClick={onSearch}>
          {imgSearching ? t("searching") : t("search")}
        </button>
        <button className="btn" onClick={onClear}>{t("clear")}</button>
      </div>

      {imgError && <ErrorInline message={imgError} onDismiss={() => setImgError("")} />}

      {/* Search results full width, run form below */}
      <div className="panel">
        <div className="panel-title"><div className="panel-title-icon">{I.layers}</div>{t("results")}</div>
        {imgResults.length === 0 ? (
          <div className="hint">{t("searchHint")}</div>
        ) : (
          <div className="table">
            <div className="tr head">
              <div>{t("source")}</div>
              <div>{t("image")}</div>
              <div className="right">{t("stars")}</div>
              <div className="right">{t("pulls")}</div>
              <div className="grow">{t("description")}</div>
              <div className="right">{t("actions")}</div>
            </div>
            {imgResults.map((r, idx) => (
              <div className="tr" key={`${r.source}-${r.reference}-${idx}`}>
                <div className="badge">{r.source}</div>
                <div className="mono">{r.reference}{r.official ? ` (${t("official")})` : ""}</div>
                <div className="right">{r.stars ?? "-"}</div>
                <div className="right">{r.pulls ?? "-"}</div>
                <div className="grow">{r.description || "-"}</div>
                <div className="right">
                  <button className="btn small" onClick={() => { setRunImage(r.reference); setRunName(""); setRunResult(null) }}>
                    {t("run")}
                  </button>
                  <button className="btn small" disabled={!canTags(r.reference)} onClick={() => onTags(r.reference)}>
                    {t("tags")}
                  </button>
                </div>
              </div>
            ))}
          </div>
        )}
      </div>

      <div className="grid2">
        <div className="panel">
          <div className="panel-title"><div className="panel-title-icon">{I.play}</div>{t("runContainer")}</div>
          <div className="form">
            <div className="row">
              <label>{t("image")}</label>
              <input className="input" value={runImage} onChange={e => setRunImage(e.target.value)} placeholder="nginx:latest" />
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
            <div className="row">
              <button className="btn primary" disabled={runLoading || !runImage.trim()} onClick={onRun}>
                {runLoading ? t("creating") : t("create")}
              </button>
            </div>
            {runResult && (
              <div className="result">
                <div className="result-title">{t("loginCommand")}</div>
                <div className="result-code">
                  <code>{runResult.login_cmd}</code>
                  <button className="icon-btn" onClick={() => onCopy(runResult.login_cmd)} title={t("copy")}>{I.copy}</button>
                </div>
              </div>
            )}
          </div>
        </div>

        <div className="panel">
          <div className="panel-subtitle">{t("tags")}</div>
          {imgTagsLoading ? (
            <div className="hint">{t("loading")}</div>
          ) : imgTags.length === 0 ? (
            <div className="hint">{imgTagsRef ? t("noTags") : t("tagsHint")}</div>
          ) : (
            <div className="tags">
              {imgTags.map(tag => (
                <div className="tag" key={tag} onClick={() => { setRunImage(`${imgTagsRef}:${tag}`); setRunResult(null) }}>
                  {tag}
                </div>
              ))}
            </div>
          )}
        </div>
      </div>

      <div className="grid2">
        <div className="panel">
          <div className="panel-title"><div className="panel-title-icon">{I.plus}</div>{t("importImage")}</div>
          <div className="form">
            <div className="row">
              <label>{t("imageArchivePath")}</label>
              <input className="input" value={loadPath} onChange={e => setLoadPath(e.target.value)} placeholder="/path/to/image.tar" />
            </div>
            <div className="row">
              <button className="btn" disabled={loadLoading || !loadPath.trim()} onClick={onLoad}>
                {loadLoading ? t("working") : t("load")}
              </button>
            </div>
            <div className="hint">{t("importHint")}</div>
          </div>
        </div>
        <div className="panel">
          <div className="panel-title"><div className="panel-title-icon">{I.cpu}</div>{t("pushImage")}</div>
          <div className="form">
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
  )
}
