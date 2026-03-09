use super::*;

#[derive(Debug, Serialize)]
pub(crate) struct SandboxExecResultDto {
    pub(crate) ok: bool,
    pub(crate) output: String,
    pub(crate) stdout: String,
    pub(crate) stderr: String,
    pub(crate) exit_code: Option<i64>,
}

pub(crate) async fn sandbox_cleanup_expired_internal() -> Result<SandboxCleanupResultDto, String> {
    let docker = connect_docker().map_err(|e| sandbox_connect_error(&e))?;
    let opts = ListContainersOptions::<String> {
        all: true,
        ..Default::default()
    };
    let containers = docker
        .list_containers(Some(opts))
        .await
        .map_err(|e| sandbox_docker_error("list", "sandboxes for cleanup", &e))?;

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
            .await
            .map_err(|e| sandbox_docker_error("remove expired sandbox", &normalized_name, &e))?;
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

#[tauri::command]
pub(crate) async fn sandbox_cleanup_expired() -> Result<SandboxCleanupResultDto, String> {
    sandbox_cleanup_expired_internal().await
}

#[tauri::command]
pub(crate) async fn sandbox_exec(
    id: String,
    command: String,
) -> Result<SandboxExecResultDto, String> {
    let trimmed = command.trim();
    if trimmed.is_empty() {
        return Err(sandbox_validation_error("Sandbox command is required"));
    }
    let docker = connect_docker().map_err(|e| sandbox_connect_error(&e))?;
    let (_, name) = sandbox_require_managed(&docker, &id).await?;
    let exec = docker
        .create_exec(
            &id,
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
        .await
        .map_err(|e| sandbox_docker_error("create exec in", &name, &e))?;
    let output = docker
        .start_exec(&exec.id, None)
        .await
        .map_err(|e| sandbox_docker_error("start exec in", &name, &e))?;

    let mut stdout = String::new();
    let mut stderr = String::new();
    if let StartExecResults::Attached { mut output, .. } = output {
        while let Some(chunk) = output
            .try_next()
            .await
            .map_err(|e| sandbox_stream_error("sandbox exec output from", &name, &e))?
        {
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

    let exec_inspect = docker
        .inspect_exec(&exec.id)
        .await
        .map_err(|e| sandbox_docker_error("inspect exec in", &name, &e))?;
    let exit_code = exec_inspect.exit_code;

    let command_len = trimmed.len();
    sandbox_audit_log(
        "exec",
        &sandbox_short_id(&id),
        &name,
        if exit_code.unwrap_or_default() == 0 {
            "ok"
        } else {
            "warn"
        },
        &format!(
            "command_len={} exit_code={}",
            command_len,
            exit_code
                .map(|code| code.to_string())
                .unwrap_or_else(|| "unknown".to_string())
        ),
    );

    let output = if stderr.trim().is_empty() {
        stdout.clone()
    } else if stdout.trim().is_empty() {
        stderr.clone()
    } else {
        format!(
            "{}
{}",
            stdout, stderr
        )
    };

    Ok(SandboxExecResultDto {
        ok: exit_code.unwrap_or_default() == 0,
        output,
        stdout,
        stderr,
        exit_code,
    })
}
