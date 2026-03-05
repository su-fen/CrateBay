import { useState, useEffect, useCallback } from "react"
import { invoke } from "@tauri-apps/api/core"
import { getCurrentWindow } from "@tauri-apps/api/window"
import { messages } from "./i18n/messages"
import { I } from "./icons"
import { useContainers } from "./hooks/useContainers"
import { useImageSearch } from "./hooks/useImageSearch"
import { useVms } from "./hooks/useVms"
import { useVolumes } from "./hooks/useVolumes"
import { useModal } from "./hooks/useModal"
import { AppModal } from "./components/AppModal"
import { EmptyState } from "./components/EmptyState"
import { UpdateChecker } from "./components/UpdateChecker"
import { toast } from "sonner"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { ScrollArea } from "@/components/ui/scroll-area"
import { Toaster } from "@/components/ui/sonner"
import { cn } from "@/lib/utils"
import { Dashboard } from "./pages/Dashboard"
import { Containers } from "./pages/Containers"
import { Images } from "./pages/Images"
import { Vms } from "./pages/Vms"
import { Volumes } from "./pages/Volumes"
import { Settings } from "./pages/Settings"
import { Kubernetes } from "./pages/Kubernetes"
import type { NavPage, Theme, VmInfoDto, LocalImageInfo } from "./types"

