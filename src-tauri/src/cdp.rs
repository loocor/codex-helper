use std::time::Duration;

use serde::{Deserialize, Serialize};
use tokio_tungstenite::connect_async;

const CDP_CONNECT_TIMEOUT: Duration = Duration::from_secs(5);

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
    let codex_page = injectable.iter().find(|target| {
        format!(
            "{} {}",
            target.title.as_deref().unwrap_or_default(),
            target.url.as_deref().unwrap_or_default()
        )
        .to_lowercase()
        .contains("codex")
    });
    let selected = codex_page.copied().or_else(|| injectable.first().copied());
    selected
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("No injectable Codex page target found"))
}

pub async fn wait_for_codex_target(debug_port: u16) -> anyhow::Result<CdpTarget> {
    let mut last_error = None;
    for _ in 0..40 {
        match list_targets(debug_port)
            .await
            .and_then(|targets| pick_codex_page_target(&targets))
        {
            Ok(target) => return Ok(target),
            Err(error) => {
                last_error = Some(error);
                tokio::time::sleep(Duration::from_millis(250)).await;
            }
        }
    }
    Err(last_error.unwrap_or_else(|| anyhow::anyhow!("Timed out waiting for Codex CDP target")))
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
    fn cdp_rejects_missing_websocket_targets() {
        let targets = vec![target("one", "page", "Codex", None)];

        let error = pick_codex_page_target(&targets).unwrap_err();

        assert_eq!(error.to_string(), "No injectable Codex page target found");
    }
}
