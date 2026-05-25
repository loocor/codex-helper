use std::sync::Arc;

use serde_json::{json, Value};

use crate::cdp::{list_targets, pick_codex_page_target, CdpTarget};
use crate::logging::DiagnosticLogger;
use crate::ports::{
    discover_remote_listening_ports, discovery_request_from_payload, request_from_payload,
    PortForwardManager,
};
use crate::session_actions::{
    delete_session_response, deleted_sessions_response, export_markdown_response,
    move_thread_workspace_response, restore_deleted_session_response, undo_delete_response,
};
use crate::settings::{read_settings, update_settings};
use crate::state_dir::StateDir;
use crate::zed::{
    fallback_open_request_response, open_zed_remote, resolve_ssh_target_for_host_id,
    resolve_ssh_target_response, zed_remote_status,
};

#[derive(Clone)]
pub struct BridgeContext {
    pub state_dir: StateDir,
    pub logger: Arc<DiagnosticLogger>,
    pub debug_port: u16,
    pub port_manager: PortForwardManager,
}

pub async fn handle_bridge_request(ctx: BridgeContext, path: &str, payload: Value) -> Value {
    match path {
        "/backend/status" => json!({
            "status": "ok",
            "message": "Codex Helper backend connected",
        }),
        "/diagnostics/log" => {
            let event = payload
                .get("event")
                .and_then(Value::as_str)
                .unwrap_or("renderer.event");
            match ctx.logger.append(event, payload.clone()) {
                Ok(()) => json!({ "status": "ok" }),
                Err(error) => json!({ "status": "failed", "message": error.to_string() }),
            }
        }
        "/runtime/user-scripts" => match user_script_inventory(&ctx.state_dir) {
            Ok(scripts) => json!({
                "status": "ok",
                "path": ctx.state_dir.scripts_dir.to_string_lossy(),
                "scripts": scripts,
            }),
            Err(error) => json!({ "status": "failed", "message": error.to_string() }),
        },
        "/settings/get" => match read_settings(&ctx.state_dir.config_path) {
            Ok(settings) => json!({ "status": "ok", "settings": settings }),
            Err(error) => json!({ "status": "failed", "message": error.to_string() }),
        },
        "/settings/set" => match update_settings(&ctx.state_dir.config_path, &payload) {
            Ok(settings) => json!({ "status": "ok", "settings": settings }),
            Err(error) => json!({ "status": "failed", "message": error.to_string() }),
        },
        "/diagnostics/read-latest" => read_latest_log_response(&ctx.logger),
        "/diagnostics/reveal-log" => reveal_path_response(ctx.logger.log_path()),
        "/logs/reveal" => reveal_path_response(&ctx.state_dir.logs_dir),
        "/scripts/reveal" => reveal_path_response(&ctx.state_dir.scripts_dir),
        "/backups/reveal" => reveal_path_response(&ctx.state_dir.backups_dir),
        "/state/reveal" => reveal_path_response(&ctx.state_dir.root),
        "/devtools/open" => open_devtools_response(ctx.debug_port).await,
        "/url/open-external" => open_external_local_url_response(&payload),
        "/delete" => delete_session_response(&ctx.state_dir, &payload),
        "/undo" => undo_delete_response(&ctx.state_dir, &payload),
        "/backups/list" => deleted_sessions_response(&ctx.state_dir),
        "/backups/restore" => restore_deleted_session_response(&ctx.state_dir, &payload),
        "/export-markdown" => export_markdown_response(&payload),
        "/move-thread-workspace" => move_thread_workspace_response(&ctx.state_dir, &payload),
        "/ports/list" => ctx.port_manager.list().await,
        "/ports/discover" => match discovery_request_from_payload(&payload) {
            Ok(request) => match resolve_ssh_target_for_host_id(&request.host_id, None) {
                Ok(target) => match discover_remote_listening_ports(&request, &target).await {
                    Ok(ports) => json!({
                        "status": "ok",
                        "hostId": request.host_id,
                        "remotePath": request.remote_path,
                        "threadId": request.thread_id,
                        "ports": ports,
                    }),
                    Err(message) => json!({ "status": "failed", "message": message }),
                },
                Err(error) => json!({ "status": "failed", "message": error.to_string() }),
            },
            Err(message) => json!({ "status": "failed", "message": message }),
        },
        "/ports/forward" => match request_from_payload(&payload) {
            Ok(request) => match resolve_ssh_target_for_host_id(&request.host_id, None) {
                Ok(target) => ctx.port_manager.start(request, target).await,
                Err(error) => json!({ "status": "failed", "message": error.to_string() }),
            },
            Err(message) => json!({ "status": "failed", "message": message }),
        },
        "/ports/stop" => {
            let id = payload.get("id").and_then(Value::as_str).unwrap_or("");
            ctx.port_manager.stop(id).await
        }
        "/zed-remote/status" => zed_remote_status(),
        "/zed-remote/resolve-host" => resolve_ssh_target_response(&payload),
        "/zed-remote/fallback-request" => fallback_open_request_response(&payload),
        "/zed-remote/open" => open_zed_remote(&payload),
        _ => json!({
            "status": "failed",
            "message": format!("Unknown Codex Helper bridge path: {path}")
        }),
    }
}

