use std::sync::Arc;

use crate::codex_control::CodexController;
use crate::ports::PortForwardManager;

pub fn run() {
    let port_manager = PortForwardManager::new();
    let controller = CodexController::new();
    let startup_controller = controller.clone();
    let startup_port_manager = port_manager.clone();
    let shutdown_port_manager = port_manager.clone();
    tauri::Builder::default()
        .setup(move |app| {
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);
            install_menu_bar_item(app.handle(), controller.clone(), port_manager.clone())?;
            tauri::async_runtime::spawn(async move {
                if let Err(error) = startup_controller
                    .initial_launch(startup_port_manager)
                    .await
                {
                    eprintln!("{error}");
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

fn install_menu_bar_item(
    app: &tauri::AppHandle,
    controller: Arc<CodexController>,
    port_manager: PortForwardManager,
) -> anyhow::Result<()> {
    use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
    use tauri::tray::TrayIconBuilder;

    let quit_codex = MenuItem::with_id(app, "quit-codex", "Quit Codex", true, None::<&str>)?;
    let reload_codex = MenuItem::with_id(app, "reload-codex", "Reload Codex", true, None::<&str>)?;
    let restart_codex =
        MenuItem::with_id(app, "restart-codex", "Restart Codex", true, None::<&str>)?;
    let separator = PredefinedMenuItem::separator(app)?;
    let quit_helper =
        MenuItem::with_id(app, "quit-helper", "Quit Codex Helper", true, None::<&str>)?;
    let menu = Menu::with_items(
        app,
        &[
            &quit_codex,
            &reload_codex,
            &restart_codex,
            &separator,
            &quit_helper,
        ],
    )?;
    let mut tray = TrayIconBuilder::with_id("codex-helper")
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
        "quit-codex" => {
            let controller = controller.clone();
            tauri::async_runtime::spawn(async move {
                if let Err(error) = controller.quit_codex().await {
                    eprintln!("{error}");
                }
            });
        }
        "reload-codex" => {
            let controller = controller.clone();
            tauri::async_runtime::spawn(async move {
                if let Err(error) = controller.reload_codex().await {
                    eprintln!("{error}");
                }
            });
        }
        "restart-codex" => {
            let controller = controller.clone();
            tauri::async_runtime::spawn(async move {
                if let Err(error) = controller.restart_codex().await {
                    eprintln!("{error}");
                }
            });
        }
        "quit-helper" => {
            port_manager.stop_all();
            app.exit(0);
        }
        _ => {}
    })
    .build(app)?;
    Ok(())
}
