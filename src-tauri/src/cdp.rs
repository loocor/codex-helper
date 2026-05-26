use std::collections::HashMap;
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;

const CDP_CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const CDP_COMMAND_TIMEOUT: Duration = Duration::from_secs(5);

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

pub async fn is_debug_port_ready(debug_port: u16) -> bool {
    browser_websocket_url(debug_port).await.is_ok()
}

pub async fn has_codex_cdp_target(debug_port: u16) -> bool {
    match list_targets(debug_port).await {
        Ok(targets) => find_codex_page_target(&targets).is_some(),
        Err(_) => false,
    }
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

pub async fn browser_websocket_url(debug_port: u16) -> anyhow::Result<String> {
    let url = format!("http://127.0.0.1:{debug_port}/json/version");
    let response = reqwest::get(&url)
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
    let response = reqwest::get(&url)
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

pub fn codex_page_targets(targets: &[CdpTarget]) -> Vec<CdpTarget> {
    targets
        .iter()
        .filter(|target| is_codex_page_target(target))
        .cloned()
        .collect()
}

pub fn find_codex_page_target(targets: &[CdpTarget]) -> Option<&CdpTarget> {
    targets.iter().find(|target| is_codex_page_target(target))
}

fn is_codex_page_target(target: &CdpTarget) -> bool {
    target.target_type == "page"
        && target
            .web_socket_debugger_url
            .as_deref()
            .is_some_and(|url| !url.trim().is_empty())
        && format!(
            "{} {}",
            target.title.as_deref().unwrap_or_default(),
            target.url.as_deref().unwrap_or_default()
        )
        .to_lowercase()
        .contains("codex")
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

pub async fn reload_codex_page(debug_port: u16) -> anyhow::Result<()> {
    let websocket_url = browser_websocket_url(debug_port).await?;
    let targets = list_targets(debug_port).await?;
    let target = pick_codex_page_target(&targets)?;
    let target_id = target.id;

    let socket = connect_cdp_websocket(&websocket_url).await?;
    let mut session = OneShotCdpSession::new(socket);
    let attached = session
        .send_command(
            1,
            "Target.attachToTarget",
            json!({ "targetId": target_id, "flatten": true }),
        )
        .await?;
    let session_id = attached
        .get("result")
        .and_then(|result| result.get("sessionId"))
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("CDP attach response did not include sessionId"))?
        .to_string();
    session.set_session_id(session_id);
    session
        .send_command(2, "Page.reload", json!({ "ignoreCache": false }))
        .await?;
    Ok(())
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

    fn set_session_id(&mut self, session_id: String) {
        self.session_id = Some(session_id);
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
        let targets = vec![
            target("one", "page", "Codex", Some("ws://one")),
            target("two", "page", "Codex", Some("ws://two")),
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
            vec!["one", "two"]
        );
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
}
