use std::sync::Arc;

use crate::bridge::{BridgeCaller, BridgeRequest};
use crate::codex_control::CodexController;
use crate::logging::DiagnosticLogger;
use crate::ports::PortForwardManager;
use crate::provider_proxy::global_provider_proxy;
use crate::providers::{providers_in_display_order, read_store, Provider, ProviderStore};
use crate::proxy_env::configure_process_loopback_no_proxy;
use crate::routes::{activate_provider_response, handle_bridge_request, BridgeContext};
use crate::settings_window::{
    open_settings_callback, request_show_settings_window, SETTINGS_WINDOW_TARGET_ID,
};
use crate::state_dir::StateDir;
use serde_json::{json, Value};
use tauri::Manager;
use tauri_plugin_dialog::{
    DialogExt, MessageDialogButtons, MessageDialogKind, MessageDialogResult,
};

struct HelperState {
    state_dir: StateDir,
    logger: Arc<DiagnosticLogger>,
    port_manager: PortForwardManager,
    controller: Arc<CodexController>,
}

const TRAY_ICON_ID: &str = "codex-helper";
const TRAY_PROVIDERS_SUBMENU_ID: &str = "providers";
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

fn tray_provider_display_name(provider: &Provider) -> &str {
    let name = provider.name.trim();
    if name.is_empty() {
        provider.id.as_str()
    } else {
        name
    }
}

fn tray_provider_label(provider: &Provider) -> String {
    tray_provider_display_name(provider).replace('&', "&&")
}

fn tray_provider_item_specs(store: &ProviderStore) -> Vec<TrayProviderItemSpec> {
    providers_in_display_order(store)
        .into_iter()
        .map(|provider| TrayProviderItemSpec {
            id: tray_activate_provider_id(&provider.id),
            label: tray_provider_label(provider),
            checked: provider.id == store.active_id,
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
    if matches!(
        path.as_str(),
        "/providers/save" | "/providers/delete" | "/providers/activate" | "/providers/reorder"
    ) && result.get("status").and_then(Value::as_str) == Some("ok")
    {
        if let Err(error) = rebuild_tray_menu(&app) {
            eprintln!("failed to rebuild tray menu: {error}");
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
        .invoke_handler(tauri::generate_handler![helper_bridge])
        .setup(move |app| {
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);
            let handle = app.handle().clone();
            controller.bind_app(handle.clone());
            let state_dir = StateDir::init()?;
            let logger = Arc::new(DiagnosticLogger::new(state_dir.logs_dir.clone()));
            handle.manage(HelperState {
                state_dir: state_dir.clone(),
                logger,
                port_manager: port_manager.clone(),
                controller: controller.clone(),
            });
            if let Err(error) = sync_launch_at_login(&state_dir) {
                eprintln!("failed to sync launch at login: {error}");
            }
            install_menu_bar_item(app.handle(), controller.clone(), port_manager.clone())?;
            let startup_app = app.handle().clone();
            let proxy = global_provider_proxy();
            proxy.set_state_root(state_dir.root.clone());
            if let Ok(store) = read_store(&state_dir.root) {
                proxy.set_store(store);
            }
            if let Err(error) = tauri::async_runtime::block_on(proxy.bind_and_serve()) {
                eprintln!("provider proxy failed: {error}");
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
    use tauri::menu::{CheckMenuItem, IsMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu};

    let specs = tray_menu_item_specs();
    let open_settings = MenuItem::with_id(app, specs[0].id, specs[0].label, true, None::<&str>)?;
    let restart_chatgpt = MenuItem::with_id(app, specs[1].id, specs[1].label, true, None::<&str>)?;
    let separator = PredefinedMenuItem::separator(app)?;
    let quit_helper = MenuItem::with_id(app, specs[2].id, specs[2].label, true, None::<&str>)?;
    let store = provider_store_for_tray(app)?;
    let provider_items = tray_provider_item_specs(&store)
        .into_iter()
        .map(|item| {
            CheckMenuItem::with_id(app, item.id, item.label, true, item.checked, None::<&str>)
        })
        .collect::<Result<Vec<_>, _>>()?;
    let provider_refs: Vec<&dyn IsMenuItem<_>> = provider_items
        .iter()
        .map(|item| item as &dyn IsMenuItem<_>)
        .collect();
    let providers_menu = Submenu::with_id_and_items(
        app,
        TRAY_PROVIDERS_SUBMENU_ID,
        "Providers",
        true,
        &provider_refs,
    )?;
    Ok(Menu::with_items(
        app,
        &[
            &open_settings,
            &providers_menu,
            &restart_chatgpt,
            &separator,
            &quit_helper,
        ],
    )?)
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
    let response = activate_provider_response(&state.state_dir.root, provider_id);
    if response.get("status").and_then(Value::as_str) != Some("ok") {
        let message = response
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or("Failed to switch provider");
        anyhow::bail!("{message}");
    }
    Ok(Some(provider_switch_message(
        "Activated",
        &name,
        response.get("refresh").and_then(Value::as_str),
    )))
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
    fn tray_provider_menu_lists_configured_providers_with_active_checkmark() {
        let mut store = ProviderStore::default();
        store.providers.push(Provider {
            id: "grok".to_string(),
            name: "Grok".to_string(),
            ..Provider::default()
        });
        store.active_id = "grok".to_string();

        let items = tray_provider_item_specs(&store);

        assert_eq!(items.len(), 2);
        assert_eq!(items[0].id, "activate-provider:official");
        assert_eq!(items[0].label, "Official");
        assert!(!items[0].checked);
        assert_eq!(items[1].id, "activate-provider:grok");
        assert_eq!(items[1].label, "Grok");
        assert!(items[1].checked);
    }

    #[test]
    fn tray_provider_menu_keeps_official_first() {
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

        assert_eq!(items[0].id, "activate-provider:official");
        assert_eq!(items[0].label, "Official");
        assert_eq!(items[1].id, "activate-provider:grok");
        assert_eq!(items[1].label, "Grok");
    }

    #[test]
    fn tray_provider_label_escapes_menu_mnemonics() {
        let provider = Provider {
            id: "foo".to_string(),
            name: "Foo & Bar".to_string(),
            ..Provider::default()
        };

        assert_eq!(tray_provider_display_name(&provider), "Foo & Bar");
        assert_eq!(tray_provider_label(&provider), "Foo && Bar");
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
