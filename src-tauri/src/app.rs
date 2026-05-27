use std::sync::Arc;

use crate::codex_control::CodexController;
use crate::ports::PortForwardManager;
use crate::proxy_env::configure_process_loopback_no_proxy;
use tauri_plugin_dialog::{
    DialogExt, MessageDialogButtons, MessageDialogKind, MessageDialogResult,
};

struct TrayMenuItemSpec {
    id: &'static str,
    label: &'static str,
}

fn tray_menu_item_specs() -> [TrayMenuItemSpec; 1] {
    [TrayMenuItemSpec {
        id: "quit-helper",
        label: "Quit Codex Helper",
    }]
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HelperQuitChoice {
    Quit,
    Cancel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StartupRecoveryChoice {
    CleanUpAndStart,
    QuitHelper,
}

const QUIT_HELPER_LABEL: &str = "Quit";
const CANCEL_QUIT_LABEL: &str = "Cancel";
const CLEAN_UP_AND_START_LABEL: &str = "Clean Up and Start Codex";
const QUIT_HELPER_STARTUP_LABEL: &str = "Quit Helper";

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
        .setup(move |app| {
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);
            install_menu_bar_item(app.handle(), controller.clone(), port_manager.clone())?;
            let startup_app = app.handle().clone();
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

fn install_menu_bar_item(
    app: &tauri::AppHandle,
    controller: Arc<CodexController>,
    port_manager: PortForwardManager,
) -> anyhow::Result<()> {
    use tauri::menu::{Menu, MenuItem};
    use tauri::tray::TrayIconBuilder;

    let specs = tray_menu_item_specs();
    let quit_helper = MenuItem::with_id(app, specs[0].id, specs[0].label, true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&quit_helper])?;
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
        _ => {}
    })
    .build(app)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tray_menu_only_exposes_helper_quit() {
        let items = tray_menu_item_specs();

        assert_eq!(items.len(), 1);
        assert_eq!(items[0].id, "quit-helper");
        assert_eq!(items[0].label, "Quit Codex Helper");
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
