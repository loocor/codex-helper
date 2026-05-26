use crate::models::{ExportResult, ExportStatus};
use serde_json::Value;
use std::fs;
use std::path::Path;

#[derive(Debug)]
pub struct ExportableRollout {
    messages: Vec<Message>,
}

pub fn export_validated_rollout(
    thread_id: &str,
    title: &str,
    rollout: &ExportableRollout,
) -> ExportResult {
    let title = display_title(title);
    let filename = build_filename(&title, thread_id);
    let markdown = render_markdown(&title, &rollout.messages);
    ExportResult {
        status: ExportStatus::Exported,
        session_id: thread_id.to_string(),
        message: format!("Exported as Markdown:{filename}"),
        filename: Some(filename),
        markdown: Some(markdown),
    }
}

pub fn validate_exportable_rollout(
    thread_id: &str,
    rollout_path: &Path,
) -> Result<ExportableRollout, ExportResult> {
    if !rollout_path.is_file() {
        return Err(failed(
            thread_id,
            format!(
                "Rollout file does not exist:{}",
                rollout_path.to_string_lossy()
            ),
        ));
    }
    match load_messages(rollout_path) {
        Ok(messages) if messages.is_empty() => Err(failed(
            thread_id,
            "No exportable user or assistant messages found",
        )),
        Ok(messages) => Ok(ExportableRollout { messages }),
        Err(error) => Err(failed(thread_id, format!("Failed to read rollout:{error}"))),
    }
}

#[derive(Debug)]
struct Message {
    speaker: &'static str,
    timestamp: Option<String>,
    body: String,
}

fn failed(session_id: &str, message: impl Into<String>) -> ExportResult {
    ExportResult {
        status: ExportStatus::Failed,
        session_id: session_id.to_string(),
        message: message.into(),
        filename: None,
        markdown: None,
    }
}

fn load_messages(path: &Path) -> anyhow::Result<Vec<Message>> {
    let mut messages = Vec::new();
    for raw in fs::read_to_string(path)?.lines() {
        if raw.trim().is_empty() {
            continue;
        }
        let event: Value = serde_json::from_str(raw)?;
        if event.get("type") != Some(&Value::String("response_item".to_string())) {
            continue;
        }
        let payload = &event["payload"];
        if payload.get("type") != Some(&Value::String("message".to_string())) {
            continue;
        }
        let role = payload.get("role").and_then(Value::as_str).unwrap_or("");
        let speaker = match role {
            "user" => "User",
            "assistant" => "Assistant",
            _ => continue,
        };
        let body = serialize_message_content(&payload["content"]);
        if body.is_empty() {
            continue;
        }
        messages.push(Message {
            speaker,
            timestamp: format_timestamp(event.get("timestamp")),
            body,
        });
    }
    Ok(messages)
}

fn serialize_message_content(content: &Value) -> String {
    let Some(items) = content.as_array() else {
        return String::new();
    };
    items
        .iter()
        .filter_map(|block| {
            let block_type = block.get("type").and_then(Value::as_str)?;
            match block_type {
                "input_text" | "output_text" => {
                    let text =
                        normalize_newlines(block.get("text").and_then(Value::as_str).unwrap_or(""))
                            .trim_matches('\n')
                            .to_string();
                    (!text.trim().is_empty()).then_some(text)
                }
                "input_image" => {
                    let image_url = block
                        .get("image_url")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .trim();
                    if image_url.is_empty() || image_url.starts_with("data:") {
                        Some("> Image attachment".to_string())
                    } else {
                        Some(format!("> Image attachment\n[Image link](<{image_url}>)"))
                    }
                }
                _ => None,
            }
        })
        .filter(|block| !block.trim().is_empty())
        .collect::<Vec<_>>()
        .join("\n\n")
        .trim()
        .to_string()
}

fn format_timestamp(value: Option<&Value>) -> Option<String> {
    let raw = value?.as_str()?.trim();
    if raw.is_empty() {
        return None;
    }
    let normalized = raw
        .strip_suffix('Z')
        .map_or_else(|| raw.to_string(), |prefix| format!("{prefix}+00:00"));
    let parsed = chrono::DateTime::parse_from_rfc3339(&normalized).ok()?;
    Some(
        parsed
            .with_timezone(&chrono::Local)
            .format("%Y-%m-%d %H:%M:%S")
            .to_string(),
    )
}

fn display_title(value: &str) -> String {
    let normalized = normalize_newlines(value)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    if normalized.is_empty() {
        "Untitled session".to_string()
    } else {
        normalized
    }
}

fn build_filename(title: &str, thread_id: &str) -> String {
    let cleaned = collapse_whitespace(&replace_windows_filename_chars(title, " "))
        .trim_matches([' ', '.'])
        .to_string();
    let mut safe_title = cleaned
        .chars()
        .take(80)
        .collect::<String>()
        .trim_matches([' ', '.'])
        .to_string();
    if safe_title.is_empty() {
        safe_title = "Untitled session".to_string();
    }
    let safe_thread_id = replace_windows_filename_chars(thread_id, "-");
    format!("{safe_title}-{}.md", safe_thread_id.trim())
}

fn render_markdown(title: &str, messages: &[Message]) -> String {
    let mut lines = vec![format!("# {title}"), String::new()];
    for message in messages {
        lines.push(format!("### {}", message.speaker));
        if let Some(timestamp) = &message.timestamp {
            lines.push(format!("_{timestamp}_"));
        }
        lines.push(String::new());
        lines.push(message.body.trim_end().to_string());
        lines.push(String::new());
    }
    format!("{}\n", lines.join("\n").trim_end())
}

fn normalize_newlines(value: &str) -> String {
    value.replace("\r\n", "\n").replace('\r', "\n")
}

fn replace_windows_filename_chars(value: &str, replacement: &str) -> String {
    let mut output = String::new();
    for ch in value.chars() {
        if matches!(ch, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*') || ch.is_control() {
            output.push_str(replacement);
        } else {
            output.push(ch);
        }
    }
    output
}

fn collapse_whitespace(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_exportable_rollout_rejects_empty_rollouts() {
        let file = tempfile::NamedTempFile::new().expect("rollout");

        let result = validate_exportable_rollout("thread-1", file.path())
            .expect_err("empty rollout should not be exportable");

        assert_eq!(result.status, ExportStatus::Failed);
        assert!(result
            .message
            .contains("No exportable user or assistant messages"));
    }

    #[test]
    fn validate_exportable_rollout_accepts_message_rollouts() {
        let file = tempfile::NamedTempFile::new().expect("rollout");
        fs::write(
            file.path(),
            r#"{"type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"hello"}]}}"#,
        )
        .expect("write rollout");

        let result = validate_exportable_rollout("thread-1", file.path());

        assert!(result.is_ok());
    }
}