fn user_script_inventory(state_dir: &StateDir) -> anyhow::Result<Vec<String>> {
    let mut scripts = Vec::new();
    for entry in std::fs::read_dir(&state_dir.scripts_dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|value| value.to_str()) == Some("js") {
            if let Some(name) = path.file_name().and_then(|value| value.to_str()) {
                scripts.push(name.to_string());
            }
        }
    }
    scripts.sort();
    Ok(scripts)
}

fn read_latest_log_response(logger: &DiagnosticLogger) -> Value {
    match std::fs::read_to_string(logger.log_path()) {
        Ok(contents) => {
            let lines = contents.lines().rev().take(80).collect::<Vec<_>>();
            let latest = lines.into_iter().rev().collect::<Vec<_>>().join("\n");
            json!({
                "status": "ok",
                "path": logger.log_path().to_string_lossy(),
                "contents": latest,
            })
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => json!({
            "status": "ok",
            "path": logger.log_path().to_string_lossy(),
            "contents": "",
        }),
        Err(error) => json!({ "status": "failed", "message": error.to_string() }),
    }
}

fn reveal_path_response(path: &std::path::Path) -> Value {
    let result = if path.is_dir() {
        std::process::Command::new("open").arg(path).spawn()
    } else {
        std::process::Command::new("open")
            .arg("-R")
            .arg(path)
            .spawn()
    };
    match result {
        Ok(_) => json!({ "status": "ok", "path": path.to_string_lossy() }),
        Err(error) => json!({ "status": "failed", "message": error.to_string() }),
    }
}

async fn open_devtools_response(debug_port: u16) -> Value {
    let target = match list_targets(debug_port)
        .await
        .and_then(|targets| pick_codex_page_target(&targets))
    {
        Ok(target) => target,
        Err(error) => return json!({ "status": "failed", "message": error.to_string() }),
    };
    let target_id = target.id.clone();
    if target_id.trim().is_empty() {
        return json!({
            "status": "failed",
            "message": "Codex DevTools target id is empty",
        });
    }
    let url = match devtools_url(debug_port, &target) {
        Ok(url) => url,
        Err(error) => return json!({ "status": "failed", "message": error.to_string() }),
    };
    match std::process::Command::new("open").arg(&url).spawn() {
        Ok(_) => json!({ "status": "ok", "targetId": target_id, "url": url }),
        Err(error) => json!({ "status": "failed", "message": error.to_string() }),
    }
}

fn open_external_local_url_response(payload: &Value) -> Value {
    let url = match local_browser_url_from_payload(payload) {
        Ok(url) => url,
        Err(message) => return json!({ "status": "failed", "message": message }),
    };
    match std::process::Command::new("open").arg(&url).spawn() {
        Ok(_) => json!({ "status": "ok", "url": url }),
        Err(error) => json!({ "status": "failed", "message": error.to_string() }),
    }
}

fn local_browser_url_from_payload(payload: &Value) -> Result<String, String> {
    let raw = payload
        .get("url")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "URL is required".to_string())?;
    let rest = if let Some(rest) = raw.strip_prefix("http://") {
        rest
    } else if let Some(rest) = raw.strip_prefix("https://") {
        rest
    } else {
        return Err("Only http(s) URLs can be opened".to_string());
    };
    let authority = rest
        .split(['/', '?', '#'])
        .next()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "URL is invalid".to_string())?;
    if authority.contains('@') {
        return Err("URL is invalid".to_string());
    }
    let (host, port) = if let Some(after_bracket) = authority.strip_prefix('[') {
        let (host, suffix) = after_bracket
            .split_once(']')
            .ok_or_else(|| "URL is invalid".to_string())?;
        let port = suffix
            .strip_prefix(':')
            .ok_or_else(|| "Local forwarded URL must include a port".to_string())?;
        (host, port)
    } else {
        let (host, port) = authority
            .rsplit_once(':')
            .ok_or_else(|| "Local forwarded URL must include a port".to_string())?;
        (host, port)
    };
    let normalized_host = host.to_ascii_lowercase();
    if !matches!(
        normalized_host.as_str(),
        "localhost" | "127.0.0.1" | "::1" | "0.0.0.0"
    ) {
        return Err("Only local forwarded URLs can be opened".to_string());
    }
    if port.is_empty() || !port.chars().all(|value| value.is_ascii_digit()) {
        return Err("Local forwarded URL must include a port".to_string());
    }
    Ok(raw.to_string())
}

