import { useState, useEffect, useCallback, useRef } from "react"
import { invoke } from "@tauri-apps/api/core"
import type { VolumeInfo } from "../types"

const volumeNameCollator = new Intl.Collator(undefined, { numeric: true, sensitivity: "base" })

function compareStrings(a: string, b: string): number {
  const primary = volumeNameCollator.compare(a, b)
  if (primary !== 0) return primary
  // Deterministic tie-breaker when collator considers strings equal (e.g. different casing).
  return a.localeCompare(b)
}

function sortVolumesByName(vols: VolumeInfo[]): VolumeInfo[] {
  return [...vols].sort((a, b) => compareStrings(a?.name ?? "", b?.name ?? ""))
}

function normalizeKv(obj: Record<string, string> | null | undefined): string {
  if (!obj) return ""
  const entries = Object.entries(obj)
  entries.sort(([ka], [kb]) => compareStrings(ka, kb))
  return entries.map(([k, v]) => `${k}=${v}`).join("\n")
}

function volumeSignature(v: VolumeInfo): string {
  // Include all fields to avoid skipping updates when data changes but UI doesn't.
  return [
    v?.name ?? "",
    v?.driver ?? "",
    v?.mountpoint ?? "",
    v?.created_at ?? "",
    v?.scope ?? "",
    normalizeKv(v?.labels),
    normalizeKv(v?.options),
  ].join("\u0000")
}

function areVolumesListEqual(prev: VolumeInfo[], next: VolumeInfo[]): boolean {
  if (prev === next) return true
  if (prev.length !== next.length) return false
  for (let i = 0; i < prev.length; i++) {
    if (volumeSignature(prev[i]) !== volumeSignature(next[i])) return false
  }
  return true
}

export function useVolumes() {
  const [volumes, setVolumes] = useState<VolumeInfo[]>([])
  const [error, setError] = useState("")
  const [loading, setLoading] = useState(true)
  const latestReqId = useRef(0)

  const fetchVolumes = useCallback(async () => {
    const reqId = ++latestReqId.current
    try {
      const result = await invoke<VolumeInfo[]>("volume_list")
      const next = sortVolumesByName(Array.isArray(result) ? result : [])

      // Only allow the latest in-flight request to update state (prevents stale overwrite).
      if (reqId !== latestReqId.current) return

      setVolumes(prev => (areVolumesListEqual(prev, next) ? prev : next))
      setError("")
    } catch (e) {
      if (reqId !== latestReqId.current) return
      setError(String(e))
    }
    if (reqId === latestReqId.current) {
      setLoading(false)
    }
  }, [])

  useEffect(() => {
    const initialFetch = setTimeout(() => {
      void fetchVolumes()
    }, 0)
    const iv = setInterval(() => {
      void fetchVolumes()
    }, 5000)
    return () => {
      clearTimeout(initialFetch)
      clearInterval(iv)
    }
  }, [fetchVolumes])

  const createVolume = useCallback(async (name: string, driver: string) => {
    const vol = await invoke<VolumeInfo>("volume_create", { name, driver })
    await fetchVolumes()
    return vol
  }, [fetchVolumes])

  const inspectVolume = useCallback(async (name: string) => {
    return await invoke<VolumeInfo>("volume_inspect", { name })
  }, [])

  const removeVolume = useCallback(async (name: string) => {
    await invoke<void>("volume_remove", { name })
    await fetchVolumes()
  }, [fetchVolumes])

  return {
    volumes,
    error,
    setError,
    loading,
    fetchVolumes,
    createVolume,
    inspectVolume,
    removeVolume,
  }
}
