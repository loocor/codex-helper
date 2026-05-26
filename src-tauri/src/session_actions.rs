use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use rusqlite::Connection;
use serde_json::{json, Value};

use crate::codex_app_server::{CodexAppServerClient, ThreadForker};
use crate::markdown::{export_rollout, MarkdownExportService};
use crate::models::SessionRef;
use crate::zed::{resolve_ssh_target_for_host_id, SshTarget};

pub fn export_markdown_response(payload: &Value) -> Value {
    match session_from_payload(payload) {
        Ok(session) => {
            let host_id = string_payload(payload, "host_id")
                .or_else_nonempty(|| string_payload(payload, "hostId"));
            let friendly_title = match friendly_title_for_export(payload, &session, &host_id) {
                Ok(title) => title,
                Err(error) => return failed_export_value(&session.session_id, error.to_string()),
            };
            if host_id.is_empty() {
                return match default_codex_db_path() {
                    Ok(db_path) => serde_json::to_value(
                        MarkdownExportService::new(Some(db_path))
                            .export_with_title(&session, friendly_title.as_deref()),
                    )
                    .unwrap_or_else(failed_value),
                    Err(error) => failed_export_value(&session.session_id, error.to_string()),
                };
            }
            let result = (|| -> anyhow::Result<Value> {
                let target = resolve_ssh_target_for_host_id(&host_id, None)?;
                let record = remote_thread_record(&target, &session)?;
                let rollout = download_remote_rollout(&target, &record.rollout_path)?;
                Ok(serde_json::to_value(export_rollout(
                    &crate::codex_app_server::normalize_thread_id(&session.session_id),
                    friendly_title
                        .as_deref()
                        .unwrap_or_else(|| record.title.as_deref().unwrap_or(&session.title)),
                    rollout.path(),
                ))?)
            })();
            result
                .unwrap_or_else(|error| failed_export_value(&session.session_id, error.to_string()))
        }
        Err(error) => failed_export_value("", error.to_string()),
    }
}

pub fn auto_rename_chat_response(payload: &Value) -> Value {
    let result = (|| -> anyhow::Result<Value> {
        let session = session_from_payload(payload)?;
        let options = auto_naming_options_from_payload(payload)?;
        let host_id = string_payload(payload, "host_id")
            .or_else_nonempty(|| string_payload(payload, "hostId"));
        let client = app_server_client_for_host(&host_id)?;
        let name = client.generate_thread_name(
            &session.session_id,
            options.min_chars,
            options.max_chars,
        )?;
        client.set_thread_name(&session.session_id, &name)?;
        Ok(json!({
            "status": "renamed",
            "session_id": crate::codex_app_server::normalize_thread_id(&session.session_id),
            "name": name,
            "source": "generated",
            "message": format!("Regenerated chat title: {name}"),
        }))
    })();
    result.unwrap_or_else(|error| {
        json!({
            "status": "failed",
            "session_id": payload.get("session_id").or_else(|| payload.get("sessionId")).and_then(Value::as_str).unwrap_or(""),
            "message": error.to_string(),
        })
    })
}

pub fn fork_thread_project_response(payload: &Value) -> Value {
    let session = match session_from_payload(payload) {
        Ok(session) => session,
        Err(error) => return json!({ "status": "failed", "message": error.to_string() }),
    };
    let target_cwd = payload
        .get("target_cwd")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let target_name = payload
        .get("target_name")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .unwrap_or(&session.title);
    let source_host_id = string_payload(payload, "source_host_id")
        .or_else_nonempty(|| string_payload(payload, "sourceHostId"));
    let target_host_id = string_payload(payload, "target_host_id")
        .or_else_nonempty(|| string_payload(payload, "targetHostId"));
    let fork_result = fork_thread_project(
        &session,
        &source_host_id,
        &target_host_id,
        target_cwd,
        target_name,
    );
    match fork_result {
        Ok(thread) => json!({
            "status": "forked",
            "session_id": crate::codex_app_server::normalize_thread_id(&session.session_id),
            "new_session_id": thread.session_id,
            "message": "Conversation forked",
            "target_cwd": target_cwd.trim(),
            "target_name": target_name,
            "warning": thread.warning,
        }),
        Err(error) => {
            json!({ "status": "failed", "session_id": session.session_id, "message": error.to_string() })
        }
    }
}

