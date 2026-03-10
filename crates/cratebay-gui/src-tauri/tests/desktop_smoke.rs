use fantoccini::{Client, ClientBuilder, Locator};
use serde_json::json;
use std::{
    env,
    process::Command,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tokio::time::sleep;

type TestResult<T> = Result<T, String>;

struct DockerCleanup {
    container_name: String,
}

impl DockerCleanup {
    fn new(container_name: String) -> Self {
        cleanup_container(&container_name);
        Self { container_name }
    }
}

impl Drop for DockerCleanup {
    fn drop(&mut self) {
        cleanup_container(&self.container_name);
    }
}

fn cleanup_container(container_name: &str) {
    let _ = Command::new("docker")
        .args(["rm", "-f", container_name])
        .output();
}

fn docker_running_state(container_name: &str) -> Option<bool> {
    let output = Command::new("docker")
        .args(["inspect", "-f", "{{.State.Running}}", container_name])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let state = String::from_utf8_lossy(&output.stdout);
    match state.trim() {
        "true" => Some(true),
        "false" => Some(false),
        _ => None,
    }
}

fn sanitize_test_key(raw: &str) -> String {
    raw.chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch
            } else {
                '-'
            }
        })
        .collect()
}

async fn connect_client() -> Client {
    let app = env::var("CRATEBAY_DESKTOP_E2E_APP")
        .expect("CRATEBAY_DESKTOP_E2E_APP must point to the built Tauri binary");
    let webdriver_url =
        env::var("TAURI_DRIVER_URL").unwrap_or_else(|_| "http://127.0.0.1:4444".to_string());

    let mut caps = fantoccini::wd::Capabilities::new();
    caps.insert(
        "tauri:options".into(),
        json!({
            "application": app,
            "args": [],
        }),
    );
    caps.insert("browserName".into(), json!("wry"));

    let mut builder = ClientBuilder::rustls().expect("rustls client builder");
    builder.capabilities(caps);
    builder
        .connect(&webdriver_url)
        .await
        .expect("connect to tauri-driver")
}

async fn wait_for_css(client: &Client, selector: &str, timeout: Duration) -> TestResult<()> {
    client
        .wait()
        .at_most(timeout)
        .for_element(Locator::Css(selector))
        .await
        .map_err(|e| format!("timed out waiting for selector {selector}: {e}"))?;
    Ok(())
}

async fn click_css(client: &Client, selector: &str) -> TestResult<()> {
    client
        .find(Locator::Css(selector))
        .await
        .map_err(|e| format!("failed to find selector {selector}: {e}"))?
        .click()
        .await
        .map_err(|e| format!("failed to click selector {selector}: {e}"))?;
    Ok(())
}

async fn clear_and_type_css(client: &Client, selector: &str, value: &str) -> TestResult<()> {
    let input = client
        .find(Locator::Css(selector))
        .await
        .map_err(|e| format!("failed to find input {selector}: {e}"))?;
    input
        .click()
        .await
        .map_err(|e| format!("failed to focus input {selector}: {e}"))?;
    input
        .clear()
        .await
        .map_err(|e| format!("failed to clear input {selector}: {e}"))?;
    input
        .send_keys(value)
        .await
        .map_err(|e| format!("failed to type into input {selector}: {e}"))?;
    Ok(())
}

async fn wait_for_page_text(client: &Client, needle: &str, timeout: Duration) -> TestResult<()> {
    let deadline = Instant::now() + timeout;
    loop {
        let source = client
            .source()
            .await
            .map_err(|e| format!("failed to read page source: {e}"))?;
        if source.contains(needle) {
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err(format!("timed out waiting for page text: {needle}"));
        }
        sleep(Duration::from_millis(250)).await;
    }
}

async fn wait_for_absent_css(client: &Client, selector: &str, timeout: Duration) -> TestResult<()> {
    let deadline = Instant::now() + timeout;
    loop {
        if client.find(Locator::Css(selector)).await.is_err() {
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err(format!(
                "timed out waiting for selector to disappear: {selector}"
            ));
        }
        sleep(Duration::from_millis(250)).await;
    }
}

async fn wait_for_css_text(
    client: &Client,
    selector: &str,
    needle: &str,
    timeout: Duration,
) -> TestResult<()> {
    let deadline = Instant::now() + timeout;
    loop {
        if let Ok(element) = client.find(Locator::Css(selector)).await {
            if let Ok(text) = element.text().await {
                if text.contains(needle) {
                    return Ok(());
                }
            }
        }
        if Instant::now() >= deadline {
            return Err(format!(
                "timed out waiting for selector text: {selector} -> {needle}"
            ));
        }
        sleep(Duration::from_millis(250)).await;
    }
}

async fn wait_for_docker_state(
    container_name: &str,
    expected: bool,
    timeout: Duration,
) -> TestResult<()> {
    let deadline = Instant::now() + timeout;
    loop {
        if docker_running_state(container_name) == Some(expected) {
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err(format!(
                "timed out waiting for docker state on {container_name}: expected {expected}"
            ));
        }
        sleep(Duration::from_millis(500)).await;
    }
}

