use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use crate::bridge::{BridgeCaller, BridgeRequest};
use crate::codex_control::CodexController;
use crate::logging::DiagnosticLogger;
use crate::ports::PortForwardManager;
use crate::provider_proxy::global_provider_proxy;
use crate::providers::{providers_in_display_order, read_store, Provider, ProviderStore};
use crate::proxy_env::configure_process_loopback_no_proxy;
use crate::routes::{activate_provider_exclusive_response, handle_bridge_request, BridgeContext};
use crate::settings_window::{
    open_settings_callback, request_show_settings_window, SETTINGS_WINDOW_TARGET_ID,
};
use crate::state_dir::StateDir;
use crate::sync::{replica_poll_interval, replica_tick, ReplicaWatch};
use serde_json::{json, Value};
use tauri::Manager;
use tauri_plugin_dialog::{
    DialogExt, MessageDialogButtons, MessageDialogKind, MessageDialogResult,
};
use tokio::time::sleep;

struct HelperState {
    state_dir: StateDir,
    logger: Arc<DiagnosticLogger>,
    port_manager: PortForwardManager,
    controller: Arc<CodexController>,
}

const TRAY_ICON_ID: &str = "codex-helper";
const TRAY_FAILOVER_ID: &str = "toggle-provider-failover";
const TRAY_ACTIVATE_PROVIDER_PREFIX: &str = "activate-provider:";

struct TrayMenuItemSpec {
    id: &'static str,
    label: &'static str,
}

struct TrayProviderItemSpec {
    id: String,
    label: String,
    checked: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TrayMenuSection {
    Settings,
    Providers,
    Separator,
    Failover,
    Restart,
    Quit,
}

/// Top to bottom: Settings, optional provider group, Automatic fallback,
/// Restart ChatGPT, then Quit alone. Separators sit between those groups.
fn tray_menu_sections(has_providers: bool) -> Vec<TrayMenuSection> {
    let mut sections = vec![TrayMenuSection::Settings];
    if has_providers {
        sections.push(TrayMenuSection::Separator);
        sections.push(TrayMenuSection::Providers);
    }
    sections.extend([
        TrayMenuSection::Separator,
        TrayMenuSection::Failover,
        TrayMenuSection::Restart,
        TrayMenuSection::Separator,
        TrayMenuSection::Quit,
    ]);
    sections
}

fn tray_menu_item_specs() -> [TrayMenuItemSpec; 3] {
    [
        TrayMenuItemSpec {
            id: "open-settings",
            label: "Settings…",
        },
        TrayMenuItemSpec {
            id: "restart-chatgpt",
            label: "Restart ChatGPT",
        },
        TrayMenuItemSpec {
            id: "quit-helper",
            label: "Quit Codex Helper",
        },
    ]
}

fn tray_activate_provider_id(provider_id: &str) -> String {
    format!("{TRAY_ACTIVATE_PROVIDER_PREFIX}{provider_id}")
}

fn parse_tray_activate_provider_id(event_id: &str) -> Option<&str> {
    event_id
        .strip_prefix(TRAY_ACTIVATE_PROVIDER_PREFIX)
        .filter(|provider_id| !provider_id.is_empty())
}

fn provider_failover_enabled_for_tray(app: &tauri::AppHandle) -> bool {
    app.try_state::<HelperState>()
        .map(|state| crate::settings::provider_failover_enabled(&state.state_dir.root))
        .unwrap_or(true)
}

fn tray_provider_display_name(provider: &Provider) -> &str {
    let name = provider.name.trim();
    if name.is_empty() {
        provider.id.as_str()
    } else {
        name
    }
}

fn escape_menu_mnemonic(text: &str) -> String {
    text.replace('&', "&&")
}

struct TrayUsageWindow {
    label: String,
    percent: String,
}

fn tray_usage_text(snapshot: &crate::provider_usage::UsageSnapshot) -> Option<String> {
    let summary = snapshot.summary.trim();
    if !summary.is_empty() {
        if let Some(text) = compact_tray_usage(summary) {
            if !text.is_empty() && !looks_like_usage_meter(&text) {
                return Some(text);
            }
        }
    }
    // `used_percent` stays consumed so quota exhaustion still means 100% used.
    // The tray shows what is left, matching a remaining balance.
    snapshot
        .used_percent
        .filter(|value| value.is_finite())
        .map(|used| format_tray_percent(remaining_percent(used)))
}

fn compact_tray_usage(summary: &str) -> Option<String> {
    let mut windows = Vec::new();
    let mut balances = Vec::new();
    let mut unlimited = false;
    for part in summary.split('·') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        if let Some(window) = parse_tray_window(part) {
            windows.push(window);
            continue;
        }
        if let Some(amount) = parse_tray_balance(part) {
            balances.push(amount);
            continue;
        }
        if part.to_ascii_lowercase().contains("unlimited") {
            unlimited = true;
        }
    }
    windows.sort_by_key(|window| tray_window_rank(&window.label));
    let mut pieces: Vec<String> = windows
        .into_iter()
        .map(|window| {
            if window.label.is_empty() {
                window.percent
            } else {
                format!("{} {}", window.label, window.percent)
            }
        })
        .collect();
    if pieces.is_empty() && unlimited {
        pieces.push("unlimited".to_string());
    }
    pieces.extend(balances);
    if !pieces.is_empty() {
        return Some(pieces.join(" / "));
    }
    let stripped = collapse_ws(&strip_usage_meter_bars(summary));
    if looks_like_usage_meter(summary) {
        if let Some(percent) = trailing_percent(&stripped) {
            return Some(invert_percent_text(&percent));
        }
    }
    if let Some(percent) = trailing_percent(&stripped) {
        return Some(percent);
    }
    if stripped.is_empty() || looks_like_usage_meter(&stripped) {
        return None;
    }
    Some(stripped.chars().take(42).collect())
}

