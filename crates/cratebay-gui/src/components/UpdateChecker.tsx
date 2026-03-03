import { useState, useEffect } from "react"
import { invoke } from "@tauri-apps/api/core"

interface UpdateInfo {
  available: boolean
  current_version: string
  latest_version: string
  release_notes: string
  download_url: string
}

interface UpdateCheckerProps {
  t: (key: string) => string
}

export function UpdateChecker({ t }: UpdateCheckerProps) {
  const [update, setUpdate] = useState<UpdateInfo | null>(null)
  const [dismissed, setDismissed] = useState(false)

  useEffect(() => {
    const timer = setTimeout(async () => {
      try {
        const info = await invoke<UpdateInfo>("check_update")
        if (info.available) {
          setUpdate(info)
        }
      } catch {
        // Silently ignore update check failures on startup
      }
    }, 3000)
    return () => clearTimeout(timer)
  }, [])

  if (!update || !update.available || dismissed) return null

  const handleViewRelease = async () => {
    try {
      await invoke("open_release_page", { url: update.download_url })
    } catch {
      // Fallback: try window.open
      window.open(update.download_url, "_blank")
    }
  }

  return (
    <div className="update-banner">
      <div className="update-banner-content">
        <svg className="update-banner-icon" viewBox="0 0 24 24">
          <circle cx="12" cy="12" r="10" />
          <line x1="12" y1="16" x2="12" y2="12" />
          <line x1="12" y1="8" x2="12.01" y2="8" />
        </svg>
        <span className="update-banner-text">
          {t("updateAvailable")}: <strong>v{update.latest_version}</strong>
        </span>
        <button className="update-banner-link" onClick={handleViewRelease}>
          {t("viewRelease")}
        </button>
      </div>
      <button className="update-banner-dismiss" onClick={() => setDismissed(true)}>
        &times;
      </button>
    </div>
  )
}
