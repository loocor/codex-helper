use std::sync::Arc;

use crate::runtime::build_settings_runtime;
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder, WindowEvent};

pub const SETTINGS_WINDOW_LABEL: &str = "settings";
pub const SETTINGS_WINDOW_TARGET_ID: &str = "settings-window";

pub type OpenSettings = Arc<dyn Fn(&str) -> Result<(), String> + Send + Sync>;

pub fn settings_page_id(page: &str) -> &'static str {
    match page.trim() {
        "providers" => "providers",
        "endpoint" => "endpoint",
        "logs" => "logs",
        "about" => "about",
        _ => "general",
    }
}

pub fn open_settings_callback(app: AppHandle) -> OpenSettings {
    Arc::new(move |page: &str| request_show_settings_window(&app, page))
}

pub fn request_show_settings_window(app: &AppHandle, page: &str) -> Result<(), String> {
    let app = app.clone();
    let page = settings_page_id(page).to_string();
    app.clone()
        .run_on_main_thread(move || {
            if let Err(error) = show_settings_window(&app, &page) {
                eprintln!("failed to open Helper Settings: {error}");
            }
        })
        .map_err(|error| error.to_string())
}

fn show_settings_window(app: &AppHandle, page: &str) -> anyhow::Result<()> {
    let page = settings_page_id(page);
    #[cfg(target_os = "macos")]
    {
        let _ = app.set_activation_policy(tauri::ActivationPolicy::Regular);
    }
    if let Some(window) = app.get_webview_window(SETTINGS_WINDOW_LABEL) {
        window.unminimize()?;
        window.show()?;
        window.set_focus()?;
        let script = format!(
            "window.__codexHelperOpenSettingsPage({});",
            serde_json::to_string(page)?
        );
        window.eval(&script)?;
        return Ok(());
    }
    create_settings_window(app, page)
}

fn create_settings_window(app: &AppHandle, page: &str) -> anyhow::Result<()> {
    let page = settings_page_id(page);
    let pending_page = serde_json::to_string(page)?;
    let mut builder = WebviewWindowBuilder::new(
        app,
        SETTINGS_WINDOW_LABEL,
        WebviewUrl::App("settings.html".into()),
    )
    .title("Codex Helper Settings")
    .inner_size(960.0, 640.0)
    .min_inner_size(760.0, 480.0)
    .resizable(true)
    .visible(true)
    .initialization_script(settings_bridge_script(&pending_page))
    .initialization_script(build_settings_runtime())
    .theme(None);
    #[cfg(target_os = "macos")]
    {
        builder = builder
            .hidden_title(true)
            .title_bar_style(tauri::TitleBarStyle::Overlay)
            .traffic_light_position(tauri::LogicalPosition::new(16.0, 18.0));
    }
    let window = builder.build()?;
    let hidden = window.clone();
    let app_handle = window.app_handle().clone();
    window.on_window_event(move |event| {
        if let WindowEvent::CloseRequested { api, .. } = event {
            api.prevent_close();
            let _ = hidden.hide();
            #[cfg(target_os = "macos")]
            {
                let _ = app_handle.set_activation_policy(tauri::ActivationPolicy::Accessory);
            }
        }
    });
    window.set_focus()?;
    Ok(())
}

fn settings_bridge_script(pending_page_json: &str) -> String {
    format!(
        r#"
(() => {{
  const invoke = (command, args) => {{
    const internals = window.__TAURI_INTERNALS__;
    if (internals && typeof internals.invoke === "function") {{
      return internals.invoke(command, args);
    }}
    const core = window.__TAURI__ && window.__TAURI__.core;
    if (core && typeof core.invoke === "function") {{
      return core.invoke(command, args);
    }}
    return Promise.resolve({{
      status: "failed",
      message: "Codex Helper bridge is not installed",
    }});
  }};
  window.__codexHelperPendingSettingsPage = {pending_page_json};
  window.__codexHelperBridge = (path, payload = {{}}) =>
    invoke("helper_bridge", {{ path, payload: payload || {{}} }});
  window.__codexHelperOpenSettingsPage = (pageId) => {{
    window.__codexHelperPendingSettingsPage = pageId || "general";
    if (typeof window.__codexHelperShowSettingsPage === "function") {{
      window.__codexHelperShowSettingsPage(window.__codexHelperPendingSettingsPage);
    }}
  }};
}})();
"#
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_page_id_accepts_known_pages() {
        assert_eq!(settings_page_id("general"), "general");
        assert_eq!(settings_page_id("providers"), "providers");
        assert_eq!(settings_page_id("endpoint"), "endpoint");
        assert_eq!(settings_page_id("logs"), "logs");
        assert_eq!(settings_page_id("about"), "about");
    }

    #[test]
    fn settings_page_id_defaults_unknown_pages_to_general() {
        assert_eq!(settings_page_id(""), "general");
        assert_eq!(settings_page_id("missing"), "general");
        assert_eq!(settings_page_id(" user-scripts "), "general");
    }

    #[test]
    fn settings_window_uses_macos_main_title_bar() {
        let source = include_str!("settings_window.rs");
        assert!(source.contains("TitleBarStyle::Overlay"));
        assert!(source.contains("hidden_title(true)"));
        assert!(source.contains("traffic_light_position"));
    }

    #[test]
    fn settings_window_follows_system_theme() {
        let source = include_str!("settings_window.rs");
        assert!(source.contains(".theme(None)"));
        let html = include_str!("../assets/settings.html");
        assert!(html.contains(r#"name="color-scheme""#));
        assert!(html.contains("light dark"));
    }
}
