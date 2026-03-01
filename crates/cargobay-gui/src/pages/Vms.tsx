import { I } from "../icons"
import { ErrorInline } from "../components/ErrorDisplay"
import type { VmInfoDto } from "../types"

interface VmsProps {
  vms: VmInfoDto[]
  vmLoading: boolean
  vmError: string
  setVmError: (v: string) => void
  vmName: string
  setVmName: (v: string) => void
  vmCpus: number
  setVmCpus: (v: number) => void
  vmMem: number
  setVmMem: (v: number) => void
  vmDisk: number
  setVmDisk: (v: number) => void
  vmRosetta: boolean
  setVmRosetta: (v: boolean) => void
  vmActing: string
  vmLoginUser: string
  setVmLoginUser: (v: string) => void
  vmLoginHost: string
  setVmLoginHost: (v: string) => void
  vmLoginPort: number | ""
  setVmLoginPort: (v: number | "") => void
  mountVmId: string
  setMountVmId: (v: string) => void
  mountTag: string
  setMountTag: (v: string) => void
  mountHostPath: string
  setMountHostPath: (v: string) => void
  mountGuestPath: string
  setMountGuestPath: (v: string) => void
  mountReadonly: boolean
  setMountReadonly: (v: boolean) => void
  onFetchVms: () => void
  onVmAction: (cmd: string, id: string) => void
  onCreateVm: () => void
  onLoginCmd: (vm: VmInfoDto) => void
  onAddMount: () => void
  onRemoveMount: (vmId: string, tag: string) => void
  t: (key: string) => string
}

