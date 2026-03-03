import { describe, it, expect, vi, beforeEach } from "vitest"
import { renderHook, waitFor, act } from "@testing-library/react"
import { invoke } from "@tauri-apps/api/core"
import { useVolumes } from "../useVolumes"
import type { VolumeInfo } from "../../types"

function mkVol(overrides: Partial<VolumeInfo>): VolumeInfo {
  return {
    name: "vol",
    driver: "local",
    mountpoint: "/var/lib/docker/volumes/vol/_data",
    created_at: "2024-01-01T00:00:00Z",
    labels: {},
    options: {},
    scope: "local",
    ...overrides,
  }
}

function deferred<T>() {
  let resolve!: (value: T) => void
  let reject!: (reason?: unknown) => void
  const promise = new Promise<T>((res, rej) => {
    resolve = res
    reject = rej
  })
  return { promise, resolve, reject }
}

describe("useVolumes", () => {
  beforeEach(() => {
    vi.clearAllMocks()
  })

  it("sorts volumes by name (case-insensitive, numeric-aware)", async () => {
    vi.mocked(invoke).mockResolvedValueOnce([
      mkVol({ name: "b" }),
      mkVol({ name: "a" }),
    ])

    const { result, unmount } = renderHook(() => useVolumes())
    await waitFor(() => expect(result.current.loading).toBe(false))

    expect(result.current.volumes.map(v => v.name)).toEqual(["a", "b"])
    unmount()
  })

  it("sorts volumes with numeric ordering (vol-2 before vol-10)", async () => {
    vi.mocked(invoke).mockResolvedValueOnce([
      mkVol({ name: "vol-2" }),
      mkVol({ name: "vol-10" }),
      mkVol({ name: "vol-1" }),
    ])

    const { result, unmount } = renderHook(() => useVolumes())
    await waitFor(() => expect(result.current.loading).toBe(false))

    expect(result.current.volumes.map(v => v.name)).toEqual(["vol-1", "vol-2", "vol-10"])
    unmount()
  })

  it("prevents stale responses from overwriting newer results", async () => {
    const d1 = deferred<VolumeInfo[]>()
    const d2 = deferred<VolumeInfo[]>()

    vi.mocked(invoke)
      .mockImplementationOnce(async () => await d1.promise)
      .mockImplementationOnce(async () => await d2.promise)

    const { result, unmount } = renderHook(() => useVolumes())

    // Ensure the initial poll kicked off (useEffect -> fetchVolumes -> invoke).
    await waitFor(() => expect(vi.mocked(invoke)).toHaveBeenCalledTimes(1))

    // Trigger a second fetch while the first is still in-flight.
    act(() => {
      void result.current.fetchVolumes()
    })
    await waitFor(() => expect(vi.mocked(invoke)).toHaveBeenCalledTimes(2))

    // Resolve the newer request first.
    d2.resolve([mkVol({ name: "new" })])
    await waitFor(() => expect(result.current.volumes.map(v => v.name)).toEqual(["new"]))

    // Resolve the older request later; it must NOT overwrite the state.
    d1.resolve([mkVol({ name: "old" })])
    await waitFor(() => expect(result.current.volumes.map(v => v.name)).toEqual(["new"]))

    unmount()
  })
})

