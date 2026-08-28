//! Device-code OAuth for GitHub Copilot and xAI Grok.
//!
//! Protocol follows CC Switch (MIT): GitHub device flow + Copilot token
//! exchange, and xAI OpenID device authorization grant.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::Context;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::codex_live::set_secret_file_permissions;

const GITHUB_CLIENT_ID: &str = "Iv1.b507a08c87ecfe98";
const COPILOT_USER_AGENT: &str = "GitHubCopilotChat/0.38.2";
const COPILOT_EDITOR_VERSION: &str = "vscode/1.110.1";
const COPILOT_PLUGIN_VERSION: &str = "copilot-chat/0.38.2";
const COPILOT_API_VERSION: &str = "2025-10-01";
const COPILOT_INTEGRATION_ID: &str = "vscode-chat";

const XAI_DISCOVERY_URL: &str = "https://auth.x.ai/.well-known/openid-configuration";
const XAI_CLIENT_ID: &str = "b1a00492-073a-47ea-816f-4c329264a828";
const XAI_SCOPE: &str = "openid profile email offline_access grok-cli:access api:access";
const XAI_USER_AGENT: &str = "codex-helper-xai-oauth";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OAuthKind {
    GithubCopilot,
    Xai,
}

impl OAuthKind {
    pub fn parse(value: &str) -> anyhow::Result<Self> {
        match value.trim() {
            "github_copilot" | "github-copilot" | "copilot" => Ok(Self::GithubCopilot),
            "xai_oauth" | "xai-oauth" | "xai" | "grok" => Ok(Self::Xai),
            other => anyhow::bail!("Unsupported OAuth kind: {other}"),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::GithubCopilot => "github_copilot",
            Self::Xai => "xai_oauth",
        }
    }

    fn file_name(self) -> &'static str {
        match self {
            Self::GithubCopilot => "github_copilot.json",
            Self::Xai => "xai.json",
        }
    }

    pub fn default_base_url(self) -> &'static str {
        match self {
            Self::GithubCopilot => "https://api.githubcopilot.com",
            Self::Xai => "https://api.x.ai/v1",
        }
    }

    pub fn default_wire_api(self) -> &'static str {
        match self {
            Self::GithubCopilot => "chat",
            Self::Xai => "responses",
        }
    }

    pub fn compat_name(self) -> &'static str {
        match self {
            Self::GithubCopilot => "github-copilot",
            Self::Xai => "xai-oauth",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::GithubCopilot => "GitHub Copilot",
            Self::Xai => "xAI Grok",
        }
    }
}

/// Placeholder written into Codex `experimental_bearer_token` for device-OAuth
/// providers. The helper proxy replaces it with a fresh token per request.
pub const HELPER_OAUTH_LIVE_TOKEN: &str = "HELPER_MANAGED";

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct CopilotStore {
    login: String,
    account_id: String,
    github_token: String,
    copilot_token: String,
    copilot_expires_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct XaiStore {
    login: String,
    account_id: String,
    refresh_token: String,
    access_token: String,
    expires_at_ms: i64,
}

#[derive(Debug, Clone)]
struct PendingFlow {
    kind: OAuthKind,
    token_endpoint: String,
}

fn pending_flows() -> &'static Mutex<HashMap<String, PendingFlow>> {
    static FLOWS: OnceLock<Mutex<HashMap<String, PendingFlow>>> = OnceLock::new();
    FLOWS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn oauth_dir(state_root: &Path) -> PathBuf {
    state_root.join("oauth")
}

fn oauth_path(state_root: &Path, kind: OAuthKind) -> PathBuf {
    oauth_dir(state_root).join(kind.file_name())
}

fn write_secret_json(path: &Path, value: &impl Serialize) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, serde_json::to_vec_pretty(value)?)
        .with_context(|| format!("Failed to write {}", path.display()))?;
    set_secret_file_permissions(path)?;
    Ok(())
}

fn http_client() -> anyhow::Result<reqwest::Client> {
    reqwest::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(20))
        .build()
        .context("Failed to build OAuth client")
}

