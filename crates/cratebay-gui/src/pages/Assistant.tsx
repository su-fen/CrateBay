import { useMemo, useState } from "react"
import { invoke } from "@tauri-apps/api/core"
import { I } from "../icons"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import { Card, CardContent } from "@/components/ui/card"
import type {
  AssistantPlanResult,
  AssistantPlanStep,
  AssistantStepExecutionResult,
} from "../types"

interface AssistantProps {
  t: (key: string) => string
}

export function Assistant({ t }: AssistantProps) {
  const [prompt, setPrompt] = useState("")
  const [plan, setPlan] = useState<AssistantPlanResult | null>(null)
  const [planError, setPlanError] = useState("")
  const [generating, setGenerating] = useState(false)
  const [executingStepId, setExecutingStepId] = useState("")
  const [stepArgsMap, setStepArgsMap] = useState<Record<string, string>>({})
  const [stepResultMap, setStepResultMap] = useState<Record<string, string>>({})

  const canGenerate = useMemo(() => prompt.trim().length > 0 && !generating, [prompt, generating])

  const handleGeneratePlan = async () => {
    if (!prompt.trim()) return
    setGenerating(true)
    setPlanError("")
    setPlan(null)
    setStepResultMap({})
    try {
      const result = await invoke<AssistantPlanResult>("ai_generate_plan", {
        prompt: prompt.trim(),
        preferModel: true,
      })
      setPlan(result)
      const nextArgs: Record<string, string> = {}
      for (const step of result.steps) {
        nextArgs[step.id] = JSON.stringify(step.args ?? {}, null, 2)
      }
      setStepArgsMap(nextArgs)
    } catch (e) {
      setPlanError(String(e))
    } finally {
      setGenerating(false)
    }
  }

  const runStep = async (step: AssistantPlanStep) => {
    const rawArgs = stepArgsMap[step.id] ?? "{}"
    let argsObj: Record<string, unknown> = {}
    try {
      const parsed = JSON.parse(rawArgs) as unknown
      if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) {
        throw new Error("args must be an object")
      }
      argsObj = parsed as Record<string, unknown>
    } catch (e) {
      setStepResultMap((prev) => ({
        ...prev,
        [step.id]: `Invalid args JSON: ${String(e)}`,
      }))
      return
    }

    const confirmed = step.requires_confirmation
      ? window.confirm(`${t("assistantConfirmAction")}\n${step.title}\n${step.command}`)
      : true
    if (!confirmed) return

    setExecutingStepId(step.id)
    setStepResultMap((prev) => ({ ...prev, [step.id]: t("working") }))
    try {
      const result = await invoke<AssistantStepExecutionResult>("assistant_execute_step", {
        command: step.command,
        args: argsObj,
        riskLevel: step.risk_level,
        requiresConfirmation: step.requires_confirmation,
        confirmed,
      })
      const output =
        typeof result.output === "string"
          ? result.output
          : JSON.stringify(result.output, null, 2) || t("done")
      setStepResultMap((prev) => ({
        ...prev,
        [step.id]: result.request_id ? `${output}\nrequest_id=${result.request_id}` : output,
      }))
    } catch (e) {
      setStepResultMap((prev) => ({
        ...prev,
        [step.id]: String(e),
      }))
    } finally {
      setExecutingStepId("")
    }
  }

  return (
    <div className="space-y-4">
      <Card className="py-0">
        <CardContent className="space-y-3 py-4">
          <div className="flex items-center gap-3">
            <div className="size-10 shrink-0 rounded-lg bg-primary/10 text-primary flex items-center justify-center [&_svg]:size-5 [&_svg]:fill-none [&_svg]:stroke-current [&_svg]:stroke-2 [&_svg]:stroke-linecap-round [&_svg]:stroke-linejoin-round">
              {I.command}
            </div>
            <div className="min-w-0 flex-1">
              <div className="text-sm font-semibold text-foreground">{t("assistant")}</div>
              <div className="text-xs text-muted-foreground">{t("assistantDesc")}</div>
            </div>
          </div>

          <textarea
            value={prompt}
            onChange={(event) => setPrompt(event.target.value)}
            spellCheck={false}
            className="min-h-24 w-full rounded-md border border-border bg-background px-3 py-2 text-sm shadow-sm focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring"
            placeholder={t("assistantPromptPlaceholder")}
          />

          <div className="flex items-center gap-2">
            <Button
              type="button"
              variant="secondary"
              size="sm"
              onClick={handleGeneratePlan}
              disabled={!canGenerate}
            >
              {generating ? t("working") : t("assistantGeneratePlan")}
            </Button>
          </div>
        </CardContent>
      </Card>

      {planError && (
        <Alert variant="destructive">
          <AlertTitle>{t("assistant")}</AlertTitle>
          <AlertDescription>
            <p className="whitespace-pre-wrap">{planError}</p>
          </AlertDescription>
        </Alert>
      )}

      {plan && (
        <Card className="py-0">
          <CardContent className="space-y-4 py-4">
            <div className="text-sm text-muted-foreground">
              <strong>{t("assistantStrategy")}:</strong> {plan.strategy}
              {plan.fallback_used ? ` · ${t("assistantFallbackUsed")}` : ""}
              {plan.request_id ? ` · request_id=${plan.request_id}` : ""}
            </div>
            <div className="text-sm text-foreground">{plan.notes}</div>

            <div className="space-y-3">
              {plan.steps.map((step) => (
                <div
                  key={step.id}
                  className="rounded-md border border-border/70 bg-card px-3 py-3"
                >
                  <div className="flex flex-wrap items-center gap-2">
                    <div className="text-sm font-semibold text-foreground">{step.title}</div>
                    <span className="rounded bg-muted px-1.5 py-0.5 text-[10px] text-muted-foreground">
                      {step.command}
                    </span>
                    <span className="rounded bg-muted px-1.5 py-0.5 text-[10px] text-muted-foreground">
                      {step.risk_level}
                    </span>
                    {step.requires_confirmation && (
                      <span className="rounded bg-amber-100 px-1.5 py-0.5 text-[10px] text-amber-700 dark:bg-amber-950 dark:text-amber-300">
                        {t("assistantNeedConfirm")}
                      </span>
                    )}
                  </div>
                  <div className="mt-1 text-xs text-muted-foreground">{step.explain}</div>

                  <div className="mt-2">
                    <label className="text-xs font-semibold text-muted-foreground">
                      {t("assistantStepArgs")}
                    </label>
                    <textarea
                      value={stepArgsMap[step.id] ?? "{}"}
                      onChange={(event) =>
                        setStepArgsMap((prev) => ({
                          ...prev,
                          [step.id]: event.target.value,
                        }))
                      }
                      spellCheck={false}
                      className="mt-1 min-h-24 w-full rounded-md border border-border bg-background px-3 py-2 text-sm shadow-sm focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring"
                    />
                  </div>

                  <div className="mt-2 flex items-center gap-2">
                    <Button
                      type="button"
                      variant="outline"
                      size="sm"
                      onClick={() => runStep(step)}
                      disabled={executingStepId === step.id}
                    >
                      {executingStepId === step.id ? t("working") : t("assistantRunStep")}
                    </Button>
                  </div>

                  {stepResultMap[step.id] && (
                    <pre className="mt-2 max-h-40 overflow-auto whitespace-pre-wrap rounded-md bg-background px-2 py-2 text-xs text-muted-foreground">
                      {stepResultMap[step.id]}
                    </pre>
                  )}
                </div>
              ))}
            </div>
          </CardContent>
        </Card>
      )}
    </div>
  )
}
