import { useState, useEffect } from "react"
import { messages } from "./i18n/messages"
import { I } from "./icons"
import { useContainers } from "./hooks/useContainers"
import { useImageSearch } from "./hooks/useImageSearch"
import { useVms } from "./hooks/useVms"
import { useVolumes } from "./hooks/useVolumes"
import { useToast } from "./hooks/useToast"
import { useModal } from "./hooks/useModal"
import { AppModal } from "./components/AppModal"
import { EmptyState } from "./components/EmptyState"
import { Dashboard } from "./pages/Dashboard"
import { Containers } from "./pages/Containers"
import { Images } from "./pages/Images"
import { Vms } from "./pages/VMs"
import { Volumes } from "./pages/Volumes"
import { Settings } from "./pages/Settings"
import type { NavPage, Theme } from "./types"
import "./App.css"

function App() {
  const [activePage, setActivePage] = useState<NavPage>("dashboard")
  const [theme, setTheme] = useState<Theme>(() => (localStorage.getItem("theme") as Theme) || "dark")
  const normalizeLang = (value: string | null) => (value === "zh" ? "zh" : "en")
  const [lang, setLang] = useState(() => normalizeLang(localStorage.getItem("lang")))

  const t = (key: string) => messages[lang]?.[key] || messages.en[key] || key

  useEffect(() => { localStorage.setItem("theme", theme) }, [theme])
  useEffect(() => { localStorage.setItem("lang", lang) }, [lang])

  const { toast, showToast } = useToast()
  const modal = useModal(t)
  const containers = useContainers()
  const images = useImageSearch()
  const vmHook = useVms()
  const volumeHook = useVolumes()

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
  ]

  const pageNames: Record<NavPage, string> = {
    dashboard: t("dashboard"), containers: t("containers"),
    vms: t("vms"), images: t("images"), volumes: t("volumes"), settings: t("settings"),
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
            connected={containers.connected}
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
            }}
            onLoginCmd={async (vm) => {
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
            onDownloadOsImage={async (imageId) => {
              const ok = await vmHook.downloadOsImage(imageId)
              if (ok) showToast(t("osImageDownloaded"))
            }}
            onDeleteOsImage={async (imageId) => {
              const ok = await vmHook.deleteOsImage(imageId)
              if (ok) showToast(t("osImageDeleted"))
            }}
            onAddPortForward={vmHook.addPortForward}
            onRemovePortForward={vmHook.removePortForward}
            t={t}
          />
        )
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
    <div className={`app ${theme === "light" ? "light" : ""}`}>
      <AppModal
        {...modal}
        onClose={modal.closeModal}
        onCopy={copyText}
        onOpenTextModal={modal.openTextModal}
        onError={containers.setError}
        onToast={showToast}
        t={t}
      />
      {toast && <div className="toast">{toast}</div>}
      <div className="sidebar">
        <div className="sidebar-header">
          <img src="/logo.png" alt={t("appName")} />
          <span className="brand-name">{t("appName")}</span>
          <span className="brand-version">v0.1</span>
        </div>
        <div className="sidebar-nav">
          {navItems.map(item => (
            <div
              key={item.page}
              className={`nav-item ${activePage === item.page ? "active" : ""}`}
              onClick={() => setActivePage(item.page)}
            >
              <span className="nav-icon">{item.icon}</span>
              <span className="nav-label">{pageNames[item.page]}</span>
              {item.count != null && item.count > 0 && <span className="nav-count">{item.count}</span>}
              {item.soon && <span className="nav-badge">{t("soon")}</span>}
            </div>
          ))}
          <div style={{ flex: 1 }} />
          <div
            className={`nav-item ${activePage === "settings" ? "active" : ""}`}
            onClick={() => setActivePage("settings")}
          >
            <span className="nav-icon">{I.settings}</span>
            <span className="nav-label">{t("settings")}</span>
          </div>
        </div>
      </div>

      <div className="main">
        <div className="topbar">
          <div className="topbar-left">
            <h1>{pageNames[activePage]}</h1>
            {activePage === "containers" && containers.running.length > 0 && (
              <span className="count-chip">{containers.running.length} {t("runningCount")}</span>
            )}
            {activePage === "vms" && vmHook.running.length > 0 && (
              <span className="count-chip">{vmHook.running.length} {t("runningCount")}</span>
            )}
          </div>
          <div className="topbar-right">
            <div className="status-pill">
              <span className={`dot ${containers.connected ? "on" : "off"}`} />
              {containers.connected ? t("connected") : t("disconnected")}
            </div>
          </div>
        </div>
        <div className="content">
          {renderPage()}
        </div>
      </div>
    </div>
  )
}

export default App
