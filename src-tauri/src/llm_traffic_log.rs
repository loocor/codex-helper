use std::sync::Arc;
use std::time::Instant;

use hyper::HeaderMap;
use serde_json::{json, Map, Value};

use crate::logging::DiagnosticLogger;

pub const USER_PREVIEW_MAX_CHARS: usize = 500;

pub struct PendingLlmLog {
    logger: Arc<DiagnosticLogger>,
    started: Instant,
    path: String,
    method: String,
    provider_id: String,
    model: Option<String>,
    request_bytes: usize,
    session_id: Option<String>,
    thread_id: Option<String>,
    turn_id: Option<String>,
    request_id: Option<String>,
    user_preview: Option<String>,
    user_preview_truncated: bool,
    request_headers: Value,
    finished: bool,
}

impl PendingLlmLog {
    pub fn start(
        logger: Arc<DiagnosticLogger>,
        path: String,
        method: String,
        provider_id: String,
        request_headers: &HeaderMap,
        request_body: &[u8],
    ) -> Self {
        let parsed = serde_json::from_slice::<Value>(request_body).ok();
        let model = parsed.as_ref().and_then(|value| {
            value
                .get("model")
                .and_then(Value::as_str)
                .map(str::to_string)
        });
        let (session_id, thread_id, turn_id, request_id) =
            request_correlation(request_headers, parsed.as_ref());
        let (user_preview, user_preview_truncated) = parsed
            .as_ref()
            .map(extract_user_preview)
            .unwrap_or((None, false));
        Self {
            logger,
            started: Instant::now(),
            path,
            method,
            provider_id,
            model,
            request_bytes: request_body.len(),
            session_id,
            thread_id,
            turn_id,
            request_id,
            user_preview,
            user_preview_truncated,
            request_headers: redact_header_map(request_headers),
            finished: false,
        }
    }

    pub fn fail(mut self, error: &str) {
        self.finish_incomplete(error);
    }

    pub fn succeed(
        mut self,
        status: u16,
        sse: bool,
        response_headers: Value,
        response_bytes: usize,
    ) {
        if let Some(request_id) = value_string(&response_headers, "x-request-id") {
            self.request_id = Some(request_id);
        }
        self.write(status, sse, response_headers, response_bytes, None);
    }

    fn write(
        &mut self,
        status: u16,
        sse: bool,
        response_headers: Value,
        response_bytes: usize,
        error: Option<&str>,
    ) {
        if self.finished {
            return;
        }
        self.finished = true;
        let mut detail = json!({
            "path": self.path,
            "method": self.method,
            "status": status,
            "providerId": self.provider_id,
            "durationMs": self.started.elapsed().as_millis() as u64,
            "sse": sse,
            "requestBytes": self.request_bytes,
            "responseBytes": response_bytes,
            "request": { "headers": self.request_headers },
            "response": { "headers": response_headers },
        });
        insert_optional(&mut detail, "model", self.model.as_deref());
        insert_optional(&mut detail, "sessionId", self.session_id.as_deref());
        insert_optional(&mut detail, "threadId", self.thread_id.as_deref());
        insert_optional(&mut detail, "turnId", self.turn_id.as_deref());
        insert_optional(&mut detail, "requestId", self.request_id.as_deref());
        insert_optional(&mut detail, "userPreview", self.user_preview.as_deref());
        if self.user_preview_truncated {
            detail["userPreviewTruncated"] = json!(true);
        }
        if let Some(error) = error {
            detail["error"] = json!(error);
        }
        if let Err(error) = self.logger.append("llm.request", detail) {
            eprintln!("failed to append llm.request log: {error}");
        }
    }

    fn finish_incomplete(&mut self, error: &str) {
        self.write(0, false, json!({}), 0, Some(error));
    }
}

impl Drop for PendingLlmLog {
    fn drop(&mut self) {
        if !self.finished {
            self.finish_incomplete("proxy request did not complete");
        }
    }
}

pub fn redact_header_map(headers: &HeaderMap) -> Value {
    redact_header_pairs(
        headers
            .iter()
            .filter_map(|(name, value)| value.to_str().ok().map(|value| (name.as_str(), value))),
    )
}

pub fn redact_header_pairs<'a, I>(headers: I) -> Value
where
    I: IntoIterator<Item = (&'a str, &'a str)>,
{
    let mut map = Map::new();
    for (key, value) in headers {
        if secret_key_name(key) {
            continue;
        }
        map.insert(key.to_string(), Value::String(value.to_string()));
    }
    Value::Object(map)
}

