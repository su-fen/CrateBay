import { describe, it, expect, vi, beforeEach } from "vitest"
import { render, screen } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { invoke } from "@tauri-apps/api/core"
import { Images } from "../Images"
import { messages } from "../../i18n/messages"
import type { ImageSearchResult, RunContainerResult } from "../../types"

const t = (key: string) => messages.en[key] || key

const mockSearchResult = (overrides?: Partial<ImageSearchResult>): ImageSearchResult => ({
  source: "dockerhub",
  reference: "nginx",
  description: "Official Nginx image",
  stars: 1500,
  pulls: 5000000,
  official: true,
  ...overrides,
})

const defaultProps = {
  imgQuery: "",
  setImgQuery: vi.fn(),
  imgSource: "all",
  setImgSource: vi.fn(),
  imgResults: [] as ImageSearchResult[],
  imgSearching: false,
  imgError: "",
  setImgError: vi.fn(),
  imgTags: [] as string[],
  imgTagsRef: "",
  imgTagsLoading: false,
  runImage: "",
  setRunImage: vi.fn(),
  runName: "",
  setRunName: vi.fn(),
  runCpus: "" as number | "",
  setRunCpus: vi.fn(),
  runMem: "" as number | "",
  setRunMem: vi.fn(),
  runPull: true,
  setRunPull: vi.fn(),
  runLoading: false,
  runResult: null as RunContainerResult | null,
  setRunResult: vi.fn(),
  loadPath: "",
  setLoadPath: vi.fn(),
  loadLoading: false,
  pushRef: "",
  setPushRef: vi.fn(),
  pushLoading: false,
  onSearch: vi.fn(),
  onTags: vi.fn(),
  onRun: vi.fn(),
  onLoad: vi.fn(),
  onPush: vi.fn(),
  onCopy: vi.fn(),
  t,
}

const renderImages = (props: Partial<typeof defaultProps> = {}) => {
  render(<Images {...defaultProps} {...props} />)
}

const openSearchTab = async (user: ReturnType<typeof userEvent.setup>) => {
  await user.click(screen.getByTestId("images-tab-search"))
}

