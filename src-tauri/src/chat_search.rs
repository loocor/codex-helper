use anyhow::Context;
use chrono::{DateTime, Utc};
use rusqlite::Connection;
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use crate::state_dir::StateDir;

const CHAT_SEARCH_RESULT_LIMIT: usize = 100;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ChatMatchKind {
    Metadata,
    Content,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ChatSearchMatch {
    pub session_id: String,
    pub title: String,
    pub cwd: Option<String>,
    pub time: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
    pub match_kind: ChatMatchKind,
    pub matched_fields: Vec<String>,
}

#[derive(Debug)]
struct ChatSearchEntry {
    session_id: String,
    title: String,
    cwd: Option<String>,
    time: Option<String>,
    token: Option<String>,
    content: Option<String>,
}

pub fn search_chats_response(state_dir: &StateDir, payload: &Value) -> Value {
    let scope = payload
        .get("scope")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim();
    let query = payload
        .get("query")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim();
    if query.is_empty() {
        return json!({
            "status": "ok",
            "scope": scope,
            "query": query,
            "matches": [],
        });
    }
    let result = match scope {
        "archived" => default_codex_db_path().and_then(|path| search_archived_chats(path, query)),
        "deleted" => search_deleted_chats(&state_dir.backups_dir, query),
        _ => Err(anyhow::anyhow!("Unknown chat search scope: {scope}")),
    };
    match result {
        Ok(matches) => json!({
            "status": "ok",
            "scope": scope,
            "query": query,
            "matches": matches,
        }),
        Err(error) => json!({
            "status": "failed",
            "scope": scope,
            "query": query,
            "message": error.to_string(),
        }),
    }
}

pub fn search_archived_chats(
    db_path: impl AsRef<Path>,
    query: &str,
) -> anyhow::Result<Vec<ChatSearchMatch>> {
    let query = normalize_query(query);
    if query.is_empty() {
        return Ok(Vec::new());
    }
    let db_path = db_path.as_ref();
    if !db_path.exists() {
        anyhow::bail!("Database does not exist: {}", db_path.to_string_lossy());
    }
    let db = Connection::open(db_path)?;
    let columns = table_columns(&db, "threads")?;
    let existing: HashSet<&str> = columns.iter().map(String::as_str).collect();
    for required in ["id", "title", "cwd", "rollout_path", "archived", "source"] {
        if !existing.contains(required) {
            anyhow::bail!("threads table missing required column: {required}");
        }
    }
    let updated_at_ms = optional_thread_column(&existing, "updated_at_ms");
    let updated_at = optional_thread_column(&existing, "updated_at");
    let created_at_ms = optional_thread_column(&existing, "created_at_ms");
    let created_at = optional_thread_column(&existing, "created_at");
    let thread_source = if existing.contains("thread_source") {
        "thread_source"
    } else {
        "NULL"
    };
    let sql = format!(
        "SELECT id, title, cwd, rollout_path, source, {thread_source}, {updated_at_ms}, {updated_at}, {created_at_ms}, {created_at}
         FROM threads
         WHERE archived = 1
         ORDER BY COALESCE({updated_at_ms}, {updated_at} * 1000, {created_at_ms}, {created_at} * 1000, 0) DESC, id DESC"
    );
    let mut stmt = db.prepare(&sql)?;
    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, Option<String>>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, String>(4)?,
            row.get::<_, Option<String>>(5)?,
            timestamp_from_columns(
                row.get::<_, Option<i64>>(6)?,
                row.get::<_, Option<i64>>(7)?,
                row.get::<_, Option<i64>>(8)?,
                row.get::<_, Option<i64>>(9)?,
            ),
        ))
    })?;
    let mut entries = Vec::new();
    for row in rows {
        let (session_id, title, cwd, rollout_path, source, thread_source, time): (
            String,
            String,
            Option<String>,
            String,
            String,
            Option<String>,
            Option<String>,
        ) = row?;
        if !archived_thread_is_local(&source, thread_source.as_deref()) {
            continue;
        }
        let content = read_rollout_content(Path::new(&rollout_path))
            .with_context(|| format!("failed to read rollout {}", rollout_path))?;
        entries.push(ChatSearchEntry {
            session_id,
            title,
            cwd,
            time,
            token: None,
            content: Some(content),
        });
    }
    collect_matches(entries, &query)
}

