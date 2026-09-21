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
use crate::provider_oauth::{
    copilot_github_token, copilot_request_headers, oauth_bearer_token, oauth_is_signed_in,
    OAuthKind,
};
use crate::providers::{
    provider_device_oauth_kind, provider_is_bigmodel, provider_is_deepseek, provider_is_kimi,
    provider_is_minimax, read_store, Provider, ProviderStore,
};

const CHATGPT_USAGE_URL: &str = "https://chatgpt.com/";
const CHATGPT_USAGE_API: &str = "https://chatgpt.com/backend-api/wham/usage";
const XAI_USAGE_URL: &str = "https://grok.com/?_s=usage";
const XAI_CONSOLE_URL: &str = "https://console.x.ai/";
const COPILOT_USAGE_URL: &str = "https://github.com/settings/copilot";
const DEEPSEEK_USAGE_URL: &str = "https://platform.deepseek.com/usage";
const MOONSHOT_USAGE_URL: &str = "https://platform.moonshot.cn/console";
const OPENROUTER_USAGE_URL: &str = "https://openrouter.ai/activity";
const BIGMODEL_USAGE_PAGE_URL: &str = "https://bigmodel.cn/coding-plan/personal/usage";
const BIGMODEL_USAGE_API: &str = "https://open.bigmodel.cn/api/monitor/usage/quota/limit";
const MINIMAX_USAGE_PAGE_URL: &str = "https://platform.minimax.cn";
const MINIMAX_USAGE_API_CN: &str =
    "https://api.minimaxi.com/v1/api/openplatform/coding_plan/remains";
const MINIMAX_USAGE_API_INTL: &str =
    "https://api.minimax.io/v1/api/openplatform/coding_plan/remains";