function App() {
  const [activePage, setActivePage] = useState<NavPage>("dashboard")
  const [theme, setTheme] = useState<Theme>(() => (localStorage.getItem("theme") as Theme) || "system")
  const normalizeLang = (value: string | null) => (value === "zh" ? "zh" : "en")
  const [lang, setLang] = useState(() => normalizeLang(localStorage.getItem("lang")))

  const t = (key: string) => messages[lang]?.[key] || messages.en[key] || key

  // Resolve effective theme (dark/light) from preference (which may be "system")
  const getEffective = (pref: Theme): "dark" | "light" => {
    if (pref === "system") return window.matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light"
    return pref
  }
  const [effective, setEffective] = useState<"dark" | "light">(() => getEffective(theme))

  useEffect(() => {
    localStorage.setItem("theme", theme)
    setEffective(getEffective(theme))

    // Listen for OS theme changes when in "system" mode
    if (theme === "system") {
      const mq = window.matchMedia("(prefers-color-scheme: dark)")
      const handler = (e: MediaQueryListEvent) => setEffective(e.matches ? "dark" : "light")
      mq.addEventListener("change", handler)
      return () => mq.removeEventListener("change", handler)
    }
  }, [theme])

  useEffect(() => {
    document.documentElement.classList.toggle("dark", effective === "dark")
    document.documentElement.style.colorScheme = effective
    // Sync native window theme (affects macOS title bar color)
    invoke("set_window_theme", { theme: effective }).catch(() => {})
  }, [effective])
  useEffect(() => { localStorage.setItem("lang", lang) }, [lang])

  const showToast = useCallback((msg: string) => toast(msg), [])
  const modal = useModal(t)
  const containers = useContainers()
  const images = useImageSearch()
  const vmHook = useVms()
  const volumeHook = useVolumes()

  // Window controls (Windows: right-side buttons; macOS: left-side traffic lights)
  const isWindows = navigator.userAgent.includes("Windows")
  const appWindow = getCurrentWindow()
  const [maximized, setMaximized] = useState(false)

  // Restore saved window size/position on startup
  const MIN_WIDTH = 1100
  const MIN_HEIGHT = 650
  useEffect(() => {
    const restore = async () => {
      try {
        const saved = localStorage.getItem("windowState")
        if (!saved) return
        const { width, height, x, y, maximized: wasMax } = JSON.parse(saved)
        if (wasMax) {
          await appWindow.maximize()
        } else if (width && height) {
          const { LogicalSize, LogicalPosition } = await import("@tauri-apps/api/dpi")
          await appWindow.setSize(new LogicalSize(
            Math.max(width, MIN_WIDTH),
            Math.max(height, MIN_HEIGHT),
          ))
          if (x != null && y != null) {
            await appWindow.setPosition(new LogicalPosition(x, y))
          }
        }
      } catch { /* ignore restore errors */ }
    }
    restore()
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [])

  // Save window size/position on resize, track maximized state, enforce min size
  useEffect(() => {
    let saveTimer: ReturnType<typeof setTimeout> | null = null
    const unlisten = appWindow.onResized(async () => {
      const isMax = await appWindow.isMaximized()
      setMaximized(isMax)

      // Enforce minimum window size (decorations:false may not honor minWidth/minHeight)
      if (!isMax) {
        try {
          const size = await appWindow.innerSize()
          const factor = await appWindow.scaleFactor()
          const logicalW = Math.round(size.width / factor)
          const logicalH = Math.round(size.height / factor)
          if (logicalW < MIN_WIDTH || logicalH < MIN_HEIGHT) {
            const { LogicalSize } = await import("@tauri-apps/api/dpi")
            await appWindow.setSize(new LogicalSize(
              Math.max(logicalW, MIN_WIDTH),
              Math.max(logicalH, MIN_HEIGHT),
            ))
            return // skip save — will fire another resize
          }
        } catch { /* ignore */ }
      }

      // Debounce saving to avoid excessive writes
      if (saveTimer) clearTimeout(saveTimer)
      saveTimer = setTimeout(async () => {
        try {
          if (isMax) {
            localStorage.setItem("windowState", JSON.stringify({ maximized: true }))
          } else {
            const size = await appWindow.innerSize()
            const pos = await appWindow.outerPosition()
            const factor = await appWindow.scaleFactor()
            localStorage.setItem("windowState", JSON.stringify({
              width: Math.round(size.width / factor),
              height: Math.round(size.height / factor),
              x: Math.round(pos.x / factor),
              y: Math.round(pos.y / factor),
              maximized: false,
            }))
          }
        } catch { /* ignore */ }
      }, 500)
    })
    return () => {
      unlisten.then(f => f())
      if (saveTimer) clearTimeout(saveTimer)
    }
  }, [appWindow])

  const handleMinimize = useCallback(() => appWindow.minimize(), [appWindow])
  const handleMaximize = useCallback(() => appWindow.toggleMaximize(), [appWindow])
  const handleClose = useCallback(() => appWindow.close(), [appWindow])

  // Installed (local) Docker images count for Dashboard
  const [installedImagesCount, setInstalledImagesCount] = useState(0)
  useEffect(() => {
    let cancelled = false
    const poll = async () => {
      try {
        const result = await invoke<LocalImageInfo[]>("image_list")
        if (!cancelled) setInstalledImagesCount(result.length)
      } catch { /* Docker may not be running */ }
    }
    poll()
    const iv = setInterval(poll, 10000)
    return () => { cancelled = true; clearInterval(iv) }
  }, [])

  const copyText = async (text: string) => {
    try { await navigator.clipboard.writeText(text); showToast(t("copied")) }
    catch { showToast(t("copyFailed")) }
  }

  const navItems: { page: NavPage; icon: React.ReactNode; count?: number; soon?: boolean }[] = [
    { page: "dashboard", icon: I.dashboard },
    { page: "containers", icon: I.box, count: containers.containers.length },
    { page: "images", icon: I.layers },
    { page: "volumes", icon: I.hardDrive, count: volumeHook.volumes.length },
    { page: "vms", icon: I.server, count: vmHook.vms.length },
    { page: "kubernetes", icon: I.kubernetes },
  ]

  const pageNames: Record<NavPage, string> = {
    dashboard: t("dashboard"), containers: t("containers"),
    vms: t("vms"), images: t("images"), volumes: t("volumes"),
    kubernetes: t("kubernetes"), settings: t("settings"),
  }

  const renderPage = () => {
    switch (activePage) {
      case "dashboard":
        return (
          <Dashboard
            containers={containers.containers}
            running={containers.running}
            vmsCount={vmHook.vms.length}
            vmsRunningCount={vmHook.running.length}
            runningVms={vmHook.running}
            imgResultsCount={images.imgResults.length}
            installedImagesCount={installedImagesCount}
            volumesCount={volumeHook.volumes.length}
            onNavigate={setActivePage}
            t={t}
          />
        )
      case "containers":
        return (
          <Containers
            containers={containers.containers}
            groups={containers.groups}
            loading={containers.loading}
            error={containers.error}
            acting={containers.acting}
            expandedGroups={containers.expandedGroups}
            onContainerAction={containers.containerAction}
            onToggleGroup={containers.toggleGroup}
            onOpenTextModal={modal.openTextModal}
            onOpenPackageModal={modal.openPackageModal}
            onFetch={containers.fetchContainers}
            onRun={async (image: string, name: string, cpus: number | "", mem: number | "", pull: boolean, env?: string[]) => {
              const result = await images.doRunDirect(image, name, cpus, mem, pull, containers.fetchContainers, env)
              if (result) showToast(t("containerCreated"))
              return result
            }}
            t={t}
          />
        )
      case "images":
        return (
          <Images
            {...images}
            onSearch={images.doSearch}
            onTags={images.doTags}
            onRun={async () => {
              const result = await images.doRun(containers.fetchContainers)
              if (result) showToast(t("containerCreated"))
            }}
            onLoad={async () => {
              const out = await images.doLoad()
              if (out != null) {
                modal.openTextModal(t("imageLoaded"), out || t("done"), out || t("done"))
                showToast(t("done"))
              }
            }}
            onPush={async () => {
              const out = await images.doPush()
              if (out != null) {
                modal.openTextModal(t("imagePushed"), out || t("done"), out || t("done"))
                showToast(t("done"))
              }
            }}
            onCopy={copyText}
            t={t}
          />
        )
      case "volumes":
        return (
          <Volumes
            volumes={volumeHook.volumes}
            loading={volumeHook.loading}
            error={volumeHook.error}
            onFetch={volumeHook.fetchVolumes}
            onCreate={volumeHook.createVolume}
            onInspect={volumeHook.inspectVolume}
            onRemove={volumeHook.removeVolume}
            onToast={showToast}
            t={t}
          />
        )
      case "vms":
        return (
          <Vms
            vms={vmHook.vms}
            vmLoading={vmHook.vmLoading}
            vmError={vmHook.vmError}
            setVmError={vmHook.setVmError}
            vmName={vmHook.vmName}
            setVmName={vmHook.setVmName}
            vmCpus={vmHook.vmCpus}
            setVmCpus={vmHook.setVmCpus}
            vmMem={vmHook.vmMem}
            setVmMem={vmHook.setVmMem}
            vmDisk={vmHook.vmDisk}
            setVmDisk={vmHook.setVmDisk}
            vmRosetta={vmHook.vmRosetta}
            setVmRosetta={vmHook.setVmRosetta}
            vmActing={vmHook.vmActing}
            vmLoginUser={vmHook.vmLoginUser}
            setVmLoginUser={vmHook.setVmLoginUser}
            vmLoginHost={vmHook.vmLoginHost}
            setVmLoginHost={vmHook.setVmLoginHost}
            vmLoginPort={vmHook.vmLoginPort}
            setVmLoginPort={vmHook.setVmLoginPort}
            mountVmId={vmHook.mountVmId}
            setMountVmId={vmHook.setMountVmId}
            mountTag={vmHook.mountTag}
            setMountTag={vmHook.setMountTag}
            mountHostPath={vmHook.mountHostPath}
            setMountHostPath={vmHook.setMountHostPath}
            mountGuestPath={vmHook.mountGuestPath}
            setMountGuestPath={vmHook.setMountGuestPath}
            mountReadonly={vmHook.mountReadonly}
            setMountReadonly={vmHook.setMountReadonly}
            pfVmId={vmHook.pfVmId}
            setPfVmId={vmHook.setPfVmId}
            pfHostPort={vmHook.pfHostPort}
            setPfHostPort={vmHook.setPfHostPort}
            pfGuestPort={vmHook.pfGuestPort}
            setPfGuestPort={vmHook.setPfGuestPort}
            pfProtocol={vmHook.pfProtocol}
            setPfProtocol={vmHook.setPfProtocol}
            onFetchVms={vmHook.fetchVms}
            onVmAction={vmHook.vmAction}
            onCreateVm={async () => {
              const ok = await vmHook.createVm()
              if (ok) showToast(t("vmCreated"))
              return Boolean(ok)
            }}
            onLoginCmd={async (vm: VmInfoDto) => {
              const cmd = await vmHook.getLoginCmd(vm)
              if (cmd) modal.openTextModal(t("loginCommand"), cmd, cmd)
            }}
            onAddMount={vmHook.addMount}
            onRemoveMount={vmHook.removeMount}
            osImages={vmHook.osImages}
            selectedOsImage={vmHook.selectedOsImage}
            setSelectedOsImage={vmHook.setSelectedOsImage}
            downloadingImage={vmHook.downloadingImage}
            downloadProgress={vmHook.downloadProgress}
            onDownloadOsImage={async (imageId: string) => {
              const ok = await vmHook.downloadOsImage(imageId)
              if (ok) showToast(t("osImageDownloaded"))
            }}
            onDeleteOsImage={async (imageId: string) => {
              const ok = await vmHook.deleteOsImage(imageId)
              if (ok) showToast(t("osImageDeleted"))
            }}
            onAddPortForward={vmHook.addPortForward}
            onRemovePortForward={vmHook.removePortForward}
            t={t}
          />
        )
      case "kubernetes":
        return <Kubernetes t={t} />
      case "settings":
        return (
          <Settings
            theme={theme}
            setTheme={setTheme}
            lang={lang}
            setLang={(v) => setLang(normalizeLang(v))}
            t={t}
          />
        )
      default:
        return (
          <EmptyState
            icon={I.plus}
            title={t("comingSoon")}
            description={`${pageNames[activePage]} ${t("underDev")}`}
          />
        )
    }
  }

  return (
    <div className={cn("h-full w-full flex", isWindows && "platform-windows")}>
      <AppModal
        {...modal}
        onClose={modal.closeModal}
        onCopy={copyText}
        onOpenTextModal={modal.openTextModal}
        onError={containers.setError}
        onToast={showToast}
        t={t}
      />
      <Toaster theme={effective} />
      <UpdateChecker t={t} />
      <aside className="w-[220px] shrink-0 border-r border-border/70 bg-card flex flex-col select-none">
        <div className="h-[42px] px-4 flex items-center gap-2.5 border-b border-border/70" data-tauri-drag-region>
          <img
            src="/logo.png"
            alt={t("appName")}
            className="size-8 rounded-lg"
          />
          <span className="text-[15px] font-bold tracking-tight text-foreground">
            {t("appName")}
          </span>
          <Badge
            variant="secondary"
            className="ml-auto rounded-md px-1.5 py-0 text-[10px] text-muted-foreground"
          >
            v1.0.0
          </Badge>
        </div>

        <ScrollArea className="flex-1 px-2 py-3">
          <nav className="flex flex-col gap-1">
            {navItems.map((item) => (
              <Button
                key={item.page}
                type="button"
                variant="ghost"
                size="sm"
                data-testid={`nav-${item.page}`}
                className={cn(
                  "w-full justify-start gap-2 px-3 text-sm font-medium text-muted-foreground hover:text-foreground",
                  activePage === item.page && "bg-primary/10 text-primary hover:bg-primary/15"
                )}
                onClick={() => setActivePage(item.page)}
              >
                <span className="size-5 shrink-0 flex items-center justify-center [&>svg]:size-[18px] [&>svg]:fill-none [&>svg]:stroke-current [&>svg]:stroke-2 [&>svg]:stroke-linecap-round [&>svg]:stroke-linejoin-round">
                  {item.icon}
                </span>
                <span className="flex-1 text-left">
                  {pageNames[item.page]}
                </span>
                {item.count != null && item.count > 0 && (
                  <Badge
                    variant="secondary"
                    className="rounded-md border border-brand-cyan/15 bg-brand-cyan/10 px-1.5 py-0 text-[11px] text-brand-cyan"
                  >
                    {item.count}
                  </Badge>
                )}
                {item.soon && (
                  <Badge
                    variant="secondary"
                    className="rounded-md px-1.5 py-0 text-[10px] text-muted-foreground"
                  >
                    {t("soon")}
                  </Badge>
                )}
              </Button>
            ))}
          </nav>
        </ScrollArea>

        <div className="px-2 pb-2">
          <Button
            type="button"
            variant="ghost"
            size="sm"
            data-testid="nav-settings"
            className={cn(
              "w-full justify-start gap-2 px-3 text-sm font-medium text-muted-foreground hover:text-foreground",
              activePage === "settings" && "bg-primary/10 text-primary hover:bg-primary/15"
            )}
            onClick={() => setActivePage("settings")}
          >
            <span className="size-5 shrink-0 flex items-center justify-center [&>svg]:size-[18px] [&>svg]:fill-none [&>svg]:stroke-current [&>svg]:stroke-2 [&>svg]:stroke-linecap-round [&>svg]:stroke-linejoin-round">
              {I.settings}
            </span>
            <span className="flex-1 text-left">
              {t("settings")}
            </span>
          </Button>
        </div>
      </aside>

      <div className="flex-1 min-w-0 flex flex-col">
        <header className="h-[42px] px-6 flex items-center justify-between border-b border-border/70 bg-card" data-tauri-drag-region>
          <div className="flex items-center gap-3">
            <h1 className="text-base font-bold text-foreground">
              {pageNames[activePage]}
            </h1>
            {activePage === "containers" && containers.running.length > 0 && (
              <Badge
                variant="secondary"
                className="rounded-md border border-primary/15 bg-primary/10 text-primary"
              >
                {containers.running.length} {t("runningCount")}
              </Badge>
            )}
            {activePage === "vms" && vmHook.running.length > 0 && (
              <Badge
                variant="secondary"
                className="rounded-md border border-primary/15 bg-primary/10 text-primary"
              >
                {vmHook.running.length} {t("runningCount")}
              </Badge>
            )}
          </div>

          <div className="flex items-center gap-2">
            <Badge
              variant="secondary"
              className={cn(
                "rounded-full gap-2 px-3 py-1 text-xs font-medium text-muted-foreground border border-border/60 bg-popover/40"
              )}
            >
              <span
                className={cn(
                  "size-1.5 rounded-full",
                  containers.connected
                    ? "bg-brand-green shadow-[0_0_10px_hsl(var(--brand-green)/0.6)]"
                    : "bg-destructive"
                )}
              />
              {containers.connected ? t("connected") : t("disconnected")}
            </Badge>

            {isWindows && (
              <div className="flex items-center gap-1">
                <Button
                  type="button"
                  variant="ghost"
                  size="icon-xs"
                  onClick={handleMinimize}
                  aria-label="Minimize"
                >
                  {I.winMinimize}
                </Button>
                <Button
                  type="button"
                  variant="ghost"
                  size="icon-xs"
                  onClick={handleMaximize}
                  aria-label={maximized ? "Restore" : "Maximize"}
                >
                  {maximized ? I.winRestore : I.winMaximize}
                </Button>
                <Button
                  type="button"
                  variant="ghost"
                  size="icon-xs"
                  className="hover:bg-destructive/10 hover:text-destructive"
                  onClick={handleClose}
                  aria-label="Close"
                >
                  {I.winClose}
                </Button>
              </div>
            )}
          </div>
        </header>

        <div className="flex-1 overflow-hidden min-[1600px]:max-w-[1440px] min-[1600px]:mx-auto w-full">
          <div
            key={activePage}
            className="h-full overflow-auto px-6 py-5 animate-in fade-in-0 slide-in-from-top-1 duration-200"
          >
            {renderPage()}
          </div>
        </div>
      </div>
    </div>
  )
}

export default App
