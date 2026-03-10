use anyhow::{anyhow, Context as _, Result};
use bollard::container::{
    Config, CreateContainerOptions, InspectContainerOptions, ListContainersOptions, LogOutput,
    RemoveContainerOptions, StartContainerOptions, StopContainerOptions,
};
use bollard::exec::{CreateExecOptions, StartExecResults};
use bollard::image::CreateImageOptions;
use bollard::service::HostConfig;
use bollard::Docker;
use futures_util::stream::TryStreamExt;
use modelcontextprotocol_server::mcp_protocol::types::tool::{ToolCallResult, ToolContent};
use modelcontextprotocol_server::transport::StdioTransport;
use modelcontextprotocol_server::ServerBuilder;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashMap;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use tracing::{info, warn};

const SANDBOX_LABEL_MANAGED: &str = "com.cratebay.sandbox.managed";
const SANDBOX_LABEL_TEMPLATE_ID: &str = "com.cratebay.sandbox.template_id";
const SANDBOX_LABEL_OWNER: &str = "com.cratebay.sandbox.owner";
const SANDBOX_LABEL_CREATED_AT: &str = "com.cratebay.sandbox.created_at";
const SANDBOX_LABEL_EXPIRES_AT: &str = "com.cratebay.sandbox.expires_at";
const SANDBOX_LABEL_TTL_HOURS: &str = "com.cratebay.sandbox.ttl_hours";
const SANDBOX_LABEL_CPU_CORES: &str = "com.cratebay.sandbox.cpu_cores";
const SANDBOX_LABEL_MEMORY_MB: &str = "com.cratebay.sandbox.memory_mb";
static SANDBOX_SEQ: AtomicU64 = AtomicU64::new(1);

#[derive(Clone)]
struct AppContext {
    workspace_root: Option<PathBuf>,
}

impl AppContext {
    fn from_env() -> Result<Self> {
        let workspace_root = std::env::var("CRATEBAY_MCP_WORKSPACE_ROOT")
            .ok()
            .map(|raw| raw.trim().to_string())
            .filter(|v| !v.is_empty())
            .map(|raw| {
                let path = PathBuf::from(raw);
                path.canonicalize()
                    .context("canonicalize CRATEBAY_MCP_WORKSPACE_ROOT")
            })
            .transpose()?;
        Ok(Self { workspace_root })
    }
}

#[derive(Debug, Clone, Serialize)]
struct SandboxTemplateDto {
    id: String,
    name: String,
    description: String,
    image: String,
    default_command: String,
    cpu_default: u32,
    memory_mb_default: u64,
    ttl_hours_default: u32,
    tags: Vec<String>,
}

#[derive(Debug, Clone)]
struct SandboxTemplateDef {
    id: &'static str,
    name: &'static str,
    description: &'static str,
    image: &'static str,
    default_command: &'static str,
    cpu_default: u32,
    memory_mb_default: u64,
    ttl_hours_default: u32,
    tags: &'static [&'static str],
    default_env: &'static [&'static str],
}

fn sandbox_template_defs() -> Vec<SandboxTemplateDef> {
    vec![
        SandboxTemplateDef {
            id: "node-dev",
            name: "Node.js Dev",
            description: "Node development runtime for coding agents and MCP tasks",
            image: "node:22-bookworm",
            default_command: "sleep infinity",
            cpu_default: 2,
            memory_mb_default: 2048,
            ttl_hours_default: 8,
            tags: &["node", "javascript", "typescript"],
            default_env: &["CRATEBAY_SANDBOX=1", "NODE_ENV=development"],
        },
        SandboxTemplateDef {
            id: "python-dev",
            name: "Python Dev",
            description: "Python runtime for agent tools, scripts, and notebooks",
            image: "python:3.12-bookworm",
            default_command: "sleep infinity",
            cpu_default: 2,
            memory_mb_default: 3072,
            ttl_hours_default: 8,
            tags: &["python", "llm-tools", "automation"],
            default_env: &["CRATEBAY_SANDBOX=1", "PYTHONUNBUFFERED=1"],
        },
        SandboxTemplateDef {
            id: "rust-dev",
            name: "Rust Dev",
            description: "Rust toolchain runtime for compile/test oriented agent workflows",
            image: "rust:1.77-bookworm",
            default_command: "sleep infinity",
            cpu_default: 2,
            memory_mb_default: 4096,
            ttl_hours_default: 8,
            tags: &["rust", "cargo", "systems"],
            default_env: &["CRATEBAY_SANDBOX=1", "CARGO_TERM_COLOR=always"],
        },
    ]
}

fn sandbox_find_template(template_id: &str) -> Option<SandboxTemplateDef> {
    sandbox_template_defs()
        .into_iter()
        .find(|it| it.id == template_id)
}

fn sandbox_templates_catalog() -> Vec<SandboxTemplateDto> {
    sandbox_template_defs()
        .into_iter()
        .map(|it| SandboxTemplateDto {
            id: it.id.to_string(),
            name: it.name.to_string(),
            description: it.description.to_string(),
            image: it.image.to_string(),
            default_command: it.default_command.to_string(),
            cpu_default: it.cpu_default,
            memory_mb_default: it.memory_mb_default,
            ttl_hours_default: it.ttl_hours_default,
            tags: it.tags.iter().map(|v| v.to_string()).collect(),
        })
        .collect()
}

fn sandbox_short_id(id: &str) -> String {
    id.chars().take(12).collect::<String>()
}

fn sandbox_is_managed(labels: &HashMap<String, String>) -> bool {
    labels
        .get(SANDBOX_LABEL_MANAGED)
        .map(|v| v == "true")
        .unwrap_or(false)
}

fn sandbox_is_expired(expires_at: &str) -> bool {
    chrono::DateTime::parse_from_rfc3339(expires_at)
        .map(|dt| dt.with_timezone(&chrono::Utc) <= chrono::Utc::now())
        .unwrap_or(false)
}