pub fn search_deleted_chats(
    backups_dir: impl AsRef<Path>,
    query: &str,
) -> anyhow::Result<Vec<ChatSearchMatch>> {
    let query = normalize_query(query);
    if query.is_empty() {
        return Ok(Vec::new());
    }
    let backups_dir = backups_dir.as_ref();
    if !backups_dir.exists() {
        return Ok(Vec::new());
    }
    let mut entries = Vec::new();
    for entry in fs::read_dir(backups_dir).with_context(|| {
        format!(
            "failed to read backup directory {}",
            backups_dir.to_string_lossy()
        )
    })? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|value| value.to_str()) != Some("json") {
            continue;
        }
        let metadata = entry.metadata().ok();
        let text = fs::read_to_string(&path)
            .with_context(|| format!("failed to read backup {}", path.to_string_lossy()))?;
        let payload: Value = serde_json::from_str(&text)
            .with_context(|| format!("failed to parse backup {}", path.to_string_lossy()))?;
        entries.push(
            deleted_entry_from_payload(&payload, metadata.and_then(|m| m.modified().ok()))
                .with_context(|| {
                    format!("failed to read backup content {}", path.to_string_lossy())
                })?,
        );
    }
    entries.sort_by(|a, b| b.time.cmp(&a.time).then(a.title.cmp(&b.title)));
    collect_matches(entries, &query)
}

fn deleted_entry_from_payload(
    payload: &Value,
    modified: Option<SystemTime>,
) -> anyhow::Result<ChatSearchEntry> {
    let session_id = payload
        .get("session_id")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let token = payload
        .get("token")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(str::to_string);
    let thread = payload
        .pointer("/tables/threads/0")
        .or_else(|| payload.pointer("/tables/sessions/0"));
    let title = thread
        .and_then(|row| row.get("title"))
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .unwrap_or(&session_id)
        .to_string();
    let cwd = thread
        .and_then(|row| row.get("cwd"))
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(str::to_string);
    let time = thread
        .and_then(chat_time_from_thread)
        .or_else(|| modified.map(system_time_rfc3339));
    Ok(ChatSearchEntry {
        session_id,
        title,
        cwd,
        time,
        token,
        content: deleted_backup_content(payload)?,
    })
}

fn collect_matches(
    entries: Vec<ChatSearchEntry>,
    query: &str,
) -> anyhow::Result<Vec<ChatSearchMatch>> {
    let mut matches = Vec::new();
    for entry in entries {
        let mut fields = metadata_matches(&entry, query);
        let content_matches = entry
            .content
            .as_deref()
            .is_some_and(|content| contains_literal(content, query));
        if fields.is_empty() && content_matches {
            fields.push("content".to_string());
        }
        if fields.is_empty() {
            continue;
        }
        let match_kind = if fields.iter().any(|field| field != "content") {
            ChatMatchKind::Metadata
        } else {
            ChatMatchKind::Content
        };
        matches.push(ChatSearchMatch {
            session_id: entry.session_id,
            title: display_title(&entry.title),
            cwd: entry.cwd,
            time: entry.time,
            token: entry.token,
            match_kind,
            matched_fields: fields,
        });
        if matches.len() >= CHAT_SEARCH_RESULT_LIMIT {
            break;
        }
    }
    Ok(matches)
}

fn archived_thread_is_local(source: &str, thread_source: Option<&str>) -> bool {
    let source = source.trim();
    let thread_source = thread_source.unwrap_or("").trim();
    let source_lower = source.to_lowercase();
    let thread_source_lower = thread_source.to_lowercase();
    if source_lower.contains("remote")
        || source_lower.contains("ssh")
        || thread_source_lower.contains("remote")
        || thread_source_lower.contains("ssh")
    {
        return false;
    }
    matches!(source, "local" | "vscode" | "cli")
        || (thread_source == "subagent" && source_lower.contains("\"subagent\""))
}

