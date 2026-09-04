use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use serde_json::{json, Map, Value};

const FUNCTION_CALL_NOTES_CAP: usize = 12;
const ITEM_TYPE_CAP: usize = 24;
const WALK_DEPTH_CAP: usize = 8;
const WALK_KEYS: [&str; 4] = ["output", "item", "response", "items"];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ArgumentsKind {
    Object,
    String,
    Missing,
    Other,
}

impl ArgumentsKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Object => "object",
            Self::String => "string",
            Self::Missing => "missing",
            Self::Other => "other",
        }
    }
}

#[derive(Clone, Debug)]
struct FunctionCallNote {
    name: String,
    call_id: Option<String>,
    arguments_kind: ArgumentsKind,
    rewritten: bool,
}

impl FunctionCallNote {
    fn to_json(&self) -> Value {
        json!({
            "name": self.name,
            "argumentsKind": self.arguments_kind.as_str(),
            "rewritten": self.rewritten,
        })
    }
}

#[derive(Clone, Debug, Default)]
pub struct ResponseCompatInventory {
    item_types: Vec<String>,
    function_calls: Vec<FunctionCallNote>,
    parse_errors: u64,
    truncated: bool,
    saw_reasoning: bool,
    saw_message: bool,
    saw_function_call: bool,
    arguments_object: bool,
    rewritten_function_call: bool,
}

impl ResponseCompatInventory {
    pub fn observe_value(&mut self, value: &Value) {
        self.walk_types(value);
        for call in collect_calls(value) {
            self.merge_call(call.into_note(false));
        }
    }

    pub fn observe_pair(&mut self, before: &Value, after: &Value) {
        self.walk_types(before);
        self.walk_types(after);
        for note in pair_calls(collect_calls(before), collect_calls(after)) {
            self.merge_call(note);
        }
    }

    pub fn observe_json_bytes(&mut self, bytes: &[u8]) {
        match serde_json::from_slice::<Value>(bytes) {
            Ok(value) => self.observe_value(&value),
            Err(_) => self.parse_errors += 1,
        }
    }

    pub fn observe_json_pair_bytes(&mut self, before: &[u8], after: &[u8]) {
        match (
            serde_json::from_slice::<Value>(before),
            serde_json::from_slice::<Value>(after),
        ) {
            (Ok(before), Ok(after)) => self.observe_pair(&before, &after),
            (Ok(before), Err(_)) => {
                self.observe_value(&before);
                self.parse_errors += 1;
            }
            (Err(_), Ok(after)) => {
                self.parse_errors += 1;
                self.observe_value(&after);
            }
            (Err(_), Err(_)) => self.parse_errors += 1,
        }
    }

    pub fn observe_sse_block(&mut self, block: &str) {
        self.apply_sse_payload(parse_sse_payload(block));
    }

    pub fn observe_sse_pair(&mut self, before_block: &str, after_bytes: &[u8]) {
        let after_block = String::from_utf8_lossy(after_bytes);
        match (
            parse_sse_payload(before_block),
            parse_sse_payload(after_block.as_ref()),
        ) {
            (SsePayload::Json(before), SsePayload::Json(after)) => {
                self.observe_pair(&before, &after);
            }
            (SsePayload::Json(before), after) => {
                self.observe_value(&before);
                self.apply_sse_payload(after);
            }
            (SsePayload::Skip, SsePayload::Json(after)) => self.observe_value(&after),
            (SsePayload::Skip, SsePayload::Skip) => {}
            (_, _) => self.parse_errors += 1,
        }
    }

    fn apply_sse_payload(&mut self, payload: SsePayload) {
        match payload {
            SsePayload::Skip => {}
            SsePayload::Invalid => self.parse_errors += 1,
            SsePayload::Json(value) => self.observe_value(&value),
        }
    }

