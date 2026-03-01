import { useState, useEffect, useCallback } from "react"
import { invoke } from "@tauri-apps/api/core"
import type { VmInfoDto } from "../types"

export function useVms() {
  const [vms, setVms] = useState<VmInfoDto[]>([])
  const [vmLoading, setVmLoading] = useState(false)
  const [vmError, setVmError] = useState("")
  const [vmName, setVmName] = useState("")
  const [vmCpus, setVmCpus] = useState(2)
  const [vmMem, setVmMem] = useState(2048)
  const [vmDisk, setVmDisk] = useState(20)
  const [vmRosetta, setVmRosetta] = useState(false)
  const [vmActing, setVmActing] = useState("")
  const [vmLoginUser, setVmLoginUser] = useState("root")
  const [vmLoginHost, setVmLoginHost] = useState("127.0.0.1")
  const [vmLoginPort, setVmLoginPort] = useState<number | "">(2222)

  const [mountVmId, setMountVmId] = useState("")
  const [mountTag, setMountTag] = useState("")
  const [mountHostPath, setMountHostPath] = useState("")
  const [mountGuestPath, setMountGuestPath] = useState("/mnt/host")
  const [mountReadonly, setMountReadonly] = useState(false)

  const fetchVms = useCallback(async () => {
    setVmLoading(true)
    try {
      const result = await invoke<VmInfoDto[]>("vm_list")
      setVms(result)
      setVmError("")
    } catch (e) {
      setVmError(String(e))
    } finally {
      setVmLoading(false)
    }
  }, [])

  useEffect(() => {
    fetchVms()
  }, [fetchVms])

  const vmAction = useCallback(async (cmd: string, id: string) => {
    setVmActing(id)
    setVmError("")
    try {
      await invoke(cmd, { id })
      await fetchVms()
    } catch (e) {
      setVmError(String(e))
    } finally {
      setVmActing("")
    }
  }, [fetchVms])

  const createVm = useCallback(async () => {
    if (!vmName.trim()) return
    setVmActing("create")
    setVmError("")
    try {
      await invoke<string>("vm_create", {
        name: vmName.trim(),
        cpus: vmCpus,
        memory_mb: vmMem,
        disk_gb: vmDisk,
        rosetta: vmRosetta,
      })
      setVmName("")
      await fetchVms()
      return true
    } catch (e) {
      setVmError(String(e))
      return false
    } finally {
      setVmActing("")
    }
  }, [vmName, vmCpus, vmMem, vmDisk, vmRosetta, fetchVms])

  const getLoginCmd = useCallback(async (vm: VmInfoDto) => {
    setVmError("")
    try {
      const cmd = await invoke<string>("vm_login_cmd", {
        name: vm.name || vm.id,
        user: vmLoginUser,
        host: vmLoginHost,
        port: vmLoginPort === "" ? null : vmLoginPort,
      })
      return cmd
    } catch (e) {
      setVmError(String(e))
      return null
    }
  }, [vmLoginUser, vmLoginHost, vmLoginPort])

  const addMount = useCallback(async () => {
    if (!mountVmId || !mountTag.trim() || !mountHostPath.trim()) return
    setVmError("")
    try {
      await invoke("vm_mount_add", {
        vm: mountVmId,
        tag: mountTag.trim(),
        host_path: mountHostPath.trim(),
        guest_path: mountGuestPath.trim() || "/mnt/host",
        readonly: mountReadonly,
      })
      setMountTag("")
      setMountHostPath("")
      await fetchVms()
      return true
    } catch (e) {
      setVmError(String(e))
      return false
    }
  }, [mountVmId, mountTag, mountHostPath, mountGuestPath, mountReadonly, fetchVms])

  const removeMount = useCallback(async (vmId: string, tag: string) => {
    setVmError("")
    try {
      await invoke("vm_mount_remove", { vm: vmId, tag })
      await fetchVms()
      return true
    } catch (e) {
      setVmError(String(e))
      return false
    }
  }, [fetchVms])

  return {
    vms, vmLoading, vmError, setVmError,
    vmName, setVmName, vmCpus, setVmCpus,
    vmMem, setVmMem, vmDisk, setVmDisk,
    vmRosetta, setVmRosetta, vmActing,
    vmLoginUser, setVmLoginUser,
    vmLoginHost, setVmLoginHost,
    vmLoginPort, setVmLoginPort,
    mountVmId, setMountVmId,
    mountTag, setMountTag,
    mountHostPath, setMountHostPath,
    mountGuestPath, setMountGuestPath,
    mountReadonly, setMountReadonly,
    fetchVms, vmAction, createVm,
    getLoginCmd, addMount, removeMount,
  }
}