describe("Images", () => {
  beforeEach(() => {
    vi.clearAllMocks()
    vi.mocked(invoke).mockResolvedValue([])
  })

  it("renders the search input and buttons", async () => {
    const user = userEvent.setup()
    renderImages()
    await openSearchTab(user)

    expect(screen.getByPlaceholderText(t("searchImages"))).toBeInTheDocument()
    expect(screen.getByText(t("search"))).toBeInTheDocument()
  })

  it("shows the local images section header", () => {
    renderImages()

    expect(screen.getByTestId("images-tab-local")).toBeInTheDocument()
  })

  it("shows empty search state with hint", async () => {
    const user = userEvent.setup()
    renderImages()
    await openSearchTab(user)

    expect(screen.getByText(t("searchHint"))).toBeInTheDocument()
  })

  it("calls onSearch when the search button is clicked", async () => {
    const user = userEvent.setup()
    const onSearch = vi.fn()
    renderImages({ imgQuery: "nginx", onSearch })
    await openSearchTab(user)

    const searchBtn = screen.getByText(t("search"))
    await user.click(searchBtn)

    expect(onSearch).toHaveBeenCalled()
  })

  it("disables search button when imgQuery is empty", async () => {
    const user = userEvent.setup()
    renderImages({ imgQuery: "" })
    await openSearchTab(user)

    const searchBtn = screen.getByText(t("search"))
    expect(searchBtn).toBeDisabled()
  })

  it("disables search button while searching", async () => {
    const user = userEvent.setup()
    renderImages({ imgQuery: "nginx", imgSearching: true })
    await openSearchTab(user)

    expect(screen.getByText(t("searching"))).toBeDisabled()
  })

  it("renders search results as cards", async () => {
    const user = userEvent.setup()
    const results = [
      mockSearchResult(),
      mockSearchResult({
        source: "quay",
        reference: "quay.io/coreos/etcd",
        description: "etcd service",
        official: false,
        stars: 100,
        pulls: 50000,
      }),
    ]

    renderImages({ imgResults: results })
    await openSearchTab(user)

    expect(screen.getByText("nginx")).toBeInTheDocument()
    expect(screen.getByText("Official Nginx image")).toBeInTheDocument()
    expect(screen.getByText("quay.io/coreos/etcd")).toBeInTheDocument()
    expect(screen.getByText("etcd service")).toBeInTheDocument()
  })

  it("shows official badge for official images", async () => {
    const user = userEvent.setup()
    const results = [mockSearchResult({ official: true })]
    renderImages({ imgResults: results })
    await openSearchTab(user)

    expect(screen.getByText(t("official"))).toBeInTheDocument()
  })

  it("does not show official badge for non-official images", async () => {
    const user = userEvent.setup()
    const results = [mockSearchResult({ official: false })]
    renderImages({ imgResults: results })
    await openSearchTab(user)

    expect(screen.queryByText(t("official"))).not.toBeInTheDocument()
  })

  it("shows source badge on result cards", async () => {
    const user = userEvent.setup()
    const results = [mockSearchResult({ source: "dockerhub" })]
    renderImages({ imgResults: results })
    await openSearchTab(user)

    expect(screen.getByText("dockerhub")).toBeInTheDocument()
  })

  it("renders run and tags buttons on each result card", async () => {
    const user = userEvent.setup()
    const results = [mockSearchResult()]
    renderImages({ imgResults: results })
    await openSearchTab(user)

    expect(screen.getAllByText(t("run")).length).toBeGreaterThanOrEqual(1)
    expect(screen.getAllByText(t("tags")).length).toBeGreaterThanOrEqual(1)
  })

  it("calls onTags when tags button is clicked on a tagged image", async () => {
    const user = userEvent.setup()
    const onTags = vi.fn()
    const results = [mockSearchResult({ reference: "quay.io/coreos/etcd" })]

    renderImages({ imgResults: results, onTags })
    await openSearchTab(user)

    // Find the tags button within the search result card
    const tagsButtons = screen.getAllByText(t("tags"))
    const enabledTags = tagsButtons.filter(btn => !(btn as HTMLButtonElement).disabled)
    expect(enabledTags.length).toBeGreaterThanOrEqual(1)
    await user.click(enabledTags[0])

    expect(onTags).toHaveBeenCalledWith("quay.io/coreos/etcd")
  })

  it("displays tags when available", async () => {
    const user = userEvent.setup()
    renderImages({
      imgTags: ["latest", "1.0", "2.0"],
      imgTagsRef: "quay.io/coreos/etcd",
    })
    await openSearchTab(user)

    expect(screen.getByText(/Tags/)).toBeInTheDocument()
    expect(screen.getByText("latest")).toBeInTheDocument()
    expect(screen.getByText("1.0")).toBeInTheDocument()
    expect(screen.getByText("2.0")).toBeInTheDocument()
  })

  it("shows the import image button", () => {
    renderImages()

    expect(screen.getByText(t("importImage"))).toBeInTheDocument()
  })

  it("opens the import/push modal when import button is clicked", async () => {
    const user = userEvent.setup()
    renderImages()

    const importBtn = screen.getByText(t("importImage"))
    await user.click(importBtn)

    expect(screen.getByText(t("imageArchivePath"))).toBeInTheDocument()
    expect(screen.getByText(t("load"))).toBeInTheDocument()
  })

  it("shows error message when imgError is set", () => {
    renderImages({ imgError: "Something went wrong" })

    expect(screen.getByText("Something went wrong")).toBeInTheDocument()
  })

  it("dismisses error when dismiss button is clicked", async () => {
    const user = userEvent.setup()
    const setImgError = vi.fn()
    renderImages({ imgError: "Some error", setImgError })

    const dismissBtn = screen.getByTestId("error-inline-dismiss")
    await user.click(dismissBtn)

    expect(setImgError).toHaveBeenCalledWith("")
  })

  it("shows confirm remove dialog when delete is clicked on a local image", async () => {
    const user = userEvent.setup()
    vi.mocked(invoke).mockResolvedValue([
      {
        id: "sha256:abc123",
        repo_tags: ["nginx:latest"],
        size_bytes: 141000000,
        size_human: "141 MB",
        created: 1700000000,
      },
    ])

    renderImages()

    // Wait for local images to load
    const removeBtn = await screen.findByTitle(t("removeImage"))
    await user.click(removeBtn)

    expect(screen.getByText(t("confirmRemoveImage"))).toBeInTheDocument()
    // The modal shows the image reference -- there will be multiple "nginx:latest"
    // (one in card, one in modal), so use getAllByText
    const refs = screen.getAllByText("nginx:latest")
    expect(refs.length).toBeGreaterThanOrEqual(2)
    expect(screen.getByText(t("remove"))).toBeInTheDocument()
  })

  it("calls invoke with image_remove when confirm remove is clicked", async () => {
    const user = userEvent.setup()
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "image_list") {
        return [
          {
            id: "sha256:abc123",
            repo_tags: ["nginx:latest"],
            size_bytes: 141000000,
            size_human: "141 MB",
            created: 1700000000,
          },
        ]
      }
      return undefined
    })

    renderImages()

    // Wait for local images to load and click remove
    const removeBtn = await screen.findByTitle(t("removeImage"))
    await user.click(removeBtn)

    // Click the confirm remove button
    const confirmBtn = screen.getByText(t("remove"))
    await user.click(confirmBtn)

    expect(invoke).toHaveBeenCalledWith("image_remove", { id: "nginx:latest" })
  })

  it("shows source filter dropdown", async () => {
    const user = userEvent.setup()
    renderImages()
    await openSearchTab(user)

    expect(screen.getByText(t("sourceAll"))).toBeInTheDocument()
  })

  it("formats pull counts with K and M suffixes", async () => {
    const user = userEvent.setup()
    const results = [
      mockSearchResult({ reference: "nginx-official", pulls: 5000000 }),
      mockSearchResult({ reference: "small-image", pulls: 1500, source: "quay" }),
    ]

    renderImages({ imgResults: results })
    await openSearchTab(user)

    expect(screen.getByText("5.0M")).toBeInTheDocument()
    expect(screen.getByText("1.5K")).toBeInTheDocument()
  })

  it("shows local image filter input", () => {
    renderImages()

    expect(screen.getByPlaceholderText(t("filterLocalImages"))).toBeInTheDocument()
  })

  it("shows no local images message when list is empty", async () => {
    vi.mocked(invoke).mockResolvedValue([])
    renderImages()

    expect(await screen.findByText(t("noLocalImages"))).toBeInTheDocument()
  })
})
