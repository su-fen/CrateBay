import { langNames } from "../i18n/messages"
import { I } from "../icons"
import type { Theme } from "../types"

interface SettingsProps {
  theme: Theme
  setTheme: (v: Theme) => void
  lang: string
  setLang: (v: string) => void
  t: (key: string) => string
}

export function Settings({ theme, setTheme, lang, setLang, t }: SettingsProps) {
  const normalizeLang = (value: string) => (value === "zh" ? "zh" : "en")

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
    </div>
  )
}