async fn wait_for_container_removed(container_name: &str, timeout: Duration) -> TestResult<()> {
    let deadline = Instant::now() + timeout;
    loop {
        if docker_running_state(container_name).is_none() {
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err(format!(
                "timed out waiting for container removal: {container_name}"
            ));
        }
        sleep(Duration::from_millis(500)).await;
    }
}

#[tokio::test]
#[ignore = "requires Linux desktop automation runtime"]
async fn desktop_shell_renders_and_navigates() {
    let client = connect_client().await;

    let result: TestResult<()> = async {
        wait_for_css(
            &client,
            "[data-testid='nav-dashboard']",
            Duration::from_secs(30),
        )
        .await?;

        click_css(&client, "[data-testid='nav-ai']").await?;
        wait_for_css(&client, "[role='tab']", Duration::from_secs(15)).await?;

        let page = client
            .source()
            .await
            .map_err(|e| format!("failed to read page source: {e}"))?;
        if !page.contains("Assistant") {
            return Err("assistant tab should render in desktop shell".into());
        }

        click_css(&client, "[data-testid='nav-settings']").await?;

        let settings_page = client
            .source()
            .await
            .map_err(|e| format!("failed to read settings source: {e}"))?;
        if !settings_page.contains("General") {
            return Err("settings general tab should render".into());
        }
        if !settings_page.contains("AI") {
            return Err("settings AI tab should render".into());
        }

        Ok(())
    }
    .await;

    let _ = client.close().await;
    result.unwrap_or_else(|err| panic!("{err}"));
}

#[tokio::test]
#[ignore = "requires Linux desktop automation runtime"]
async fn desktop_shell_runs_container_lifecycle() {
    let docker_info = Command::new("docker")
        .arg("info")
        .output()
        .expect("docker info should run");
    assert!(
        docker_info.status.success(),
        "docker daemon should be available for desktop runtime smoke"
    );

    let suffix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time")
        .as_secs();
    let container_name = format!("cbx-desktop-smoke-{}-{}", std::process::id(), suffix);
    let test_key = sanitize_test_key(&container_name);
    let env_key = "CRATEBAY_E2E";
    let env_value = format!("desktop-{suffix}");
    let card_selector = format!("[data-testid='container-card-{test_key}']");
    let env_button_selector = format!("[data-testid='container-env-{test_key}']");
    let login_button_selector = format!("[data-testid='container-login-{test_key}']");
    let stop_button_selector = format!("[data-testid='container-stop-{test_key}']");
    let start_button_selector = format!("[data-testid='container-start-{test_key}']");
    let delete_button_selector = format!("[data-testid='container-delete-{test_key}']");
    let _cleanup = DockerCleanup::new(container_name.clone());

    let client = connect_client().await;

    let result: TestResult<()> = async {
        wait_for_css(
            &client,
            "[data-testid='nav-dashboard']",
            Duration::from_secs(30),
        )
        .await?;
        click_css(&client, "[data-testid='nav-containers']").await?;
        wait_for_css(
            &client,
            "[data-testid='containers-run']",
            Duration::from_secs(15),
        )
        .await?;

        click_css(&client, "[data-testid='containers-run']").await?;
        wait_for_css(
            &client,
            "[data-testid='containers-dialog-run']",
            Duration::from_secs(15),
        )
        .await?;

        clear_and_type_css(
            &client,
            "[data-testid='containers-dialog-run'] input[placeholder='nginx:latest']",
            "nginx:1.27-alpine",
        )
        .await?;
        clear_and_type_css(
            &client,
            "[data-testid='containers-dialog-run'] input[placeholder='my-container']",
            &container_name,
        )
        .await?;

        click_css(&client, "[data-testid='containers-run-add-env']").await?;
        wait_for_css(
            &client,
            "[data-testid='containers-run-env-key-0']",
            Duration::from_secs(10),
        )
        .await?;
        clear_and_type_css(&client, "[data-testid='containers-run-env-key-0']", env_key).await?;
        clear_and_type_css(
            &client,
            "[data-testid='containers-run-env-value-0']",
            &env_value,
        )
        .await?;
        click_css(&client, "[data-testid='containers-run-submit']").await?;

        wait_for_css(&client, &card_selector, Duration::from_secs(120)).await?;
        wait_for_docker_state(&container_name, true, Duration::from_secs(60)).await?;
        wait_for_css(&client, &stop_button_selector, Duration::from_secs(30)).await?;

        click_css(&client, &env_button_selector).await?;
        wait_for_css(
            &client,
            "[data-testid='containers-dialog-env']",
            Duration::from_secs(20),
        )
        .await?;
        wait_for_page_text(&client, env_key, Duration::from_secs(20)).await?;
        wait_for_page_text(&client, &env_value, Duration::from_secs(20)).await?;
        click_css(&client, "[data-testid='containers-env-close']").await?;
        wait_for_absent_css(
            &client,
            "[data-testid='containers-dialog-env']",
            Duration::from_secs(10),
        )
        .await?;

        click_css(&client, &login_button_selector).await?;
        wait_for_css(
            &client,
            "[data-testid='app-modal-text']",
            Duration::from_secs(20),
        )
        .await?;
        wait_for_page_text(
            &client,
            &format!("docker exec -it {container_name} /bin/sh"),
            Duration::from_secs(20),
        )
        .await?;
        click_css(&client, "[data-testid='app-modal-close']").await?;
        wait_for_absent_css(
            &client,
            "[data-testid='app-modal-text']",
            Duration::from_secs(10),
        )
        .await?;

        click_css(&client, &stop_button_selector).await?;
        wait_for_docker_state(&container_name, false, Duration::from_secs(60)).await?;
        wait_for_css(&client, &start_button_selector, Duration::from_secs(30)).await?;

        click_css(&client, &start_button_selector).await?;
        wait_for_docker_state(&container_name, true, Duration::from_secs(60)).await?;
        wait_for_css(&client, &stop_button_selector, Duration::from_secs(30)).await?;

        click_css(&client, &delete_button_selector).await?;
        wait_for_css(
            &client,
            "[data-testid='containers-dialog-remove']",
            Duration::from_secs(10),
        )
        .await?;
        click_css(&client, "[data-testid='containers-remove-confirm']").await?;
        wait_for_container_removed(&container_name, Duration::from_secs(60)).await?;
        wait_for_absent_css(&client, &card_selector, Duration::from_secs(30)).await?;

        Ok(())
    }
    .await;

    let _ = client.close().await;
    result.unwrap_or_else(|err| panic!("{err}"));
}

