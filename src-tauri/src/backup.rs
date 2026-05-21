use anyhow::Context;
use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::json;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DeletedSessionBackup {
    pub token: String,
    pub session_id: String,
    pub title: String,
    pub cwd: Option<String>,
    pub deleted_at: String,
    pub backup_path: String,
}

#[derive(Debug, Clone)]
pub struct BackupStore {
    root: PathBuf,
}

impl BackupStore {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn write_backup(
        &self,
        session_id: &str,
        source_db: &Path,
        tables: serde_json::Value,
    ) -> anyhow::Result<String> {
        let epoch = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let token = format!("{epoch}-{}", Uuid::new_v4().simple());
        fs::create_dir_all(&self.root).with_context(|| {
            format!(
                "failed to create backup directory {}",
                self.root.to_string_lossy()
            )
        })?;
        let payload = json!({
            "token": token,
            "session_id": session_id,
            "source_db": source_db.to_string_lossy(),
            "tables": tables,
        });
        fs::write(
            self.path_for(&token),
            serde_json::to_string_pretty(&payload)?,
        )?;
        Ok(token)
    }

    pub fn read_backup(&self, token: &str) -> anyhow::Result<serde_json::Value> {
        let path = self.path_for(token);
        let text = fs::read_to_string(&path)
            .with_context(|| format!("Backup token not found: {token}"))?;
        Ok(serde_json::from_str(&text)?)
    }

    pub fn remove_backup(&self, token: &str) -> anyhow::Result<()> {
        let path = self.path_for(token);
        fs::remove_file(&path)
            .with_context(|| format!("failed to remove backup {}", path.to_string_lossy()))
    }

    pub fn list_deleted_sessions(&self) -> anyhow::Result<Vec<DeletedSessionBackup>> {
        if !self.root.exists() {
            return Ok(Vec::new());
        }
        let mut backups = Vec::new();
        for entry in fs::read_dir(&self.root).with_context(|| {
            format!(
                "failed to read backup directory {}",
                self.root.to_string_lossy()
            )
        })? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|value| value.to_str()) != Some("json") {
                continue;
            }
            let text = fs::read_to_string(&path)
                .with_context(|| format!("failed to read backup {}", path.to_string_lossy()))?;
            let payload: serde_json::Value = serde_json::from_str(&text)
                .with_context(|| format!("failed to parse backup {}", path.to_string_lossy()))?;
            let metadata = entry.metadata()?;
            backups.push(deleted_session_backup_from_payload(
                &path,
                &payload,
                metadata.modified().ok(),
            )?);
        }
        backups.sort_by(|a, b| b.deleted_at.cmp(&a.deleted_at).then(a.title.cmp(&b.title)));
        Ok(backups)
    }

    pub fn path_for(&self, token: &str) -> PathBuf {
        let safe: String = token
            .chars()
            .filter(|ch| ch.is_ascii_alphanumeric() || *ch == '-' || *ch == '_')
            .collect();
        self.root.join(format!("{safe}.json"))
    }
}

fn deleted_session_backup_from_payload(
    path: &Path,
    payload: &serde_json::Value,
    modified: Option<SystemTime>,
) -> anyhow::Result<DeletedSessionBackup> {
    let token = payload
        .get("token")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default()
        .trim();
    if token.is_empty() {
        anyhow::bail!("backup token is missing: {}", path.to_string_lossy());
    }
    let session_id = payload
        .get("session_id")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default()
        .trim();
    if session_id.is_empty() {
        anyhow::bail!("backup session_id is missing: {}", path.to_string_lossy());
    }
    let thread = payload
        .pointer("/tables/threads/0")
        .or_else(|| payload.pointer("/tables/sessions/0"));
    let title = thread
        .and_then(|row| row.get("title"))
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .unwrap_or(session_id)
        .to_string();
    let cwd = thread
        .and_then(|row| row.get("cwd"))
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(str::to_string);
    let deleted_at = modified
        .map(DateTime::<Utc>::from)
        .unwrap_or_else(|| DateTime::<Utc>::from(SystemTime::UNIX_EPOCH))
        .to_rfc3339();

    Ok(DeletedSessionBackup {
        token: token.to_string(),
        session_id: session_id.to_string(),
        title,
        cwd,
        deleted_at,
        backup_path: path.to_string_lossy().to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn list_deleted_sessions_reads_thread_backup_metadata() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let store = BackupStore::new(temp_dir.path());
        let token = store
            .write_backup(
                "thread-1",
                Path::new("/tmp/state.sqlite"),
                json!({
                    "threads": [{
                        "id": "thread-1",
                        "title": "Deleted Thread",
                        "cwd": "/repo",
                        "rollout_path": "/tmp/rollout.jsonl"
                    }]
                }),
            )
            .expect("write backup");

        let backups = store.list_deleted_sessions().expect("list backups");

        assert_eq!(backups.len(), 1);
        assert_eq!(backups[0].token, token);
        assert_eq!(backups[0].session_id, "thread-1");
        assert_eq!(backups[0].title, "Deleted Thread");
        assert_eq!(backups[0].cwd.as_deref(), Some("/repo"));
    }

    #[test]
    fn list_deleted_sessions_returns_empty_for_missing_directory() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let store = BackupStore::new(temp_dir.path().join("missing"));

        let backups = store.list_deleted_sessions().expect("list backups");

        assert!(backups.is_empty());
    }
}
