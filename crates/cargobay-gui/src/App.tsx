import { useState, useEffect } from "react"
import { messages } from "./i18n/messages"
import { I } from "./icons"
import { useContainers } from "./hooks/useContainers"
import { useVms } from "./hooks/useVms"
import { useImageSearch } from "./hooks/useImageSearch"
import { useToast } from "./hooks/useToast"
import { useModal } from "./hooks/useModal"
import { AppModal } from "./components/AppModal"
import { EmptyState } from "./components/EmptyState"
import { Dashboard } from "./pages/Dashboard"
import { Containers } from "./pages/Containers"
import { Images } from "./pages/Images"
import { Vms } from "./pages/Vms"
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
  const vms = useVms()
  const images = useImageSearch()

  const copyText = async (text: string) => {
    try { await navigator.clipboard.writeText(text); showToast(t("copied")) }
    catch { showToast(t("copyFailed")) }
  }

  const navItems: { page: NavPage; icon: React.ReactNode; count?: number; soon?: boolean }[] = [
    { page: "dashboard", icon: I.dashboard },
    { page: "containers", icon: I.box, count: containers.containers.length },
    { page: "vms", icon: I.server },
    { page: "images", icon: I.layers },
  ]

  const pageNames: Record<NavPage, string> = {
    dashboard: t("dashboard"), containers: t("containers"),
    vms: t("vms"), images: t("images"), settings: t("settings"),
  }

  const renderPage = () => {
    switch (activePage) {
      case "dashboard":
        return (
          <Dashboard
            containers={containers.containers}
            running={containers.running}
            vmsCount={vms.vms.length}
            vmsRunningCount={vms.vms.filter(v => v.state === "running").length}
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
            onClear={images.clearResults}
            onCopy={copyText}
            t={t}
          />
        )
      case "vms":
        return (
          <Vms
            {...vms}
            onFetchVms={vms.fetchVms}
            onVmAction={vms.vmAction}
            onCreateVm={async () => {
              const ok = await vms.createVm()
              if (ok) showToast(t("done"))
            }}
            onLoginCmd={async (vm) => {
              const cmd = await vms.getLoginCmd(vm)
              if (cmd) modal.openTextModal(t("loginCommand"), cmd, cmd)
            }}
            onAddMount={async () => {
              const ok = await vms.addMount()
              if (ok) showToast(t("done"))
            }}
            onRemoveMount={async (vmId, tag) => {
              const ok = await vms.removeMount(vmId, tag)
              if (ok) showToast(t("done"))
            }}
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