fn insert_optional(detail: &mut Value, key: &str, value: Option<&str>) {
    if let Some(value) = value.filter(|value| !value.is_empty()) {
        detail[key] = json!(value);
    }
}

fn request_correlation(
    headers: &HeaderMap,
    body: Option<&Value>,
) -> (
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
) {
    let metadata = header_json(headers, "x-codex-turn-metadata");
    let session_id = first_nonempty(&[
        header_value(headers, "session-id"),
        value_string(&metadata, "session_id"),
        body_path(body, &["client_metadata", "session_id"]),
    ]);
    let thread_id = first_nonempty(&[
        header_value(headers, "thread-id"),
        value_string(&metadata, "thread_id"),
        body_path(body, &["client_metadata", "thread_id"]),
    ]);
    let turn_id = first_nonempty(&[
        value_string(&metadata, "turn_id"),
        body_path(body, &["client_metadata", "turn_id"]),
    ]);
    let request_id = first_nonempty(&[
        header_value(headers, "x-request-id"),
        header_value(headers, "x-client-request-id"),
    ]);
    (session_id, thread_id, turn_id, request_id)
}

fn extract_user_preview(body: &Value) -> (Option<String>, bool) {
    let text = last_user_text(body.get("input")).or_else(|| last_user_text(body.get("messages")));
    match text {
        Some(text) => clip_user_preview(&text),
        None => (None, false),
    }
}

fn last_user_text(items: Option<&Value>) -> Option<String> {
    let items = items?.as_array()?;
    for item in items.iter().rev() {
        if let Some(text) = user_message_text(item) {
            return Some(text);
        }
    }
    None
}

fn user_message_text(item: &Value) -> Option<String> {
    let role = item.get("role").and_then(Value::as_str).unwrap_or("");
    if role != "user" {
        return None;
    }
    content_text(item.get("content")?)
}

fn content_text(content: &Value) -> Option<String> {
    match content {
        Value::String(text) => nonempty_text(text),
        Value::Array(parts) => {
            let mut chunks = Vec::new();
            for part in parts {
                if let Some(text) = part.get("text").and_then(Value::as_str) {
                    if let Some(text) = nonempty_text(text) {
                        chunks.push(text);
                    }
                } else if let Some(text) = part.as_str().and_then(nonempty_text) {
                    chunks.push(text);
                }
            }
            if chunks.is_empty() {
                None
            } else {
                Some(chunks.join("\n"))
            }
        }
        _ => None,
    }
}

fn nonempty_text(text: &str) -> Option<String> {
    let text = text.trim();
    if text.is_empty() {
        None
    } else {
        Some(text.to_string())
    }
}

fn clip_user_preview(text: &str) -> (Option<String>, bool) {
    let collapsed = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.is_empty() {
        return (None, false);
    }
    let char_count = collapsed.chars().count();
    if char_count <= USER_PREVIEW_MAX_CHARS {
        return (Some(collapsed), false);
    }
    (
        Some(format!(
            "{}…",
            collapsed
                .chars()
                .take(USER_PREVIEW_MAX_CHARS)
                .collect::<String>()
        )),
        true,
    )
}

