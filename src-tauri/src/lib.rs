mod app;
mod bridge;
mod cdp;
mod codex_app_server;
mod codex_control;
mod debug_port;
mod launcher;
mod logging;
mod markdown;
mod models;
mod ports;
mod routes;
mod runtime;
pub mod session_actions;
mod settings;
pub mod state_dir;
pub mod zed;

pub fn run() {
    app::run();
}
