//! Lightweight official usage display for Helper Settings.
//!
//! Live queries are provider-specific adapters:
//! - ChatGPT OAuth: remaining quota percent
//! - xAI Grok OAuth: remaining quota percent
//! - DeepSeek API key: remaining account balance (not a pie-friendly quota)
//! Other providers only expose an allowlisted official usage page URL.

use std::collections::HashMap;
use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::Context;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::codex_live::{auth_has_oauth_login, default_codex_home, read_auth};
use crate::provider_oauth::{oauth_bearer_token, oauth_is_signed_in, OAuthKind};
use crate::providers::{
    provider_device_oauth_kind, provider_is_deepseek, read_store, Provider, ProviderStore,
};

const CHATGPT_USAGE_URL: &str = "https://chatgpt.com/";
const CHATGPT_USAGE_API: &str = "https://chatgpt.com/backend-api/wham/usage";
const XAI_USAGE_URL: &str = "https://grok.com/?_s=usage";
const XAI_CONSOLE_URL: &str = "https://console.x.ai/";
const COPILOT_USAGE_URL: &str = "https://github.com/settings/copilot";
const DEEPSEEK_USAGE_URL: &str = "https://platform.deepseek.com/usage";
const MOONSHOT_USAGE_URL: &str = "https://platform.moonshot.cn/console";
const OPENROUTER_USAGE_URL: &str = "https://openrouter.ai/activity";
const GROK_BILLING_ENDPOINT: &str =
    "https://grok.com/grok_api_v2.GrokBuildBilling/GetGrokCreditsConfig";

pub fn usage_page_url(provider: &Provider) -> Option<String> {
    let stored = provider.usage_page_url.trim();
    if !stored.is_empty() {
        return Some(stored.to_string());
    }
    inferred_usage_page_url(provider).map(str::to_string)
}

fn inferred_usage_page_url(provider: &Provider) -> Option<&'static str> {
    if provider.id == "official" {
        return Some(CHATGPT_USAGE_URL);
    }
    match provider_device_oauth_kind(provider) {
        Some(OAuthKind::Xai) => return Some(XAI_USAGE_URL),
        Some(OAuthKind::GithubCopilot) => return Some(COPILOT_USAGE_URL),
        None => {}
    }
    let host = url_host(&provider.base_url)?;
    if host == "api.x.ai" || host.ends_with(".x.ai") || host == "x.ai" {
        return Some(XAI_CONSOLE_URL);
    }
    if host.contains("deepseek") {
        return Some(DEEPSEEK_USAGE_URL);
    }
    if host.contains("moonshot") || host.contains("kimi") {
        return Some(MOONSHOT_USAGE_URL);
    }
    if host.contains("openrouter") {
        return Some(OPENROUTER_USAGE_URL);
    }
    if host.contains("githubcopilot") {
        return Some(COPILOT_USAGE_URL);
    }
    None
}

fn http_usage_url(raw: &str) -> Option<String> {
    let url = raw.trim();
    if url.is_empty() {
        return None;
    }
    let lower = url.to_ascii_lowercase();
    if !(lower.starts_with("https://") || lower.starts_with("http://")) {
        return None;
    }
    if url.chars().any(|ch| ch.is_ascii_whitespace() || ch == '\0') {
        return None;
    }
    Some(url.to_string())
}

pub fn validated_usage_page_url(raw: &str) -> Result<String, String> {
    http_usage_url(raw).ok_or_else(|| "Usage URL must be an http or https link".to_string())
}

pub fn attach_usage_page_urls(response: &mut Value, store: &ProviderStore) {
    let Some(providers) = response.get_mut("providers").and_then(Value::as_array_mut) else {
        return;
    };
    for (provider, value) in store.providers.iter().zip(providers.iter_mut()) {
        if let Some(url) = usage_page_url(provider).and_then(|url| http_usage_url(&url)) {
            if let Some(object) = value.as_object_mut() {
                object.insert("usagePageUrl".to_string(), json!(url));
            }
        }
    }
}

pub fn usage_page_url_for_store(
    store: &ProviderStore,
    provider_id: &str,
) -> Result<String, String> {
    let provider = store
        .providers
        .iter()
        .find(|provider| provider.id == provider_id)
        .ok_or_else(|| format!("Unknown provider: {provider_id}"))?;
    let url = usage_page_url(provider)
        .ok_or_else(|| "No official usage page for this provider".to_string())?;
    http_usage_url(&url).ok_or_else(|| "Usage URL must be an http or https link".to_string())
}