fn parse_tray_window(part: &str) -> Option<TrayUsageWindow> {
    if part.to_ascii_lowercase().contains("% used") {
        return parse_used_window(part);
    }
    parse_remaining_window(part)
}

/// `5h 89%` and `1w 70%` are remaining. A bare `89%` is remaining too.
fn parse_remaining_window(part: &str) -> Option<TrayUsageWindow> {
    let (raw_label, value) = split_label_percent(part)?;
    if !is_tray_window_label(&raw_label) {
        return None;
    }
    Some(TrayUsageWindow {
        label: format_tray_window_label(&raw_label),
        percent: format_tray_percent(value),
    })
}

/// Cached summaries still say `80% used (5h)`. Show the remainder, not the spend.
fn parse_used_window(part: &str) -> Option<TrayUsageWindow> {
    let lower = part.to_ascii_lowercase();
    let index = lower.find("% used")?;
    let before = part[..index].trim();
    let number = before.split_whitespace().last()?;
    let value = number.parse::<f64>().ok()?;
    if !value.is_finite() {
        return None;
    }
    let label = part[index + "% used".len()..]
        .trim()
        .strip_prefix('(')
        .and_then(|rest| rest.split(')').next())
        .map(str::trim)
        .filter(|label| is_tray_window_label(label))
        .map(format_tray_window_label)
        .unwrap_or_default();
    Some(TrayUsageWindow {
        label,
        percent: format_tray_percent(remaining_percent(value)),
    })
}

fn split_label_percent(part: &str) -> Option<(String, f64)> {
    let trimmed = part.trim();
    if trimmed.is_empty() || trimmed.contains('(') {
        return None;
    }
    let tokens: Vec<&str> = trimmed.split_whitespace().collect();
    if tokens.is_empty() || tokens.len() > 2 {
        return None;
    }
    let number = tokens.last()?.trim_end_matches('%');
    if !tokens.last()?.ends_with('%') {
        return None;
    }
    let value = number.parse::<f64>().ok()?;
    if !value.is_finite() {
        return None;
    }
    let label = if tokens.len() == 1 {
        String::new()
    } else {
        tokens[0].to_string()
    };
    Some((label, value))
}

fn is_tray_window_label(label: &str) -> bool {
    let label = label.trim();
    if label.is_empty() {
        return true;
    }
    if label.len() > 8 || label.contains('/') || label.contains('%') {
        return false;
    }
    matches!(
        label.to_ascii_lowercase().as_str(),
        "5h" | "1w" | "1m" | "week" | "weekly" | "day" | "daily" | "month" | "monthly"
    )
}

fn format_tray_window_label(label: &str) -> String {
    match label.trim().to_ascii_lowercase().as_str() {
        "week" | "weekly" | "1w" => "1w".to_string(),
        "5h" => "5h".to_string(),
        "month" | "monthly" | "1m" => "1m".to_string(),
        "day" | "daily" => "Day".to_string(),
        _ => String::new(),
    }
}

fn tray_window_rank(label: &str) -> u8 {
    match label {
        "5h" => 0,
        "1w" => 1,
        "Day" => 2,
        "1m" => 3,
        "" => 5,
        _ => 4,
    }
}

fn remaining_percent(used: f64) -> f64 {
    (100.0 - used.clamp(0.0, 100.0)).clamp(0.0, 100.0)
}

fn parse_tray_balance(part: &str) -> Option<String> {
    let lower = part.to_ascii_lowercase();
    if !lower.contains("remaining") && !lower.contains("balance") {
        return None;
    }
    let head = part.split('(').next()?.trim();
    if let Some(amount) = find_symbol_amount(head) {
        return Some(amount);
    }
    let tokens: Vec<&str> = head.split_whitespace().collect();
    for pair in tokens.windows(2) {
        if let Some(symbol) = currency_symbol(pair[0]) {
            if let Some(amount) = parse_money_amount(pair[1]) {
                return Some(format!("{symbol}{amount}"));
            }
        }
    }
    for token in &tokens {
        if token.eq_ignore_ascii_case("remaining") || token.eq_ignore_ascii_case("balance") {
            break;
        }
        if let Some(amount) = parse_money_amount(token) {
            return Some(amount);
        }
    }
    None
}

fn find_symbol_amount(text: &str) -> Option<String> {
    let chars: Vec<char> = text.chars().collect();
    for (index, ch) in chars.iter().enumerate() {
        let symbol = match ch {
            '¥' | '￥' => "¥",
            '$' => "$",
            '€' => "€",
            '£' => "£",
            _ => continue,
        };
        let rest: String = chars[index + 1..].iter().collect();
        let token = rest.split_whitespace().next().unwrap_or("");
        if let Some(amount) = parse_money_amount(token) {
            return Some(format!("{symbol}{amount}"));
        }
    }
    None
}

fn currency_symbol(code: &str) -> Option<&'static str> {
    match code.trim().to_ascii_uppercase().as_str() {
        "CNY" | "RMB" => Some("¥"),
        "USD" => Some("$"),
        "EUR" => Some("€"),
        "GBP" => Some("£"),
        _ => None,
    }
}

fn parse_money_amount(token: &str) -> Option<String> {
    let cleaned: String = token
        .chars()
        .filter(|ch| ch.is_ascii_digit() || *ch == '.' || *ch == '-')
        .collect();
    if cleaned.is_empty() || cleaned == "-" || cleaned == "." {
        return None;
    }
    let value: f64 = cleaned.parse().ok()?;
    if !value.is_finite() {
        return None;
    }
    Some(format!("{value:.2}"))
}