pub async fn start_oauth(state_root: &Path, kind: OAuthKind) -> anyhow::Result<Value> {
    fs::create_dir_all(oauth_dir(state_root))
        .with_context(|| format!("Failed to create {}", oauth_dir(state_root).display()))?;
    match kind {
        OAuthKind::GithubCopilot => start_github_device().await,
        OAuthKind::Xai => start_xai_device().await,
    }
}

async fn start_github_device() -> anyhow::Result<Value> {
    let response = http_client()?
        .post("https://github.com/login/device/code")
        .header("Accept", "application/json")
        .header("User-Agent", COPILOT_USER_AGENT)
        .form(&[("client_id", GITHUB_CLIENT_ID), ("scope", "read:user")])
        .send()
        .await
        .context("GitHub device code request failed")?;
    if !response.status().is_success() {
        anyhow::bail!("GitHub device code failed: HTTP {}", response.status());
    }
    let body: Value = response.json().await.context("GitHub device code JSON")?;
    let device_code = body
        .get("device_code")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("GitHub device_code missing"))?
        .to_string();
    let user_code = body
        .get("user_code")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow::anyhow!("GitHub user_code missing"))?;
    let verification_uri = body
        .get("verification_uri")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow::anyhow!("GitHub verification_uri missing"))?;
    pending_flows().lock().expect("oauth lock").insert(
        device_code.clone(),
        PendingFlow {
            kind: OAuthKind::GithubCopilot,
            token_endpoint: "https://github.com/login/oauth/access_token".to_string(),
        },
    );
    Ok(json!({
        "status": "ok",
        "kind": "github_copilot",
        "deviceCode": device_code,
        "userCode": user_code,
        "verificationUri": verification_uri,
        "interval": body.get("interval").and_then(Value::as_u64).unwrap_or(5),
        "expiresIn": body.get("expires_in").and_then(Value::as_u64).unwrap_or(900),
    }))
}

async fn start_xai_device() -> anyhow::Result<Value> {
    let discovery: Value = http_client()?
        .get(XAI_DISCOVERY_URL)
        .header("User-Agent", XAI_USER_AGENT)
        .send()
        .await
        .context("xAI discovery failed")?
        .json()
        .await
        .context("xAI discovery JSON")?;
    let device_endpoint = discovery
        .get("device_authorization_endpoint")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("xAI device_authorization_endpoint missing"))?;
    let token_endpoint = discovery
        .get("token_endpoint")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("xAI token_endpoint missing"))?
        .to_string();
    let response = http_client()?
        .post(device_endpoint)
        .header("User-Agent", XAI_USER_AGENT)
        .form(&[("client_id", XAI_CLIENT_ID), ("scope", XAI_SCOPE)])
        .send()
        .await
        .context("xAI device code request failed")?;
    if !response.status().is_success() {
        anyhow::bail!("xAI device code failed: HTTP {}", response.status());
    }
    let body: Value = response.json().await.context("xAI device code JSON")?;
    let device_code = body
        .get("device_code")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("xAI device_code missing"))?
        .to_string();
    let user_code = body
        .get("user_code")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow::anyhow!("xAI user_code missing"))?;
    let verification_uri = body
        .get("verification_uri_complete")
        .and_then(Value::as_str)
        .or_else(|| body.get("verification_uri").and_then(Value::as_str))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow::anyhow!("xAI verification_uri missing"))?;
    pending_flows().lock().expect("oauth lock").insert(
        device_code.clone(),
        PendingFlow {
            kind: OAuthKind::Xai,
            token_endpoint,
        },
    );
    Ok(json!({
        "status": "ok",
        "kind": "xai_oauth",
        "deviceCode": device_code,
        "userCode": user_code,
        "verificationUri": verification_uri,
        "interval": body.get("interval").and_then(Value::as_u64).unwrap_or(5),
        "expiresIn": body.get("expires_in").and_then(Value::as_u64).unwrap_or(900),
    }))
}