pub async fn query_provider_usage(state_root: &Path, provider_id: &str) -> Value {
    let store = match read_store(state_root) {
        Ok(store) => store,
        Err(error) => return json!({ "status": "failed", "message": error.to_string() }),
    };
    let id = if provider_id.trim().is_empty() {
        store.active_id.clone()
    } else {
        provider_id.trim().to_string()
    };
    let Some(provider) = store.providers.iter().find(|provider| provider.id == id) else {
        return json!({ "status": "failed", "message": format!("Unknown provider: {id}") });
    };
    let page_url = usage_page_url(provider);
    match query_live_usage(state_root, provider).await {
        Ok(Some(live)) => {
            let mut response = json!({
                "status": "ok",
                "providerId": id,
                "pageUrl": page_url,
                "summary": live.summary,
            });
            if let Some(percent) = live.used_percent {
                response["usedPercent"] = json!(percent);
            }
            if let Some(resets_at) = live.resets_at {
                response["resetsAt"] = json!(resets_at);
            }
            response
        }
        Ok(None) => json!({
            "status": "ok",
            "providerId": id,
            "pageUrl": page_url,
        }),
        Err(error) => json!({
            "status": "failed",
            "providerId": id,
            "pageUrl": page_url,
            "message": error,
        }),
    }
}

struct LiveUsage {
    used_percent: Option<f64>,
    resets_at: Option<String>,
    summary: String,
}

async fn query_live_usage(
    state_root: &Path,
    provider: &Provider,
) -> Result<Option<LiveUsage>, String> {
    if provider.id == "official" {
        return query_chatgpt_usage().await;
    }
    if let Some(OAuthKind::Xai) = provider_device_oauth_kind(provider) {
        if oauth_is_signed_in(state_root, OAuthKind::Xai) {
            let token = oauth_bearer_token(state_root, OAuthKind::Xai)
                .await
                .map_err(|error| error.to_string())?;
            return query_xai_usage(&token).await.map(Some);
        }
    }
    if provider_is_deepseek(provider) {
        return query_deepseek_usage(provider).await.map(Some);
    }
    Ok(None)
}

async fn query_chatgpt_usage() -> Result<Option<LiveUsage>, String> {
    let home = default_codex_home();
    let Some(auth) = read_auth(&home).map_err(|error| error.to_string())? else {
        return Ok(None);
    };
    if !auth_has_oauth_login(&auth) {
        return Ok(None);
    }
    let tokens = auth.get("tokens").unwrap_or(&auth);
    let Some(access_token) = tokens
        .get("access_token")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|token| !token.is_empty())
    else {
        return Ok(None);
    };
    let account_id = tokens
        .get("account_id")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty());

    let client = http_client().map_err(|error| error.to_string())?;
    let mut request = client
        .get(CHATGPT_USAGE_API)
        .header("Authorization", format!("Bearer {access_token}"))
        .header("User-Agent", "codex-helper")
        .header("Accept", "application/json");
    if let Some(account_id) = account_id {
        request = request.header("ChatGPT-Account-Id", account_id);
    }
    let response = request.send().await.map_err(|error| error.to_string())?;
    let status = response.status();
    if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
        return Err(format!("ChatGPT usage query failed (HTTP {status})"));
    }
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(format!(
            "ChatGPT usage query failed (HTTP {status}): {body}"
        ));
    }
    let body: CodexUsageResponse = response
        .json()
        .await
        .map_err(|error| format!("ChatGPT usage response was not valid JSON: {error}"))?;
    live_usage_from_chatgpt(body)
        .ok_or_else(|| "ChatGPT usage response had no rate limit".to_string())
        .map(Some)
}

fn live_usage_from_chatgpt(body: CodexUsageResponse) -> Option<LiveUsage> {
    let limit = body.rate_limit?;
    let window = limit.primary_window.or(limit.secondary_window)?;
    let used_percent = window.used_percent?;
    let resets_at = window.reset_at.and_then(unix_ts_to_rfc3339);
    Some(LiveUsage {
        summary: usage_summary(used_percent, resets_at.as_deref()),
        used_percent: Some(used_percent),
        resets_at,
    })
}

