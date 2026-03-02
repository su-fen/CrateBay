export interface ContainerInfo {
  id: string
  name: string
  image: string
  state: string
  status: string
  ports: string
}

export interface ImageSearchResult {
  source: string
  reference: string
  description: string
  stars?: number
  pulls?: number
  official: boolean
}

export interface RunContainerResult {
  id: string
  name: string
  image: string
  login_cmd: string
}

export interface SharedDirectoryDto {
  tag: string
  host_path: string
  guest_path: string
  read_only: boolean
}

export interface PortForwardDto {
  host_port: number
  guest_port: number
  protocol: string
}

export interface VmInfoDto {
  id: string
  name: string
  state: string
  cpus: number
  memory_mb: number
  disk_gb: number
  rosetta_enabled: boolean
  mounts: SharedDirectoryDto[]
  port_forwards: PortForwardDto[]
}

export interface OsImageDto {
  id: string
  name: string
  version: string
  arch: string
  size_bytes: number
  status: "not_downloaded" | "downloading" | "ready"
  default_cmdline: string
}

export interface OsImageDownloadProgressDto {
  image_id: string
  current_file: string
  bytes_downloaded: number
  bytes_total: number
  done: boolean
  error: string | null
}

export type NavPage = "dashboard" | "containers" | "vms" | "images" | "settings"
export type Theme = "dark" | "light"
export type ModalKind = "" | "text" | "package"

export interface ContainerGroup {
  key: string
  containers: ContainerInfo[]
  runningCount: number
  stoppedCount: number
}

export interface ContainerStats {
  cpu_percent: number
  memory_usage_mb: number
  memory_limit_mb: number
  memory_percent: number
  network_rx_bytes: number
  network_tx_bytes: number
}

export interface VmStats {
  cpu_percent: number
  memory_usage_mb: number
  disk_usage_gb: number
}
