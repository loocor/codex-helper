use std::path::PathBuf;

use serde_json::{json, Value};

use crate::backup::BackupStore;
use crate::markdown::MarkdownExportService;
use crate::models::SessionRef;
use crate::state_dir::StateDir;
use crate::storage::SQLiteStorageAdapter;

pub fn delete_session_response(state_dir: &StateDir, payload: &Value) -> Value {
    match session_from_payload(payload) {
        Ok(session) => match storage_adapter(state_dir) {
            Ok(adapter) => {
                serde_json::to_value(adapter.delete_local(&session)).unwrap_or_else(failed_value)
            }
            Err(error) => failed_session_value(&session.session_id, error.to_string()),
        },
        Err(error) => failed_session_value("", error.to_string()),
    }
}

pub fn undo_delete_response(state_dir: &StateDir, payload: &Value) -> Value {
    let undo_token = payload
        .get("undo_token")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if undo_token.is_empty() {
        return failed_session_value("", "undo_token cannot be empty");
    }
    match storage_adapter(state_dir) {
        Ok(adapter) => serde_json::to_value(adapter.undo(undo_token)).unwrap_or_else(failed_value),
        Err(error) => failed_session_value("", error.to_string()),
    }
}

pub fn deleted_sessions_response(state_dir: &StateDir) -> Value {
    match BackupStore::new(state_dir.backups_dir.clone()).list_deleted_sessions() {
        Ok(backups) => json!({ "status": "ok", "backups": backups }),
        Err(error) => json!({ "status": "failed", "message": error.to_string() }),
    }
}

pub fn restore_deleted_session_response(state_dir: &StateDir, payload: &Value) -> Value {
    let undo_token = payload
        .get("undo_token")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let mut result = undo_delete_response(state_dir, payload);
    if result.get("status").and_then(Value::as_str) == Some("undone") {
        if let Err(error) =
            BackupStore::new(state_dir.backups_dir.clone()).remove_backup(undo_token)
        {
            if let Some(message) = result.get_mut("message") {
                *message = json!(format!(
                    "{}; failed to remove backup: {error}",
                    message
                        .as_str()
                        .unwrap_or("Local session restored from backup")
                ));
            }
        }
    }
    result
}

pub fn export_markdown_response(payload: &Value) -> Value {
    match session_from_payload(payload) {
        Ok(session) => match default_codex_db_path() {
            Ok(db_path) => {
                serde_json::to_value(MarkdownExportService::new(Some(db_path)).export(&session))
                    .unwrap_or_else(failed_value)
            }
            Err(error) => failed_export_value(&session.session_id, error.to_string()),
        },
        Err(error) => failed_export_value("", error.to_string()),
    }
}

pub fn move_thread_workspace_response(state_dir: &StateDir, payload: &Value) -> Value {
    let session = match session_from_payload(payload) {
        Ok(session) => session,
        Err(error) => return json!({ "status": "failed", "message": error.to_string() }),
    };
    let target_cwd = payload
        .get("target_cwd")
        .and_then(Value::as_str)
        .unwrap_or_default();
    match storage_adapter(state_dir) {
        Ok(adapter) => adapter.move_codex_thread_workspace(&session, target_cwd),
        Err(error) => {
            json!({ "status": "failed", "session_id": session.session_id, "message": error.to_string() })
        }
    }
}

fn storage_adapter(state_dir: &StateDir) -> anyhow::Result<SQLiteStorageAdapter> {
    Ok(SQLiteStorageAdapter::new(
        default_codex_db_path()?,
        BackupStore::new(state_dir.backups_dir.clone()),
    ))
}

fn session_from_payload(payload: &Value) -> anyhow::Result<SessionRef> {
    let session_id = payload
        .get("session_id")
        .or_else(|| payload.get("sessionId"))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim();
    let title = payload
        .get("title")
        .and_then(Value::as_str)
        .unwrap_or("Untitled session");
    SessionRef::new(session_id, title)
}

fn default_codex_db_path() -> anyhow::Result<PathBuf> {
    let home = dirs::home_dir().ok_or_else(|| anyhow::anyhow!("Home directory not found"))?;
    Ok(home.join(".codex").join("state_5.sqlite"))
}

fn failed_value(error: serde_json::Error) -> Value {
    failed_session_value("", error.to_string())
}

fn failed_session_value(session_id: &str, message: impl Into<String>) -> Value {
    json!({
        "status": "failed",
        "session_id": session_id,
        "message": message.into(),
        "undo_token": null,
        "backup_path": null,
    })
}

fn failed_export_value(session_id: &str, message: impl Into<String>) -> Value {
    json!({
        "status": "failed",
        "session_id": session_id,
        "message": message.into(),
        "filename": null,
        "markdown": null,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_from_payload_accepts_session_id() {
        let session = session_from_payload(&json!({
            "session_id": "local:thread-1",
            "title": "Thread"
        }))
        .expect("session");

        assert_eq!(session.session_id, "local:thread-1");
        assert_eq!(session.title, "Thread");
    }

    #[test]
    fn session_from_payload_rejects_missing_id() {
        let error = session_from_payload(&json!({ "title": "Thread" })).expect_err("missing id");

        assert!(error.to_string().contains("session_id"));
    }
}
