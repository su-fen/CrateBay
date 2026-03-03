import { useState, useEffect, useCallback } from "react"
import { invoke } from "@tauri-apps/api/core"
import type { VolumeInfo } from "../types"

export function useVolumes() {
  const [volumes, setVolumes] = useState<VolumeInfo[]>([])
  const [error, setError] = useState("")
  const [loading, setLoading] = useState(true)

  const fetchVolumes = useCallback(async () => {
    try {
      const result = await invoke<VolumeInfo[]>("volume_list")
      setVolumes(result)
      setError("")
    } catch (e) {
      setError(String(e))
    } finally {
      setLoading(false)
    }
  }, [])

  useEffect(() => {
    fetchVolumes()
    const iv = setInterval(fetchVolumes, 5000)
    return () => clearInterval(iv)
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