pub async fn poll_oauth(
    state_root: &Path,
    kind: OAuthKind,
    device_code: &str,
) -> anyhow::Result<Value> {
    let pending = pending_flows()
        .lock()
        .expect("oauth lock")
        .get(device_code)
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("OAuth device code is not active. Start sign-in again."))?;
    if pending.kind != kind {
        anyhow::bail!("OAuth device code does not match provider kind");
    }
    match kind {
        OAuthKind::GithubCopilot => {
            poll_github(state_root, device_code, &pending.token_endpoint).await
        }
        OAuthKind::Xai => poll_xai(state_root, device_code, &pending.token_endpoint).await,
    }
}

pub(crate) fn oauth_error_status(body: &Value) -> Option<&'static str> {
    match body.get("error").and_then(Value::as_str)? {
        "authorization_pending" | "slow_down" => Some("pending"),
        "access_denied" => Some("denied"),
        "expired_token" => Some("expired"),
        _ => Some("failed"),
    }
}

async fn poll_github(
    state_root: &Path,
    device_code: &str,
    token_endpoint: &str,
) -> anyhow::Result<Value> {
    let response = http_client()?
        .post(token_endpoint)
        .header("Accept", "application/json")
        .header("User-Agent", COPILOT_USER_AGENT)
        .form(&[
            ("client_id", GITHUB_CLIENT_ID),
            ("device_code", device_code),
            ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
        ])
        .send()
        .await
        .context("GitHub token poll failed")?;
    let body: Value = response.json().await.context("GitHub token JSON")?;
    if let Some(status) = oauth_error_status(&body) {
        return Ok(json!({ "status": status, "message": body.get("error") }));
    }
    let github_token = body
        .get("access_token")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("GitHub access_token missing"))?;
    let user: Value = http_client()?
        .get("https://api.github.com/user")
        .header("Authorization", format!("token {github_token}"))
        .header("User-Agent", COPILOT_USER_AGENT)
        .header("Editor-Version", COPILOT_EDITOR_VERSION)
        .header("Editor-Plugin-Version", COPILOT_PLUGIN_VERSION)
        .send()
        .await
        .context("GitHub user request failed")?
        .json()
        .await
        .context("GitHub user JSON")?;
    let login = user
        .get("login")
        .and_then(Value::as_str)
        .unwrap_or("github")
        .to_string();
    let account_id = user
        .get("id")
        .and_then(Value::as_u64)
        .map(|id| id.to_string())
        .unwrap_or_else(|| login.clone());
    let copilot = http_client()?
        .get("https://api.github.com/copilot_internal/v2/token")
        .header("Authorization", format!("token {github_token}"))
        .header("User-Agent", COPILOT_USER_AGENT)
        .header("Editor-Version", COPILOT_EDITOR_VERSION)
        .header("Editor-Plugin-Version", COPILOT_PLUGIN_VERSION)
        .send()
        .await
        .context("Copilot token request failed")?;
    if copilot.status() == reqwest::StatusCode::FORBIDDEN {
        anyhow::bail!("This GitHub account does not have a Copilot subscription");
    }
    if !copilot.status().is_success() {
        anyhow::bail!("Copilot token failed: HTTP {}", copilot.status());
    }
    let copilot_body: Value = copilot.json().await.context("Copilot token JSON")?;
    let store = CopilotStore {
        login: login.clone(),
        account_id,
        github_token: github_token.to_string(),
        copilot_token: copilot_body
            .get("token")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        copilot_expires_at: copilot_body
            .get("expires_at")
            .and_then(Value::as_i64)
            .unwrap_or(0),
    };
    if store.copilot_token.is_empty() {
        anyhow::bail!("Copilot token missing from GitHub response");
    }
    write_secret_json(&oauth_path(state_root, OAuthKind::GithubCopilot), &store)?;
    pending_flows()
        .lock()
        .expect("oauth lock")
        .remove(device_code);
    Ok(json!({ "status": "ok", "kind": "github_copilot", "login": login }))
}