#[tokio::test]
#[ignore = "requires Linux desktop automation runtime"]
async fn desktop_shell_runs_mcp_lifecycle() {
    let suffix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time")
        .as_secs();
    let ready_marker = format!("MCP_DESKTOP_READY_{suffix}");
    let workdir = env::var("CRATEBAY_DESKTOP_E2E_WORKDIR").unwrap_or_else(|_| {
        env::current_dir()
            .expect("current dir")
            .display()
            .to_string()
    });
    let client = connect_client().await;

    let result: TestResult<()> = async {
        wait_for_css(
            &client,
            "[data-testid='nav-dashboard']",
            Duration::from_secs(30),
        )
        .await?;
        click_css(&client, "[data-testid='nav-ai']").await?;
        wait_for_css(
            &client,
            "[data-testid='aihub-tab-mcp']",
            Duration::from_secs(20),
        )
        .await?;
        click_css(&client, "[data-testid='aihub-tab-mcp']").await?;

        click_css(&client, "[data-testid='mcp-add-server']").await?;
        wait_for_css(
            &client,
            "[data-testid='mcp-input-id']",
            Duration::from_secs(15),
        )
        .await?;
        let mcp_id = client
            .find(Locator::Css("[data-testid='mcp-input-id']"))
            .await
            .map_err(|e| format!("failed to find generated MCP id: {e}"))?
            .prop("value")
            .await
            .map_err(|e| format!("failed to read generated MCP id: {e}"))?
            .unwrap_or_default();
        if mcp_id.trim().is_empty() {
            return Err("generated MCP id should not be empty".into());
        }
        let row_selector = format!("[data-testid='mcp-row-{mcp_id}']");
        let toggle_selector = format!("[data-testid='mcp-toggle-{mcp_id}']");
        let status_selector = format!("[data-testid='mcp-status-{mcp_id}']");

        clear_and_type_css(&client, "[data-testid='mcp-input-command']", "/bin/sh").await?;
        clear_and_type_css(
            &client,
            "[data-testid='mcp-input-args']",
            &format!("-lc\necho {ready_marker}; while true; do sleep 1; done"),
        )
        .await?;
        clear_and_type_css(&client, "[data-testid='mcp-input-working-dir']", &workdir).await?;
        clear_and_type_css(
            &client,
            "[data-testid='mcp-input-notes']",
            "desktop runtime smoke",
        )
        .await?;

        click_css(&client, "[data-testid='mcp-save-registry']").await?;
        wait_for_css(&client, &row_selector, Duration::from_secs(30)).await?;
        wait_for_css_text(
            &client,
            &status_selector,
            "Stopped",
            Duration::from_secs(20),
        )
        .await?;

        click_css(&client, &toggle_selector).await?;
        wait_for_css_text(
            &client,
            &status_selector,
            "Running",
            Duration::from_secs(30),
        )
        .await?;
        wait_for_css_text(
            &client,
            "[data-testid='mcp-selected-status']",
            "Running",
            Duration::from_secs(30),
        )
        .await?;
        wait_for_css_text(
            &client,
            "[data-testid='mcp-logs-output']",
            &ready_marker,
            Duration::from_secs(30),
        )
        .await?;

        click_css(&client, &toggle_selector).await?;
        wait_for_css_text(&client, &status_selector, "Exited", Duration::from_secs(30)).await?;
        wait_for_css_text(
            &client,
            "[data-testid='mcp-selected-status']",
            "Exited",
            Duration::from_secs(30),
        )
        .await?;

        Ok(())
    }
    .await;

    let _ = client.close().await;
    result.unwrap_or_else(|err| panic!("{err}"));
}
