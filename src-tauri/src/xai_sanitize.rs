//! xAI / Grok native Responses compatibility.
//!
//! Port of local CC Switch `fix/xai-codex-6815`
//! (`transform_codex_responses_xai_sanitize.rs` + namespace flatten/restore).
//! Request-side: strip private fields, flatten `namespace` tools, rewrite
//! `tool_search` and `custom` tools into functions (including history
//! `tool_search_call` / `custom_tool_call` items), rewrite `agent_message`,
//! collapse mixed root `oneOf`/`anyOf` schemas (shallow `$ref`/`$defs`,
//! drop non-object branches, lift a single object). Return-path: stringify
//! object `function_call.arguments`, rewrite whole-float tool arguments so
//! Codex Desktop serde accepts Grok `92116.0` as `i32`/`u64`, then restore
//! flattened names and `tool_search_call` / `custom_tool_call`.

use std::collections::{HashMap, HashSet};

use bytes::Bytes;
use serde_json::{json, Map, Number, Value};
use sha2::{Digest, Sha256};

use crate::compat_custom;
use crate::providers::rewrite_unmatched_request_model;

const CHAT_TOOL_NAME_MAX_LEN: usize = 64;
const PROVIDER_COMPAT_HINT_PREFIX: &str = "[codex-helper:provider-compat]";
const TOOL_SEARCH_NAME: &str = "tool_search";
const RECURSIVE_UNSUPPORTED_FIELDS: &[&str] = &["external_web_access"];
const TOP_LEVEL_UNSUPPORTED_FIELDS: &[&str] = &["prompt_cache_retention", "safety_identifier"];
const GROK_45_UNSUPPORTED_FIELDS: &[&str] = &[
    "presence_penalty",
    "presencePenalty",
    "frequency_penalty",
    "frequencyPenalty",
    "stop",
];
const XAI_SUPPORTED_TOOL_TYPES: &[&str] = &[
    "function",
    "web_search",
    "x_search",
    "image_generation",
    "collections_search",
    "file_search",
    "code_execution",
    "code_interpreter",
    "mcp",
    "shell",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NamespacedName {
    pub namespace: String,
    pub name: String,
}

#[derive(Debug, Clone, Default)]
pub struct XaiNativeRestoreMap {
    pub namespaces: HashMap<String, NamespacedName>,
    pub custom_tool_names: HashSet<String>,
}

pub fn apply_xai_native_responses_request_compat(
    body: &mut Value,
    upstream_model: Option<&str>,
    allowed_models: &HashSet<String>,
) -> XaiNativeRestoreMap {
    promote_tool_search_output_tools(body);
    let namespaces = namespace_restore_map(body);
    let custom_tool_names = compat_custom::custom_tool_names_from_request(body, &HashSet::new());
    // Decide the live model first so grok-4.5 field strips see the rewritten SKU.
    if let Some(upstream_model) = upstream_model {
        rewrite_xai_unknown_request_model(body, upstream_model, allowed_models);
    }
    sanitize_xai_responses_request(body);
    rewrite_xai_agent_message_input_items(body);
    XaiNativeRestoreMap {
        namespaces,
        custom_tool_names,
    }
}

pub fn sanitize_xai_responses_request(body: &mut Value) -> bool {
    if !body.is_object() {
        return false;
    }
    let mut changed = false;
    for field in TOP_LEVEL_UNSUPPORTED_FIELDS {
        changed |= remove_top_level_field(body, field);
    }
    if request_targets_grok_45(body) {
        for field in GROK_45_UNSUPPORTED_FIELDS {
            changed |= remove_top_level_field(body, field);
        }
    }
    for field in RECURSIVE_UNSUPPORTED_FIELDS {
        changed |= remove_field_recursive(body, field);
    }
    changed |= promote_additional_tools(body);
    changed |= promote_tool_search_output_tools(body);
    changed |= flatten_request_namespaces(body);
    changed |= rewrite_tool_search_as_function(body);
    changed |= rewrite_tool_search_input_items(body);
    changed |= compat_custom::rewrite_custom_as_function(body, &HashSet::new());
    changed |= compat_custom::rewrite_custom_input_items(body, &HashSet::new());
    changed |= strip_null_reasoning_content(body);
    changed |= filter_unsupported_tools(body);
    changed |= normalize_xai_function_tool_parameter_schemas(body);
    changed
}

pub fn rewrite_xai_agent_message_input_items(body: &mut Value) -> bool {
    rewrite_agent_message_value(body)
}

pub fn rewrite_xai_unknown_request_model(
    body: &mut Value,
    upstream_model: &str,
    allowed_models: &HashSet<String>,
) -> Option<(String, String)> {
    let request = body
        .get("model")
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or("")
        .to_string();
    if !request.is_empty() && request_model_looks_like_grok(&request) {
        return None;
    }
    if !rewrite_unmatched_request_model(body, upstream_model, allowed_models) {
        return None;
    }
    Some((request, upstream_model.trim().to_string()))
}

fn request_model_looks_like_grok(model: &str) -> bool {
    let mut slug = model.trim();
    if let Some(idx) = slug.rfind('/') {
        slug = slug[idx + 1..].trim();
    }
    let slug = slug.to_ascii_lowercase();
    slug == "grok" || slug.starts_with("grok-")
}

fn request_targets_grok_45(body: &Value) -> bool {
    let Some(model) = body.get("model").and_then(Value::as_str) else {
        return false;
    };
    let mut model = model.trim();
    if let Some(idx) = model.rfind('/') {
        model = model[idx + 1..].trim();
    }
    let model = model.to_ascii_lowercase();
    model == "grok-4.5" || model.starts_with("grok-4.5") || model.contains("grok-4-5")
}

fn remove_top_level_field(body: &mut Value, field: &str) -> bool {
    body.as_object_mut()
        .is_some_and(|object| object.remove(field).is_some())
}

fn remove_field_recursive(value: &mut Value, field: &str) -> bool {
    let mut changed = false;
    match value {
        Value::Object(object) => {
            changed |= object.remove(field).is_some();
            for child in object.values_mut() {
                changed |= remove_field_recursive(child, field);
            }
        }
        Value::Array(items) => {
            for item in items {
                changed |= remove_field_recursive(item, field);
            }
        }
        _ => {}
    }
    changed
}

fn json_type(value: &Value) -> Option<&str> {
    value.get("type").and_then(Value::as_str).map(str::trim)
}

fn is_xai_supported_tool_type(kind: &str) -> bool {
    XAI_SUPPORTED_TOOL_TYPES
        .iter()
        .any(|supported| supported.eq_ignore_ascii_case(kind))
}

fn xai_tool_search_function() -> Value {
    json!({
        "type": "function",
        "name": TOOL_SEARCH_NAME,
        "description": "Search and load Codex tools, plugins, connectors, and MCP namespaces for the current task.",
        "parameters": {
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "Search query for tools or connectors to load."
                },
                "limit": {
                    "type": "integer",
                    "description": "Maximum number of tool groups to return."
                }
            },
            "required": ["query"]
        }
    })
}

fn function_tool_name(tool: &Value) -> Option<&str> {
    tool.get("name")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .or_else(|| {
            tool.get("function")
                .and_then(|function| function.get("name"))
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|name| !name.is_empty())
        })
}

fn rewrite_tool_search_tool_choice(body: &mut Value) -> bool {
    let Some(choice) = body.get_mut("tool_choice") else {
        return false;
    };
    if json_type(choice) != Some(TOOL_SEARCH_NAME) {
        return false;
    }
    *choice = json!({ "type": "function", "name": TOOL_SEARCH_NAME });
    true
}

fn rewrite_tool_search_as_function(body: &mut Value) -> bool {
    let mut changed = false;
    if let Some(tools) = body.get("tools").and_then(Value::as_array).cloned() {
        let already_has_function = tools.iter().any(|tool| {
            json_type(tool) == Some("function")
                && function_tool_name(tool) == Some(TOOL_SEARCH_NAME)
        });
        let mut next = Vec::with_capacity(tools.len());
        let mut saw_type = false;
        for tool in tools {
            if json_type(&tool) == Some(TOOL_SEARCH_NAME) {
                saw_type = true;
                continue;
            }
            next.push(tool);
        }
        if saw_type {
            if !already_has_function {
                next.push(xai_tool_search_function());
            }
            if let Some(obj) = body.as_object_mut() {
                obj.insert("tools".to_string(), Value::Array(next));
            }
            changed = true;
        }
    }
    changed |= rewrite_tool_search_tool_choice(body);
    changed
}

fn json_compact_string(value: &Value) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "{}".to_string())
}

fn parse_tool_search_arguments_object(arguments: &str) -> Value {
    let trimmed = arguments.trim();
    if trimmed.is_empty() {
        return json!({});
    }
    serde_json::from_str::<Value>(trimmed)
        .ok()
        .filter(Value::is_object)
        .unwrap_or_else(|| json!({ "query": trimmed }))
}

fn function_call_arguments_string(arguments: Option<&Value>) -> Value {
    match arguments {
        Some(Value::String(text)) => Value::String(text.clone()),
        Some(value) => Value::String(json_compact_string(value)),
        None => Value::String("{}".to_string()),
    }
}

