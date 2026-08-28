//! Shared Codex `custom` tool <-> function rewrite.
//!
//! Provider sanitizers own policy. This module only does the round-trip:
//! named `custom` tools become functions, history items follow, and the
//! return path restores `custom_tool_call` for rewritten names.
//!
//! xAI rewrites every named custom tool (`keep_native` empty). DeepSeek
//! keeps native `apply_patch` and rewrites the rest (especially `exec`).

use std::collections::HashSet;

use serde_json::{json, Map, Value};

const CUSTOM_TOOL_TYPE: &str = "custom";
const CUSTOM_TOOL_CALL_TYPE: &str = "custom_tool_call";
const CUSTOM_TOOL_OUTPUT_TYPE: &str = "custom_tool_call_output";
const CUSTOM_TOOL_INPUT_FIELD: &str = "input";
const CUSTOM_TOOL_INPUT_DESCRIPTION: &str = "Raw string input for the original custom tool. Preserve formatting exactly and follow the original tool definition embedded in the description.";
const CUSTOM_TOOL_PRESERVED_METADATA_HEADING: &str = "Original tool definition:";

fn json_type(value: &Value) -> Option<&str> {
    value.get("type").and_then(Value::as_str).map(str::trim)
}

fn json_compact_string(value: &Value) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "{}".to_string())
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

fn keep_native_name(name: &str, keep_native: &HashSet<String>) -> bool {
    keep_native.iter().any(|kept| kept == name)
}

fn custom_as_function(tool: &Value) -> Option<Value> {
    let name = function_tool_name(tool)?;
    Some(json!({
        "type": "function",
        "name": name,
        "description": format!(
            "{heading}\n```json\n{definition}\n```",
            heading = CUSTOM_TOOL_PRESERVED_METADATA_HEADING,
            definition = json_compact_string(tool)
        ),
        "parameters": {
            "type": "object",
            "properties": {
                CUSTOM_TOOL_INPUT_FIELD: {
                    "type": "string",
                    "description": CUSTOM_TOOL_INPUT_DESCRIPTION
                }
            },
            "required": [CUSTOM_TOOL_INPUT_FIELD]
        }
    }))
}

fn rewrite_custom_tool_choice(body: &mut Value, keep_native: &HashSet<String>) -> bool {
    let Some(choice) = body.get_mut("tool_choice") else {
        return false;
    };
    if json_type(choice) != Some(CUSTOM_TOOL_TYPE) {
        return false;
    }
    let Some(name) = choice
        .get("name")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|name| !name.is_empty())
    else {
        return false;
    };
    if keep_native_name(name, keep_native) {
        return false;
    }
    *choice = json!({ "type": "function", "name": name });
    true
}

/// Rewrite named `custom` tools into functions, leaving `keep_native` names
/// untouched. Unnamed custom tools are left for the provider filter.
pub fn rewrite_custom_as_function(body: &mut Value, keep_native: &HashSet<String>) -> bool {
    let mut changed = false;
    if let Some(tools) = body.get("tools").and_then(Value::as_array).cloned() {
        let existing_function_names: HashSet<String> = tools
            .iter()
            .filter_map(|tool| {
                if json_type(tool) == Some("function") {
                    function_tool_name(tool).map(str::to_string)
                } else {
                    None
                }
            })
            .collect();
        let mut next = Vec::with_capacity(tools.len());
        let mut rewritten = Vec::new();
        for tool in tools {
            if json_type(&tool) == Some(CUSTOM_TOOL_TYPE) {
                let name = function_tool_name(&tool).unwrap_or("");
                if !name.is_empty() && keep_native_name(name, keep_native) {
                    next.push(tool);
                    continue;
                }
                if let Some(function) = custom_as_function(&tool) {
                    let function_name = function_tool_name(&function).unwrap_or("");
                    if !function_name.is_empty() && !existing_function_names.contains(function_name)
                    {
                        rewritten.push(function);
                    }
                    changed = true;
                    continue;
                }
            }
            next.push(tool);
        }
        if changed {
            next.extend(rewritten);
            if let Some(obj) = body.as_object_mut() {
                obj.insert("tools".to_string(), Value::Array(next));
            }
        }
    }
    changed |= rewrite_custom_tool_choice(body, keep_native);
    changed
}