fn invert_percent_text(text: &str) -> String {
    let number = text.trim().trim_end_matches('%');
    let value = number.parse::<f64>().unwrap_or(0.0);
    format_tray_percent(remaining_percent(value))
}

fn format_tray_percent(value: f64) -> String {
    let value = value.clamp(0.0, 100.0);
    if (value - value.round()).abs() < 0.05 {
        format!("{value:.0}%")
    } else {
        format!("{value:.1}%")
    }
}

fn trailing_percent(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    let mut last = None;
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index].is_ascii_digit() {
            let start = index;
            while index < bytes.len() && (bytes[index].is_ascii_digit() || bytes[index] == b'.') {
                index += 1;
            }
            if index < bytes.len() && bytes[index] == b'%' {
                if let Ok(value) = text[start..index].parse::<f64>() {
                    if value.is_finite() {
                        last = Some(format_tray_percent(value));
                    }
                }
                index += 1;
                continue;
            }
            continue;
        }
        index += 1;
    }
    last
}

fn strip_usage_meter_bars(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::new();
    let mut index = 0;
    while index < chars.len() {
        if chars[index] == '[' {
            if let Some(end) = chars[index + 1..].iter().position(|ch| *ch == ']') {
                let inner: String = chars[index + 1..index + 1 + end].iter().collect();
                if !inner.is_empty() && inner.chars().all(|ch| ch == '#' || ch == '-' || ch == '=')
                {
                    index += end + 2;
                    continue;
                }
            }
        }
        out.push(chars[index]);
        index += 1;
    }
    out
}

