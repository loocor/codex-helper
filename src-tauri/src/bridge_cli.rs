use codex_helper::codex_live::default_codex_home;
use codex_helper::provider_oauth::{oauth_status, poll_oauth, start_oauth, OAuthKind};
use codex_helper::provider_proxy::{
    fetch_provider_models, global_provider_proxy, test_provider_connection,
};
use codex_helper::providers::{
    activate_provider, delete_provider, list_response, read_store, upsert_provider, LiveRefresh,
};
use codex_helper::state_dir::StateDir;
use serde_json::json;
use std::env;

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.get(1).map(String::as_str) == Some("--provider-proxy") {
        run_provider_proxy();
    }
    if args.len() < 2 {
        eprintln!("usage: codex-helper-bridge <path> [json-payload]");
        eprintln!("       codex-helper-bridge --provider-proxy");
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
        "/providers/list"
        | "/providers/save"
        | "/providers/delete"
        | "/providers/activate"
        | "/providers/test"
        | "/providers/models"
        | "/providers/oauth/start"
        | "/providers/oauth/poll"
        | "/providers/oauth/status" => match StateDir::init() {
            Ok(state_dir) => provider_bridge(path, &payload, &state_dir),
            Err(error) => json!({ "status": "failed", "message": error.to_string() }),
        },
        _ => json!({
            "status": "failed",
            "message": format!("Unknown Codex Helper bridge path: {path}")
        }),
    };
    println!("{}", response);
}

fn provider_bridge(
    path: &str,
    payload: &serde_json::Value,
    state_dir: &StateDir,
) -> serde_json::Value {
    match path {
        "/providers/list" => match read_store(&state_dir.root) {
            Ok(store) => provider_store_response(store),
            Err(error) => json!({ "status": "failed", "message": error.to_string() }),
        },
        "/providers/save" => match upsert_provider(&state_dir.root, payload) {
            Ok((store, saved_id)) => {
                if store.active_id == saved_id {
                    return project_named_provider(
                        &state_dir.root,
                        &saved_id.clone(),
                        Some(saved_id),
                    );
                }
                let mut response = provider_store_response(store);
                response["savedId"] = json!(saved_id);
                response
            }
            Err(error) => json!({ "status": "failed", "message": error.to_string() }),
        },
        "/providers/delete" => match delete_provider(
            &state_dir.root,
            payload
                .get("id")
                .and_then(|value| value.as_str())
                .unwrap_or(""),
            &default_codex_home(),
        ) {
            Ok(store) => provider_store_response(store),
            Err(error) => json!({ "status": "failed", "message": error.to_string() }),
        },
        "/providers/models" => {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build();
            match runtime {
                Err(error) => json!({ "status": "failed", "message": error.to_string() }),
                Ok(runtime) => {
                    match runtime.block_on(fetch_provider_models(&state_dir.root, payload)) {
                        Ok(models) => json!({
                            "status": "ok",
                            "models": models,
                            "message": format!("{} models", models.len()),
                        }),
                        Err(error) => json!({ "status": "failed", "message": error.to_string() }),
                    }
                }
            }
        }
        "/providers/test" => {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build();
            match runtime {
                Err(error) => json!({ "status": "failed", "message": error.to_string() }),
                Ok(runtime) => {
                    match runtime.block_on(test_provider_connection(&state_dir.root, payload)) {
                        Ok((status_code, preview)) => json!({
                            "status": if (200..300).contains(&status_code) { "ok" } else { "failed" },
                            "statusCode": status_code,
                            "message": if preview.is_empty() {
                                format!("HTTP {status_code}")
                            } else {
                                format!("HTTP {status_code}: {preview}")
                            },
                        }),
                        Err(error) => json!({ "status": "failed", "message": error.to_string() }),
                    }
                }
            }
        }
        "/providers/activate" => project_named_provider(
            &state_dir.root,
            payload
                .get("id")
                .and_then(|value| value.as_str())
                .unwrap_or(""),
            None,
        ),
        "/providers/oauth/start" | "/providers/oauth/poll" | "/providers/oauth/status" => {
            oauth_bridge(path, payload, state_dir)
        }
        _ => json!({
            "status": "failed",
            "message": format!("Unknown Codex Helper bridge path: {path}")
        }),
    }
}