fn sandbox_default_owner() -> String {
    std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .unwrap_or_else(|_| "local-user".to_string())
}

fn sandbox_normalize_owner(owner: Option<String>) -> String {
    let mut value = owner
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .unwrap_or_else(sandbox_default_owner);
    if value.len() > 64 {
        value = value.chars().take(64).collect();
    }
    value
}

fn sandbox_normalize_env(env: Option<Vec<String>>) -> Result<Vec<String>> {
    let mut out = Vec::new();
    for item in env.unwrap_or_default() {
        let trimmed = item.trim();
        if trimmed.is_empty() {
            continue;
        }
        if trimmed.len() > 512 {
            return Err(anyhow!("sandbox env entry is too long (max 512 chars)"));
        }
        if trimmed.contains('\0') {
            return Err(anyhow!("sandbox env entry contains null byte"));
        }
        if !trimmed.contains('=') {
            return Err(anyhow!(
                "sandbox env entry '{}' must follow KEY=VALUE",
                trimmed
            ));
        }
        let key = trimmed.split('=').next().unwrap_or_default().trim();
        if key.is_empty() {
            return Err(anyhow!("sandbox env entry '{}' has empty key", trimmed));
        }
        if !key
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '-')
        {
            return Err(anyhow!(
                "sandbox env key '{}' contains invalid characters",
                key
            ));
        }
        out.push(trimmed.to_string());
    }
    if out.len() > 64 {
        return Err(anyhow!("sandbox env has too many entries (max 64)"));
    }
    Ok(out)
}

fn sandbox_generate_name(template_id: &str) -> String {
    let suffix = SANDBOX_SEQ.fetch_add(1, Ordering::Relaxed) % 10_000;
    let stamp = chrono::Utc::now().format("%m%d%H%M%S");
    let mut name = format!("cbx-{}-{}-{:04}", template_id, stamp, suffix);
    if name.len() > 128 {
        name = name.chars().take(128).collect();
    }
    name
}

fn sandbox_audit_path() -> PathBuf {
    cratebay_core::config_dir()
        .join("audit")
        .join("sandboxes.jsonl")
}

fn sandbox_audit_log(
    action: &str,
    sandbox_id: &str,
    sandbox_name: &str,
    level: &str,
    detail: &str,
) {
    let path = sandbox_audit_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let event = json!({
        "timestamp": chrono::Utc::now().to_rfc3339(),
        "action": action,
        "sandbox_id": sandbox_id,
        "sandbox_name": sandbox_name,
        "level": level,
        "detail": cratebay_core::validation::sanitize_log_string(detail),
    });
    if let Ok(line) = serde_json::to_string(&event) {
        if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
            let _ = writeln!(file, "{}", line);
        }
    }
}

#[cfg(unix)]
fn detect_docker_socket() -> Option<String> {
    let home = std::env::var("HOME").unwrap_or_default();
    let candidates = [
        format!("{}/.colima/default/docker.sock", home),
        format!("{}/.orbstack/run/docker.sock", home),
        "/var/run/docker.sock".to_string(),
        format!("{}/.docker/run/docker.sock", home),
    ];
    candidates
        .into_iter()
        .find(|p| std::path::Path::new(p).exists())
}

fn connect_docker() -> Result<Docker> {
    if std::env::var("DOCKER_HOST").is_ok() {
        Docker::connect_with_local_defaults()
            .map_err(|e| anyhow!("Failed to connect via DOCKER_HOST: {}", e))
    } else {
        #[cfg(unix)]
        {
            if let Some(sock) = detect_docker_socket() {
                Docker::connect_with_socket(&sock, 120, bollard::API_DEFAULT_VERSION)
                    .map_err(|e| anyhow!("Failed to connect to Docker at {}: {}", sock, e))
            } else {
                Err(anyhow!(
                    "No Docker socket found. Set DOCKER_HOST or start a Docker-compatible runtime."
                ))
            }
        }

        #[cfg(windows)]
        {
            let candidates = [
                r"//./pipe/docker_engine",
                r"//./pipe/dockerDesktopLinuxEngine",
            ];
            candidates
                .iter()
                .find_map(|pipe| {
                    Docker::connect_with_named_pipe(pipe, 120, bollard::API_DEFAULT_VERSION).ok()
                })
                .ok_or_else(|| {
                    anyhow!(
                        "No Docker named pipe found. Set DOCKER_HOST or start a Docker-compatible runtime."
                    )
                })
        }

        #[cfg(not(any(unix, windows)))]
        {
            Docker::connect_with_local_defaults()
                .map_err(|e| anyhow!("Failed to connect to Docker: {}", e))
        }
    }
}

fn split_image_reference(reference: &str) -> (String, String) {
    let no_digest = reference.split('@').next().unwrap_or(reference);
    let last_slash = no_digest.rfind('/').unwrap_or(0);
    let last_colon = no_digest.rfind(':');

    if let Some(colon_idx) = last_colon {
        if colon_idx > last_slash {
            let image = &no_digest[..colon_idx];
            let tag = &no_digest[(colon_idx + 1)..];
            if !image.is_empty() && !tag.is_empty() {
                return (image.to_string(), tag.to_string());
            }
        }
    }

    (no_digest.to_string(), "latest".to_string())
}

async fn docker_pull_image(docker: &Docker, reference: &str) -> Result<()> {
    let (from_image, tag) = split_image_reference(reference);
    let opts = CreateImageOptions {
        from_image,
        tag,
        ..Default::default()
    };

    let mut stream = docker.create_image(Some(opts), None, None);
    while let Some(_progress) = stream
        .try_next()
        .await
        .map_err(|e| anyhow!(e.to_string()))?
    {}
    Ok(())
}