#[cfg(test)]
fn fork_thread_project_response_with_forker(payload: &Value, forker: &impl ThreadForker) -> Value {
    let session = match session_from_payload(payload) {
        Ok(session) => session,
        Err(error) => return json!({ "status": "failed", "message": error.to_string() }),
    };
    let target_cwd = payload
        .get("target_cwd")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let target_name = payload
        .get("target_name")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .unwrap_or(&session.title);
    let fork_result = forker.fork_thread_to_workspace(&session.session_id, target_cwd, target_name);
    match fork_result {
        Ok(thread) => json!({
            "status": "forked",
            "session_id": crate::codex_app_server::normalize_thread_id(&session.session_id),
            "new_session_id": thread.session_id,
            "message": "Conversation forked",
            "target_cwd": target_cwd.trim(),
            "target_name": target_name,
        }),
        Err(error) => {
            json!({ "status": "failed", "session_id": session.session_id, "message": error.to_string() })
        }
    }
}

fn fork_thread_project(
    session: &SessionRef,
    source_host_id: &str,
    target_host_id: &str,
    target_cwd: &str,
    target_name: &str,
) -> anyhow::Result<crate::codex_app_server::ForkedThread> {
    let target_client = app_server_client_for_host(target_host_id)?;
    if source_host_id.trim() == target_host_id.trim() {
        return target_client.fork_thread_to_workspace(
            &session.session_id,
            target_cwd,
            target_name,
        );
    }
    let rollout = source_rollout_local_path(session, source_host_id)?;
    target_client.fork_rollout_to_workspace(rollout.path(), target_cwd, target_name)
}

fn string_payload(payload: &Value, key: &str) -> String {
    payload
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim()
        .to_string()
}

#[derive(Debug, Clone, Copy)]
struct AutoNamingOptions {
    min_chars: u8,
    max_chars: u8,
}

fn auto_naming_options_from_payload(payload: &Value) -> anyhow::Result<AutoNamingOptions> {
    let min_chars = char_count_payload_alias(payload, "autoNamingMinChars", "autoNamingMinWords")?;
    let max_chars = char_count_payload_alias(payload, "autoNamingMaxChars", "autoNamingMaxWords")?;
    if min_chars > max_chars {
        anyhow::bail!("autoNamingMinChars must be less than or equal to autoNamingMaxChars");
    }
    Ok(AutoNamingOptions {
        min_chars,
        max_chars,
    })
}

fn char_count_payload(payload: &Value, key: &str) -> anyhow::Result<u8> {
    let value = payload
        .get(key)
        .and_then(Value::as_u64)
        .ok_or_else(|| anyhow::anyhow!("{key} must be an integer"))?;
    if !(1..=20).contains(&value) {
        anyhow::bail!("{key} must be between 1 and 20");
    }
    Ok(value as u8)
}

fn char_count_payload_alias(payload: &Value, key: &str, legacy_key: &str) -> anyhow::Result<u8> {
    if payload.get(key).is_some() {
        return char_count_payload(payload, key);
    }
    char_count_payload(payload, legacy_key)
}

fn friendly_title_for_export(
    payload: &Value,
    session: &SessionRef,
    host_id: &str,
) -> anyhow::Result<Option<String>> {
    if payload
        .get("friendlyFilename")
        .and_then(Value::as_bool)
        .unwrap_or(false)
        != true
    {
        return Ok(None);
    }
    let options = auto_naming_options_from_payload(payload)?;
    let client = app_server_client_for_host(host_id)?;
    Ok(Some(client.generate_thread_name(
        &session.session_id,
        options.min_chars,
        options.max_chars,
    )?))
}

#[derive(Debug, Clone)]
struct ThreadRecord {
    title: Option<String>,
    rollout_path: String,
}

fn app_server_client_for_host(host_id: &str) -> anyhow::Result<CodexAppServerClient> {
    let host_id = host_id.trim();
    if host_id.is_empty() {
        return Ok(CodexAppServerClient::default());
    }
    Ok(CodexAppServerClient::remote(
        resolve_ssh_target_for_host_id(host_id, None)?,
    ))
}

enum SourceRollout {
    Local(PathBuf),
    Temp(tempfile::NamedTempFile),
}

impl SourceRollout {
    fn path(&self) -> &Path {
        match self {
            Self::Local(path) => path,
            Self::Temp(file) => file.path(),
        }
    }
}

fn source_rollout_local_path(
    session: &SessionRef,
    source_host_id: &str,
) -> anyhow::Result<SourceRollout> {
    let source_host_id = source_host_id.trim();
    if source_host_id.is_empty() {
        return Ok(SourceRollout::Local(rollout_path_for_session(session)?));
    }
    let target = resolve_ssh_target_for_host_id(source_host_id, None)?;
    let record = remote_thread_record(&target, session)?;
    Ok(SourceRollout::Temp(download_remote_rollout(
        &target,
        &record.rollout_path,
    )?))
}

