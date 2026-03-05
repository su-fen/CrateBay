import { useState } from "react"
import { invoke } from "@tauri-apps/api/core"
import { langNames } from "../i18n/messages"
import { I } from "../icons"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Card, CardContent } from "@/components/ui/card"
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select"
import { cardActionOutline, cardActionSecondary } from "@/lib/styles"
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

  const sectionTitle = (key: string) => {
    const value = t(key)
    return value.length <= 24 ? value.toUpperCase() : value
  }

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
    <div className="space-y-6">
      <section className="space-y-3">
        <div className="text-xs font-semibold tracking-widest text-muted-foreground">
          {sectionTitle("theme")}
        </div>
        <Card className="py-0">
          <CardContent className="py-4">
            <div className="flex items-center gap-4">
              <div className="size-10 shrink-0 rounded-lg bg-primary/10 text-primary flex items-center justify-center [&_svg]:size-5 [&_svg]:fill-none [&_svg]:stroke-current [&_svg]:stroke-2 [&_svg]:stroke-linecap-round [&_svg]:stroke-linejoin-round">
                {I.moon}
              </div>
              <div className="min-w-0 flex-1">
                <div className="text-sm font-semibold text-foreground">
                  {t("theme")}
                </div>
                <div className="text-xs text-muted-foreground">
                  {t("themeDesc")}
                </div>
              </div>
              <Select
                value={theme}
                onValueChange={(v) => setTheme(v as Theme)}
              >
                <SelectTrigger size="sm" className="w-[140px] justify-between">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent align="end">
                  <SelectItem value="system">{t("systemTheme")}</SelectItem>
                  <SelectItem value="dark">{t("dark")}</SelectItem>
                  <SelectItem value="light">{t("light")}</SelectItem>
                </SelectContent>
              </Select>
            </div>
          </CardContent>
        </Card>
      </section>

      <section className="space-y-3">
        <div className="text-xs font-semibold tracking-widest text-muted-foreground">
          {sectionTitle("language")}
        </div>
        <Card className="py-0">
          <CardContent className="py-4">
            <div className="flex items-center gap-4">
              <div className="size-10 shrink-0 rounded-lg bg-primary/10 text-primary flex items-center justify-center [&_svg]:size-5 [&_svg]:fill-none [&_svg]:stroke-current [&_svg]:stroke-2 [&_svg]:stroke-linecap-round [&_svg]:stroke-linejoin-round">
                {I.globe}
              </div>
              <div className="min-w-0 flex-1">
                <div className="text-sm font-semibold text-foreground">
                  {t("language")}
                </div>
                <div className="text-xs text-muted-foreground">
                  {t("languageDesc")}
                </div>
              </div>
              <Select
                value={lang}
                onValueChange={(v) => setLang(normalizeLang(v))}
              >
                <SelectTrigger size="sm" className="w-[140px] justify-between">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent align="end">
                  {Object.entries(langNames).map(([code, name]) => (
                    <SelectItem key={code} value={code}>
                      {name}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </div>
          </CardContent>
        </Card>
      </section>

      <section className="space-y-3">
        <div className="text-xs font-semibold tracking-widest text-muted-foreground">
          {sectionTitle("updates")}
        </div>
        <Card className="py-0">
          <CardContent className="py-4">
            <div className="flex items-center gap-4">
              <div className="size-10 shrink-0 rounded-lg bg-primary/10 text-primary flex items-center justify-center [&_svg]:size-5 [&_svg]:fill-none [&_svg]:stroke-current [&_svg]:stroke-2 [&_svg]:stroke-linecap-round [&_svg]:stroke-linejoin-round">
                {I.refresh}
              </div>
              <div className="min-w-0 flex-1">
                <div className="text-sm font-semibold text-foreground">
                  {t("updates")}
                </div>
                <div className="text-xs text-muted-foreground">
                  {t("currentVersion")}:{" "}
                  <Badge variant="secondary" className="ml-1 rounded-md px-1.5 py-0 text-[10px]">
                    v{updateInfo?.current_version ?? "1.0.0"}
                  </Badge>
                </div>
              </div>
              <Button
                type="button"
                variant="outline"
                size="sm"
                className={cardActionOutline}
                onClick={handleCheckUpdate}
                disabled={checking}
              >
                {checking ? t("checkingUpdates") : t("checkUpdates")}
              </Button>
            </div>
          </CardContent>
        </Card>

        {updateInfo && (
          <Alert className="border-border/70 bg-card">
            <div className="flex items-start gap-3">
              <div className="mt-0.5 text-primary [&_svg]:size-4 [&_svg]:fill-none [&_svg]:stroke-current [&_svg]:stroke-2 [&_svg]:stroke-linecap-round [&_svg]:stroke-linejoin-round">
                {updateInfo.available ? I.alertCircle : I.check}
              </div>
              <div className="min-w-0 flex-1">
                <AlertTitle>
                  {updateInfo.available
                    ? `${t("updateAvailable")}: v${updateInfo.latest_version}`
                    : t("noUpdates")}
                </AlertTitle>
                {updateInfo.available && updateInfo.release_notes && (
                  <AlertDescription>
                    <p className="whitespace-pre-wrap">
                      {updateInfo.release_notes}
                    </p>
                  </AlertDescription>
                )}
              </div>
              {updateInfo.available && (
                <Button
                  type="button"
                  variant="secondary"
                  size="sm"
                  className={cardActionSecondary}
                  onClick={handleViewRelease}
                >
                  {t("viewRelease")}
                </Button>
              )}
            </div>
          </Alert>
        )}

        {updateError && (
          <Alert variant="destructive">
            <div className="flex items-start gap-3">
              <div className="mt-0.5 [&_svg]:size-4 [&_svg]:fill-none [&_svg]:stroke-current [&_svg]:stroke-2 [&_svg]:stroke-linecap-round [&_svg]:stroke-linejoin-round">
                {I.alertCircle}
              </div>
              <div className="min-w-0 flex-1">
                <AlertTitle>{t("updates")}</AlertTitle>
                <AlertDescription>
                  <p className="whitespace-pre-wrap">{updateError}</p>
                </AlertDescription>
              </div>
            </div>
          </Alert>
        )}
      </section>
    </div>
  )
}