fn docker_host_for_docker_cli() -> Option<String> {
    if let Ok(v) = std::env::var("DOCKER_HOST") {
        return Some(v);
    }
    #[cfg(unix)]
    {
        detect_docker_socket().map(|sock| format!("unix://{}", sock))
    }
    #[cfg(not(unix))]
    {
        None
    }
}

fn resolve_workspace_root(ctx: &AppContext) -> Result<PathBuf> {
    ctx.workspace_root
        .clone()
        .ok_or_else(|| anyhow!("CRATEBAY_MCP_WORKSPACE_ROOT is not configured"))
}

fn resolve_workspace_path_for_read(ctx: &AppContext, user_path: &str) -> Result<PathBuf> {
    let root = resolve_workspace_root(ctx)?;
    let user_path = user_path.trim();
    if user_path.is_empty() {
        return Err(anyhow!("local_path is required"));
    }
    let candidate = {
        let p = PathBuf::from(user_path);
        if p.is_absolute() {
            p
        } else {
            root.join(p)
        }
    };
    let canon = candidate
        .canonicalize()
        .with_context(|| format!("resolve local path {}", candidate.display()))?;
    if !canon.starts_with(&root) {
        return Err(anyhow!(
            "local path is outside workspace root: {}",
            canon.display()
        ));
    }
    Ok(canon)
}

fn resolve_workspace_path_for_write(ctx: &AppContext, user_path: &str) -> Result<PathBuf> {
    let root = resolve_workspace_root(ctx)?;
    let user_path = user_path.trim();
    if user_path.is_empty() {
        return Err(anyhow!("local_path is required"));
    }
    let candidate = {
        let p = PathBuf::from(user_path);
        if p.is_absolute() {
            p
        } else {
            root.join(p)
        }
    };

    if candidate
        .components()
        .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        return Err(anyhow!(
            "local_path must not contain '..' traversal components"
        ));
    }
    if !candidate.starts_with(&root) {
        return Err(anyhow!(
            "local path is outside workspace root: {}",
            candidate.display()
        ));
    }
    Ok(candidate)
}

fn require_confirmed(confirmed: Option<bool>, hint: &str) -> Result<()> {
    if confirmed != Some(true) {
        return Err(anyhow!(
            "This tool requires explicit confirmation: set confirmed=true ({})",
            hint
        ));
    }
    Ok(())
}

fn ok_text(text: impl Into<String>) -> ToolCallResult {
    ToolCallResult {
        content: vec![ToolContent::Text { text: text.into() }],
        is_error: None,
    }
}

fn ok_json(value: serde_json::Value) -> ToolCallResult {
    let text = serde_json::to_string_pretty(&value).unwrap_or_else(|_| value.to_string());
    ok_text(text)
}

fn err_text(text: impl Into<String>) -> ToolCallResult {
    ToolCallResult {
        content: vec![ToolContent::Text { text: text.into() }],
        is_error: Some(true),
    }
}

fn run_async<T>(future: impl std::future::Future<Output = Result<T>> + Send) -> Result<T>
where
    T: Send,
{
    tokio::task::block_in_place(|| tokio::runtime::Handle::current().block_on(future))
}

#[derive(Debug, Serialize)]
struct SandboxInfoDto {
    id: String,
    short_id: String,
    name: String,
    image: String,
    state: String,
    status: String,
    template_id: String,
    owner: String,
    created_at: String,
    expires_at: String,
    ttl_hours: u32,
    cpu_cores: u32,
    memory_mb: u64,
    is_expired: bool,
}

#[derive(Debug, Serialize)]
struct SandboxInspectDto {
    id: String,
    short_id: String,
    name: String,
    image: String,
    template_id: String,
    owner: String,
    created_at: String,
    expires_at: String,
    ttl_hours: u32,
    cpu_cores: u32,
    memory_mb: u64,
    running: bool,
    command: String,
    env: Vec<String>,
}

async fn sandbox_list() -> Result<Vec<SandboxInfoDto>> {
    let docker = connect_docker()?;
    let opts = ListContainersOptions::<String> {
        all: true,
        ..Default::default()
    };
    let containers = docker.list_containers(Some(opts)).await?;

    let mut sandboxes = containers
        .into_iter()
        .filter_map(|item| {
            let labels = item.labels.unwrap_or_default();
            if !sandbox_is_managed(&labels) {
                return None;
            }

            let id = item.id.unwrap_or_default();
            let short_id = sandbox_short_id(&id);
            let name = item
                .names
                .unwrap_or_default()
                .first()
                .cloned()
                .unwrap_or_default()
                .trim_start_matches('/')
                .to_string();

            let template_id = labels
                .get(SANDBOX_LABEL_TEMPLATE_ID)
                .cloned()
                .unwrap_or_else(|| "custom".to_string());
            let owner = labels
                .get(SANDBOX_LABEL_OWNER)
                .cloned()
                .unwrap_or_else(sandbox_default_owner);
            let created_at = labels
                .get(SANDBOX_LABEL_CREATED_AT)
                .cloned()
                .unwrap_or_default();
            let expires_at = labels
                .get(SANDBOX_LABEL_EXPIRES_AT)
                .cloned()
                .unwrap_or_default();
            let ttl_hours = labels
                .get(SANDBOX_LABEL_TTL_HOURS)
                .and_then(|v| v.parse::<u32>().ok())
                .unwrap_or(8);
            let cpu_cores = labels
                .get(SANDBOX_LABEL_CPU_CORES)
                .and_then(|v| v.parse::<u32>().ok())
                .unwrap_or(2);
            let memory_mb = labels
                .get(SANDBOX_LABEL_MEMORY_MB)
                .and_then(|v| v.parse::<u64>().ok())
                .unwrap_or(2048);

            Some(SandboxInfoDto {
                id,
                short_id,
                name,
                image: item.image.unwrap_or_default(),
                state: item.state.unwrap_or_default(),
                status: item.status.unwrap_or_default(),
                template_id,
                owner,
                created_at: created_at.clone(),
                expires_at: expires_at.clone(),
                ttl_hours,
                cpu_cores,
                memory_mb,
                is_expired: sandbox_is_expired(&expires_at),
            })
        })
        .collect::<Vec<_>>();
    sandboxes.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    Ok(sandboxes)
}

