import { describe, it, expect, vi, beforeEach } from "vitest"
import { render, screen, within } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { invoke } from "@tauri-apps/api/core"
import { Dashboard } from "../Dashboard"
import { messages } from "../../i18n/messages"
import type { ContainerInfo, VmInfoDto } from "../../types"

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

const mockVm = (overrides?: Partial<VmInfoDto>): VmInfoDto => ({
  id: "vm-1",
  name: "dev-vm",
  state: "running",
  cpus: 2,
  memory_mb: 2048,
  disk_gb: 20,
  rosetta_enabled: false,
  mounts: [],
  port_forwards: [],
  os_image: null,
  ...overrides,
})

const defaultProps = {
  containers: [] as ContainerInfo[],
  running: [] as ContainerInfo[],
  vmsCount: 0,
  vmsRunningCount: 0,
  runningVms: [] as VmInfoDto[],
  imgResultsCount: 0,
  installedImagesCount: 0,
  volumesCount: 0,
  onNavigate: vi.fn(),
  t,
}

describe("Dashboard", () => {
  beforeEach(() => {
    vi.clearAllMocks()
    vi.mocked(invoke).mockResolvedValue(null)
  })

  it("renders four dashboard cards", () => {
    render(<Dashboard {...defaultProps} />)

    expect(screen.getByText(t("containers"))).toBeInTheDocument()
    expect(screen.getByText(t("vms"))).toBeInTheDocument()
    expect(screen.getByText(t("images"))).toBeInTheDocument()
    expect(screen.getByText(t("volumes"))).toBeInTheDocument()
  })

  it("shows container count on the containers card", () => {
    const containers = [mockContainer(), mockContainer({ id: "def456", name: "api" })]
    render(<Dashboard {...defaultProps} containers={containers} />)

    const containersCard = screen.getByTestId("dashboard-card-containers")
    expect(within(containersCard).getByText("2")).toBeInTheDocument()
  })

  it("shows VM count on the VMs card", () => {
    render(<Dashboard {...defaultProps} vmsCount={3} vmsRunningCount={1} />)

    const vmsCard = screen.getByTestId("dashboard-card-vms")
    expect(within(vmsCard).getByText("3")).toBeInTheDocument()
    expect(within(vmsCard).getByText(/1 running/)).toBeInTheDocument()
  })

  it("navigates to containers page when containers card is clicked", async () => {
    const user = userEvent.setup()
    const onNavigate = vi.fn()
    render(<Dashboard {...defaultProps} onNavigate={onNavigate} />)

    const containersCard = screen.getByTestId("dashboard-card-containers")
    await user.click(containersCard)

    expect(onNavigate).toHaveBeenCalledWith("containers")
  })

  it("navigates to VMs page when VMs card is clicked", async () => {
    const user = userEvent.setup()
    const onNavigate = vi.fn()
    render(<Dashboard {...defaultProps} onNavigate={onNavigate} />)

    const vmsCard = screen.getByTestId("dashboard-card-vms")
    await user.click(vmsCard)

    expect(onNavigate).toHaveBeenCalledWith("vms")
  })

  it("navigates to images page when images card is clicked", async () => {
    const user = userEvent.setup()
    const onNavigate = vi.fn()
    render(<Dashboard {...defaultProps} onNavigate={onNavigate} />)

    const imagesCard = screen.getByTestId("dashboard-card-images")
    await user.click(imagesCard)

    expect(onNavigate).toHaveBeenCalledWith("images")
  })

  it("shows running containers list when there are running containers", () => {
    const running = [
      mockContainer(),
      mockContainer({ id: "def456", name: "api-server", image: "node:18" }),
    ]
    render(
      <Dashboard {...defaultProps} containers={running} running={running} />
    )

    expect(screen.getByText("web-server")).toBeInTheDocument()
    expect(screen.getByText("api-server")).toBeInTheDocument()
    expect(screen.getByText(t("running"))).toBeInTheDocument()
    expect(screen.getAllByTestId("dashboard-running-item")).toHaveLength(2)
  })

  it("does not show running containers section when none are running", () => {
    render(<Dashboard {...defaultProps} />)

    expect(screen.queryByTestId("dashboard-running-item")).not.toBeInTheDocument()
  })

  it("shows 'view all' link when more than 5 containers are running", async () => {
    const user = userEvent.setup()
    const onNavigate = vi.fn()
    const running = Array.from({ length: 7 }, (_, i) =>
      mockContainer({ id: `id-${i}`, name: `container-${i}` })
    )
    render(
      <Dashboard
        {...defaultProps}
        containers={running}
        running={running}
        onNavigate={onNavigate}
      />
    )

    const viewAll = screen.getByText(t("viewAll"))
    expect(viewAll).toBeInTheDocument()

    await user.click(viewAll)
    expect(onNavigate).toHaveBeenCalledWith("containers")
  })

  it("renders only up to 5 running container cards", () => {
    const running = Array.from({ length: 7 }, (_, i) =>
      mockContainer({ id: `id-${i}`, name: `container-${i}` })
    )
    render(
      <Dashboard {...defaultProps} containers={running} running={running} />
    )

    // Should only render 5 running-item elements in the running section
    expect(screen.getAllByTestId("dashboard-running-item")).toHaveLength(5)
  })

  it("shows resource panel when containers or VMs are running", () => {
    const running = [mockContainer()]
    render(
      <Dashboard {...defaultProps} containers={running} running={running} />
    )

    expect(screen.getByText(t("cpuUsage"))).toBeInTheDocument()
    expect(screen.getByText(t("memoryUsage"))).toBeInTheDocument()
  })

  it("does not show resource panel when nothing is running", () => {
    render(<Dashboard {...defaultProps} />)

    expect(screen.queryByText(t("cpuUsage"))).not.toBeInTheDocument()
    expect(screen.queryByText(t("memoryUsage"))).not.toBeInTheDocument()
  })

  it("shows image results count", () => {
    render(
      <Dashboard {...defaultProps} imgResultsCount={15} installedImagesCount={8} />
    )

    const imagesCard = screen.getByTestId("dashboard-card-images")
    expect(within(imagesCard).getByText("8")).toBeInTheDocument()
    expect(
      within(imagesCard).getByText(new RegExp(`15\\s+${t("searchResults")}`, "i"))
    ).toBeInTheDocument()
  })

  it("fetches stats for running containers", () => {
    const running = [mockContainer()]
    vi.mocked(invoke).mockResolvedValue({
      cpu_percent: 25.0,
      memory_usage_mb: 128,
      memory_limit_mb: 512,
      memory_percent: 25,
      network_rx_bytes: 0,
      network_tx_bytes: 0,
    })

    render(
      <Dashboard {...defaultProps} containers={running} running={running} />
    )

    expect(invoke).toHaveBeenCalledWith("container_stats", { id: "abc123" })
  })

  it("fetches stats for running VMs", () => {
    const runningVm = mockVm()
    vi.mocked(invoke).mockResolvedValue({
      cpu_percent: 10.0,
      memory_usage_mb: 512,
      disk_usage_gb: 5,
    })

    render(
      <Dashboard
        {...defaultProps}
        vmsCount={1}
        vmsRunningCount={1}
        runningVms={[runningVm]}
      />
    )

    expect(invoke).toHaveBeenCalledWith("vm_stats", { id: "vm-1" })
  })
})
