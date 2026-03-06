import { describe, it, expect, vi, beforeEach } from "vitest"
import { render, screen, waitFor, within } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { invoke } from "@tauri-apps/api/core"
import { AiHub } from "../AiHub"
import { messages } from "../../i18n/messages"
import type {
  AiHubActionResultDto,
  AiSettings,
  McpServerEntry,
  McpServerStatusDto,
  OllamaModelDto,
  OllamaStatusDto,
  OllamaStorageInfoDto,
  OpenSandboxStatusDto,
  SandboxAuditEventDto,
  SandboxCleanupResultDto,
  SandboxInfoDto,
  SandboxTemplateDto,
} from "../../types"

const t = (key: string) => messages.en[key] || key

class ResizeObserverMock {
  observe() {}
  unobserve() {}
  disconnect() {}
}

vi.stubGlobal("ResizeObserver", ResizeObserverMock)

const baseOllamaStatus: OllamaStatusDto = {
  installed: true,
  running: true,
  version: "0.6.2",
  base_url: "http://127.0.0.1:11434",
}

const baseOllamaStorage: OllamaStorageInfoDto = {
  path: "/Users/test/.ollama/models",
  exists: true,
  model_count: 1,
  total_size_bytes: 7516192768,
  total_size_human: "7.0 GB",
}

const baseTemplate: SandboxTemplateDto = {
  id: "node-dev",
  name: "Node Dev",
  description: "Node sandbox",
  image: "node:22",
  default_command: "sleep infinity",
  cpu_default: 2,
  memory_mb_default: 2048,
  ttl_hours_default: 8,
  tags: ["node"],
}

const baseOpenSandboxStatus: OpenSandboxStatusDto = {
  installed: true,
  enabled: true,
  configured: true,
  reachable: true,
  base_url: "http://127.0.0.1:8080",
  config_path: "/Users/test/.cratebay/opensandbox.toml",
}

const buildAiSettings = (servers: McpServerEntry[]): AiSettings => ({
  active_profile_id: "",
  profiles: [],
  skills: [],
  security_policy: {
    destructive_action_confirmation: true,
    mcp_remote_enabled: false,
    mcp_allowed_actions: [],
    mcp_auth_token_ref: "",
    mcp_audit_enabled: true,
    cli_command_allowlist: ["codex"],
  },
  mcp_servers: servers,
  opensandbox: {
    enabled: true,
    base_url: baseOpenSandboxStatus.base_url,
    config_path: baseOpenSandboxStatus.config_path,
  },
})