async fn sandbox_inspect(id: &str) -> Result<SandboxInspectDto> {
    let docker = connect_docker()?;
    let inspect = docker
        .inspect_container(id, None::<InspectContainerOptions>)
        .await?;
    let labels = inspect
        .config
        .as_ref()
        .and_then(|cfg| cfg.labels.clone())
        .unwrap_or_default();
    if !sandbox_is_managed(&labels) {
        return Err(anyhow!("container is not a CrateBay-managed sandbox"));
    }

    let template_id = labels
        .get(SANDBOX_LABEL_TEMPLATE_ID)
        .cloned()
        .unwrap_or_else(|| "custom".to_string());
    let owner = labels
        .get(SANDBOX_LABEL_OWNER)
        .cloned()
        .unwrap_or_else(sandbox_default_owner);
    let created_at = labels
        .get(SANDBOX_LABEL_CREATED_AT)
        .cloned()
        .unwrap_or_default();
    let expires_at = labels
        .get(SANDBOX_LABEL_EXPIRES_AT)
        .cloned()
        .unwrap_or_default();
    let ttl_hours = labels
        .get(SANDBOX_LABEL_TTL_HOURS)
        .and_then(|v| v.parse::<u32>().ok())
        .unwrap_or(8);
    let cpu_cores = labels
        .get(SANDBOX_LABEL_CPU_CORES)
        .and_then(|v| v.parse::<u32>().ok())
        .unwrap_or(2);
    let memory_mb = labels
        .get(SANDBOX_LABEL_MEMORY_MB)
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(2048);

    let running = inspect
        .state
        .as_ref()
        .and_then(|s| s.running)
        .unwrap_or(false);
    let image = inspect
        .config
        .as_ref()
        .and_then(|cfg| cfg.image.clone())
        .unwrap_or_default();
    let command = inspect
        .config
        .as_ref()
        .and_then(|cfg| cfg.cmd.clone())
        .unwrap_or_default()
        .join(" ");
    let env = inspect
        .config
        .as_ref()
        .and_then(|cfg| cfg.env.clone())
        .unwrap_or_default();
    let name = inspect
        .name
        .unwrap_or_else(|| id.to_string())
        .trim_start_matches('/')
        .to_string();

    Ok(SandboxInspectDto {
        id: id.to_string(),
        short_id: sandbox_short_id(id),
        name,
        image,
        template_id,
        owner,
        created_at,
        expires_at,
        ttl_hours,
        cpu_cores,
        memory_mb,
        running,
        command,
        env,
    })
}

#[derive(Debug, Deserialize)]
struct SandboxInspectArgs {
    id: String,
}

#[derive(Debug, Deserialize)]
struct SandboxExecArgs {
    id: String,
    command: String,
    #[serde(default)]
    timeout_sec: Option<u64>,
    confirmed: Option<bool>,
}

#[derive(Debug, Serialize)]
struct SandboxExecResultDto {
    ok: bool,
    output: String,
    stdout: String,
    stderr: String,
    exit_code: Option<i64>,
    duration_ms: u128,
}

async fn sandbox_exec(id: &str, command: &str, timeout_sec: u64) -> Result<SandboxExecResultDto> {
    let trimmed = command.trim();
    if trimmed.is_empty() {
        return Err(anyhow!("command is required"));
    }
    let docker = connect_docker()?;

    let inspect = docker
        .inspect_container(id, None::<InspectContainerOptions>)
        .await?;
    let labels = inspect
        .config
        .as_ref()
        .and_then(|cfg| cfg.labels.clone())
        .unwrap_or_default();
    if !sandbox_is_managed(&labels) {
        return Err(anyhow!("container is not a CrateBay-managed sandbox"));
    }
    let name = inspect
        .name
        .unwrap_or_else(|| id.to_string())
        .trim_start_matches('/')
        .to_string();

    let exec = docker
        .create_exec(
            id,
            CreateExecOptions {
                attach_stdout: Some(true),
                attach_stderr: Some(true),
                cmd: Some(vec![
                    "/bin/sh".to_string(),
                    "-lc".to_string(),
                    trimmed.to_string(),
                ]),
                ..Default::default()
            },
        )
        .await?;

    let start = std::time::Instant::now();
    let output = tokio::time::timeout(
        Duration::from_secs(timeout_sec.max(1)),
        docker.start_exec(&exec.id, None),
    )
    .await??;

    let mut stdout = String::new();
    let mut stderr = String::new();
    if let StartExecResults::Attached { mut output, .. } = output {
        while let Some(chunk) = output.try_next().await? {
            match chunk {
                LogOutput::StdOut { message } | LogOutput::Console { message } => {
                    stdout.push_str(&String::from_utf8_lossy(&message));
                }
                LogOutput::StdErr { message } => {
                    stderr.push_str(&String::from_utf8_lossy(&message));
                }
                LogOutput::StdIn { .. } => {}
            }
        }
    }

    let exec_inspect = docker.inspect_exec(&exec.id).await?;
    let exit_code = exec_inspect.exit_code;
    let duration_ms = start.elapsed().as_millis();

    sandbox_audit_log(
        "exec",
        &sandbox_short_id(id),
        &name,
        if exit_code.unwrap_or_default() == 0 {
            "ok"
        } else {
            "warn"
        },
        &format!(
            "command_len={} exit_code={}",
            trimmed.len(),
            exit_code.unwrap_or(-1)
        ),
    );

    let output_combined = if stderr.trim().is_empty() {
        stdout.clone()
    } else if stdout.trim().is_empty() {
        stderr.clone()
    } else {
        format!("{}\n{}", stdout, stderr)
    };

    Ok(SandboxExecResultDto {
        ok: exit_code.unwrap_or_default() == 0,
        output: output_combined,
        stdout,
        stderr,
        exit_code,
        duration_ms,
    })
}