pub fn devtools_url(debug_port: u16, target: &CdpTarget) -> anyhow::Result<String> {
    if let Some(frontend_url) = target
        .devtools_frontend_url
        .as_deref()
        .map(str::trim)
        .filter(|url| !url.is_empty())
    {
        return Ok(normalize_devtools_frontend_url(debug_port, frontend_url));
    }

    let websocket_url = target
        .web_socket_debugger_url
        .as_deref()
        .map(str::trim)
        .filter(|url| !url.is_empty())
        .ok_or_else(|| anyhow::anyhow!("Selected Codex DevTools target has no websocket URL"))?;
    let websocket_endpoint = websocket_url
        .strip_prefix("ws://")
        .ok_or_else(|| anyhow::anyhow!("Codex DevTools websocket URL must start with ws://"))?;
    Ok(format!(
        "http://127.0.0.1:{debug_port}/devtools/inspector.html?ws={websocket_endpoint}"
    ))
}

fn normalize_devtools_frontend_url(debug_port: u16, frontend_url: &str) -> String {
    if frontend_url.starts_with("http://")
        || frontend_url.starts_with("https://")
        || frontend_url.starts_with("devtools://")
        || frontend_url.starts_with("chrome-devtools://")
    {
        return frontend_url.to_string();
    }
    if frontend_url.starts_with('/') {
        return format!("http://127.0.0.1:{debug_port}{frontend_url}");
    }
    format!("http://127.0.0.1:{debug_port}/{frontend_url}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn devtools_url_targets_selected_page() {
        let target = CdpTarget {
            id: "target-1".to_string(),
            target_type: "page".to_string(),
            title: Some("Codex".to_string()),
            url: Some("https://codex.test".to_string()),
            devtools_frontend_url: None,
            web_socket_debugger_url: Some("ws://127.0.0.1:9229/devtools/page/target-1".to_string()),
        };

        assert_eq!(
            devtools_url(9229, &target).expect("devtools url"),
            "http://127.0.0.1:9229/devtools/inspector.html?ws=127.0.0.1:9229/devtools/page/target-1"
        );
    }

    #[test]
    fn devtools_url_uses_reported_websocket_endpoint() {
        let target = CdpTarget {
            id: "target-1".to_string(),
            target_type: "page".to_string(),
            title: Some("Codex".to_string()),
            url: Some("https://codex.test".to_string()),
            devtools_frontend_url: None,
            web_socket_debugger_url: Some(
                "ws://localhost:9229/devtools/page/reported-target".to_string(),
            ),
        };

        assert_eq!(
            devtools_url(9229, &target).expect("devtools url"),
            "http://127.0.0.1:9229/devtools/inspector.html?ws=localhost:9229/devtools/page/reported-target"
        );
    }

    #[test]
    fn devtools_url_expands_relative_frontend_url() {
        let target = CdpTarget {
            id: "target-1".to_string(),
            target_type: "page".to_string(),
            title: Some("Codex".to_string()),
            url: Some("https://codex.test".to_string()),
            devtools_frontend_url: Some(
                "/devtools/inspector.html?ws=localhost:9229/devtools/page/target-1".to_string(),
            ),
            web_socket_debugger_url: Some("ws://localhost:9229/devtools/page/target-1".to_string()),
        };

        assert_eq!(
            devtools_url(9229, &target).expect("devtools url"),
            "http://127.0.0.1:9229/devtools/inspector.html?ws=localhost:9229/devtools/page/target-1"
        );
    }

    #[test]
    fn local_browser_url_rejects_external_hosts() {
        let payload = json!({ "url": "https://example.com:3000" });

        assert_eq!(
            local_browser_url_from_payload(&payload).expect_err("external host"),
            "Only local forwarded URLs can be opened"
        );
    }

    #[test]
    fn local_browser_url_accepts_localhost_with_port() {
        let payload = json!({ "url": "http://localhost:3000/path" });

        assert_eq!(
            local_browser_url_from_payload(&payload).expect("local url"),
            "http://localhost:3000/path"
        );
    }
}
