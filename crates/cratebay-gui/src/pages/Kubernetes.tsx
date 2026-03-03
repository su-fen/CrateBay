import { useState, useEffect, useCallback } from "react"
import { invoke } from "@tauri-apps/api/core"
import { I } from "../icons"
import { ErrorInline } from "../components/ErrorDisplay"
import type { K3sStatusDto, K8sPod, K8sService, K8sDeployment } from "../types"

interface KubernetesProps {
  t: (key: string) => string
}

type K8sTab = "overview" | "pods" | "services" | "deployments"

export function Kubernetes({ t }: KubernetesProps) {
  // K3s cluster state
  const [status, setStatus] = useState<K3sStatusDto | null>(null)
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState("")
  const [acting, setActing] = useState("")

  // K8s dashboard state
  const [tab, setTab] = useState<K8sTab>("overview")
  const [namespace, setNamespace] = useState("")
  const [namespaces, setNamespaces] = useState<string[]>([])
  const [pods, setPods] = useState<K8sPod[]>([])
  const [services, setServices] = useState<K8sService[]>([])
  const [deployments, setDeployments] = useState<K8sDeployment[]>([])
  const [k8sLoading, setK8sLoading] = useState(false)
  const [k8sError, setK8sError] = useState("")

  // Pod logs modal
  const [logPod, setLogPod] = useState<K8sPod | null>(null)
  const [podLogs, setPodLogs] = useState("")
  const [logsLoading, setLogsLoading] = useState(false)

  // ── K3s status polling ──────────────────────────────────────────────
  const fetchStatus = useCallback(async () => {
    try {
      const s = await invoke<K3sStatusDto>("k3s_status")
      setStatus(s)
      setError("")
    } catch (e: unknown) {
      setError(String(e))
    } finally {
      setLoading(false)
    }
  }, [])

  useEffect(() => {
    fetchStatus()
    const iv = setInterval(fetchStatus, 5000)
    return () => clearInterval(iv)
  }, [fetchStatus])

  const doAction = async (action: string, label: string) => {
    setActing(label)
    setError("")
    try {
      await invoke(action)
      await fetchStatus()
    } catch (e: unknown) {
      setError(String(e))
    } finally {
      setActing("")
    }
  }

  // ── K8s dashboard data fetching ─────────────────────────────────────
  const fetchK8sData = useCallback(async () => {
    if (!status?.running) return
    setK8sLoading(true)
    setK8sError("")
    try {
      const [ns, p, s, d] = await Promise.all([
        invoke<string[]>("k8s_list_namespaces"),
        invoke<K8sPod[]>("k8s_list_pods", { namespace: namespace || null }),
        invoke<K8sService[]>("k8s_list_services", { namespace: namespace || null }),
        invoke<K8sDeployment[]>("k8s_list_deployments", { namespace: namespace || null }),
      ])
      setNamespaces(ns)
      setPods(p)
      setServices(s)
      setDeployments(d)
    } catch (e: unknown) {
      setK8sError(String(e))
    } finally {
      setK8sLoading(false)
    }
  }, [status?.running, namespace])

  useEffect(() => {
    if (status?.running) {
      fetchK8sData()
      const iv = setInterval(fetchK8sData, 10000)
      return () => clearInterval(iv)
    }
  }, [fetchK8sData, status?.running])

  const fetchPodLogs = async (pod: K8sPod) => {
    setLogPod(pod)
    setLogsLoading(true)
    setPodLogs("")
    try {
      const logs = await invoke<string>("k8s_pod_logs", {
        name: pod.name,
        namespace: pod.namespace,
        tail: 200,
      })
      setPodLogs(logs)
    } catch (e: unknown) {
      setPodLogs(`Error: ${String(e)}`)
    } finally {
      setLogsLoading(false)
    }
  }

  const isInstalled = status?.installed ?? false
  const isRunning = status?.running ?? false

  return (
    <div className="page">
      {/* Toolbar */}
      <div className="toolbar">
        <button className="btn" onClick={fetchStatus} disabled={loading}>
          <span className="icon">{I.refresh}</span>{loading ? t("loading") : t("refresh")}
        </button>
        {isRunning && (
          <>
            <div style={{ borderLeft: "1px solid var(--border, #333)", height: 20, margin: "0 8px" }} />
            <select
              value={namespace}
              onChange={(e) => setNamespace(e.target.value)}
              style={{
                background: "var(--input-bg, #2a2a3e)",
                color: "var(--text, #eee)",
                border: "1px solid var(--border, #333)",
                borderRadius: 6,
                padding: "4px 8px",
                fontSize: 13,
              }}
            >
              <option value="">{t("allNamespaces")}</option>
              {namespaces.map((ns) => (
                <option key={ns} value={ns}>{ns}</option>
              ))}
            </select>
          </>
        )}
        <div style={{ flex: 1 }} />
      </div>

      {error && <ErrorInline message={error} onDismiss={() => setError("")} />}
      {k8sError && <ErrorInline message={k8sError} onDismiss={() => setK8sError("")} />}

      {/* K3s Cluster Status Card */}
      <div style={{
        background: "var(--card-bg, #1a1a2e)",
        border: "1px solid var(--border, #333)",
        borderRadius: 12,
        padding: 24,
        marginBottom: 20,
      }}>
        <div style={{ display: "flex", alignItems: "center", gap: 12, marginBottom: 20 }}>
          <span style={{ width: 32, height: 32, display: "flex", alignItems: "center", justifyContent: "center", opacity: 0.8 }}>{I.kubernetes}</span>
          <h2 style={{ margin: 0, fontSize: 20 }}>{t("k3sCluster")}</h2>
          <span className={`dot ${isRunning ? "running" : "stopped"}`} style={{ marginLeft: 8 }} />
          <span style={{ fontSize: 13, opacity: 0.7 }}>
            {isRunning ? t("running") : t("stopped")}
          </span>
        </div>

        <div className="vm-stats-grid" style={{ marginBottom: 20 }}>
          <div className="vm-stat-card">
            <div className="vm-stat-label">{t("clusterStatus")}</div>
            <div className="vm-stat-value">
              {isInstalled ? (
                <span style={{ color: "var(--green, #4caf50)" }}>{t("installed")}</span>
              ) : (
                <span style={{ opacity: 0.5 }}>{t("notInstalled")}</span>
              )}
            </div>
          </div>
          <div className="vm-stat-card">
            <div className="vm-stat-label">{t("k3sVersion")}</div>
            <div className="vm-stat-value mono">
              {status?.version || "-"}
            </div>
          </div>
          <div className="vm-stat-card">
            <div className="vm-stat-label">{t("nodeCount")}</div>
            <div className="vm-stat-value">
              {isRunning ? status?.node_count ?? 0 : "-"}
            </div>
          </div>
          <div className="vm-stat-card">
            <div className="vm-stat-label">{t("kubeconfig")}</div>
            <div className="vm-stat-value mono" style={{ fontSize: 11, wordBreak: "break-all" }}>
              {status?.kubeconfig_path || "-"}
            </div>
          </div>
        </div>

        {/* Action Buttons */}
        <div style={{ display: "flex", gap: 8, flexWrap: "wrap" }}>
          {!isInstalled && (
            <button
              className="btn primary"
              disabled={!!acting}
              onClick={() => doAction("k3s_install", "install")}
            >
              <span className="icon">{I.plus}</span>
              {acting === "install" ? t("working") : t("installK3s")}
            </button>
          )}
          {isInstalled && !isRunning && (
            <button
              className="btn primary"
              disabled={!!acting}
              onClick={() => doAction("k3s_start", "start")}
            >
              <span className="icon">{I.play}</span>
              {acting === "start" ? t("working") : t("startCluster")}
            </button>
          )}
          {isRunning && (
            <button
              className="btn"
              disabled={!!acting}
              onClick={() => doAction("k3s_stop", "stop")}
            >
              <span className="icon">{I.stop}</span>
              {acting === "stop" ? t("working") : t("stopCluster")}
            </button>
          )}
          {isInstalled && (
            <button
              className="btn"
              disabled={!!acting || isRunning}
              onClick={() => doAction("k3s_uninstall", "uninstall")}
              title={isRunning ? t("stopCluster") : ""}
            >
              <span className="icon">{I.trash}</span>
              {acting === "uninstall" ? t("working") : t("uninstallK3s")}
            </button>
          )}
        </div>
      </div>

      {/* K8s Dashboard (only when cluster is running) */}
      {isRunning && (
        <div style={{
          background: "var(--card-bg, #1a1a2e)",
          border: "1px solid var(--border, #333)",
          borderRadius: 12,
          padding: 24,
        }}>
          {/* Tabs */}
          <div style={{ display: "flex", gap: 4, marginBottom: 20, borderBottom: "1px solid var(--border, #333)", paddingBottom: 8 }}>
            {(["overview", "pods", "services", "deployments"] as K8sTab[]).map((t2) => (
              <button
                key={t2}
                className={`btn ${tab === t2 ? "primary" : ""}`}
                style={{ fontSize: 13, padding: "6px 14px" }}
                onClick={() => setTab(t2)}
              >
                {t(t2)}
              </button>
            ))}
            <div style={{ flex: 1 }} />
            {k8sLoading && <span style={{ fontSize: 12, opacity: 0.5 }}>{t("loading")}</span>}
          </div>

          {/* Overview Tab */}
          {tab === "overview" && (
            <div className="vm-stats-grid">
              <div className="vm-stat-card" style={{ cursor: "pointer" }} onClick={() => setTab("pods")}>
                <div className="vm-stat-label">{t("pods")}</div>
                <div className="vm-stat-value">{pods.length}</div>
              </div>
              <div className="vm-stat-card" style={{ cursor: "pointer" }} onClick={() => setTab("services")}>
                <div className="vm-stat-label">{t("services")}</div>
                <div className="vm-stat-value">{services.length}</div>
              </div>
              <div className="vm-stat-card" style={{ cursor: "pointer" }} onClick={() => setTab("deployments")}>
                <div className="vm-stat-label">{t("deployments")}</div>
                <div className="vm-stat-value">{deployments.length}</div>
              </div>
              <div className="vm-stat-card">
                <div className="vm-stat-label">{t("namespace")}</div>
                <div className="vm-stat-value">{namespaces.length}</div>
              </div>
            </div>
          )}

          {/* Pods Tab */}
          {tab === "pods" && (
            <div style={{ overflowX: "auto" }}>
              {pods.length === 0 ? (
                <p style={{ textAlign: "center", opacity: 0.5 }}>{t("noPods")}</p>
              ) : (
                <table className="data-table" style={{ width: "100%", borderCollapse: "collapse", fontSize: 13 }}>
                  <thead>
                    <tr style={{ borderBottom: "1px solid var(--border, #333)" }}>
                      <th style={thStyle}>{t("name")}</th>
                      <th style={thStyle}>{t("namespace")}</th>
                      <th style={thStyle}>{t("status")}</th>
                      <th style={thStyle}>{t("ready")}</th>
                      <th style={thStyle}>{t("restarts")}</th>
                      <th style={thStyle}>{t("age")}</th>
                      <th style={thStyle}>{t("actions")}</th>
                    </tr>
                  </thead>
                  <tbody>
                    {pods.map((pod) => (
                      <tr key={`${pod.namespace}/${pod.name}`} style={{ borderBottom: "1px solid var(--border, #222)" }}>
                        <td style={tdStyle} className="mono">{pod.name}</td>
                        <td style={tdStyle}>{pod.namespace}</td>
                        <td style={tdStyle}>
                          <span style={{ color: pod.status === "Running" ? "var(--green, #4caf50)" : pod.status === "Pending" ? "var(--yellow, #ff9800)" : "var(--red, #f44336)" }}>
                            {pod.status}
                          </span>
                        </td>
                        <td style={tdStyle}>{pod.ready}</td>
                        <td style={tdStyle}>{pod.restarts}</td>
                        <td style={tdStyle}>{pod.age}</td>
                        <td style={tdStyle}>
                          <button className="btn" style={{ fontSize: 11, padding: "2px 8px" }} onClick={() => fetchPodLogs(pod)}>
                            {t("logs")}
                          </button>
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              )}
            </div>
          )}

          {/* Services Tab */}
          {tab === "services" && (
            <div style={{ overflowX: "auto" }}>
              {services.length === 0 ? (
                <p style={{ textAlign: "center", opacity: 0.5 }}>{t("noServices")}</p>
              ) : (
                <table className="data-table" style={{ width: "100%", borderCollapse: "collapse", fontSize: 13 }}>
                  <thead>
                    <tr style={{ borderBottom: "1px solid var(--border, #333)" }}>
                      <th style={thStyle}>{t("name")}</th>
                      <th style={thStyle}>{t("namespace")}</th>
                      <th style={thStyle}>{t("type")}</th>
                      <th style={thStyle}>{t("clusterIp")}</th>
                      <th style={thStyle}>{t("ports")}</th>
                    </tr>
                  </thead>
                  <tbody>
                    {services.map((svc) => (
                      <tr key={`${svc.namespace}/${svc.name}`} style={{ borderBottom: "1px solid var(--border, #222)" }}>
                        <td style={tdStyle} className="mono">{svc.name}</td>
                        <td style={tdStyle}>{svc.namespace}</td>
                        <td style={tdStyle}>{svc.service_type}</td>
                        <td style={tdStyle} className="mono">{svc.cluster_ip}</td>
                        <td style={tdStyle} className="mono">{svc.ports}</td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              )}
            </div>
          )}

          {/* Deployments Tab */}
          {tab === "deployments" && (
            <div style={{ overflowX: "auto" }}>
              {deployments.length === 0 ? (
                <p style={{ textAlign: "center", opacity: 0.5 }}>{t("noDeployments")}</p>
              ) : (
                <table className="data-table" style={{ width: "100%", borderCollapse: "collapse", fontSize: 13 }}>
                  <thead>
                    <tr style={{ borderBottom: "1px solid var(--border, #333)" }}>
                      <th style={thStyle}>{t("name")}</th>
                      <th style={thStyle}>{t("namespace")}</th>
                      <th style={thStyle}>{t("ready")}</th>
                      <th style={thStyle}>{t("upToDate")}</th>
                      <th style={thStyle}>{t("available")}</th>
                      <th style={thStyle}>{t("age")}</th>
                    </tr>
                  </thead>
                  <tbody>
                    {deployments.map((dep) => (
                      <tr key={`${dep.namespace}/${dep.name}`} style={{ borderBottom: "1px solid var(--border, #222)" }}>
                        <td style={tdStyle} className="mono">{dep.name}</td>
                        <td style={tdStyle}>{dep.namespace}</td>
                        <td style={tdStyle}>{dep.ready}</td>
                        <td style={tdStyle}>{dep.up_to_date}</td>
                        <td style={tdStyle}>{dep.available}</td>
                        <td style={tdStyle}>{dep.age}</td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              )}
            </div>
          )}
        </div>
      )}

      {/* Pod Logs Modal */}
      {logPod && (
        <div className="modal-overlay" onClick={() => setLogPod(null)}>
          <div
            className="modal"
            style={{ maxWidth: 800, width: "90%" }}
            onClick={(e) => e.stopPropagation()}
          >
            <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center", marginBottom: 12 }}>
              <h3 style={{ margin: 0, fontSize: 16 }}>
                {t("podLogs")}: {logPod.name}
              </h3>
              <button className="btn" onClick={() => setLogPod(null)}>
                {t("close")}
              </button>
            </div>
            <pre
              style={{
                background: "var(--bg, #0d0d1a)",
                padding: 16,
                borderRadius: 8,
                maxHeight: 400,
                overflow: "auto",
                fontSize: 12,
                fontFamily: "monospace",
                whiteSpace: "pre-wrap",
                wordBreak: "break-all",
              }}
            >
              {logsLoading ? t("loading") : podLogs || t("noLogs")}
            </pre>
          </div>
        </div>
      )}
    </div>
  )
}

const thStyle: React.CSSProperties = {
  textAlign: "left",
  padding: "8px 12px",
  fontWeight: 600,
  fontSize: 12,
  opacity: 0.7,
  whiteSpace: "nowrap",
}

const tdStyle: React.CSSProperties = {
  padding: "8px 12px",
  whiteSpace: "nowrap",
}