fn rewrite_tool_search_call_input_item(item: &mut Value) -> bool {
    let Some(obj) = item.as_object_mut() else {
        return false;
    };
    if obj.get("type").and_then(Value::as_str).map(str::trim) != Some("tool_search_call") {
        return false;
    }
    let call_id = obj
        .get("call_id")
        .cloned()
        .or_else(|| obj.get("id").cloned());
    let id = obj.get("id").cloned();
    let status = obj.get("status").cloned();
    let arguments = function_call_arguments_string(obj.get("arguments"));
    let mut next = Map::new();
    next.insert("type".to_string(), json!("function_call"));
    next.insert("name".to_string(), json!(TOOL_SEARCH_NAME));
    next.insert("arguments".to_string(), arguments);
    if let Some(call_id) = call_id {
        next.insert("call_id".to_string(), call_id);
    }
    if let Some(id) = id {
        next.insert("id".to_string(), id);
    }
    if let Some(status) = status {
        next.insert("status".to_string(), status);
    }
    *obj = next;
    true
}

fn rewrite_tool_search_output_input_item(item: &mut Value) -> bool {
    let Some(obj) = item.as_object_mut() else {
        return false;
    };
    if obj.get("type").and_then(Value::as_str).map(str::trim) != Some("tool_search_output") {
        return false;
    }
    let call_id = obj
        .get("call_id")
        .cloned()
        .or_else(|| obj.get("id").cloned());
    let output = match obj.get("output") {
        Some(Value::String(text)) => Value::String(text.clone()),
        Some(value) => Value::String(json_compact_string(value)),
        None => match obj.get("tools") {
            Some(tools) => Value::String(json_compact_string(&json!({ "tools": tools }))),
            None => Value::String("{}".to_string()),
        },
    };
    let mut next = Map::new();
    next.insert("type".to_string(), json!("function_call_output"));
    if let Some(call_id) = call_id {
        next.insert("call_id".to_string(), call_id);
    }
    next.insert("output".to_string(), output);
    *obj = next;
    true
}

fn rewrite_tool_search_input_value(value: &mut Value) -> bool {
    if rewrite_tool_search_call_input_item(value) || rewrite_tool_search_output_input_item(value) {
        return true;
    }
    let mut changed = false;
    match value {
        Value::Array(items) => {
            for item in items {
                changed |= rewrite_tool_search_input_value(item);
            }
        }
        Value::Object(obj) => {
            for child in obj.values_mut() {
                changed |= rewrite_tool_search_input_value(child);
            }
        }
        _ => {}
    }
    changed
}

fn rewrite_tool_search_input_items(body: &mut Value) -> bool {
    match body.get_mut("input") {
        Some(input) => rewrite_tool_search_input_value(input),
        None => false,
    }
}

fn collect_tool_search_output_tools(value: &Value, extra: &mut Vec<Value>) {
    match value {
        Value::Array(items) => {
            for item in items {
                collect_tool_search_output_tools(item, extra);
            }
        }
        Value::Object(obj) => {
            if obj.get("type").and_then(Value::as_str).map(str::trim) == Some("tool_search_output")
            {
                if let Some(tools) = obj.get("tools").and_then(Value::as_array) {
                    extra.extend(tools.iter().cloned());
                }
            }
            for child in obj.values() {
                collect_tool_search_output_tools(child, extra);
            }
        }
        _ => {}
    }
}

fn promote_tool_search_output_tools(body: &mut Value) -> bool {
    let mut extra = Vec::new();
    if let Some(input) = body.get("input") {
        collect_tool_search_output_tools(input, &mut extra);
    }
    if extra.is_empty() {
        return false;
    }
    let mut merged = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    if let Some(tools) = body.get("tools").and_then(Value::as_array) {
        for tool in tools {
            seen.insert(tool_dedup_key(tool));
            merged.push(tool.clone());
        }
    }
    let mut added = false;
    for tool in extra {
        if seen.insert(tool_dedup_key(&tool)) {
            merged.push(tool);
            added = true;
        }
    }
    if !added {
        return false;
    }
    if let Some(obj) = body.as_object_mut() {
        obj.insert("tools".to_string(), Value::Array(merged));
        true
    } else {
        false
    }
}

fn restore_tool_search_call_item(item: &mut Value) -> bool {
    let Some(obj) = item.as_object_mut() else {
        return false;
    };
    if obj.get("type").and_then(Value::as_str) != Some("function_call") {
        return false;
    }
    let name = obj
        .get("name")
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or("");
    if name != TOOL_SEARCH_NAME {
        return false;
    }
    let call_id = obj
        .get("call_id")
        .cloned()
        .or_else(|| obj.get("id").cloned());
    let id = obj.get("id").cloned();
    let status = obj.get("status").cloned();
    obj.insert("type".to_string(), json!("tool_search_call"));
    obj.insert("execution".to_string(), json!("client"));
    obj.remove("name");
    if let Some(call_id) = call_id {
        obj.insert("call_id".to_string(), call_id);
    }
    if let Some(id) = id {
        obj.insert("id".to_string(), id);
    }
    if let Some(status) = status {
        obj.insert("status".to_string(), status);
    }
    match obj.get("arguments").cloned() {
        Some(Value::String(raw)) => {
            obj.insert(
                "arguments".to_string(),
                parse_tool_search_arguments_object(&raw),
            );
        }
        Some(Value::Object(_)) => {}
        Some(_) | None => {
            obj.insert("arguments".to_string(), json!({}));
        }
    }
    true
}

fn restore_tool_search_calls(value: &mut Value) -> bool {
    let mut changed = restore_tool_search_call_item(value);
    match value {
        Value::Array(items) => {
            for item in items {
                changed |= restore_tool_search_calls(item);
            }
        }
        Value::Object(obj) => {
            for child in obj.values_mut() {
                changed |= restore_tool_search_calls(child);
            }
        }
        _ => {}
    }
    changed
}

/// Keep only whitelisted tool types and drop a `tool_choice` that now points at
/// a removed or unsupported tool. An empty `tools` array is removed so xAI does
/// not see `tool_choice` with "no tools specified". When anything was dropped,
/// append a request-only hint so the model can continue without retrying the
/// unsupported tools.
fn filter_unsupported_tools(body: &mut Value) -> bool {
    let original_tools = body.get("tools").and_then(Value::as_array).cloned();
    let original_len = original_tools.as_ref().map(Vec::len);
    let mut dropped_types: Vec<String> = Vec::new();
    let filtered: Vec<Value> = original_tools
        .as_ref()
        .map(|tools| {
            let mut kept = Vec::new();
            for tool in tools {
                match json_type(tool) {
                    Some(kind) if is_xai_supported_tool_type(kind) => kept.push(tool.clone()),
                    Some(kind) => push_unique_type(&mut dropped_types, kind),
                    None => push_unique_type(&mut dropped_types, "unknown"),
                }
            }
            kept
        })
        .unwrap_or_default();
    let remaining_labels: Vec<String> = filtered
        .iter()
        .filter_map(tool_hint_label)
        .take(12)
        .collect();

    let mut changed = false;
    if let Some(len) = original_len {
        if filtered.len() != len || len == 0 {
            if let Some(obj) = body.as_object_mut() {
                if filtered.is_empty() {
                    obj.remove("tools");
                } else {
                    obj.insert("tools".to_string(), Value::Array(filtered.clone()));
                }
            }
            changed = true;
        }
    }

    let mut dropped_tool_choice = false;
    if body.get("tool_choice").is_some() && should_drop_tool_choice(body, &filtered) {
        if let Some(obj) = body.as_object_mut() {
            obj.remove("tool_choice");
        }
        dropped_tool_choice = true;
        changed = true;
    }
    changed |=
        inject_provider_compat_hint(body, &dropped_types, &remaining_labels, dropped_tool_choice);
    changed
}

fn push_unique_type(types: &mut Vec<String>, kind: &str) {
    if !types
        .iter()
        .any(|existing| existing.eq_ignore_ascii_case(kind))
    {
        types.push(kind.to_string());
    }
}

fn tool_hint_label(tool: &Value) -> Option<String> {
    let kind = json_type(tool)?;
    let name = tool
        .get("name")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .or_else(|| {
            tool.get("function")
                .and_then(|function| function.get("name"))
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|name| !name.is_empty())
        });
    Some(match name {
        Some(name) => format!("{kind}:{name}"),
        None => kind.to_string(),
    })
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