fn collapse_ws(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn looks_like_usage_meter(text: &str) -> bool {
    text.contains("----") || text.contains("####") || text.contains("[#")
}

fn tray_provider_label(
    provider: &Provider,
    usage: Option<&crate::provider_usage::UsageSnapshot>,
) -> String {
    let name = escape_menu_mnemonic(tray_provider_display_name(provider));
    let Some(text) = usage.and_then(tray_usage_text) else {
        return name;
    };
    if text.is_empty() {
        return name;
    }
    format!("{name} - {}", escape_menu_mnemonic(&text))
}

fn tray_provider_is_selected(store: &ProviderStore, provider_id: &str) -> bool {
    // Same membership as Settings "Include in the model list". Empty legacy
    // stores mean the active provider. Official stays out of this menu.
    crate::providers::effective_selected_ids(store)
        .iter()
        .any(|id| id == provider_id)
}

fn tray_provider_item_specs(store: &ProviderStore) -> Vec<TrayProviderItemSpec> {
    providers_in_display_order(store)
        .into_iter()
        .filter(|provider| provider.id != "official")
        .map(|provider| TrayProviderItemSpec {
            id: tray_activate_provider_id(&provider.id),
            label: tray_provider_label(
                provider,
                crate::provider_usage::usage_snapshot(&provider.id).as_ref(),
            ),
            checked: tray_provider_is_selected(store, &provider.id),
        })
        .collect()
}

fn provider_switch_message(verb: &str, name: &str, refresh: Option<&str>) -> String {
    let label = if name.is_empty() { "provider" } else { name };
    match refresh {
        Some("restart_desktop") => format!(
            "{verb} {label}. Helper is already using it. Restart ChatGPT desktop so login and the model picker refresh."
        ),
        Some("new_conversation") => format!(
            "{verb} {label}. Helper is already using it. Start a new ChatGPT conversation to pick it up."
        ),
        _ => format!("{verb} {label}."),
    }
}

#[tauri::command]
async fn helper_bridge(
    app: tauri::AppHandle,
    state: tauri::State<'_, HelperState>,
    path: String,
    payload: Option<Value>,
) -> Result<Value, String> {
    let payload = payload.unwrap_or_else(|| json!({}));
    if path == "/update/check" {
        return Ok(crate::updater::check_for_update(&app).await);
    }
    if path == "/update/install" {
        return Ok(crate::updater::install_update(&app).await);
    }
    if path == "/settings/set" {
        if let Some(enabled) = payload.get("launchAtLoginEnabled").and_then(Value::as_bool) {
            if let Err(error) = crate::launch_at_login::apply_launch_at_login(enabled) {
                return Ok(json!({
                    "status": "failed",
                    "message": error.to_string(),
                }));
            }
        }
    }
    let debug_port = state.controller.debug_port().await.unwrap_or(0);
    let ctx = BridgeContext {
        state_dir: state.state_dir.clone(),
        logger: state.logger.clone(),
        debug_port,
        port_manager: state.port_manager.clone(),
        runtime_activity: state.controller.runtime_activity(),
        open_settings: Some(open_settings_callback(app.clone())),
    };
    let request = BridgeRequest {
        id: SETTINGS_WINDOW_TARGET_ID.to_string(),
        path: path.clone(),
        payload,
        caller: BridgeCaller {
            target_id: SETTINGS_WINDOW_TARGET_ID.to_string(),
            helper_instance_id: SETTINGS_WINDOW_TARGET_ID.to_string(),
            href: "helper://settings".to_string(),
            has_focus: true,
            visibility_state: "visible".to_string(),
        },
    };
    let result = handle_bridge_request(ctx, request).await;
    if result.get("status").and_then(Value::as_str) == Some("ok") {
        if matches!(
            path.as_str(),
            "/providers/save"
                | "/providers/delete"
                | "/providers/activate"
                | "/providers/select"
                | "/providers/reorder"
                | "/settings/set"
        ) {
            if let Err(error) = rebuild_tray_menu(&app) {
                eprintln!("failed to rebuild tray menu: {error}");
            }
        } else if path == "/providers/usage" {
            schedule_tray_menu_rebuild(app.clone());
        }
    }
    Ok(result)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HelperQuitChoice {
    Quit,
    Cancel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RestartChatgptChoice {
    Restart,
    Cancel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StartupRecoveryChoice {
    CleanUpAndStart,
    QuitHelper,
}

const QUIT_HELPER_LABEL: &str = "Quit";
const CANCEL_QUIT_LABEL: &str = "Cancel";
const RESTART_CHATGPT_LABEL: &str = "Restart";
const CANCEL_RESTART_LABEL: &str = "Cancel";
const CLEAN_UP_AND_START_LABEL: &str = "Clean Up and Start Codex";
const QUIT_HELPER_STARTUP_LABEL: &str = "Quit Helper";

fn restart_chatgpt_choice_from_dialog_result(result: MessageDialogResult) -> RestartChatgptChoice {
    match result {
        MessageDialogResult::Ok => RestartChatgptChoice::Restart,
        MessageDialogResult::Custom(label) if label == RESTART_CHATGPT_LABEL => {
            RestartChatgptChoice::Restart
        }
        _ => RestartChatgptChoice::Cancel,
    }
}

fn helper_quit_choice_from_dialog_result(result: MessageDialogResult) -> HelperQuitChoice {
    match result {
        MessageDialogResult::Ok => HelperQuitChoice::Quit,
        MessageDialogResult::Custom(label) if label == QUIT_HELPER_LABEL => HelperQuitChoice::Quit,
        _ => HelperQuitChoice::Cancel,
    }
}

fn startup_recovery_choice_from_dialog_result(
    result: MessageDialogResult,
) -> StartupRecoveryChoice {
    match result {
        MessageDialogResult::Ok => StartupRecoveryChoice::CleanUpAndStart,
        MessageDialogResult::Custom(label) if label == CLEAN_UP_AND_START_LABEL => {
            StartupRecoveryChoice::CleanUpAndStart
        }
        _ => StartupRecoveryChoice::QuitHelper,
    }
}

fn should_confirm_helper_quit(has_connected_codex: bool) -> bool {
    has_connected_codex
}

fn show_restart_chatgpt_confirmation<F>(app: &tauri::AppHandle, on_choice: F)
where
    F: FnOnce(RestartChatgptChoice) + Send + 'static,
{
    app.dialog()
        .message(
            "ChatGPT will quit if it is running, then open again so Helper can reattach. Unsaved work in ChatGPT may be lost. Codex Helper will keep running.",
        )
        .title("Restart ChatGPT?")
        .kind(MessageDialogKind::Warning)
        .buttons(MessageDialogButtons::OkCancelCustom(
            RESTART_CHATGPT_LABEL.to_string(),
            CANCEL_RESTART_LABEL.to_string(),
        ))
        .show_with_result(move |result| {
            on_choice(restart_chatgpt_choice_from_dialog_result(result))
        });
}

fn show_restart_chatgpt_failed(app: &tauri::AppHandle, error: &str) {
    app.dialog()
        .message(error)
        .title("Restart ChatGPT failed")
        .kind(MessageDialogKind::Error)
        .show(|_| {});
}

fn show_helper_quit_confirmation<F>(app: &tauri::AppHandle, on_choice: F)
where
    F: FnOnce(HelperQuitChoice) + Send + 'static,
{
    app.dialog()
        .message(
            "Quitting Codex Helper will stop Helper features. Codex windows will keep running.",
        )
        .title("Quit Codex Helper?")
        .kind(MessageDialogKind::Warning)
        .buttons(MessageDialogButtons::OkCancelCustom(
            QUIT_HELPER_LABEL.to_string(),
            CANCEL_QUIT_LABEL.to_string(),
        ))
        .show_with_result(move |result| on_choice(helper_quit_choice_from_dialog_result(result)));
}

fn show_startup_recovery_confirmation<F>(app: &tauri::AppHandle, on_choice: F)
where
    F: FnOnce(StartupRecoveryChoice) + Send + 'static,
{
    app.dialog()
        .message("Codex Helper found an existing Codex debugging environment but could not attach to it. It can close Codex debugging instances and start a clean Codex session.")
        .title("Start Codex Helper?")
        .kind(MessageDialogKind::Warning)
        .buttons(MessageDialogButtons::OkCancelCustom(
            CLEAN_UP_AND_START_LABEL.to_string(),
            QUIT_HELPER_STARTUP_LABEL.to_string(),
        ))
        .show_with_result(move |result| {
            on_choice(startup_recovery_choice_from_dialog_result(result))
        });
}

pub fn run() {
    configure_process_loopback_no_proxy();
    let port_manager = PortForwardManager::new();
    let controller = CodexController::new();
    let startup_controller = controller.clone();
    let startup_port_manager = port_manager.clone();
    let shutdown_port_manager = port_manager.clone();
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .invoke_handler(tauri::generate_handler![helper_bridge])
        .setup(move |app| {
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);
            let handle = app.handle().clone();
            controller.bind_app(handle.clone());
            let state_dir = StateDir::init()?;
            let logger = Arc::new(DiagnosticLogger::new(state_dir.logs_dir.clone()));
            handle.manage(HelperState {
                state_dir: state_dir.clone(),
                logger: logger.clone(),
                port_manager: port_manager.clone(),
                controller: controller.clone(),
            });
            if let Err(error) = sync_launch_at_login(&state_dir) {
                eprintln!("failed to sync launch at login: {error}");
            }
            install_menu_bar_item(app.handle(), controller.clone(), port_manager.clone())?;
            spawn_usage_refresh_loop(app.handle().clone(), state_dir.root.clone());
            let startup_app = app.handle().clone();
            let proxy = global_provider_proxy();
            proxy.set_state_root(state_dir.root.clone());
            proxy.set_logger(logger);
            match crate::settings::read_settings(&state_dir.config_path) {
                Ok(settings) => proxy.set_log_llm_traffic(settings.log_llm_traffic_enabled),
                Err(error) => eprintln!("failed to load LLM log setting: {error}"),
            }
            if let Ok(store) = read_store(&state_dir.root) {
                proxy.set_store(store);
            }
            if let Err(error) = tauri::async_runtime::block_on(proxy.bind_and_serve()) {
                eprintln!("provider proxy failed: {error}");
            }
            {
                let root = state_dir.root.clone();
                let controller = controller.clone();
                let port_manager = port_manager.clone();
                let proxy_url = proxy.base_url().unwrap_or_default();
                tauri::async_runtime::spawn(async move {
                    let mut watch = ReplicaWatch::default();
                    loop {
                        sleep(replica_poll_interval()).await;
                        match replica_tick(&root, &mut watch, &proxy_url) {
                            Ok(Some(change)) if change.restart_desktop => {
                                if controller.has_connected_codex_instance().await {
                                    if let Err(error) =
                                        controller.restart_chatgpt(port_manager.clone()).await
                                    {
                                        eprintln!(
                                            "failed to restart ChatGPT after replica sync: {error}"
                                        );
                                    }
                                }
                            }
                            Ok(_) => {}
                            Err(error) => {
                                eprintln!("replica sync apply failed: {error}");
                            }
                        }
                    }
                });
            }
            tauri::async_runtime::spawn(async move {
                if let Err(error) = startup_controller
                    .initial_launch(startup_port_manager.clone())
                    .await
                {
                    eprintln!("{error}");
                    let controller = startup_controller.clone();
                    let port_manager = startup_port_manager.clone();
                    let app = startup_app.clone();
                    show_startup_recovery_confirmation(&startup_app, move |choice| {
                        tauri::async_runtime::spawn(async move {
                            match choice {
                                StartupRecoveryChoice::CleanUpAndStart => {
                                    if let Err(error) =
                                        controller.recover_codex_launch(port_manager.clone()).await
                                    {
                                        eprintln!("{error}");
                                        port_manager.stop_all();
                                        app.exit(1);
                                    }
                                }
                                StartupRecoveryChoice::QuitHelper => {
                                    port_manager.stop_all();
                                    app.exit(0);
                                }
                            }
                        });
                    });
                }
            });
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("failed to build CodexHelper")
        .run(move |_app, event| {
            if let tauri::RunEvent::Exit | tauri::RunEvent::ExitRequested { .. } = event {
                shutdown_port_manager.stop_all();
            }
        });
}

fn sync_launch_at_login(state_dir: &StateDir) -> anyhow::Result<()> {
    let settings = crate::settings::read_settings(&state_dir.config_path)?;
    crate::launch_at_login::apply_launch_at_login(settings.launch_at_login_enabled)
}

fn provider_store_for_tray(app: &tauri::AppHandle) -> anyhow::Result<ProviderStore> {
    let state = app
        .try_state::<HelperState>()
        .ok_or_else(|| anyhow::anyhow!("Helper state is unavailable"))?;
    read_store(&state.state_dir.root)
}

fn build_tray_menu(app: &tauri::AppHandle) -> anyhow::Result<tauri::menu::Menu<tauri::Wry>> {
    use tauri::menu::{CheckMenuItem, IsMenuItem, Menu, MenuItem, PredefinedMenuItem};

    let specs = tray_menu_item_specs();
    let open_settings = MenuItem::with_id(app, specs[0].id, specs[0].label, true, None::<&str>)?;
    let restart_chatgpt = MenuItem::with_id(app, specs[1].id, specs[1].label, true, None::<&str>)?;
    let quit_helper = MenuItem::with_id(app, specs[2].id, specs[2].label, true, None::<&str>)?;
    let failover_item = CheckMenuItem::with_id(
        app,
        TRAY_FAILOVER_ID,
        "Automatic fallback",
        true,
        provider_failover_enabled_for_tray(app),
        None::<&str>,
    )?;
    let store = provider_store_for_tray(app)?;
    let provider_items = tray_provider_item_specs(&store)
        .into_iter()
        .map(|item| {
            CheckMenuItem::with_id(app, item.id, item.label, true, item.checked, None::<&str>)
        })
        .collect::<Result<Vec<_>, _>>()?;
    let sections = tray_menu_sections(!provider_items.is_empty());
    let separators = sections
        .iter()
        .filter(|section| **section == TrayMenuSection::Separator)
        .map(|_| PredefinedMenuItem::separator(app))
        .collect::<Result<Vec<_>, _>>()?;
    let mut separators = separators.iter();
    let mut items: Vec<&dyn IsMenuItem<_>> = Vec::new();
    for section in &sections {
        match section {
            TrayMenuSection::Settings => items.push(&open_settings),
            TrayMenuSection::Providers => {
                for item in &provider_items {
                    items.push(item);
                }
            }
            TrayMenuSection::Separator => {
                items.push(
                    separators
                        .next()
                        .expect("tray separator count matches sections"),
                );
            }
            TrayMenuSection::Failover => items.push(&failover_item),
            TrayMenuSection::Restart => items.push(&restart_chatgpt),
            TrayMenuSection::Quit => items.push(&quit_helper),
        }
    }
    Ok(Menu::with_items(app, &items)?)
}

fn rebuild_tray_menu(app: &tauri::AppHandle) -> anyhow::Result<()> {
    let menu = build_tray_menu(app)?;
    let tray = app
        .tray_by_id(TRAY_ICON_ID)
        .ok_or_else(|| anyhow::anyhow!("Helper tray icon is unavailable"))?;
    tray.set_menu(Some(menu))?;
    Ok(())
}

fn activate_provider_from_tray(
    app: &tauri::AppHandle,
    provider_id: &str,
) -> anyhow::Result<Option<String>> {
    let state = app
        .try_state::<HelperState>()
        .ok_or_else(|| anyhow::anyhow!("Helper state is unavailable"))?;
    let store = read_store(&state.state_dir.root)?;
    if store.active_id == provider_id {
        return Ok(None);
    }
    let name = store
        .providers
        .iter()
        .find(|provider| provider.id == provider_id)
        .map(tray_provider_display_name)
        .unwrap_or(provider_id)
        .to_string();
    let response = activate_provider_exclusive_response(&state.state_dir.root, provider_id);
    if response.get("status").and_then(Value::as_str) != Some("ok") {
        let message = response
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or("Failed to switch provider");
        anyhow::bail!("{message}");
    }
    let mut message = provider_switch_message(
        "Activated",
        &name,
        response.get("refresh").and_then(Value::as_str),
    );
    if let Some(sync) = response.get("sync") {
        if sync.get("status").and_then(Value::as_str) == Some("ok") {
            message.push_str(" Synced to peers.");
        } else if let Some(detail) = sync.get("message").and_then(Value::as_str) {
            message.push_str(" Peer sync failed: ");
            message.push_str(detail);
        }
    }
    Ok(Some(message))
}

fn show_provider_switch_message(app: &tauri::AppHandle, message: &str, failed: bool) {
    let dialog = app.dialog().message(message);
    let dialog = if failed {
        dialog
            .title("Switch Provider failed")
            .kind(MessageDialogKind::Error)
    } else {
        dialog
            .title("Switch Provider")
            .kind(MessageDialogKind::Info)
    };
    dialog.show(|_| {});
}

fn toggle_provider_failover(app: &tauri::AppHandle) -> anyhow::Result<()> {
    let state = app
        .try_state::<HelperState>()
        .ok_or_else(|| anyhow::anyhow!("Helper state is unavailable"))?;
    let current = crate::settings::read_settings(&state.state_dir.config_path)?;
    crate::settings::update_settings(
        &state.state_dir.config_path,
        &json!({ "providerFailoverEnabled": !current.provider_failover_enabled }),
    )?;
    Ok(())
}

fn schedule_tray_menu_rebuild(app: tauri::AppHandle) {
    static PENDING: AtomicBool = AtomicBool::new(false);
    if PENDING.swap(true, Ordering::SeqCst) {
        return;
    }
    tauri::async_runtime::spawn(async move {
        sleep(Duration::from_millis(900)).await;
        PENDING.store(false, Ordering::SeqCst);
        let rebuild_app = app.clone();
        let _ = app.run_on_main_thread(move || {
            if let Err(error) = rebuild_tray_menu(&rebuild_app) {
                eprintln!("failed to rebuild tray menu: {error}");
            }
        });
    });
}

fn spawn_usage_refresh_loop(app: tauri::AppHandle, state_root: PathBuf) {
    tauri::async_runtime::spawn(async move {
        let mut first = true;
        loop {
            let delay = if first {
                first = false;
                Duration::from_secs(2)
            } else {
                Duration::from_secs(180)
            };
            sleep(delay).await;
            crate::provider_usage::refresh_usage_cache(&state_root).await;
            let rebuild_app = app.clone();
            let _ = app.run_on_main_thread(move || {
                if let Err(error) = rebuild_tray_menu(&rebuild_app) {
                    eprintln!("failed to rebuild tray menu after usage refresh: {error}");
                }
            });
        }
    });
}

fn install_menu_bar_item(
    app: &tauri::AppHandle,
    controller: Arc<CodexController>,
    port_manager: PortForwardManager,
) -> anyhow::Result<()> {
    use tauri::tray::TrayIconBuilder;

    let menu = build_tray_menu(app)?;
    let mut tray = TrayIconBuilder::with_id(TRAY_ICON_ID)
        .icon(tauri::include_image!("icons/tray-menu.png"))
        .tooltip("Codex Helper is running")
        .menu(&menu)
        .show_menu_on_left_click(true);
    #[cfg(target_os = "macos")]
    {
        // Template image: black + alpha only; macOS inverts for light/dark menu bar.
        tray = tray.icon_as_template(true);
    }
    tray.on_menu_event(move |app, event| match event.id().as_ref() {
        "toggle-provider-failover" => {
            if let Err(error) = toggle_provider_failover(app) {
                eprintln!("failed to toggle automatic fallback: {error}");
            }
            if let Err(error) = rebuild_tray_menu(app) {
                eprintln!("failed to rebuild tray menu: {error}");
            }
        }
        "open-settings" => {
            if let Err(error) = request_show_settings_window(app, "general") {
                eprintln!("failed to open Helper Settings: {error}");
            }
        }
        "restart-chatgpt" => {
            let controller = controller.clone();
            let port_manager = port_manager.clone();
            let app = app.clone();
            let confirmation_app = app.clone();
            show_restart_chatgpt_confirmation(&confirmation_app, move |choice| {
                if choice != RestartChatgptChoice::Restart {
                    return;
                }
                tauri::async_runtime::spawn(async move {
                    if let Err(error) = controller.restart_chatgpt(port_manager).await {
                        eprintln!("{error}");
                        let message = error.to_string();
                        let dialog_app = app.clone();
                        let _ = app.run_on_main_thread(move || {
                            show_restart_chatgpt_failed(&dialog_app, &message);
                        });
                    }
                });
            });
        }
        "quit-helper" => {
            let controller = controller.clone();
            let port_manager = port_manager.clone();
            let app = app.clone();
            tauri::async_runtime::spawn(async move {
                if !should_confirm_helper_quit(controller.has_connected_codex_instance().await) {
                    if let Err(error) = controller.prepare_helper_shutdown().await {
                        eprintln!("{error}");
                    }
                    port_manager.stop_all();
                    app.exit(0);
                    return;
                }
                let confirmation_app = app.clone();
                show_helper_quit_confirmation(&confirmation_app, move |choice| {
                    tauri::async_runtime::spawn(async move {
                        match choice {
                            HelperQuitChoice::Cancel => {}
                            HelperQuitChoice::Quit => {
                                if let Err(error) = controller.prepare_helper_shutdown().await {
                                    eprintln!("{error}");
                                }
                                port_manager.stop_all();
                                app.exit(0);
                            }
                        }
                    });
                });
            });
        }
        other => {
            if let Some(provider_id) = parse_tray_activate_provider_id(other) {
                let app = app.clone();
                let provider_id = provider_id.to_string();
                tauri::async_runtime::spawn(async move {
                    match activate_provider_from_tray(&app, &provider_id) {
                        Ok(None) => {}
                        Ok(Some(message)) => {
                            let dialog_app = app.clone();
                            let _ = app.run_on_main_thread(move || {
                                show_provider_switch_message(&dialog_app, &message, false);
                            });
                        }
                        Err(error) => {
                            let message = error.to_string();
                            let dialog_app = app.clone();
                            let _ = app.run_on_main_thread(move || {
                                show_provider_switch_message(&dialog_app, &message, true);
                            });
                        }
                    }
                    if let Err(error) = rebuild_tray_menu(&app) {
                        eprintln!("failed to rebuild tray menu: {error}");
                    }
                });
            }
        }
    })
    .build(app)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tray_menu_groups_providers_then_controls_then_quit() {
        assert_eq!(
            tray_menu_sections(true),
            vec![
                TrayMenuSection::Settings,
                TrayMenuSection::Separator,
                TrayMenuSection::Providers,
                TrayMenuSection::Separator,
                TrayMenuSection::Failover,
                TrayMenuSection::Restart,
                TrayMenuSection::Separator,
                TrayMenuSection::Quit,
            ]
        );
        assert_eq!(
            tray_menu_sections(false),
            vec![
                TrayMenuSection::Settings,
                TrayMenuSection::Separator,
                TrayMenuSection::Failover,
                TrayMenuSection::Restart,
                TrayMenuSection::Separator,
                TrayMenuSection::Quit,
            ]
        );
    }

    #[test]
    fn tray_menu_exposes_settings_restart_and_quit() {
        let items = tray_menu_item_specs();

        assert_eq!(items.len(), 3);
        assert_eq!(items[0].id, "open-settings");
        assert_eq!(items[0].label, "Settings…");
        assert_eq!(items[1].id, "restart-chatgpt");
        assert_eq!(items[1].label, "Restart ChatGPT");
        assert_eq!(items[2].id, "quit-helper");
        assert_eq!(items[2].label, "Quit Codex Helper");
    }

    #[test]
    fn tray_provider_menu_lists_configured_providers_by_name() {
        let mut store = ProviderStore::default();
        store.providers.push(Provider {
            id: "grok".to_string(),
            name: "Grok".to_string(),
            model: "grok-4".to_string(),
            ..Provider::default()
        });
        store.providers.push(Provider {
            id: "mimo".to_string(),
            name: "MiMo".to_string(),
            ..Provider::default()
        });
        store.active_id = "grok".to_string();
        store.selected_ids = vec!["official".to_string(), "grok".to_string()];

        let items = tray_provider_item_specs(&store);

        assert_eq!(items.len(), 2);
        assert!(items.iter().all(|item| item.id != "activate-provider:official"));
        assert_eq!(items[0].id, "activate-provider:grok");
        assert_eq!(items[0].label, "Grok");
        assert!(items[0].checked);
        assert!(!items[0].label.contains("grok-4"));
        assert_eq!(items[1].id, "activate-provider:mimo");
        assert_eq!(items[1].label, "MiMo");
        assert!(!items[1].checked);
    }

    #[test]
    fn tray_provider_menu_omits_official() {
        let mut store = ProviderStore::default();
        store.providers.insert(
            0,
            Provider {
                id: "grok".to_string(),
                name: "Grok".to_string(),
                ..Provider::default()
            },
        );

        let items = tray_provider_item_specs(&store);

        assert_eq!(items.len(), 1);
        assert_eq!(items[0].id, "activate-provider:grok");
        assert_eq!(items[0].label, "Grok");
        assert!(!items[0].checked);
    }

    #[test]
    fn tray_provider_checkmark_follows_model_list_membership() {
        let mut store = ProviderStore::default();
        store.providers.push(Provider {
            id: "grok".to_string(),
            name: "Grok".to_string(),
            ..Provider::default()
        });
        store.providers.push(Provider {
            id: "deepseek".to_string(),
            name: "DeepSeek".to_string(),
            ..Provider::default()
        });
        store.active_id = "grok".to_string();
        store.selected_ids = vec!["grok".to_string(), "deepseek".to_string()];

        let items = tray_provider_item_specs(&store);

        assert!(items.iter().all(|item| item.checked));

        store.selected_ids.clear();
        let legacy = tray_provider_item_specs(&store);
        assert!(legacy[0].checked);
        assert!(!legacy[1].checked);
    }

    #[test]
    fn tray_provider_label_uses_real_usage_without_model_or_meter() {
        let provider = Provider {
            id: "grok".to_string(),
            name: "Grok".to_string(),
            model: "grok-4".to_string(),
            ..Provider::default()
        };
        let windows = crate::provider_usage::UsageSnapshot {
            used_percent: Some(75.0),
            summary: "75% used (day) · resets in 4h · 15% used (week)".to_string(),
        };
        assert_eq!(
            tray_provider_label(&provider, Some(&windows)),
            "Grok - 1w 85% / Day 25%"
        );
        let balance = crate::provider_usage::UsageSnapshot {
            used_percent: None,
            summary: "CNY 13.08 remaining (granted 1.00)".to_string(),
        };
        let deepseek = Provider {
            id: "deepseek".to_string(),
            name: "DeepSeek".to_string(),
            model: "deepseek-v4".to_string(),
            ..Provider::default()
        };
        assert_eq!(
            tray_provider_label(&deepseek, Some(&balance)),
            "DeepSeek - ¥13.08"
        );
        assert!(!tray_provider_label(&provider, Some(&windows)).contains("grok-4"));
        assert!(!tray_provider_label(&provider, Some(&windows)).contains("----"));
        assert!(!tray_provider_label(&provider, Some(&windows)).contains('#'));
    }

    #[test]
    fn tray_usage_text_compacts_known_summaries() {
        let five_hour = crate::provider_usage::UsageSnapshot {
            used_percent: Some(80.0),
            summary: "80% used (5h) · 19,173/60,000 credits · 31% used (week)".to_string(),
        };
        assert_eq!(
            tray_usage_text(&five_hour).as_deref(),
            Some("5h 20% / 1w 69%")
        );
        let remaining = crate::provider_usage::UsageSnapshot {
            used_percent: Some(20.0),
            summary: "5h 80% · 1w 69%".to_string(),
        };
        assert_eq!(
            tray_usage_text(&remaining).as_deref(),
            Some("5h 80% / 1w 69%")
        );
        let meter = crate::provider_usage::UsageSnapshot {
            used_percent: Some(52.0),
            summary: "[#####-----] 52%".to_string(),
        };
        assert_eq!(tray_usage_text(&meter).as_deref(), Some("48%"));
        let missing = crate::provider_usage::UsageSnapshot {
            used_percent: None,
            summary: String::new(),
        };
        assert_eq!(tray_usage_text(&missing), None);
    }

    #[test]
    fn tray_provider_label_escapes_menu_mnemonics() {
        let provider = Provider {
            id: "foo".to_string(),
            name: "Foo & Bar".to_string(),
            ..Provider::default()
        };

        assert_eq!(tray_provider_display_name(&provider), "Foo & Bar");
        assert_eq!(tray_provider_label(&provider, None), "Foo && Bar");
    }

    #[test]
    fn tray_activate_provider_id_round_trips() {
        assert_eq!(
            parse_tray_activate_provider_id("activate-provider:official"),
            Some("official")
        );
        assert_eq!(parse_tray_activate_provider_id("open-settings"), None);
        assert_eq!(parse_tray_activate_provider_id("activate-provider:"), None);
    }

    #[test]
    fn provider_switch_message_matches_settings_copy() {
        assert_eq!(
            provider_switch_message("Activated", "Grok", Some("restart_desktop")),
            "Activated Grok. Helper is already using it. Restart ChatGPT desktop so login and the model picker refresh."
        );
        assert_eq!(
            provider_switch_message(
                "Activated",
                "Official",
                Some("new_conversation")
            ),
            "Activated Official. Helper is already using it. Start a new ChatGPT conversation to pick it up."
        );
    }

    #[test]
    fn restart_chatgpt_choice_maps_ok_to_restart() {
        assert_eq!(
            restart_chatgpt_choice_from_dialog_result(tauri_plugin_dialog::MessageDialogResult::Ok),
            RestartChatgptChoice::Restart
        );
    }

    #[test]
    fn restart_chatgpt_choice_maps_cancel_or_unknown_to_cancel() {
        assert_eq!(
            restart_chatgpt_choice_from_dialog_result(
                tauri_plugin_dialog::MessageDialogResult::Cancel
            ),
            RestartChatgptChoice::Cancel
        );
    }

    #[test]
    fn helper_quit_choice_maps_ok_to_quit() {
        assert_eq!(
            helper_quit_choice_from_dialog_result(tauri_plugin_dialog::MessageDialogResult::Ok),
            HelperQuitChoice::Quit
        );
    }

    #[test]
    fn helper_quit_choice_maps_cancel_or_unknown_to_cancel() {
        assert_eq!(
            helper_quit_choice_from_dialog_result(tauri_plugin_dialog::MessageDialogResult::Cancel),
            HelperQuitChoice::Cancel
        );
    }

    #[test]
    fn helper_quit_only_confirms_when_codex_is_connected() {
        assert!(should_confirm_helper_quit(true));
        assert!(!should_confirm_helper_quit(false));
    }

    #[test]
    fn startup_recovery_choice_maps_ok_to_cleanup() {
        assert_eq!(
            startup_recovery_choice_from_dialog_result(
                tauri_plugin_dialog::MessageDialogResult::Ok
            ),
            StartupRecoveryChoice::CleanUpAndStart
        );
    }

    #[test]
    fn startup_recovery_choice_maps_cancel_to_quit_helper() {
        assert_eq!(
            startup_recovery_choice_from_dialog_result(
                tauri_plugin_dialog::MessageDialogResult::Cancel
            ),
            StartupRecoveryChoice::QuitHelper
        );
    }
}
