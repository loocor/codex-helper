mod app;
mod bridge;
mod cdp;
mod codex_control;
pub mod codex_live;
mod compat_custom;
mod debug_port;
mod deepseek_sanitize;
mod endpoint;
mod launch_at_login;
mod launcher;
mod llm_compat_inventory;
mod llm_traffic_log;
mod logging;
pub mod model_catalog;
mod ports;
pub mod provider_oauth;
pub mod provider_proxy;
mod provider_usage;
pub mod providers;
mod proxy_env;
mod routes;
mod runtime;
mod settings;
mod settings_window;
pub mod state_dir;
mod updater;
mod xai_sanitize;
pub mod zed;

pub fn run() {
    app::run();
}
