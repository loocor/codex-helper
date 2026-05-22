use std::sync::Arc;

use serde_json::{json, Value};

use crate::cdp::{list_targets, pick_codex_page_target};
use crate::logging::DiagnosticLogger;
use crate::ports::{request_from_payload, PortForwardManager};
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
            Ok(scripts) => json!({ "status": "ok", "scripts": scripts }),
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
        "/state/reveal" => reveal_path_response(&ctx.state_dir.root),
        "/devtools/open" => open_devtools_response(ctx.debug_port).await,
        "/delete" => delete_session_response(&ctx.state_dir, &payload),
        "/undo" => undo_delete_response(&ctx.state_dir, &payload),
        "/backups/list" => deleted_sessions_response(&ctx.state_dir),
        "/backups/restore" => restore_deleted_session_response(&ctx.state_dir, &payload),
        "/export-markdown" => export_markdown_response(&payload),
        "/move-thread-workspace" => move_thread_workspace_response(&ctx.state_dir, &payload),
        "/ports/list" => ctx.port_manager.list().await,
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
    let target_id = target.id;
    if target_id.trim().is_empty() {
        return json!({
            "status": "failed",
            "message": "Codex DevTools target id is empty",
        });
    }
    let url = target
        .devtools_frontend_url
        .filter(|url| !url.trim().is_empty())
        .unwrap_or_else(|| devtools_url(debug_port, &target_id));
    match std::process::Command::new("open").arg(&url).spawn() {
        Ok(_) => json!({ "status": "ok", "targetId": target_id, "url": url }),
        Err(error) => json!({ "status": "failed", "message": error.to_string() }),
    }
}

pub fn devtools_url(debug_port: u16, target_id: &str) -> String {
    format!(
        "http://127.0.0.1:{debug_port}/devtools/inspector.html?ws=127.0.0.1:{debug_port}/devtools/page/{target_id}"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn devtools_url_targets_selected_page() {
        assert_eq!(
            devtools_url(9229, "target-1"),
            "http://127.0.0.1:9229/devtools/inspector.html?ws=127.0.0.1:9229/devtools/page/target-1"
        );
    }
}