#[derive(Debug, Deserialize)]
struct CodexRateLimitWindow {
    used_percent: Option<f64>,
    reset_at: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct CodexRateLimit {
    primary_window: Option<CodexRateLimitWindow>,
    secondary_window: Option<CodexRateLimitWindow>,
}

#[derive(Debug, Deserialize)]
struct CodexUsageResponse {
    rate_limit: Option<CodexRateLimit>,
}

async fn query_xai_usage(access_token: &str) -> Result<LiveUsage, String> {
    let client = http_client().map_err(|error| error.to_string())?;
    let response = client
        .post(GROK_BILLING_ENDPOINT)
        .header("Authorization", format!("Bearer {access_token}"))
        .header("Origin", "https://grok.com")
        .header("Referer", "https://grok.com/?_s=usage")
        .header("Accept", "*/*")
        .header("Content-Type", "application/grpc-web+proto")
        .header("x-grpc-web", "1")
        .header("x-user-agent", "connect-es/2.1.1")
        .header("User-Agent", "codex-helper")
        .body(vec![0u8; 5])
        .send()
        .await
        .map_err(|error| error.to_string())?;
    let status = response.status();
    if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
        return Err(format!("xAI usage query failed (HTTP {status})"));
    }
    if status == reqwest::StatusCode::REQUEST_TIMEOUT {
        return Err(format!("xAI usage query timed out (HTTP {status})"));
    }
    let header_status = response
        .headers()
        .get("grpc-status")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<i64>().ok());
    let header_message = response
        .headers()
        .get("grpc-message")
        .and_then(|value| value.to_str().ok())
        .map(percent_decode)
        .unwrap_or_default();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(format!("xAI usage query failed (HTTP {status}): {body}"));
    }
    if let Some(code) = header_status {
        if code != 0 {
            return Err(format!(
                "xAI usage query failed (grpc-status {code}): {header_message}"
            ));
        }
    }
    let raw = response
        .bytes()
        .await
        .map_err(|error| format!("Failed to read xAI usage response: {error}"))?;
    let trailers = grpc_web_trailer_fields(&raw);
    if let Some(code) = trailers
        .get("grpc-status")
        .and_then(|value| value.parse::<i64>().ok())
    {
        if code != 0 {
            let message = trailers.get("grpc-message").cloned().unwrap_or_default();
            return Err(format!(
                "xAI usage query failed (grpc-status {code}): {message}"
            ));
        }
    }
    let now_secs = now_secs();
    let snapshot = parse_billing_payload(&raw, now_secs)?;
    let resets_at = snapshot.resets_at.and_then(unix_ts_to_rfc3339);
    Ok(LiveUsage {
        summary: usage_summary(snapshot.used_percent, resets_at.as_deref()),
        used_percent: Some(snapshot.used_percent),
        resets_at,
    })
}

fn usage_summary(used_percent: f64, resets_at: Option<&str>) -> String {
    let used = format!("{:.0}% used", used_percent.clamp(0.0, 100.0));
    match resets_at.and_then(reset_label) {
        Some(label) => format!("{used} · {label}"),
        None => used,
    }
}

fn reset_label(resets_at: &str) -> Option<String> {
    let timestamp = chrono::DateTime::parse_from_rfc3339(resets_at)
        .ok()?
        .timestamp();
    let now = now_secs();
    let delta = timestamp.saturating_sub(now);
    if delta <= 0 {
        return Some("resets soon".to_string());
    }
    let hours = delta / 3600;
    if hours >= 48 {
        Some(format!("resets in {}d", (hours + 12) / 24))
    } else if hours >= 1 {
        Some(format!("resets in {hours}h"))
    } else {
        Some("resets soon".to_string())
    }
}

fn unix_ts_to_rfc3339(ts: i64) -> Option<String> {
    chrono::DateTime::from_timestamp(ts, 0).map(|dt| dt.to_rfc3339())
}

fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

async fn query_deepseek_usage(provider: &Provider) -> Result<LiveUsage, String> {
    let api_key = provider.api_key.trim();
    if api_key.is_empty() {
        return Err("DeepSeek API key is required".to_string());
    }
    let url = deepseek_balance_url(&provider.base_url)?;
    let client = http_client().map_err(|error| error.to_string())?;
    let response = client
        .get(&url)
        .header("Authorization", format!("Bearer {api_key}"))
        .header("Accept", "application/json")
        .send()
        .await
        .map_err(|error| format!("DeepSeek usage query failed: {error}"))?;
    let status = response.status();
    let body = response
        .text()
        .await
        .map_err(|error| format!("Failed to read DeepSeek usage response: {error}"))?;
    if !status.is_success() {
        return Err(format!(
            "DeepSeek usage query failed (HTTP {status}): {body}"
        ));
    }
    let parsed: DeepSeekBalanceResponse = serde_json::from_str(&body)
        .map_err(|error| format!("DeepSeek usage response was not valid JSON: {error}"))?;
    live_usage_from_deepseek(parsed)
}