fn local_thread_record(db_path: &Path, session: &SessionRef) -> anyhow::Result<ThreadRecord> {
    let thread_id = crate::codex_app_server::normalize_thread_id(&session.session_id);
    let db = Connection::open(db_path)?;
    let (title, rollout_path): (Option<String>, String) = db.query_row(
        "SELECT title, rollout_path FROM threads WHERE id = ?1",
        [&thread_id],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    if rollout_path.trim().is_empty() {
        anyhow::bail!("Session missing rollout file path for thread {thread_id}");
    }
    Ok(ThreadRecord {
        title,
        rollout_path,
    })
}

fn remote_thread_record(target: &SshTarget, session: &SessionRef) -> anyhow::Result<ThreadRecord> {
    let candidates = remote_state_db_candidates(target)?;
    let mut errors = Vec::new();
    for remote_db_path in candidates {
        let db_file = temp_download_file("remote-state", "sqlite")?;
        if let Err(error) = download_remote_file(
            target,
            &remote_db_path,
            db_file.path(),
            "Remote Codex database download",
        ) {
            errors.push(format!("{remote_db_path}: {error}"));
            continue;
        }
        match local_thread_record(db_file.path(), session) {
            Ok(record) => return Ok(record),
            Err(error) => errors.push(format!("{remote_db_path}: {error}")),
        }
    }
    match remote_rollout_path_by_thread_id(target, session) {
        Ok(rollout_path) => {
            return Ok(ThreadRecord {
                title: None,
                rollout_path,
            });
        }
        Err(error) => errors.push(format!("rollout lookup: {error}")),
    }
    anyhow::bail!(
        "No matching remote Codex database found for {}; checked: {}",
        crate::codex_app_server::normalize_thread_id(&session.session_id),
        if errors.is_empty() {
            "none".to_string()
        } else {
            errors.join("; ")
        }
    )
}

fn remote_state_db_candidates(target: &SshTarget) -> anyhow::Result<Vec<String>> {
    let script = r#"
if [ -n "${CODEX_HOME:-}" ]; then
  printf '%s\n' "$CODEX_HOME/state_5.sqlite"
fi
printf '%s\n' "$HOME/.codex/state_5.sqlite"
for root in "$HOME/.codex" "$HOME/Library/Application Support"; do
  if [ -d "$root" ]; then
    find "$root" -maxdepth 5 \( -name 'state_*.sqlite' -o -name 'state.sqlite' \) -type f 2>/dev/null || true
  fi
done
"#;
    let output = ssh_shell_output(target, script)?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        anyhow::bail!(
            "{}",
            if stderr.is_empty() {
                format!(
                    "Remote Codex database discovery failed with status {}",
                    output.status
                )
            } else {
                stderr
            }
        );
    }
    let mut candidates = Vec::new();
    for line in String::from_utf8_lossy(&output.stdout).lines() {
        let candidate = line.trim();
        if candidate.is_empty() || candidates.iter().any(|value| value == candidate) {
            continue;
        }
        candidates.push(candidate.to_string());
    }
    Ok(candidates)
}

fn remote_rollout_path_by_thread_id(
    target: &SshTarget,
    session: &SessionRef,
) -> anyhow::Result<String> {
    let thread_id = crate::codex_app_server::normalize_thread_id(&session.session_id);
    if thread_id.is_empty()
        || !thread_id
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '-')
    {
        anyhow::bail!("Invalid thread id for remote rollout lookup");
    }
    let script = format!(
        r#"
thread_id={}
{{
  if [ -n "${{CODEX_HOME:-}}" ] && [ -d "$CODEX_HOME/sessions" ]; then
    find "$CODEX_HOME/sessions" -type f -name "*$thread_id*.jsonl" 2>/dev/null
  fi
  if [ -d "$HOME/.codex/sessions" ]; then
    find "$HOME/.codex/sessions" -type f -name "*$thread_id*.jsonl" 2>/dev/null
  fi
}} | sort | tail -n 1
"#,
        shell_quote(&thread_id)
    );
    let output = ssh_shell_output(target, script)?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        anyhow::bail!(
            "{}",
            if stderr.is_empty() {
                format!("Remote rollout lookup failed with status {}", output.status)
            } else {
                stderr
            }
        );
    }
    let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if path.is_empty() {
        anyhow::bail!("Remote rollout file not found for thread {thread_id}");
    }
    Ok(path)
}

fn download_remote_rollout(
    target: &SshTarget,
    remote_path: &str,
) -> anyhow::Result<tempfile::NamedTempFile> {
    let remote_path = remote_path.trim();
    if remote_path.is_empty() {
        anyhow::bail!("Remote rollout path is empty");
    }
    let destination = temp_download_file("remote-rollout", "jsonl")?;
    download_remote_file(
        target,
        remote_path,
        destination.path(),
        "Remote rollout download",
    )?;
    Ok(destination)
}

