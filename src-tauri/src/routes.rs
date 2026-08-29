use std::sync::{Arc, Mutex};

use serde_json::{json, Value};
use tauri_plugin_opener::{open_path, open_url, reveal_item_in_dir};

use crate::bridge::{BridgeCaller, BridgeRequest};
use crate::cdp::{list_targets, CdpTarget};
use crate::codex_live::default_codex_home;
use crate::endpoint;
use crate::logging::DiagnosticLogger;
use crate::ports::{
    discover_remote_listening_ports, discovery_request_from_payload, request_from_payload,
    PortForwardManager,
};
use crate::provider_oauth::{oauth_status, poll_oauth, start_oauth, OAuthKind};
use crate::provider_proxy::{
    fetch_provider_models, global_provider_proxy, test_provider_connection,
};
use crate::provider_usage::{
    attach_usage_page_urls, query_provider_usage, usage_page_url_for_store,
    validated_usage_page_url,
};
use crate::providers::{
    activate_provider, delete_provider, list_response, provider_api_key, read_store,
    reorder_providers, upsert_provider, LiveRefresh,
};
use crate::settings::{read_settings, update_settings};
use crate::settings_window::{settings_page_id, OpenSettings, SETTINGS_WINDOW_TARGET_ID};
use crate::state_dir::StateDir;
use crate::zed::{
    fallback_open_request_response, resolve_ssh_target_for_host_id, resolve_ssh_target_response,
};

#[derive(Clone)]
pub struct BridgeContext {
    pub state_dir: StateDir,
    pub logger: Arc<DiagnosticLogger>,
    pub debug_port: u16,
    pub port_manager: PortForwardManager,
    pub runtime_activity: RuntimeActivity,
    pub open_settings: Option<OpenSettings>,
}

#[derive(Clone, Default)]
pub struct RuntimeActivity {
    inner: Arc<Mutex<Option<RuntimeActivitySnapshot>>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeActivitySnapshot {
    pub target_id: String,
    pub helper_instance_id: String,
    pub href: String,
    pub has_focus: bool,
    pub visibility_state: String,
}

impl RuntimeActivity {
    pub fn record(&self, caller: &BridgeCaller) {
        if caller.target_id.trim().is_empty() || caller.helper_instance_id.trim().is_empty() {
            return;
        }
        *self.inner.lock().expect("runtime activity poisoned") = Some(RuntimeActivitySnapshot {
            target_id: caller.target_id.clone(),
            helper_instance_id: caller.helper_instance_id.clone(),
            href: caller.href.clone(),
            has_focus: caller.has_focus,
            visibility_state: caller.visibility_state.clone(),
        });
    }

