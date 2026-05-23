use codex_helper::session_actions::{
    delete_session_response, deleted_sessions_response, export_markdown_response,
    move_thread_workspace_response, restore_deleted_session_response, undo_delete_response,
};
use codex_helper::state_dir::StateDir;
use serde_json::json;
use std::env;

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("usage: codex-helper-bridge <path> [json-payload]");
        std::process::exit(1);
    }
    let path = args[1].as_str();
    let payload = if args.len() > 2 {
        serde_json::from_str(&args[2]).unwrap_or_else(|error| {
            eprintln!("{error}");
            std::process::exit(1);
        })
    } else {
        json!({})
    };
    let state_dir = match StateDir::init() {
        Ok(state_dir) => state_dir,
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    };
    let response = match path {
        "/delete" => delete_session_response(&state_dir, &payload),
        "/undo" => undo_delete_response(&state_dir, &payload),
        "/backups/list" => deleted_sessions_response(&state_dir),
        "/backups/restore" => restore_deleted_session_response(&state_dir, &payload),
        "/export-markdown" => export_markdown_response(&payload),
        "/move-thread-workspace" => move_thread_workspace_response(&state_dir, &payload),
        _ => json!({
            "status": "failed",
            "message": format!("Unknown Codex Helper bridge path: {path}")
        }),
    };
    println!("{}", response);
}