    pub fn log_value(&self, status: u16, error: Option<&str>) -> Option<Value> {
        if !self.should_persist(status, error) {
            return None;
        }
        let (function_calls, calls_truncated) = self.selected_function_calls();
        Some(json!({
            "suspect": self.is_suspect(status, error),
            "reasons": self.reasons(status, error),
            "itemTypes": self.item_types,
            "functionCalls": function_calls,
            "parseErrors": self.parse_errors,
            "truncated": self.truncated || calls_truncated,
        }))
    }

    fn should_persist(&self, status: u16, error: Option<&str>) -> bool {
        self.is_suspect(status, error) || self.rewritten_function_call
    }

    fn is_suspect(&self, status: u16, error: Option<&str>) -> bool {
        error.is_some()
            || is_http_error_status(status)
            || self.parse_errors > 0
            || self.arguments_object
            || self.reasoning_without_output()
    }

    fn reasoning_without_output(&self) -> bool {
        self.saw_reasoning && !self.saw_message && !self.saw_function_call
    }

    fn reasons(&self, status: u16, error: Option<&str>) -> Vec<&'static str> {
        let mut reasons = Vec::new();
        if let Some(reason) = error.map(error_reason) {
            push_reason(&mut reasons, reason);
        }
        if is_http_error_status(status) {
            push_reason(&mut reasons, "http_status");
        }
        if self.parse_errors > 0 {
            push_reason(&mut reasons, "parse_error");
        }
        if self.arguments_object {
            push_reason(&mut reasons, "arguments_object");
        }
        if self.rewritten_function_call {
            push_reason(&mut reasons, "rewritten_function_call");
        }
        if self.reasoning_without_output() {
            push_reason(&mut reasons, "reasoning_without_output");
        }
        reasons
    }

    fn selected_function_calls(&self) -> (Vec<Value>, bool) {
        let mut chosen = Vec::new();
        let mut truncated = self.function_calls.len() > FUNCTION_CALL_NOTES_CAP;
        let mut consider = |pred: fn(&FunctionCallNote) -> bool| {
            for (index, call) in self.function_calls.iter().enumerate() {
                if chosen.len() >= FUNCTION_CALL_NOTES_CAP {
                    truncated = true;
                    return;
                }
                if pred(call) && !chosen.contains(&index) {
                    chosen.push(index);
                }
            }
        };
        consider(|call| call.arguments_kind == ArgumentsKind::Object);
        consider(|call| call.rewritten);
        consider(|_| true);
        let values = chosen
            .into_iter()
            .map(|index| self.function_calls[index].to_json())
            .collect();
        (values, truncated)
    }

    fn walk_types(&mut self, value: &Value) {
        walk_objects(value, 0, &mut |map| {
            if let Some(typ) = map.get("type").and_then(Value::as_str) {
                self.record_item_type(typ);
                self.classify_type(typ);
            }
        });
    }

    fn record_item_type(&mut self, typ: &str) {
        if self.item_types.iter().any(|existing| existing == typ) {
            return;
        }
        if self.item_types.len() >= ITEM_TYPE_CAP {
            self.truncated = true;
            return;
        }
        self.item_types.push(typ.to_string());
    }

    fn classify_type(&mut self, typ: &str) {
        if typ == "reasoning" || typ.starts_with("response.reasoning") {
            self.saw_reasoning = true;
        }
        if typ == "message" || typ == "output_text" || typ.starts_with("response.output_text") {
            self.saw_message = true;
        }
        if is_call_type(typ) {
            self.saw_function_call = true;
        }
    }

    fn merge_call(&mut self, note: FunctionCallNote) {
        if note.arguments_kind == ArgumentsKind::Object {
            self.arguments_object = true;
        }
        if note.rewritten {
            self.rewritten_function_call = true;
        }
        if let Some(existing) = self.existing_call_mut(note.call_id.as_deref()) {
            if existing.name.is_empty() {
                existing.name = note.name;
            }
            if note.arguments_kind == ArgumentsKind::Object {
                existing.arguments_kind = ArgumentsKind::Object;
            }
            if note.rewritten {
                existing.rewritten = true;
            }
            return;
        }
        self.function_calls.push(note);
    }

    fn existing_call_mut(&mut self, call_id: Option<&str>) -> Option<&mut FunctionCallNote> {
        let call_id = call_id?;
        self.function_calls
            .iter_mut()
            .find(|existing| existing.call_id.as_deref() == Some(call_id))
    }
}