fn call_id_string(item: &Map<String, Value>) -> Option<String> {
    item.get("call_id")
        .or_else(|| item.get("id"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn custom_tool_call_input_string(item: &Map<String, Value>) -> String {
    match item.get(CUSTOM_TOOL_INPUT_FIELD) {
        Some(Value::String(text)) => text.clone(),
        Some(value) => json_compact_string(value),
        None => String::new(),
    }
}

fn rewrite_custom_tool_call_input_item(item: &mut Value) -> bool {
    let Some(obj) = item.as_object_mut() else {
        return false;
    };
    if obj.get("type").and_then(Value::as_str).map(str::trim) != Some(CUSTOM_TOOL_CALL_TYPE) {
        return false;
    }
    let name = obj
        .get("name")
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or("")
        .to_string();
    let call_id = obj
        .get("call_id")
        .cloned()
        .or_else(|| obj.get("id").cloned());
    let id = obj.get("id").cloned();
    let status = obj.get("status").cloned();
    let input = custom_tool_call_input_string(obj);
    let arguments = json_compact_string(&json!({ CUSTOM_TOOL_INPUT_FIELD: input }));
    let mut next = Map::new();
    next.insert("type".to_string(), json!("function_call"));
    next.insert("name".to_string(), json!(name));
    next.insert("arguments".to_string(), json!(arguments));
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

fn rewrite_custom_tool_output_input_item(item: &mut Value) -> bool {
    let Some(obj) = item.as_object_mut() else {
        return false;
    };
    if obj.get("type").and_then(Value::as_str).map(str::trim) != Some(CUSTOM_TOOL_OUTPUT_TYPE) {
        return false;
    }
    let call_id = obj
        .get("call_id")
        .cloned()
        .or_else(|| obj.get("id").cloned());
    let output = match obj.get("output") {
        Some(Value::String(text)) => Value::String(text.clone()),
        Some(value) => Value::String(json_compact_string(value)),
        None => Value::String(String::new()),
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

fn collect_native_custom_call_ids(
    value: &Value,
    keep_native: &HashSet<String>,
    native_call_ids: &mut HashSet<String>,
) {
    match value {
        Value::Array(items) => {
            for item in items {
                collect_native_custom_call_ids(item, keep_native, native_call_ids);
            }
        }
        Value::Object(obj) => {
            if obj.get("type").and_then(Value::as_str).map(str::trim) == Some(CUSTOM_TOOL_CALL_TYPE)
            {
                let name = obj
                    .get("name")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .unwrap_or("");
                if keep_native_name(name, keep_native) {
                    if let Some(call_id) = call_id_string(obj) {
                        native_call_ids.insert(call_id);
                    }
                }
            }
            for child in obj.values() {
                collect_native_custom_call_ids(child, keep_native, native_call_ids);
            }
        }
        _ => {}
    }
}

fn rewrite_custom_input_value(
    value: &mut Value,
    keep_native: &HashSet<String>,
    native_call_ids: &HashSet<String>,
) -> bool {
    if let Some(obj) = value.as_object() {
        let kind = obj
            .get("type")
            .and_then(Value::as_str)
            .map(str::trim)
            .unwrap_or("");
        if kind == CUSTOM_TOOL_CALL_TYPE {
            let name = obj
                .get("name")
                .and_then(Value::as_str)
                .map(str::trim)
                .unwrap_or("");
            if keep_native_name(name, keep_native) {
                return false;
            }
            return rewrite_custom_tool_call_input_item(value);
        }
        if kind == CUSTOM_TOOL_OUTPUT_TYPE {
            let call_id = call_id_string(obj);
            if call_id
                .as_deref()
                .is_some_and(|id| native_call_ids.contains(id))
            {
                return false;
            }
            return rewrite_custom_tool_output_input_item(value);
        }
    }
    let mut changed = false;
    match value {
        Value::Array(items) => {
            for item in items {
                changed |= rewrite_custom_input_value(item, keep_native, native_call_ids);
            }
        }
        Value::Object(obj) => {
            for child in obj.values_mut() {
                changed |= rewrite_custom_input_value(child, keep_native, native_call_ids);
            }
        }
        _ => {}
    }
    changed
}

/// Rewrite history `custom_tool_call` / `custom_tool_call_output` items.
/// Calls whose names are in `keep_native` stay native, and their outputs stay
/// with them. Every other custom history item becomes a function item.
pub fn rewrite_custom_input_items(body: &mut Value, keep_native: &HashSet<String>) -> bool {
    let Some(input) = body.get_mut("input") else {
        return false;
    };
    let mut native_call_ids = HashSet::new();
    collect_native_custom_call_ids(input, keep_native, &mut native_call_ids);
    rewrite_custom_input_value(input, keep_native, &native_call_ids)
}

fn collect_custom_tool_names(value: &Value, names: &mut HashSet<String>) {
    match value {
        Value::Array(items) => {
            for item in items {
                collect_custom_tool_names(item, names);
            }
        }
        Value::Object(obj) => {
            let kind = obj.get("type").and_then(Value::as_str).map(str::trim);
            if kind == Some(CUSTOM_TOOL_TYPE) || kind == Some(CUSTOM_TOOL_CALL_TYPE) {
                if let Some(name) = function_tool_name(value) {
                    names.insert(name.to_string());
                }
            }
            for child in obj.values() {
                collect_custom_tool_names(child, names);
            }
        }
        _ => {}
    }
}

/// Names of custom tools/calls that will be rewritten (not in `keep_native`).
pub fn custom_tool_names_from_request(
    body: &Value,
    keep_native: &HashSet<String>,
) -> HashSet<String> {
    let mut names = HashSet::new();
    collect_custom_tool_names(body, &mut names);
    names.retain(|name| !keep_native_name(name, keep_native));
    names
}

fn custom_tool_input_from_arguments(arguments: &Value) -> String {
    match arguments {
        Value::String(raw) => match serde_json::from_str::<Value>(raw) {
            Ok(Value::Object(obj)) => obj
                .get(CUSTOM_TOOL_INPUT_FIELD)
                .and_then(Value::as_str)
                .unwrap_or(raw)
                .to_string(),
            _ => raw.clone(),
        },
        Value::Object(obj) => obj
            .get(CUSTOM_TOOL_INPUT_FIELD)
            .and_then(Value::as_str)
            .map(str::to_string)
            .unwrap_or_else(|| json_compact_string(arguments)),
        other => json_compact_string(other),
    }
}

fn restore_custom_tool_call_item(item: &mut Value, custom_names: &HashSet<String>) -> bool {
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
        .unwrap_or("")
        .to_string();
    if name.is_empty() || !custom_names.contains(&name) {
        return false;
    }
    let call_id = obj
        .get("call_id")
        .cloned()
        .or_else(|| obj.get("id").cloned());
    let id = obj.get("id").cloned();
    let status = obj.get("status").cloned();
    let input = obj
        .get("arguments")
        .map(custom_tool_input_from_arguments)
        .unwrap_or_default();
    obj.insert("type".to_string(), json!(CUSTOM_TOOL_CALL_TYPE));
    obj.insert("name".to_string(), json!(name));
    obj.insert("input".to_string(), json!(input));
    obj.remove("arguments");
    if let Some(call_id) = call_id {
        obj.insert("call_id".to_string(), call_id);
    }
    if let Some(id) = id {
        obj.insert("id".to_string(), id);
    }
    if let Some(status) = status {
        obj.insert("status".to_string(), status);
    }
    true
}

pub fn restore_custom_tool_calls(value: &mut Value, custom_names: &HashSet<String>) -> bool {
    if custom_names.is_empty() {
        return false;
    }
    let mut changed = restore_custom_tool_call_item(value, custom_names);
    match value {
        Value::Array(items) => {
            for item in items {
                changed |= restore_custom_tool_calls(item, custom_names);
            }
        }
        Value::Object(obj) => {
            for child in obj.values_mut() {
                changed |= restore_custom_tool_calls(child, custom_names);
            }
        }
        _ => {}
    }
    changed
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn keep_apply_patch() -> HashSet<String> {
        let mut names = HashSet::new();
        names.insert("apply_patch".to_string());
        names
    }

    #[test]
    fn rewrites_exec_and_keeps_apply_patch() {
        let mut body = json!({
            "tools": [
                { "type": "custom", "name": "exec" },
                { "type": "custom", "name": "apply_patch" },
                { "type": "function", "name": "read_file" }
            ],
            "tool_choice": { "type": "custom", "name": "exec" }
        });
        assert!(rewrite_custom_as_function(&mut body, &keep_apply_patch()));
        let tools = body["tools"].as_array().unwrap();
        assert_eq!(tools.len(), 3);
        assert_eq!(tools[0]["type"], "custom");
        assert_eq!(tools[0]["name"], "apply_patch");
        assert_eq!(tools[1]["type"], "function");
        assert_eq!(tools[1]["name"], "read_file");
        assert_eq!(tools[2]["type"], "function");
        assert_eq!(tools[2]["name"], "exec");
        assert_eq!(
            body["tool_choice"],
            json!({ "type": "function", "name": "exec" })
        );
    }

    #[test]
    fn history_rewrites_exec_and_keeps_apply_patch_pair() {
        let mut body = json!({
            "input": [
                {
                    "type": "custom_tool_call",
                    "call_id": "call_exec",
                    "name": "exec",
                    "input": "ls"
                },
                {
                    "type": "custom_tool_call_output",
                    "call_id": "call_exec",
                    "output": "ok"
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
                    "output": "applied"
                }
            ]
        });
        assert!(rewrite_custom_input_items(&mut body, &keep_apply_patch()));
        assert_eq!(body["input"][0]["type"], "function_call");
        assert_eq!(body["input"][0]["name"], "exec");
        assert_eq!(body["input"][1]["type"], "function_call_output");
        assert_eq!(body["input"][2]["type"], "custom_tool_call");
        assert_eq!(body["input"][2]["name"], "apply_patch");
        assert_eq!(body["input"][3]["type"], "custom_tool_call_output");
        assert_eq!(body["input"][3]["output"], "applied");
    }
}