fn download_remote_file(
    target: &SshTarget,
    remote_path: &str,
    destination: &Path,
    label: &str,
) -> anyhow::Result<()> {
    let script = format!("cat {}", shell_quote(remote_path));
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)?;
    }
    let file = fs::File::create(destination)?;
    let output = ssh_command(target)
        .arg(format!("sh -lc {}", shell_quote(&script)))
        .stdout(Stdio::from(file))
        .stderr(Stdio::piped())
        .output()?;
    ensure_success_status(&output, label)
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn ensure_success_status(output: &std::process::Output, label: &str) -> anyhow::Result<()> {
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        anyhow::bail!(
            "{}",
            if stderr.is_empty() {
                format!("{label} failed with status {}", output.status)
            } else {
                stderr
            }
        );
    }
    Ok(())
}

fn ssh_command(target: &SshTarget) -> Command {
    let mut command = Command::new("ssh");
    if let Some(port) = target.port {
        command.arg("-p").arg(port.to_string());
    }
    command.arg(ssh_target_arg(target));
    command
}

fn ssh_shell_output(
    target: &SshTarget,
    script: impl AsRef<str>,
) -> anyhow::Result<std::process::Output> {
    Ok(ssh_command(target)
        .arg(format!("sh -lc {}", shell_quote(script.as_ref())))
        .output()?)
}

fn ssh_target_arg(target: &SshTarget) -> String {
    let user_prefix = if target.user.trim().is_empty() {
        String::new()
    } else {
        format!("{}@", target.user.trim())
    };
    format!("{user_prefix}{}", target.host)
}

fn temp_download_file(prefix: &str, extension: &str) -> anyhow::Result<tempfile::NamedTempFile> {
    Ok(tempfile::Builder::new()
        .prefix(&format!("{prefix}-"))
        .suffix(&format!(".{}", extension.trim_start_matches('.')))
        .tempfile()?)
}

trait NonEmptyStringExt {
    fn or_else_nonempty<F>(self, fallback: F) -> String
    where
        F: FnOnce() -> String;
}

impl NonEmptyStringExt for String {
    fn or_else_nonempty<F>(self, fallback: F) -> String
    where
        F: FnOnce() -> String,
    {
        if self.is_empty() {
            fallback()
        } else {
            self
        }
    }
}

fn rollout_path_for_session(session: &SessionRef) -> anyhow::Result<PathBuf> {
    let thread_id = crate::codex_app_server::normalize_thread_id(&session.session_id);
    let db = Connection::open(default_codex_db_path()?)?;
    let rollout_path: String = db.query_row(
        "SELECT rollout_path FROM threads WHERE id = ?1",
        [&thread_id],
        |row| row.get(0),
    )?;
    let path = PathBuf::from(rollout_path.trim());
    if !path.is_file() {
        anyhow::bail!(
            "Rollout file not found for session {thread_id}: {}",
            path.display()
        );
    }
    Ok(path)
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
    failed_export_value("", error.to_string())
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

    struct FakeThreadForker {
        session_id: String,
    }

    impl crate::codex_app_server::ThreadForker for FakeThreadForker {
        fn fork_thread_to_workspace(
            &self,
            _thread_id: &str,
            _target_cwd: &str,
            _name: &str,
        ) -> anyhow::Result<crate::codex_app_server::ForkedThread> {
            Ok(crate::codex_app_server::ForkedThread {
                session_id: self.session_id.clone(),
                warning: None,
            })
        }

        fn fork_rollout_to_workspace(
            &self,
            _rollout_path: &std::path::Path,
            _target_cwd: &str,
            _name: &str,
        ) -> anyhow::Result<crate::codex_app_server::ForkedThread> {
            Ok(crate::codex_app_server::ForkedThread {
                session_id: self.session_id.clone(),
                warning: None,
            })
        }
    }

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

    #[test]
    fn fork_thread_project_response_returns_forked_thread() {
        let response = fork_thread_project_response_with_forker(
            &json!({
                "session_id": "local:thread-1",
                "title": "Original",
                "target_cwd": "/tmp/project"
            }),
            &FakeThreadForker {
                session_id: "thread-2".to_string(),
            },
        );

        assert_eq!(response["status"], "forked");
        assert_eq!(response["session_id"], "thread-1");
        assert_eq!(response["new_session_id"], "thread-2");
        assert_eq!(response["target_cwd"], "/tmp/project");
        assert_eq!(response["message"], "Conversation forked");
    }
}