#[derive(Clone, Debug)]
struct RawCall {
    name: String,
    call_id: Option<String>,
    item_type: String,
    arguments_kind: ArgumentsKind,
    fingerprint: u64,
}

impl RawCall {
    fn into_note(self, rewritten: bool) -> FunctionCallNote {
        FunctionCallNote {
            name: self.name,
            call_id: self.call_id,
            arguments_kind: self.arguments_kind,
            rewritten,
        }
    }

    fn differs_from(&self, before: &RawCall) -> bool {
        self.arguments_kind != before.arguments_kind
            || self.fingerprint != before.fingerprint
            || self.item_type != before.item_type
    }
}

enum SsePayload {
    Skip,
    Invalid,
    Json(Value),
}

fn is_success_status(status: u16) -> bool {
    (200..300).contains(&status)
}

fn is_http_error_status(status: u16) -> bool {
    status != 0 && !is_success_status(status)
}

fn error_reason(error: &str) -> &'static str {
    if error.contains("client disconnected") {
        "client_disconnect"
    } else if error.contains("did not complete") {
        "incomplete"
    } else if error.contains("upstream request failed") {
        "upstream_error"
    } else {
        "stream_error"
    }
}

fn push_reason(reasons: &mut Vec<&'static str>, reason: &'static str) {
    if !reasons.contains(&reason) {
        reasons.push(reason);
    }
}

fn parse_sse_payload(block: &str) -> SsePayload {
    let mut data_parts = Vec::new();
    for line in block.lines() {
        let line = line.strip_suffix('\r').unwrap_or(line);
        if let Some(data) = line.strip_prefix("data:") {
            data_parts.push(data.strip_prefix(' ').unwrap_or(data));
        }
    }
    if data_parts.is_empty() {
        return SsePayload::Skip;
    }
    let data = data_parts.join("\n");
    let trimmed = data.trim();
    if trimmed.is_empty() || trimmed == "[DONE]" {
        return SsePayload::Skip;
    }
    match serde_json::from_str::<Value>(trimmed) {
        Ok(value) => SsePayload::Json(value),
        Err(_) => SsePayload::Invalid,
    }
}

fn collect_calls(value: &Value) -> Vec<RawCall> {
    let mut out = Vec::new();
    walk_objects(value, 0, &mut |map| {
        if let Some(typ) = map.get("type").and_then(Value::as_str) {
            if is_call_type(typ) {
                out.push(raw_call(map, typ));
            }
        }
    });
    out
}

fn walk_objects(value: &Value, depth: usize, visit: &mut impl FnMut(&Map<String, Value>)) {
    if depth > WALK_DEPTH_CAP {
        return;
    }
    match value {
        Value::Array(items) => {
            for item in items {
                walk_objects(item, depth + 1, visit);
            }
        }
        Value::Object(map) => {
            visit(map);
            for key in WALK_KEYS {
                if let Some(child) = map.get(key) {
                    walk_objects(child, depth + 1, visit);
                }
            }
        }
        _ => {}
    }
}

fn is_call_type(typ: &str) -> bool {
    matches!(
        typ,
        "function_call"
            | "custom_tool_call"
            | "tool_search"
            | "tool_search_call"
            | "response.function_call_arguments.done"
            | "response.custom_tool_call_input.done"
    )
}