async fn poll_xai(
    state_root: &Path,
    device_code: &str,
    token_endpoint: &str,
) -> anyhow::Result<Value> {
    let response = http_client()?
        .post(token_endpoint)
        .header("User-Agent", XAI_USER_AGENT)
        .form(&[
            ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
            ("client_id", XAI_CLIENT_ID),
            ("device_code", device_code),
        ])
        .send()
        .await
        .context("xAI token poll failed")?;
    let body: Value = response.json().await.context("xAI token JSON")?;
    if let Some(status) = oauth_error_status(&body) {
        return Ok(json!({ "status": status, "message": body.get("error") }));
    }
    let access_token = body
        .get("access_token")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("xAI access_token missing"))?;
    let refresh_token = body
        .get("refresh_token")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("xAI refresh_token missing"))?;
    let (account_id, login) = identity_from_jwt(body.get("id_token").and_then(Value::as_str))
        .unwrap_or_else(|| ("xai".to_string(), "xai".to_string()));
    let expires_in = body
        .get("expires_in")
        .and_then(Value::as_i64)
        .unwrap_or(3600);
    let store = XaiStore {
        login: login.clone(),
        account_id,
        refresh_token: refresh_token.to_string(),
        access_token: access_token.to_string(),
        expires_at_ms: now_ms().saturating_add(expires_in.saturating_mul(1000)),
    };
    write_secret_json(&oauth_path(state_root, OAuthKind::Xai), &store)?;
    pending_flows()
        .lock()
        .expect("oauth lock")
        .remove(device_code);
    Ok(json!({ "status": "ok", "kind": "xai_oauth", "login": login }))
}

fn identity_from_jwt(id_token: Option<&str>) -> Option<(String, String)> {
    let token = id_token?;
    let payload = token.split('.').nth(1)?;
    let bytes = URL_SAFE_NO_PAD.decode(payload).ok()?;
    let claims: Value = serde_json::from_slice(&bytes).ok()?;
    let account_id = claims.get("sub").and_then(Value::as_str)?.to_string();
    let login = claims
        .get("email")
        .or_else(|| claims.get("preferred_username"))
        .or_else(|| claims.get("name"))
        .and_then(Value::as_str)
        .unwrap_or(account_id.as_str())
        .to_string();
    Some((account_id, login))
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|value| value.as_millis() as i64)
        .unwrap_or(0)
}

fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|value| value.as_secs() as i64)
        .unwrap_or(0)
}

pub fn oauth_status(state_root: &Path, kind: OAuthKind) -> Value {
    match kind {
        OAuthKind::GithubCopilot => {
            let path = oauth_path(state_root, kind);
            match fs::read_to_string(path)
                .ok()
                .and_then(|raw| serde_json::from_str::<CopilotStore>(&raw).ok())
            {
                Some(store) if !store.github_token.is_empty() => json!({
                    "status": "ok",
                    "signedIn": true,
                    "kind": kind.as_str(),
                    "login": store.login,
                }),
                _ => json!({ "status": "ok", "signedIn": false, "kind": kind.as_str() }),
            }
        }
        OAuthKind::Xai => {
            let path = oauth_path(state_root, kind);
            match fs::read_to_string(path)
                .ok()
                .and_then(|raw| serde_json::from_str::<XaiStore>(&raw).ok())
            {
                Some(store) if !store.refresh_token.is_empty() => json!({
                    "status": "ok",
                    "signedIn": true,
                    "kind": kind.as_str(),
                    "login": store.login,
                }),
                _ => json!({ "status": "ok", "signedIn": false, "kind": kind.as_str() }),
            }
        }
    }
}

pub async fn oauth_bearer_token(state_root: &Path, kind: OAuthKind) -> anyhow::Result<String> {
    match kind {
        OAuthKind::GithubCopilot => copilot_bearer(state_root).await,
        OAuthKind::Xai => xai_bearer(state_root).await,
    }
}

