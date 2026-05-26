use codex_helper::session_actions::{
    auto_rename_chat_response, export_markdown_response, fork_thread_project_response,
};
use codex_helper::zed::remote_projects_response;
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
    let response = match path {
        "/auto-rename-chat" => auto_rename_chat_response(&payload),
        "/export-markdown" => export_markdown_response(&payload),
        "/fork-thread-project" => fork_thread_project_response(&payload),
        "/projects/remote-list" => remote_projects_response(&payload),
        _ => json!({
            "status": "failed",
            "message": format!("Unknown Codex Helper bridge path: {path}")
        }),
    };
    println!("{}", response);
}