fn raw_call(map: &Map<String, Value>, typ: &str) -> RawCall {
    let arguments = map.get("arguments").or_else(|| map.get("input"));
    RawCall {
        name: map
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        call_id: map
            .get("call_id")
            .or_else(|| map.get("item_id"))
            .or_else(|| map.get("id"))
            .and_then(Value::as_str)
            .map(str::to_string),
        item_type: typ.to_string(),
        arguments_kind: arguments_kind(arguments),
        fingerprint: fingerprint(arguments),
    }
}

fn arguments_kind(value: Option<&Value>) -> ArgumentsKind {
    match value {
        None | Some(Value::Null) => ArgumentsKind::Missing,
        Some(Value::String(_)) => ArgumentsKind::String,
        Some(Value::Object(_)) => ArgumentsKind::Object,
        Some(_) => ArgumentsKind::Other,
    }
}

fn fingerprint(value: Option<&Value>) -> u64 {
    let mut hasher = DefaultHasher::new();
    match value {
        Some(value) => value.to_string().hash(&mut hasher),
        None => 0.hash(&mut hasher),
    }
    hasher.finish()
}

fn pair_calls(before: Vec<RawCall>, after: Vec<RawCall>) -> Vec<FunctionCallNote> {
    let mut used = vec![false; after.len()];
    let mut notes = Vec::with_capacity(before.len());
    for call in before {
        let rewritten = match match_after_call(&call, &after, &used) {
            Some(index) => {
                used[index] = true;
                after[index].differs_from(&call)
            }
            None => false,
        };
        notes.push(call.into_note(rewritten));
    }
    notes
}