export function Vms({
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
  onFetchVms, onVmAction, onCreateVm,
  onLoginCmd, onAddMount, onRemoveMount, t,
}: VmsProps) {
  return (
    <div className="page">
      <div className="toolbar">
        <button className="btn" onClick={onFetchVms} disabled={vmLoading}>
          <span className="icon">{I.refresh}</span>{vmLoading ? t("loading") : t("refresh")}
        </button>
        <div className="hint" style={{ marginLeft: 8 }}>{t("vmHint")}</div>
      </div>

      {vmError && <ErrorInline message={vmError} onDismiss={() => setVmError("")} />}

      {/* VM list first for better information hierarchy */}
      <div className="panel">
        <div className="panel-title"><div className="panel-title-icon">{I.server}</div>{t("vms")}</div>
        {vms.length === 0 ? (
          <div className="hint">{t("noVms")}</div>
        ) : (
          vms.map(vm => (
            <div className="container-card" key={vm.id}>
              <div className={`card-icon ${vm.state === "running" ? "" : "stopped"}`}>{I.server}</div>
              <div className="card-body">
                <div className="card-name">{vm.name} <span className="sub">({vm.id})</span></div>
                <div className="card-meta">
                  {t("state")}: {vm.state} · {vm.cpus} {t("cpus")} · {vm.memory_mb} {t("memoryMb")} · {vm.rosetta_enabled ? t("rosettaOn") : t("rosettaOff")}
                </div>
                {vm.mounts?.length > 0 && (
                  <div className="mounts">
                    {vm.mounts.map(m => (
                      <div className="mount" key={`${vm.id}-${m.tag}`}>
                        <span className="mono">{m.tag}</span>
                        <span className="muted">{m.host_path} → {m.guest_path} ({m.read_only ? "ro" : "rw"})</span>
                        <button className="btn tiny" onClick={() => onRemoveMount(vm.id, m.tag)}>{t("remove")}</button>
                      </div>
                    ))}
                  </div>
                )}
              </div>
              <div className="card-status">
                <span className={`dot ${vm.state === "running" ? "running" : "stopped"}`} />
                <span>{vm.state}</span>
              </div>
              <div className="card-actions">
                {vm.state === "running" ? (
                  <button className="action-btn" disabled={vmActing === vm.id} onClick={() => onVmAction("vm_stop", vm.id)} title={t("stop")}>{I.stop}</button>
                ) : (
                  <button className="action-btn" disabled={vmActing === vm.id} onClick={() => onVmAction("vm_start", vm.id)} title={t("start")}>{I.play}</button>
                )}
                <button className="action-btn" disabled={vmActing === vm.id} onClick={() => onLoginCmd(vm)} title={t("loginCommand")}>{I.terminal}</button>
                <button className="action-btn danger" disabled={vmActing === vm.id} onClick={() => onVmAction("vm_delete", vm.id)} title={t("delete")}>{I.trash}</button>
              </div>
            </div>
          ))
        )}
      </div>

      <div className="grid2">
        <div className="panel">
          <div className="panel-title"><div className="panel-title-icon">{I.plus}</div>{t("createVm")}</div>
          <div className="form">
            <div className="row">
              <label>{t("name")}</label>
              <input className="input" value={vmName} onChange={e => setVmName(e.target.value)} placeholder="myvm" />
            </div>
            <div className="row two">
              <div>
                <label>{t("cpus")}</label>
                <input className="input" type="number" min={1} value={vmCpus} onChange={e => setVmCpus(Number(e.target.value) || 2)} />
              </div>
              <div>
                <label>{t("memoryMb")}</label>
                <input className="input" type="number" min={256} value={vmMem} onChange={e => setVmMem(Number(e.target.value) || 2048)} />
              </div>
            </div>
            <div className="row">
              <label>{t("diskGb")}</label>
              <input className="input" type="number" min={10} value={vmDisk} onChange={e => setVmDisk(Number(e.target.value) || 20)} />
            </div>
            <div className="row inline">
              <input type="checkbox" checked={vmRosetta} onChange={e => setVmRosetta(e.target.checked)} />
              <span>{t("enableRosetta")}</span>
            </div>
            <div className="row">
              <button className="btn primary" disabled={vmActing === "create" || !vmName.trim()} onClick={onCreateVm}>
                {vmActing === "create" ? t("creating") : t("create")}
              </button>
            </div>
          </div>
        </div>

        <div className="panel">
          <div className="panel-title"><div className="panel-title-icon">{I.terminal}</div>{t("loginCommand")}</div>
          <div className="form">
            <div className="row two">
              <div>
                <label>{t("user")}</label>
                <input className="input" value={vmLoginUser} onChange={e => setVmLoginUser(e.target.value)} />
              </div>
              <div>
                <label>{t("host")}</label>
                <input className="input" value={vmLoginHost} onChange={e => setVmLoginHost(e.target.value)} />
              </div>
            </div>
            <div className="row">
              <label>{t("port")}</label>
              <input className="input" type="number" min={1} value={vmLoginPort} onChange={e => setVmLoginPort(e.target.value === "" ? "" : Number(e.target.value))} />
            </div>
            <div className="hint">{t("vmLoginHint")}</div>
          </div>
        </div>
      </div>

      <div className="panel">
        <div className="panel-title"><div className="panel-title-icon">{I.layers}</div>{t("virtiofs")}</div>
        <div className="form">
          <div className="row">
            <label>{t("vm")}</label>
            <select className="select" value={mountVmId} onChange={e => setMountVmId(e.target.value)}>
              <option value="">{t("selectVm")}</option>
              {vms.map(vm => <option key={vm.id} value={vm.id}>{vm.name} ({vm.id})</option>)}
            </select>
          </div>
          <div className="row two">
            <div>
              <label>{t("tag")}</label>
              <input className="input" value={mountTag} onChange={e => setMountTag(e.target.value)} placeholder="code" />
            </div>
            <div>
              <label>{t("guestPath")}</label>
              <input className="input" value={mountGuestPath} onChange={e => setMountGuestPath(e.target.value)} placeholder="/mnt/code" />
            </div>
          </div>
          <div className="row">
            <label>{t("hostPath")}</label>
            <input className="input" value={mountHostPath} onChange={e => setMountHostPath(e.target.value)} placeholder="~/code" />
          </div>
          <div className="row inline">
            <input type="checkbox" checked={mountReadonly} onChange={e => setMountReadonly(e.target.checked)} />
            <span>{t("readOnly")}</span>
          </div>
          <div className="row">
            <button className="btn" onClick={onAddMount} disabled={!mountVmId || !mountTag.trim() || !mountHostPath.trim()}>
              {t("addMount")}
            </button>
          </div>
          <div className="hint">{t("virtiofsHint")}</div>
        </div>
      </div>
    </div>
  )
}
