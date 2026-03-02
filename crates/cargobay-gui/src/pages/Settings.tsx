import { useState } from "react"
import { invoke } from "@tauri-apps/api/core"
import { langNames } from "../i18n/messages"
import { I } from "../icons"
import type { Theme } from "../types"

interface UpdateInfo {
  available: boolean
  current_version: string
  latest_version: string
  release_notes: string
  download_url: string
}

interface SettingsProps {
  theme: Theme
  setTheme: (v: Theme) => void
  lang: string
  setLang: (v: string) => void
  t: (key: string) => string
}

export function Settings({ theme, setTheme, lang, setLang, t }: SettingsProps) {
  const normalizeLang = (value: string) => (value === "zh" ? "zh" : "en")
  const [checking, setChecking] = useState(false)
  const [updateInfo, setUpdateInfo] = useState<UpdateInfo | null>(null)
  const [updateError, setUpdateError] = useState("")

  const handleCheckUpdate = async () => {
    setChecking(true)
    setUpdateError("")
    setUpdateInfo(null)
    try {
      const info = await invoke<UpdateInfo>("check_update")
      setUpdateInfo(info)
    } catch (e) {
      setUpdateError(String(e))
    } finally {
      setChecking(false)
    }
  }

  const handleViewRelease = async () => {
    if (!updateInfo?.download_url) return
    try {
      await invoke("open_release_page", { url: updateInfo.download_url })
    } catch {
      window.open(updateInfo.download_url, "_blank")
    }
  }

  return (
    <div className="settings">
      <div className="settings-section-title">{t("theme")}</div>
      <div className="setting-row">
        <div className="setting-icon">{I.moon}</div>
        <div className="setting-info">
          <div className="setting-label">{t("theme")}</div>
          <div className="setting-desc">{t("themeDesc")}</div>
        </div>
        <select value={theme} onChange={e => setTheme(e.target.value as Theme)}>
          <option value="dark">{t("dark")}</option>
          <option value="light">{t("light")}</option>
        </select>
      </div>
      <div className="settings-section-title">{t("language")}</div>
      <div className="setting-row">
        <div className="setting-icon">{I.globe}</div>
        <div className="setting-info">
          <div className="setting-label">{t("language")}</div>
          <div className="setting-desc">{t("languageDesc")}</div>
        </div>
        <select value={lang} onChange={e => setLang(normalizeLang(e.target.value))}>
          {Object.entries(langNames).map(([code, name]) => (
            <option key={code} value={code}>{name}</option>
          ))}
        </select>
      </div>
      <div className="settings-section-title">{t("updates")}</div>
      <div className="setting-row">
        <div className="setting-icon">{I.refresh}</div>
        <div className="setting-info">
          <div className="setting-label">{t("updates")}</div>
          <div className="setting-desc">
            {t("currentVersion")}: v{updateInfo?.current_version ?? "0.1.0"}
          </div>
        </div>
        <button
          className="btn"
          onClick={handleCheckUpdate}
          disabled={checking}
        >
          {checking ? t("checkingUpdates") : t("checkUpdates")}
        </button>
      </div>
      {updateInfo && (
        <div className="setting-row update-result">
          <div className="setting-icon">
            <svg viewBox="0 0 24 24">
              <circle cx="12" cy="12" r="10" />
              {updateInfo.available
                ? <><line x1="12" y1="16" x2="12" y2="12" /><line x1="12" y1="8" x2="12.01" y2="8" /></>
                : <polyline points="9 12 11 14 15 10" />
              }
            </svg>
          </div>
          <div className="setting-info">
            <div className="setting-label">
              {updateInfo.available
                ? `${t("updateAvailable")}: v${updateInfo.latest_version}`
                : t("noUpdates")
              }
            </div>
            {updateInfo.available && updateInfo.release_notes && (
              <div className="setting-desc update-notes">{updateInfo.release_notes}</div>
            )}
          </div>
          {updateInfo.available && (
            <button className="btn btn-accent" onClick={handleViewRelease}>
              {t("viewRelease")}
            </button>
          )}
        </div>
      )}
      {updateError && (
        <div className="setting-row">
          <div className="setting-icon">{I.alertCircle}</div>
          <div className="setting-info">
            <div className="setting-desc" style={{ color: "var(--red)" }}>{updateError}</div>
          </div>
        </div>
      )}
    </div>
  )
}
