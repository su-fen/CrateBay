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

export interface VmInfoDto {
  id: string
  name: string
  state: string
  cpus: number
  memory_mb: number
  disk_gb: number
  rosetta_enabled: boolean
  mounts: SharedDirectoryDto[]
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
