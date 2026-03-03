import { useState, useEffect, useCallback } from "react"
import { invoke } from "@tauri-apps/api/core"
import type { VmInfoDto, OsImageDto, OsImageDownloadProgressDto } from "../types"

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

  // OS Image state
  const [osImages, setOsImages] = useState<OsImageDto[]>([])
  const [selectedOsImage, setSelectedOsImage] = useState("")
  const [downloadingImage, setDownloadingImage] = useState("")
  const [downloadProgress, setDownloadProgress] = useState<OsImageDownloadProgressDto | null>(null)

  // Port forward form state
  const [pfVmId, setPfVmId] = useState("")
  const [pfHostPort, setPfHostPort] = useState<number | "">(8080)
  const [pfGuestPort, setPfGuestPort] = useState<number | "">(80)
  const [pfProtocol, setPfProtocol] = useState("tcp")

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

  const fetchOsImages = useCallback(async () => {
    try {
      const result = await invoke<OsImageDto[]>("image_catalog")
      setOsImages(result)
    } catch (e) {
      // Silently fail — catalog is best-effort.
      console.error("Failed to fetch OS image catalog:", e)
    }
  }, [])

  useEffect(() => {
    fetchVms()
    fetchOsImages()
    const iv = setInterval(fetchVms, 3000)
    return () => clearInterval(iv)
  }, [fetchVms, fetchOsImages])

  // Poll download progress when a download is active.
  useEffect(() => {
    if (!downloadingImage) return
    const iv = setInterval(async () => {
      try {
        const progress = await invoke<OsImageDownloadProgressDto>("image_download_status", {
          image_id: downloadingImage,
        })
        setDownloadProgress(progress)
        if (progress.done || progress.error) {
          setDownloadingImage("")
          setDownloadProgress(null)
          await fetchOsImages()
        }
      } catch {
        // Ignore polling errors.
      }
    }, 500)
    return () => clearInterval(iv)
  }, [downloadingImage, fetchOsImages])

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
        os_image: selectedOsImage || null,
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
  }, [vmName, vmCpus, vmMem, vmDisk, vmRosetta, selectedOsImage, fetchVms])

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

  const downloadOsImage = useCallback(async (imageId: string) => {
    setDownloadingImage(imageId)
    setDownloadProgress(null)
    setVmError("")
    try {
      await invoke("image_download_os", { image_id: imageId })
      await fetchOsImages()
      return true
    } catch (e) {
      setVmError(String(e))
      return false
    } finally {
      setDownloadingImage("")
      setDownloadProgress(null)
    }
  }, [fetchOsImages])

  const deleteOsImage = useCallback(async (imageId: string) => {
    setVmError("")
    try {
      await invoke("image_delete_os", { image_id: imageId })
      await fetchOsImages()
      if (selectedOsImage === imageId) {
        setSelectedOsImage("")
      }
      return true
    } catch (e) {
      setVmError(String(e))
      return false
    }
  }, [fetchOsImages, selectedOsImage])

  const addPortForward = useCallback(async () => {
    if (!pfVmId || pfHostPort === "" || pfGuestPort === "") return
    setVmError("")
    try {
      await invoke("vm_port_forward_add", {
        vm_id: pfVmId,
        host_port: pfHostPort,
        guest_port: pfGuestPort,
        protocol: pfProtocol || "tcp",
      })
      setPfHostPort(8080)
      setPfGuestPort(80)
      await fetchVms()
      return true
    } catch (e) {
      setVmError(String(e))
      return false
    }
  }, [pfVmId, pfHostPort, pfGuestPort, pfProtocol, fetchVms])

  const removePortForward = useCallback(async (vmId: string, hostPort: number) => {
    setVmError("")
    try {
      await invoke("vm_port_forward_remove", { vm_id: vmId, host_port: hostPort })
      await fetchVms()
      return true
    } catch (e) {
      setVmError(String(e))
      return false
    }
  }, [fetchVms])

  const running = vms.filter(v => v.state === "running")

  return {
    vms, running, vmLoading, vmError, setVmError,
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
    pfVmId, setPfVmId,
    pfHostPort, setPfHostPort,
    pfGuestPort, setPfGuestPort,
    pfProtocol, setPfProtocol,
    fetchVms, vmAction, createVm,
    getLoginCmd, addMount, removeMount,
    // OS images
    osImages, selectedOsImage, setSelectedOsImage,
    downloadingImage, downloadProgress,
    downloadOsImage, deleteOsImage, fetchOsImages,
    addPortForward, removePortForward,
  }
}