fn match_after_call(call: &RawCall, after: &[RawCall], used: &[bool]) -> Option<usize> {
    if let Some(id) = call.call_id.as_ref() {
        if let Some(index) = after
            .iter()
            .position(|candidate| candidate.call_id.as_ref() == Some(id))
        {
            return Some(index);
        }
    }
    after.iter().enumerate().position(|(index, candidate)| {
        !used[index]
            && (candidate.name == call.name || call.name.is_empty() || candidate.name.is_empty())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn object_arguments_are_suspect_and_omit_values() {
        let mut inventory = ResponseCompatInventory::default();
        inventory.observe_value(&json!({
            "output": [{
                "type": "function_call",
                "name": "view_image",
                "call_id": "call_1",
                "arguments": { "path": "/secret/cursor.png" }
            }]
        }));
        let value = inventory.log_value(200, None).expect("compat");
        let rendered = value.to_string();
        assert_eq!(value["suspect"], true);
        assert_eq!(value["reasons"], json!(["arguments_object"]));
        assert_eq!(value["functionCalls"][0]["name"], "view_image");
        assert_eq!(value["functionCalls"][0]["argumentsKind"], "object");
        assert_eq!(value["functionCalls"][0]["rewritten"], false);
        assert!(!rendered.contains("/secret/cursor.png"));
        assert!(!rendered.contains("cursor.png"));
        assert!(!rendered.contains("path"));
    }

    #[test]
    fn rewritten_function_call_is_kept_as_success_sample() {
        let mut inventory = ResponseCompatInventory::default();
        inventory.observe_pair(
            &json!({
                "type": "function_call",
                "name": "exec",
                "call_id": "call_1",
                "arguments": { "yield_time_ms": 120000.0 }
            }),
            &json!({
                "type": "function_call",
                "name": "exec",
                "call_id": "call_1",
                "arguments": "{\"yield_time_ms\":120000}"
            }),
        );
        let value = inventory.log_value(200, None).expect("compat");
        assert_eq!(value["suspect"], true);
        assert_eq!(
            value["reasons"],
            json!(["arguments_object", "rewritten_function_call"])
        );
        assert_eq!(value["functionCalls"][0]["rewritten"], true);
        assert_eq!(value["functionCalls"][0]["argumentsKind"], "object");
        assert!(!value.to_string().contains("yield_time_ms"));
    }

    #[test]
    fn string_float_rewrite_is_not_suspect() {
        let mut inventory = ResponseCompatInventory::default();
        inventory.observe_pair(
            &json!({
                "type": "function_call",
                "name": "exec",
                "call_id": "call_1",
                "arguments": "{\"session_id\":92116.0}"
            }),
            &json!({
                "type": "function_call",
                "name": "exec",
                "call_id": "call_1",
                "arguments": "{\"session_id\":92116}"
            }),
        );
        let value = inventory.log_value(200, None).expect("compat");
        assert_eq!(value["suspect"], false);
        assert_eq!(value["reasons"], json!(["rewritten_function_call"]));
        assert_eq!(value["functionCalls"][0]["argumentsKind"], "string");
        assert_eq!(value["functionCalls"][0]["rewritten"], true);
        assert!(!value.to_string().contains("92116"));
    }

    #[test]
    fn reasoning_without_output_is_suspect_without_storing_payload() {
        let mut inventory = ResponseCompatInventory::default();
        inventory.observe_sse_block(
            r#"data: {"type":"reasoning","encrypted_content":"SECRET_REASONING"}"#,
        );
        let value = inventory.log_value(200, None).expect("compat");
        assert_eq!(value["suspect"], true);
        assert_eq!(value["reasons"], json!(["reasoning_without_output"]));
        assert_eq!(value["itemTypes"], json!(["reasoning"]));
        assert!(value["functionCalls"].as_array().unwrap().is_empty());
        assert!(!value.to_string().contains("SECRET_REASONING"));
    }

    #[test]
    fn clean_success_does_not_persist_compat() {
        let mut inventory = ResponseCompatInventory::default();
        inventory.observe_value(&json!({
            "output": [
                { "type": "message", "content": [{ "type": "output_text", "text": "hello" }] },
                { "type": "function_call", "name": "exec", "arguments": "{\"cmd\":\"ls\"}" }
            ]
        }));
        assert!(inventory.log_value(200, None).is_none());
    }

    #[test]
    fn sse_pair_records_object_stringify() {
        let mut inventory = ResponseCompatInventory::default();
        inventory.observe_sse_pair(
            r#"data: {"type":"function_call","name":"view_image","call_id":"c1","arguments":{"path":"/tmp/a.png"}}"#,
            br#"data: {"type":"function_call","name":"view_image","call_id":"c1","arguments":"{\"path\":\"/tmp/a.png\"}"}"#,
        );
        let value = inventory.log_value(200, None).expect("compat");
        assert_eq!(value["functionCalls"][0]["name"], "view_image");
        assert_eq!(value["functionCalls"][0]["argumentsKind"], "object");
        assert_eq!(value["functionCalls"][0]["rewritten"], true);
        assert!(!value.to_string().contains("/tmp/a.png"));
    }

    #[test]
    fn tool_search_restore_is_rewritten_sample() {
        let mut inventory = ResponseCompatInventory::default();
        inventory.observe_pair(
            &json!({
                "type": "function_call",
                "name": "tool_search",
                "call_id": "call_1",
                "arguments": "{\"query\":\"mail\"}"
            }),
            &json!({
                "type": "tool_search_call",
                "name": "tool_search",
                "call_id": "call_1",
                "arguments": { "query": "mail" }
            }),
        );
        let value = inventory.log_value(200, None).expect("compat");
        assert_eq!(value["suspect"], false);
        assert_eq!(value["reasons"], json!(["rewritten_function_call"]));
        assert_eq!(value["functionCalls"][0]["name"], "tool_search");
        assert_eq!(value["functionCalls"][0]["rewritten"], true);
        assert!(!value.to_string().contains("mail"));
    }

    #[test]
    fn http_error_without_calls_still_persists() {
        let inventory = ResponseCompatInventory::default();
        let value = inventory.log_value(500, None).expect("compat");
        assert_eq!(value["suspect"], true);
        assert_eq!(value["reasons"], json!(["http_status"]));
        assert!(value["functionCalls"].as_array().unwrap().is_empty());
    }
}