async fn copilot_bearer(state_root: &Path) -> anyhow::Result<String> {
    let path = oauth_path(state_root, OAuthKind::GithubCopilot);
    let mut store: CopilotStore = serde_json::from_str(
        &fs::read_to_string(&path).with_context(|| "GitHub Copilot is not signed in")?,
    )?;
    if store.github_token.is_empty() {
        anyhow::bail!("GitHub Copilot is not signed in");
    }
    if !store.copilot_token.is_empty() && store.copilot_expires_at > now_secs() + 60 {
        return Ok(store.copilot_token);
    }
    let response = http_client()?
        .get("https://api.github.com/copilot_internal/v2/token")
        .header("Authorization", format!("token {}", store.github_token))
        .header("User-Agent", COPILOT_USER_AGENT)
        .header("Editor-Version", COPILOT_EDITOR_VERSION)
        .header("Editor-Plugin-Version", COPILOT_PLUGIN_VERSION)
        .send()
        .await
        .context("Copilot token refresh failed")?;
    if !response.status().is_success() {
        anyhow::bail!("Copilot token refresh failed: HTTP {}", response.status());
    }
    let body: Value = response.json().await.context("Copilot refresh JSON")?;
    store.copilot_token = body
        .get("token")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    store.copilot_expires_at = body.get("expires_at").and_then(Value::as_i64).unwrap_or(0);
    if store.copilot_token.is_empty() {
        anyhow::bail!("Copilot token refresh returned an empty token");
    }
    write_secret_json(&path, &store)?;
    Ok(store.copilot_token)
}

async fn xai_bearer(state_root: &Path) -> anyhow::Result<String> {
    let path = oauth_path(state_root, OAuthKind::Xai);
    let mut store: XaiStore = serde_json::from_str(
        &fs::read_to_string(&path).with_context(|| "xAI Grok is not signed in")?,
    )?;
    if store.refresh_token.is_empty() {
        anyhow::bail!("xAI Grok is not signed in");
    }
    if !store.access_token.is_empty() && store.expires_at_ms > now_ms() + 60_000 {
        return Ok(store.access_token);
    }
    let discovery: Value = http_client()?
        .get(XAI_DISCOVERY_URL)
        .header("User-Agent", XAI_USER_AGENT)
        .send()
        .await
        .context("xAI discovery failed")?
        .json()
        .await
        .context("xAI discovery JSON")?;
    let token_endpoint = discovery
        .get("token_endpoint")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("xAI token_endpoint missing"))?;
    let response = http_client()?
        .post(token_endpoint)
        .header("User-Agent", XAI_USER_AGENT)
        .form(&[
            ("grant_type", "refresh_token"),
            ("client_id", XAI_CLIENT_ID),
            ("refresh_token", store.refresh_token.as_str()),
        ])
        .send()
        .await
        .context("xAI refresh failed")?;
    if !response.status().is_success() {
        anyhow::bail!("xAI refresh failed: HTTP {}", response.status());
    }
    let body: Value = response.json().await.context("xAI refresh JSON")?;
    store.access_token = body
        .get("access_token")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    if let Some(refresh) = body.get("refresh_token").and_then(Value::as_str) {
        if !refresh.is_empty() {
            store.refresh_token = refresh.to_string();
        }
    }
    let expires_in = body
        .get("expires_in")
        .and_then(Value::as_i64)
        .unwrap_or(3600);
    store.expires_at_ms = now_ms().saturating_add(expires_in.saturating_mul(1000));
    if store.access_token.is_empty() {
        anyhow::bail!("xAI refresh returned an empty token");
    }
    write_secret_json(&path, &store)?;
    Ok(store.access_token)
}

pub fn oauth_kind_from_provider(compat: &str, base_url: &str) -> Option<OAuthKind> {
    let compat = compat.trim().to_ascii_lowercase();
    if compat == "github-copilot" || compat == "github_copilot" {
        return Some(OAuthKind::GithubCopilot);
    }
    if compat == "xai-oauth" || compat == "xai_oauth" {
        return Some(OAuthKind::Xai);
    }
    if base_url
        .to_ascii_lowercase()
        .contains("api.githubcopilot.com")
    {
        return Some(OAuthKind::GithubCopilot);
    }
    None
}