#[derive(Debug, Deserialize)]
struct SandboxCleanupArgs {
    confirmed: Option<bool>,
}

#[derive(Debug, Serialize)]
struct SandboxCleanupResultDto {
    removed_count: usize,
    removed_names: Vec<String>,
    message: String,
}

async fn sandbox_cleanup_expired() -> Result<SandboxCleanupResultDto> {
    let docker = connect_docker()?;
    let opts = ListContainersOptions::<String> {
        all: true,
        ..Default::default()
    };
    let containers = docker.list_containers(Some(opts)).await?;

    let mut removed_names = Vec::new();
    for item in containers {
        let id = item.id.unwrap_or_default();
        if id.is_empty() {
            continue;
        }
        let labels = item.labels.unwrap_or_default();
        if !sandbox_is_managed(&labels) {
            continue;
        }
        let expires_at = labels
            .get(SANDBOX_LABEL_EXPIRES_AT)
            .cloned()
            .unwrap_or_default();
        if !sandbox_is_expired(&expires_at) {
            continue;
        }
        let name = item
            .names
            .and_then(|mut names| names.drain(..).next())
            .unwrap_or_else(|| sandbox_short_id(&id));
        let normalized_name = name.trim_start_matches('/').to_string();
        let _ = docker
            .stop_container(&id, Some(StopContainerOptions { t: 5 }))
            .await;
        docker
            .remove_container(
                &id,
                Some(RemoveContainerOptions {
                    force: true,
                    ..Default::default()
                }),
            )
            .await?;
        sandbox_audit_log(
            "cleanup",
            &sandbox_short_id(&id),
            &normalized_name,
            "ok",
            "expired sandbox reclaimed",
        );
        removed_names.push(normalized_name);
    }

    Ok(SandboxCleanupResultDto {
        removed_count: removed_names.len(),
        message: if removed_names.is_empty() {
            "No expired sandboxes found".to_string()
        } else {
            format!("Removed {} expired sandboxes", removed_names.len())
        },
        removed_names,
    })
}

#[derive(Debug, Deserialize)]
struct SandboxDeleteArgs {
    id: String,
    confirmed: Option<bool>,
}

async fn sandbox_delete(id: &str) -> Result<()> {
    let docker = connect_docker()?;
    let inspect = docker
        .inspect_container(id, None::<InspectContainerOptions>)
        .await?;
    let labels = inspect
        .config
        .as_ref()
        .and_then(|cfg| cfg.labels.clone())
        .unwrap_or_default();
    if !sandbox_is_managed(&labels) {
        return Err(anyhow!("container is not a CrateBay-managed sandbox"));
    }
    let name = inspect
        .name
        .unwrap_or_else(|| id.to_string())
        .trim_start_matches('/')
        .to_string();
    let _ = docker
        .stop_container(id, Some(StopContainerOptions { t: 5 }))
        .await;
    docker
        .remove_container(
            id,
            Some(RemoveContainerOptions {
                force: true,
                ..Default::default()
            }),
        )
        .await?;
    sandbox_audit_log("delete", id, &name, "ok", "sandbox removed");
    Ok(())
}

#[derive(Debug, Deserialize)]
struct SandboxStartStopArgs {
    id: String,
    confirmed: Option<bool>,
}

async fn sandbox_start(id: &str) -> Result<()> {
    let docker = connect_docker()?;
    let inspect = docker
        .inspect_container(id, None::<InspectContainerOptions>)
        .await?;
    let labels = inspect
        .config
        .as_ref()
        .and_then(|cfg| cfg.labels.clone())
        .unwrap_or_default();
    if !sandbox_is_managed(&labels) {
        return Err(anyhow!("container is not a CrateBay-managed sandbox"));
    }
    let name = inspect
        .name
        .unwrap_or_else(|| id.to_string())
        .trim_start_matches('/')
        .to_string();
    docker
        .start_container(id, None::<StartContainerOptions<String>>)
        .await?;
    sandbox_audit_log("start", id, &name, "ok", "sandbox started");
    Ok(())
}

async fn sandbox_stop(id: &str) -> Result<()> {
    let docker = connect_docker()?;
    let inspect = docker
        .inspect_container(id, None::<InspectContainerOptions>)
        .await?;
    let labels = inspect
        .config
        .as_ref()
        .and_then(|cfg| cfg.labels.clone())
        .unwrap_or_default();
    if !sandbox_is_managed(&labels) {
        return Err(anyhow!("container is not a CrateBay-managed sandbox"));
    }
    let name = inspect
        .name
        .unwrap_or_else(|| id.to_string())
        .trim_start_matches('/')
        .to_string();
    docker
        .stop_container(id, Some(StopContainerOptions { t: 10 }))
        .await?;
    sandbox_audit_log("stop", id, &name, "ok", "sandbox stopped");
    Ok(())
}

#[derive(Debug, Deserialize)]
struct SandboxMountDto {
    host_path: String,
    container_path: String,
    #[serde(default = "default_true")]
    read_only: bool,
}

#[derive(Debug, Deserialize)]
struct SandboxCreateArgs {
    template_id: String,
    name: Option<String>,
    image: Option<String>,
    command: Option<String>,
    env: Option<Vec<String>>,
    cpu_cores: Option<u32>,
    memory_mb: Option<u64>,
    ttl_hours: Option<u32>,
    owner: Option<String>,
    #[serde(default)]
    mounts: Vec<SandboxMountDto>,
    confirmed: Option<bool>,
}

#[derive(Debug, Serialize)]
struct SandboxCreateResultDto {
    id: String,
    short_id: String,
    name: String,
    image: String,
    login_cmd: String,
}

