import { describe, it, expect, vi, beforeEach } from "vitest"
import { render, screen } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { invoke } from "@tauri-apps/api/core"
import { Containers } from "../Containers"
import { messages } from "../../i18n/messages"
import type { ContainerInfo, ContainerGroup, RunContainerResult } from "../../types"

const t = (key: string) => messages.en[key] || key

const mockContainer = (overrides?: Partial<ContainerInfo>): ContainerInfo => ({
  id: "abc123",
  name: "web-server",
  image: "nginx:latest",
  state: "running",
  status: "Up 2 hours",
  ports: "0.0.0.0:80->80/tcp",
  ...overrides,
})

const makeSingleGroup = (c: ContainerInfo): ContainerGroup => ({
  key: c.name || c.id,
  containers: [c],
  runningCount: c.state === "running" ? 1 : 0,
  stoppedCount: c.state === "running" ? 0 : 1,
})

const defaultProps = {
  containers: [] as ContainerInfo[],
  groups: [] as ContainerGroup[],
  loading: false,
  error: "",
  acting: "",
  expandedGroups: {} as Record<string, boolean>,
  onContainerAction: vi.fn(),
  onToggleGroup: vi.fn(),
  onOpenTextModal: vi.fn(),
  onOpenPackageModal: vi.fn(),
  onFetch: vi.fn(),
  onRun: vi.fn().mockResolvedValue(null),
  t,
}