fn provider_compat_hint_item(text: String) -> Value {
    json!({
        "type": "message",
        "role": "user",
        "content": [{ "type": "input_text", "text": text }]
    })
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
    let text = provider_compat_hint_text(dropped_types, remaining_labels);
    let item = provider_compat_hint_item(text.clone());
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

/// Whether `tool_choice` should be dropped given the surviving `tools`. String
/// choices (`"auto"`, `"none"`, `"required"`) stay when tools remain; they are
/// dropped when no tools survive, matching xAI
/// `A tool_choice was set on the request but no tools were specified.`
fn should_drop_tool_choice(body: &Value, tools: &[Value]) -> bool {
    let Some(tool_choice) = body.get("tool_choice") else {
        return false;
    };
    if tools.is_empty() {
        return true;
    }
    let Some(choice) = tool_choice.as_object() else {
        return false;
    };
    let choice_type = choice
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim();
    if choice_type.is_empty() {
        return false;
    }
    if !is_xai_supported_tool_type(choice_type) {
        return true;
    }
    if !choice_type.eq_ignore_ascii_case("function") {
        return false;
    }
    let choice_name = choice
        .get("name")
        .and_then(Value::as_str)
        .or_else(|| {
            choice
                .get("function")
                .and_then(|function| function.get("name"))
                .and_then(Value::as_str)
        })
        .unwrap_or("")
        .trim();
    if choice_name.is_empty() {
        return false;
    }
    !tools.iter().any(|tool| {
        json_type(tool) == Some("function")
            && tool
                .get("name")
                .and_then(Value::as_str)
                .or_else(|| {
                    tool.get("function")
                        .and_then(|function| function.get("name"))
                        .and_then(Value::as_str)
                })
                .map(str::trim)
                == Some(choice_name)
    })
}

fn is_additional_tools_item(item: &Value) -> bool {
    item.get("type").and_then(Value::as_str).map(str::trim) == Some("additional_tools")
}

fn tool_dedup_key(tool: &Value) -> String {
    let tool_type = tool
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim();
    if !tool_type.is_empty() {
        if let Some(name) = tool.get("name").and_then(Value::as_str) {
            let name = name.trim();
            if !name.is_empty() {
                return format!("type:{tool_type}\u{0}name:{name}");
            }
        }
        if tool_type == "mcp" {
            if let Some(label) = tool.get("server_label").and_then(Value::as_str) {
                let label = label.trim();
                if !label.is_empty() {
                    return format!("type:mcp\u{0}server_label:{label}");
                }
            }
        }
    }
    format!("json:{tool}")
}

fn promote_additional_tools(body: &mut Value) -> bool {
    let input_items: Vec<Value> = match body.get("input").and_then(Value::as_array) {
        Some(arr) if arr.iter().any(is_additional_tools_item) => arr.clone(),
        _ => return false,
    };
    let mut merged: Vec<Value> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    if let Some(tools) = body.get("tools").and_then(Value::as_array) {
        for tool in tools {
            seen.insert(tool_dedup_key(tool));
            merged.push(tool.clone());
        }
    }
    let mut filtered_input: Vec<Value> = Vec::with_capacity(input_items.len());
    let mut promoted = false;
    for item in input_items {
        if is_additional_tools_item(&item) {
            if let Some(carrier_tools) = item.get("tools").and_then(Value::as_array) {
                for tool in carrier_tools {
                    if seen.insert(tool_dedup_key(tool)) {
                        merged.push(tool.clone());
                        promoted = true;
                    }
                }
            }
            continue;
        }
        filtered_input.push(item);
    }
    if let Some(obj) = body.as_object_mut() {
        obj.insert("input".to_string(), Value::Array(filtered_input));
        if promoted {
            obj.insert("tools".to_string(), Value::Array(merged));
        }
    }
    true
}

fn strip_null_reasoning_content(body: &mut Value) -> bool {
    let Some(input) = body.get_mut("input").and_then(Value::as_array_mut) else {
        return false;
    };
    let mut changed = false;
    for item in input.iter_mut() {
        if item.get("type").and_then(Value::as_str).map(str::trim) != Some("reasoning") {
            continue;
        }
        if let Some(obj) = item.as_object_mut() {
            if matches!(obj.get("content"), Some(Value::Null)) {
                obj.remove("content");
                changed = true;
            }
        }
    }
    changed
}

fn namespace_children(tool: &Value) -> Vec<Value> {
    tool.get("tools")
        .or_else(|| tool.get("children"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
}

fn short_sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest
        .iter()
        .take(8)
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn flatten_namespace_tool_name(namespace: &str, name: &str) -> String {
    let full_name = format!("{namespace}__{name}");
    if full_name.len() <= CHAT_TOOL_NAME_MAX_LEN {
        return full_name;
    }
    let hash = short_sha256_hex(full_name.as_bytes());
    let suffix = format!("__{hash}");
    let prefix_len = CHAT_TOOL_NAME_MAX_LEN.saturating_sub(suffix.len());
    let mut prefix = String::new();
    for ch in full_name.chars() {
        if prefix.len() + ch.len_utf8() > prefix_len {
            break;
        }
        prefix.push(ch);
    }
    format!("{prefix}{suffix}")
}

pub fn namespace_restore_map(request_body: &Value) -> HashMap<String, NamespacedName> {
    let mut map = HashMap::new();
    let Some(tools) = request_body.get("tools").and_then(Value::as_array) else {
        return map;
    };
    for tool in tools {
        if json_type(tool) != Some("namespace") {
            continue;
        }
        let Some(namespace) = tool.get("name").and_then(Value::as_str).map(str::trim) else {
            continue;
        };
        if namespace.is_empty() {
            continue;
        }
        for child in namespace_children(tool) {
            if json_type(&child) != Some("function") {
                continue;
            }
            let Some(name) = child.get("name").and_then(Value::as_str).map(str::trim) else {
                continue;
            };
            if name.is_empty() {
                continue;
            }
            let flat = flatten_namespace_tool_name(namespace, name);
            map.entry(flat).or_insert_with(|| NamespacedName {
                namespace: namespace.to_string(),
                name: name.to_string(),
            });
        }
    }
    map
}

fn flatten_request_namespaces(body: &mut Value) -> bool {
    let Some(tools) = body.get("tools").and_then(Value::as_array) else {
        return false;
    };
    if !tools
        .iter()
        .any(|tool| json_type(tool) == Some("namespace"))
    {
        return false;
    }

    let owners = namespace_restore_map(body);
    let tools = tools.clone();
    let mut flattened: Vec<Value> = Vec::with_capacity(tools.len());
    let mut seen_flat = HashSet::new();
    for tool in tools {
        if json_type(&tool) != Some("namespace") {
            flattened.push(tool);
            continue;
        }
        let namespace = tool
            .get("name")
            .and_then(Value::as_str)
            .map(str::trim)
            .unwrap_or("");
        for child in namespace_children(&tool) {
            if json_type(&child) != Some("function") {
                continue;
            }
            let Some(name) = child
                .get("name")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|name| !name.is_empty())
                .map(ToOwned::to_owned)
            else {
                continue;
            };
            let mut lifted = child;
            if !namespace.is_empty() {
                let flat = flatten_namespace_tool_name(namespace, &name);
                if !seen_flat.insert(flat.clone()) {
                    continue;
                }
                if let Some(obj) = lifted.as_object_mut() {
                    obj.insert("name".to_string(), json!(flat));
                }
            }
            flattened.push(lifted);
        }
    }
    body["tools"] = json!(flattened);

    if let Some(input) = body.get_mut("input") {
        rewrite_namespace_qualified_calls(input, &owners);
    }
    if let Some(choice) = body.get_mut("tool_choice") {
        if json_type(choice) == Some("namespace") {
            *choice = json!("auto");
        } else {
            rewrite_namespace_qualified_call(choice, &owners);
        }
    }
    true
}

fn rewrite_namespace_qualified_calls(value: &mut Value, owners: &HashMap<String, NamespacedName>) {
    match value {
        Value::Array(items) => {
            for item in items {
                rewrite_namespace_qualified_calls(item, owners);
            }
        }
        Value::Object(obj) => {
            if obj.get("type").and_then(Value::as_str) == Some("function_call") {
                rewrite_namespace_qualified_call(value, owners);
                return;
            }
            for child in obj.values_mut() {
                rewrite_namespace_qualified_calls(child, owners);
            }
        }
        _ => {}
    }
}

fn rewrite_namespace_qualified_call(
    item: &mut Value,
    owners: &HashMap<String, NamespacedName>,
) -> bool {
    let Some(obj) = item.as_object_mut() else {
        return false;
    };
    let namespace = obj
        .get("namespace")
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or("")
        .to_string();
    let name = obj
        .get("name")
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or("")
        .to_string();
    if namespace.is_empty() || name.is_empty() {
        return false;
    }
    let flat = flatten_namespace_tool_name(&namespace, &name);
    match owners.get(&flat) {
        Some(entry) if entry.namespace == namespace && entry.name == name => {
            obj.insert("name".to_string(), json!(flat));
            obj.remove("namespace");
            true
        }
        _ => false,
    }
}

pub fn restore_response_namespaces(
    value: &mut Value,
    map: &HashMap<String, NamespacedName>,
) -> bool {
    if map.is_empty() {
        return false;
    }
    restore_namespace_value(value, map)
}

fn restore_namespace_value(value: &mut Value, map: &HashMap<String, NamespacedName>) -> bool {
    let mut changed = false;
    match value {
        Value::Array(items) => {
            for item in items {
                changed |= restore_namespace_value(item, map);
            }
        }
        Value::Object(obj) => {
            if obj.get("type").and_then(Value::as_str) == Some("function_call") {
                if let Some(flat) = obj.get("name").and_then(Value::as_str) {
                    if let Some(entry) = map.get(flat) {
                        obj.insert("name".to_string(), json!(entry.name));
                        obj.insert("namespace".to_string(), json!(entry.namespace));
                        changed = true;
                    }
                }
            }
            for child in obj.values_mut() {
                changed |= restore_namespace_value(child, map);
            }
        }
        _ => {}
    }
    changed
}

fn normalize_xai_function_tool_parameter_schemas(body: &mut Value) -> bool {
    let Some(tools) = body.get_mut("tools").and_then(Value::as_array_mut) else {
        return false;
    };
    let mut changed = false;
    for tool in tools {
        changed |= normalize_xai_function_tool_parameters(tool);
    }
    changed
}

fn xai_safe_empty_object_schema() -> Value {
    json!({"type": "object", "properties": {}, "additionalProperties": true})
}

fn xai_function_parameters_need_simplification(params: &Value) -> bool {
    match params {
        Value::Null => true,
        Value::Object(obj) if obj.is_empty() => true,
        Value::Object(obj) => {
            if obj.get("$ref").is_some() {
                return true;
            }
            if root_union_needs_rewrite(obj) {
                return true;
            }
            match obj.get("type") {
                None | Some(Value::Null) => true,
                Some(Value::String(type_name)) if type_name != "object" => true,
                _ => false,
            }
        }
        _ => true,
    }
}

fn root_union_key(obj: &Map<String, Value>) -> Option<&'static str> {
    ["oneOf", "anyOf"].into_iter().find(|key| {
        obj.get(*key)
            .and_then(Value::as_array)
            .is_some_and(|branches| !branches.is_empty())
    })
}

fn root_union_needs_rewrite(obj: &Map<String, Value>) -> bool {
    let Some(union_key) = root_union_key(obj) else {
        return false;
    };
    let Some(branches) = obj.get(union_key).and_then(Value::as_array) else {
        return false;
    };
    branches
        .iter()
        .any(|branch| branch.get("$ref").is_some() || !is_object_schema(branch))
}

fn local_schema_defs(schema: &Map<String, Value>) -> Option<&Map<String, Value>> {
    schema
        .get("$defs")
        .or_else(|| schema.get("definitions"))
        .or_else(|| schema.get("defs"))
        .and_then(Value::as_object)
}

fn local_def_name(reference: &str) -> Option<&str> {
    let name = reference
        .strip_prefix("#/$defs/")
        .or_else(|| reference.strip_prefix("#/definitions/"))
        .or_else(|| reference.strip_prefix("#/defs/"))?;
    if name.is_empty() || name.contains('/') {
        return None;
    }
    Some(name)
}

fn resolve_shallow_schema_ref(branch: &Value, defs: Option<&Map<String, Value>>) -> Value {
    let Some(reference) = branch.get("$ref").and_then(Value::as_str) else {
        return branch.clone();
    };
    let Some(name) = local_def_name(reference) else {
        return branch.clone();
    };
    defs.and_then(|defs| defs.get(name).cloned())
        .unwrap_or_else(|| branch.clone())
}

fn is_object_schema(value: &Value) -> bool {
    let Some(obj) = value.as_object() else {
        return false;
    };
    match obj.get("type").and_then(Value::as_str) {
        Some("object") => true,
        Some(_) => false,
        None => obj.contains_key("properties"),
    }
}

fn normalize_object_schema_branch(mut branch: Value) -> Value {
    if let Some(obj) = branch.as_object_mut() {
        obj.insert("type".to_string(), json!("object"));
        obj.entry("properties".to_string())
            .or_insert_with(|| json!({}));
        obj.remove("$ref");
    }
    branch
}

fn copy_local_schema_defs(source: &Map<String, Value>, target: &mut Map<String, Value>) {
    for key in ["$defs", "definitions", "defs"] {
        if let Some(value) = source.get(key) {
            target.insert(key.to_string(), value.clone());
        }
    }
}

fn rewrite_root_union_schema(obj: &Map<String, Value>) -> Option<Value> {
    let union_key = root_union_key(obj)?;
    let branches = obj.get(union_key).and_then(Value::as_array)?;
    let defs = local_schema_defs(obj);
    let mut object_branches: Vec<Value> = branches
        .iter()
        .map(|branch| resolve_shallow_schema_ref(branch, defs))
        .filter(|branch| is_object_schema(branch))
        .map(normalize_object_schema_branch)
        .collect();

    match object_branches.len() {
        0 => Some(xai_safe_empty_object_schema()),
        1 => object_branches.pop(),
        _ => {
            let mut result = Map::new();
            result.insert(union_key.to_string(), Value::Array(object_branches));
            copy_local_schema_defs(obj, &mut result);
            Some(Value::Object(result))
        }
    }
}

fn simplify_xai_function_parameters(params: Option<&Value>) -> Value {
    match params {
        None | Some(Value::Null) => xai_safe_empty_object_schema(),
        Some(Value::Object(obj)) if obj.is_empty() => xai_safe_empty_object_schema(),
        Some(Value::Object(obj)) => {
            let resolved =
                resolve_shallow_schema_ref(&Value::Object(obj.clone()), local_schema_defs(obj));
            let Some(resolved_obj) = resolved.as_object() else {
                return xai_safe_empty_object_schema();
            };
            if let Some(rewritten) = rewrite_root_union_schema(resolved_obj) {
                return rewritten;
            }
            let mut result = Value::Object(resolved_obj.clone());
            if let Some(result_obj) = result.as_object_mut() {
                if result_obj.get("type").and_then(Value::as_str) != Some("object") {
                    result_obj.insert("type".to_string(), json!("object"));
                    result_obj
                        .entry("properties".to_string())
                        .or_insert_with(|| json!({}));
                }
                result_obj.remove("$ref");
            }
            result
        }
        _ => xai_safe_empty_object_schema(),
    }
}

fn normalize_xai_function_tool_parameters(tool: &mut Value) -> bool {
    if tool.get("type").and_then(Value::as_str) != Some("function")
        && tool.get("function").is_none()
    {
        return false;
    }
    let params = tool
        .get("parameters")
        .cloned()
        .or_else(|| tool.pointer("/function/parameters").cloned());
    if let Some(params) = params.as_ref() {
        if !xai_function_parameters_need_simplification(params) {
            return false;
        }
    } else if params.is_none() {
        return false;
    }
    let simplified = simplify_xai_function_parameters(params.as_ref());
    if params.as_ref() == Some(&simplified) {
        return false;
    }
    if let Some(obj) = tool.as_object_mut() {
        if obj.contains_key("parameters") || params.is_some() && obj.get("function").is_none() {
            obj.insert("parameters".to_string(), simplified);
            return true;
        }
        if let Some(function) = obj.get_mut("function").and_then(Value::as_object_mut) {
            function.insert("parameters".to_string(), simplified);
            return true;
        }
    }
    false
}

fn rewrite_agent_message_value(value: &mut Value) -> bool {
    let mut changed = rewrite_agent_message_item(value);
    match value {
        Value::Array(items) => {
            for item in items {
                changed |= rewrite_agent_message_value(item);
            }
        }
        Value::Object(object) => {
            for child in object.values_mut() {
                changed |= rewrite_agent_message_value(child);
            }
        }
        _ => {}
    }
    changed
}

fn rewrite_agent_message_item(item: &mut Value) -> bool {
    if json_type(item) != Some("agent_message") {
        return false;
    }
    let id = item.get("id").cloned();
    let content = flatten_agent_message_content(item.get("content"));
    let mut message = json!({
        "type": "message",
        "role": "user",
        "content": content,
    });
    if let Some(id) = id {
        message["id"] = id;
    }
    *item = message;
    true
}

fn flatten_agent_message_content(content: Option<&Value>) -> Vec<Value> {
    match content {
        Some(Value::Array(parts)) => parts.iter().filter_map(part_to_input_text).collect(),
        Some(Value::String(text)) if !text.is_empty() => vec![input_text_part(text)],
        _ => Vec::new(),
    }
}

fn part_to_input_text(part: &Value) -> Option<Value> {
    let text = if json_type(part) == Some("encrypted_content") {
        part.get("encrypted_content")
            .or_else(|| part.get("text"))
            .and_then(Value::as_str)
    } else {
        part.get("text").and_then(Value::as_str)
    }?;
    if text.is_empty() {
        None
    } else {
        Some(input_text_part(text))
    }
}

fn input_text_part(text: &str) -> Value {
    json!({ "type": "input_text", "text": text })
}

/// Normalize completed function-call argument payloads for Codex Desktop.
///
/// Grok often emits `arguments` as a JSON object, especially for small tools
/// such as `view_image`. Codex serde expects a JSON string; an object is dropped
/// and the turn ends with no visible message. Whole-number JSON floats
/// (`92116.0`) are also rewritten to integers (`92116`) because Codex fails
/// local serde (`expected i32` / `expected u64`) and never runs the tool.
pub fn normalize_xai_function_call_integer_arguments(value: &mut Value) -> bool {
    match value {
        Value::Array(items) => {
            let mut changed = false;
            for item in items {
                changed |= normalize_xai_function_call_integer_arguments(item);
            }
            changed
        }
        Value::Object(obj) => {
            let event_type = obj
                .get("type")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned);
            if event_type.as_deref() == Some("response.function_call_arguments.delta") {
                return false;
            }
            let mut changed = false;
            if event_type.as_deref() == Some("response.function_call_arguments.done")
                || event_type.as_deref() == Some("function_call")
            {
                changed |= normalize_function_call_arguments_field(obj);
            }
            for child in obj.values_mut() {
                changed |= normalize_xai_function_call_integer_arguments(child);
            }
            changed
        }
        _ => false,
    }
}

