import { useState, useCallback } from "react"
import { invoke } from "@tauri-apps/api/core"
import type { ImageSearchResult, RunContainerResult } from "../types"

export function useImageSearch() {
  const [imgQuery, setImgQuery] = useState("")
  const [imgSource, setImgSource] = useState("all")
  const [imgResults, setImgResults] = useState<ImageSearchResult[]>([])
  const [imgSearching, setImgSearching] = useState(false)
  const [imgError, setImgError] = useState("")
  const [imgTags, setImgTags] = useState<string[]>([])
  const [imgTagsRef, setImgTagsRef] = useState("")
  const [imgTagsLoading, setImgTagsLoading] = useState(false)
  const [runImage, setRunImage] = useState("")
  const [runName, setRunName] = useState("")
  const [runCpus, setRunCpus] = useState<number | "">("")
  const [runMem, setRunMem] = useState<number | "">("")
  const [runPull, setRunPull] = useState(true)
  const [runLoading, setRunLoading] = useState(false)
  const [runResult, setRunResult] = useState<RunContainerResult | null>(null)
  const [loadPath, setLoadPath] = useState("")
  const [loadLoading, setLoadLoading] = useState(false)
  const [pushRef, setPushRef] = useState("")
  const [pushLoading, setPushLoading] = useState(false)

  const doSearch = useCallback(async () => {
    setImgSearching(true)
    setImgError("")
    setRunResult(null)
    try {
      const result = await invoke<ImageSearchResult[]>("image_search", {
        query: imgQuery, source: imgSource, limit: 20,
      })
      setImgResults(result)
    } catch (e) {
      setImgError(String(e))
    } finally {
      setImgSearching(false)
    }
  }, [imgQuery, imgSource])

  const doTags = useCallback(async (reference: string) => {
    setImgTagsLoading(true)
    setImgTagsRef(reference)
    try {
      const tags = await invoke<string[]>("image_tags", { reference, limit: 50 })
      setImgTags(tags)
    } catch (e) {
      setImgTags([])
      setImgError(String(e))
    } finally {
      setImgTagsLoading(false)
    }
  }, [])

  const doRun = useCallback(async (fetchContainers: () => Promise<void>) => {
    if (!runImage) return null
    setRunLoading(true)
    setImgError("")
    try {
      const result = await invoke<RunContainerResult>("docker_run", {
        image: runImage,
        name: runName.trim() ? runName.trim() : null,
        cpus: runCpus === "" ? null : runCpus,
        memory_mb: runMem === "" ? null : runMem,
        pull: runPull,
      })
      setRunResult(result)
      await fetchContainers()
      return result
    } catch (e) {
      setImgError(String(e))
      return null
    } finally {
      setRunLoading(false)
    }
  }, [runImage, runName, runCpus, runMem, runPull])

  const doRunDirect = useCallback(async (
    image: string, name: string, cpus: number | "", mem: number | "", pull: boolean,
    fetchContainers: () => Promise<void>,
    env?: string[],
  ) => {
    setRunLoading(true)
    setImgError("")
    try {
      const result = await invoke<RunContainerResult>("docker_run", {
        image,
        name: name.trim() ? name.trim() : null,
        cpus: cpus === "" ? null : cpus,
        memory_mb: mem === "" ? null : mem,
        pull,
        env: env && env.length > 0 ? env : null,
      })
      await fetchContainers()
      return result
    } catch (e) {
      setImgError(String(e))
      return null
    } finally {
      setRunLoading(false)
    }
  }, [])

  const doLoad = useCallback(async () => {
    if (!loadPath.trim()) return null
    setLoadLoading(true)
    setImgError("")
    try {
      const out = await invoke<string>("image_load", { path: loadPath.trim() })
      return out
    } catch (e) {
      setImgError(String(e))
      return null
    } finally {
      setLoadLoading(false)
    }
  }, [loadPath])

  const doPush = useCallback(async () => {
    if (!pushRef.trim()) return null
    setPushLoading(true)
    setImgError("")
    try {
      const out = await invoke<string>("image_push", { reference: pushRef.trim() })
      return out
    } catch (e) {
      setImgError(String(e))
      return null
    } finally {
      setPushLoading(false)
    }
  }, [pushRef])

  const clearResults = useCallback(() => {
    setImgResults([])
    setImgTags([])
    setImgError("")
    setRunResult(null)
  }, [])

  return {
    imgQuery, setImgQuery,
    imgSource, setImgSource,
    imgResults, imgSearching, imgError, setImgError,
    imgTags, imgTagsRef, imgTagsLoading,
    runImage, setRunImage, runName, setRunName,
    runCpus, setRunCpus, runMem, setRunMem,
    runPull, setRunPull, runLoading, runResult, setRunResult,
    loadPath, setLoadPath, loadLoading,
    pushRef, setPushRef, pushLoading,
    doSearch, doTags, doRun, doRunDirect, doLoad, doPush, clearResults,
  }
}