fn header_value(headers: &HeaderMap, name: &str) -> Option<String> {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn header_json(headers: &HeaderMap, name: &str) -> Value {
    header_value(headers, name)
        .and_then(|value| serde_json::from_str(&value).ok())
        .unwrap_or(Value::Null)
}

fn value_string(value: &Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn body_path(body: Option<&Value>, path: &[&str]) -> Option<String> {
    let mut current = body?;
    for key in path {
        current = current.get(*key)?;
    }
    current
        .as_str()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn first_nonempty(values: &[Option<String>]) -> Option<String> {
    values
        .iter()
        .flatten()
        .find(|value| !value.is_empty())
        .cloned()
}

fn secret_key_name(name: &str) -> bool {
    let normalized = name.replace('-', "_").to_ascii_lowercase();
    normalized.contains("authorization")
        || normalized.contains("cookie")
        || normalized.contains("password")
        || normalized.contains("secret")
        || normalized.contains("api_key")
        || normalized.contains("apikey")
        || normalized == "token"
        || normalized.ends_with("_token")
}

#[cfg(test)]
mod tests {
    use super::*;
    use hyper::header::{HeaderValue, AUTHORIZATION, CONTENT_TYPE};

    #[test]
    fn redact_header_map_drops_authorization() {
        let mut headers = HeaderMap::new();
        headers.insert(AUTHORIZATION, HeaderValue::from_static("Bearer sk-secret"));
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        headers.insert("x-api-key", HeaderValue::from_static("abc"));
        headers.insert("x-openai-api-key", HeaderValue::from_static("sk-openai"));
        headers.insert(
            "anthropic-api-key",
            HeaderValue::from_static("sk-anthropic"),
        );

        let redacted = redact_header_map(&headers);

        assert_eq!(redacted["content-type"], "application/json");
        assert!(redacted.get("authorization").is_none());
        assert!(redacted.get("x-api-key").is_none());
        assert!(redacted.get("x-openai-api-key").is_none());
        assert!(redacted.get("anthropic-api-key").is_none());
        assert!(!redacted.to_string().contains("sk-secret"));
        assert!(!redacted.to_string().contains("abc"));
        assert!(!redacted.to_string().contains("sk-openai"));
        assert!(!redacted.to_string().contains("sk-anthropic"));
    }

    #[test]
    fn extract_user_preview_uses_last_user_message() {
        let body = json!({
            "client_metadata": { "session_id": "sess-1" },
            "input": [
                { "role": "user", "content": [{ "type": "input_text", "text": "first" }] },
                { "role": "assistant", "content": [{ "text": "ok" }] },
                { "role": "user", "content": [{ "type": "input_text", "text": "latest question" }] }
            ]
        });
        let (preview, truncated) = extract_user_preview(&body);
        assert_eq!(preview.as_deref(), Some("latest question"));
        assert!(!truncated);
    }

    #[test]
    fn clip_user_preview_collapses_and_caps_long_text() {
        let text = format!("hello\n{}", "x".repeat(USER_PREVIEW_MAX_CHARS + 20));
        let (preview, truncated) = clip_user_preview(&text);
        let preview = preview.expect("preview");
        assert!(truncated);
        assert!(preview.ends_with('…'));
        assert_eq!(preview.chars().count(), USER_PREVIEW_MAX_CHARS + 1);
        assert!(preview.starts_with("hello x"));
    }

    #[test]
    fn pending_llm_log_stores_preview_ids_and_headers_without_bodies() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let logger = Arc::new(DiagnosticLogger::new(temp_dir.path().join("logs")));
        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        headers.insert("session-id", HeaderValue::from_static("sess-1"));
        headers.insert("thread-id", HeaderValue::from_static("thread-1"));
        headers.insert(
            "x-codex-turn-metadata",
            HeaderValue::from_static(r#"{"turn_id":"turn-1","session_id":"sess-1"}"#),
        );
        headers.insert(AUTHORIZATION, HeaderValue::from_static("Bearer sk-secret"));
        let body = serde_json::to_vec(&json!({
            "model": "kimi-k2.5",
            "api_key": "sk-request",
            "messages": [
                { "role": "user", "content": "old" },
                { "role": "assistant", "content": "ok" },
                { "role": "user", "content": "search me" }
            ]
        }))
        .expect("request json");

        let pending = PendingLlmLog::start(
            logger.clone(),
            "/v1/chat/completions".to_string(),
            "POST".to_string(),
            "kimi".to_string(),
            &headers,
            &body,
        );
        pending.succeed(
            200,
            false,
            json!({
                "server": "cloudflare",
                "x-request-id": "req-1",
                "x-ratelimit-remaining-requests": "12"
            }),
            24,
        );

        let page = logger.read_latest().expect("latest");
        let detail = &page.records[0].detail;
        let stored = detail.to_string();
        assert_eq!(detail["model"], "kimi-k2.5");
        assert_eq!(detail["userPreview"], "search me");
        assert_eq!(detail["sessionId"], "sess-1");
        assert_eq!(detail["threadId"], "thread-1");
        assert_eq!(detail["turnId"], "turn-1");
        assert_eq!(detail["requestId"], "req-1");
        assert_eq!(detail["requestBytes"], body.len());
        assert_eq!(
            detail["request"]["headers"]["content-type"],
            "application/json"
        );
        assert_eq!(detail["response"]["headers"]["server"], "cloudflare");
        assert!(detail["request"].get("body").is_none());
        assert!(detail["response"].get("body").is_none());
        assert!(!stored.contains("sk-secret"));
        assert!(!stored.contains("sk-request"));
        assert_eq!(page.records[0].summary, "search me");
    }
}
