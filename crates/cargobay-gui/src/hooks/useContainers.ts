import { useState, useEffect, useCallback } from "react"
import { invoke } from "@tauri-apps/api/core"
import type { ContainerInfo, ContainerGroup } from "../types"

function containerGroupCandidates(name: string): string[] {
  const trimmed = name.trim()
  if (!trimmed) return []

  const out = new Set<string>()
  out.add(trimmed)

  const base = trimmed.replace(/[-_]\d+$/, "")
  if (base) out.add(base)

  for (let i = 0; i < trimmed.length; i++) {
    const ch = trimmed[i]
    if (ch === "-" || ch === "_") {
      const prefix = trimmed.slice(0, i)
      if (prefix) out.add(prefix)
    }
  }

  return Array.from(out)
}

function groupContainersByNamePrefix(containers: ContainerInfo[]): ContainerGroup[] {
  if (containers.length === 0) return []

  const groups = new Map<string, ContainerInfo[]>()
  const assigned = new Set<string>()

  for (const c of containers) {
    const candidates = containerGroupCandidates(c.name)
    let bestKey = ""
    let bestCount = 0

    for (const cand of candidates) {
      if (cand === c.name) continue
      let count = 0
      for (const other of containers) {
        if (other.id === c.id) continue
        const otherCands = containerGroupCandidates(other.name)
        if (otherCands.includes(cand)) count++
      }
      if (count > bestCount) {
        bestCount = count
        bestKey = cand
      }
    }

    if (bestKey && bestCount > 0) {
      if (!groups.has(bestKey)) groups.set(bestKey, [])
      groups.get(bestKey)!.push(c)
      assigned.add(c.id)
    }
  }

  const out: ContainerGroup[] = []

  for (const [key, members] of groups) {
    out.push({
      key,
      containers: members,
      runningCount: members.filter(c => c.state === "running").length,
      stoppedCount: members.filter(c => c.state !== "running").length,
    })
  }

  for (const c of containers) {
    if (!assigned.has(c.id)) {
      out.push({
        key: c.name || c.id,
        containers: [c],
        runningCount: c.state === "running" ? 1 : 0,
        stoppedCount: c.state === "running" ? 0 : 1,
      })
    }
  }

  out.sort((a, b) => {
    if (a.containers.length !== b.containers.length) return b.containers.length - a.containers.length
    return a.key.localeCompare(b.key)
  })

  return out
}

export function useContainers() {
  const [containers, setContainers] = useState<ContainerInfo[]>([])
  const [error, setError] = useState("")
  const [loading, setLoading] = useState(true)
  const [connected, setConnected] = useState(false)
  const [acting, setActing] = useState("")
  const [expandedGroups, setExpandedGroups] = useState<Record<string, boolean>>({})

  const fetchContainers = useCallback(async () => {
    try {
      const result = await invoke<ContainerInfo[]>("list_containers")
      setContainers(result)
      setError("")
      setConnected(true)
    } catch (e) {
      setError(String(e))
      setConnected(false)
    } finally {
      setLoading(false)
    }
  }, [])

  useEffect(() => {
    fetchContainers()
    const iv = setInterval(fetchContainers, 3000)
    return () => clearInterval(iv)
  }, [fetchContainers])

  const containerAction = useCallback(async (cmd: string, id: string) => {
    setActing(id)
    try {
      await invoke(cmd, { id })
      await fetchContainers()
    } catch (e) {
      setError(String(e))
    } finally {
      setActing("")
    }
  }, [fetchContainers])

  const toggleGroup = useCallback((key: string) => {
    setExpandedGroups(prev => ({ ...prev, [key]: !prev[key] }))
  }, [])

  const running = containers.filter(c => c.state === "running")
  const groups = groupContainersByNamePrefix(containers)

  return {
    containers,
    running,
    groups,
    error,
    setError,
    loading,
    connected,
    acting,
    expandedGroups,
    fetchContainers,
    containerAction,
    toggleGroup,
  }
}