fn metadata_matches(entry: &ChatSearchEntry, query: &str) -> Vec<String> {
    let mut fields = Vec::new();
    if contains_literal(&entry.title, query) {
        fields.push("title".to_string());
    }
    if contains_literal(&entry.session_id, query) {
        fields.push("session_id".to_string());
    }
    if entry
        .cwd
        .as_deref()
        .is_some_and(|cwd| contains_literal(cwd, query))
    {
        fields.push("cwd".to_string());
    }
    if entry
        .time
        .as_deref()
        .is_some_and(|time| contains_literal(time, query))
    {
        fields.push("time".to_string());
    }
    fields
}

fn contains_literal(value: &str, query: &str) -> bool {
    value.to_lowercase().contains(query)
}

fn normalize_query(query: &str) -> String {
    query.trim().to_lowercase()
}

fn table_columns(db: &Connection, table: &str) -> anyhow::Result<Vec<String>> {
    let mut stmt = db.prepare(&format!(
        "PRAGMA table_info(\"{}\")",
        table.replace('"', "\"\"")
    ))?;
    let rows = stmt.query_map([], |row| row.get::<_, String>(1))?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

fn optional_thread_column(existing: &HashSet<&str>, column: &str) -> &'static str {
    if existing.contains(column) {
        match column {
            "updated_at_ms" => "updated_at_ms",
            "updated_at" => "updated_at",
            "created_at_ms" => "created_at_ms",
            "created_at" => "created_at",
            _ => "NULL",
        }
    } else {
        "NULL"
    }
}

fn read_rollout_content(path: &Path) -> anyhow::Result<String> {
    let mut output = String::new();
    for raw in fs::read_to_string(path)?.lines() {
        if raw.trim().is_empty() {
            continue;
        }
        let event: Value = serde_json::from_str(raw)?;
        append_message_content(&mut output, &event);
    }
    Ok(output)
}

fn deleted_backup_content(payload: &Value) -> anyhow::Result<Option<String>> {
    let mut output = String::new();
    let Some(files) = payload.pointer("/tables/__files").and_then(Value::as_array) else {
        return Ok(None);
    };
    for file in files {
        let Some(encoded) = file.get("content_b64").and_then(Value::as_str) else {
            continue;
        };
        let bytes = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, encoded)
            .context("failed to decode backup transcript content")?;
        let text = String::from_utf8(bytes).context("backup transcript content is not utf-8")?;
        for raw in text.lines() {
            if raw.trim().is_empty() {
                continue;
            }
            let event: Value =
                serde_json::from_str(raw).context("failed to parse backup transcript JSONL")?;
            append_message_content(&mut output, &event);
        }
    }
    Ok((!output.trim().is_empty()).then_some(output))
}

fn append_message_content(output: &mut String, event: &Value) {
    if event.get("type") != Some(&Value::String("response_item".to_string())) {
        return;
    }
    let payload = &event["payload"];
    if payload.get("type") != Some(&Value::String("message".to_string())) {
        return;
    }
    let role = payload.get("role").and_then(Value::as_str).unwrap_or("");
    if !matches!(role, "user" | "assistant") {
        return;
    }
    let Some(content) = payload.get("content").and_then(Value::as_array) else {
        return;
    };
    for block in content {
        let block_type = block.get("type").and_then(Value::as_str).unwrap_or("");
        if matches!(block_type, "input_text" | "output_text") {
            let text = block.get("text").and_then(Value::as_str).unwrap_or("");
            if !text.trim().is_empty() {
                output.push_str(text);
                output.push('\n');
            }
        }
    }
}

fn chat_time_from_thread(thread: &Value) -> Option<String> {
    timestamp_from_columns(
        thread.get("updated_at_ms").and_then(Value::as_i64),
        thread.get("updated_at").and_then(Value::as_i64),
        thread.get("created_at_ms").and_then(Value::as_i64),
        thread.get("created_at").and_then(Value::as_i64),
    )
}

fn timestamp_from_columns(
    updated_at_ms: Option<i64>,
    updated_at: Option<i64>,
    created_at_ms: Option<i64>,
    created_at: Option<i64>,
) -> Option<String> {
    updated_at_ms
        .or_else(|| updated_at.map(|value| value * 1000))
        .or(created_at_ms)
        .or_else(|| created_at.map(|value| value * 1000))
        .and_then(timestamp_ms_rfc3339)
}