fn normalize_function_call_arguments_field(obj: &mut Map<String, Value>) -> bool {
    let Some(arguments) = obj.get("arguments") else {
        return false;
    };
    if arguments.is_null() {
        return false;
    }
    if let Some(text) = arguments.as_str() {
        return match rewrite_whole_float_arguments_json(text) {
            Ok(Some(rewritten)) => {
                obj.insert("arguments".to_string(), Value::String(rewritten));
                true
            }
            _ => false,
        };
    }
    let mut value = obj.remove("arguments").unwrap_or(Value::Null);
    let _ = rewrite_whole_number_floats(&mut value);
    obj.insert(
        "arguments".to_string(),
        Value::String(json_compact_string(&value)),
    );
    true
}

fn rewrite_whole_float_arguments_json(
    arguments: &str,
) -> Result<Option<String>, serde_json::Error> {
    let mut value: Value = serde_json::from_str(arguments)?;
    if !rewrite_whole_number_floats(&mut value) {
        return Ok(None);
    }
    Ok(Some(serde_json::to_string(&value)?))
}

fn rewrite_whole_number_floats(value: &mut Value) -> bool {
    match value {
        Value::Number(number) => {
            if let Some(integer) = whole_float_to_json_int(number) {
                *number = integer;
                true
            } else {
                false
            }
        }
        Value::Array(items) => {
            let mut changed = false;
            for item in items {
                changed |= rewrite_whole_number_floats(item);
            }
            changed
        }
        Value::Object(map) => {
            let mut changed = false;
            for child in map.values_mut() {
                changed |= rewrite_whole_number_floats(child);
            }
            changed
        }
        _ => false,
    }
}