async fn sandbox_create(
    ctx: &AppContext,
    args: SandboxCreateArgs,
) -> Result<SandboxCreateResultDto> {
    require_confirmed(args.confirmed, "sandbox_create")?;

    let template_id = args.template_id.trim().to_string();
    if template_id.is_empty() {
        return Err(anyhow!("template_id is required"));
    }
    let template = sandbox_find_template(&template_id)
        .ok_or_else(|| anyhow!("Unknown sandbox template '{}'", template_id))?;

    let image = args
        .image
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| template.image.to_string());
    cratebay_core::validation::validate_image_reference(&image)
        .map_err(|e| anyhow!("Invalid sandbox image '{}': {}", image, e))?;

    let command = args
        .command
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| template.default_command.to_string());

    let name = args
        .name
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| sandbox_generate_name(&template_id));
    cratebay_core::validation::validate_container_name(&name)
        .map_err(|e| anyhow!("Invalid sandbox name '{}': {}", name, e))?;

    let owner = sandbox_normalize_owner(args.owner);
    let cpu_cores = args.cpu_cores.unwrap_or(template.cpu_default).clamp(1, 16);
    let memory_mb = args
        .memory_mb
        .unwrap_or(template.memory_mb_default)
        .clamp(256, 65536);
    let ttl_hours = args
        .ttl_hours
        .unwrap_or(template.ttl_hours_default)
        .clamp(1, 168);

    let created_at = chrono::Utc::now();
    let expires_at = created_at + chrono::Duration::hours(ttl_hours as i64);

    let mut env = template
        .default_env
        .iter()
        .map(|v| v.to_string())
        .collect::<Vec<_>>();
    let mut custom_env = sandbox_normalize_env(args.env)?;
    env.append(&mut custom_env);

    let mut labels = HashMap::new();
    labels.insert(SANDBOX_LABEL_MANAGED.to_string(), "true".to_string());
    labels.insert(SANDBOX_LABEL_TEMPLATE_ID.to_string(), template_id.clone());
    labels.insert(SANDBOX_LABEL_OWNER.to_string(), owner.clone());
    labels.insert(
        SANDBOX_LABEL_CREATED_AT.to_string(),
        created_at.to_rfc3339(),
    );
    labels.insert(
        SANDBOX_LABEL_EXPIRES_AT.to_string(),
        expires_at.to_rfc3339(),
    );
    labels.insert(SANDBOX_LABEL_TTL_HOURS.to_string(), ttl_hours.to_string());
    labels.insert(SANDBOX_LABEL_CPU_CORES.to_string(), cpu_cores.to_string());
    labels.insert(SANDBOX_LABEL_MEMORY_MB.to_string(), memory_mb.to_string());

    let mounts = args.mounts;
    if !mounts.is_empty() && ctx.workspace_root.is_none() {
        return Err(anyhow!(
            "mounts require CRATEBAY_MCP_WORKSPACE_ROOT (to safely constrain host paths)"
        ));
    }

    let binds = if mounts.is_empty() {
        None
    } else {
        let mut binds = Vec::new();
        for mount in mounts {
            let host = resolve_workspace_path_for_read(ctx, &mount.host_path)?;
            let container_path = mount.container_path.trim().to_string();
            cratebay_core::validation::validate_mount_path(&container_path)
                .map_err(|e| anyhow!("Invalid container_path '{}': {}", container_path, e))?;
            let host_str = host.to_string_lossy();
            cratebay_core::validation::validate_mount_path(&host_str)
                .map_err(|e| anyhow!("Invalid host_path '{}': {}", host_str, e))?;
            let mode = if mount.read_only { "ro" } else { "rw" };
            binds.push(format!("{}:{}:{}", host_str, container_path, mode));
        }
        Some(binds)
    };

    let host_config = HostConfig {
        nano_cpus: Some((cpu_cores as i64) * 1_000_000_000),
        memory: Some((memory_mb as i64).saturating_mul(1024).saturating_mul(1024)),
        binds,
        ..Default::default()
    };

    let config = Config::<String> {
        image: Some(image.clone()),
        cmd: Some(vec![
            "/bin/sh".to_string(),
            "-lc".to_string(),
            command.clone(),
        ]),
        host_config: Some(host_config),
        labels: Some(labels),
        env: if env.is_empty() { None } else { Some(env) },
        tty: Some(true),
        open_stdin: Some(true),
        ..Default::default()
    };

    let docker = connect_docker()?;
    if docker.inspect_image(&image).await.is_err() {
        docker_pull_image(&docker, &image).await?;
    }

    let created = docker
        .create_container(
            Some(CreateContainerOptions {
                name: name.clone(),
                platform: None,
            }),
            config,
        )
        .await?;
    docker
        .start_container(&created.id, None::<StartContainerOptions<String>>)
        .await?;

    let short_id = sandbox_short_id(&created.id);
    sandbox_audit_log(
        "create",
        &short_id,
        &name,
        "ok",
        &format!(
            "template={} image={} ttl={}h cpu={} mem={}MB",
            template_id, image, ttl_hours, cpu_cores, memory_mb
        ),
    );

    let login_cmd = if let Some(host) = docker_host_for_docker_cli() {
        format!("DOCKER_HOST={} docker exec -it {} /bin/sh", host, name)
    } else {
        format!("docker exec -it {} /bin/sh", name)
    };

    Ok(SandboxCreateResultDto {
        id: created.id,
        short_id,
        name,
        image,
        login_cmd,
    })
}

