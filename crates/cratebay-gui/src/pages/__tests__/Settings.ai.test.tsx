import { describe, it, expect, vi, beforeEach } from "vitest"
import { render, screen, waitFor } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { invoke } from "@tauri-apps/api/core"
import { Settings } from "../Settings"
import { messages } from "../../i18n/messages"
import type { AgentCliRunResult, AiConnectionTestResult, AiSettings, Theme } from "../../types"

const t = (key: string) => messages.en[key] || key

const baseAiSettings: AiSettings = {
  active_profile_id: "openai-default",
  profiles: [
    {
      id: "openai-default",
      provider_id: "openai",
      display_name: "OpenAI",
      model: "gpt-4o-mini",
      base_url: "https://api.openai.com/v1",
      api_key_ref: "cratebay.ai.openai.default",
      headers: {},
    },
  ],
  skills: [
    {
      id: "agent-cli-openclaw-plan",
      display_name: "OpenClaw CLI Plan",
      description: "Invoke openclaw preset for planning",
      tags: ["agent-cli", "openclaw"],
      executor: "agent_cli_preset",
      target: "openclaw",
      input_schema: {
        type: "object",
        properties: { prompt: { type: "string" } },
      },
      enabled: true,
    },
  ],
  security_policy: {
    destructive_action_confirmation: true,
    mcp_remote_enabled: false,
    mcp_allowed_actions: ["list_containers"],
    mcp_auth_token_ref: "",
    mcp_audit_enabled: true,
    cli_command_allowlist: ["codex", "openclaw"],
  },
}

const defaultProps = {
  theme: "dark" as Theme,
  setTheme: vi.fn(),
  lang: "en",
  setLang: vi.fn(),
  t,
}

describe("Settings AI section", () => {
  beforeEach(() => {
    vi.clearAllMocks()
    vi.mocked(invoke).mockResolvedValue(null)
  })

  it("tests AI connection with active profile id", async () => {
    const user = userEvent.setup()
    vi.mocked(invoke).mockImplementation(async (command) => {
      if (command === "load_ai_settings") return baseAiSettings
      if (command === "agent_cli_list_presets") {
        return [
          {
            id: "codex",
            name: "OpenAI Codex CLI",
            description: "Run codex in non-interactive mode",
            command: "codex",
            args_template: ["exec", "{{prompt}}"],
            timeout_sec: 180,
            dangerous: false,
          },
        ]
      }
      if (command === "ai_secret_exists") return true
      if (command === "ai_test_connection") {
        const out: AiConnectionTestResult = {
          ok: true,
          request_id: "ai-test-1",
          message: "Connection succeeded: PONG",
          latency_ms: 123,
        }
        return out
      }
      return null
    })

    render(<Settings {...defaultProps} />)
    await screen.findByRole("button", { name: t("aiTestConnection") })

    await user.click(screen.getByRole("button", { name: t("aiTestConnection") }))

    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith("ai_test_connection", {
        profileId: "openai-default",
      })
    )
    expect(screen.getByText(/Connection succeeded: PONG/)).toBeInTheDocument()
    expect(screen.getByText(/request_id=ai-test-1/)).toBeInTheDocument()
  })

  it("runs Agent CLI dry-run preset from settings", async () => {
    const user = userEvent.setup()
    vi.mocked(invoke).mockImplementation(async (command) => {
      if (command === "load_ai_settings") return baseAiSettings
      if (command === "agent_cli_list_presets") {
        return [
          {
            id: "openclaw",
            name: "OpenClaw CLI",
            description: "Invoke openclaw cli prompt mode",
            command: "openclaw",
            args_template: ["run", "--prompt", "{{prompt}}"],
            timeout_sec: 180,
            dangerous: false,
          },
        ]
      }
      if (command === "ai_secret_exists") return true
      if (command === "agent_cli_run") {
        const out: AgentCliRunResult = {
          ok: true,
          request_id: "ai-cli-1",
          command_line: "openclaw run --prompt",
          exit_code: 0,
          stdout: "",
          stderr: "",
          duration_ms: 0,
        }
        return out
      }
      return null
    })

    render(<Settings {...defaultProps} />)
    await screen.findByRole("button", { name: t("agentCliRun") })

    await user.click(screen.getByRole("button", { name: t("agentCliRun") }))

    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith("agent_cli_run", {
        presetId: "openclaw",
        command: null,
        args: null,
        prompt: "",
        dryRun: true,
      })
    )
    expect(screen.getByText(/openclaw run --prompt/)).toBeInTheDocument()
  })

  it("toggles skill scaffold and saves settings", async () => {
    const user = userEvent.setup()
    vi.mocked(invoke).mockImplementation(async (command, args) => {
      if (command === "load_ai_settings") return baseAiSettings
      if (command === "agent_cli_list_presets") return []
      if (command === "ai_secret_exists") return true
      if (command === "save_ai_settings") return (args as { settings: AiSettings }).settings
      return null
    })

    render(<Settings {...defaultProps} />)
    const skillLabel = await screen.findByText(/OpenClaw CLI Plan/)
    await user.click(skillLabel)

    await user.click(screen.getByRole("button", { name: t("aiSaveSettings") }))

    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith(
        "save_ai_settings",
        expect.objectContaining({
          settings: expect.objectContaining({
            skills: expect.arrayContaining([
              expect.objectContaining({
                id: "agent-cli-openclaw-plan",
                enabled: false,
              }),
            ]),
          }),
        })
      )
    )
  })
})