const KIMI_CODING_USAGE_API: &str = "https://api.kimi.com/coding/v1/usages";
const KIMI_PLATFORM_BALANCE_API: &str = "https://api.moonshot.cn/v1/users/me/balance";
const COPILOT_USAGE_API: &str = "https://api.github.com/copilot_internal/user";
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
    if host.contains("bigmodel") {
        return Some(BIGMODEL_USAGE_PAGE_URL);
    }
    if host.contains("minimax") {
        return Some(MINIMAX_USAGE_PAGE_URL);
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

#[derive(Debug)]
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
    if provider_is_bigmodel(provider) {
        return query_bigmodel_usage(provider).await.map(Some);
    }
    if provider_is_minimax(provider) {
        return query_minimax_usage(provider).await.map(Some);
    }
    if provider_is_kimi(provider) {
        return query_kimi_usage(provider).await.map(Some);
    }
    if let Some(OAuthKind::GithubCopilot) = provider_device_oauth_kind(provider) {
        if oauth_is_signed_in(state_root, OAuthKind::GithubCopilot) {
            let token = copilot_github_token(state_root)
                .await
                .map_err(|error| error.to_string())?;
            return query_copilot_usage(&token).await.map(Some);
        }
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

async fn query_bigmodel_usage(provider: &Provider) -> Result<LiveUsage, String> {
    let api_key = provider.api_key.trim();
    if api_key.is_empty() {
        return Err("BigModel API key is required".to_string());
    }
    let client = http_client().map_err(|error| error.to_string())?;
    let response = client
        .get(BIGMODEL_USAGE_API)
        .header("Authorization", api_key)
        .header("Accept", "application/json")
        .header("Accept-Language", "en-US,en")
        .send()
        .await
        .map_err(|error| format!("BigModel usage query failed: {error}"))?;
    let status = response.status();
    let body = response
        .text()
        .await
        .map_err(|error| format!("Failed to read BigModel usage response: {error}"))?;
    if !status.is_success() {
        return Err(format!(
            "BigModel usage query failed (HTTP {status}): {body}"
        ));
    }
    let parsed: BigModelUsageResponse = serde_json::from_str(&body)
        .map_err(|error| format!("BigModel usage response was not valid JSON: {error}"))?;
    live_usage_from_bigmodel(parsed)
}

#[derive(Debug, Deserialize)]
struct BigModelUsageResponse {
    code: Option<i64>,
    msg: Option<String>,
    success: Option<bool>,
    data: Option<BigModelUsageData>,
}

#[derive(Debug, Deserialize)]
struct BigModelUsageData {
    #[allow(dead_code)]
    level: Option<String>,
    limits: Option<Vec<BigModelUsageLimit>>,
}

#[derive(Debug, Deserialize)]
struct BigModelUsageLimit {
    #[serde(rename = "type")]
    limit_type: Option<String>,
    percentage: Option<f64>,
    unit: Option<i64>,
    current_value: Option<f64>,
    usage: Option<f64>,
    next_reset_time: Option<Value>,
}

impl BigModelUsageLimit {
    fn percentage(&self) -> f64 {
        self.percentage.unwrap_or(0.0)
    }

    /// Official usage page shows consumed credits over the window total
    /// (`currentValue` / `usage`).
    fn credits(&self) -> Option<(f64, f64)> {
        match (self.current_value, self.usage) {
            (Some(current), Some(total)) if total > 0.0 => Some((current, total)),
            _ => None,
        }
    }
}

enum BigModelWindow {
    FiveHour,
    Weekly,
}

impl BigModelWindow {
    /// `unit: 3` marks the 5-hour rolling window and `unit: 6` the weekly
    /// window. Reset times cannot classify the buckets: near the end of a
    /// week the weekly window can reset before the rolling one.
    fn from_limit(limit: &BigModelUsageLimit) -> Option<Self> {
        match limit.unit {
            Some(3) => Some(Self::FiveHour),
            Some(6) => Some(Self::Weekly),
            _ => None,
        }
    }
}

fn is_bigmodel_token_quota(kind: &str) -> bool {
    kind.eq_ignore_ascii_case("TOKENS_LIMIT") || kind.eq_ignore_ascii_case("CREDIT_LIMIT")
}

fn bigmodel_rejection_message(body: &BigModelUsageResponse) -> String {
    let message = body
        .msg
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    match (body.code, message) {
        (Some(code), Some(message)) => {
            format!("BigModel usage query failed (code {code}): {message}")
        }
        (Some(code), None) => format!("BigModel usage query failed (code {code})"),
        (None, Some(message)) => format!("BigModel usage query failed: {message}"),
        (None, None) => "BigModel usage query failed".to_string(),
    }
}

fn bigmodel_reset_secs(value: &Option<Value>) -> Option<i64> {
    match value.as_ref()? {
        Value::Number(number) => number
            .as_i64()
            .or_else(|| number.as_f64().map(|value| value as i64))
            .map(normalize_epoch_secs),
        Value::String(text) => {
            let trimmed = text.trim();
            if let Ok(number) = trimmed.parse::<i64>() {
                return Some(normalize_epoch_secs(number));
            }
            chrono::DateTime::parse_from_rfc3339(trimmed)
                .ok()
                .map(|dt| dt.timestamp())
        }
        _ => None,
    }
}

fn normalize_epoch_secs(secs: i64) -> i64 {
    if secs > 1_000_000_000_000 {
        secs / 1000
    } else {
        secs
    }
}

fn live_usage_from_bigmodel(body: BigModelUsageResponse) -> Result<LiveUsage, String> {
    if body.success == Some(false) {
        return Err(bigmodel_rejection_message(&body));
    }
    if let Some(code) = body.code {
        if code != 200 {
            return Err(bigmodel_rejection_message(&body));
        }
    }
    let limits = body
        .data
        .ok_or_else(|| "BigModel usage response had no data".to_string())?
        .limits
        .unwrap_or_default();
    let mut five_hour: Option<(&BigModelUsageLimit, Option<i64>)> = None;
    let mut weekly: Option<(&BigModelUsageLimit, Option<i64>)> = None;
    let mut unclassified: Vec<(&BigModelUsageLimit, Option<i64>)> = Vec::new();
    for limit in limits.iter().filter(|limit| {
        limit
            .limit_type
            .as_deref()
            .is_some_and(is_bigmodel_token_quota)
    }) {
        let entry = (limit, bigmodel_reset_secs(&limit.next_reset_time));
        match BigModelWindow::from_limit(limit) {
            Some(BigModelWindow::FiveHour) if five_hour.is_none() => five_hour = Some(entry),
            Some(BigModelWindow::Weekly) if weekly.is_none() => weekly = Some(entry),
            _ => unclassified.push(entry),
        }
    }
    // Entries without a recognizable `unit` fall back to reset-time order,
    // but a missing reset time means the rolling window (which may sit at 0%
    // with no reset scheduled).
    unclassified.sort_by_key(|(_, reset)| (reset.is_some(), reset.unwrap_or(i64::MIN)));
    for entry in unclassified {
        if five_hour.is_none() {
            five_hour = Some(entry);
        } else if weekly.is_none() {
            weekly = Some(entry);
        }
    }
    if five_hour.is_none() {
        let mut observed: Vec<String> = limits
            .iter()
            .filter_map(|limit| limit.limit_type.clone())
            .collect();
        observed.sort();
        observed.dedup();
        let observed = if observed.is_empty() {
            "none reported".to_string()
        } else {
            observed.join(", ")
        };
        return Err(format!(
            "BigModel usage response had no token quota (limit types: {observed})"
        ));
    }
    let (five_hour, five_hour_reset) =
        five_hour.ok_or_else(|| "BigModel usage response had no 5-hour quota".to_string())?;
    let used_percent = five_hour.percentage();
    let weekly = weekly.map(|(limit, _)| (limit.percentage(), limit.credits()));
    let resets_at = five_hour_reset.and_then(unix_ts_to_rfc3339);
    Ok(LiveUsage {
        summary: bigmodel_usage_summary(
            (used_percent, five_hour.credits()),
            resets_at.as_deref(),
            weekly,
        ),
        used_percent: Some(used_percent.clamp(0.0, 100.0)),
        resets_at,
    })
}

fn format_credits(current: f64, total: f64) -> String {
    fn count(value: f64) -> String {
        if value.fract() == 0.0 && value.abs() < 1e15 {
            let n = value as i64;
            let digits = n.abs().to_string();
            let mut out = String::new();
            for (index, ch) in digits.chars().enumerate() {
                if index > 0 && (digits.len() - index) % 3 == 0 {
                    out.push(',');
                }
                out.push(ch);
            }
            if n < 0 {
                format!("-{out}")
            } else {
                out
            }
        } else {
            format!("{value}")
        }
    }
    format!("{}/{} credits", count(current), count(total))
}

fn bigmodel_usage_summary(
    five_hour: (f64, Option<(f64, f64)>),
    resets_at: Option<&str>,
    weekly: Option<(f64, Option<(f64, f64)>)>,
) -> String {
    let mut text = format!("{:.0}% used (5h)", five_hour.0.clamp(0.0, 100.0));
    if let Some((current, total)) = five_hour.1 {
        text.push_str(" · ");
        text.push_str(&format_credits(current, total));
    }
    if let Some(label) = resets_at.and_then(reset_label) {
        text.push_str(" · ");
        text.push_str(&label);
    }
    if let Some((percent, credits)) = weekly {
        text.push_str(&format!(" · {:.0}% used (week)", percent.clamp(0.0, 100.0)));
        if let Some((current, total)) = credits {
            text.push_str(" · ");
            text.push_str(&format_credits(current, total));
        }
    }
    text
}

async fn query_minimax_usage(provider: &Provider) -> Result<LiveUsage, String> {
    let api_key = provider.api_key.trim();
    if api_key.is_empty() {
        return Err("MiniMax API key is required".to_string());
    }
    // International accounts live on minimax.io; the CN platform answers on
    // minimaxi.com (and minimax.cn). Anything else falls back to the CN host.
    let haystack =
        format!("{} {} {}", provider.id, provider.name, provider.base_url).to_ascii_lowercase();
    let url = if haystack.contains("minimax.io") {
        MINIMAX_USAGE_API_INTL
    } else {
        MINIMAX_USAGE_API_CN
    };
    let client = http_client().map_err(|error| error.to_string())?;
    let response = client
        .get(url)
        .header("Authorization", format!("Bearer {api_key}"))
        .header("Accept", "application/json")
        .send()
        .await
        .map_err(|error| format!("MiniMax usage query failed: {error}"))?;
    let status = response.status();
    let body = response
        .text()
        .await
        .map_err(|error| format!("Failed to read MiniMax usage response: {error}"))?;
    if !status.is_success() {
        return Err(format!(
            "MiniMax usage query failed (HTTP {status}): {body}"
        ));
    }
    let parsed: MiniMaxRemainsResponse = serde_json::from_str(&body)
        .map_err(|error| format!("MiniMax usage response was not valid JSON: {error}"))?;
    live_usage_from_minimax(parsed)
}

#[derive(Debug, Deserialize)]
struct MiniMaxRemainsResponse {
    base_resp: Option<MiniMaxBaseResp>,
    model_remains: Option<Vec<MiniMaxModelRemains>>,
}

#[derive(Debug, Deserialize)]
struct MiniMaxBaseResp {
    status_code: Option<i64>,
    status_msg: Option<String>,
}

#[derive(Debug, Deserialize)]
struct MiniMaxModelRemains {
    model_name: Option<String>,
    current_interval_remaining_percent: Option<f64>,
    end_time: Option<Value>,
    current_weekly_status: Option<i64>,
    current_weekly_remaining_percent: Option<f64>,
    weekly_end_time: Option<Value>,
}

fn live_usage_from_minimax(body: MiniMaxRemainsResponse) -> Result<LiveUsage, String> {
    if let Some(base_resp) = &body.base_resp {
        let code = base_resp.status_code.unwrap_or(0);
        if code != 0 {
            let message = base_resp
                .status_msg
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .unwrap_or("unknown error");
            return Err(format!(
                "MiniMax usage query failed (code {code}): {message}"
            ));
        }
    }
    let item = body
        .model_remains
        .unwrap_or_default()
        .into_iter()
        .find(|item| item.model_name.as_deref() == Some("general"))
        .ok_or_else(|| "MiniMax usage response had no coding plan quota".to_string())?;
    let five_hour = 100.0 - item.current_interval_remaining_percent.unwrap_or(0.0);
    let resets_at = bigmodel_reset_secs(&item.end_time).and_then(unix_ts_to_rfc3339);
    // Weekly status 1 means the plan has a weekly bucket; other values (such
    // as 3) mark plans without one, where the percent is pinned at 100.
    let weekly = if item.current_weekly_status == Some(1) {
        item.current_weekly_remaining_percent
            .map(|remain| 100.0 - remain)
    } else {
        None
    };
    let weekly_resets = if weekly.is_some() {
        bigmodel_reset_secs(&item.weekly_end_time).and_then(unix_ts_to_rfc3339)
    } else {
        None
    };
    let summary = bigmodel_usage_summary(
        (five_hour, None),
        resets_at.as_deref().or(weekly_resets.as_deref()),
        weekly.map(|percent| (percent, None)),
    );
    Ok(LiveUsage {
        summary,
        used_percent: Some(five_hour.clamp(0.0, 100.0)),
        resets_at,
    })
}

async fn query_kimi_usage(provider: &Provider) -> Result<LiveUsage, String> {
    let api_key = provider.api_key.trim();
    if api_key.is_empty() {
        return Err("Kimi API key is required".to_string());
    }
    // Kimi For Coding subscription keys (api.kimi.com/coding) have rolling
    // usage windows; pay-as-you-go platform keys (api.moonshot.cn) only
    // expose the account balance.
    let haystack =
        format!("{} {} {}", provider.id, provider.name, provider.base_url).to_ascii_lowercase();
    if haystack.contains("api.kimi.com/coding") {
        query_kimi_coding_usage(api_key).await
    } else {
        query_kimi_platform_balance(api_key).await
    }
}

async fn query_kimi_coding_usage(api_key: &str) -> Result<LiveUsage, String> {
    let client = http_client().map_err(|error| error.to_string())?;
    let response = client
        .get(KIMI_CODING_USAGE_API)
        .header("Authorization", format!("Bearer {api_key}"))
        .header("Accept", "application/json")
        .send()
        .await
        .map_err(|error| format!("Kimi usage query failed: {error}"))?;
    let status = response.status();
    let body = response
        .text()
        .await
        .map_err(|error| format!("Failed to read Kimi usage response: {error}"))?;
    if !status.is_success() {
        return Err(format!("Kimi usage query failed (HTTP {status}): {body}"));
    }
    let parsed: KimiUsageResponse = serde_json::from_str(&body)
        .map_err(|error| format!("Kimi usage response was not valid JSON: {error}"))?;
    live_usage_from_kimi(parsed)
}

async fn query_kimi_platform_balance(api_key: &str) -> Result<LiveUsage, String> {
    let client = http_client().map_err(|error| error.to_string())?;
    let response = client
        .get(KIMI_PLATFORM_BALANCE_API)
        .header("Authorization", format!("Bearer {api_key}"))
        .header("Accept", "application/json")
        .send()
        .await
        .map_err(|error| format!("Kimi balance query failed: {error}"))?;
    let status = response.status();
    let body = response
        .text()
        .await
        .map_err(|error| format!("Failed to read Kimi balance response: {error}"))?;
    if !status.is_success() {
        return Err(format!("Kimi balance query failed (HTTP {status}): {body}"));
    }
    let parsed: KimiBalanceResponse = serde_json::from_str(&body)
        .map_err(|error| format!("Kimi balance response was not valid JSON: {error}"))?;
    live_usage_from_kimi_balance(parsed)
}

#[derive(Debug, Deserialize)]
struct KimiUsageResponse {
    limits: Option<Vec<KimiUsageLimit>>,
}

#[derive(Debug, Deserialize)]
struct KimiUsageLimit {
    detail: Option<KimiUsageDetail>,
}

#[derive(Debug, Deserialize)]
struct KimiUsageDetail {
    limit: Option<f64>,
    remaining: Option<f64>,
    reset_time: Option<Value>,
}

fn live_usage_from_kimi(body: KimiUsageResponse) -> Result<LiveUsage, String> {
    let detail = body
        .limits
        .unwrap_or_default()
        .into_iter()
        .find_map(|limit| limit.detail)
        .ok_or_else(|| "Kimi usage response had no quota details".to_string())?;
    let total = detail.limit.unwrap_or(0.0);
    if total <= 0.0 {
        return Err("Kimi usage response had no quota limit".to_string());
    }
    let remaining = detail.remaining.unwrap_or(0.0);
    let used_percent = (((total - remaining) / total) * 100.0).clamp(0.0, 100.0);
    let resets_at = bigmodel_reset_secs(&detail.reset_time).and_then(unix_ts_to_rfc3339);
    Ok(LiveUsage {
        summary: usage_summary(used_percent, resets_at.as_deref()),
        used_percent: Some(used_percent),
        resets_at,
    })
}

#[derive(Debug, Deserialize)]
struct KimiBalanceResponse {
    code: Option<i64>,
    status: Option<bool>,
    data: Option<KimiBalanceData>,
}

#[derive(Debug, Deserialize)]
struct KimiBalanceData {
    available_balance: Option<f64>,
    voucher_balance: Option<f64>,
    #[allow(dead_code)]
    cash_balance: Option<f64>,
}

fn live_usage_from_kimi_balance(body: KimiBalanceResponse) -> Result<LiveUsage, String> {
    if body.status == Some(false) || body.code.is_some_and(|code| code != 0) {
        return Err(format!(
            "Kimi balance query failed (code {}, status {})",
            body.code
                .map(|code| code.to_string())
                .unwrap_or_else(|| "?".to_string()),
            body.status
                .map(|status| status.to_string())
                .unwrap_or_else(|| "?".to_string()),
        ));
    }
    let data = body
        .data
        .ok_or_else(|| "Kimi balance response had no data".to_string())?;
    let available = data
        .available_balance
        .ok_or_else(|| "Kimi balance response had no available_balance".to_string())?;
    let voucher = data.voucher_balance.unwrap_or(0.0);
    let mut summary = format!("¥{available:.2} remaining");
    if voucher > 0.0 {
        summary.push_str(&format!(" (voucher ¥{voucher:.2})"));
    }
    Ok(LiveUsage {
        summary,
        used_percent: None,
        resets_at: None,
    })
}

async fn query_copilot_usage(github_token: &str) -> Result<LiveUsage, String> {
    // api.github.com is often unreachable without a proxy, so this client
    // honors the system proxy configuration instead of forcing direct
    // connections like the domestic provider queries do.
    let client = http_system_proxy_client().map_err(|error| error.to_string())?;
    let mut request = client
        .get(COPILOT_USAGE_API)
        .header("Authorization", format!("token {github_token}"))
        .header("Accept", "application/json");
    for (name, value) in copilot_request_headers() {
        request = request.header(name, value);
    }
    let response = request
        .send()
        .await
        .map_err(|error| format!("Copilot usage query failed: {error}"))?;
    let status = response.status();
    let body = response
        .text()
        .await
        .map_err(|error| format!("Failed to read Copilot usage response: {error}"))?;
    if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
        return Err(format!("Copilot usage query failed (HTTP {status})"));
    }
    if !status.is_success() {
        return Err(format!(
            "Copilot usage query failed (HTTP {status}): {body}"
        ));
    }
    let parsed: CopilotUsageResponse = serde_json::from_str(&body)
        .map_err(|error| format!("Copilot usage response was not valid JSON: {error}"))?;
    Ok(live_usage_from_copilot(parsed))
}

#[derive(Debug, Deserialize)]
struct CopilotUsageResponse {
    copilot_plan: Option<String>,
    quota_reset_date: Option<String>,
    quota_snapshots: Option<CopilotQuotaSnapshots>,
}

#[derive(Debug, Deserialize)]
struct CopilotQuotaSnapshots {
    premium_interactions: Option<CopilotQuotaDetail>,
}

#[derive(Debug, Deserialize)]
struct CopilotQuotaDetail {
    entitlement: Option<f64>,
    remaining: Option<f64>,
    unlimited: Option<bool>,
}

fn live_usage_from_copilot(body: CopilotUsageResponse) -> LiveUsage {
    let plan = body
        .copilot_plan
        .as_deref()
        .map(str::trim)
        .filter(|plan| !plan.is_empty());
    let snapshot = body
        .quota_snapshots
        .and_then(|snapshots| snapshots.premium_interactions);
    let mut parts: Vec<String> = Vec::new();
    let mut used_percent: Option<f64> = None;
    if let Some(snapshot) = snapshot {
        let entitlement = snapshot.entitlement.unwrap_or(0.0);
        let unlimited = snapshot.unlimited.unwrap_or(false);
        if unlimited || entitlement <= 0.0 {
            parts.push("premium unlimited".to_string());
        } else {
            let remaining = snapshot.remaining.unwrap_or(0.0);
            let percent = (((entitlement - remaining) / entitlement) * 100.0).clamp(0.0, 100.0);
            used_percent = Some(percent);
            parts.push(format!("{:.0}% used (premium)", percent));
        }
    }
    if let Some(plan) = plan {
        parts.push(plan.to_string());
    }
    if let Some(reset) = body
        .quota_reset_date
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        parts.push(format!("resets {reset}"));
    }
    LiveUsage {
        summary: if parts.is_empty() {
            "Usage unavailable".to_string()
        } else {
            parts.join(" · ")
        },
        used_percent,
        resets_at: None,
    }
}