#[derive(Debug, Deserialize)]
struct SandboxPutPathArgs {
    id: String,
    local_path: String,
    container_path: String,
    confirmed: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct SandboxGetPathArgs {
    id: String,
    container_path: String,
    local_path: String,
    confirmed: Option<bool>,
}

fn docker_cp(args: &[String], confirmed: Option<bool>) -> Result<String> {
    require_confirmed(confirmed, "docker cp")?;

    let mut cmd = Command::new("docker");
    cmd.arg("cp");
    for item in args {
        cmd.arg(item);
    }
    if let Some(host) = docker_host_for_docker_cli() {
        cmd.env("DOCKER_HOST", host);
    }
    cmd.env_remove("DOCKER_CONTEXT");
    let output = cmd.output().context("spawn docker cp")?;
    if output.status.success() {
        Ok(String::new())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(anyhow!("docker cp failed: {}", stderr.trim()))
    }
}

fn tool_schema(object: serde_json::Value) -> serde_json::Value {
    json!({
        "type": "object",
        "properties": object["properties"].clone(),
        "required": object["required"].clone(),
        "additionalProperties": false
    })
}

fn default_true() -> bool {
    true
}

#[tokio::main]
async fn main() -> Result<()> {
    cratebay_core::logging::init();
    let ctx = AppContext::from_env()?;

    let transport = StdioTransport::new();
    let ctx_for_create = ctx.clone();
    let ctx_for_put = ctx.clone();
    let ctx_for_get = ctx.clone();

    let server = ServerBuilder::new("cratebay", env!("CARGO_PKG_VERSION"))
        .with_transport(transport)
        .with_tool(
            "cratebay_sandbox_templates",
            Some("List built-in CrateBay sandbox templates."),
            tool_schema(json!({
                "properties": {},
                "required": []
            })),
            move |_args| {
                let templates = sandbox_templates_catalog();
                Ok(ok_json(json!({ "templates": templates })))
            },
        )
        .with_tool(
            "cratebay_sandbox_list",
            Some("List CrateBay-managed sandboxes (Docker containers)."),
            tool_schema(json!({
                "properties": {},
                "required": []
            })),
            move |_args| {
                match run_async(async { sandbox_list().await }) {
                    Ok(items) => Ok(ok_json(json!({ "sandboxes": items }))),
                    Err(err) => Ok(err_text(err.to_string())),
                }
            },
        )
        .with_tool(
            "cratebay_sandbox_inspect",
            Some("Inspect a managed sandbox by id."),
            tool_schema(json!({
                "properties": {
                    "id": { "type": "string" }
                },
                "required": ["id"]
            })),
            move |args| {
                let parsed: SandboxInspectArgs = serde_json::from_value(args)?;
                match run_async(async { sandbox_inspect(&parsed.id).await }) {
                    Ok(item) => Ok(ok_json(json!({ "sandbox": item }))),
                    Err(err) => Ok(err_text(err.to_string())),
                }
            },
        )
        .with_tool(
            "cratebay_sandbox_exec",
            Some("Execute a shell command inside a sandbox. Requires confirmed=true."),
            tool_schema(json!({
                "properties": {
                    "id": { "type": "string" },
                    "command": { "type": "string" },
                    "timeout_sec": { "type": "integer" },
                    "confirmed": { "type": "boolean" }
                },
                "required": ["id", "command", "confirmed"]
            })),
            move |args| {
                let parsed: SandboxExecArgs = serde_json::from_value(args)?;
                if let Err(err) = require_confirmed(parsed.confirmed, "sandbox_exec") {
                    return Ok(err_text(err.to_string()));
                }
                let timeout = parsed.timeout_sec.unwrap_or(30).clamp(1, 600);
                match run_async(async { sandbox_exec(&parsed.id, &parsed.command, timeout).await }) {
                    Ok(result) => Ok(ok_json(json!({ "result": result }))),
                    Err(err) => Ok(err_text(err.to_string())),
                }
            },
        )
        .with_tool(
            "cratebay_sandbox_cleanup_expired",
            Some("Delete expired managed sandboxes. Requires confirmed=true."),
            tool_schema(json!({
                "properties": { "confirmed": { "type": "boolean" } },
                "required": ["confirmed"]
            })),
            move |args| {
                let parsed: SandboxCleanupArgs = serde_json::from_value(args)?;
                if let Err(err) = require_confirmed(parsed.confirmed, "sandbox_cleanup_expired") {
                    return Ok(err_text(err.to_string()));
                }
                match run_async(async { sandbox_cleanup_expired().await }) {
                    Ok(result) => Ok(ok_json(json!({ "result": result }))),
                    Err(err) => Ok(err_text(err.to_string())),
                }
            },
        )
        .with_tool(
            "cratebay_sandbox_delete",
            Some("Delete a sandbox by id. Requires confirmed=true."),
            tool_schema(json!({
                "properties": { "id": { "type": "string" }, "confirmed": { "type": "boolean" } },
                "required": ["id", "confirmed"]
            })),
            move |args| {
                let parsed: SandboxDeleteArgs = serde_json::from_value(args)?;
                if let Err(err) = require_confirmed(parsed.confirmed, "sandbox_delete") {
                    return Ok(err_text(err.to_string()));
                }
                match run_async(async { sandbox_delete(&parsed.id).await }) {
                    Ok(_) => Ok(ok_text("ok")),
                    Err(err) => Ok(err_text(err.to_string())),
                }
            },
        )
        .with_tool(
            "cratebay_sandbox_start",
            Some("Start a stopped sandbox. Requires confirmed=true."),
            tool_schema(json!({
                "properties": { "id": { "type": "string" }, "confirmed": { "type": "boolean" } },
                "required": ["id", "confirmed"]
            })),
            move |args| {
                let parsed: SandboxStartStopArgs = serde_json::from_value(args)?;
                if let Err(err) = require_confirmed(parsed.confirmed, "sandbox_start") {
                    return Ok(err_text(err.to_string()));
                }
                match run_async(async { sandbox_start(&parsed.id).await }) {
                    Ok(_) => Ok(ok_text("ok")),
                    Err(err) => Ok(err_text(err.to_string())),
                }
            },
        )
        .with_tool(
            "cratebay_sandbox_stop",
            Some("Stop a running sandbox. Requires confirmed=true."),
            tool_schema(json!({
                "properties": { "id": { "type": "string" }, "confirmed": { "type": "boolean" } },
                "required": ["id", "confirmed"]
            })),
            move |args| {
                let parsed: SandboxStartStopArgs = serde_json::from_value(args)?;
                if let Err(err) = require_confirmed(parsed.confirmed, "sandbox_stop") {
                    return Ok(err_text(err.to_string()));
                }
                match run_async(async { sandbox_stop(&parsed.id).await }) {
                    Ok(_) => Ok(ok_text("ok")),
                    Err(err) => Ok(err_text(err.to_string())),
                }
            },
        )
        .with_tool(
            "cratebay_sandbox_create",
            Some("Create and start a managed sandbox from a template. Supports optional workspace mounts. Requires confirmed=true."),
            tool_schema(json!({
                "properties": {
                    "template_id": { "type": "string" },
                    "name": { "type": ["string", "null"] },
                    "image": { "type": ["string", "null"] },
                    "command": { "type": ["string", "null"] },
                    "env": { "type": ["array", "null"], "items": { "type": "string" } },
                    "cpu_cores": { "type": ["integer", "null"] },
                    "memory_mb": { "type": ["integer", "null"] },
                    "ttl_hours": { "type": ["integer", "null"] },
                    "owner": { "type": ["string", "null"] },
                    "mounts": {
                      "type": "array",
                      "items": {
                        "type": "object",
                        "properties": {
                          "host_path": { "type": "string" },
                          "container_path": { "type": "string" },
                          "read_only": { "type": "boolean" }
                        },
                        "required": ["host_path", "container_path"],
                        "additionalProperties": false
                      }
                    },
                    "confirmed": { "type": "boolean" }
                },
                "required": ["template_id", "confirmed"]
            })),
            move |args| {
                let parsed: SandboxCreateArgs = serde_json::from_value(args)?;
                match run_async(async { sandbox_create(&ctx_for_create, parsed).await }) {
                    Ok(result) => Ok(ok_json(json!({ "result": result }))),
                    Err(err) => Ok(err_text(err.to_string())),
                }
            },
        )
        .with_tool(
            "cratebay_sandbox_put_path",
            Some("Copy a local file/dir into a sandbox using `docker cp`. Requires confirmed=true and CRATEBAY_MCP_WORKSPACE_ROOT."),
            tool_schema(json!({
                "properties": {
                    "id": { "type": "string" },
                    "local_path": { "type": "string" },
                    "container_path": { "type": "string" },
                    "confirmed": { "type": "boolean" }
                },
                "required": ["id", "local_path", "container_path", "confirmed"]
            })),
            move |args| {
                let parsed: SandboxPutPathArgs = serde_json::from_value(args)?;
                let local = match resolve_workspace_path_for_read(&ctx_for_put, &parsed.local_path) {
                    Ok(p) => p,
                    Err(err) => return Ok(err_text(err.to_string())),
                };
                let container_path = parsed.container_path.trim().to_string();
                if let Err(err) = cratebay_core::validation::validate_mount_path(&container_path) {
                    return Ok(err_text(format!("Invalid container_path: {}", err)));
                }
                let src = local.display().to_string();
                let dst = format!("{}:{}", parsed.id.trim(), container_path);
                match docker_cp(&[src, dst], parsed.confirmed) {
                    Ok(_) => Ok(ok_text("ok")),
                    Err(err) => Ok(err_text(err.to_string())),
                }
            },
        )
        .with_tool(
            "cratebay_sandbox_get_path",
            Some("Copy a file/dir from sandbox to local workspace using `docker cp`. Requires confirmed=true and CRATEBAY_MCP_WORKSPACE_ROOT."),
            tool_schema(json!({
                "properties": {
                    "id": { "type": "string" },
                    "container_path": { "type": "string" },
                    "local_path": { "type": "string" },
                    "confirmed": { "type": "boolean" }
                },
                "required": ["id", "container_path", "local_path", "confirmed"]
            })),
            move |args| {
                let parsed: SandboxGetPathArgs = serde_json::from_value(args)?;
                let local = match resolve_workspace_path_for_write(&ctx_for_get, &parsed.local_path) {
                    Ok(p) => p,
                    Err(err) => return Ok(err_text(err.to_string())),
                };
                let container_path = parsed.container_path.trim().to_string();
                if let Err(err) = cratebay_core::validation::validate_mount_path(&container_path) {
                    return Ok(err_text(format!("Invalid container_path: {}", err)));
                }
                if let Some(parent) = local.parent() {
                    if let Err(err) = std::fs::create_dir_all(parent) {
                        return Ok(err_text(format!(
                            "Failed to create local directory {}: {}",
                            parent.display(),
                            err
                        )));
                    }
                    match parent.canonicalize() {
                        Ok(parent_canon) => {
                            if let Some(root) = ctx_for_get.workspace_root.as_ref() {
                                if !parent_canon.starts_with(root) {
                                    return Ok(err_text(format!(
                                        "local path is outside workspace root: {}",
                                        parent_canon.display()
                                    )));
                                }
                            }
                        }
                        Err(err) => {
                            return Ok(err_text(format!(
                                "Failed to resolve local directory {}: {}",
                                parent.display(),
                                err
                            )))
                        }
                    }
                }
                let src = format!("{}:{}", parsed.id.trim(), container_path);
                let dst = local.display().to_string();
                match docker_cp(&[src, dst], parsed.confirmed) {
                    Ok(_) => Ok(ok_text("ok")),
                    Err(err) => Ok(err_text(err.to_string())),
                }
            },
        )
        .build()
        .context("build MCP server")?;

    if ctx.workspace_root.is_some() {
        info!(
            "cratebay-mcp started (workspace_root={})",
            ctx.workspace_root
                .as_ref()
                .map(|p| p.display().to_string())
                .unwrap_or_default()
        );
    } else {
        warn!("cratebay-mcp started (workspace_root not configured; file ops disabled)");
    }

    server.run().await?;
    Ok(())
}