describe("AiHub", () => {
  beforeEach(() => {
    vi.clearAllMocks()
    vi.mocked(invoke).mockResolvedValue(null)
  })

  it("supports Ollama pull and delete flows", async () => {
    const user = userEvent.setup()
    const confirmSpy = vi.spyOn(window, "confirm").mockReturnValue(true)

    const models: OllamaModelDto[] = [
      {
        name: "qwen2.5:7b",
        size_bytes: 7516192768,
        size_human: "7.0 GB",
        modified_at: "2026-03-06T01:00:00Z",
        digest: "sha256:qwen",
        family: "qwen2.5",
        parameter_size: "7B",
        quantization_level: "Q4_K_M",
      },
    ]

    vi.mocked(invoke).mockImplementation(async (command, args) => {
      if (command === "ollama_status") return baseOllamaStatus
      if (command === "ollama_storage_info") {
        return { ...baseOllamaStorage, model_count: models.length }
      }
      if (command === "ollama_list_models") return [...models]
      if (command === "ollama_pull_model") {
        const { name } = args as { name: string }
        models.push({
          name,
          size_bytes: 2147483648,
          size_human: "2.0 GB",
          modified_at: "2026-03-06T02:00:00Z",
          digest: `sha256:${name}`,
          family: "llama3.2",
          parameter_size: "3B",
          quantization_level: "Q4_K_M",
        })
        const out: AiHubActionResultDto = { ok: true, message: "Model pulled" }
        return out
      }
      if (command === "ollama_delete_model") {
        const { name } = args as { name: string }
        const index = models.findIndex((item) => item.name === name)
        if (index >= 0) models.splice(index, 1)
        const out: AiHubActionResultDto = { ok: true, message: "Model deleted" }
        return out
      }
      if (command === "sandbox_templates") return [baseTemplate]
      if (command === "sandbox_list") return [] as SandboxInfoDto[]
      if (command === "sandbox_audit_list") return [] as SandboxAuditEventDto[]
      if (command === "load_ai_settings") return buildAiSettings([])
      if (command === "mcp_list_servers") return [] as McpServerStatusDto[]
      if (command === "opensandbox_status") return baseOpenSandboxStatus
      if (command === "mcp_server_logs") return [] as string[]
      return null
    })

    render(<AiHub t={t} />)
    await user.click(screen.getByRole("tab", { name: t("models") }))

    expect(await screen.findByText(baseOllamaStorage.path)).toBeInTheDocument()

    await user.type(screen.getByPlaceholderText(t("ollamaPullPlaceholder")), "llama3.2:3b")
    await user.click(screen.getByRole("button", { name: t("ollamaPullAction") }))

    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith("ollama_pull_model", { name: "llama3.2:3b" })
    )
    expect(await screen.findByText("llama3.2:3b")).toBeInTheDocument()

    const modelRow = screen.getByText("llama3.2:3b").closest("tr")
    expect(modelRow).not.toBeNull()
    await user.click(within(modelRow as HTMLElement).getByRole("button", { name: t("ollamaDeleteAction") }))

    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith("ollama_delete_model", { name: "llama3.2:3b" })
    )
    expect(confirmSpy).toHaveBeenCalledWith(t("confirmDeleteModel"))
    await waitFor(() => expect(screen.queryByText("llama3.2:3b")).not.toBeInTheDocument())
  })

  it("manages MCP registry, runtime, logs, and export", async () => {
    const user = userEvent.setup()

    let registry: McpServerEntry[] = [
      {
        id: "local-mcp-1",
        name: "Filesystem MCP",
        command: "npx",
        args: ["-y", "@modelcontextprotocol/server-filesystem", "/tmp"],
        env: ["DEBUG=1"],
        working_dir: "/tmp",
        enabled: true,
        notes: "local dev",
      },
    ]

    let statuses: McpServerStatusDto[] = [
      {
        ...registry[0],
        running: false,
        status: "stopped",
        started_at: "",
        exit_code: null,
      },
    ]

    const logsById: Record<string, string[]> = {
      "local-mcp-1": ["filesystem server ready"],
      "local-mcp-renamed": ["renamed server ready"],
    }

    vi.mocked(invoke).mockImplementation(async (command, args) => {
      if (command === "ollama_status") return { ...baseOllamaStatus, running: false }
      if (command === "ollama_storage_info") return baseOllamaStorage
      if (command === "sandbox_templates") return [baseTemplate]
      if (command === "sandbox_list") return [] as SandboxInfoDto[]
      if (command === "sandbox_audit_list") return [] as SandboxAuditEventDto[]
      if (command === "load_ai_settings") return buildAiSettings(registry)
      if (command === "mcp_list_servers") return statuses
      if (command === "opensandbox_status") return baseOpenSandboxStatus
      if (command === "mcp_server_logs") {
        const { id } = args as { id: string }
        return logsById[id] ?? []
      }
      if (command === "mcp_save_servers") {
        const { servers } = args as { servers: McpServerEntry[] }
        const previousId = registry[0]?.id
        registry = servers
        statuses = servers.map((server) => ({
          ...server,
          running: false,
          status: "stopped",
          started_at: "",
          exit_code: null,
        }))
        if (previousId && registry[0]?.id && logsById[previousId]) {
          logsById[registry[0].id] = logsById[previousId]
        }
        return registry
      }
      if (command === "mcp_start_server") {
        const { id } = args as { id: string }
        statuses = statuses.map((server) =>
          server.id === id
            ? { ...server, running: true, status: "running", started_at: "2026-03-06T03:00:00Z" }
            : server
        )
        const out: AiHubActionResultDto = { ok: true, message: "started" }
        return out
      }
      if (command === "mcp_export_client_config") {
        return JSON.stringify(
          {
            client: (args as { client: string }).client,
            servers: registry.map((server) => server.id),
          },
          null,
          2
        )
      }
      return null
    })

    render(<AiHub t={t} />)
    await user.click(screen.getByRole("tab", { name: t("mcp") }))

    expect(await screen.findByText("Filesystem MCP")).toBeInTheDocument()
    expect(await screen.findByText(/filesystem server ready/)).toBeInTheDocument()

    const serverIdInput = screen.getByDisplayValue("local-mcp-1")
    await user.clear(serverIdInput)
    await user.type(serverIdInput, "local-mcp-renamed")

    const commandInput = screen.getByDisplayValue("npx")
    await user.clear(commandInput)
    await user.type(commandInput, "uvx")

    await user.click(screen.getByRole("button", { name: t("mcpSaveRegistry") }))

    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith(
        "mcp_save_servers",
        expect.objectContaining({
          servers: expect.arrayContaining([
            expect.objectContaining({
              id: "local-mcp-renamed",
              command: "uvx",
            }),
          ]),
        })
      )
    )

    expect(await screen.findByDisplayValue("local-mcp-renamed")).toBeInTheDocument()

    const renamedRow = screen.getByText("local-mcp-renamed").closest("tr")
    expect(renamedRow).not.toBeNull()
    await user.click(within(renamedRow as HTMLElement).getByRole("button", { name: t("start") }))

    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith("mcp_start_server", { id: "local-mcp-renamed" })
    )
    expect((await screen.findAllByText(t("mcpRunning"))).length).toBeGreaterThan(0)

    await user.click(screen.getByRole("button", { name: t("mcpExportConfig") }))
    expect((await screen.findAllByDisplayValue(/local-mcp-renamed/)).length).toBeGreaterThan(0)
    await waitFor(() => expect(screen.getByText(/filesystem server ready/)).toBeInTheDocument())
  })

  it("shows OpenSandbox status and cleanup entry", async () => {
    const user = userEvent.setup()

    let sandboxes: SandboxInfoDto[] = [
      {
        id: "sandbox-1",
        short_id: "sandbox-1",
        name: "Expired Sandbox",
        image: "node:22",
        state: "exited",
        status: "Exited",
        template_id: baseTemplate.id,
        owner: "test",
        created_at: "2026-03-05T00:00:00Z",
        expires_at: "2026-03-05T08:00:00Z",
        ttl_hours: 8,
        cpu_cores: 2,
        memory_mb: 2048,
        is_expired: true,
      },
    ]

    vi.mocked(invoke).mockImplementation(async (command) => {
      if (command === "ollama_status") return { ...baseOllamaStatus, running: false }
      if (command === "ollama_storage_info") return baseOllamaStorage
      if (command === "sandbox_templates") return [baseTemplate]
      if (command === "sandbox_list") return sandboxes
      if (command === "sandbox_audit_list") return [] as SandboxAuditEventDto[]
      if (command === "load_ai_settings") return buildAiSettings([])
      if (command === "mcp_list_servers") return [] as McpServerStatusDto[]
      if (command === "opensandbox_status") return baseOpenSandboxStatus
      if (command === "mcp_server_logs") return [] as string[]
      if (command === "sandbox_cleanup_expired") {
        sandboxes = []
        const out: SandboxCleanupResultDto = {
          removed_count: 1,
          removed_names: ["Expired Sandbox"],
          message: "Removed 1 expired sandbox",
        }
        return out
      }
      return null
    })

    render(<AiHub t={t} />)
    await user.click(screen.getByRole("tab", { name: t("sandboxes") }))

    expect(await screen.findByText(baseOpenSandboxStatus.base_url)).toBeInTheDocument()
    expect(screen.getByText(baseOpenSandboxStatus.config_path)).toBeInTheDocument()

    await user.click(screen.getByRole("button", { name: t("sandboxCleanupExpired") }))

    await waitFor(() => expect(invoke).toHaveBeenCalledWith("sandbox_cleanup_expired"))
    expect(await screen.findByText("Removed 1 expired sandbox")).toBeInTheDocument()
    await waitFor(() => expect(screen.queryByText("Expired Sandbox")).not.toBeInTheDocument())
  })
})
