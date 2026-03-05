import { useState, useEffect } from "react"
import { invoke } from "@tauri-apps/api/core"
import { I } from "../icons"
import { Alert, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import { cn } from "@/lib/utils"

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
    <div className="fixed top-[42px] left-[220px] right-0 z-50 px-6 py-2 pointer-events-none">
      <Alert className="pointer-events-auto border-primary/20 bg-card/95 shadow-sm backdrop-blur supports-[backdrop-filter]:bg-card/70">
        <div className="flex items-start gap-3">
          <div className={cn("mt-0.5 text-primary", "[&_svg]:size-4", "[&_svg]:fill-none", "[&_svg]:stroke-current", "[&_svg]:stroke-2")}>
            {I.alertCircle}
          </div>
          <div className="min-w-0 flex-1">
            <AlertTitle>
              {t("updateAvailable")}: v{update.latest_version}
            </AlertTitle>
          </div>
          <div className="flex items-center gap-2">
            <Button type="button" variant="secondary" size="sm" onClick={handleViewRelease}>
              {t("viewRelease")}
            </Button>
            <Button
              type="button"
              variant="ghost"
              size="icon-xs"
              onClick={() => setDismissed(true)}
              aria-label="Close"
            >
              ×
            </Button>
          </div>
        </div>
      </Alert>
    </div>
  )
}