fn whole_float_to_json_int(number: &Number) -> Option<Number> {
    if number.is_i64() || number.is_u64() {
        return None;
    }
    let float = number.as_f64()?;
    if !float.is_finite() || float.fract() != 0.0 {
        return None;
    }
    if float >= 0.0 {
        if float > u64::MAX as f64 {
            return None;
        }
        let integer = float as u64;
        if integer as f64 != float {
            return None;
        }
        Some(Number::from(integer))
    } else {
        if float < i64::MIN as f64 {
            return None;
        }
        let integer = float as i64;
        if integer as f64 != float {
            return None;
        }
        Some(Number::from(integer))
    }
}

fn rewrite_xai_native_response_value(value: &mut Value, restore_map: &XaiNativeRestoreMap) -> bool {
    // Stringify/normalize arguments while the item is still `function_call`.
    // Custom/tool_search restore would otherwise move the payload and leave
    // object arguments or whole-floats in a shape Codex Desktop cannot serde.
    let mut changed = normalize_xai_function_call_integer_arguments(value);
    changed |= restore_tool_search_calls(value);
    changed |= compat_custom::restore_custom_tool_calls(value, &restore_map.custom_tool_names);
    changed |= restore_response_namespaces(value, &restore_map.namespaces);
    changed
}

pub fn rewrite_xai_native_json_bytes(bytes: &[u8], restore_map: &XaiNativeRestoreMap) -> Vec<u8> {
    let Ok(mut value) = serde_json::from_slice::<Value>(bytes) else {
        return bytes.to_vec();
    };
    if !rewrite_xai_native_response_value(&mut value, restore_map) {
        return bytes.to_vec();
    }
    serde_json::to_vec(&value).unwrap_or_else(|_| bytes.to_vec())
}

