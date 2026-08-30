use serde_json::{json, Value};
use tauri::{AppHandle, Runtime};
use tauri_plugin_updater::UpdaterExt;

pub async fn check_for_update<R: Runtime>(app: &AppHandle<R>) -> Value {
    match check_for_update_result(app).await {
        Ok(payload) => payload,
        Err(error) => json!({
            "status": "failed",
            "message": error.to_string(),
        }),
    }
}

pub async fn install_update<R: Runtime>(app: &AppHandle<R>) -> Value {
    match install_update_result(app).await {
        Ok(()) => {
            let handle = app.clone();
            tauri::async_runtime::spawn(async move {
                tokio::time::sleep(std::time::Duration::from_millis(300)).await;
                handle.restart();
            });
            json!({
                "status": "ok",
                "restarting": true,
            })
        }
        Err(error) => json!({
            "status": "failed",
            "message": error.to_string(),
        }),
    }
}

async fn check_for_update_result<R: Runtime>(app: &AppHandle<R>) -> anyhow::Result<Value> {
    let current_version = app.package_info().version.to_string();
    let updater = app.updater()?;
    match updater.check().await? {
        Some(update) => Ok(checked_update_payload(
            &current_version,
            Some(CheckedUpdate {
                version: update.version,
                notes: update.body.unwrap_or_default(),
                pub_date: update.date.map(|date| date.to_string()),
            }),
        )),
        None => Ok(checked_update_payload(&current_version, None)),
    }
}

async fn install_update_result<R: Runtime>(app: &AppHandle<R>) -> anyhow::Result<()> {
    let updater = app.updater()?;
    let update = updater
        .check()
        .await?
        .ok_or_else(|| anyhow::anyhow!("No update is available"))?;
    update
        .download_and_install(|_, _| {}, || {})
        .await
        .map_err(|error| anyhow::anyhow!("{error}"))?;
    Ok(())
}

struct CheckedUpdate {
    version: String,
    notes: String,
    pub_date: Option<String>,
}

fn checked_update_payload(current_version: &str, update: Option<CheckedUpdate>) -> Value {
    match update {
        Some(update) => json!({
            "status": "ok",
            "available": true,
            "currentVersion": current_version,
            "latestVersion": update.version,
            "notes": update.notes,
            "pubDate": update.pub_date,
        }),
        None => json!({
            "status": "ok",
            "available": false,
            "currentVersion": current_version,
            "latestVersion": current_version,
            "notes": "",
            "pubDate": Value::Null,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checked_update_payload_includes_latest_release_notes() {
        let payload = checked_update_payload(
            "0.1.0",
            Some(CheckedUpdate {
                version: "0.2.2".to_string(),
                notes: "## What's Changed\n* fix updater".to_string(),
                pub_date: Some("2026-08-30T00:00:00Z".to_string()),
            }),
        );
        assert_eq!(payload["status"], "ok");
        assert_eq!(payload["available"], true);
        assert_eq!(payload["currentVersion"], "0.1.0");
        assert_eq!(payload["latestVersion"], "0.2.2");
        assert_eq!(payload["notes"], "## What's Changed\n* fix updater");
        assert_eq!(payload["pubDate"], "2026-08-30T00:00:00Z");
    }

    #[test]
    fn checked_update_payload_marks_current_version_up_to_date() {
        let payload = checked_update_payload("0.2.2", None);
        assert_eq!(payload["status"], "ok");
        assert_eq!(payload["available"], false);
        assert_eq!(payload["currentVersion"], "0.2.2");
        assert_eq!(payload["latestVersion"], "0.2.2");
        assert_eq!(payload["notes"], "");
        assert_eq!(payload["pubDate"], Value::Null);
    }

    #[test]
    fn updater_endpoint_uses_github_release_asset_not_rest_api() {
        let conf = include_str!("../tauri.conf.json");
        assert!(conf.contains(
            "https://github.com/loocor/codex-helper/releases/latest/download/latest.json"
        ));
        assert!(!conf.contains("api.github.com"));
    }
}
