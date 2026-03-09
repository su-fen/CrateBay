import { describe, it, expect, vi, beforeEach } from "vitest"
import { render, screen, waitFor, within } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { invoke } from "@tauri-apps/api/core"
import { AiHub } from "../AiHub"
import { messages } from "../../i18n/messages"
import type {
  AiHubActionResultDto,
  GpuStatusDto,
  AiSettings,
  McpServerEntry,
  McpServerStatusDto,
  OllamaModelDto,
  OllamaStatusDto,
  OllamaStorageInfoDto,
  SandboxAuditEventDto,
  SandboxCleanupResultDto,
  SandboxInfoDto,
  SandboxRuntimeUsageDto,
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

const baseGpuStatus: GpuStatusDto = {
  available: true,
  utilization_supported: true,
  backend: "nvidia-smi",
  message: "Live GPU telemetry is available for 1 device(s).",
  devices: [
    {
      index: 0,
      name: "NVIDIA RTX 4090",
      utilization_percent: 62,
      memory_used_bytes: 6442450944,
      memory_total_bytes: 25769803776,
      memory_used_human: "6.0 GB",
      memory_total_human: "24.0 GB",
      temperature_celsius: 58,
      power_watts: 210.5,
    },
  ],
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

const baseSandboxRuntimeUsage: SandboxRuntimeUsageDto = {
  running: true,
  cpu_percent: 18.5,
  memory_usage_mb: 256,
  memory_limit_mb: 1024,
  memory_percent: 25,
  network_rx_bytes: 1024,
  network_tx_bytes: 2048,
  gpu_attribution_supported: true,
  gpu_message: "Matched 1 GPU process(es) across 1 device(s).",
  gpu_processes: [
    {
      gpu_index: 0,
      gpu_name: "NVIDIA RTX 4090",
      pid: 4242,
      process_name: "python",
      memory_used_bytes: 2147483648,
      memory_used_human: "2.0 GB",
    },
  ],
  gpu_memory_used_bytes: 2147483648,
  gpu_memory_used_human: "2.0 GB",
}

const baseMcpStatuses: McpServerStatusDto[] = []

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
})