fn http_client() -> anyhow::Result<reqwest::Client> {
    reqwest::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(15))
        .build()
        .context("Failed to build usage client")
}

/// Honors the system proxy configuration (environment variables and, on
/// macOS, the system proxy settings) for endpoints that are unreachable by
/// direct connections.
fn http_system_proxy_client() -> anyhow::Result<reqwest::Client> {
    reqwest::Client::builder()
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
            usage_page_url(&provider("glm", "", "https://open.bigmodel.cn/api/v1")).as_deref(),
            Some(BIGMODEL_USAGE_PAGE_URL)
        );
        assert_eq!(
            usage_page_url(&provider("minimax", "", "https://api.minimax.cn/v1")).as_deref(),
            Some(MINIMAX_USAGE_PAGE_URL)
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
            deepseek_balance_url("https://api.deepseek.com").unwrap(),
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

    fn bigmodel_limit(
        limit_type: &str,
        percentage: f64,
        next_reset_time: Option<Value>,
    ) -> BigModelUsageLimit {
        BigModelUsageLimit {
            limit_type: Some(limit_type.to_string()),
            percentage: Some(percentage),
            unit: None,
            current_value: None,
            usage: None,
            next_reset_time,
        }
    }

    fn bigmodel_limit_with_unit(
        limit_type: &str,
        percentage: f64,
        unit: i64,
        next_reset_time: Option<Value>,
    ) -> BigModelUsageLimit {
        BigModelUsageLimit {
            limit_type: Some(limit_type.to_string()),
            percentage: Some(percentage),
            unit: Some(unit),
            current_value: None,
            usage: None,
            next_reset_time,
        }
    }

    #[test]
    fn bigmodel_summary_includes_credits_like_the_usage_page() {
        let live = live_usage_from_bigmodel(BigModelUsageResponse {
            code: Some(200),
            msg: Some("Operation successful".to_string()),
            success: Some(true),
            data: Some(BigModelUsageData {
                level: Some("pro".to_string()),
                limits: Some(vec![
                    BigModelUsageLimit {
                        limit_type: Some("CREDIT_LIMIT".to_string()),
                        percentage: Some(11.0),
                        unit: Some(3),
                        current_value: Some(1354.0),
                        usage: Some(12000.0),
                        next_reset_time: Some(json!(1_789_943_340_000i64)),
                    },
                    BigModelUsageLimit {
                        limit_type: Some("CREDIT_LIMIT".to_string()),
                        percentage: Some(31.0),
                        unit: Some(6),
                        current_value: Some(19173.0),
                        usage: Some(60000.0),
                        next_reset_time: Some(json!(1_790_411_880_000i64)),
                    },
                ]),
            }),
        })
        .expect("usage");
        assert_eq!(live.used_percent, Some(11.0));
        assert!(
            live.summary
                .contains("11% used (5h) · 1,354/12,000 credits"),
            "{}",
            live.summary
        );
        assert!(
            live.summary
                .contains("31% used (week) · 19,173/60,000 credits"),
            "{}",
            live.summary
        );
    }

    #[test]
    fn bigmodel_unit_field_classifies_windows_over_reset_order() {
        let live = live_usage_from_bigmodel(BigModelUsageResponse {
            code: Some(200),
            msg: Some("ok".to_string()),
            success: Some(true),
            data: Some(BigModelUsageData {
                level: Some("pro".to_string()),
                limits: Some(vec![
                    bigmodel_limit("TIME_LIMIT", 7.0, None),
                    // Near week end the weekly window can reset before the
                    // rolling one; unit=3 must still win the 5h slot.
                    bigmodel_limit_with_unit(
                        "TOKENS_LIMIT",
                        53.0,
                        6,
                        Some(json!(1_791_000_000_000i64)),
                    ),
                    bigmodel_limit_with_unit(
                        "TOKENS_LIMIT",
                        44.0,
                        3,
                        Some(json!(1_791_500_000_000i64)),
                    ),
                ]),
            }),
        })
        .expect("usage");
        assert_eq!(live.used_percent, Some(44.0));
        assert_eq!(live.resets_at.as_deref(), Some("2026-10-08T22:53:20+00:00"));
        assert!(live.summary.contains("44% used (5h)"), "{}", live.summary);
        assert!(live.summary.contains("53% used (week)"), "{}", live.summary);
    }

    #[test]
    fn bigmodel_unclassified_limits_fall_back_to_reset_order() {
        let live = live_usage_from_bigmodel(BigModelUsageResponse {
            code: Some(200),
            msg: None,
            success: Some(true),
            data: Some(BigModelUsageData {
                level: None,
                limits: Some(vec![
                    bigmodel_limit("TOKENS_LIMIT", 53.0, Some(json!(1_791_000_000_000i64))),
                    bigmodel_limit("TOKENS_LIMIT", 44.0, None),
                ]),
            }),
        })
        .expect("usage");
        assert_eq!(live.used_percent, Some(44.0));
        assert_eq!(weekly_percent(&live.summary), Some(53.0));
    }

    fn weekly_percent(summary: &str) -> Option<f64> {
        let marker = summary.find("% used (week)")?;
        let start = summary[..marker].rfind(' ')? + 1;
        summary[start..marker].trim().parse().ok()
    }

    #[test]
    fn bigmodel_credit_limit_counts_as_token_quota() {
        let live = live_usage_from_bigmodel(BigModelUsageResponse {
            code: Some(200),
            msg: None,
            success: Some(true),
            data: Some(BigModelUsageData {
                level: None,
                limits: Some(vec![bigmodel_limit_with_unit(
                    "CREDIT_LIMIT",
                    12.5,
                    3,
                    Some(json!(1_800_000_000_000i64)),
                )]),
            }),
        })
        .expect("usage");
        assert_eq!(live.used_percent, Some(12.5));
    }

    #[test]
    fn bigmodel_epoch_millis_reset_time_is_normalized() {
        let live = live_usage_from_bigmodel(BigModelUsageResponse {
            code: Some(200),
            msg: None,
            success: Some(true),
            data: Some(BigModelUsageData {
                level: None,
                limits: Some(vec![bigmodel_limit_with_unit(
                    "TOKENS_LIMIT",
                    12.5,
                    3,
                    Some(json!(1_800_000_000_000i64)),
                )]),
            }),
        })
        .expect("usage");
        assert_eq!(live.used_percent, Some(12.5));
        assert_eq!(live.resets_at.as_deref(), Some("2027-01-15T08:00:00+00:00"));
        assert!(live.summary.contains("resets in"), "{}", live.summary);
    }

    #[test]
    fn bigmodel_rejected_response_surfaces_message() {
        let error = live_usage_from_bigmodel(BigModelUsageResponse {
            code: Some(401),
            msg: Some("令牌已过期或验证不正确".to_string()),
            success: Some(false),
            data: None,
        })
        .expect_err("rejected");
        assert!(error.contains("code 401"), "{}", error);
        assert!(error.contains("令牌已过期或验证不正确"), "{}", error);
    }

    #[test]
    fn bigmodel_missing_token_quota_reports_observed_types() {
        let error = live_usage_from_bigmodel(BigModelUsageResponse {
            code: Some(200),
            msg: None,
            success: Some(true),
            data: Some(BigModelUsageData {
                level: None,
                limits: Some(vec![bigmodel_limit("TIME_LIMIT", 7.0, None)]),
            }),
        })
        .expect_err("no token quota");
        assert!(error.contains("no token quota"), "{}", error);
        assert!(error.contains("TIME_LIMIT"), "{}", error);
    }

    fn minimax_remains(
        interval_remaining: f64,
        weekly_status: Option<i64>,
        weekly_remaining: Option<f64>,
    ) -> MiniMaxRemainsResponse {
        MiniMaxRemainsResponse {
            base_resp: Some(MiniMaxBaseResp {
                status_code: Some(0),
                status_msg: Some("success".to_string()),
            }),
            model_remains: Some(vec![MiniMaxModelRemains {
                model_name: Some("general".to_string()),
                current_interval_remaining_percent: Some(interval_remaining),
                end_time: Some(json!(1_800_000_000_000i64)),
                current_weekly_status: weekly_status,
                current_weekly_remaining_percent: weekly_remaining,
                weekly_end_time: None,
            }]),
        }
    }

    #[test]
    fn minimax_remaining_percent_becomes_used_summary() {
        let live =
            live_usage_from_minimax(minimax_remains(88.0, Some(1), Some(80.0))).expect("usage");
        assert_eq!(live.used_percent, Some(12.0));
        assert!(live.summary.contains("12% used (5h)"), "{}", live.summary);
        assert!(live.summary.contains("20% used (week)"), "{}", live.summary);
    }

    #[test]
    fn minimax_inactive_weekly_bucket_is_skipped() {
        let live =
            live_usage_from_minimax(minimax_remains(50.0, Some(3), Some(100.0))).expect("usage");
        assert_eq!(live.used_percent, Some(50.0));
        assert!(!live.summary.contains("(week)"), "{}", live.summary);
    }

    #[test]
    fn minimax_base_resp_error_surfaces_message() {
        let error = live_usage_from_minimax(MiniMaxRemainsResponse {
            base_resp: Some(MiniMaxBaseResp {
                status_code: Some(1004),
                status_msg: Some("login fail".to_string()),
            }),
            model_remains: None,
        })
        .expect_err("rejected");
        assert!(error.contains("code 1004"), "{}", error);
        assert!(error.contains("login fail"), "{}", error);
    }

    #[test]
    fn kimi_usage_detail_becomes_used_percent() {
        let live = live_usage_from_kimi(KimiUsageResponse {
            limits: Some(vec![KimiUsageLimit {
                detail: Some(KimiUsageDetail {
                    limit: Some(1000.0),
                    remaining: Some(250.0),
                    reset_time: Some(json!("2026-09-25T13:38:00Z")),
                }),
            }]),
        })
        .expect("usage");
        assert_eq!(live.used_percent, Some(75.0));
        assert!(live.summary.starts_with("75% used"), "{}", live.summary);
        assert_eq!(live.resets_at.as_deref(), Some("2026-09-25T13:38:00+00:00"));
    }

    #[test]
    fn kimi_missing_limits_is_an_error() {
        let error =
            live_usage_from_kimi(KimiUsageResponse { limits: None }).expect_err("no limits");
        assert!(error.contains("no quota details"), "{}", error);
    }

    #[test]
    fn copilot_quota_snapshot_becomes_premium_percent() {
        let live = live_usage_from_copilot(CopilotUsageResponse {
            copilot_plan: Some("pro".to_string()),
            quota_reset_date: Some("2026-10-01".to_string()),
            quota_snapshots: Some(CopilotQuotaSnapshots {
                premium_interactions: Some(CopilotQuotaDetail {
                    entitlement: Some(300.0),
                    remaining: Some(120.0),
                    unlimited: Some(false),
                }),
            }),
        });
        assert_eq!(live.used_percent, Some(60.0));
        assert_eq!(live.summary, "60% used (premium) · pro · resets 2026-10-01");
    }

    #[test]
    fn copilot_unlimited_quota_reports_no_percent() {
        let live = live_usage_from_copilot(CopilotUsageResponse {
            copilot_plan: Some("free".to_string()),
            quota_reset_date: None,
            quota_snapshots: Some(CopilotQuotaSnapshots {
                premium_interactions: Some(CopilotQuotaDetail {
                    entitlement: Some(50.0),
                    remaining: Some(50.0),
                    unlimited: Some(true),
                }),
            }),
        });
        assert_eq!(live.used_percent, None);
        assert_eq!(live.summary, "premium unlimited · free");
    }

    #[test]
    fn kimi_balance_becomes_remaining_summary() {
        let live = live_usage_from_kimi_balance(KimiBalanceResponse {
            code: Some(0),
            status: Some(true),
            data: Some(KimiBalanceData {
                available_balance: Some(54.78392),
                voucher_balance: Some(1.5),
                cash_balance: Some(54.78392),
            }),
        })
        .expect("usage");
        assert_eq!(live.used_percent, None);
        assert_eq!(live.summary, "¥54.78 remaining (voucher ¥1.50)");
    }

    #[test]
    fn kimi_balance_rejection_surfaces_code() {
        let error = live_usage_from_kimi_balance(KimiBalanceResponse {
            code: Some(1001),
            status: Some(false),
            data: None,
        })
        .expect_err("rejected");
        assert!(error.contains("code 1001"), "{}", error);
        assert!(error.contains("false"), "{}", error);
    }
}
