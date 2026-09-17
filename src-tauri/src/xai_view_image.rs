//! Grok-facing `view_image` adapter.
//!
//! Codex Desktop executes `view_image` as a single-path function call.
//! Grok often wants several files at once (extracted frames, screenshots)
//! and then either skips the call or emits `paths` / an array. Request-side
//! this module publishes a batch-friendly schema. Return-path it expands one
//! batch call into Codex single-path `function_call` items.
//!
//! This does not invent `view_image` when Grok emitted no function call.

use serde_json::{json, Map, Value};

const VIEW_IMAGE: &str = "view_image";
const VIEW_IMAGE_DESCRIPTION: &str = "View local image files that already exist on disk. For one image, pass path. For several frames or screenshots, pass paths in this same call. Emit this function call; do not only mention it in reasoning.";

pub fn rewrite_view_image_tool(body: &mut Value) -> bool {
    let Some(tools) = body.get_mut("tools").and_then(Value::as_array_mut) else {
        return false;
    };
    let mut changed = false;
    for tool in tools {
        changed |= rewrite_one_view_image_tool(tool);
    }
    changed
}

fn rewrite_one_view_image_tool(tool: &mut Value) -> bool {
    if function_name(tool) != Some(VIEW_IMAGE) {
        return false;
    }
    let parameters = view_image_parameters();
    let Some(obj) = tool.as_object_mut() else {
        return false;
    };
    let mut changed = false;
    if obj.get("description").and_then(Value::as_str) != Some(VIEW_IMAGE_DESCRIPTION) {
        obj.insert("description".to_string(), json!(VIEW_IMAGE_DESCRIPTION));
        changed = true;
    }
    if let Some(function) = obj.get_mut("function").and_then(Value::as_object_mut) {
        if function.get("description").and_then(Value::as_str) != Some(VIEW_IMAGE_DESCRIPTION) {
            function.insert("description".to_string(), json!(VIEW_IMAGE_DESCRIPTION));
            changed = true;
        }
        if function.get("parameters") != Some(&parameters) {
            function.insert("parameters".to_string(), parameters.clone());
            changed = true;
        }
    }
    if obj.get("parameters") != Some(&parameters) {
        obj.insert("parameters".to_string(), parameters);
        changed = true;
    }
    changed
}

fn view_image_parameters() -> Value {
    json!({
        "type": "object",
        "properties": {
            "path": {
                "type": "string",
                "description": "Absolute filesystem path to one image file."
            },
            "paths": {
                "type": "array",
                "items": { "type": "string" },
                "description": "Absolute filesystem paths to view together in this call."
            }
        },
        "additionalProperties": true
    })
}

