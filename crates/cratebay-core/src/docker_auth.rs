use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use serde::Deserialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegistryAuth {
    pub server_address: String,
    pub username: Option<String>,
    pub password: Option<String>,
    pub identity_token: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct DockerConfigFile {
    auths: Option<HashMap<String, DockerAuthEntry>>,
    #[serde(rename = "credsStore")]
    creds_store: Option<String>,
    #[serde(rename = "credHelpers")]
    cred_helpers: Option<HashMap<String, String>>,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct DockerAuthEntry {
    auth: Option<String>,
    username: Option<String>,
    password: Option<String>,
    #[serde(rename = "identitytoken")]
    identity_token: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct DockerCredentialHelperGetResponse {
    #[serde(rename = "Username")]
    username: String,
    #[serde(rename = "Secret")]
    secret: String,
}

fn normalize_registry_key(key: &str) -> String {
    let mut s = key.trim().to_ascii_lowercase();
    if let Some(rest) = s.strip_prefix("https://") {
        s = rest.to_string();
    } else if let Some(rest) = s.strip_prefix("http://") {
        s = rest.to_string();
    }
    if let Some((host, _path)) = s.split_once('/') {
        s = host.to_string();
    }
    s.trim_end_matches('/').to_string()
}

fn docker_config_path() -> Option<PathBuf> {
    if let Ok(dir) = std::env::var("DOCKER_CONFIG") {
        let p = PathBuf::from(dir).join("config.json");
        return Some(p);
    }

    if let Ok(home) = std::env::var("HOME") {
        return Some(Path::new(&home).join(".docker").join("config.json"));
    }

    // Windows fallbacks.
    if let Ok(profile) = std::env::var("USERPROFILE") {
        return Some(Path::new(&profile).join(".docker").join("config.json"));
    }
    if let (Ok(drive), Ok(path)) = (std::env::var("HOMEDRIVE"), std::env::var("HOMEPATH")) {
        return Some(
            PathBuf::from(format!("{}{}", drive, path))
                .join(".docker")
                .join("config.json"),
        );
    }

    None
}

fn dockerhub_registry_hosts() -> &'static [&'static str] {
    &["index.docker.io", "registry-1.docker.io", "docker.io"]
}

fn registry_host_from_image_reference(reference: &str) -> Option<String> {
    // Strip tag and digest first, keep only the repository path.
    let repo = reference.split('@').next().unwrap_or(reference);
    let repo = repo
        .rsplit_once(':')
        .filter(|(_, tag)| !tag.is_empty() && !tag.contains('/'))
        .map(|(left, _)| left)
        .unwrap_or(repo);

    let first = repo.split('/').next().unwrap_or("");
    if first.is_empty() {
        return None;
    }

    // Docker's reference parsing rule: a registry domain must contain a '.' or ':', or be localhost.
    if first == "localhost" || first.contains('.') || first.contains(':') {
        return Some(first.to_string());
    }
    None
}

fn find_registry_key<'a>(
    mut keys: impl Iterator<Item = &'a String>,
    registry_host: Option<&str>,
) -> Option<&'a String> {
    let Some(reg_host) = registry_host else {
        // Docker Hub: match any of the known hosts.
        return keys.find(|k| {
            let normalized = normalize_registry_key(k);
            dockerhub_registry_hosts().iter().any(|h| normalized == *h)
        });
    };

    let wanted = reg_host.to_ascii_lowercase();
    keys.find(|k| normalize_registry_key(k) == wanted)
}

fn decode_docker_auth(auth_b64: &str) -> Result<(String, String), String> {
    let decoded = STANDARD
        .decode(auth_b64.trim())
        .map_err(|e| format!("Invalid base64 auth: {}", e))?;
    let decoded =
        String::from_utf8(decoded).map_err(|e| format!("Invalid auth encoding: {}", e))?;
    let (user, pass) = decoded
        .split_once(':')
        .ok_or_else(|| "Invalid auth payload (expected username:password)".to_string())?;
    Ok((user.to_string(), pass.to_string()))
}

fn parse_config(json: &str) -> Result<DockerConfigFile, String> {
    serde_json::from_str::<DockerConfigFile>(json)
        .map_err(|e| format!("Failed to parse docker config json: {}", e))
}

