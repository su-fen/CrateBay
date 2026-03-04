export interface ContainerInfo {
  id: string
  name: string
  image: string
  state: string
  status: string
  ports: string
}

export interface EnvVar {
  key: string
  value: string
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
  os_image: string | null
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

export interface VolumeInfo {
  name: string
  driver: string
  mountpoint: string
  created_at: string
  labels: Record<string, string>
  options: Record<string, string>
  scope: string
}

export interface LocalImageInfo {
  id: string
  repo_tags: string[]
  size_bytes: number
  size_human: string
  created: number
}

export interface ImageInspectInfo {
  id: string
  repo_tags: string[]
  size_bytes: number
  created: string
  architecture: string
  os: string
  docker_version: string
  layers: number
}

export type NavPage = "dashboard" | "containers" | "vms" | "images" | "volumes" | "kubernetes" | "settings"
export type Theme = "dark" | "light" | "system"
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

export interface K3sStatusDto {
  installed: boolean
  running: boolean
  version: string
  node_count: number
  kubeconfig_path: string
}

export interface K8sPod {
  name: string
  namespace: string
  status: string
  ready: string
  restarts: number
  age: string
}

export interface K8sService {
  name: string
  namespace: string
  service_type: string
  cluster_ip: string
  ports: string
}

export interface K8sDeployment {
  name: string
  namespace: string
  ready: string
  up_to_date: number
  available: number
  age: string
}