fn function_name(tool: &Value) -> Option<&str> {
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

pub fn adapt_view_image_function_calls(value: &mut Value) -> bool {
    let mut changed = false;
    match value {
        Value::Array(items) => {
            let mut next = Vec::with_capacity(items.len());
            for mut item in items.drain(..) {
                changed |= adapt_view_image_function_calls(&mut item);
                let split = split_view_image_event(&item);
                if split.len() != 1 {
                    changed = true;
                } else if split.first() != Some(&item) {
                    changed = true;
                }
                next.extend(split);
            }
            *items = next;
        }
        Value::Object(_) => {
            if let Some(obj) = value.as_object_mut() {
                for child in obj.values_mut() {
                    changed |= adapt_view_image_function_calls(child);
                }
            }
            if let Some(normalized) = normalize_single_path_view_image(value) {
                if normalized != *value {
                    *value = normalized;
                    changed = true;
                }
            }
        }
        _ => {}
    }
    changed
}

pub fn split_view_image_event(value: &Value) -> Vec<Value> {
    let Some(call) = view_image_call_target(value) else {
        return vec![value.clone()];
    };
    let paths = view_image_paths(call);
    if paths.len() <= 1 {
        return vec![normalize_single_path_view_image(value).unwrap_or_else(|| value.clone())];
    }
    paths
        .iter()
        .enumerate()
        .map(|(index, path)| rewrite_view_image_copy(value, path, index))
        .collect()
}

fn view_image_call_target(value: &Value) -> Option<&Value> {
    if is_view_image_function_call(value) {
        return Some(value);
    }
    let item = value.get("item")?;
    if is_view_image_function_call(item) {
        Some(item)
    } else {
        None
    }
}

fn is_view_image_function_call(value: &Value) -> bool {
    value.get("type").and_then(Value::as_str) == Some("function_call")
        && value.get("name").and_then(Value::as_str) == Some(VIEW_IMAGE)
}

fn normalize_single_path_view_image(value: &Value) -> Option<Value> {
    let call = view_image_call_target(value)?;
    let paths = view_image_paths(call);
    if paths.len() != 1 {
        return None;
    }
    Some(rewrite_view_image_copy(value, &paths[0], 0))
}

fn rewrite_view_image_copy(value: &Value, path: &str, index: usize) -> Value {
    let mut next = value.clone();
    if is_view_image_function_call(&next) {
        apply_single_path(&mut next, path, index);
        return next;
    }
    if let Some(item) = next.get_mut("item") {
        if is_view_image_function_call(item) {
            apply_single_path(item, path, index);
        }
    }
    next
}

fn apply_single_path(call: &mut Value, path: &str, index: usize) {
    let Some(obj) = call.as_object_mut() else {
        return;
    };
    obj.insert(
        "arguments".to_string(),
        Value::String(json!({ "path": path }).to_string()),
    );
    suffix_id_field(obj, "call_id", index);
    suffix_id_field(obj, "id", index);
}

fn suffix_id_field(obj: &mut Map<String, Value>, key: &str, index: usize) {
    if index == 0 {
        return;
    }
    let Some(base) = obj.get(key).and_then(Value::as_str).map(str::trim) else {
        return;
    };
    if base.is_empty() {
        return;
    }
    obj.insert(key.to_string(), json!(format!("{base}__{index}")));
}

fn view_image_paths(call: &Value) -> Vec<String> {
    let Some(arguments) = call.get("arguments") else {
        return Vec::new();
    };
    collect_paths(arguments)
}

fn collect_paths(arguments: &Value) -> Vec<String> {
    match arguments {
        Value::String(text) => {
            let trimmed = text.trim();
            if trimmed.is_empty() {
                return Vec::new();
            }
            match serde_json::from_str::<Value>(trimmed) {
                Ok(parsed) => collect_paths(&parsed),
                Err(_) => vec![trimmed.to_string()],
            }
        }
        Value::Array(items) => string_paths(items),
        Value::Object(obj) => {
            let mut paths = Vec::new();
            match obj.get("path") {
                Some(Value::String(path)) => push_path(&mut paths, path),
                Some(Value::Array(items)) => {
                    for path in string_paths(items) {
                        push_path(&mut paths, &path);
                    }
                }
                _ => {}
            }
            if let Some(Value::Array(items)) = obj.get("paths") {
                for path in string_paths(items) {
                    push_path(&mut paths, &path);
                }
            }
            paths
        }
        _ => Vec::new(),
    }
}

fn string_paths(items: &[Value]) -> Vec<String> {
    items
        .iter()
        .filter_map(Value::as_str)
        .map(str::trim)
        .filter(|path| !path.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

fn push_path(paths: &mut Vec<String>, path: &str) {
    let path = path.trim();
    if path.is_empty() {
        return;
    }
    if !paths.iter().any(|existing| existing == path) {
        paths.push(path.to_string());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rewrites_view_image_tool_schema_for_grok() {
        let mut body = json!({
            "tools": [{
                "type": "function",
                "name": "view_image",
                "description": "View a local image file from the filesystem when visual inspection is needed.",
                "parameters": {
                    "type": "object",
                    "additionalProperties": false,
                    "properties": {
                        "path": { "type": "string" }
                    },
                    "required": ["path"]
                }
            }]
        });
        assert!(rewrite_view_image_tool(&mut body));
        assert_eq!(body["tools"][0]["description"], VIEW_IMAGE_DESCRIPTION);
        assert_eq!(
            body["tools"][0]["parameters"]["properties"]["paths"]["type"],
            "array"
        );
        assert_eq!(
            body["tools"][0]["parameters"]["properties"]["path"]["type"],
            "string"
        );
        assert!(!rewrite_view_image_tool(&mut body));
    }

    #[test]
    fn does_not_touch_other_tools() {
        let mut body = json!({
            "tools": [{
                "type": "function",
                "name": "exec_command",
                "parameters": { "type": "object", "properties": { "cmd": { "type": "string" } } }
            }]
        });
        let before = body.clone();
        assert!(!rewrite_view_image_tool(&mut body));
        assert_eq!(body, before);
    }

    #[test]
    fn expands_paths_array_into_single_path_function_calls() {
        let mut value = json!({
            "output": [{
                "type": "function_call",
                "name": "view_image",
                "call_id": "call_1",
                "arguments": { "paths": ["/tmp/a.png", "/tmp/b.png"] }
            }]
        });
        assert!(adapt_view_image_function_calls(&mut value));
        assert_eq!(value["output"].as_array().unwrap().len(), 2);
        assert_eq!(value["output"][0]["call_id"], "call_1");
        assert_eq!(value["output"][1]["call_id"], "call_1__1");
        let first: Value =
            serde_json::from_str(value["output"][0]["arguments"].as_str().unwrap()).unwrap();
        let second: Value =
            serde_json::from_str(value["output"][1]["arguments"].as_str().unwrap()).unwrap();
        assert_eq!(first, json!({ "path": "/tmp/a.png" }));
        assert_eq!(second, json!({ "path": "/tmp/b.png" }));
    }

    #[test]
    fn normalizes_single_paths_entry_to_path() {
        let mut value = json!({
            "type": "function_call",
            "name": "view_image",
            "call_id": "call_1",
            "arguments": { "paths": ["/tmp/a.png"] }
        });
        assert!(adapt_view_image_function_calls(&mut value));
        let arguments: Value = serde_json::from_str(value["arguments"].as_str().unwrap()).unwrap();
        assert_eq!(arguments, json!({ "path": "/tmp/a.png" }));
        assert_eq!(value["call_id"], "call_1");
    }

    #[test]
    fn splits_wrapped_output_item_events() {
        let value = json!({
            "type": "response.output_item.done",
            "item": {
                "type": "function_call",
                "name": "view_image",
                "call_id": "c1",
                "id": "fc_1",
                "arguments": "{\"path\":[\"/tmp/a.png\",\"/tmp/b.png\"]}"
            }
        });
        let split = split_view_image_event(&value);
        assert_eq!(split.len(), 2);
        assert_eq!(split[0]["item"]["call_id"], "c1");
        assert_eq!(split[1]["item"]["call_id"], "c1__1");
        assert_eq!(split[1]["item"]["id"], "fc_1__1");
        let first: Value =
            serde_json::from_str(split[0]["item"]["arguments"].as_str().unwrap()).unwrap();
        assert_eq!(first["path"], "/tmp/a.png");
    }

    #[test]
    fn does_not_invent_a_call_without_function_call() {
        let mut value = json!({
            "output": [{
                "type": "reasoning",
                "summary": [{ "type": "summary_text", "text": "Let me view frames with view_image." }]
            }]
        });
        let before = value.clone();
        assert!(!adapt_view_image_function_calls(&mut value));
        assert_eq!(value, before);
    }
}