    pub fn last(&self) -> Option<RuntimeActivitySnapshot> {
        self.inner
            .lock()
            .expect("runtime activity poisoned")
            .clone()
    }
}

fn devtools_target_id(ctx: &BridgeContext, caller: &BridgeCaller) -> String {
    if caller.target_id == SETTINGS_WINDOW_TARGET_ID {
        return ctx
            .runtime_activity
            .last()
            .map(|snapshot| snapshot.target_id)
            .unwrap_or_default();
    }
    caller.target_id.clone()
}

fn settings_only_action(caller: &BridgeCaller) -> Option<Value> {
    if caller.target_id == SETTINGS_WINDOW_TARGET_ID {
        None
    } else {
        Some(json!({
            "status": "failed",
            "message": "This action is only available in Helper Settings",
        }))
    }
}

pub async fn handle_bridge_request(ctx: BridgeContext, request: BridgeRequest) -> Value {
    let BridgeRequest {
        path,
        payload,
        caller,
        ..
    } = request;
    let response = match path.as_str() {
        "/backend/status" => json!({
            "status": "ok",
            "message": "Codex Helper backend connected",
        }),
        "/diagnostics/log" => {
            let event = payload
                .get("event")
                .and_then(Value::as_str)
                .unwrap_or("renderer.event");
            let detail = compact_diagnostic_detail(
                payload.get("detail").cloned().unwrap_or_else(|| json!({})),
            );
            match ctx.logger.append(event, detail) {
                Ok(()) => json!({ "status": "ok" }),
                Err(error) => json!({ "status": "failed", "message": error.to_string() }),
            }
        }
        "/runtime/activity" => {
            ctx.runtime_activity.record(&caller);
            json!({ "status": "ok" })
        }
        "/runtime/user-scripts" => match user_script_inventory(&ctx.state_dir) {
            Ok(scripts) => json!({
                "status": "ok",
                "path": ctx.state_dir.scripts_dir.to_string_lossy(),
                "scripts": scripts,
            }),
            Err(error) => json!({ "status": "failed", "message": error.to_string() }),
        },
        "/settings/open" => match &ctx.open_settings {
            Some(open) => match open(settings_page_id(
                payload
                    .get("page")
                    .and_then(Value::as_str)
                    .unwrap_or("general"),
            )) {
                Ok(()) => json!({ "status": "ok" }),
                Err(message) => json!({ "status": "failed", "message": message }),
            },
            None => json!({
                "status": "failed",
                "message": "Helper Settings window is unavailable",
            }),
        },
        "/settings/get" => match read_settings(&ctx.state_dir.config_path) {
            Ok(settings) => json!({ "status": "ok", "settings": settings }),
            Err(error) => json!({ "status": "failed", "message": error.to_string() }),
        },
        "/settings/set" => match update_settings(&ctx.state_dir.config_path, &payload) {
            Ok(settings) => json!({ "status": "ok", "settings": settings }),
            Err(error) => json!({ "status": "failed", "message": error.to_string() }),
        },
        "/providers/list" => providers_list_response(&ctx.state_dir.root),
        "/providers/save" => {
            if let Some(response) = settings_only_action(&caller) {
                response
            } else {
                let response = providers_save_response(&ctx.state_dir.root, &payload);
                log_provider_event(&ctx.logger, "providers.saved", &response);
                response
            }
        }
        "/providers/delete" => {
            if let Some(response) = settings_only_action(&caller) {
                response
            } else {
                let response = providers_delete_response(&ctx.state_dir.root, &payload);
                log_provider_event(&ctx.logger, "providers.deleted", &response);
                response
            }
        }
        "/providers/activate" => {
            let response = providers_activate_response(&ctx.state_dir.root, &payload);
            log_provider_event(&ctx.logger, "providers.activated", &response);
            response
        }
        "/providers/reorder" => {
            if let Some(response) = settings_only_action(&caller) {
                response
            } else {
                let response = providers_reorder_response(&ctx.state_dir.root, &payload);
                log_provider_event(&ctx.logger, "providers.reordered", &response);
                response
            }
        }
        "/providers/test" => {
            if let Some(response) = settings_only_action(&caller) {
                response
            } else {
                let response = providers_test_response(&ctx.state_dir.root, &payload).await;
                log_provider_event(&ctx.logger, "providers.tested", &response);
                response
            }
        }
        "/providers/models" => {
            if let Some(response) = settings_only_action(&caller) {
                response
            } else {
                let response = providers_models_response(&ctx.state_dir.root, &payload).await;
                log_provider_event(&ctx.logger, "providers.models_fetched", &response);
                response
            }
        }
        "/providers/oauth/start" => {
            if let Some(response) = settings_only_action(&caller) {
                response
            } else {
                let response = providers_oauth_start_response(&ctx.state_dir.root, &payload).await;
                log_provider_event(&ctx.logger, "providers.oauth_started", &response);
                response
            }
        }
        "/providers/oauth/poll" => {
            if let Some(response) = settings_only_action(&caller) {
                response
            } else {
                let response = providers_oauth_poll_response(&ctx.state_dir.root, &payload).await;
                log_provider_event(&ctx.logger, "providers.oauth_polled", &response);
                response
            }
        }
        "/providers/oauth/status" => {
            let response = providers_oauth_status_response(&ctx.state_dir.root, &payload);
            log_provider_event(&ctx.logger, "providers.oauth_status", &response);
            response
        }
        "/providers/usage" => {
            let response = providers_usage_response(&ctx.state_dir.root, &payload).await;
            log_provider_event(&ctx.logger, "providers.usage", &response);
            response
        }
        "/providers/usage/open" => {
            let response = providers_usage_open_response(&ctx.state_dir.root, &payload);
            log_provider_event(&ctx.logger, "providers.usage_open", &response);
            response
        }
        "/providers/secret" => {
            if let Some(response) = settings_only_action(&caller) {
                response
            } else {
                let response = providers_secret_response(&ctx.state_dir.root, &payload);
                log_provider_event(&ctx.logger, "providers.secret", &response);
                response
            }
        }
        "/endpoint/get" => {
            if let Some(response) = settings_only_action(&caller) {
                response
            } else {
                endpoint_get_response(&ctx.state_dir.root)
            }
        }
        "/endpoint/keys" => {
            if let Some(response) = settings_only_action(&caller) {
                response
            } else {
                let response = endpoint_create_key_response(&ctx.state_dir.root, &payload);
                log_provider_event(&ctx.logger, "endpoint.key_created", &response);
                response
            }
        }
        "/endpoint/keys/delete" => {
            if let Some(response) = settings_only_action(&caller) {
                response
            } else {
                let response = endpoint_delete_key_response(&ctx.state_dir.root, &payload);
                log_provider_event(&ctx.logger, "endpoint.key_deleted", &response);
                response
            }
        }
        "/diagnostics/read-latest" => read_latest_log_response(&ctx.logger),
        "/diagnostics/list" => list_logs_response(&ctx.logger, &payload),
        "/diagnostics/search" => search_logs_response(&ctx.logger, &payload),
        "/diagnostics/reveal-log" => reveal_path_response(&ctx.logger.log_path()),
        "/logs/reveal" => reveal_path_response(&ctx.state_dir.logs_dir),
        "/scripts/reveal" => reveal_path_response(&ctx.state_dir.scripts_dir),
        "/state/reveal" => reveal_path_response(&ctx.state_dir.root),
        "/devtools/open" => {
            open_devtools_response(ctx.debug_port, &devtools_target_id(&ctx, &caller)).await
        }
        "/url/open-external" => open_external_local_url_response(&payload),
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
        "/zed-remote/resolve-host" => resolve_ssh_target_response(&payload),
        "/zed-remote/fallback-request" => fallback_open_request_response(&payload),
        _ => json!({
            "status": "failed",
            "message": format!("Unknown Codex Helper bridge path: {path}")
        }),
    };
    log_bridge_request(&ctx.logger, &path, &caller, &response);
    response
}

fn log_bridge_request(
    logger: &DiagnosticLogger,
    path: &str,
    caller: &BridgeCaller,
    response: &Value,
) {
    let status = response
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    if !should_log_bridge_request(path, status) {
        return;
    }
    let _ = logger.append(
        "bridge.request",
        bridge_request_diagnostic(path, caller, response),
    );
}

fn should_log_bridge_request(_path: &str, status: &str) -> bool {
    status != "ok"
}

fn compact_diagnostic_detail(detail: Value) -> Value {
    let serialized = detail.to_string();
    if serialized.len() <= 2048 {
        return detail;
    }
    json!({
        "truncated": true,
        "preview": serialized.chars().take(2048).collect::<String>(),
    })
}

fn bridge_request_diagnostic(path: &str, caller: &BridgeCaller, response: &Value) -> Value {
    let mut diagnostic = serde_json::Map::new();
    diagnostic.insert("path".to_string(), json!(path));
    diagnostic.insert(
        "status".to_string(),
        json!(response
            .get("status")
            .and_then(Value::as_str)
            .unwrap_or("unknown")),
    );
    diagnostic.insert("caller".to_string(), json!(caller));
    if let Some(message) = response.get("message").and_then(Value::as_str) {
        if !message.is_empty() {
            diagnostic.insert("message".to_string(), json!(message));
        }
    }
    Value::Object(diagnostic)
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
    match logger.read_latest() {
        Ok(page) => log_page_response(page),
        Err(error) => json!({ "status": "failed", "message": error.to_string() }),
    }
}

fn list_logs_response(logger: &DiagnosticLogger, payload: &Value) -> Value {
    match log_page_args(payload).and_then(|(date, cursor, limit)| {
        log_event_filter(payload).and_then(|event| {
            logger.list_records(date.as_deref(), cursor.as_deref(), limit, event.as_deref())
        })
    }) {
        Ok(page) => log_page_response(page),
        Err(error) => json!({ "status": "failed", "message": error.to_string() }),
    }
}

fn search_logs_response(logger: &DiagnosticLogger, payload: &Value) -> Value {
    let pattern = payload.get("pattern").and_then(Value::as_str).unwrap_or("");
    let regex = payload
        .get("regex")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    match log_page_args(payload).and_then(|(date, cursor, limit)| {
        log_event_filter(payload).and_then(|event| {
            logger.search_records(
                pattern,
                regex,
                date.as_deref(),
                cursor.as_deref(),
                limit,
                event.as_deref(),
            )
        })
    }) {
        Ok(page) => json!({
            "status": "ok",
            "path": page.path.to_string_lossy(),
            "dates": page.dates,
            "matches": page
                .matches
                .iter()
                .map(|record| json!({
                    "date": record.date,
                    "path": record.path.to_string_lossy(),
                    "timestamp": record.timestamp,
                    "event": record.event,
                    "summary": record.summary,
                    "preview": record.preview,
                    "detail": record.detail,
                }))
                .collect::<Vec<_>>(),
            "cursor": page.cursor,
            "hasMore": page.has_more,
        }),
        Err(error) => json!({ "status": "failed", "message": error.to_string() }),
    }
}

fn log_event_filter(payload: &Value) -> anyhow::Result<Option<String>> {
    let Some(value) = payload.get("event").and_then(Value::as_str) else {
        return Ok(None);
    };
    let value = value.trim();
    if value.is_empty() {
        return Ok(None);
    }
    if value.chars().count() > 128 {
        anyhow::bail!("Log event filter is too long");
    }
    if value.contains('\n') || value.contains('\r') {
        anyhow::bail!("Log event filter is invalid");
    }
    Ok(Some(value.to_string()))
}

fn log_page_args(payload: &Value) -> anyhow::Result<(Option<String>, Option<String>, usize)> {
    let date = payload
        .get("date")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let cursor = payload
        .get("cursor")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let limit = match payload.get("limit") {
        None => 50,
        Some(Value::Number(value)) => {
            let limit = value
                .as_u64()
                .ok_or_else(|| anyhow::anyhow!("Log page limit must be a number"))?
                as usize;
            if limit == 0 {
                anyhow::bail!("Log page limit must be greater than 0");
            }
            if limit > 200 {
                anyhow::bail!("Log page limit must be at most 200");
            }
            limit
        }
        Some(_) => anyhow::bail!("Log page limit must be a number"),
    };
    Ok((date, cursor, limit))
}

fn log_page_response(page: crate::logging::LogPage) -> Value {
    json!({
        "status": "ok",
        "path": page.path.to_string_lossy(),
        "date": page.date,
        "dates": page.dates,
        "records": page
            .records
            .iter()
            .map(|record| json!({
                "timestamp": record.timestamp,
                "event": record.event,
                "summary": record.summary,
                "detail": record.detail,
            }))
            .collect::<Vec<_>>(),
        "cursor": page.cursor,
        "hasMore": page.has_more,
    })
}

fn endpoint_get_response(state_root: &std::path::Path) -> Value {
    match endpoint::read_store(state_root) {
        Ok(store) => endpoint_store_response(state_root, store),
        Err(error) => json!({ "status": "failed", "message": error.to_string() }),
    }
}

fn endpoint_create_key_response(state_root: &std::path::Path, payload: &Value) -> Value {
    match endpoint::create_key(state_root, payload) {
        Ok(store) => endpoint_store_response(state_root, store),
        Err(error) => json!({ "status": "failed", "message": error.to_string() }),
    }
}

fn endpoint_delete_key_response(state_root: &std::path::Path, payload: &Value) -> Value {
    match endpoint::delete_key(state_root, payload) {
        Ok(store) => endpoint_store_response(state_root, store),
        Err(error) => json!({ "status": "failed", "message": error.to_string() }),
    }
}

fn endpoint_store_response(state_root: &std::path::Path, store: endpoint::EndpointStore) -> Value {
    let provider = global_provider_proxy()
        .active_provider()
        .ok()
        .flatten()
        .or_else(|| {
            crate::providers::read_store(state_root)
                .ok()
                .and_then(|store| {
                    let active_id = store.active_id.clone();
                    store
                        .providers
                        .into_iter()
                        .find(|item| item.id == active_id)
                })
        });
    match global_provider_proxy().base_url() {
        Ok(url) => endpoint::list_response(&store, &url, provider.as_ref()),
        Err(error) => {
            let mut response = endpoint::list_response(&store, "", provider.as_ref());
            response["proxyError"] = json!(error.to_string());
            response
        }
    }
}

fn providers_list_response(state_root: &std::path::Path) -> Value {
    match read_store(state_root) {
        Ok(store) => provider_store_response(store),
        Err(error) => json!({ "status": "failed", "message": error.to_string() }),
    }
}

fn provider_store_response(store: crate::providers::ProviderStore) -> Value {
    let proxy = global_provider_proxy();
    proxy.set_store(store.clone());
    let mut response = match proxy.base_url() {
        Ok(url) => list_response(&store, &url),
        Err(error) => {
            let mut response = list_response(&store, "");
            response["proxyError"] = json!(error.to_string());
            response
        }
    };
    attach_usage_page_urls(&mut response, &store);
    response
}

fn providers_save_response(state_root: &std::path::Path, payload: &Value) -> Value {
    match upsert_provider(state_root, payload) {
        Ok((store, saved_id)) => {
            if store.active_id == saved_id {
                return project_named_provider(state_root, &saved_id.clone(), Some(saved_id));
            }
            let mut response = provider_store_response(store);
            response["savedId"] = json!(saved_id);
            response
        }
        Err(error) => json!({ "status": "failed", "message": error.to_string() }),
    }
}

fn providers_delete_response(state_root: &std::path::Path, payload: &Value) -> Value {
    let id = payload
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim();
    match delete_provider(state_root, id, &default_codex_home()) {
        Ok(store) => provider_store_response(store),
        Err(error) => json!({ "status": "failed", "message": error.to_string() }),
    }
}

fn providers_activate_response(state_root: &std::path::Path, payload: &Value) -> Value {
    let id = payload
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim();
    activate_provider_response(state_root, id)
}

fn providers_reorder_response(state_root: &std::path::Path, payload: &Value) -> Value {
    match reorder_providers(state_root, payload) {
        Ok(store) => provider_store_response(store),
        Err(error) => json!({ "status": "failed", "message": error.to_string() }),
    }
}

pub(crate) fn activate_provider_response(state_root: &std::path::Path, id: &str) -> Value {
    project_named_provider(state_root, id, None)
}

fn project_named_provider(
    state_root: &std::path::Path,
    id: &str,
    saved_id: Option<String>,
) -> Value {
    let proxy_url = global_provider_proxy()
        .base_url()
        .map_err(|error| error.to_string())
        .unwrap_or_default();
    match activate_provider(state_root, id, &proxy_url, &default_codex_home()) {
        Ok((store, refresh)) => {
            let mut response = provider_store_response(store);
            if let Some(saved_id) = saved_id {
                response["savedId"] = json!(saved_id);
            }
            attach_refresh(response, refresh)
        }
        Err(error) => json!({ "status": "failed", "message": error.to_string() }),
    }
}

fn attach_refresh(mut response: Value, refresh: LiveRefresh) -> Value {
    response["refresh"] = json!(refresh.as_str());
    response
}

fn parse_oauth_kind(payload: &Value) -> Result<OAuthKind, String> {
    OAuthKind::parse(
        payload
            .get("kind")
            .or_else(|| payload.get("authMode"))
            .and_then(Value::as_str)
            .unwrap_or(""),
    )
    .map_err(|error| error.to_string())
}

async fn providers_oauth_start_response(state_root: &std::path::Path, payload: &Value) -> Value {
    let kind = match parse_oauth_kind(payload) {
        Ok(kind) => kind,
        Err(message) => return json!({ "status": "failed", "message": message }),
    };
    match start_oauth(state_root, kind).await {
        Ok(mut value) => {
            if let Some(uri) = value
                .get("verificationUri")
                .and_then(Value::as_str)
                .map(str::to_string)
            {
                match open_url(&uri, None::<&str>) {
                    Ok(_) => {
                        value["browserOpened"] = json!(true);
                    }
                    Err(error) => {
                        value["browserOpened"] = json!(false);
                        value["browserError"] = json!(error.to_string());
                    }
                }
            }
            value
        }
        Err(error) => json!({ "status": "failed", "message": error.to_string() }),
    }
}

async fn providers_oauth_poll_response(state_root: &std::path::Path, payload: &Value) -> Value {
    let kind = match parse_oauth_kind(payload) {
        Ok(kind) => kind,
        Err(message) => return json!({ "status": "failed", "message": message }),
    };
    let device_code = payload
        .get("deviceCode")
        .or_else(|| payload.get("device_code"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim();
    if device_code.is_empty() {
        return json!({ "status": "failed", "message": "OAuth device code is required" });
    }
    match poll_oauth(state_root, kind, device_code).await {
        Ok(value) => value,
        Err(error) => json!({ "status": "failed", "message": error.to_string() }),
    }
}

fn providers_oauth_status_response(state_root: &std::path::Path, payload: &Value) -> Value {
    match parse_oauth_kind(payload) {
        Ok(kind) => oauth_status(state_root, kind),
        Err(message) => json!({ "status": "failed", "message": message }),
    }
}

async fn providers_usage_response(state_root: &std::path::Path, payload: &Value) -> Value {
    let id = payload
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim();
    query_provider_usage(state_root, id).await
}

fn providers_usage_open_response(state_root: &std::path::Path, payload: &Value) -> Value {
    let explicit_url = payload
        .get("url")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim();
    let url = if !explicit_url.is_empty() {
        match validated_usage_page_url(explicit_url) {
            Ok(url) => url,
            Err(message) => return json!({ "status": "failed", "message": message }),
        }
    } else {
        let id = payload
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or("")
            .trim();
        if id.is_empty() {
            return json!({ "status": "failed", "message": "Provider id is required" });
        }
        let store = match read_store(state_root) {
            Ok(store) => store,
            Err(error) => return json!({ "status": "failed", "message": error.to_string() }),
        };
        match usage_page_url_for_store(&store, id) {
            Ok(url) => url,
            Err(message) => return json!({ "status": "failed", "message": message }),
        }
    };
    match open_url(&url, None::<&str>) {
        Ok(_) => json!({ "status": "ok", "url": url }),
        Err(error) => json!({ "status": "failed", "message": error.to_string() }),
    }
}

fn providers_secret_response(state_root: &std::path::Path, payload: &Value) -> Value {
    let id = payload
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim();
    match provider_api_key(state_root, id) {
        Ok(api_key) => json!({ "status": "ok", "apiKey": api_key }),
        Err(error) => json!({ "status": "failed", "message": error.to_string() }),
    }
}

fn log_provider_event(logger: &crate::logging::DiagnosticLogger, event: &str, response: &Value) {
    let status = response
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    let keep = status != "ok"
        || matches!(
            event,
            "providers.saved"
                | "providers.deleted"
                | "providers.activated"
                | "providers.reordered"
                | "endpoint.key_created"
                | "endpoint.key_deleted"
        );
    if !keep {
        return;
    }
    let mut detail = serde_json::Map::new();
    if let Some(status) = response.get("status") {
        detail.insert("status".to_string(), status.clone());
    }
    if let Some(id) = response
        .get("savedId")
        .or_else(|| response.get("activeId"))
        .or_else(|| response.get("id"))
    {
        detail.insert("id".to_string(), id.clone());
    }
    if let Some(message) = response.get("message") {
        detail.insert("message".to_string(), message.clone());
    }
    let _ = logger.append(event, Value::Object(detail));
}

async fn providers_models_response(state_root: &std::path::Path, payload: &Value) -> Value {
    match fetch_provider_models(state_root, payload).await {
        Ok(models) => json!({
            "status": "ok",
            "models": models,
            "message": if models.is_empty() {
                "No models returned".to_string()
            } else {
                format!("{} models", models.len())
            },
        }),
        Err(error) => json!({ "status": "failed", "message": error.to_string() }),
    }
}

async fn providers_test_response(state_root: &std::path::Path, payload: &Value) -> Value {
    match test_provider_connection(state_root, payload).await {
        Ok((status_code, preview)) => json!({
            "status": if (200..300).contains(&status_code) { "ok" } else { "failed" },
            "statusCode": status_code,
            "message": if preview.is_empty() {
                format!("HTTP {status_code}")
            } else {
                format!("HTTP {status_code}: {preview}")
            },
        }),
        Err(error) => json!({ "status": "failed", "message": error.to_string() }),
    }
}

fn reveal_path_response(path: &std::path::Path) -> Value {
    let result = if path.is_dir() {
        open_path(path, None::<&str>)
    } else {
        reveal_item_in_dir(path)
    };
    match result {
        Ok(_) => json!({ "status": "ok", "path": path.to_string_lossy() }),
        Err(error) => json!({ "status": "failed", "message": error.to_string() }),
    }
}

async fn open_devtools_response(debug_port: u16, target_id: &str) -> Value {
    let targets = match list_targets(debug_port).await {
        Ok(targets) => targets,
        Err(error) => return json!({ "status": "failed", "message": error.to_string() }),
    };
    let url = match devtools_url_for_target_id(debug_port, &targets, target_id) {
        Ok(url) => url,
        Err(error) => return json!({ "status": "failed", "message": error.to_string() }),
    };
    let target_id = target_id.trim();
    match open_devtools_url(&url) {
        Ok(_) => json!({ "status": "ok", "targetId": target_id, "url": url }),
        Err(error) => json!({ "status": "failed", "message": error }),
    }
}

fn open_devtools_url(url: &str) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        const CHROME_APP: &str = "/Applications/Google Chrome.app";
        if !std::path::Path::new(CHROME_APP).exists() {
            return Err("Google Chrome is required to open DevTools".to_string());
        }
        return open_url(url, Some(CHROME_APP)).map_err(|error| error.to_string());
    }
    #[cfg(not(target_os = "macos"))]
    open_url(url, None::<&str>).map_err(|error| error.to_string())
}

fn open_external_local_url_response(payload: &Value) -> Value {
    let url = match local_browser_url_from_payload(payload) {
        Ok(url) => url,
        Err(message) => return json!({ "status": "failed", "message": message }),
    };
    match open_url(&url, None::<&str>) {
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
    let websocket_url = target
        .web_socket_debugger_url
        .as_deref()
        .map(str::trim)
        .filter(|url| !url.is_empty());
    if let Some(websocket_url) = websocket_url {
        let websocket_endpoint = websocket_url
            .strip_prefix("ws://")
            .ok_or_else(|| anyhow::anyhow!("Codex DevTools websocket URL must start with ws://"))?;
        return Ok(format!(
            "http://127.0.0.1:{debug_port}/devtools/inspector.html?ws={websocket_endpoint}"
        ));
    }

    let frontend_url = target
        .devtools_frontend_url
        .as_deref()
        .map(str::trim)
        .filter(|url| !url.is_empty())
        .ok_or_else(|| anyhow::anyhow!("Selected Codex DevTools target has no inspector URL"))?;
    Ok(normalize_devtools_frontend_url(debug_port, frontend_url))
}

fn devtools_url_for_target_id(
    debug_port: u16,
    targets: &[CdpTarget],
    target_id: &str,
) -> anyhow::Result<String> {
    let target_id = target_id.trim();
    if target_id.is_empty() {
        anyhow::bail!("Codex DevTools caller target id is empty");
    }
    let target = targets
        .iter()
        .find(|target| target.id == target_id)
        .ok_or_else(|| anyhow::anyhow!("Codex DevTools target not found: {target_id}"))?;
    devtools_url(debug_port, target)
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
    fn devtools_url_prefers_local_inspector_over_hosted_frontend() {
        let target = CdpTarget {
            id: "target-1".to_string(),
            target_type: "page".to_string(),
            title: Some("ChatGPT".to_string()),
            url: Some("app://-/index.html".to_string()),
            devtools_frontend_url: Some(
                "https://chrome-devtools-frontend.appspot.com/serve_rev/@abc/inspector.html?ws=127.0.0.1:9229/devtools/page/target-1".to_string(),
            ),
            web_socket_debugger_url: Some("ws://127.0.0.1:9229/devtools/page/target-1".to_string()),
        };

        assert_eq!(
            devtools_url(9229, &target).expect("devtools url"),
            "http://127.0.0.1:9229/devtools/inspector.html?ws=127.0.0.1:9229/devtools/page/target-1"
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
    fn devtools_url_reports_missing_inspector_endpoint() {
        let target = CdpTarget {
            id: "target-1".to_string(),
            target_type: "page".to_string(),
            title: Some("Codex".to_string()),
            url: Some("https://codex.test".to_string()),
            devtools_frontend_url: None,
            web_socket_debugger_url: None,
        };

        let error = devtools_url(9229, &target).expect_err("missing inspector");
        assert!(
            error.to_string().contains("inspector URL"),
            "error should name the inspector URL, got {error}"
        );
        assert!(
            !error.to_string().contains("websocket URL"),
            "error should not blame a missing websocket when falling back to inspector URL"
        );
    }

    #[test]
    fn devtools_open_uses_caller_target() {
        let targets = vec![
            CdpTarget {
                id: "first-target".to_string(),
                target_type: "page".to_string(),
                title: Some("Codex".to_string()),
                url: Some("app://-/index.html".to_string()),
                devtools_frontend_url: None,
                web_socket_debugger_url: Some(
                    "ws://127.0.0.1:9229/devtools/page/first-target".to_string(),
                ),
            },
            CdpTarget {
                id: "caller-target".to_string(),
                target_type: "page".to_string(),
                title: Some("Codex".to_string()),
                url: Some("app://-/index.html".to_string()),
                devtools_frontend_url: None,
                web_socket_debugger_url: Some(
                    "ws://127.0.0.1:9229/devtools/page/caller-target".to_string(),
                ),
            },
        ];

        assert_eq!(
            devtools_url_for_target_id(9229, &targets, "caller-target").expect("devtools url"),
            "http://127.0.0.1:9229/devtools/inspector.html?ws=127.0.0.1:9229/devtools/page/caller-target"
        );
    }

    #[test]
    fn settings_only_action_allows_settings_window() {
        let settings = BridgeCaller {
            target_id: SETTINGS_WINDOW_TARGET_ID.to_string(),
            helper_instance_id: SETTINGS_WINDOW_TARGET_ID.to_string(),
            href: "helper://settings".to_string(),
            has_focus: true,
            visibility_state: "visible".to_string(),
        };
        assert!(settings_only_action(&settings).is_none());
        let renderer = BridgeCaller {
            target_id: "page-1".to_string(),
            helper_instance_id: "helper-1".to_string(),
            href: "app://-/index.html".to_string(),
            has_focus: true,
            visibility_state: "visible".to_string(),
        };
        let denied = settings_only_action(&renderer).expect("denied");
        assert_eq!(denied["status"], "failed");
        assert!(denied["message"]
            .as_str()
            .unwrap()
            .contains("Helper Settings"));
    }

    #[test]
    fn runtime_activity_records_caller_identity() {
        let activity = RuntimeActivity::default();
        let caller = BridgeCaller {
            target_id: "target-1".to_string(),
            helper_instance_id: "helper-1".to_string(),
            href: "app://-/index.html".to_string(),
            has_focus: true,
            visibility_state: "visible".to_string(),
        };

        activity.record(&caller);

        assert_eq!(
            activity.last(),
            Some(RuntimeActivitySnapshot {
                target_id: "target-1".to_string(),
                helper_instance_id: "helper-1".to_string(),
                href: "app://-/index.html".to_string(),
                has_focus: true,
                visibility_state: "visible".to_string(),
            })
        );
    }

    #[test]
    fn bridge_request_diagnostic_includes_route_status_and_caller() {
        let caller = BridgeCaller {
            target_id: "target-1".to_string(),
            helper_instance_id: "helper-1".to_string(),
            href: "app://-/index.html".to_string(),
            has_focus: true,
            visibility_state: "visible".to_string(),
        };

        let diagnostic =
            bridge_request_diagnostic("/backend/status", &caller, &json!({ "status": "ok" }));

        assert_eq!(diagnostic["path"], "/backend/status");
        assert_eq!(diagnostic["status"], "ok");
        assert_eq!(diagnostic["caller"]["targetId"], "target-1");
        assert_eq!(diagnostic["caller"]["helperInstanceId"], "helper-1");
        assert_eq!(diagnostic["caller"]["href"], "app://-/index.html");
        assert_eq!(diagnostic["caller"]["hasFocus"], true);
        assert_eq!(diagnostic["caller"]["visibilityState"], "visible");
        assert!(diagnostic.get("message").is_none());

        let failed = bridge_request_diagnostic(
            "/providers/save",
            &caller,
            &json!({ "status": "failed", "message": "Provider base URL is required" }),
        );
        assert_eq!(failed["status"], "failed");
        assert_eq!(failed["message"], "Provider base URL is required");
    }

    #[test]
    fn bridge_request_logging_suppresses_noisy_success_routes() {
        assert!(!should_log_bridge_request("/ports/list", "ok"));
        assert!(!should_log_bridge_request("/runtime/activity", "ok"));
        assert!(should_log_bridge_request("/ports/list", "failed"));
        assert!(!should_log_bridge_request("/backend/status", "ok"));
        assert!(should_log_bridge_request("/backend/status", "failed"));
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
