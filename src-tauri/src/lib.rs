mod app;
mod backup;
mod bridge;
mod cdp;
mod launcher;
mod logging;
mod markdown;
mod models;
mod routes;
mod runtime;
mod session_actions;
mod settings;
mod state_dir;
mod storage;
mod zed;

pub fn run() {
    app::run();
}
