//! DeepSeek native Responses compatibility.
//!
//! DeepSeek `/v1/responses` accepts `{"type":"custom"}` only when
//! `name` is `apply_patch`. Codex Desktop also sends `exec` (and may send
//! other named custom tools), which the gateway rejects with HTTP 400
//! `Unsupported custom tool: 'exec'. Only 'apply_patch' is supported.`
//!
//! Policy (same three-layer shape as xAI, different keep-list):
//! 1. Rewrite named custom tools except `apply_patch` into functions.
//! 2. Drop only leftover illegal custom tools (unnamed).
//! 3. Hint the upstream request copy when something had to be dropped.
//!
//! Shared rewrite/restore lives in `compat_custom`. xAI keeps no native
//! custom tools; DeepSeek keeps native `apply_patch`.

use std::collections::HashSet;

use bytes::Bytes;
use serde_json::{json, Value};

use crate::compat_custom::{
    custom_tool_names_from_request, restore_custom_tool_calls, rewrite_custom_as_function,
    rewrite_custom_input_items,
};
const APPLY_PATCH: &str = "apply_patch";
const PROVIDER_COMPAT_HINT_PREFIX: &str = "[codex-helper:provider-compat]";

#[derive(Debug, Clone, Default)]
pub struct DeepSeekRestoreMap {
    pub custom_tool_names: HashSet<String>,
}

fn keep_native() -> HashSet<String> {
    let mut names = HashSet::new();
    names.insert(APPLY_PATCH.to_string());
    names
}

pub fn apply_deepseek_responses_request_compat(body: &mut Value) -> DeepSeekRestoreMap {
    let custom_tool_names = custom_tool_names_from_request(body, &keep_native());
    sanitize_deepseek_responses_request(body);
    DeepSeekRestoreMap { custom_tool_names }
}

/// Rewrite DeepSeek-unsupported custom tools in place. Returns whether
/// anything changed.
pub fn sanitize_deepseek_responses_request(body: &mut Value) -> bool {
    if !body.is_object() {
        return false;
    }
    let keep = keep_native();
    let mut changed = rewrite_custom_as_function(body, &keep);
    changed |= rewrite_custom_input_items(body, &keep);
    changed |= filter_remaining_custom_tools(body);
    changed
}

fn json_type(value: &Value) -> Option<&str> {
    value.get("type").and_then(Value::as_str).map(str::trim)
}

fn tool_name(tool: &Value) -> &str {
    tool.get("name")
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or("")
}

fn keep_deepseek_tool(tool: &Value) -> bool {
    if json_type(tool) != Some("custom") {
        return true;
    }
    tool_name(tool) == APPLY_PATCH
}

fn tool_hint_label(tool: &Value) -> Option<String> {
    let kind = json_type(tool)?;
    let name = tool_name(tool);
    Some(if name.is_empty() {
        kind.to_string()
    } else {
        format!("{kind}:{name}")
    })
}

fn filter_remaining_custom_tools(body: &mut Value) -> bool {
    let Some(tools) = body.get("tools").and_then(Value::as_array).cloned() else {
        return drop_tool_choice_if_needed(body, &[]);
    };
    let original_len = tools.len();
    let mut dropped_types: Vec<String> = Vec::new();
    let filtered: Vec<Value> = tools
        .into_iter()
        .filter(|tool| {
            if keep_deepseek_tool(tool) {
                true
            } else {
                let kind = json_type(tool).unwrap_or("unknown");
                if !dropped_types
                    .iter()
                    .any(|existing| existing.eq_ignore_ascii_case(kind))
                {
                    dropped_types.push(kind.to_string());
                }
                false
            }
        })
        .collect();
    let remaining_labels: Vec<String> = filtered
        .iter()
        .filter_map(tool_hint_label)
        .take(12)
        .collect();
    let mut changed = false;
    if filtered.len() != original_len {
        if let Some(obj) = body.as_object_mut() {
            if filtered.is_empty() {
                obj.remove("tools");
            } else {
                obj.insert("tools".to_string(), Value::Array(filtered.clone()));
            }
        }
        changed = true;
    }
    let dropped_tool_choice = drop_tool_choice_if_needed(body, &filtered);
    changed |= dropped_tool_choice;
    changed |=
        inject_provider_compat_hint(body, &dropped_types, &remaining_labels, dropped_tool_choice);
    changed
}