pub fn rewrite_xai_native_sse_block(block: &str, restore_map: &XaiNativeRestoreMap) -> Bytes {
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
    if !rewrite_xai_native_response_value(&mut event, restore_map) {
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

pub fn take_sse_block(buffer: &mut String) -> Option<String> {
    let mut best: Option<(usize, usize)> = None;
    for (delimiter, len) in [("\r\n\r\n", 4usize), ("\n\n", 2usize)] {
        if let Some(pos) = buffer.find(delimiter) {
            if best.is_none_or(|(best_pos, _)| pos < best_pos) {
                best = Some((pos, len));
            }
        }
    }
    let (pos, len) = best?;
    let block = buffer[..pos].to_string();
    buffer.drain(..pos + len);
    Some(block)
}

pub fn append_utf8_safe(buffer: &mut String, remainder: &mut Vec<u8>, new_bytes: &[u8]) {
    let (owned, bytes): (Option<Vec<u8>>, &[u8]) = if remainder.is_empty() {
        (None, new_bytes)
    } else if remainder.len() > 3 {
        buffer.push_str(&String::from_utf8_lossy(remainder));
        remainder.clear();
        (None, new_bytes)
    } else {
        let mut combined = std::mem::take(remainder);
        combined.extend_from_slice(new_bytes);
        (Some(combined), &[])
    };
    let input = owned.as_deref().unwrap_or(bytes);
    let mut pos = 0;
    loop {
        match std::str::from_utf8(&input[pos..]) {
            Ok(s) => {
                buffer.push_str(s);
                return;
            }
            Err(e) => {
                let valid_up_to = pos + e.valid_up_to();
                let valid_slice = &input[pos..valid_up_to];
                match std::str::from_utf8(valid_slice) {
                    Ok(valid) => buffer.push_str(valid),
                    Err(_) => buffer.push_str(&String::from_utf8_lossy(valid_slice)),
                }
                if let Some(invalid_len) = e.error_len() {
                    buffer.push('\u{FFFD}');
                    pos = valid_up_to + invalid_len;
                } else {
                    *remainder = input[valid_up_to..].to_vec();
                    return;
                }
            }
        }
    }
}

fn strip_sse_field<'a>(line: &'a str, field: &str) -> Option<&'a str> {
    line.strip_prefix(&format!("{field}: "))
        .or_else(|| line.strip_prefix(&format!("{field}:")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    fn provider_compat_hint_text_from_body(body: &Value) -> Option<String> {
        body.get("input")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .find(|item| is_provider_compat_hint(item))
            .and_then(|item| {
                item.get("content")
                    .and_then(Value::as_array)
                    .and_then(|parts| parts.first())
                    .and_then(|part| part.get("text"))
                    .and_then(Value::as_str)
                    .map(ToOwned::to_owned)
            })
    }

    #[test]
    fn rewrites_agent_message_at_input_index_matching_xai_422() {
        let mut body = json!({
            "model": "grok-4.6",
            "input": [
                { "type": "message", "role": "user", "content": [{ "type": "input_text", "text": "hi" }] },
                {
                    "type": "agent_message",
                    "id": "item-4",
                    "content": [{ "type": "encrypted_content", "encrypted_content": "spawn a worker" }]
                }
            ]
        });
        assert!(rewrite_xai_agent_message_input_items(&mut body));
        assert_eq!(body["input"][1]["type"], "message");
        assert_eq!(body["input"][1]["role"], "user");
        assert_eq!(body["input"][1]["content"][0]["text"], "spawn a worker");
        assert_eq!(body["input"][1]["id"], "item-4");
    }

    #[test]
    fn remaps_unknown_openai_role_sku_to_upstream_model() {
        let mut body = json!({ "model": "gpt-5.6-sol" });
        let changed = rewrite_xai_unknown_request_model(&mut body, "grok-4.6", &HashSet::new());
        assert_eq!(
            changed,
            Some(("gpt-5.6-sol".to_string(), "grok-4.6".to_string()))
        );
        assert_eq!(body["model"], "grok-4.6");
    }

    #[test]
    fn keeps_catalog_grok_model() {
        let mut allowed = HashSet::new();
        allowed.insert("grok-4.5".to_string());
        let mut body = json!({ "model": "grok-4.5" });
        assert_eq!(
            rewrite_xai_unknown_request_model(&mut body, "grok-4.6", &allowed),
            None
        );
        assert_eq!(body["model"], "grok-4.5");
    }

    #[test]
    fn keeps_unknown_grok_prefixed_request_model() {
        let mut body = json!({ "model": "grok-4.20" });
        assert_eq!(
            rewrite_xai_unknown_request_model(&mut body, "grok-4.6", &HashSet::new()),
            None
        );
        assert_eq!(body["model"], "grok-4.20");

        let mut prefixed = json!({ "model": "xai/grok-4.20" });
        assert_eq!(
            rewrite_xai_unknown_request_model(&mut prefixed, "grok-4.6", &HashSet::new()),
            None
        );
        assert_eq!(prefixed["model"], "xai/grok-4.20");
    }

    #[test]
    fn keeps_exact_grok_slug() {
        let mut body = json!({ "model": "grok" });
        assert_eq!(
            rewrite_xai_unknown_request_model(&mut body, "grok-4.6", &HashSet::new()),
            None
        );
        assert_eq!(body["model"], "grok");
    }

    #[test]
    fn remaps_non_hyphen_grok_prefix() {
        let mut body = json!({ "model": "grokking-1" });
        assert_eq!(
            rewrite_xai_unknown_request_model(&mut body, "grok-4.6", &HashSet::new()),
            Some(("grokking-1".to_string(), "grok-4.6".to_string()))
        );
        assert_eq!(body["model"], "grok-4.6");
    }

    #[test]
    fn apply_compat_rewrites_model_before_grok_45_field_strip() {
        let mut body = json!({
            "model": "gpt-5.6-sol",
            "stop": ["x"],
            "presence_penalty": 0.2
        });
        apply_xai_native_responses_request_compat(&mut body, Some("grok-4.5"), &HashSet::new());
        assert_eq!(body["model"], "grok-4.5");
        assert!(body.get("stop").is_none());
        assert!(body.get("presence_penalty").is_none());
    }

    #[test]
    fn lifts_ref_union_object_branch_instead_of_empty_schema() {
        let mut body = json!({
            "model": "grok-4.6",
            "tools": [{
                "type": "function",
                "name": "mcp__codex_app__automation_update",
                "parameters": {
                    "$defs": {
                        "Create": {
                            "type": "object",
                            "properties": {
                                "action": { "type": "string" },
                                "prompt": { "type": "string" }
                            },
                            "required": ["action"]
                        }
                    },
                    "oneOf": [
                        { "$ref": "#/$defs/Create" },
                        { "type": "null" }
                    ]
                }
            }]
        });
        assert!(sanitize_xai_responses_request(&mut body));
        let params = &body["tools"][0]["parameters"];
        assert_eq!(params["type"], "object");
        assert_eq!(params["properties"]["action"]["type"], "string");
        assert_eq!(params["properties"]["prompt"]["type"], "string");
        assert_eq!(params["required"], json!(["action"]));
        assert!(params.get("oneOf").is_none());
        assert!(params.get("$defs").is_none());
    }

    #[test]
    fn drops_null_and_keeps_object_union_branches() {
        let mut body = json!({
            "model": "grok-4.6",
            "tools": [{
                "type": "function",
                "name": "lookup",
                "parameters": {
                    "oneOf": [
                        {
                            "type": "object",
                            "properties": {
                                "id": { "type": "string" },
                                "slug": { "type": "string" }
                            },
                            "required": ["id"]
                        },
                        {
                            "type": "object",
                            "properties": {
                                "id": { "type": "string" },
                                "slug": { "type": "string" }
                            },
                            "required": ["slug"]
                        },
                        { "type": "null" }
                    ]
                }
            }]
        });
        assert!(sanitize_xai_responses_request(&mut body));
        let params = &body["tools"][0]["parameters"];
        assert!(params.get("type").is_none());
        assert_eq!(params["oneOf"].as_array().unwrap().len(), 2);
        assert_eq!(params["oneOf"][0]["required"], json!(["id"]));
        assert_eq!(params["oneOf"][1]["required"], json!(["slug"]));
        assert!(params.get("required").is_none());
    }

    #[test]
    fn keeps_all_object_union_untouched() {
        let mut body = json!({
            "model": "grok-4.6",
            "tools": [{
                "type": "function",
                "name": "lookup",
                "parameters": {
                    "anyOf": [
                        {
                            "type": "object",
                            "properties": {
                                "action": { "type": "string" },
                                "id": { "type": "string" }
                            },
                            "required": ["action", "id"]
                        },
                        {
                            "type": "object",
                            "properties": {
                                "action": { "type": "string" },
                                "slug": { "type": "string" }
                            },
                            "required": ["action", "slug"]
                        }
                    ]
                }
            }]
        });
        let original = body["tools"][0]["parameters"].clone();
        sanitize_xai_responses_request(&mut body);
        assert_eq!(body["tools"][0]["parameters"], original);
    }

    #[test]
    fn keeps_resolved_ref_union_instead_of_merging_or_emptying() {
        let mut body = json!({
            "model": "grok-4.6",
            "tools": [{
                "type": "function",
                "name": "mcp__codex_app__automation_update",
                "parameters": {
                    "$defs": {
                        "Create": {
                            "type": "object",
                            "properties": {
                                "action": { "type": "string" },
                                "prompt": { "type": "string" }
                            },
                            "required": ["action", "prompt"]
                        },
                        "Update": {
                            "type": "object",
                            "properties": {
                                "action": { "type": "string" },
                                "id": { "type": "string" }
                            },
                            "required": ["action", "id"]
                        }
                    },
                    "oneOf": [
                        { "$ref": "#/$defs/Create" },
                        { "$ref": "#/$defs/Update" },
                        { "type": "null" }
                    ]
                }
            }]
        });
        assert!(sanitize_xai_responses_request(&mut body));
        let params = &body["tools"][0]["parameters"];
        assert_eq!(params["oneOf"].as_array().unwrap().len(), 2);
        assert_eq!(params["oneOf"][0]["required"], json!(["action", "prompt"]));
        assert_eq!(params["oneOf"][1]["required"], json!(["action", "id"]));
        assert!(params.get("type").is_none());
        assert!(params.get("required").is_none());
        assert!(params.get("properties").is_none());
        assert_eq!(params["oneOf"][0]["properties"]["prompt"]["type"], "string");
        assert_eq!(params["oneOf"][1]["properties"]["id"]["type"], "string");
    }

    #[test]
    fn drops_tool_choice_when_no_tools_remain() {
        let mut body = json!({
            "model": "grok-4.6",
            "tools": [{ "type": "computer_use" }],
            "tool_choice": "auto"
        });
        assert!(sanitize_xai_responses_request(&mut body));
        assert!(body.get("tools").is_none(), "empty tools must be removed");
        assert!(
            body.get("tool_choice").is_none(),
            "tool_choice must drop when no tools remain"
        );
        let hint = provider_compat_hint_text_from_body(&body).expect("compat hint");
        assert!(hint.contains("computer_use"), "{hint}");
        assert!(hint.contains("Answer in text only"), "{hint}");
    }

    #[test]
    fn drops_tool_choice_when_tools_key_is_missing() {
        let mut body = json!({
            "model": "grok-4.6",
            "tool_choice": "required"
        });
        assert!(sanitize_xai_responses_request(&mut body));
        assert!(body.get("tool_choice").is_none());
        let hint = provider_compat_hint_text_from_body(&body).expect("compat hint");
        assert!(hint.contains("No tools are available"), "{hint}");
    }

    #[test]
    fn drops_dangling_function_tool_choice() {
        let mut body = json!({
            "model": "grok-4.6",
            "tools": [{ "type": "computer_use" }],
            "tool_choice": { "type": "function", "name": "gone" }
        });
        assert!(sanitize_xai_responses_request(&mut body));
        assert!(body.get("tools").is_none());
        assert!(body.get("tool_choice").is_none());
    }

    #[test]
    fn keeps_valid_function_tool_choice() {
        let mut body = json!({
            "model": "grok-4.6",
            "tools": [{ "type": "function", "name": "run" }],
            "tool_choice": { "type": "function", "name": "run" }
        });
        assert!(!sanitize_xai_responses_request(&mut body));
        assert_eq!(
            body.get("tool_choice").unwrap(),
            &json!({ "type": "function", "name": "run" })
        );
    }

    #[test]
    fn injects_compat_hint_when_dropping_unsupported_type_but_keeping_functions() {
        let mut body = json!({
            "model": "grok-4.6",
            "input": [{
                "type": "message",
                "role": "user",
                "content": [{ "type": "input_text", "text": "continue" }]
            }],
            "tools": [
                { "type": "function", "name": "run" },
                { "type": "computer_use" }
            ],
            "tool_choice": "auto"
        });
        assert!(sanitize_xai_responses_request(&mut body));
        assert_eq!(body["tool_choice"], "auto");
        assert_eq!(body["tools"].as_array().unwrap().len(), 1);
        let hint = provider_compat_hint_text_from_body(&body).expect("compat hint");
        assert!(hint.contains("computer_use"), "{hint}");
        assert!(hint.contains("function:run"), "{hint}");
        assert!(!hint.contains("Answer in text only"), "{hint}");
        assert!(!sanitize_xai_responses_request(&mut body));
        let hints = body["input"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|item| is_provider_compat_hint(item))
            .count();
        assert_eq!(hints, 1, "compat hint must be idempotent");
    }

    #[test]
    fn rewrites_tool_search_to_function_and_keeps_tool_choice() {
        let mut body = json!({
            "model": "grok-4.6",
            "tools": [{ "type": "tool_search" }],
            "tool_choice": { "type": "tool_search" }
        });
        assert!(sanitize_xai_responses_request(&mut body));
        let tools = body["tools"].as_array().expect("tools");
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0]["type"], "function");
        assert_eq!(tools[0]["name"], "tool_search");
        assert_eq!(
            body.get("tool_choice").unwrap(),
            &json!({ "type": "function", "name": "tool_search" })
        );
        assert!(provider_compat_hint_text_from_body(&body).is_none());
    }

    #[test]
    fn promotes_tools_from_tool_search_output() {
        let mut body = json!({
            "model": "grok-4.6",
            "tools": [{ "type": "tool_search" }],
            "input": [{
                "type": "tool_search_output",
                "call_id": "call_tool_search_1",
                "tools": [{
                    "type": "namespace",
                    "name": "mcp__files__",
                    "tools": [{ "type": "function", "name": "read" }]
                }]
            }]
        });
        assert!(sanitize_xai_responses_request(&mut body));
        let names: Vec<&str> = body["tools"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|tool| tool.get("name").and_then(Value::as_str))
            .collect();
        assert!(names.contains(&"tool_search"), "{names:?}");
        assert!(names.contains(&"mcp__files____read"), "{names:?}");
        assert_eq!(body["input"][0]["type"], "function_call_output");
        assert_eq!(body["input"][0]["call_id"], "call_tool_search_1");
        assert!(
            body["input"][0]["output"]
                .as_str()
                .unwrap()
                .contains("mcp__files__"),
            "{}",
            body["input"][0]["output"]
        );
        assert!(provider_compat_hint_text_from_body(&body).is_none());
    }

    #[test]
    fn rewrites_tool_search_history_to_function_items() {
        let mut body = json!({
            "model": "grok-4.6",
            "tools": [{ "type": "tool_search" }],
            "input": [
                {
                    "type": "tool_search_call",
                    "call_id": "call_tool_search_1",
                    "status": "completed",
                    "execution": "client",
                    "arguments": { "query": "Gmail search emails", "limit": 5 }
                },
                {
                    "type": "tool_search_output",
                    "call_id": "call_tool_search_1",
                    "status": "completed",
                    "execution": "client",
                    "tools": [{
                        "type": "namespace",
                        "name": "mcp__codex_apps__gmail",
                        "tools": [{ "type": "function", "name": "_search_emails" }]
                    }]
                },
                {
                    "type": "message",
                    "role": "user",
                    "content": [{ "type": "input_text", "text": "Search unread inbox mail." }]
                }
            ]
        });
        assert!(sanitize_xai_responses_request(&mut body));
        assert_eq!(body["input"][0]["type"], "function_call");
        assert_eq!(body["input"][0]["name"], "tool_search");
        assert_eq!(body["input"][0]["call_id"], "call_tool_search_1");
        assert_eq!(body["input"][0]["status"], "completed");
        assert!(body["input"][0].get("execution").is_none());
        let arguments: Value =
            serde_json::from_str(body["input"][0]["arguments"].as_str().unwrap()).unwrap();
        assert_eq!(arguments["query"], "Gmail search emails");
        assert_eq!(body["input"][1]["type"], "function_call_output");
        assert_eq!(body["input"][1]["call_id"], "call_tool_search_1");
        assert!(
            body["input"][1]["output"]
                .as_str()
                .unwrap()
                .contains("mcp__codex_apps__gmail"),
            "{}",
            body["input"][1]["output"]
        );
        let names: Vec<&str> = body["tools"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|tool| tool.get("name").and_then(Value::as_str))
            .collect();
        assert!(names.contains(&"tool_search"), "{names:?}");
        assert!(
            names.contains(&"mcp__codex_apps__gmail___search_emails"),
            "{names:?}"
        );
        assert!(provider_compat_hint_text_from_body(&body).is_none());
        assert!(!sanitize_xai_responses_request(&mut body));
    }

    #[test]
    fn restores_function_call_named_tool_search() {
        let mut value = json!({
            "output": [{
                "type": "function_call",
                "name": "tool_search",
                "call_id": "call_tool_search_1",
                "arguments": r#"{"query":"gmail","limit":5}"#
            }]
        });
        assert!(restore_tool_search_calls(&mut value));
        assert_eq!(value["output"][0]["type"], "tool_search_call");
        assert_eq!(value["output"][0]["call_id"], "call_tool_search_1");
        assert_eq!(value["output"][0]["execution"], "client");
        assert_eq!(value["output"][0]["arguments"]["query"], "gmail");
        assert!(value["output"][0].get("name").is_none());
    }

    #[test]
    fn sse_block_restores_tool_search_call() {
        let block = concat!(
            "event: response.output_item.done\n",
            r#"data: {"type":"function_call","name":"tool_search","call_id":"call_1","arguments":"{\"query\":\"mail\"}"}"#,
            "\n\n"
        );
        let rewritten = rewrite_xai_native_sse_block(block, &XaiNativeRestoreMap::default());
        let text = String::from_utf8(rewritten.to_vec()).unwrap();
        let data = text
            .lines()
            .find_map(|line| line.strip_prefix("data: "))
            .expect("sse data");
        let event: Value = serde_json::from_str(data).unwrap();
        assert_eq!(event["type"], "tool_search_call");
        assert_eq!(event["call_id"], "call_1");
        assert_eq!(event["arguments"]["query"], "mail");
    }

    #[test]
    fn keeps_string_tool_choice_when_tools_remain() {
        let mut body = json!({
            "model": "grok-4.6",
            "tools": [{ "type": "function", "name": "run" }],
            "tool_choice": "auto"
        });
        assert!(!sanitize_xai_responses_request(&mut body));
        assert_eq!(body.get("tool_choice").unwrap(), &json!("auto"));
    }

    #[test]
    fn drops_namespace_tools_and_private_fields() {
        let mut body = json!({
            "model": "grok-4.6",
            "prompt_cache_retention": "24h",
            "tools": [
                {
                    "type": "namespace",
                    "name": "mcp__files__",
                    "tools": [{ "type": "function", "name": "ok" }]
                },
                { "type": "tool_search" }
            ]
        });
        let restore = apply_xai_native_responses_request_compat(&mut body, None, &HashSet::new());
        assert!(body.get("prompt_cache_retention").is_none());
        let tools = body["tools"].as_array().expect("tools");
        let names: Vec<&str> = tools
            .iter()
            .filter_map(|tool| tool.get("name").and_then(Value::as_str))
            .collect();
        assert!(names.contains(&"mcp__files____ok"), "{names:?}");
        assert!(names.contains(&"tool_search"), "{names:?}");
        assert_eq!(restore.namespaces["mcp__files____ok"].name, "ok");
        assert_eq!(
            restore.namespaces["mcp__files____ok"].namespace,
            "mcp__files__"
        );
        assert!(provider_compat_hint_text_from_body(&body).is_none());
    }

    #[test]
    fn whole_floats_92116_and_120000_become_integers() {
        let mut value: Value =
            serde_json::from_str(r#"{"session_id":92116.0,"yield_time_ms":120000.0,"wait":1.5}"#)
                .unwrap();
        assert!(rewrite_whole_number_floats(&mut value));
        assert_eq!(value["session_id"].as_i64(), Some(92116));
        assert_eq!(value["yield_time_ms"].as_u64(), Some(120000));
        assert_eq!(value["wait"].as_f64(), Some(1.5));
        assert!(value["wait"].as_i64().is_none());
        let encoded = serde_json::to_string(&value).unwrap();
        assert!(encoded.contains(r#""session_id":92116"#));
        assert!(!encoded.contains("92116.0"));
    }

    #[test]
    fn function_call_arguments_done_rewrites_whole_floats_recursively() {
        let mut event = json!({
            "type": "response.function_call_arguments.done",
            "item_id": "fc_exec",
            "arguments": r#"{"session_id":92116.0,"yield_time_ms":120000.0,"nested":{"n":92116.0},"arr":[120000.0,1.5]}"#
        });
        assert!(normalize_xai_function_call_integer_arguments(&mut event));
        let arguments: Value = serde_json::from_str(event["arguments"].as_str().unwrap()).unwrap();
        assert_eq!(arguments["session_id"].as_i64(), Some(92116));
        assert_eq!(arguments["yield_time_ms"].as_u64(), Some(120000));
        assert_eq!(arguments["nested"]["n"].as_i64(), Some(92116));
        assert_eq!(arguments["arr"][0].as_u64(), Some(120000));
        assert_eq!(arguments["arr"][1].as_f64(), Some(1.5));
    }

    #[test]
    fn function_call_object_arguments_are_stringified() {
        let mut body = json!({
            "output": [{
                "type": "function_call",
                "name": "view_image",
                "call_id": "call_1",
                "arguments": { "path": "/tmp/a.png" }
            }]
        });
        assert!(normalize_xai_function_call_integer_arguments(&mut body));
        let arguments = body["output"][0]["arguments"]
            .as_str()
            .expect("string args");
        let parsed: Value = serde_json::from_str(arguments).unwrap();
        assert_eq!(parsed["path"], "/tmp/a.png");
    }

    #[test]
    fn function_call_object_arguments_stringify_and_rewrite_whole_floats() {
        let mut body = json!({
            "output": [{
                "type": "function_call",
                "name": "write_stdin",
                "arguments": { "session_id": 92116.0, "yield_time_ms": 120000.0 }
            }]
        });
        assert!(normalize_xai_function_call_integer_arguments(&mut body));
        let arguments: Value =
            serde_json::from_str(body["output"][0]["arguments"].as_str().unwrap()).unwrap();
        assert_eq!(arguments["session_id"].as_i64(), Some(92116));
        assert_eq!(arguments["yield_time_ms"].as_u64(), Some(120000));
    }

    #[test]
    fn sse_block_stringifies_object_function_call_arguments() {
        let restore = XaiNativeRestoreMap::default();
        let block = concat!(
            "event: response.output_item.done\n",
            r#"data: {"type":"function_call","name":"view_image","call_id":"call_1","arguments":{"path":"/tmp/a.png"}}"#,
            "\n\n"
        );
        let rewritten = rewrite_xai_native_sse_block(block, &restore);
        let text = String::from_utf8(rewritten.to_vec()).unwrap();
        let data = text
            .lines()
            .find_map(|line| line.strip_prefix("data: "))
            .expect("sse data");
        let event: Value = serde_json::from_str(data).unwrap();
        let arguments: Value = serde_json::from_str(event["arguments"].as_str().unwrap()).unwrap();
        assert_eq!(event["name"], "view_image");
        assert_eq!(arguments["path"], "/tmp/a.png");
    }

    #[test]
    fn sse_block_normalizes_object_arguments_before_custom_restore() {
        let mut restore = XaiNativeRestoreMap::default();
        restore.custom_tool_names.insert("apply_patch".to_string());
        let block = concat!(
            "event: response.output_item.done\n",
            r#"data: {"type":"function_call","name":"apply_patch","call_id":"call_1","arguments":{"input":"patch","yield_time_ms":120000.0}}"#,
            "\n\n"
        );
        let rewritten = rewrite_xai_native_sse_block(block, &restore);
        let text = String::from_utf8(rewritten.to_vec()).unwrap();
        let data = text
            .lines()
            .find_map(|line| line.strip_prefix("data: "))
            .expect("sse data");
        let event: Value = serde_json::from_str(data).unwrap();
        assert_eq!(event["type"], "custom_tool_call");
        assert_eq!(event["name"], "apply_patch");
        assert_eq!(event["input"], "patch");
        assert!(event.get("arguments").is_none());
    }

    #[test]
    fn completed_function_call_item_rewrites_whole_float_arguments() {
        let mut body = json!({
            "output": [{
                "type": "function_call",
                "name": "write_stdin",
                "arguments": r#"{"session_id":92116.0,"yield_time_ms":120000.0}"#
            }]
        });
        assert!(normalize_xai_function_call_integer_arguments(&mut body));
        let arguments: Value =
            serde_json::from_str(body["output"][0]["arguments"].as_str().unwrap()).unwrap();
        assert_eq!(arguments["session_id"].as_i64(), Some(92116));
        assert_eq!(arguments["yield_time_ms"].as_u64(), Some(120000));
    }

    #[test]
    fn function_call_argument_deltas_are_not_rewritten() {
        let mut event = json!({
            "type": "response.function_call_arguments.delta",
            "delta": r#"{"session_id":92116.0"#,
            "item": {
                "type": "function_call",
                "arguments": r#"{"session_id":92116.0}"#
            }
        });
        let original = event.clone();
        assert!(!normalize_xai_function_call_integer_arguments(&mut event));
        assert_eq!(event, original);
    }

    #[test]
    fn sse_block_restores_namespace_and_rewrites_whole_floats() {
        let mut restore = XaiNativeRestoreMap::default();
        restore.namespaces.insert(
            "mcp__files____read".to_string(),
            NamespacedName {
                namespace: "mcp__files__".to_string(),
                name: "read".to_string(),
            },
        );
        let block = concat!(
            "event: response.output_item.done\n",
            r#"data: {"type":"function_call","name":"mcp__files____read","arguments":"{\"n\":92116.0}"}"#,
            "\n\n"
        );
        let rewritten = rewrite_xai_native_sse_block(block, &restore);
        let text = String::from_utf8(rewritten.to_vec()).unwrap();
        let data = text
            .lines()
            .find_map(|line| line.strip_prefix("data: "))
            .expect("sse data");
        let event: Value = serde_json::from_str(data).unwrap();
        assert_eq!(event["name"], "read");
        assert_eq!(event["namespace"], "mcp__files__");
        let arguments: Value = serde_json::from_str(event["arguments"].as_str().unwrap()).unwrap();
        assert_eq!(arguments["n"].as_i64(), Some(92116));
        assert!(!data.contains("92116.0"));
    }

    #[test]
    fn rewrites_every_agent_message_in_input() {
        let mut body = json!({
            "input": [
                { "type": "agent_message", "content": "one" },
                { "type": "message", "role": "user", "content": [{ "type": "input_text", "text": "mid" }] },
                { "type": "agent_message", "content": "two" }
            ]
        });
        assert!(rewrite_xai_agent_message_input_items(&mut body));
        assert_eq!(body["input"][0]["type"], "message");
        assert_eq!(body["input"][0]["content"][0]["text"], "one");
        assert_eq!(body["input"][2]["type"], "message");
        assert_eq!(body["input"][2]["content"][0]["text"], "two");
    }

    #[test]
    fn promotes_additional_tools_and_strips_null_reasoning_content() {
        let mut body = json!({
            "model": "grok-4.6",
            "input": [
                { "type": "reasoning", "content": null },
                {
                    "type": "additional_tools",
                    "tools": [{ "type": "function", "name": "ok", "parameters": { "oneOf": [{ "type": "null" }, { "type": "object", "properties": {} }] } }]
                }
            ]
        });
        assert!(sanitize_xai_responses_request(&mut body));
        assert!(body["input"]
            .as_array()
            .expect("input")
            .iter()
            .all(|item| item.get("type").and_then(Value::as_str) != Some("additional_tools")));
        assert!(body["input"][0].get("content").is_none());
        assert_eq!(body["tools"][0]["name"], "ok");
        assert_eq!(body["tools"][0]["parameters"]["type"], "object");
    }

    #[test]
    fn apply_compat_rewrites_agent_message_and_unknown_model_together() {
        let mut body = json!({
            "model": "gpt-5.6-sol",
            "input": [{ "type": "agent_message", "content": "task" }]
        });
        apply_xai_native_responses_request_compat(&mut body, Some("grok-4.6"), &HashSet::new());
        assert_eq!(body["model"], "grok-4.6");
        assert_eq!(body["input"][0]["type"], "message");
    }

    #[test]
    fn rewrites_custom_to_function_and_keeps_tool_choice() {
        let mut body = json!({
            "model": "grok-4.6",
            "tools": [{
                "type": "custom",
                "name": "apply_patch",
                "description": "Apply a patch",
                "format": { "type": "grammar", "syntax": "lark", "definition": "start: patch" }
            }],
            "tool_choice": { "type": "custom", "name": "apply_patch" }
        });
        assert!(sanitize_xai_responses_request(&mut body));
        let tools = body["tools"].as_array().expect("tools");
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0]["type"], "function");
        assert_eq!(tools[0]["name"], "apply_patch");
        assert_eq!(tools[0]["parameters"]["required"][0], "input");
        assert!(
            tools[0]["description"]
                .as_str()
                .unwrap()
                .contains("\"type\":\"custom\""),
            "{}",
            tools[0]["description"]
        );
        assert_eq!(
            body.get("tool_choice").unwrap(),
            &json!({ "type": "function", "name": "apply_patch" })
        );
        assert!(provider_compat_hint_text_from_body(&body).is_none());
    }

    #[test]
    fn rewrites_custom_history_to_function_items() {
        let mut body = json!({
            "model": "grok-4.6",
            "tools": [{ "type": "custom", "name": "apply_patch" }],
            "input": [
                {
                    "type": "custom_tool_call",
                    "call_id": "call_patch_1",
                    "name": "apply_patch",
                    "input": "*** Begin Patch\n*** End Patch"
                },
                {
                    "type": "custom_tool_call_output",
                    "call_id": "call_patch_1",
                    "output": "success"
                }
            ]
        });
        let restore = apply_xai_native_responses_request_compat(&mut body, None, &HashSet::new());
        assert!(restore.custom_tool_names.contains("apply_patch"));
        assert_eq!(body["input"][0]["type"], "function_call");
        assert_eq!(body["input"][0]["name"], "apply_patch");
        assert_eq!(body["input"][0]["call_id"], "call_patch_1");
        let arguments: Value =
            serde_json::from_str(body["input"][0]["arguments"].as_str().unwrap()).unwrap();
        assert_eq!(arguments["input"], "*** Begin Patch\n*** End Patch");
        assert_eq!(body["input"][1]["type"], "function_call_output");
        assert_eq!(body["input"][1]["call_id"], "call_patch_1");
        assert_eq!(body["input"][1]["output"], "success");
        assert!(provider_compat_hint_text_from_body(&body).is_none());
    }

    #[test]
    fn restores_function_call_named_custom_tool() {
        let mut restore = XaiNativeRestoreMap::default();
        restore.custom_tool_names.insert("apply_patch".to_string());
        let mut value = json!({
            "output": [{
                "type": "function_call",
                "name": "apply_patch",
                "call_id": "call_patch_1",
                "arguments": "{\"input\":\"*** Begin Patch\"}"
            }]
        });
        let encoded = serde_json::to_vec(&value).unwrap();
        let rewritten = rewrite_xai_native_json_bytes(&encoded, &restore);
        value = serde_json::from_slice(&rewritten).unwrap();
        assert_eq!(value["output"][0]["type"], "custom_tool_call");
        assert_eq!(value["output"][0]["name"], "apply_patch");
        assert_eq!(value["output"][0]["call_id"], "call_patch_1");
        assert_eq!(value["output"][0]["input"], "*** Begin Patch");
        assert!(value["output"][0].get("arguments").is_none());
    }

    #[test]
    fn sse_block_restores_custom_tool_call() {
        let mut restore = XaiNativeRestoreMap::default();
        restore.custom_tool_names.insert("apply_patch".to_string());
        let block = concat!(
            "event: response.output_item.done\n",
            r#"data: {"type":"function_call","name":"apply_patch","call_id":"call_1","arguments":"{\"input\":\"patch\"}"}"#,
            "\n\n"
        );
        let rewritten = rewrite_xai_native_sse_block(block, &restore);
        let text = String::from_utf8(rewritten.to_vec()).unwrap();
        let data = text
            .lines()
            .find_map(|line| line.strip_prefix("data: "))
            .expect("sse data");
        let event: Value = serde_json::from_str(data).unwrap();
        assert_eq!(event["type"], "custom_tool_call");
        assert_eq!(event["name"], "apply_patch");
        assert_eq!(event["call_id"], "call_1");
        assert_eq!(event["input"], "patch");
    }
}