fn project_named_provider(
    state_root: &std::path::Path,
    id: &str,
    saved_id: Option<String>,
) -> serde_json::Value {
    let proxy_url = global_provider_proxy()
        .base_url()
        .unwrap_or_else(|_| String::new());
    match activate_provider(state_root, id, &proxy_url, &default_codex_home()) {
        Ok((store, refresh)) => {
            let mut response = provider_store_response(store);
            if let Some(saved_id) = saved_id {
                response["savedId"] = json!(saved_id);
            }
            attach_refresh(response, refresh)
        }
        Err(error) => json!({ "status": "failed", "message": error.to_string() }),
    }
}

fn attach_refresh(mut response: serde_json::Value, refresh: LiveRefresh) -> serde_json::Value {
    response["refresh"] = json!(refresh.as_str());
    response
}

fn provider_store_response(store: codex_helper::providers::ProviderStore) -> serde_json::Value {
    let proxy = global_provider_proxy();
    proxy.set_store(store.clone());
    match proxy.base_url() {
        Ok(url) => list_response(&store, &url),
        Err(error) => {
            let mut response = list_response(&store, "");
            response["proxyError"] = json!(error.to_string());
            response
        }
    }
}

fn run_provider_proxy() -> ! {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap_or_else(|error| {
            eprintln!("Failed to start provider proxy runtime: {error}");
            std::process::exit(1);
        });
    runtime.block_on(async {
        let proxy = global_provider_proxy();
        let state_dir = match StateDir::init() {
            Ok(state_dir) => state_dir,
            Err(error) => {
                eprintln!("Failed to init Helper state dir: {error}");
                std::process::exit(1);
            }
        };
        proxy.set_state_root(state_dir.root.clone());
        match read_store(&state_dir.root) {
            Ok(store) => proxy.set_store(store),
            Err(error) => {
                eprintln!("Failed to read provider store: {error}");
                std::process::exit(1);
            }
        }
        if let Err(error) = proxy.bind_and_serve().await {
            eprintln!("{error}");
            std::process::exit(1);
        }
        std::future::pending::<()>().await;
    });
    unreachable!("provider proxy runtime ended");
}

fn oauth_bridge(
    path: &str,
    payload: &serde_json::Value,
    state_dir: &StateDir,
) -> serde_json::Value {
    let kind = match OAuthKind::parse(
        payload
            .get("kind")
            .or_else(|| payload.get("authMode"))
            .and_then(|value| value.as_str())
            .unwrap_or(""),
    ) {
        Ok(kind) => kind,
        Err(error) => return json!({ "status": "failed", "message": error.to_string() }),
    };
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build();
    match path {
        "/providers/oauth/status" => oauth_status(&state_dir.root, kind),
        "/providers/oauth/start" => match runtime {
            Err(error) => json!({ "status": "failed", "message": error.to_string() }),
            Ok(runtime) => match runtime.block_on(start_oauth(&state_dir.root, kind)) {
                Ok(mut value) => {
                    if let Some(uri) = value
                        .get("verificationUri")
                        .and_then(|value| value.as_str())
                        .map(str::to_string)
                    {
                        match codex_helper::provider_oauth::open_verification_uri(&uri) {
                            Ok(()) => value["browserOpened"] = json!(true),
                            Err(error) => {
                                value["browserOpened"] = json!(false);
                                value["browserError"] = json!(error.to_string());
                            }
                        }
                    }
                    value
                }
                Err(error) => json!({ "status": "failed", "message": error.to_string() }),
            },
        },
        "/providers/oauth/poll" => {
            let device_code = payload
                .get("deviceCode")
                .or_else(|| payload.get("device_code"))
                .and_then(|value| value.as_str())
                .unwrap_or("")
                .trim();
            if device_code.is_empty() {
                return json!({ "status": "failed", "message": "OAuth device code is required" });
            }
            match runtime {
                Err(error) => json!({ "status": "failed", "message": error.to_string() }),
                Ok(runtime) => {
                    match runtime.block_on(poll_oauth(&state_dir.root, kind, device_code)) {
                        Ok(value) => value,
                        Err(error) => json!({ "status": "failed", "message": error.to_string() }),
                    }
                }
            }
        }
        _ => json!({
            "status": "failed",
            "message": format!("Unknown Codex Helper bridge path: {path}")
        }),
    }
}
