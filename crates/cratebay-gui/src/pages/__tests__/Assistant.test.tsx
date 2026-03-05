import { describe, it, expect, vi, beforeEach } from "vitest"
import { fireEvent, render, screen, waitFor } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { invoke } from "@tauri-apps/api/core"
import { Assistant } from "../Assistant"
import { messages } from "../../i18n/messages"
import type { AssistantPlanResult, AssistantStepExecutionResult } from "../../types"

const t = (key: string) => messages.en[key] || key

const mockPlan: AssistantPlanResult = {
  request_id: "ai-plan-1",
  strategy: "heuristic",
  notes: "test plan",
  fallback_used: true,
  steps: [
    {
      id: "step-1",
      title: "Stop container",
      command: "stop_container",
      args: { id: "abc123" },
      risk_level: "write",
      requires_confirmation: true,
      explain: "Stops a running container.",
    },
  ],
}

describe("Assistant", () => {
  beforeEach(() => {
    vi.clearAllMocks()
    vi.mocked(invoke).mockResolvedValue(null)
  })

  it("routes step execution through assistant_execute_step", async () => {
    const user = userEvent.setup()
    const confirmSpy = vi.spyOn(window, "confirm").mockReturnValue(true)
    vi.mocked(invoke).mockImplementation(async (command) => {
      if (command === "ai_generate_plan") return mockPlan
      if (command === "assistant_execute_step") {
        const out: AssistantStepExecutionResult = {
          ok: true,
          request_id: "ai-exec-1",
          command: "stop_container",
          risk_level: "write",
          output: { ok: true },
        }
        return out
      }
      return null
    })

    render(<Assistant t={t} />)
    await user.type(screen.getByPlaceholderText(t("assistantPromptPlaceholder")), "stop web")
    await user.click(screen.getByRole("button", { name: t("assistantGeneratePlan") }))
    await screen.findByText("Stop container")

    await user.click(screen.getByRole("button", { name: t("assistantRunStep") }))

    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith("assistant_execute_step", {
        command: "stop_container",
        args: { id: "abc123" },
        riskLevel: "write",
        requiresConfirmation: true,
        confirmed: true,
      })
    )
    expect(confirmSpy).toHaveBeenCalled()
    expect(screen.getByText(/request_id=ai-exec-1/)).toBeInTheDocument()
    expect(invoke).not.toHaveBeenCalledWith("stop_container", expect.anything())
  })

  it("does not execute when confirmation is denied", async () => {
    const user = userEvent.setup()
    vi.spyOn(window, "confirm").mockReturnValue(false)
    vi.mocked(invoke).mockImplementation(async (command) => {
      if (command === "ai_generate_plan") return mockPlan
      return null
    })

    render(<Assistant t={t} />)
    await user.type(screen.getByPlaceholderText(t("assistantPromptPlaceholder")), "stop web")
    await user.click(screen.getByRole("button", { name: t("assistantGeneratePlan") }))
    await screen.findByText("Stop container")
    await user.click(screen.getByRole("button", { name: t("assistantRunStep") }))

    expect(invoke).not.toHaveBeenCalledWith("assistant_execute_step", expect.anything())
  })

  it("shows invalid args error for malformed step JSON", async () => {
    const user = userEvent.setup()
    vi.mocked(invoke).mockImplementation(async (command) => {
      if (command === "ai_generate_plan") return mockPlan
      return null
    })

    render(<Assistant t={t} />)
    await user.type(screen.getByPlaceholderText(t("assistantPromptPlaceholder")), "stop web")
    await user.click(screen.getByRole("button", { name: t("assistantGeneratePlan") }))
    await screen.findByText("Stop container")

    const textareas = screen.getAllByRole("textbox")
    fireEvent.change(textareas[1], { target: { value: "{invalid json}" } })
    await user.click(screen.getByRole("button", { name: t("assistantRunStep") }))

    expect(screen.getByText(/Invalid args JSON/)).toBeInTheDocument()
    expect(invoke).not.toHaveBeenCalledWith("assistant_execute_step", expect.anything())
  })
})