fn timestamp_ms_rfc3339(value: i64) -> Option<String> {
    DateTime::<Utc>::from_timestamp_millis(value).map(|date| date.to_rfc3339())
}

fn system_time_rfc3339(value: SystemTime) -> String {
    DateTime::<Utc>::from(value).to_rfc3339()
}

fn display_title(value: &str) -> String {
    let title = value.split_whitespace().collect::<Vec<_>>().join(" ");
    if title.is_empty() {
        "Untitled chat".to_string()
    } else {
        title
    }
}

fn default_codex_db_path() -> anyhow::Result<PathBuf> {
    let home = dirs::home_dir().context("home directory is not available")?;
    Ok(home.join(".codex").join("state_5.sqlite"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::Engine;
    use rusqlite::Connection;
    use serde_json::json;
    use std::fs;
    use std::path::Path;

    fn create_codex_db(path: &Path) {
        let db = Connection::open(path).expect("db");
        db.execute_batch(
            r#"
            CREATE TABLE threads (
                id TEXT PRIMARY KEY,
                rollout_path TEXT NOT NULL,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL,
                source TEXT NOT NULL,
                model_provider TEXT NOT NULL,
                cwd TEXT NOT NULL,
                title TEXT NOT NULL,
                sandbox_policy TEXT NOT NULL,
                approval_mode TEXT NOT NULL,
                archived INTEGER NOT NULL DEFAULT 0,
                created_at_ms INTEGER,
                updated_at_ms INTEGER,
                thread_source TEXT
            );
            "#,
        )
        .expect("schema");
    }

    fn write_rollout(path: &Path, body: &str) {
        let event = json!({
            "type": "response_item",
            "timestamp": "2026-05-25T08:00:00Z",
            "payload": {
                "type": "message",
                "role": "user",
                "content": [{ "type": "input_text", "text": body }]
            }
        });
        fs::write(path, format!("{event}\n")).expect("rollout");
    }

    #[test]
    fn local_archived_search_matches_metadata() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let db_path = temp_dir.path().join("state_5.sqlite");
        let rollout_path = temp_dir.path().join("rollout.jsonl");
        create_codex_db(&db_path);
        write_rollout(&rollout_path, "unrelated body");
        let db = Connection::open(&db_path).expect("db");
        db.execute(
            "INSERT INTO threads (id, rollout_path, created_at, updated_at, source, model_provider, cwd, title, sandbox_policy, approval_mode, archived, created_at_ms, updated_at_ms)
             VALUES (?1, ?2, 1, 2, 'local', 'openai', '/repo/CodexHelper', 'Refactor settings page', 'workspace-write', 'on-request', 1, 1000, 2000)",
            (&"thread-1", &rollout_path.to_string_lossy().to_string()),
        )
        .expect("insert");

        let matches = search_archived_chats(&db_path, "settings").expect("search");

        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].session_id, "thread-1");
        assert_eq!(matches[0].title, "Refactor settings page");
        assert_eq!(matches[0].match_kind, ChatMatchKind::Metadata);
        assert!(matches[0].matched_fields.contains(&"title".to_string()));
    }

    #[test]
    fn local_archived_search_matches_rollout_content() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let db_path = temp_dir.path().join("state_5.sqlite");
        let rollout_path = temp_dir.path().join("rollout.jsonl");
        create_codex_db(&db_path);
        write_rollout(&rollout_path, "needle only appears inside transcript");
        let db = Connection::open(&db_path).expect("db");
        db.execute(
            "INSERT INTO threads (id, rollout_path, created_at, updated_at, source, model_provider, cwd, title, sandbox_policy, approval_mode, archived, created_at_ms, updated_at_ms)
             VALUES (?1, ?2, 1, 2, 'local', 'openai', '/repo/CodexHelper', 'Unrelated title', 'workspace-write', 'on-request', 1, 1000, 2000)",
            (&"thread-2", &rollout_path.to_string_lossy().to_string()),
        )
        .expect("insert");

        let matches = search_archived_chats(&db_path, "NEEDLE").expect("search");

        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].session_id, "thread-2");
        assert_eq!(matches[0].match_kind, ChatMatchKind::Content);
        assert!(matches[0].matched_fields.contains(&"content".to_string()));
    }

    #[test]
    fn local_archived_search_ignores_remote_threads() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let db_path = temp_dir.path().join("state_5.sqlite");
        let local_rollout_path = temp_dir.path().join("local-rollout.jsonl");
        let remote_rollout_path = temp_dir.path().join("remote-rollout.jsonl");
        create_codex_db(&db_path);
        write_rollout(&local_rollout_path, "shared search term");
        write_rollout(&remote_rollout_path, "shared search term");
        let db = Connection::open(&db_path).expect("db");
        db.execute(
            "INSERT INTO threads (id, rollout_path, created_at, updated_at, source, model_provider, cwd, title, sandbox_policy, approval_mode, archived, created_at_ms, updated_at_ms)
             VALUES (?1, ?2, 1, 2, 'local', 'openai', '/repo/CodexHelper', 'Shared local chat', 'workspace-write', 'on-request', 1, 1000, 2000)",
            (&"local-thread", &local_rollout_path.to_string_lossy().to_string()),
        )
        .expect("insert local");
        db.execute(
            "INSERT INTO threads (id, rollout_path, created_at, updated_at, source, model_provider, cwd, title, sandbox_policy, approval_mode, archived, created_at_ms, updated_at_ms)
             VALUES (?1, ?2, 1, 2, 'remote', 'openai', '/repo/CodexHelper', 'Shared remote chat', 'workspace-write', 'on-request', 1, 1000, 3000)",
            (&"remote-thread", &remote_rollout_path.to_string_lossy().to_string()),
        )
        .expect("insert remote");
        db.execute(
            "INSERT INTO threads (id, rollout_path, created_at, updated_at, source, model_provider, cwd, title, sandbox_policy, approval_mode, archived, created_at_ms, updated_at_ms, thread_source)
             VALUES (?1, ?2, 1, 2, 'vscode', 'openai', '/repo/CodexHelper', 'Shared remote-kind chat', 'workspace-write', 'on-request', 1, 1000, 4000, 'remote')",
            (&"remote-kind-thread", &remote_rollout_path.to_string_lossy().to_string()),
        )
        .expect("insert remote kind");

        let matches = search_archived_chats(&db_path, "shared").expect("search");

        assert_eq!(
            matches
                .iter()
                .map(|entry| entry.session_id.as_str())
                .collect::<Vec<_>>(),
            vec!["local-thread"],
        );
    }

    #[test]
    fn deleted_search_matches_metadata_and_content() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let backups_dir = temp_dir.path().join("backups");
        fs::create_dir_all(&backups_dir).expect("backups dir");
        let rollout = json!({
            "type": "response_item",
            "payload": {
                "type": "message",
                "role": "assistant",
                "content": [{ "type": "output_text", "text": "transcript needle" }]
            }
        });
        let payload = json!({
            "token": "token-1",
            "session_id": "thread-3",
            "tables": {
                "threads": [{
                    "id": "thread-3",
                    "title": "Deleted settings chat",
                    "cwd": "/repo/CodexHelper",
                    "updated_at_ms": 2000,
                    "created_at_ms": 1000
                }],
                "__files": [{
                    "path": "/tmp/rollout.jsonl",
                    "content_b64": base64::engine::general_purpose::STANDARD.encode(format!("{rollout}\n"))
                }]
            }
        });
        fs::write(
            backups_dir.join("token-1.json"),
            serde_json::to_string_pretty(&payload).expect("json"),
        )
        .expect("backup");

        let metadata_matches = search_deleted_chats(&backups_dir, "settings").expect("search");
        let content_matches = search_deleted_chats(&backups_dir, "NEEDLE").expect("search");

        assert_eq!(metadata_matches.len(), 1);
        assert_eq!(metadata_matches[0].match_kind, ChatMatchKind::Metadata);
        assert_eq!(content_matches.len(), 1);
        assert_eq!(content_matches[0].match_kind, ChatMatchKind::Content);
    }
}
