use std::sync::Arc;

use crate::bridge::install_bridge;
use crate::cdp::wait_for_codex_target;
use crate::launcher::{launch_codex, resolve_codex_app_path, DEFAULT_DEBUG_PORT};
use crate::logging::DiagnosticLogger;
use crate::routes::BridgeContext;
use crate::runtime::build_runtime_bundle;
use crate::state_dir::StateDir;

pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);
            install_menu_bar_item(app.handle())?;
            tauri::async_runtime::spawn(async {
                if let Err(error) = launch_on_startup().await {
                    eprintln!("{error}");
                }
            });
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("failed to build CodexHelper")
        .run(|_app, _event| {});
}

fn install_menu_bar_item(app: &tauri::AppHandle) -> anyhow::Result<()> {
    use tauri::menu::{Menu, MenuItem};
    use tauri::tray::TrayIconBuilder;

    let quit = MenuItem::with_id(app, "quit", "Quit Codex Helper", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&quit])?;
    TrayIconBuilder::with_id("codex-helper")
        .icon(tauri::include_image!("icons/tray-icon.png"))
        .tooltip("Codex Helper is running")
        .menu(&menu)
        .show_menu_on_left_click(true)
        .on_menu_event(|app, event| {
            if event.id().as_ref() == "quit" {
                app.exit(0);
            }
        })
        .build(app)?;
    Ok(())
}

async fn launch_on_startup() -> anyhow::Result<()> {
    let state_dir = StateDir::init()?;
    let logger = DiagnosticLogger::new(state_dir.logs_dir.clone());
    logger.append(
        "launcher.starting",
        serde_json::json!({ "debugPort": DEFAULT_DEBUG_PORT }),
    )?;
    let app_path = resolve_codex_app_path(None)?;
    logger.append(
        "launcher.codex_app_resolved",
        serde_json::json!({ "appPath": app_path }),
    )?;
    let _codex = launch_codex(&app_path, DEFAULT_DEBUG_PORT).await?;
    logger.append(
        "launcher.codex_started",
        serde_json::json!({ "debugPort": DEFAULT_DEBUG_PORT }),
    )?;
    let target = wait_for_codex_target(DEFAULT_DEBUG_PORT).await?;
    let websocket_url = target
        .web_socket_debugger_url
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("Selected Codex CDP target has no websocket URL"))?;
    logger.append(
        "cdp.target_selected",
        serde_json::json!({
            "id": target.id,
            "title": target.title,
            "url": target.url,
        }),
    )?;
    let runtime_scripts = build_runtime_bundle(&state_dir, &logger)?;
    let ctx = BridgeContext {
        state_dir,
        logger: Arc::new(logger.clone()),
        debug_port: DEFAULT_DEBUG_PORT,
    };
    install_bridge(websocket_url, ctx, runtime_scripts).await?;
    logger.append("bridge.injected", serde_json::json!({}))?;
    Ok(())
}
