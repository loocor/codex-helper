use chrono::{DateTime, Utc};
use serde_json::Value;
use std::time::SystemTime;

pub(crate) fn thread_time_from_json(thread: &Value) -> Option<String> {
    thread_time_from_columns(
        thread.get("updated_at_ms").and_then(Value::as_i64),
        thread.get("updated_at").and_then(Value::as_i64),
        thread.get("created_at_ms").and_then(Value::as_i64),
        thread.get("created_at").and_then(Value::as_i64),
    )
}

pub(crate) fn thread_time_from_columns(
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

pub(crate) fn system_time_rfc3339(value: SystemTime) -> String {
    DateTime::<Utc>::from(value).to_rfc3339()
}

fn timestamp_ms_rfc3339(value: i64) -> Option<String> {
    DateTime::<Utc>::from_timestamp_millis(value).map(|date| date.to_rfc3339())
}