fn deepseek_balance_url(base_url: &str) -> Result<String, String> {
    let trimmed = base_url.trim();
    if trimmed.is_empty() {
        return Err("DeepSeek base URL is required".to_string());
    }
    let (scheme, rest) = if let Some(rest) = trimmed.strip_prefix("https://") {
        ("https", rest)
    } else if let Some(rest) = trimmed.strip_prefix("http://") {
        ("http", rest)
    } else {
        return Err("DeepSeek base URL must be http or https".to_string());
    };
    let authority = rest
        .split(['/', '?', '#'])
        .next()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "DeepSeek base URL is missing a host".to_string())?;
    Ok(format!("{scheme}://{authority}/user/balance"))
}

#[derive(Debug, Deserialize)]
struct DeepSeekBalanceResponse {
    is_available: Option<bool>,
    balance_infos: Option<Vec<DeepSeekBalanceInfo>>,
}

#[derive(Debug, Deserialize)]
struct DeepSeekBalanceInfo {
    currency: Option<String>,
    total_balance: Option<String>,
    granted_balance: Option<String>,
    #[allow(dead_code)]
    topped_up_balance: Option<String>,
}

fn live_usage_from_deepseek(body: DeepSeekBalanceResponse) -> Result<LiveUsage, String> {
    let infos = body.balance_infos.unwrap_or_default();
    if infos.is_empty() {
        return Err("DeepSeek balance response had no balance_infos".to_string());
    }
    let parts: Vec<String> = infos.iter().filter_map(format_deepseek_balance).collect();
    if parts.is_empty() {
        return Err("DeepSeek balance response had no total_balance".to_string());
    }
    let mut summary = parts.join(" · ");
    if body.is_available == Some(false) {
        summary.push_str(" (unavailable)");
    }
    Ok(LiveUsage {
        used_percent: None,
        resets_at: None,
        summary,
    })
}

fn format_deepseek_balance(info: &DeepSeekBalanceInfo) -> Option<String> {
    let total = info.total_balance.as_deref()?.trim();
    if total.is_empty() {
        return None;
    }
    let currency = info.currency.as_deref().unwrap_or("").trim();
    let mut text = if currency.is_empty() {
        format!("{total} remaining")
    } else {
        format!("{currency} {total} remaining")
    };
    let granted = info
        .granted_balance
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty() && *value != "0" && *value != "0.00");
    if let Some(granted) = granted {
        text.push_str(" (granted ");
        text.push_str(granted);
        text.push(')');
    }
    Some(text)
}

fn http_client() -> anyhow::Result<reqwest::Client> {
    reqwest::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(15))
        .build()
        .context("Failed to build usage client")
}

fn url_host(base_url: &str) -> Option<String> {
    let trimmed = base_url.trim();
    let rest = trimmed
        .strip_prefix("https://")
        .or_else(|| trimmed.strip_prefix("http://"))?;
    let authority = rest
        .split(['/', '?', '#'])
        .next()
        .map(str::trim)
        .filter(|value| !value.is_empty() && !value.contains('@'))?;
    let host = if let Some(after) = authority.strip_prefix('[') {
        after.split(']').next()?
    } else {
        authority.split(':').next().unwrap_or(authority)
    };
    let host = host.trim().trim_end_matches('.').to_ascii_lowercase();
    (!host.is_empty()).then_some(host)
}

#[derive(Default)]
struct ProtobufScan {
    fixed32_fields: Vec<(Vec<u64>, f32, usize)>,
    varint_fields: Vec<(Vec<u64>, u64)>,
}

fn read_varint(bytes: &[u8], index: &mut usize) -> Option<u64> {
    let mut value: u64 = 0;
    let mut shift: u32 = 0;
    while *index < bytes.len() && shift < 64 {
        let byte = bytes[*index];
        *index += 1;
        value |= u64::from(byte & 0x7F) << shift;
        if byte & 0x80 == 0 {
            return Some(value);
        }
        shift += 7;
    }
    None
}

