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

export type NavPage =
  | "dashboard"
  | "containers"
  | "vms"
  | "images"
  | "volumes"
  | "kubernetes"
  | "ai"
  | "settings"
export type Theme = "dark" | "light" | "system"
export type ModalKind = "" | "text" | "package"

export interface AiProviderProfile {
  id: string
  provider_id: string
  display_name: string
  model: string
  base_url: string
  api_key_ref: string
  headers: Record<string, string>
}

export interface AiSecurityPolicy {
  destructive_action_confirmation: boolean
  mcp_remote_enabled: boolean
  mcp_allowed_actions: string[]
  mcp_auth_token_ref: string
  mcp_audit_enabled: boolean
  cli_command_allowlist: string[]
}

export interface AiSkillDefinition {
  id: string
  display_name: string
  description: string
  tags: string[]
  executor: string
  target: string
  input_schema: Record<string, unknown>
  enabled: boolean
}

export interface AiSettings {
  profiles: AiProviderProfile[]
  active_profile_id: string
  skills: AiSkillDefinition[]
  security_policy: AiSecurityPolicy
}

export interface AiProfileValidationResult {
  ok: boolean
  message: string
}

export interface AiChatMessage {
  role: "system" | "user" | "assistant"
  content: string
}

export interface AiUsage {
  prompt_tokens?: number
  completion_tokens?: number
  total_tokens?: number
}

export interface AiToolCall {
  name: string
  arguments: Record<string, unknown>
}

export interface AiChatResponse {
  request_id: string
  provider_id: string
  model: string
  text: string
  usage?: AiUsage
  tool_calls: AiToolCall[]
  error_type?: string
}

export interface AiConnectionTestResult {
  ok: boolean
  request_id: string
  message: string
  error_type?: string
  latency_ms: number
}

export interface DockerRuntimeSetupResult {
  ok: boolean
  request_id: string
  message: string
}

export interface AssistantPlanStep {
  id: string
  title: string
  command: string
  args: Record<string, unknown>
  risk_level: "read" | "write" | "destructive"
  requires_confirmation: boolean
  explain: string
}

export interface AssistantPlanResult {
  request_id: string
  strategy: string
  notes: string
  fallback_used: boolean
  steps: AssistantPlanStep[]
}

export interface AssistantStepExecutionResult {
  ok: boolean
  request_id: string
  command: string
  risk_level: "read" | "write" | "destructive" | string
  output: unknown
}

export interface AgentCliPreset {
  id: string
  name: string
  description: string
  command: string
  args_template: string[]
  timeout_sec: number
  dangerous: boolean
}

export interface AgentCliRunResult {
  ok: boolean
  request_id: string
  command_line: string
  exit_code: number
  stdout: string
  stderr: string
  duration_ms: number
}

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

export interface OllamaStatusDto {
  installed: boolean
  running: boolean
  version: string
  base_url: string
}

export interface OllamaModelDto {
  name: string
  size_bytes: number
  size_human: string
  modified_at: string
  digest: string
  family: string
  parameter_size: string
  quantization_level: string
}

export interface SandboxTemplateDto {
  id: string
  name: string
  description: string
  image: string
  default_command: string
  cpu_default: number
  memory_mb_default: number
  ttl_hours_default: number
  tags: string[]
}

export interface SandboxCreateRequest {
  template_id: string
  name?: string | null
  image?: string | null
  command?: string | null
  env?: string[] | null
  cpu_cores?: number | null
  memory_mb?: number | null
  ttl_hours?: number | null
  owner?: string | null
}

export interface SandboxCreateResultDto {
  id: string
  short_id: string
  name: string
  image: string
  login_cmd: string
}

export interface SandboxInfoDto {
  id: string
  short_id: string
  name: string
  image: string
  state: string
  status: string
  template_id: string
  owner: string
  created_at: string
  expires_at: string
  ttl_hours: number
  cpu_cores: number
  memory_mb: number
  is_expired: boolean
}

export interface SandboxInspectDto {
  id: string
  short_id: string
  name: string
  image: string
  template_id: string
  owner: string
  created_at: string
  expires_at: string
  ttl_hours: number
  cpu_cores: number
  memory_mb: number
  running: boolean
  command: string
  env: string[]
}

export interface SandboxAuditEventDto {
  timestamp: string
  action: string
  sandbox_id: string
  sandbox_name: string
  level: string
  detail: string
}