describe("AiHub", () => {
  beforeEach(() => {
    vi.clearAllMocks()
    vi.mocked(invoke).mockResolvedValue(null)
  })

  it("shows GPU telemetry in the models runtime panel", async () => {
    const user = userEvent.setup()

    vi.mocked(invoke).mockImplementation(async (command) => {
      if (command === "ollama_status") return baseOllamaStatus
      if (command === "gpu_status") return baseGpuStatus
      if (command === "ollama_storage_info") return baseOllamaStorage
      if (command === "ollama_list_models") return [] as OllamaModelDto[]
      if (command === "sandbox_templates") return [baseTemplate]
      if (command === "sandbox_list") return [] as SandboxInfoDto[]
      if (command === "sandbox_audit_list") return [] as SandboxAuditEventDto[]
      if (command === "load_ai_settings") return buildAiSettings([])
      if (command === "mcp_list_servers") return [] as McpServerStatusDto[]
      if (command === "mcp_server_logs") return [] as string[]
      return null
    })

    render(<AiHub t={t} />)
    await user.click(screen.getByRole("tab", { name: t("models") }))

    const gpuCard = await screen.findByTestId("ollama-gpu-card")
    expect(within(gpuCard).getByText(t("gpuRuntime"))).toBeInTheDocument()
    expect(within(gpuCard).getByText("nvidia-smi")).toBeInTheDocument()
    expect(within(gpuCard).getByText("NVIDIA RTX 4090")).toBeInTheDocument()
    expect(within(gpuCard).getByText(/62%/)).toBeInTheDocument()
    expect(within(gpuCard).getByText(/6.0 GB \/ 24.0 GB/)).toBeInTheDocument()
  })

  it("respects initial tab deep links", async () => {
    vi.mocked(invoke).mockImplementation(async (command) => {
      if (command === "ollama_status") return { ...baseOllamaStatus, running: false }
      if (command === "gpu_status") return baseGpuStatus
      if (command === "ollama_storage_info") return baseOllamaStorage
      if (command === "sandbox_templates") return [baseTemplate]
      if (command === "sandbox_list") return [] as SandboxInfoDto[]
      if (command === "sandbox_audit_list") return [] as SandboxAuditEventDto[]
      if (command === "load_ai_settings") return buildAiSettings([])
      if (command === "mcp_list_servers") return baseMcpStatuses
      if (command === "mcp_server_logs") return [] as string[]
      return null
    })

    render(<AiHub t={t} initialTab="mcp" />)

    expect(await screen.findByText(t("mcpRegistryTitle"))).toBeInTheDocument()
    expect(screen.getByRole("tab", { name: t("mcp") })).toHaveAttribute("data-state", "active")
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
      if (command === "gpu_status") return baseGpuStatus
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
      if (command === "gpu_status") return baseGpuStatus
      if (command === "ollama_storage_info") return baseOllamaStorage
      if (command === "sandbox_templates") return [baseTemplate]
      if (command === "sandbox_list") return [] as SandboxInfoDto[]
      if (command === "sandbox_audit_list") return [] as SandboxAuditEventDto[]
      if (command === "load_ai_settings") return buildAiSettings(registry)
      if (command === "mcp_list_servers") return statuses
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

  it("shows sandbox GPU runtime usage and cleanup entry", async () => {
    const user = userEvent.setup()
    vi.spyOn(window, "confirm").mockReturnValue(true)

    let sandboxes: SandboxInfoDto[] = [
      {
        id: "sandbox-gpu",
        short_id: "sandbox-gpu",
        name: "GPU Sandbox",
        image: "python:3.12",
        state: "running",
        status: "Up 2 minutes",
        template_id: baseTemplate.id,
        owner: "test",
        created_at: "2026-03-06T00:00:00Z",
        expires_at: "2026-03-06T08:00:00Z",
        ttl_hours: 8,
        cpu_cores: 2,
        memory_mb: 2048,
        is_expired: false,
      },
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

    vi.mocked(invoke).mockImplementation(async (command, args) => {
      if (command === "ollama_status") return { ...baseOllamaStatus, running: false }
      if (command === "gpu_status") return baseGpuStatus
      if (command === "ollama_storage_info") return baseOllamaStorage
      if (command === "sandbox_templates") return [baseTemplate]
      if (command === "sandbox_list") return sandboxes
      if (command === "sandbox_audit_list") return [] as SandboxAuditEventDto[]
      if (command === "load_ai_settings") return buildAiSettings([])
      if (command === "mcp_list_servers") return [] as McpServerStatusDto[]
      if (command === "mcp_server_logs") return [] as string[]
      if (command === "sandbox_inspect") {
        const { id } = args as { id: string }
        const sandbox = sandboxes.find((item) => item.id === id)
        if (!sandbox) return null
        return {
          id: sandbox.id,
          short_id: sandbox.short_id,
          name: sandbox.name,
          image: sandbox.image,
          template_id: sandbox.template_id,
          owner: sandbox.owner,
          created_at: sandbox.created_at,
          expires_at: sandbox.expires_at,
          ttl_hours: sandbox.ttl_hours,
          cpu_cores: sandbox.cpu_cores,
          memory_mb: sandbox.memory_mb,
          running: sandbox.state === "running",
          command: "python server.py",
          env: ["CUDA_VISIBLE_DEVICES=0"],
        }
      }
      if (command === "sandbox_runtime_usage") return baseSandboxRuntimeUsage
      if (command === "sandbox_cleanup_expired") {
        sandboxes = sandboxes.filter((item) => !item.is_expired)
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

    const sandboxRow = screen.getByText("GPU Sandbox").closest("tr")
    expect(sandboxRow).not.toBeNull()
    await user.click(within(sandboxRow as HTMLElement).getByRole("button", { name: t("inspect") }))

    const runtimeCard = await screen.findByTestId("sandbox-runtime-card")
    expect(within(runtimeCard).getByText(t("sandboxRuntimeUsage"))).toBeInTheDocument()
    expect(within(runtimeCard).getByText(/18.5%/)).toBeInTheDocument()
    expect(within(runtimeCard).getByText(/256 MB \/ 1024 MB \(25%\)/)).toBeInTheDocument()
    expect(within(runtimeCard).getAllByText(/2.0 GB/).length).toBeGreaterThan(0)
    expect(within(runtimeCard).getByText(/python · GPU 0 · 2.0 GB/)).toBeInTheDocument()

    await user.click(screen.getByRole("button", { name: t("sandboxCleanupExpired") }))

    await waitFor(() => expect(invoke).toHaveBeenCalledWith("sandbox_cleanup_expired"))
    expect(await screen.findByText("Removed 1 expired sandbox")).toBeInTheDocument()
    await waitFor(() => expect(screen.queryByText("Expired Sandbox")).not.toBeInTheDocument())
  })
})
