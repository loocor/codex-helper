use std::collections::HashMap;
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;

const CDP_CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const CDP_HTTP_TIMEOUT: Duration = Duration::from_secs(3);
const CDP_COMMAND_TIMEOUT: Duration = Duration::from_secs(5);
const CODEX_APP_URL: &str = "app://-/index.html";

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CdpTarget {
    pub id: String,
    #[serde(rename = "type")]
    pub target_type: String,
    pub title: Option<String>,
    pub url: Option<String>,
    pub devtools_frontend_url: Option<String>,
    pub web_socket_debugger_url: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CdpVersion {
    pub web_socket_debugger_url: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct CdpTargetInfo {
    target_id: Option<String>,
    #[serde(rename = "type")]
    target_type: Option<String>,
    title: Option<String>,
    url: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct CdpTargetInfosResult {
    target_infos: Option<Vec<CdpTargetInfo>>,
}

pub async fn is_debug_port_ready(debug_port: u16) -> bool {
    browser_websocket_url(debug_port).await.is_ok()
}

pub async fn has_codex_cdp_target(debug_port: u16) -> bool {
    match list_browser_targets(debug_port).await {
        Ok(targets) => !codex_injectable_page_targets(&targets).is_empty(),
        Err(_) => false,
    }
}

pub async fn find_existing_codex_debug_port(ports: impl IntoIterator<Item = u16>) -> Option<u16> {
    for port in ports {
        if has_codex_cdp_target(port).await {
            return Some(port);
        }
    }
    None
}

pub async fn wait_for_debug_port(debug_port: u16, timeout: Duration) -> anyhow::Result<()> {
    let started_at = std::time::Instant::now();
    while started_at.elapsed() < timeout {
        if is_debug_port_ready(debug_port).await {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
    anyhow::bail!(
        "Timed out waiting for Codex debug port {debug_port} after {:?}",
        started_at.elapsed()
    );
}

pub async fn wait_for_debug_port_to_close(
    debug_port: u16,
    timeout: Duration,
) -> anyhow::Result<()> {
    let started_at = std::time::Instant::now();
    while started_at.elapsed() < timeout {
        if !is_debug_port_ready(debug_port).await {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
    anyhow::bail!(
        "Timed out waiting for Codex debug port {debug_port} to close after {:?}",
        started_at.elapsed()
    );
}

pub async fn wait_for_codex_targets(
    debug_port: u16,
    timeout: Duration,
) -> anyhow::Result<Vec<CdpTarget>> {
    let started_at = std::time::Instant::now();
    let mut last_error = "No injectable Codex page target found".to_string();
    while started_at.elapsed() < timeout {
        match list_browser_targets(debug_port).await {
            Ok(targets) => {
                let codex_targets = codex_injectable_page_targets(&targets);
                if !codex_targets.is_empty() {
                    return Ok(codex_targets);
                }
                last_error = format!(
                    "No injectable Codex page target found among {}",
                    targets.len()
                );
            }
            Err(error) => {
                last_error = error.to_string();
            }
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
    anyhow::bail!(
        "Timed out waiting for Codex CDP targets on port {debug_port} after {:?}: {last_error}",
        started_at.elapsed()
    );
}

pub async fn browser_websocket_url(debug_port: u16) -> anyhow::Result<String> {
    let url = format!("http://127.0.0.1:{debug_port}/json/version");
    let client = cdp_http_client()?;
    let response = client
        .get(&url)
        .send()
        .await
        .map_err(|error| anyhow::anyhow!("CDP version query failed: {error}"))?;
    if !response.status().is_success() {
        anyhow::bail!(
            "CDP version query failed: {} {}",
            response.status().as_u16(),
            response.status().canonical_reason().unwrap_or("Unknown")
        );
    }
    let version = response
        .json::<CdpVersion>()
        .await
        .map_err(|error| anyhow::anyhow!("CDP version response decode failed: {error}"))?;
    if version.web_socket_debugger_url.trim().is_empty() {
        anyhow::bail!("CDP browser websocket URL is empty");
    }
    Ok(version.web_socket_debugger_url)
}

pub async fn list_targets(debug_port: u16) -> anyhow::Result<Vec<CdpTarget>> {
    let url = format!("http://127.0.0.1:{debug_port}/json");
    let client = cdp_http_client()?;
    let response = client
        .get(&url)
        .send()
        .await
        .map_err(|error| anyhow::anyhow!("CDP target query failed: {error}"))?;
    if !response.status().is_success() {
        anyhow::bail!(
            "CDP target query failed: {} {}",
            response.status().as_u16(),
            response.status().canonical_reason().unwrap_or("Unknown")
        );
    }
    response
        .json::<Vec<CdpTarget>>()
        .await
        .map_err(|error| anyhow::anyhow!("CDP target response decode failed: {error}"))
}

fn cdp_http_client() -> anyhow::Result<reqwest::Client> {
    Ok(reqwest::Client::builder()
        .no_proxy()
        .timeout(CDP_HTTP_TIMEOUT)
        .build()?)
}

pub async fn list_browser_targets(debug_port: u16) -> anyhow::Result<Vec<CdpTarget>> {
    let websocket_url = browser_websocket_url(debug_port).await?;
    let socket = connect_cdp_websocket(&websocket_url).await?;
    let mut session = OneShotCdpSession::new(socket);
    let response = session
        .send_command(1, "Target.getTargets", json!({ "filter": [{}] }))
        .await?;
    let result = response
        .get("result")
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("Target.getTargets response did not include result"))?;
    browser_targets_from_result(result)
}

fn browser_targets_from_result(result: Value) -> anyhow::Result<Vec<CdpTarget>> {
    let result = serde_json::from_value::<CdpTargetInfosResult>(result)?;
    Ok(result
        .target_infos
        .unwrap_or_default()
        .into_iter()
        .filter_map(|target| {
            let id = target.target_id?;
            let target_type = target.target_type?;
            Some(CdpTarget {
                id,
                target_type,
                title: Some(target.title.unwrap_or_default()),
                url: Some(target.url.unwrap_or_default()),
                devtools_frontend_url: None,
                web_socket_debugger_url: None,
            })
        })
        .collect())
}

#[cfg(test)]
pub fn pick_codex_page_target(targets: &[CdpTarget]) -> anyhow::Result<CdpTarget> {
    let pages = targets
        .iter()
        .filter(|target| target.target_type == "page" && target.web_socket_debugger_url.is_some());
    let injectable: Vec<&CdpTarget> = pages.collect();
    let codex_page = find_codex_page_target(targets);
    let selected = codex_page
        .cloned()
        .or_else(|| injectable.first().copied().cloned());
    selected.ok_or_else(|| anyhow::anyhow!("No injectable Codex page target found"))
}

#[cfg(test)]
pub fn codex_page_targets(targets: &[CdpTarget]) -> Vec<CdpTarget> {
    targets
        .iter()
        .filter(|target| is_codex_page_target(target) && has_target_websocket(target))
        .cloned()
        .collect()
}

pub fn codex_injectable_page_targets(targets: &[CdpTarget]) -> Vec<CdpTarget> {
    targets
        .iter()
        .filter(|target| is_codex_page_target(target))
        .cloned()
        .collect()
}

#[cfg(test)]
pub fn find_codex_page_target(targets: &[CdpTarget]) -> Option<&CdpTarget> {
    targets
        .iter()
        .find(|target| is_codex_page_target(target) && has_target_websocket(target))
}

fn is_codex_page_target(target: &CdpTarget) -> bool {
    if target.target_type != "page" {
        return false;
    }
    if target.url.as_deref() == Some(CODEX_APP_URL) {
        return true;
    }
    format!(
        "{} {}",
        target.title.as_deref().unwrap_or_default(),
        target.url.as_deref().unwrap_or_default()
    )
    .to_lowercase()
    .contains("codex")
}

#[cfg(test)]
fn has_target_websocket(target: &CdpTarget) -> bool {
    target
        .web_socket_debugger_url
        .as_deref()
        .is_some_and(|url| !url.trim().is_empty())
}

pub async fn connect_cdp_websocket(
    websocket_url: &str,
) -> anyhow::Result<
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>,
> {
    let (socket, _) = tokio::time::timeout(CDP_CONNECT_TIMEOUT, connect_async(websocket_url))
        .await
        .map_err(|_| {
            anyhow::anyhow!(
                "timed out connecting CDP websocket after {}s",
                CDP_CONNECT_TIMEOUT.as_secs()
            )
        })?
        .map_err(|error| anyhow::anyhow!("failed to connect CDP websocket: {error}"))?;
    Ok(socket)
}

pub async fn close_browser(debug_port: u16) -> anyhow::Result<()> {
    let websocket_url = browser_websocket_url(debug_port).await?;
    let socket = connect_cdp_websocket(&websocket_url).await?;
    let mut session = OneShotCdpSession::new(socket);
    session
        .send_command(1, "Browser.close", json!({}))
        .await
        .map(|_| ())
}

fn cdp_command(id: u64, method: &str, params: Value, session_id: Option<&str>) -> Value {
    let mut command = json!({ "id": id, "method": method, "params": params });
    if let Some(session_id) = session_id {
        command["sessionId"] = json!(session_id);
    }
    command
}

struct OneShotCdpSession<S> {
    socket: S,
    responses: HashMap<u64, Value>,
    session_id: Option<String>,
}

impl<S> OneShotCdpSession<S>
where
    S: SinkExt<Message>
        + StreamExt<Item = Result<Message, tokio_tungstenite::tungstenite::Error>>
        + Unpin
        + Send,
    <S as futures_util::Sink<Message>>::Error: std::error::Error + Send + Sync + 'static,
{
    fn new(socket: S) -> Self {
        Self {
            socket,
            responses: HashMap::new(),
            session_id: None,
        }
    }

    async fn send_command(
        &mut self,
        id: u64,
        method: &str,
        params: Value,
    ) -> anyhow::Result<Value> {
        self.socket
            .send(Message::Text(
                cdp_command(id, method, params, self.session_id.as_deref())
                    .to_string()
                    .into(),
            ))
            .await?;
        tokio::time::timeout(CDP_COMMAND_TIMEOUT, self.wait_for_response(id, method))
            .await
            .map_err(|_| {
                anyhow::anyhow!(
                    "timed out waiting for CDP command {method} after {}s",
                    CDP_COMMAND_TIMEOUT.as_secs()
                )
            })?
    }

    async fn wait_for_response(&mut self, id: u64, method: &str) -> anyhow::Result<Value> {
        loop {
            if let Some(response) = self.responses.remove(&id) {
                if let Some(error) = response.get("error") {
                    anyhow::bail!("CDP command {method} failed: {error}");
                }
                return Ok(response);
            }
            let Some(message) = self.next_message().await? else {
                anyhow::bail!("CDP command {method} closed before response");
            };
            if let Some(response_id) = message.get("id").and_then(Value::as_u64) {
                if response_id == id {
                    if let Some(error) = message.get("error") {
                        anyhow::bail!("CDP command {method} failed: {error}");
                    }
                    return Ok(message);
                }
                self.responses.insert(response_id, message);
            }
        }
    }

    async fn next_message(&mut self) -> anyhow::Result<Option<Value>> {
        let Some(message) = self.socket.next().await else {
            return Ok(None);
        };
        let message = message?;
        let value = match message {
            Message::Text(text) => serde_json::from_str(&text)?,
            Message::Binary(bytes) => serde_json::from_slice(&bytes)?,
            _ => json!({}),
        };
        Ok(Some(value))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn target(id: &str, target_type: &str, title: &str, ws: Option<&str>) -> CdpTarget {
        CdpTarget {
            id: id.to_string(),
            target_type: target_type.to_string(),
            title: Some(title.to_string()),
            url: Some(format!("https://example.test/{id}")),
            devtools_frontend_url: None,
            web_socket_debugger_url: ws.map(str::to_string),
        }
    }

    #[test]
    fn cdp_prefers_codex_page_target() {
        let targets = vec![
            target("one", "page", "Other", Some("ws://one")),
            target("two", "page", "Codex", Some("ws://two")),
        ];

        let selected = pick_codex_page_target(&targets).expect("target");

        assert_eq!(selected.id, "two");
    }

    #[test]
    fn cdp_returns_all_codex_page_targets() {
        let mut settling = target(
            "settling",
            "page",
            "app://-/index.html",
            Some("ws://settling"),
        );
        settling.url = Some("app://-/index.html".to_string());
        let targets = vec![
            target("one", "page", "Codex", Some("ws://one")),
            target("two", "page", "Codex", Some("ws://two")),
            settling,
            target("worker", "worker", "Codex", Some("ws://worker")),
            target("missing", "page", "Codex", None),
            target("empty", "page", "Codex", Some("")),
        ];

        let selected = codex_page_targets(&targets);

        assert_eq!(
            selected
                .iter()
                .map(|target| target.id.as_str())
                .collect::<Vec<_>>(),
            vec!["one", "two", "settling"]
        );
    }

    #[test]
    fn cdp_injectable_targets_accept_browser_target_infos() {
        let mut browser_page = target("browser-page", "page", "", None);
        browser_page.url = Some("app://-/index.html".to_string());
        let mut browser_tab = target("browser-tab", "tab", "Codex", None);
        browser_tab.url = Some("app://-/index.html".to_string());
        let mut worker = target("worker", "worker", "Codex", None);
        worker.url = Some("app://-/index.html".to_string());

        let selected = codex_injectable_page_targets(&[browser_page, browser_tab, worker]);

        assert_eq!(
            selected
                .iter()
                .map(|target| target.id.as_str())
                .collect::<Vec<_>>(),
            vec!["browser-page"]
        );
    }

    #[test]
    fn cdp_converts_browser_target_infos() {
        let targets = browser_targets_from_result(json!({
            "targetInfos": [
                {
                    "targetId": "page",
                    "type": "page",
                    "title": "Codex",
                    "url": "app://-/index.html"
                },
                {
                    "targetId": "missing-type",
                    "title": "Codex"
                }
            ]
        }))
        .expect("targets");

        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].id, "page");
        assert_eq!(targets[0].target_type, "page");
        assert_eq!(targets[0].web_socket_debugger_url, None);
    }

    #[test]
    fn cdp_rejects_missing_websocket_targets() {
        let targets = vec![target("one", "page", "Codex", None)];

        let error = pick_codex_page_target(&targets).unwrap_err();

        assert_eq!(error.to_string(), "No injectable Codex page target found");
    }

    #[test]
    fn cdp_strict_rejects_non_codex_page_target() {
        let targets = vec![target("one", "page", "Other", Some("ws://one"))];

        assert!(find_codex_page_target(&targets).is_none());
    }

    #[tokio::test]
    async fn cdp_wait_for_debug_port_to_close_returns_when_port_is_absent() {
        wait_for_debug_port_to_close(9, Duration::from_millis(50))
            .await
            .expect("closed port");
    }
}