fn drop_tool_choice_if_needed(body: &mut Value, tools: &[Value]) -> bool {
    if body.get("tool_choice").is_some() && should_drop_tool_choice(body, tools) {
        if let Some(obj) = body.as_object_mut() {
            obj.remove("tool_choice");
        }
        return true;
    }
    false
}

fn should_drop_tool_choice(body: &Value, tools: &[Value]) -> bool {
    let Some(tool_choice) = body.get("tool_choice") else {
        return false;
    };
    if tools.is_empty() {
        return true;
    }
    match tool_choice {
        Value::String(choice) => {
            let choice = choice.trim();
            choice.eq_ignore_ascii_case("required") && tools.is_empty()
        }
        Value::Object(choice) => {
            let choice_type = choice
                .get("type")
                .and_then(Value::as_str)
                .unwrap_or("")
                .trim();
            if choice_type == "function" {
                let name = choice
                    .get("name")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .unwrap_or("");
                !tools
                    .iter()
                    .any(|tool| json_type(tool) == Some("function") && tool_name(tool) == name)
            } else if choice_type == "custom" {
                let name = choice
                    .get("name")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .unwrap_or("");
                !tools
                    .iter()
                    .any(|tool| json_type(tool) == Some("custom") && tool_name(tool) == name)
            } else {
                false
            }
        }
        _ => false,
    }
}

fn provider_compat_hint_text(dropped_types: &[String], remaining_labels: &[String]) -> String {
    let mut text = String::from(PROVIDER_COMPAT_HINT_PREFIX);
    text.push(' ');
    if dropped_types.is_empty() {
        text.push_str("No tools are available on this request.");
    } else {
        text.push_str("This provider does not support Codex-only tools: ");
        text.push_str(&dropped_types.join(", "));
        text.push_str(". Do not call them.");
    }
    if remaining_labels.is_empty() {
        text.push_str(" Answer in text only; do not emit tool calls.");
    } else {
        text.push_str(" Continue with available tools: ");
        text.push_str(&remaining_labels.join(", "));
        text.push('.');
    }
    text
}

fn is_provider_compat_hint(item: &Value) -> bool {
    if json_type(item) != Some("message") {
        return false;
    }
    match item.get("content") {
        Some(Value::Array(parts)) => parts.iter().any(|part| {
            part.get("text")
                .and_then(Value::as_str)
                .is_some_and(|text| text.starts_with(PROVIDER_COMPAT_HINT_PREFIX))
        }),
        Some(Value::String(text)) => text.starts_with(PROVIDER_COMPAT_HINT_PREFIX),
        _ => false,
    }
}

fn inject_provider_compat_hint(
    body: &mut Value,
    dropped_types: &[String],
    remaining_labels: &[String],
    dropped_tool_choice: bool,
) -> bool {
    if dropped_types.is_empty() && !dropped_tool_choice {
        return false;
    }
    let item = json!({
        "type": "message",
        "role": "user",
        "content": [{
            "type": "input_text",
            "text": provider_compat_hint_text(dropped_types, remaining_labels)
        }]
    });
    match body.get_mut("input") {
        Some(Value::Array(items)) => {
            if let Some(existing) = items.iter_mut().find(|item| is_provider_compat_hint(item)) {
                if *existing == item {
                    return false;
                }
                *existing = item;
                return true;
            }
            items.push(item);
            true
        }
        Some(_) => false,
        None => {
            if let Some(obj) = body.as_object_mut() {
                obj.insert("input".to_string(), json!([item]));
                true
            } else {
                false
            }
        }
    }
}

fn strip_sse_field<'a>(line: &'a str, field: &str) -> Option<&'a str> {
    line.strip_prefix(&format!("{field}: "))
        .or_else(|| line.strip_prefix(&format!("{field}:")))
}