fn scan_protobuf(
    bytes: &[u8],
    depth: usize,
    path: &[u64],
    order: usize,
    scan: &mut ProtobufScan,
) -> usize {
    let mut index = 0;
    let mut next_order = order;
    while index < bytes.len() {
        let field_start = index;
        let key = match read_varint(bytes, &mut index) {
            Some(key) if key != 0 => key,
            _ => {
                index = field_start + 1;
                continue;
            }
        };
        let field_number = key >> 3;
        let wire_type = key & 0x07;
        let mut field_path = path.to_vec();
        field_path.push(field_number);
        match wire_type {
            0 => match read_varint(bytes, &mut index) {
                Some(value) => scan.varint_fields.push((field_path, value)),
                None => index = field_start + 1,
            },
            1 => {
                if index + 8 > bytes.len() {
                    return next_order;
                }
                index += 8;
            }
            2 => {
                let length = match read_varint(bytes, &mut index) {
                    Some(length) if length <= (bytes.len() - index) as u64 => length as usize,
                    _ => {
                        index = field_start + 1;
                        continue;
                    }
                };
                let end = index + length;
                if depth < 4 {
                    next_order =
                        scan_protobuf(&bytes[index..end], depth + 1, &field_path, next_order, scan);
                }
                index = end;
            }
            5 => {
                if index + 4 > bytes.len() {
                    return next_order;
                }
                let bits = u32::from_le_bytes([
                    bytes[index],
                    bytes[index + 1],
                    bytes[index + 2],
                    bytes[index + 3],
                ]);
                scan.fixed32_fields
                    .push((field_path, f32::from_bits(bits), next_order));
                next_order += 1;
                index += 4;
            }
            _ => index = field_start + 1,
        }
    }
    next_order
}

fn grpc_web_data_frames(data: &[u8]) -> Vec<&[u8]> {
    let mut frames = Vec::new();
    let mut index = 0;
    while index < data.len() {
        if index + 5 > data.len() {
            return Vec::new();
        }
        let flags = data[index];
        let length = u32::from_be_bytes([
            data[index + 1],
            data[index + 2],
            data[index + 3],
            data[index + 4],
        ]) as usize;
        let start = index + 5;
        let end = start + length;
        if end > data.len() {
            return Vec::new();
        }
        if flags & 0x80 == 0 {
            frames.push(&data[start..end]);
        }
        index = end;
    }
    frames
}

fn looks_like_protobuf_payload(data: &[u8]) -> bool {
    match data.first() {
        Some(&first) => {
            let field_number = first >> 3;
            let wire_type = first & 0x07;
            field_number > 0 && matches!(wire_type, 0 | 1 | 2 | 5)
        }
        None => false,
    }
}

fn grpc_web_trailer_fields(data: &[u8]) -> HashMap<String, String> {
    let mut fields = HashMap::new();
    let mut index = 0;
    while index + 5 <= data.len() {
        let flags = data[index];
        let length = u32::from_be_bytes([
            data[index + 1],
            data[index + 2],
            data[index + 3],
            data[index + 4],
        ]) as usize;
        let start = index + 5;
        let end = start + length;
        if end > data.len() {
            break;
        }
        if flags & 0x80 != 0 {
            if let Ok(text) = std::str::from_utf8(&data[start..end]) {
                for line in text.lines().filter(|line| !line.is_empty()) {
                    if let Some((key, value)) = line.split_once(':') {
                        fields.insert(key.trim().to_lowercase(), percent_decode(value.trim()));
                    }
                }
            }
        }
        index = end;
    }
    fields
}