pub fn oauth_is_signed_in(state_root: &Path, kind: OAuthKind) -> bool {
    oauth_status(state_root, kind)
        .get("signedIn")
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

pub fn copilot_request_headers() -> [(&'static str, &'static str); 5] {
    [
        ("User-Agent", COPILOT_USER_AGENT),
        ("Editor-Version", COPILOT_EDITOR_VERSION),
        ("Editor-Plugin-Version", COPILOT_PLUGIN_VERSION),
        ("Copilot-Integration-Id", COPILOT_INTEGRATION_ID),
        ("x-github-api-version", COPILOT_API_VERSION),
    ]
}

pub fn open_verification_uri(url: &str) -> anyhow::Result<()> {
    let url = url.trim();
    if url.is_empty() {
        anyhow::bail!("OAuth verification URL is missing");
    }
    #[cfg(target_os = "macos")]
    {
        let status = std::process::Command::new("open")
            .arg(url)
            .status()
            .context("Failed to open the OAuth verification URL")?;
        if !status.success() {
            anyhow::bail!("Opening the OAuth verification URL failed");
        }
        return Ok(());
    }
    #[cfg(not(target_os = "macos"))]
    anyhow::bail!("Opening a browser is only supported on macOS");
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::Engine;
    use tempfile::tempdir;

    #[test]
    fn parses_supported_oauth_kinds() {
        assert_eq!(
            OAuthKind::parse("copilot").unwrap(),
            OAuthKind::GithubCopilot
        );
        assert_eq!(
            OAuthKind::parse("github_copilot").unwrap(),
            OAuthKind::GithubCopilot
        );
        assert_eq!(OAuthKind::parse("xai_oauth").unwrap(), OAuthKind::Xai);
        assert_eq!(OAuthKind::parse("grok").unwrap(), OAuthKind::Xai);
        assert!(OAuthKind::parse("cliproxyapi").is_err());
        assert!(OAuthKind::parse("oauthProxy").is_err());
    }

    #[test]
    fn oauth_pending_errors_stay_pending() {
        assert_eq!(
            oauth_error_status(&json!({ "error": "authorization_pending" })),
            Some("pending")
        );
        assert_eq!(
            oauth_error_status(&json!({ "error": "slow_down" })),
            Some("pending")
        );
        assert_eq!(
            oauth_error_status(&json!({ "error": "access_denied" })),
            Some("denied")
        );
        assert_eq!(
            oauth_error_status(&json!({ "error": "expired_token" })),
            Some("expired")
        );
        assert_eq!(
            oauth_error_status(&json!({ "error": "invalid_grant" })),
            Some("failed")
        );
        assert_eq!(oauth_error_status(&json!({})), None);
    }

    #[test]
    fn oauth_kind_from_provider_uses_compat_not_xai_host() {
        assert_eq!(
            oauth_kind_from_provider("github-copilot", ""),
            Some(OAuthKind::GithubCopilot)
        );
        assert_eq!(
            oauth_kind_from_provider("xai-oauth", "https://api.x.ai/v1"),
            Some(OAuthKind::Xai)
        );
        assert_eq!(
            oauth_kind_from_provider("", "https://api.githubcopilot.com"),
            Some(OAuthKind::GithubCopilot)
        );
        assert_eq!(oauth_kind_from_provider("xai", "https://api.x.ai/v1"), None);
    }

    #[test]
    fn missing_oauth_store_is_signed_out() {
        let temp = tempdir().expect("temp");
        let root = temp.path().join(".codex-helper");
        assert!(!oauth_is_signed_in(&root, OAuthKind::GithubCopilot));
        assert!(!oauth_is_signed_in(&root, OAuthKind::Xai));
        let status = oauth_status(&root, OAuthKind::GithubCopilot);
        assert_eq!(status["status"], "ok");
        assert_eq!(status["signedIn"], false);
        assert_eq!(status["kind"], "github_copilot");
    }

    #[test]
    fn identity_from_jwt_reads_email_and_sub() {
        let payload = URL_SAFE_NO_PAD.encode(br#"{"sub":"acct-1","email":"grok@x.ai"}"#);
        let token = format!("aaa.{payload}.bbb");
        assert_eq!(
            identity_from_jwt(Some(&token)),
            Some(("acct-1".to_string(), "grok@x.ai".to_string()))
        );
        assert_eq!(identity_from_jwt(None), None);
    }
}