pub fn rewrite_deepseek_native_json_bytes(
    bytes: &[u8],
    restore_map: &DeepSeekRestoreMap,
) -> Vec<u8> {
    let Ok(mut value) = serde_json::from_slice::<Value>(bytes) else {
        return bytes.to_vec();
    };
    if !restore_custom_tool_calls(&mut value, &restore_map.custom_tool_names) {
        return bytes.to_vec();
    }
    serde_json::to_vec(&value).unwrap_or_else(|_| bytes.to_vec())
}

pub fn rewrite_deepseek_native_sse_block(block: &str, restore_map: &DeepSeekRestoreMap) -> Bytes {
    let mut event_name: Option<&str> = None;
    let mut data_parts: Vec<&str> = Vec::new();
    for line in block.lines() {
        if let Some(event) = strip_sse_field(line, "event") {
            event_name = Some(event.trim());
        }
        if let Some(data) = strip_sse_field(line, "data") {
            data_parts.push(data);
        }
    }
    if data_parts.is_empty() {
        return Bytes::from(format!("{block}\n\n"));
    }
    let data = data_parts.join("\n");
    if data.trim() == "[DONE]" {
        return Bytes::from(format!("{block}\n\n"));
    }
    let mut event: Value = match serde_json::from_str(&data) {
        Ok(value) => value,
        Err(_) => return Bytes::from(format!("{block}\n\n")),
    };
    if !restore_custom_tool_calls(&mut event, &restore_map.custom_tool_names) {
        return Bytes::from(format!("{block}\n\n"));
    }
    let restored = serde_json::to_string(&event).unwrap_or(data);
    let mut out = String::new();
    if let Some(name) = event_name {
        out.push_str("event: ");
        out.push_str(name);
        out.push('\n');
    }
    out.push_str("data: ");
    out.push_str(&restored);
    out.push_str("\n\n");
    Bytes::from(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};

    fn tool_names(body: &Value) -> Vec<&str> {
        body["tools"]
            .as_array()
            .map(|tools| {
                tools
                    .iter()
                    .map(|tool| tool.get("name").and_then(Value::as_str).unwrap_or(""))
                    .collect()
            })
            .unwrap_or_default()
    }

    fn tool_type(body: &Value, name: &str) -> Option<String> {
        body["tools"].as_array().and_then(|tools| {
            tools.iter().find_map(|tool| {
                (tool.get("name").and_then(Value::as_str) == Some(name)).then(|| {
                    tool.get("type")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_string()
                })
            })
        })
    }

    #[test]
    fn deepseek_rewrites_exec_custom_tool_keeps_apply_patch() {
        let mut body = json!({
            "model": "deepseek-v4-flash",
            "tools": [
                { "type": "function", "name": "read_file", "parameters": {} },
                { "type": "custom", "name": "exec" },
                { "type": "custom", "name": "apply_patch" },
                { "type": "web_search" }
            ]
        });
        let restore = apply_deepseek_responses_request_compat(&mut body);
        assert!(restore.custom_tool_names.contains("exec"));
        assert!(!restore.custom_tool_names.contains("apply_patch"));
        let names = tool_names(&body);
        assert!(
            names.contains(&"exec"),
            "exec must be rewritten, got: {names:?}"
        );
        assert_eq!(tool_type(&body, "exec").as_deref(), Some("function"));
        assert_eq!(tool_type(&body, "apply_patch").as_deref(), Some("custom"));
        assert_eq!(body["tools"].as_array().unwrap().len(), 4);
        assert!(body.get("input").is_none());
    }

    #[test]
    fn deepseek_noop_when_only_apply_patch_present() {
        let mut body = json!({
            "model": "deepseek-v4-flash",
            "tools": [
                { "type": "custom", "name": "apply_patch" },
                { "type": "function", "name": "run" }
            ]
        });
        assert!(!sanitize_deepseek_responses_request(&mut body));
        assert_eq!(body["tools"].as_array().unwrap().len(), 2);
        assert_eq!(tool_type(&body, "apply_patch").as_deref(), Some("custom"));
    }

    #[test]
    fn deepseek_rewrites_exec_when_it_is_the_only_tool() {
        let mut body = json!({
            "model": "deepseek-v4-flash",
            "tools": [{ "type": "custom", "name": "exec" }],
            "tool_choice": { "type": "custom", "name": "exec" }
        });
        assert!(sanitize_deepseek_responses_request(&mut body));
        assert_eq!(tool_type(&body, "exec").as_deref(), Some("function"));
        assert_eq!(
            body["tool_choice"],
            json!({ "type": "function", "name": "exec" })
        );
        assert!(body.get("input").is_none());
    }

    #[test]
    fn deepseek_rewrites_exec_history_keeps_apply_patch_history() {
        let mut body = json!({
            "tools": [
                { "type": "custom", "name": "exec" },
                { "type": "custom", "name": "apply_patch" }
            ],
            "input": [
                {
                    "type": "custom_tool_call",
                    "call_id": "call_exec",
                    "name": "exec",
                    "input": "pwd"
                },
                {
                    "type": "custom_tool_call_output",
                    "call_id": "call_exec",
                    "output": "/"
                },
                {
                    "type": "custom_tool_call",
                    "call_id": "call_patch",
                    "name": "apply_patch",
                    "input": "patch"
                },
                {
                    "type": "custom_tool_call_output",
                    "call_id": "call_patch",
                    "output": "ok"
                }
            ]
        });
        apply_deepseek_responses_request_compat(&mut body);
        assert_eq!(body["input"][0]["type"], "function_call");
        assert_eq!(body["input"][0]["name"], "exec");
        assert_eq!(body["input"][1]["type"], "function_call_output");
        assert_eq!(body["input"][2]["type"], "custom_tool_call");
        assert_eq!(body["input"][2]["name"], "apply_patch");
        assert_eq!(body["input"][3]["type"], "custom_tool_call_output");
    }

    #[test]
    fn deepseek_restores_function_call_named_exec() {
        let mut restore = DeepSeekRestoreMap::default();
        restore.custom_tool_names.insert("exec".to_string());
        let value = json!({
            "output": [{
                "type": "function_call",
                "name": "exec",
                "call_id": "call_exec",
                "arguments": "{\"input\":\"ls\"}"
            }]
        });
        let rewritten =
            rewrite_deepseek_native_json_bytes(&serde_json::to_vec(&value).unwrap(), &restore);
        let value: Value = serde_json::from_slice(&rewritten).unwrap();
        assert_eq!(value["output"][0]["type"], "custom_tool_call");
        assert_eq!(value["output"][0]["name"], "exec");
        assert_eq!(value["output"][0]["input"], "ls");
        assert!(value["output"][0].get("arguments").is_none());
    }

    #[test]
    fn deepseek_sse_restores_exec_function_call() {
        let mut restore = DeepSeekRestoreMap::default();
        restore.custom_tool_names.insert("exec".to_string());
        let block = concat!(
            "event: response.output_item.done\n",
            r#"data: {"type":"function_call","name":"exec","call_id":"call_1","arguments":"{\"input\":\"ls\"}"}"#,
            "\n\n"
        );
        let rewritten = rewrite_deepseek_native_sse_block(block, &restore);
        let text = String::from_utf8(rewritten.to_vec()).unwrap();
        let data = text
            .lines()
            .find_map(|line| line.strip_prefix("data: "))
            .expect("sse data");
        let event: Value = serde_json::from_str(data).unwrap();
        assert_eq!(event["type"], "custom_tool_call");
        assert_eq!(event["name"], "exec");
        assert_eq!(event["input"], "ls");
    }

    #[test]
    fn deepseek_idempotent_second_pass() {
        let mut body = json!({
            "tools": [
                { "type": "custom", "name": "exec" },
                { "type": "custom", "name": "apply_patch" }
            ]
        });
        assert!(sanitize_deepseek_responses_request(&mut body));
        assert!(!sanitize_deepseek_responses_request(&mut body));
        assert_eq!(tool_type(&body, "exec").as_deref(), Some("function"));
        assert_eq!(tool_type(&body, "apply_patch").as_deref(), Some("custom"));
    }
}
