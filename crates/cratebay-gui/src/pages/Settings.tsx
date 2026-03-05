import { useEffect, useMemo, useState } from "react"
import { invoke } from "@tauri-apps/api/core"
import { langNames } from "../i18n/messages"
import { I } from "../icons"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Card, CardContent } from "@/components/ui/card"
import { Checkbox } from "@/components/ui/checkbox"
import { Input } from "@/components/ui/input"
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select"
import { cardActionOutline, cardActionSecondary } from "@/lib/styles"
import type {
  AgentCliPreset,
  AgentCliRunResult,
  AiConnectionTestResult,
  AiSkillDefinition,
  AiProfileValidationResult,
  AiProviderProfile,
  AiSettings,
  Theme,
} from "../types"

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

const linesToList = (value: string) =>
  value
    .split("\n")
    .map((item) => item.trim())
    .filter(Boolean)

const listToLines = (list: string[]) => list.join("\n")

export function Settings({ theme, setTheme, lang, setLang, t }: SettingsProps) {
  const normalizeLang = (value: string) => (value === "zh" ? "zh" : "en")
  const [checking, setChecking] = useState(false)
  const [updateInfo, setUpdateInfo] = useState<UpdateInfo | null>(null)
  const [updateError, setUpdateError] = useState("")
  const [aiLoading, setAiLoading] = useState(true)
  const [aiSaving, setAiSaving] = useState(false)
  const [aiValidating, setAiValidating] = useState(false)
  const [aiSettings, setAiSettings] = useState<AiSettings | null>(null)
  const [aiError, setAiError] = useState("")
  const [aiMessage, setAiMessage] = useState("")
  const [headersJson, setHeadersJson] = useState("{}")
  const [apiKeyInput, setApiKeyInput] = useState("")
  const [secretUpdating, setSecretUpdating] = useState(false)
  const [secretExists, setSecretExists] = useState<boolean | null>(null)
  const [connectionTesting, setConnectionTesting] = useState(false)
  const [connectionResult, setConnectionResult] = useState<AiConnectionTestResult | null>(null)
  const [mcpAllowedActionsText, setMcpAllowedActionsText] = useState("")
  const [cliAllowlistText, setCliAllowlistText] = useState("")
  const [agentPresets, setAgentPresets] = useState<AgentCliPreset[]>([])
  const [selectedPreset, setSelectedPreset] = useState("")
  const [agentPrompt, setAgentPrompt] = useState("")
  const [agentUseCustom, setAgentUseCustom] = useState(false)
  const [agentCommand, setAgentCommand] = useState("")
  const [agentArgsText, setAgentArgsText] = useState("")
  const [agentDryRun, setAgentDryRun] = useState(true)
  const [agentRunning, setAgentRunning] = useState(false)
  const [agentResult, setAgentResult] = useState<AgentCliRunResult | null>(null)
  const [agentError, setAgentError] = useState("")

  const sectionTitle = (key: string) => {
    const value = t(key)
    return value.length <= 24 ? value.toUpperCase() : value
  }

  const activeProfile = useMemo(() => {
    if (!aiSettings) return null
    return (
      aiSettings.profiles.find((profile) => profile.id === aiSettings.active_profile_id) ?? null
    )
  }, [aiSettings])

  useEffect(() => {
    const loadAiSettings = async () => {
      setAiLoading(true)
      setAiError("")
      try {
        const settings = await invoke<AiSettings>("load_ai_settings")
        setAiSettings(settings)
        setMcpAllowedActionsText(
          listToLines(settings.security_policy.mcp_allowed_actions ?? [])
        )
        setCliAllowlistText(
          listToLines(settings.security_policy.cli_command_allowlist ?? [])
        )
      } catch (e) {
        setAiError(String(e))
      } finally {
        setAiLoading(false)
      }
    }

    const loadAgentPresets = async () => {
      try {
        const presets = await invoke<AgentCliPreset[]>("agent_cli_list_presets")
        setAgentPresets(presets)
        setSelectedPreset((prev) => prev || presets[0]?.id || "")
      } catch {
        setAgentPresets([])
      }
    }

    loadAiSettings()
    loadAgentPresets()
  }, [])

  useEffect(() => {
    if (!activeProfile) {
      setHeadersJson("{}")
      setSecretExists(null)
      return
    }
    setHeadersJson(JSON.stringify(activeProfile.headers ?? {}, null, 2))
    if (!activeProfile.api_key_ref?.trim()) {
      setSecretExists(false)
      return
    }
    invoke<boolean>("ai_secret_exists", { apiKeyRef: activeProfile.api_key_ref })
      .then((exists) => setSecretExists(exists))
      .catch(() => setSecretExists(null))
  }, [activeProfile])

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

  const parseHeadersJson = (): Record<string, string> | null => {
    const raw = headersJson.trim()
    if (!raw) return {}
    try {
      const parsed = JSON.parse(raw) as unknown
      if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) {
        setAiError(t("aiHeadersJsonError"))
        return null
      }
      const out: Record<string, string> = {}
      for (const [key, value] of Object.entries(parsed)) {
        if (typeof value !== "string") {
          setAiError(t("aiHeadersJsonError"))
          return null
        }
        out[key] = value
      }
      return out
    } catch {
      setAiError(t("aiHeadersJsonError"))
      return null
    }
  }

  const updateActiveProfile = (
    updater: (profile: AiProviderProfile) => AiProviderProfile
  ) => {
    setAiSettings((prev) => {
      if (!prev) return prev
      return {
        ...prev,
        profiles: prev.profiles.map((profile) =>
          profile.id === prev.active_profile_id ? updater(profile) : profile
        ),
      }
    })
  }

  const updateSkill = (
    skillId: string,
    updater: (skill: AiSkillDefinition) => AiSkillDefinition
  ) => {
    setAiSettings((prev) => {
      if (!prev) return prev
      return {
        ...prev,
        skills: (prev.skills ?? []).map((skill) =>
          skill.id === skillId ? updater(skill) : skill
        ),
      }
    })
  }

  const resolveActiveProfileWithHeaders = (): AiProviderProfile | null => {
    if (!activeProfile) return null
    const headers = parseHeadersJson()
    if (!headers) return null
    return { ...activeProfile, headers }
  }

  const withPolicyInputs = (settings: AiSettings): AiSettings => ({
    ...settings,
    security_policy: {
      ...settings.security_policy,
      mcp_allowed_actions: linesToList(mcpAllowedActionsText),
      cli_command_allowlist: linesToList(cliAllowlistText),
    },
  })

  const handleAiSaveSettings = async () => {
    if (!aiSettings || !activeProfile) return
    const profile = resolveActiveProfileWithHeaders()
    if (!profile) return

    const nextSettings = withPolicyInputs({
      ...aiSettings,
      profiles: aiSettings.profiles.map((item) =>
        item.id === aiSettings.active_profile_id ? profile : item
      ),
    })

    setAiSaving(true)
    setAiError("")
    setAiMessage("")
    try {
      const saved = await invoke<AiSettings>("save_ai_settings", {
        settings: nextSettings,
      })
      setAiSettings(saved)
      setAiMessage(t("aiSettingsSaved"))
    } catch (e) {
      setAiError(String(e))
    } finally {
      setAiSaving(false)
    }
  }

  const handleAiValidateProfile = async () => {
    if (!activeProfile) return
    const profile = resolveActiveProfileWithHeaders()
    if (!profile) return

    setAiValidating(true)
    setAiError("")
    setAiMessage("")
    try {
      const result = await invoke<AiProfileValidationResult>("validate_ai_profile", {
        profile,
      })
      if (result.ok) {
        setAiMessage(result.message || t("aiValidationPassed"))
      } else {
        setAiError(result.message || t("aiValidationFailed"))
      }
    } catch (e) {
      setAiError(String(e))
    } finally {
      setAiValidating(false)
    }
  }

  const handleAiSaveSecret = async () => {
    if (!activeProfile?.api_key_ref?.trim()) {
      setAiError(t("aiApiKeyRefRequired"))
      return
    }
    if (!apiKeyInput.trim()) {
      setAiError(t("aiApiKeyValueRequired"))
      return
    }
    setSecretUpdating(true)
    setAiError("")
    setAiMessage("")
    try {
      await invoke("ai_secret_set", {
        apiKeyRef: activeProfile.api_key_ref,
        apiKey: apiKeyInput.trim(),
      })
      setApiKeyInput("")
      setSecretExists(true)
      setAiMessage(t("aiSecretSaved"))
    } catch (e) {
      setAiError(String(e))
    } finally {
      setSecretUpdating(false)
    }
  }

  const handleAiDeleteSecret = async () => {
    if (!activeProfile?.api_key_ref?.trim()) {
      setAiError(t("aiApiKeyRefRequired"))
      return
    }
    setSecretUpdating(true)
    setAiError("")
    setAiMessage("")
    try {
      await invoke("ai_secret_delete", { apiKeyRef: activeProfile.api_key_ref })
      setSecretExists(false)
      setAiMessage(t("aiSecretDeleted"))
    } catch (e) {
      setAiError(String(e))
    } finally {
      setSecretUpdating(false)
    }
  }

  const handleAiTestConnection = async () => {
    if (!activeProfile) return
    setConnectionTesting(true)
    setAiError("")
    setAiMessage("")
    setConnectionResult(null)
    try {
      const result = await invoke<AiConnectionTestResult>("ai_test_connection", {
        profileId: activeProfile.id,
      })
      setConnectionResult(result)
      if (result.ok) {
        setAiMessage(result.message || t("aiConnectionSuccess"))
      } else {
        setAiError(result.message || t("aiConnectionFailed"))
      }
    } catch (e) {
      setAiError(String(e))
    } finally {
      setConnectionTesting(false)
    }
  }

  const handleRunAgentCli = async () => {
    setAgentRunning(true)
    setAgentError("")
    setAgentResult(null)
    try {
      const customArgs = linesToList(agentArgsText.replace(/\s+/g, " ").trim()).flatMap(
        (line) => line.split(" ").filter(Boolean)
      )
      const result = await invoke<AgentCliRunResult>("agent_cli_run", {
        presetId: agentUseCustom ? null : selectedPreset || null,
        command: agentUseCustom ? agentCommand.trim() : null,
        args: agentUseCustom ? customArgs : null,
        prompt: agentPrompt,
        dryRun: agentDryRun,
      })
      setAgentResult(result)
    } catch (e) {
      setAgentError(String(e))
    } finally {
      setAgentRunning(false)
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
                <div className="text-sm font-semibold text-foreground">{t("theme")}</div>
                <div className="text-xs text-muted-foreground">{t("themeDesc")}</div>
              </div>
              <Select value={theme} onValueChange={(v) => setTheme(v as Theme)}>
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
              <Select value={lang} onValueChange={(v) => setLang(normalizeLang(v))}>
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
          {sectionTitle("aiSettings")}
        </div>
        <Card className="py-0">
          <CardContent className="space-y-4 py-4">
            <div className="flex items-center gap-4">
              <div className="size-10 shrink-0 rounded-lg bg-primary/10 text-primary flex items-center justify-center [&_svg]:size-5 [&_svg]:fill-none [&_svg]:stroke-current [&_svg]:stroke-2 [&_svg]:stroke-linecap-round [&_svg]:stroke-linejoin-round">
                {I.key}
              </div>
              <div className="min-w-0 flex-1">
                <div className="text-sm font-semibold text-foreground">
                  {t("aiSettings")}
                </div>
                <div className="text-xs text-muted-foreground">
                  {t("aiSettingsDesc")}
                </div>
              </div>
            </div>

            {aiLoading && (
              <div className="text-sm text-muted-foreground">{t("loading")}</div>
            )}

            {!aiLoading && aiSettings && activeProfile && (
              <div className="space-y-4">
                <div className="grid gap-3 md:grid-cols-2">
                  <div>
                    <label className="text-xs font-semibold text-muted-foreground">
                      {t("aiActiveProfile")}
                    </label>
                    <Select
                      value={aiSettings.active_profile_id}
                      onValueChange={(value) => {
                        setAiError("")
                        setAiMessage("")
                        setAiSettings((prev) =>
                          prev ? { ...prev, active_profile_id: value } : prev
                        )
                      }}
                    >
                      <SelectTrigger size="sm" className="mt-1 w-full justify-between">
                        <SelectValue />
                      </SelectTrigger>
                      <SelectContent align="end">
                        {aiSettings.profiles.map((profile) => (
                          <SelectItem key={profile.id} value={profile.id}>
                            {profile.display_name}
                          </SelectItem>
                        ))}
                      </SelectContent>
                    </Select>
                  </div>
                  <div>
                    <label className="text-xs font-semibold text-muted-foreground">
                      {t("aiDisplayName")}
                    </label>
                    <Input
                      className="mt-1"
                      value={activeProfile.display_name}
                      onChange={(event) =>
                        updateActiveProfile((profile) => ({
                          ...profile,
                          display_name: event.target.value,
                        }))
                      }
                    />
                  </div>
                  <div>
                    <label className="text-xs font-semibold text-muted-foreground">
                      {t("aiProviderId")}
                    </label>
                    <Input
                      className="mt-1"
                      value={activeProfile.provider_id}
                      onChange={(event) =>
                        updateActiveProfile((profile) => ({
                          ...profile,
                          provider_id: event.target.value,
                        }))
                      }
                    />
                  </div>
                  <div>
                    <label className="text-xs font-semibold text-muted-foreground">
                      {t("aiModel")}
                    </label>
                    <Input
                      className="mt-1"
                      value={activeProfile.model}
                      onChange={(event) =>
                        updateActiveProfile((profile) => ({
                          ...profile,
                          model: event.target.value,
                        }))
                      }
                    />
                  </div>
                  <div>
                    <label className="text-xs font-semibold text-muted-foreground">
                      {t("aiBaseUrl")}
                    </label>
                    <Input
                      className="mt-1"
                      value={activeProfile.base_url}
                      onChange={(event) =>
                        updateActiveProfile((profile) => ({
                          ...profile,
                          base_url: event.target.value,
                        }))
                      }
                    />
                  </div>
                  <div>
                    <label className="text-xs font-semibold text-muted-foreground">
                      {t("aiApiKeyRef")}
                    </label>
                    <Input
                      className="mt-1"
                      value={activeProfile.api_key_ref}
                      onChange={(event) =>
                        updateActiveProfile((profile) => ({
                          ...profile,
                          api_key_ref: event.target.value,
                        }))
                      }
                    />
                  </div>
                </div>

                <div className="grid gap-3 md:grid-cols-[1fr_auto_auto_auto] md:items-end">
                  <div>
                    <label className="text-xs font-semibold text-muted-foreground">
                      {t("aiApiKeyValue")}
                    </label>
                    <Input
                      type="password"
                      className="mt-1"
                      value={apiKeyInput}
                      onChange={(event) => setApiKeyInput(event.target.value)}
                      placeholder={t("aiApiKeyInputPlaceholder")}
                    />
                    <div className="mt-1 text-xs text-muted-foreground">
                      {secretExists === true
                        ? t("aiSecretExists")
                        : secretExists === false
                          ? t("aiSecretNotFound")
                          : t("aiSecretUnknown")}
                    </div>
                  </div>
                  <Button
                    type="button"
                    variant="outline"
                    size="sm"
                    className={cardActionOutline}
                    onClick={handleAiSaveSecret}
                    disabled={secretUpdating}
                  >
                    {secretUpdating ? t("working") : t("aiSaveSecret")}
                  </Button>
                  <Button
                    type="button"
                    variant="outline"
                    size="sm"
                    className={cardActionOutline}
                    onClick={handleAiDeleteSecret}
                    disabled={secretUpdating}
                  >
                    {secretUpdating ? t("working") : t("aiDeleteSecret")}
                  </Button>
                  <Button
                    type="button"
                    variant="secondary"
                    size="sm"
                    className={cardActionSecondary}
                    onClick={handleAiTestConnection}
                    disabled={connectionTesting || secretUpdating}
                  >
                    {connectionTesting ? t("working") : t("aiTestConnection")}
                  </Button>
                </div>

                {connectionResult && (
                  <div className="rounded-md border border-border/70 bg-card px-3 py-2 text-xs text-muted-foreground">
                    {t("aiConnectionLatency")}: {connectionResult.latency_ms}ms
                    {connectionResult.request_id
                      ? ` · request_id=${connectionResult.request_id}`
                      : ""}
                  </div>
                )}

                <div>
                  <label className="text-xs font-semibold text-muted-foreground">
                    {t("aiHeadersJson")}
                  </label>
                  <textarea
                    value={headersJson}
                    onChange={(event) => setHeadersJson(event.target.value)}
                    spellCheck={false}
                    className="mt-1 min-h-24 w-full rounded-md border border-border bg-background px-3 py-2 text-sm shadow-sm focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring"
                  />
                  <div className="mt-1 text-xs text-muted-foreground">
                    {t("aiHeadersHint")}
                  </div>
                </div>

                <div className="grid gap-2 md:grid-cols-2">
                  <label className="flex items-center gap-2 text-sm text-muted-foreground">
                    <Checkbox
                      checked={aiSettings.security_policy.destructive_action_confirmation}
                      onCheckedChange={(value) => {
                        const checked = value === true
                        setAiSettings((prev) =>
                          prev
                            ? {
                                ...prev,
                                security_policy: {
                                  ...prev.security_policy,
                                  destructive_action_confirmation: checked,
                                },
                              }
                            : prev
                        )
                      }}
                    />
                    <span>{t("aiDestructiveConfirm")}</span>
                  </label>
                  <label className="flex items-center gap-2 text-sm text-muted-foreground">
                    <Checkbox
                      checked={aiSettings.security_policy.mcp_remote_enabled}
                      onCheckedChange={(value) => {
                        const checked = value === true
                        setAiSettings((prev) =>
                          prev
                            ? {
                                ...prev,
                                security_policy: {
                                  ...prev.security_policy,
                                  mcp_remote_enabled: checked,
                                },
                              }
                            : prev
                        )
                      }}
                    />
                    <span>{t("aiMcpRemoteEnabled")}</span>
                  </label>
                  <label className="flex items-center gap-2 text-sm text-muted-foreground">
                    <Checkbox
                      checked={aiSettings.security_policy.mcp_audit_enabled}
                      onCheckedChange={(value) => {
                        const checked = value === true
                        setAiSettings((prev) =>
                          prev
                            ? {
                                ...prev,
                                security_policy: {
                                  ...prev.security_policy,
                                  mcp_audit_enabled: checked,
                                },
                              }
                            : prev
                        )
                      }}
                    />
                    <span>{t("aiMcpAuditEnabled")}</span>
                  </label>
                </div>

                <div className="grid gap-3 md:grid-cols-2">
                  <div>
                    <label className="text-xs font-semibold text-muted-foreground">
                      {t("aiMcpAuthTokenRef")}
                    </label>
                    <Input
                      className="mt-1"
                      value={aiSettings.security_policy.mcp_auth_token_ref}
                      onChange={(event) =>
                        setAiSettings((prev) =>
                          prev
                            ? {
                                ...prev,
                                security_policy: {
                                  ...prev.security_policy,
                                  mcp_auth_token_ref: event.target.value,
                                },
                              }
                            : prev
                        )
                      }
                    />
                  </div>
                  <div>
                    <label className="text-xs font-semibold text-muted-foreground">
                      {t("aiCliAllowlist")}
                    </label>
                    <textarea
                      value={cliAllowlistText}
                      onChange={(event) => setCliAllowlistText(event.target.value)}
                      spellCheck={false}
                      className="mt-1 min-h-24 w-full rounded-md border border-border bg-background px-3 py-2 text-sm shadow-sm focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring"
                    />
                  </div>
                  <div className="md:col-span-2">
                    <label className="text-xs font-semibold text-muted-foreground">
                      {t("aiMcpAllowedActions")}
                    </label>
                    <textarea
                      value={mcpAllowedActionsText}
                      onChange={(event) =>
                        setMcpAllowedActionsText(event.target.value)
                      }
                      spellCheck={false}
                      className="mt-1 min-h-24 w-full rounded-md border border-border bg-background px-3 py-2 text-sm shadow-sm focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring"
                    />
                  </div>
                </div>

                <div className="space-y-2 rounded-md border border-border/70 bg-card px-3 py-3">
                  <div className="text-xs font-semibold text-muted-foreground">
                    {t("aiSkills")}
                  </div>
                  <div className="text-xs text-muted-foreground">{t("aiSkillsDesc")}</div>
                  {(aiSettings.skills ?? []).length === 0 && (
                    <div className="text-xs text-muted-foreground">{t("aiSkillsEmpty")}</div>
                  )}
                  {(aiSettings.skills ?? []).map((skill) => (
                    <div
                      key={skill.id}
                      className="space-y-1 rounded-md border border-border/60 bg-background px-3 py-2"
                    >
                      <label className="flex items-center gap-2 text-sm text-foreground">
                        <Checkbox
                          checked={skill.enabled}
                          onCheckedChange={(value) =>
                            updateSkill(skill.id, (item) => ({
                              ...item,
                              enabled: value === true,
                            }))
                          }
                        />
                        <span className="font-medium">{skill.display_name}</span>
                        <Badge variant={skill.enabled ? "secondary" : "outline"}>
                          {skill.enabled ? t("aiSkillEnabled") : t("stopped")}
                        </Badge>
                      </label>
                      <div className="text-xs text-muted-foreground">
                        {t("aiSkillExecutor")}: {skill.executor} · {t("aiSkillTarget")}:{" "}
                        {skill.target}
                      </div>
                      <div className="text-xs text-muted-foreground">{skill.description}</div>
                    </div>
                  ))}
                  <div className="text-xs text-muted-foreground">
                    {t("aiSkillsPreviewHint")}
                  </div>
                </div>

                <div className="flex flex-wrap gap-2">
                  <Button
                    type="button"
                    variant="outline"
                    size="sm"
                    className={cardActionOutline}
                    onClick={handleAiValidateProfile}
                    disabled={aiValidating || aiSaving}
                  >
                    {aiValidating ? t("working") : t("aiValidateProfile")}
                  </Button>
                  <Button
                    type="button"
                    variant="secondary"
                    size="sm"
                    className={cardActionSecondary}
                    onClick={handleAiSaveSettings}
                    disabled={aiSaving || aiValidating}
                  >
                    {aiSaving ? t("working") : t("aiSaveSettings")}
                  </Button>
                </div>
              </div>
            )}

            {aiError && (
              <Alert variant="destructive">
                <div className="flex items-start gap-3">
                  <div className="mt-0.5 [&_svg]:size-4 [&_svg]:fill-none [&_svg]:stroke-current [&_svg]:stroke-2 [&_svg]:stroke-linecap-round [&_svg]:stroke-linejoin-round">
                    {I.alertCircle}
                  </div>
                  <div className="min-w-0 flex-1">
                    <AlertTitle>{t("aiSettings")}</AlertTitle>
                    <AlertDescription>
                      <p className="whitespace-pre-wrap">{aiError}</p>
                    </AlertDescription>
                  </div>
                </div>
              </Alert>
            )}

            {aiMessage && (
              <Alert className="border-border/70 bg-card">
                <div className="flex items-start gap-3">
                  <div className="mt-0.5 text-primary [&_svg]:size-4 [&_svg]:fill-none [&_svg]:stroke-current [&_svg]:stroke-2 [&_svg]:stroke-linecap-round [&_svg]:stroke-linejoin-round">
                    {I.check}
                  </div>
                  <div className="min-w-0 flex-1">
                    <AlertTitle>{t("aiSettings")}</AlertTitle>
                    <AlertDescription>
                      <p className="whitespace-pre-wrap">{aiMessage}</p>
                    </AlertDescription>
                  </div>
                </div>
              </Alert>
            )}
          </CardContent>
        </Card>
      </section>

      <section className="space-y-3">
        <div className="text-xs font-semibold tracking-widest text-muted-foreground">
          {sectionTitle("agentCliBridge")}
        </div>
        <Card className="py-0">
          <CardContent className="space-y-3 py-4">
            <div className="grid gap-3 md:grid-cols-2">
              <label className="flex items-center gap-2 text-sm text-muted-foreground">
                <Checkbox
                  checked={agentUseCustom}
                  onCheckedChange={(value) => setAgentUseCustom(value === true)}
                />
                <span>{t("agentCliUseCustom")}</span>
              </label>
              {!agentUseCustom && (
                <div>
                  <label className="text-xs font-semibold text-muted-foreground">
                    {t("agentCliPreset")}
                  </label>
                  <Select value={selectedPreset} onValueChange={setSelectedPreset}>
                    <SelectTrigger size="sm" className="mt-1 w-full justify-between">
                      <SelectValue />
                    </SelectTrigger>
                    <SelectContent align="end">
                      {agentPresets.map((preset) => (
                        <SelectItem key={preset.id} value={preset.id}>
                          {preset.name}
                        </SelectItem>
                      ))}
                    </SelectContent>
                  </Select>
                </div>
              )}
            </div>

            {agentUseCustom && (
              <div className="grid gap-3 md:grid-cols-2">
                <div>
                  <label className="text-xs font-semibold text-muted-foreground">
                    {t("agentCliCommand")}
                  </label>
                  <Input
                    className="mt-1"
                    value={agentCommand}
                    onChange={(event) => setAgentCommand(event.target.value)}
                    placeholder="codex"
                  />
                </div>
                <div>
                  <label className="text-xs font-semibold text-muted-foreground">
                    {t("agentCliArgs")}
                  </label>
                  <Input
                    className="mt-1"
                    value={agentArgsText}
                    onChange={(event) => setAgentArgsText(event.target.value)}
                    placeholder="exec --json"
                  />
                </div>
              </div>
            )}

            <div>
              <label className="text-xs font-semibold text-muted-foreground">
                {t("agentCliPrompt")}
              </label>
              <textarea
                value={agentPrompt}
                onChange={(event) => setAgentPrompt(event.target.value)}
                spellCheck={false}
                className="mt-1 min-h-24 w-full rounded-md border border-border bg-background px-3 py-2 text-sm shadow-sm focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring"
              />
            </div>

            <label className="flex items-center gap-2 text-sm text-muted-foreground">
              <Checkbox
                checked={agentDryRun}
                onCheckedChange={(value) => setAgentDryRun(value === true)}
              />
              <span>{t("agentCliDryRun")}</span>
            </label>

            <div className="flex gap-2">
              <Button
                type="button"
                variant="secondary"
                size="sm"
                className={cardActionSecondary}
                onClick={handleRunAgentCli}
                disabled={agentRunning}
              >
                {agentRunning ? t("working") : t("agentCliRun")}
              </Button>
            </div>

            {agentResult && (
              <div className="space-y-2 rounded-md border border-border/70 bg-card px-3 py-2 text-xs text-muted-foreground">
                <div>
                  <strong>{t("agentCliCommandLine")}:</strong> {agentResult.command_line}
                </div>
                <div>
                  <strong>{t("agentCliExitCode")}:</strong> {agentResult.exit_code} ·{" "}
                  <strong>{t("agentCliDuration")}:</strong> {agentResult.duration_ms}ms
                </div>
                <div>
                  <strong>stdout</strong>
                  <pre className="mt-1 max-h-40 overflow-auto whitespace-pre-wrap rounded bg-background p-2">
                    {agentResult.stdout || "-"}
                  </pre>
                </div>
                <div>
                  <strong>stderr</strong>
                  <pre className="mt-1 max-h-40 overflow-auto whitespace-pre-wrap rounded bg-background p-2">
                    {agentResult.stderr || "-"}
                  </pre>
                </div>
              </div>
            )}

            {agentError && (
              <Alert variant="destructive">
                <AlertTitle>{t("agentCliBridge")}</AlertTitle>
                <AlertDescription>
                  <p className="whitespace-pre-wrap">{agentError}</p>
                </AlertDescription>
              </Alert>
            )}
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
                  <Badge
                    variant="secondary"
                    className="ml-1 rounded-md px-1.5 py-0 text-[10px]"
                  >
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
                    <p className="whitespace-pre-wrap">{updateInfo.release_notes}</p>
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