describe("Containers", () => {
  beforeEach(() => {
    vi.clearAllMocks()
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "container_stats") {
        return {
          cpu_percent: 5.0,
          memory_usage_mb: 64,
          memory_limit_mb: 256,
          memory_percent: 25,
          network_rx_bytes: 0,
          network_tx_bytes: 0,
        }
      }
      return null
    })
  })

  it("shows loading state", () => {
    render(<Containers {...defaultProps} loading={true} />)

    expect(screen.getByText(t("loadingContainers"))).toBeInTheDocument()
  })

  it("shows error state with refresh button", async () => {
    const user = userEvent.setup()
    const onFetch = vi.fn()
    render(
      <Containers {...defaultProps} error="Connection refused" onFetch={onFetch} />
    )

    expect(screen.getByText(t("connectionError"))).toBeInTheDocument()
    expect(screen.getByText("Connection refused")).toBeInTheDocument()

    const refreshBtn = screen.getByText(t("refresh"))
    await user.click(refreshBtn)
    expect(onFetch).toHaveBeenCalled()
  })

  it("shows empty state when no containers exist", () => {
    render(<Containers {...defaultProps} />)

    expect(screen.getByText(t("noContainers"))).toBeInTheDocument()
    expect(screen.getByText(t("runContainerTip"))).toBeInTheDocument()
  })

  it("renders a single running container with correct details", () => {
    const c = mockContainer()
    const groups = [makeSingleGroup(c)]
    render(<Containers {...defaultProps} containers={[c]} groups={groups} />)

    expect(screen.getByText("web-server")).toBeInTheDocument()
    expect(screen.getByText(/nginx:latest/)).toBeInTheDocument()
    expect(screen.getByText("Up 2 hours")).toBeInTheDocument()
  })

  it("renders a stopped container with start button", () => {
    const c = mockContainer({ state: "exited", status: "Exited (0) 1 hour ago" })
    const groups = [makeSingleGroup(c)]
    render(<Containers {...defaultProps} containers={[c]} groups={groups} />)

    const startBtn = screen.getByTitle(t("start"))
    expect(startBtn).toBeInTheDocument()
    expect(screen.queryByTitle(t("stop"))).not.toBeInTheDocument()
  })

  it("renders a running container with stop button", () => {
    const c = mockContainer()
    const groups = [makeSingleGroup(c)]
    render(<Containers {...defaultProps} containers={[c]} groups={groups} />)

    const stopBtn = screen.getByTitle(t("stop"))
    expect(stopBtn).toBeInTheDocument()
    expect(screen.queryByTitle(t("start"))).not.toBeInTheDocument()
  })

  it("calls onContainerAction with stop when stop button is clicked", async () => {
    const user = userEvent.setup()
    const onContainerAction = vi.fn()
    const c = mockContainer()
    const groups = [makeSingleGroup(c)]

    render(
      <Containers
        {...defaultProps}
        containers={[c]}
        groups={groups}
        onContainerAction={onContainerAction}
      />
    )

    const stopBtn = screen.getByTitle(t("stop"))
    await user.click(stopBtn)
    expect(onContainerAction).toHaveBeenCalledWith("stop_container", "abc123")
  })

  it("calls onContainerAction with start when start button is clicked", async () => {
    const user = userEvent.setup()
    const onContainerAction = vi.fn()
    const c = mockContainer({ state: "exited", status: "Exited" })
    const groups = [makeSingleGroup(c)]

    render(
      <Containers
        {...defaultProps}
        containers={[c]}
        groups={groups}
        onContainerAction={onContainerAction}
      />
    )

    const startBtn = screen.getByTitle(t("start"))
    await user.click(startBtn)
    expect(onContainerAction).toHaveBeenCalledWith("start_container", "abc123")
  })

  it("calls onContainerAction with remove when delete is confirmed", async () => {
    const user = userEvent.setup()
    const onContainerAction = vi.fn()
    const c = mockContainer({ state: "exited", status: "Exited" })
    const groups = [makeSingleGroup(c)]

    render(
      <Containers
        {...defaultProps}
        containers={[c]}
        groups={groups}
        onContainerAction={onContainerAction}
      />
    )

    // Step 1: Click the delete button to open confirmation dialog
    const deleteBtn = screen.getByTitle(t("delete"))
    await user.click(deleteBtn)

    // Step 2: The confirmation modal should now be visible
    expect(screen.getByText(t("confirmRemoveContainer"))).toBeInTheDocument()
    // "web-server" appears in both the card and the modal
    const nameOccurrences = screen.getAllByText("web-server")
    expect(nameOccurrences.length).toBeGreaterThanOrEqual(2)

    // Step 3: Click the confirm remove button in the modal
    // The modal has a red "Remove" button
    const confirmBtn = screen.getByText(t("remove"))
    await user.click(confirmBtn)

    expect(onContainerAction).toHaveBeenCalledWith("remove_container", "abc123")
  })

  it("disables action buttons when container is being acted upon", () => {
    const c = mockContainer()
    const groups = [makeSingleGroup(c)]

    render(
      <Containers
        {...defaultProps}
        containers={[c]}
        groups={groups}
        acting="abc123"
      />
    )

    const stopBtn = screen.getByTitle(t("stop"))
    expect(stopBtn).toBeDisabled()

    const deleteBtn = screen.getByTitle(t("delete"))
    expect(deleteBtn).toBeDisabled()
  })

  it("opens the run container modal when Run Container button is clicked", async () => {
    const user = userEvent.setup()
    const c = mockContainer()
    const groups = [makeSingleGroup(c)]

    render(<Containers {...defaultProps} containers={[c]} groups={groups} />)

    const runBtn = screen.getByText(t("runNewContainer"))
    await user.click(runBtn)

    expect(screen.getByText(t("runContainer"))).toBeInTheDocument()
    expect(screen.getByPlaceholderText("nginx:latest")).toBeInTheDocument()
  })

  it("renders container groups with collapsible headers", () => {
    const c1 = mockContainer({ id: "id-1", name: "app-web" })
    const c2 = mockContainer({ id: "id-2", name: "app-api", state: "exited" })
    const group: ContainerGroup = {
      key: "app",
      containers: [c1, c2],
      runningCount: 1,
      stoppedCount: 1,
    }

    render(<Containers {...defaultProps} containers={[c1, c2]} groups={[group]} />)

    expect(screen.getByText("app")).toBeInTheDocument()
    expect(screen.getByText(/1 Running/)).toBeInTheDocument()
    expect(screen.getByText(/1 Stopped/)).toBeInTheDocument()
  })

  it("shows children containers when a group is expanded", () => {
    const c1 = mockContainer({ id: "id-1", name: "app-web" })
    const c2 = mockContainer({ id: "id-2", name: "app-api", state: "exited", status: "Exited" })
    const group: ContainerGroup = {
      key: "app",
      containers: [c1, c2],
      runningCount: 1,
      stoppedCount: 1,
    }

    render(
      <Containers
        {...defaultProps}
        containers={[c1, c2]}
        groups={[group]}
        expandedGroups={{ app: true }}
      />
    )

    expect(screen.getByText("app-web")).toBeInTheDocument()
    expect(screen.getByText("app-api")).toBeInTheDocument()
  })

  it("does not show children when group is collapsed", () => {
    const c1 = mockContainer({ id: "id-1", name: "app-web" })
    const c2 = mockContainer({ id: "id-2", name: "app-api", state: "exited", status: "Exited" })
    const group: ContainerGroup = {
      key: "app",
      containers: [c1, c2],
      runningCount: 1,
      stoppedCount: 1,
    }

    render(
      <Containers
        {...defaultProps}
        containers={[c1, c2]}
        groups={[group]}
        expandedGroups={{}}
      />
    )

    // The group header is visible but children are not
    expect(screen.getByText("app")).toBeInTheDocument()
    expect(screen.queryByText("app-web")).not.toBeInTheDocument()
    expect(screen.queryByText("app-api")).not.toBeInTheDocument()
  })

  it("calls onToggleGroup when group header is clicked", async () => {
    const user = userEvent.setup()
    const onToggleGroup = vi.fn()
    const c1 = mockContainer({ id: "id-1", name: "app-web" })
    const c2 = mockContainer({ id: "id-2", name: "app-api" })
    const group: ContainerGroup = {
      key: "app",
      containers: [c1, c2],
      runningCount: 2,
      stoppedCount: 0,
    }

    render(
      <Containers
        {...defaultProps}
        containers={[c1, c2]}
        groups={[group]}
        onToggleGroup={onToggleGroup}
      />
    )

    const headerButton = screen.getByText("app").closest("button")
    expect(headerButton).toBeTruthy()
    await user.click(headerButton!)
    expect(onToggleGroup).toHaveBeenCalledWith("app")
  })

  it("submits run container form with correct image name", async () => {
    const user = userEvent.setup()
    const onRun = vi.fn<(...args: unknown[]) => Promise<RunContainerResult | null>>().mockResolvedValue({
      id: "new-123",
      name: "my-container",
      image: "redis:latest",
      login_cmd: "docker exec -it my-container /bin/sh",
    })
    const c = mockContainer()
    const groups = [makeSingleGroup(c)]

    render(
      <Containers
        {...defaultProps}
        containers={[c]}
        groups={groups}
        onRun={onRun}
      />
    )

    // Open the run modal
    await user.click(screen.getByText(t("runNewContainer")))

    // Fill image name
    const imageInput = screen.getByPlaceholderText("nginx:latest")
    await user.type(imageInput, "redis:latest")

    // Click create
    const createBtn = screen.getByText(t("create"))
    await user.click(createBtn)

    expect(onRun).toHaveBeenCalledWith("redis:latest", "", "", "", true, undefined)
  })

  it("shows exec button only for running containers", () => {
    const running = mockContainer({ id: "running-1", name: "running-c" })
    const stopped = mockContainer({ id: "stopped-1", name: "stopped-c", state: "exited", status: "Exited" })
    const groups = [makeSingleGroup(running), makeSingleGroup(stopped)]

    render(
      <Containers
        {...defaultProps}
        containers={[running, stopped]}
        groups={groups}
      />
    )

    // There should be exactly 1 exec command button (for the running container)
    const execBtns = screen.getAllByTitle(t("execCommand"))
    expect(execBtns).toHaveLength(1)
  })

  it("renders multiple single-container groups", () => {
    const c1 = mockContainer({ id: "id-1", name: "redis" })
    const c2 = mockContainer({ id: "id-2", name: "postgres", state: "exited", status: "Exited" })
    const groups = [makeSingleGroup(c1), makeSingleGroup(c2)]

    render(
      <Containers {...defaultProps} containers={[c1, c2]} groups={groups} />
    )

    expect(screen.getByText("redis")).toBeInTheDocument()
    expect(screen.getByText("postgres")).toBeInTheDocument()
  })

  it("opens log viewer modal when logs button is clicked", async () => {
    const user = userEvent.setup()
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "container_logs") {
        return "sample log line 1\nsample log line 2"
      }
      if (cmd === "container_stats") {
        return {
          cpu_percent: 5.0,
          memory_usage_mb: 64,
          memory_limit_mb: 256,
          memory_percent: 25,
          network_rx_bytes: 0,
          network_tx_bytes: 0,
        }
      }
      return null
    })
    const c = mockContainer()
    const groups = [makeSingleGroup(c)]

    render(<Containers {...defaultProps} containers={[c]} groups={groups} />)

    const logsBtn = screen.getByTitle(t("viewLogs"))
    await user.click(logsBtn)

    expect(screen.getByText(/Logs — web-server/)).toBeInTheDocument()
    expect(invoke).toHaveBeenCalledWith("container_logs", {
      id: "abc123",
      tail: "200",
      timestamps: false,
    })
  })
})