fn percent_decode(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(hex) = std::str::from_utf8(&bytes[i + 1..i + 3]) {
                if let Ok(byte) = u8::from_str_radix(hex, 16) {
                    out.push(byte);
                    i += 3;
                    continue;
                }
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

struct GrokBillingSnapshot {
    used_percent: f64,
    resets_at: Option<i64>,
}

fn parse_billing_payload(data: &[u8], now_secs: i64) -> Result<GrokBillingSnapshot, String> {
    let mut payloads = grpc_web_data_frames(data);
    if payloads.is_empty() && looks_like_protobuf_payload(data) {
        payloads = vec![data];
    }
    if payloads.is_empty() {
        return Err("xAI usage response contained no protobuf payload".to_string());
    }
    let mut scan = ProtobufScan::default();
    for payload in payloads {
        scan_protobuf(payload, 0, &[], 0, &mut scan);
    }
    let parsed_percent = scan
        .fixed32_fields
        .iter()
        .filter(|(path, value, _)| {
            path.last() == Some(&1) && value.is_finite() && *value >= 0.0 && *value <= 100.0
        })
        .min_by_key(|(path, _, order)| (path.len(), *order))
        .map(|(_, value, _)| f64::from(*value));
    let reset_candidates: Vec<(&[u64], i64)> = scan
        .varint_fields
        .iter()
        .filter(|(_, value)| (1_700_000_000..=2_100_000_000).contains(value))
        .map(|(path, value)| (path.as_slice(), *value as i64))
        .filter(|(_, ts)| *ts > now_secs)
        .collect();
    let reset = reset_candidates
        .iter()
        .filter(|(path, _)| *path == [1, 5, 1])
        .map(|(_, ts)| *ts)
        .min()
        .or_else(|| reset_candidates.iter().map(|(_, ts)| *ts).min());
    let has_usage_period = scan.varint_fields.iter().any(|(path, value)| {
        path.starts_with(&[1, 6]) || (path.as_slice() == [1, 8, 1] && (*value == 1 || *value == 2))
    });
    let no_usage_yet = parsed_percent.is_none()
        && scan.fixed32_fields.is_empty()
        && reset.is_some()
        && has_usage_period;
    let used_percent = parsed_percent
        .or(if no_usage_yet { Some(0.0) } else { None })
        .ok_or_else(|| "Could not locate usage percent in xAI billing response".to_string())?;
    Ok(GrokBillingSnapshot {
        used_percent,
        resets_at: reset,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::ProviderKind;

    fn provider(id: &str, compat: &str, base_url: &str) -> Provider {
        Provider {
            id: id.to_string(),
            name: id.to_string(),
            kind: if id == "official" {
                ProviderKind::Oauth
            } else {
                ProviderKind::ApiKey
            },
            base_url: base_url.to_string(),
            compat: compat.to_string(),
            ..Provider::default()
        }
    }

    #[test]
    fn maps_known_usage_pages() {
        assert_eq!(
            usage_page_url(&provider("official", "", "")).as_deref(),
            Some(CHATGPT_USAGE_URL)
        );
        assert_eq!(
            usage_page_url(&provider("grok", "xai_oauth", "https://api.x.ai/v1")).as_deref(),
            Some(XAI_USAGE_URL)
        );
        assert_eq!(
            usage_page_url(&provider(
                "copilot",
                "github_copilot",
                "https://api.githubcopilot.com"
            ))
            .as_deref(),
            Some(COPILOT_USAGE_URL)
        );
        assert_eq!(
            usage_page_url(&provider("xai-key", "", "https://api.x.ai/v1")).as_deref(),
            Some(XAI_CONSOLE_URL)
        );
        assert_eq!(
            usage_page_url(&provider("ds", "", "https://api.deepseek.com/v1")).as_deref(),
            Some(DEEPSEEK_USAGE_URL)
        );
        assert_eq!(
            usage_page_url(&provider("kimi", "", "https://api.moonshot.cn/v1")).as_deref(),
            Some(MOONSHOT_USAGE_URL)
        );
        assert_eq!(
            usage_page_url(&provider("kimi-intl", "", "https://api.kimi.com/v1")).as_deref(),
            Some(MOONSHOT_USAGE_URL)
        );
        assert_eq!(
            usage_page_url(&provider("custom", "", "https://api.example.com/v1")),
            None
        );
        let mut custom = provider("custom", "", "https://api.example.com/v1");
        custom.usage_page_url = "https://status.example.com/usage".to_string();
        assert_eq!(
            usage_page_url(&custom).as_deref(),
            Some("https://status.example.com/usage")
        );
    }

    #[test]
    fn validated_usage_page_url_accepts_http_links_only() {
        assert_eq!(
            validated_usage_page_url("https://platform.deepseek.com/usage").unwrap(),
            "https://platform.deepseek.com/usage"
        );
        assert!(validated_usage_page_url("").is_err());
        assert!(validated_usage_page_url("javascript:alert(1)").is_err());
    }

    #[test]
    fn chatgpt_primary_window_becomes_summary() {
        let live = live_usage_from_chatgpt(CodexUsageResponse {
            rate_limit: Some(CodexRateLimit {
                primary_window: Some(CodexRateLimitWindow {
                    used_percent: Some(42.4),
                    reset_at: Some(1_800_000_000),
                }),
                secondary_window: None,
            }),
        })
        .expect("usage");
        assert_eq!(live.used_percent, Some(42.4));
        assert!(live.summary.starts_with("42% used"), "{}", live.summary);
    }

    fn varint(mut value: u64) -> Vec<u8> {
        let mut out = Vec::new();
        loop {
            let byte = (value & 0x7F) as u8;
            value >>= 7;
            if value == 0 {
                out.push(byte);
                break;
            }
            out.push(byte | 0x80);
        }
        out
    }

    fn field_varint(number: u64, value: u64) -> Vec<u8> {
        let mut out = varint(number << 3);
        out.extend(varint(value));
        out
    }

    fn field_float(number: u64, value: f32) -> Vec<u8> {
        let mut out = varint((number << 3) | 5);
        out.extend(value.to_bits().to_le_bytes());
        out
    }

    fn field_message(number: u64, payload: &[u8]) -> Vec<u8> {
        let mut out = varint((number << 3) | 2);
        out.extend(varint(payload.len() as u64));
        out.extend(payload);
        out
    }

    fn grpc_web_frame(flags: u8, payload: &[u8]) -> Vec<u8> {
        let mut out = vec![flags];
        out.extend((payload.len() as u32).to_be_bytes());
        out.extend(payload);
        out
    }

    const NOW: i64 = 1_750_000_000;

    #[test]
    fn parses_percent_and_reset_from_framed_payload() {
        let reset_ts = (NOW + 30 * 86400) as u64;
        let inner = [
            field_float(1, 37.5),
            field_message(5, &field_varint(1, reset_ts)),
        ]
        .concat();
        let payload = field_message(1, &inner);
        let data = grpc_web_frame(0, &payload);
        let snapshot = parse_billing_payload(&data, NOW).expect("parse ok");
        assert_eq!(snapshot.used_percent, 37.5);
        assert_eq!(snapshot.resets_at, Some(reset_ts as i64));
    }

    #[test]
    fn zero_usage_period_without_percent_field_reads_as_zero() {
        let reset_ts = (NOW + 7 * 86400) as u64;
        let inner = [
            field_message(5, &field_varint(1, reset_ts)),
            field_message(6, &field_varint(1, 3)),
        ]
        .concat();
        let payload = field_message(1, &inner);
        let data = grpc_web_frame(0, &payload);
        let snapshot = parse_billing_payload(&data, NOW).expect("parse ok");
        assert_eq!(snapshot.used_percent, 0.0);
        assert_eq!(snapshot.resets_at, Some(reset_ts as i64));
    }

    #[test]
    fn deepseek_balance_url_strips_v1_suffix() {
        assert_eq!(
            deepseek_balance_url("https://api.deepseek.com/v1").unwrap(),
            "https://api.deepseek.com/user/balance"
        );
        assert_eq!(
            deepseek_balance_url("http://127.0.0.1:9000/v1").unwrap(),
            "http://127.0.0.1:9000/user/balance"
        );
    }

    #[test]
    fn deepseek_balance_becomes_remaining_summary_without_percent() {
        let live = live_usage_from_deepseek(DeepSeekBalanceResponse {
            is_available: Some(true),
            balance_infos: Some(vec![DeepSeekBalanceInfo {
                currency: Some("CNY".to_string()),
                total_balance: Some("110.00".to_string()),
                granted_balance: Some("10.00".to_string()),
                topped_up_balance: Some("100.00".to_string()),
            }]),
        })
        .expect("usage");
        assert_eq!(live.used_percent, None);
        assert_eq!(live.summary, "CNY 110.00 remaining (granted 10.00)");
    }

    #[test]
    fn deepseek_unavailable_balance_is_marked() {
        let live = live_usage_from_deepseek(DeepSeekBalanceResponse {
            is_available: Some(false),
            balance_infos: Some(vec![DeepSeekBalanceInfo {
                currency: Some("USD".to_string()),
                total_balance: Some("0.00".to_string()),
                granted_balance: Some("0.00".to_string()),
                topped_up_balance: Some("0.00".to_string()),
            }]),
        })
        .expect("usage");
        assert_eq!(live.summary, "USD 0.00 remaining (unavailable)");
    }
}