fn run_credential_helper(helper: &str, server: &str) -> Result<RegistryAuth, String> {
    let program = format!("docker-credential-{}", helper);
    let mut child = Command::new(&program)
        .arg("get")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("Failed to run {}: {}", program, e))?;

    if let Some(mut stdin) = child.stdin.take() {
        use std::io::Write;
        stdin
            .write_all(server.as_bytes())
            .and_then(|_| stdin.write_all(b"\n"))
            .map_err(|e| format!("Failed to write to {} stdin: {}", program, e))?;
    }

    let out = child
        .wait_with_output()
        .map_err(|e| format!("Failed to read {} output: {}", program, e))?;
    if !out.status.success() {
        return Err(format!(
            "{} get failed (exit {}): {}",
            program,
            out.status.code().unwrap_or(-1),
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }

    let resp = serde_json::from_slice::<DockerCredentialHelperGetResponse>(&out.stdout)
        .map_err(|e| format!("Failed to parse {} output as JSON: {}", program, e))?;

    let username = resp.username.trim().to_string();
    let secret = resp.secret.trim().to_string();
    if username.is_empty() || secret.is_empty() {
        return Err(format!("{} returned empty credentials", program));
    }

    Ok(RegistryAuth {
        server_address: server.to_string(),
        username: Some(username),
        password: Some(secret),
        identity_token: None,
    })
}

fn resolve_from_config(
    config: &DockerConfigFile,
    reference: &str,
) -> Result<Option<RegistryAuth>, String> {
    let registry_host = registry_host_from_image_reference(reference);
    let auths = config.auths.as_ref();

    // 1) Direct auth entries.
    if let Some(auths) = auths {
        if let Some(key) = find_registry_key(auths.keys(), registry_host.as_deref()) {
            let entry = auths.get(key).cloned().unwrap_or_default();
            if let Some(token) = entry
                .identity_token
                .clone()
                .filter(|v| !v.trim().is_empty())
            {
                return Ok(Some(RegistryAuth {
                    server_address: key.to_string(),
                    username: None,
                    password: None,
                    identity_token: Some(token),
                }));
            }

            if let (Some(u), Some(p)) = (entry.username.clone(), entry.password.clone()) {
                if !u.trim().is_empty() && !p.trim().is_empty() {
                    return Ok(Some(RegistryAuth {
                        server_address: key.to_string(),
                        username: Some(u),
                        password: Some(p),
                        identity_token: None,
                    }));
                }
            }

            if let Some(auth_b64) = entry.auth.as_deref().filter(|v| !v.trim().is_empty()) {
                let (u, p) = decode_docker_auth(auth_b64)?;
                return Ok(Some(RegistryAuth {
                    server_address: key.to_string(),
                    username: Some(u),
                    password: Some(p),
                    identity_token: None,
                }));
            }
        }
    }

    // 2) Credential helpers.
    let mut helper: Option<String> = None;
    let mut server: Option<String> = None;

    if let Some(helpers) = config.cred_helpers.as_ref() {
        if let Some(key) = find_registry_key(helpers.keys(), registry_host.as_deref()) {
            helper = helpers.get(key).cloned();
            server = Some(key.to_string());
        }
    }
    if helper.is_none() {
        helper = config.creds_store.clone();
    }
    if server.is_none() {
        server = Some(match registry_host.as_deref() {
            None => "https://index.docker.io/v1/".to_string(),
            Some(host) => host.to_string(),
        });
    }

    if let Some(helper) = helper {
        let server = server.unwrap_or_else(|| "https://index.docker.io/v1/".to_string());
        return Ok(Some(run_credential_helper(&helper, &server)?));
    }

    Ok(None)
}

/// Resolve registry credentials for `docker push`-like operations.
///
/// Sources (in order):
/// 1) `DOCKER_AUTH_CONFIG` env (JSON)
/// 2) Docker config file (`$DOCKER_CONFIG/config.json` or `~/.docker/config.json`)
///
/// The function supports:
/// - `auths` with `auth` (base64 user:pass) or explicit `username`/`password`
/// - `auths` with `identitytoken`
/// - `credHelpers` / `credsStore` via `docker-credential-<helper> get`
pub fn resolve_registry_auth_for_image(reference: &str) -> Result<Option<RegistryAuth>, String> {
    if let Ok(v) = std::env::var("DOCKER_AUTH_CONFIG") {
        if !v.trim().is_empty() {
            let cfg = parse_config(&v)?;
            if let Some(auth) = resolve_from_config(&cfg, reference)? {
                return Ok(Some(auth));
            }
        }
    }

    let Some(path) = docker_config_path() else {
        return Ok(None);
    };
    if !path.exists() {
        return Ok(None);
    }
    let text = std::fs::read_to_string(&path)
        .map_err(|e| format!("Failed to read docker config {}: {}", path.display(), e))?;
    let cfg = parse_config(&text)?;
    resolve_from_config(&cfg, reference)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_registry_key_strips_scheme_and_path() {
        assert_eq!(
            normalize_registry_key("https://index.docker.io/v1/"),
            "index.docker.io"
        );
        assert_eq!(normalize_registry_key("ghcr.io"), "ghcr.io");
        assert_eq!(
            normalize_registry_key("http://localhost:5000/"),
            "localhost:5000"
        );
    }

    #[test]
    fn registry_host_from_image_reference_detects_custom_registry() {
        assert_eq!(
            registry_host_from_image_reference("ghcr.io/org/image:v1").as_deref(),
            Some("ghcr.io")
        );
        assert_eq!(
            registry_host_from_image_reference("localhost:5000/myimg:latest").as_deref(),
            Some("localhost:5000")
        );
        assert_eq!(registry_host_from_image_reference("nginx:latest"), None);
    }

    #[test]
    fn decode_docker_auth_decodes_user_pass() {
        let encoded = STANDARD.encode("user:pass");
        let (u, p) = decode_docker_auth(&encoded).unwrap();
        assert_eq!(u, "user");
        assert_eq!(p, "pass");
    }

    #[test]
    fn resolve_from_config_prefers_auth_entry() {
        let json = r#"{
  "auths": {
    "https://index.docker.io/v1/": { "auth": "dXNlcjpwYXNz" },
    "ghcr.io": { "auth": "b2N0bzp0b2tlbg==" }
  }
}"#;
        let cfg = parse_config(json).unwrap();
        let auth = resolve_from_config(&cfg, "nginx:latest").unwrap().unwrap();
        assert_eq!(auth.server_address, "https://index.docker.io/v1/");
        assert_eq!(auth.username.as_deref(), Some("user"));
        assert_eq!(auth.password.as_deref(), Some("pass"));

        let auth = resolve_from_config(&cfg, "ghcr.io/org/image:v1")
            .unwrap()
            .unwrap();
        assert_eq!(auth.server_address, "ghcr.io");
        assert_eq!(auth.username.as_deref(), Some("octo"));
        assert_eq!(auth.password.as_deref(), Some("token"));
    }
}
