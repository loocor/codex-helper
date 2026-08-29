use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::Context;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::codex_live::{
    apply_api_provider, apply_official_provider, auth_has_oauth_login, read_auth,
    read_config_document, write_config_atomic, write_secret_file_atomic, LiveProviderWrite,
    UNIFIED_SESSION_PROVIDER_ID,
};
use crate::model_catalog::{apply_provider_catalog, clear_helper_catalog};
use crate::provider_oauth::{
    oauth_is_signed_in, oauth_kind_from_provider, OAuthKind, HELPER_OAUTH_LIVE_TOKEN,
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ProviderKind {
    Oauth,
    ApiKey,
}

impl Default for ProviderKind {
    fn default() -> Self {
        Self::ApiKey
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", default)]
pub struct ModelMapping {
    pub source: String,
    pub target: String,
}

impl Default for ModelMapping {
    fn default() -> Self {
        Self {
            source: String::new(),
            target: String::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", default)]
pub struct CatalogModel {
    pub model: String,
    pub display_name: String,
    pub context_window: Option<u64>,
    pub reasoning_levels: Vec<String>,
    pub default_reasoning_level: String,
}

impl Default for CatalogModel {
    fn default() -> Self {
        Self {
            model: String::new(),
            display_name: String::new(),
            context_window: None,
            reasoning_levels: Vec::new(),
            default_reasoning_level: String::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", default)]
pub struct Provider {
    pub id: String,
    pub name: String,
    pub kind: ProviderKind,
    pub model: String,
    pub base_url: String,
    pub wire_api: String,
    pub api_key: String,
    pub compat: String,
    pub model_mappings: Vec<ModelMapping>,
    pub models: Vec<String>,
    pub catalog_models: Vec<CatalogModel>,
    pub usage_page_url: String,
}

impl Default for Provider {
    fn default() -> Self {
        Self {
            id: String::new(),
            name: String::new(),
            kind: ProviderKind::ApiKey,
            model: String::new(),
            base_url: String::new(),
            wire_api: "responses".to_string(),
            api_key: String::new(),
            compat: String::new(),
            model_mappings: Vec::new(),
            models: Vec::new(),
            catalog_models: Vec::new(),
            usage_page_url: String::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", default)]
pub struct ProviderStore {
    pub active_id: String,
    pub providers: Vec<Provider>,
}

pub const OFFICIAL_PROVIDER_ID: &str = "official";
pub const OFFICIAL_PROVIDER_NAME: &str = "Official";
pub const MASKED_API_KEY: &str = "********";

impl Default for ProviderStore {
    fn default() -> Self {
        Self {
            active_id: OFFICIAL_PROVIDER_ID.to_string(),
            providers: vec![Provider {
                id: OFFICIAL_PROVIDER_ID.to_string(),
                name: OFFICIAL_PROVIDER_NAME.to_string(),
                kind: ProviderKind::Oauth,
                model: String::new(),
                base_url: String::new(),
                wire_api: "responses".to_string(),
                api_key: String::new(),
                compat: String::new(),
                model_mappings: Vec::new(),
                models: Vec::new(),
                catalog_models: Vec::new(),
                usage_page_url: String::new(),
            }],
        }
    }
}

pub fn providers_path(state_root: &Path) -> PathBuf {
    state_root.join("providers.json")
}

pub fn read_store(state_root: &Path) -> anyhow::Result<ProviderStore> {
    let path = providers_path(state_root);
    if !path.exists() {
        let store = ProviderStore::default();
        write_store(state_root, &store)?;
        return Ok(store);
    }
    let contents =
        fs::read_to_string(&path).with_context(|| format!("Failed to read {}", path.display()))?;
    let mut store: ProviderStore = serde_json::from_str(&contents)
        .with_context(|| format!("Failed to parse {}", path.display()))?;
    let mut changed = normalize_official_provider_name(&mut store);
    if pin_official_provider_first(&mut store) {
        changed = true;
    }
    if changed {
        write_store(state_root, &store)?;
    }
    Ok(store)
}

fn normalize_official_provider_name(store: &mut ProviderStore) -> bool {
    let mut changed = false;
    for provider in &mut store.providers {
        if provider.id == OFFICIAL_PROVIDER_ID && provider.name != OFFICIAL_PROVIDER_NAME {
            provider.name = OFFICIAL_PROVIDER_NAME.to_string();
            changed = true;
        }
    }
    changed
}

fn pin_official_provider_first(store: &mut ProviderStore) -> bool {
    let Some(index) = store
        .providers
        .iter()
        .position(|provider| provider.id == OFFICIAL_PROVIDER_ID)
    else {
        return false;
    };
    if index == 0 {
        return false;
    }
    let official = store.providers.remove(index);
    store.providers.insert(0, official);
    true
}

pub fn providers_in_display_order(store: &ProviderStore) -> Vec<&Provider> {
    let mut providers = Vec::with_capacity(store.providers.len());
    if let Some(official) = store
        .providers
        .iter()
        .find(|provider| provider.id == OFFICIAL_PROVIDER_ID)
    {
        providers.push(official);
    }
    providers.extend(
        store
            .providers
            .iter()
            .filter(|provider| provider.id != OFFICIAL_PROVIDER_ID),
    );
    providers
}

pub fn write_store(state_root: &Path, store: &ProviderStore) -> anyhow::Result<()> {
    let path = providers_path(state_root);
    let contents = format!("{}\n", serde_json::to_string_pretty(store)?);
    write_secret_file_atomic(&path, contents)
}

pub fn public_store(store: &ProviderStore) -> ProviderStore {
    let mut public = store.clone();
    for provider in &mut public.providers {
        if !provider.api_key.is_empty() {
            provider.api_key = MASKED_API_KEY.to_string();
        }
    }
    public
}

pub fn provider_api_key(state_root: &Path, id: &str) -> anyhow::Result<String> {
    let id = id.trim();
    if id.is_empty() {
        anyhow::bail!("Provider id is required");
    }
    if id == OFFICIAL_PROVIDER_ID {
        anyhow::bail!("The official provider does not store an API key");
    }
    let store = read_store(state_root)?;
    let provider = store
        .providers
        .iter()
        .find(|provider| provider.id == id)
        .ok_or_else(|| anyhow::anyhow!("Provider not found: {id}"))?;
    Ok(provider.api_key.clone())
}

pub fn reorder_providers(state_root: &Path, payload: &Value) -> anyhow::Result<ProviderStore> {
    let mut store = read_store(state_root)?;
    let ids = payload
        .get("ids")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow::anyhow!("Provider ids are required"))?;
    let mut requested = Vec::new();
    let mut seen = HashSet::new();
    for value in ids {
        let id = value
            .as_str()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| anyhow::anyhow!("Provider id is required"))?;
        if id == OFFICIAL_PROVIDER_ID {
            anyhow::bail!("The official provider cannot be reordered");
        }
        if !seen.insert(id.to_string()) {
            anyhow::bail!("Provider order must include each configured provider once");
        }
        requested.push(id.to_string());
    }
    let mut official = None;
    let mut others = Vec::new();
    for provider in store.providers.drain(..) {
        if provider.id == OFFICIAL_PROVIDER_ID {
            official = Some(provider);
        } else {
            others.push(provider);
        }
    }
    if requested.len() != others.len()
        || requested
            .iter()
            .any(|id| others.iter().all(|provider| provider.id != *id))
    {
        anyhow::bail!("Provider order must include each configured provider once");
    }
    others.sort_by_key(|provider| {
        requested
            .iter()
            .position(|id| id == &provider.id)
            .unwrap_or(usize::MAX)
    });
    if let Some(official) = official {
        store.providers.push(official);
    }
    store.providers.extend(others);
    write_store(state_root, &store)?;
    Ok(store)
}

fn normalize_id(value: &str) -> anyhow::Result<String> {
    let id = value
        .trim()
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>();
    let id = id.trim_matches('-').to_string();
    if id.is_empty() {
        anyhow::bail!("Provider id is required");
    }
    if id == "openai" {
        anyhow::bail!("Provider id openai is reserved");
    }
    Ok(id)
}

pub fn upsert_provider(
    state_root: &Path,
    payload: &Value,
) -> anyhow::Result<(ProviderStore, String)> {
    let mut store = read_store(state_root)?;
    let name = payload
        .get("name")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow::anyhow!("Provider name is required"))?;
    let requested_id = payload.get("id").and_then(Value::as_str).unwrap_or(name);
    let id = normalize_id(requested_id)?;
    if id == "official" {
        anyhow::bail!("The official ChatGPT provider cannot be overwritten");
    }
    let device_oauth = parse_device_oauth(payload)?;
    let kind = match payload
        .get("kind")
        .and_then(Value::as_str)
        .unwrap_or("apiKey")
    {
        "oauth" | "official" if device_oauth.is_none() => ProviderKind::Oauth,
        _ => ProviderKind::ApiKey,
    };
    if kind == ProviderKind::Oauth {
        anyhow::bail!("OAuth login is only available as the official ChatGPT provider");
    }
    let incoming_key = payload
        .get("apiKey")
        .or_else(|| payload.get("api_key"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_string();
    let existing_key = store
        .providers
        .iter()
        .find(|provider| provider.id == id)
        .map(|provider| provider.api_key.clone())
        .unwrap_or_default();
    let api_key = if incoming_key.is_empty() || incoming_key == MASKED_API_KEY {
        existing_key
    } else {
        incoming_key
    };
    let model_mappings = parse_model_mappings(payload)?;
    let existing_models = store
        .providers
        .iter()
        .find(|provider| provider.id == id)
        .map(|provider| provider.models.clone())
        .unwrap_or_default();
    let existing_catalog = store
        .providers
        .iter()
        .find(|provider| provider.id == id)
        .map(|provider| provider.catalog_models.clone())
        .unwrap_or_default();
    let catalog_models = parse_catalog_models(payload, &existing_catalog)?;
    let models = if payload.get("models").is_some() {
        parse_models(payload, &existing_models)?
    } else if !catalog_models.is_empty() {
        catalog_models
            .iter()
            .map(|entry| entry.model.clone())
            .collect()
    } else {
        parse_models(payload, &existing_models)?
    };
    let mut provider = Provider {
        id: id.clone(),
        name: name.to_string(),
        kind,
        model: payload
            .get("model")
            .and_then(Value::as_str)
            .unwrap_or("")
            .trim()
            .to_string(),
        base_url: payload
            .get("baseUrl")
            .or_else(|| payload.get("base_url"))
            .and_then(Value::as_str)
            .unwrap_or("")
            .trim()
            .to_string(),
        wire_api: {
            let wire_api = payload
                .get("wireApi")
                .or_else(|| payload.get("wire_api"))
                .and_then(Value::as_str)
                .unwrap_or("responses")
                .trim();
            if wire_api.is_empty() {
                "responses".to_string()
            } else if wire_api == "responses" || wire_api == "chat" {
                wire_api.to_string()
            } else {
                anyhow::bail!("wireApi must be responses or chat");
            }
        },
        api_key,
        compat: payload
            .get("compat")
            .and_then(Value::as_str)
            .unwrap_or("")
            .trim()
            .to_string(),
        model_mappings,
        models,
        catalog_models,
        usage_page_url: parse_usage_page_url(payload)?,
    };
    if let Some(kind) = device_oauth {
        provider.compat = kind.compat_name().to_string();
        provider.base_url = kind.default_base_url().to_string();
        provider.wire_api = kind.default_wire_api().to_string();
        provider.api_key.clear();
    } else if provider.compat.is_empty()
        && provider.base_url.to_ascii_lowercase().contains("api.x.ai")
    {
        provider.compat = "xai".to_string();
    }
    if provider.base_url.is_empty() {
        anyhow::bail!("Provider base URL is required");
    }
    if provider_device_oauth_kind(&provider).is_none() && provider.model.trim().is_empty() {
        anyhow::bail!("Provider model is required");
    }
    if let Some(existing) = store.providers.iter_mut().find(|item| item.id == id) {
        *existing = provider;
    } else {
        store.providers.push(provider);
    }
    write_store(state_root, &store)?;
    Ok((store, id))
}

fn parse_usage_page_url(payload: &Value) -> anyhow::Result<String> {
    let raw = payload
        .get("usagePageUrl")
        .or_else(|| payload.get("usage_page_url"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim();
    if raw.is_empty() {
        return Ok(String::new());
    }
    let lower = raw.to_ascii_lowercase();
    if !(lower.starts_with("https://") || lower.starts_with("http://")) {
        anyhow::bail!("Usage URL must be an http or https link");
    }
    if raw.chars().any(|ch| ch.is_ascii_whitespace() || ch == '\0') {
        anyhow::bail!("Usage URL must be an http or https link");
    }
    Ok(raw.to_string())
}

pub fn delete_provider(
    state_root: &Path,
    id: &str,
    _codex_home: &Path,
) -> anyhow::Result<ProviderStore> {
    let id = normalize_id(id)?;
    if id == "official" {
        anyhow::bail!("The official ChatGPT provider cannot be deleted");
    }
    let mut store = read_store(state_root)?;
    if store.active_id == id {
        anyhow::bail!("The active provider cannot be deleted");
    }
    let exists = store.providers.iter().any(|provider| provider.id == id);
    if !exists {
        anyhow::bail!("Provider not found: {id}");
    }
    store.providers.retain(|provider| provider.id != id);
    write_store(state_root, &store)?;
    Ok(store)
}

pub fn provider_needs_xai_compat(provider: &Provider) -> bool {
    let compat = provider.compat.trim().to_ascii_lowercase();
    compat == "xai"
        || compat == "xai-oauth"
        || compat == "xai_oauth"
        || provider.base_url.to_ascii_lowercase().contains("api.x.ai")
}

pub fn provider_is_deepseek(provider: &Provider) -> bool {
    let haystack = format!(
        "{} {} {} {}",
        provider.id, provider.name, provider.base_url, provider.compat
    )
    .to_ascii_lowercase();
    haystack.contains("deepseek")
}

/// DeepSeek `/v1/responses` accepts custom tools but only `apply_patch`.
/// Other named custom tools (especially Codex `exec`) are rewritten to functions.
pub fn provider_needs_deepseek_responses_sanitize(provider: &Provider) -> bool {
    provider.wire_api.trim().eq_ignore_ascii_case("responses") && provider_is_deepseek(provider)
}

pub fn provider_allowed_models(provider: &Provider) -> HashSet<String> {
    provider_available_models(provider).into_iter().collect()
}

pub fn provider_available_models(provider: &Provider) -> Vec<String> {
    let mut models = Vec::new();
    let mut push = |value: &str| {
        let value = value.trim();
        if value.is_empty() {
            return;
        }
        if models
            .iter()
            .any(|existing: &String| existing.eq_ignore_ascii_case(value))
        {
            return;
        }
        models.push(value.to_string());
    };
    push(&provider.model);
    for model in &provider.models {
        push(model);
    }
    for item in &provider.catalog_models {
        push(&item.model);
    }
    models
}

pub fn provider_device_oauth_kind(provider: &Provider) -> Option<OAuthKind> {
    oauth_kind_from_provider(&provider.compat, &provider.base_url)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LiveRefresh {
    NewConversation,
    RestartDesktop,
}

impl LiveRefresh {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::NewConversation => "new_conversation",
            Self::RestartDesktop => "restart_desktop",
        }
    }
}

pub fn provider_is_official(provider: &Provider) -> bool {
    provider.id == "official" || provider.kind == ProviderKind::Oauth
}

pub fn live_refresh_for_switch(
    previous_id: &str,
    previous: Option<&Provider>,
    next: &Provider,
) -> LiveRefresh {
    let previous_official = previous
        .map(provider_is_official)
        .unwrap_or(previous_id == "official");
    if previous_official != provider_is_official(next) {
        LiveRefresh::RestartDesktop
    } else {
        LiveRefresh::NewConversation
    }
}

pub fn activate_provider(
    state_root: &Path,
    id: &str,
    proxy_base_url: &str,
    codex_home: &Path,
) -> anyhow::Result<(ProviderStore, LiveRefresh)> {
    let id = normalize_id(id)?;
    let mut store = read_store(state_root)?;
    let provider = store
        .providers
        .iter()
        .find(|provider| provider.id == id)
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("Provider not found: {id}"))?;
    let previous_id = store.active_id.clone();
    let previous = store
        .providers
        .iter()
        .find(|item| item.id == previous_id)
        .cloned();
    let refresh = live_refresh_for_switch(&previous_id, previous.as_ref(), &provider);
    let previous_model = store
        .providers
        .iter()
        .find(|item| item.id == previous_id)
        .map(|item| item.model.clone())
        .filter(|model| !model.is_empty());
    let mut document = read_config_document(codex_home)?;
    match provider.kind {
        ProviderKind::Oauth => {
            apply_official_provider(&mut document, Some(&previous_id), previous_model.as_deref());
            clear_helper_catalog(codex_home, &mut document)?;
        }
        ProviderKind::ApiKey => {
            if proxy_base_url.trim().is_empty() {
                anyhow::bail!("Provider proxy is not listening");
            }
            if provider_needs_xai_compat(&provider) && provider.model.trim().is_empty() {
                anyhow::bail!("xAI provider model is required");
            }
            let preserve_openai_login = read_auth(codex_home)?
                .as_ref()
                .is_some_and(auth_has_oauth_login);
            let live_base_url = proxy_base_url;
            let live_api_key = if let Some(kind) = provider_device_oauth_kind(&provider) {
                if !oauth_is_signed_in(state_root, kind) {
                    anyhow::bail!("{} is not signed in", kind.label());
                }
                HELPER_OAUTH_LIVE_TOKEN.to_string()
            } else if provider.api_key.trim().is_empty() {
                anyhow::bail!("Provider API key is required");
            } else {
                provider.api_key.clone()
            };
            apply_api_provider(
                &mut document,
                LiveProviderWrite {
                    id: UNIFIED_SESSION_PROVIDER_ID,
                    name: &provider.name,
                    model: &provider.model,
                    base_url: live_base_url,
                    wire_api: &provider.wire_api,
                    api_key: Some(live_api_key.as_str()),
                    preserve_openai_login,
                    previous_id: Some(previous_id.as_str())
                        .filter(|id| *id != provider.id && *id != UNIFIED_SESSION_PROVIDER_ID),
                },
            );
            apply_provider_catalog(codex_home, &mut document, &provider)?;
        }
    }
    write_config_atomic(codex_home, &document)?;
    store.active_id = provider.id;
    write_store(state_root, &store)?;
    Ok((store, refresh))
}

fn parse_device_oauth(payload: &Value) -> anyhow::Result<Option<OAuthKind>> {
    let auth_mode = payload
        .get("authMode")
        .or_else(|| payload.get("auth_mode"))
        .or_else(|| payload.get("compat"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim();
    if auth_mode.is_empty()
        || auth_mode.eq_ignore_ascii_case("apiKey")
        || auth_mode.eq_ignore_ascii_case("api-key")
        || auth_mode.eq_ignore_ascii_case("xai")
    {
        return Ok(None);
    }
    if auth_mode.eq_ignore_ascii_case("oauthProxy") || auth_mode.eq_ignore_ascii_case("oauth-proxy")
    {
        anyhow::bail!("CLIProxyAPI is not supported. Use GitHub Copilot or xAI Grok sign-in");
    }
    OAuthKind::parse(auth_mode).map(Some)
}

fn parse_model_mappings(payload: &Value) -> anyhow::Result<Vec<ModelMapping>> {
    let Some(list) = payload
        .get("modelMappings")
        .or_else(|| payload.get("model_mappings"))
    else {
        return Ok(Vec::new());
    };
    let Some(items) = list.as_array() else {
        anyhow::bail!("modelMappings must be an array");
    };
    let mut mappings = Vec::new();
    for item in items {
        let source = item
            .get("source")
            .or_else(|| item.get("from"))
            .and_then(Value::as_str)
            .unwrap_or("")
            .trim();
        let target = item
            .get("target")
            .or_else(|| item.get("to"))
            .and_then(Value::as_str)
            .unwrap_or("")
            .trim();
        if source.is_empty() && target.is_empty() {
            continue;
        }
        if source.is_empty() || target.is_empty() {
            anyhow::bail!("Each model mapping needs a source and target");
        }
        mappings.push(ModelMapping {
            source: source.to_string(),
            target: target.to_string(),
        });
    }
    Ok(mappings)
}

const CANONICAL_REASONING_LEVELS: &[&str] = &[
    "none", "minimal", "low", "medium", "high", "xhigh", "max", "ultra",
];

fn parse_catalog_models(
    payload: &Value,
    existing: &[CatalogModel],
) -> anyhow::Result<Vec<CatalogModel>> {
    let Some(list) = payload
        .get("catalogModels")
        .or_else(|| payload.get("catalog_models"))
    else {
        return Ok(existing.to_vec());
    };
    let Some(items) = list.as_array() else {
        anyhow::bail!("catalogModels must be an array");
    };
    let mut models = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for item in items {
        let model = item
            .get("model")
            .or_else(|| item.get("slug"))
            .and_then(Value::as_str)
            .unwrap_or("")
            .trim();
        if model.is_empty() || !seen.insert(model.to_string()) {
            continue;
        }
        let display_name = item
            .get("displayName")
            .or_else(|| item.get("display_name"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .unwrap_or(model)
            .to_string();
        let context_window = item
            .get("contextWindow")
            .or_else(|| item.get("context_window"))
            .and_then(|value| {
                value
                    .as_u64()
                    .or_else(|| value.as_str().and_then(|text| text.trim().parse().ok()))
            })
            .filter(|value| *value > 0);
        let reasoning_levels = canonical_reasoning_levels(
            item.get("reasoningLevels")
                .or_else(|| item.get("reasoning_levels")),
        );
        let default_reasoning_level = item
            .get("defaultReasoningLevel")
            .or_else(|| item.get("default_reasoning_level"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|level| reasoning_levels.iter().any(|item| item == level))
            .unwrap_or("")
            .to_string();
        models.push(CatalogModel {
            model: model.to_string(),
            display_name,
            context_window,
            reasoning_levels,
            default_reasoning_level,
        });
    }
    Ok(models)
}

fn canonical_reasoning_levels(value: Option<&Value>) -> Vec<String> {
    let Some(items) = value.and_then(Value::as_array) else {
        return Vec::new();
    };
    CANONICAL_REASONING_LEVELS
        .iter()
        .filter(|level| {
            items.iter().any(|item| {
                item.as_str()
                    .map(str::trim)
                    .is_some_and(|candidate| candidate.eq_ignore_ascii_case(level))
            })
        })
        .map(|level| (*level).to_string())
        .collect()
}

fn parse_models(payload: &Value, existing: &[String]) -> anyhow::Result<Vec<String>> {
    let Some(list) = payload.get("models") else {
        return Ok(existing.to_vec());
    };
    let Some(items) = list.as_array() else {
        anyhow::bail!("models must be an array");
    };
    let mut models = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for item in items {
        let slug = item.as_str().map(str::trim).unwrap_or("");
        if slug.is_empty() || !seen.insert(slug.to_string()) {
            continue;
        }
        models.push(slug.to_string());
    }
    Ok(models)
}

pub fn apply_provider_model_mappings(body: &mut Value, mappings: &[ModelMapping]) -> bool {
    if mappings.is_empty() {
        return false;
    }
    let Some(object) = body.as_object_mut() else {
        return false;
    };
    let request = object
        .get("model")
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or("")
        .to_string();
    if request.is_empty() {
        return false;
    }
    let Some(mapping) = mappings
        .iter()
        .find(|mapping| mapping.source.eq_ignore_ascii_case(&request))
    else {
        return false;
    };
    object.insert("model".to_string(), Value::String(mapping.target.clone()));
    true
}

pub fn rewrite_unmatched_request_model(
    body: &mut Value,
    default_model: &str,
    allowed_models: &HashSet<String>,
) -> bool {
    let default = default_model.trim();
    if default.is_empty() {
        return false;
    }
    let Some(object) = body.as_object_mut() else {
        return false;
    };
    let request = object
        .get("model")
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or("")
        .to_string();
    if !request.is_empty()
        && (request.eq_ignore_ascii_case(default)
            || allowed_models
                .iter()
                .any(|id| id.eq_ignore_ascii_case(&request)))
    {
        return false;
    }
    object.insert("model".to_string(), Value::String(default.to_string()));
    true
}

pub fn list_response(store: &ProviderStore, proxy_base_url: &str) -> Value {
    json!({
        "status": "ok",
        "activeId": store.active_id,
        "proxyBaseUrl": proxy_base_url,
        "providers": public_store(store).providers,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn default_store_includes_official_provider() {
        let store = ProviderStore::default();
        assert_eq!(store.active_id, OFFICIAL_PROVIDER_ID);
        assert_eq!(store.providers[0].id, OFFICIAL_PROVIDER_ID);
        assert_eq!(store.providers[0].name, OFFICIAL_PROVIDER_NAME);
        assert_eq!(store.providers[0].kind, ProviderKind::Oauth);
    }

    #[test]
    fn read_store_renames_legacy_official_provider() {
        let dir = tempdir().unwrap();
        let mut store = ProviderStore::default();
        store.providers[0].name = "ChatGPT / Codex".to_string();
        write_store(dir.path(), &store).unwrap();

        let loaded = read_store(dir.path()).unwrap();
        assert_eq!(loaded.providers[0].name, OFFICIAL_PROVIDER_NAME);

        let persisted: ProviderStore =
            serde_json::from_str(&fs::read_to_string(providers_path(dir.path())).unwrap()).unwrap();
        assert_eq!(persisted.providers[0].name, OFFICIAL_PROVIDER_NAME);
    }

    #[test]
    fn provider_api_key_returns_unmasked_secret() {
        let dir = tempdir().unwrap();
        let mut store = ProviderStore::default();
        store.providers.push(sample_api_provider("grok", "Grok"));
        write_store(dir.path(), &store).unwrap();

        assert_eq!(provider_api_key(dir.path(), "grok").unwrap(), "sk-test");
        assert_eq!(
            public_store(&read_store(dir.path()).unwrap())
                .providers
                .iter()
                .find(|provider| provider.id == "grok")
                .unwrap()
                .api_key,
            MASKED_API_KEY
        );
        assert!(provider_api_key(dir.path(), OFFICIAL_PROVIDER_ID).is_err());
        assert!(provider_api_key(dir.path(), "missing").is_err());
    }

    #[test]
    fn read_store_pins_official_provider_first() {
        let dir = tempdir().unwrap();
        let mut store = ProviderStore::default();
        store
            .providers
            .insert(0, sample_api_provider("grok", "Grok"));
        write_store(dir.path(), &store).unwrap();

        let loaded = read_store(dir.path()).unwrap();
        assert_eq!(loaded.providers[0].id, OFFICIAL_PROVIDER_ID);
        assert_eq!(loaded.providers[1].id, "grok");
    }

    #[test]
    fn reorder_providers_keeps_official_first() {
        let dir = tempdir().unwrap();
        let mut store = ProviderStore::default();
        store.providers.push(sample_api_provider("grok", "Grok"));
        store
            .providers
            .push(sample_api_provider("deepseek", "DeepSeek"));
        write_store(dir.path(), &store).unwrap();

        let loaded =
            reorder_providers(dir.path(), &json!({ "ids": ["deepseek", "grok"] })).unwrap();
        let ids: Vec<_> = loaded
            .providers
            .iter()
            .map(|provider| provider.id.as_str())
            .collect();
        assert_eq!(ids, ["official", "deepseek", "grok"]);
    }

    #[test]
    fn reorder_providers_rejects_official_and_partial_lists() {
        let dir = tempdir().unwrap();
        let mut store = ProviderStore::default();
        store.providers.push(sample_api_provider("grok", "Grok"));
        store
            .providers
            .push(sample_api_provider("deepseek", "DeepSeek"));
        write_store(dir.path(), &store).unwrap();

        let official = reorder_providers(
            dir.path(),
            &json!({ "ids": ["official", "grok", "deepseek"] }),
        )
        .unwrap_err();
        assert!(official.to_string().contains("cannot be reordered"));

        let partial = reorder_providers(dir.path(), &json!({ "ids": ["grok"] })).unwrap_err();
        assert!(partial.to_string().contains("once"));
    }

    fn sample_api_provider(id: &str, name: &str) -> Provider {
        Provider {
            id: id.to_string(),
            name: name.to_string(),
            kind: ProviderKind::ApiKey,
            model: "model".to_string(),
            base_url: "https://example.com/v1".to_string(),
            wire_api: "responses".to_string(),
            api_key: "sk-test".to_string(),
            compat: String::new(),
            model_mappings: Vec::new(),
            models: Vec::new(),
            catalog_models: Vec::new(),
            usage_page_url: String::new(),
        }
    }

    #[test]
    fn live_refresh_restarts_desktop_when_crossing_official_login() {
        let official = ProviderStore::default().providers[0].clone();
        let grok = sample_api_provider("grok", "Grok");
        let deepseek = sample_api_provider("deepseek", "DeepSeek");
        assert_eq!(
            live_refresh_for_switch("official", Some(&official), &grok),
            LiveRefresh::RestartDesktop
        );
        assert_eq!(
            live_refresh_for_switch("grok", Some(&grok), &official),
            LiveRefresh::RestartDesktop
        );
        assert_eq!(
            live_refresh_for_switch("grok", Some(&grok), &deepseek),
            LiveRefresh::NewConversation
        );
        assert_eq!(
            live_refresh_for_switch("grok", Some(&grok), &grok),
            LiveRefresh::NewConversation
        );
    }

    #[test]
    fn upsert_keeps_existing_key_when_masked() {
        let temp = tempdir().expect("temp");
        let root = temp.path().join(".codex-helper");
        write_store(
            &root,
            &ProviderStore {
                active_id: "official".to_string(),
                providers: vec![
                    ProviderStore::default().providers[0].clone(),
                    Provider {
                        id: "grok".to_string(),
                        name: "Grok".to_string(),
                        kind: ProviderKind::ApiKey,
                        model: "grok-4.6".to_string(),
                        base_url: "https://api.x.ai/v1".to_string(),
                        wire_api: "responses".to_string(),
                        api_key: "sk-live".to_string(),
                        compat: "xai".to_string(),
                        model_mappings: Vec::new(),
                        models: Vec::new(),
                        catalog_models: Vec::new(),
                        usage_page_url: String::new(),
                    },
                ],
            },
        )
        .expect("store");
        let (store, saved_id) = upsert_provider(
            &root,
            &json!({
                "id": "grok",
                "name": "Grok",
                "kind": "apiKey",
                "baseUrl": "https://api.x.ai/v1",
                "model": "grok-4.6",
                "apiKey": "********"
            }),
        )
        .expect("upsert");
        assert_eq!(saved_id, "grok");
        let grok = store
            .providers
            .iter()
            .find(|item| item.id == "grok")
            .unwrap();
        assert_eq!(grok.api_key, "sk-live");
    }

    #[test]
    fn xai_compat_detects_api_host() {
        let provider = Provider {
            base_url: "https://api.x.ai/v1".to_string(),
            ..Provider::default()
        };
        assert!(provider_needs_xai_compat(&provider));
    }

    #[test]
    fn allowed_models_include_selected_list_and_catalog() {
        let provider = Provider {
            model: "grok-4.6".to_string(),
            models: vec!["grok-4.5".to_string()],
            catalog_models: vec![CatalogModel {
                model: "grok-4.20".to_string(),
                ..CatalogModel::default()
            }],
            ..Provider::default()
        };
        let allowed = provider_allowed_models(&provider);
        assert!(allowed.contains("grok-4.6"));
        assert!(allowed.contains("grok-4.5"));
        assert!(allowed.contains("grok-4.20"));
    }

    #[test]
    fn available_models_keep_default_first_and_dedupe() {
        let provider = Provider {
            model: "grok-4.6".to_string(),
            models: vec!["grok-4.5".to_string(), "GROK-4.6".to_string()],
            catalog_models: vec![CatalogModel {
                model: "grok-4.20".to_string(),
                ..CatalogModel::default()
            }],
            ..Provider::default()
        };
        assert_eq!(
            provider_available_models(&provider),
            vec![
                "grok-4.6".to_string(),
                "grok-4.5".to_string(),
                "grok-4.20".to_string()
            ]
        );
    }

    #[test]
    fn deepseek_responses_needs_exec_filter() {
        let provider = Provider {
            id: "deepseek".to_string(),
            name: "DeepSeek".to_string(),
            base_url: "https://api.deepseek.com/v1".to_string(),
            wire_api: "responses".to_string(),
            ..Provider::default()
        };
        assert!(provider_needs_deepseek_responses_sanitize(&provider));
    }

    #[test]
    fn non_deepseek_responses_does_not_need_exec_filter() {
        let provider = Provider {
            id: "other".to_string(),
            name: "Generic".to_string(),
            base_url: "https://api.example.com".to_string(),
            wire_api: "responses".to_string(),
            ..Provider::default()
        };
        assert!(!provider_needs_deepseek_responses_sanitize(&provider));
    }

    #[test]
    fn deepseek_chat_does_not_need_exec_filter() {
        let provider = Provider {
            id: "deepseek".to_string(),
            name: "DeepSeek".to_string(),
            base_url: "https://api.deepseek.com".to_string(),
            wire_api: "chat".to_string(),
            ..Provider::default()
        };
        assert!(!provider_needs_deepseek_responses_sanitize(&provider));
    }

    #[test]
    fn activate_api_provider_writes_live_config() {
        let helper = tempdir().expect("helper");
        let codex = tempdir().expect("codex");
        let root = helper.path().join(".codex-helper");
        write_store(
            &root,
            &ProviderStore {
                active_id: "official".to_string(),
                providers: vec![
                    ProviderStore::default().providers[0].clone(),
                    Provider {
                        id: "grok".to_string(),
                        name: "Grok".to_string(),
                        kind: ProviderKind::ApiKey,
                        model: "grok-4.6".to_string(),
                        base_url: "https://api.x.ai/v1".to_string(),
                        wire_api: "responses".to_string(),
                        api_key: "sk-live".to_string(),
                        compat: "xai".to_string(),
                        model_mappings: Vec::new(),
                        models: Vec::new(),
                        catalog_models: Vec::new(),
                        usage_page_url: String::new(),
                    },
                ],
            },
        )
        .expect("store");
        let (store, refresh) =
            activate_provider(&root, "grok", "http://127.0.0.1:3721/v1", codex.path())
                .expect("activate");
        assert_eq!(store.active_id, "grok");
        assert_eq!(refresh, LiveRefresh::RestartDesktop);
        let live = std::fs::read_to_string(codex.path().join("config.toml")).expect("config");
        assert!(live.contains("model_provider = \"custom\""));
        assert!(live.contains("[model_providers.custom]"));
        assert!(!live.contains("[model_providers.grok]"));
        assert!(live.contains("base_url = \"http://127.0.0.1:3721/v1\""));
        assert!(live.contains("experimental_bearer_token = \"sk-live\""));
        assert!(live.contains("model_catalog_json = \"codex-helper-model-catalog.json\""));
        let catalog = std::fs::read_to_string(codex.path().join("codex-helper-model-catalog.json"))
            .expect("catalog");
        assert!(catalog.contains("grok-4.6"));
    }

    #[test]
    fn activate_xai_oauth_provider_writes_placeholder_bearer() {
        let helper = tempdir().expect("helper");
        let codex = tempdir().expect("codex");
        let root = helper.path().join(".codex-helper");
        write_store(
            &root,
            &ProviderStore {
                active_id: "official".to_string(),
                providers: vec![
                    ProviderStore::default().providers[0].clone(),
                    Provider {
                        id: "grok".to_string(),
                        name: "Grok".to_string(),
                        kind: ProviderKind::ApiKey,
                        model: "grok-4.6".to_string(),
                        base_url: "https://api.x.ai/v1".to_string(),
                        wire_api: "responses".to_string(),
                        api_key: String::new(),
                        compat: "xai-oauth".to_string(),
                        model_mappings: Vec::new(),
                        models: Vec::new(),
                        catalog_models: Vec::new(),
                        usage_page_url: String::new(),
                    },
                ],
            },
        )
        .expect("store");
        std::fs::create_dir_all(root.join("oauth")).expect("oauth dir");
        std::fs::write(
            root.join("oauth/xai.json"),
            r#"{
  "login": "tester",
  "accountId": "acct-1",
  "refreshToken": "refresh-token",
  "accessToken": "access-token",
  "expiresAtMs": 9999999999999
}"#,
        )
        .expect("xai oauth");
        std::fs::write(
            codex.path().join("auth.json"),
            r#"{"auth_mode":"chatgpt","tokens":{"access_token":"chatgpt-token"}}"#,
        )
        .expect("auth");
        let (store, refresh) =
            activate_provider(&root, "grok", "http://127.0.0.1:3721/v1", codex.path())
                .expect("activate");
        assert_eq!(store.active_id, "grok");
        assert_eq!(refresh, LiveRefresh::RestartDesktop);
        let live = std::fs::read_to_string(codex.path().join("config.toml")).expect("config");
        assert!(live.contains("model_provider = \"custom\""));
        assert!(live.contains(&format!(
            "experimental_bearer_token = \"{HELPER_OAUTH_LIVE_TOKEN}\""
        )));
        assert!(live.contains("requires_openai_auth = true"));
        assert!(live.contains("cli_auth_credentials_store = \"file\""));
        assert!(!live.contains("chatgpt-token"));
        assert!(!live.contains("[model_providers.grok]"));
    }

    #[test]
    fn delete_active_provider_is_rejected() {
        let helper = tempdir().expect("helper");
        let codex = tempdir().expect("codex");
        let root = helper.path().join(".codex-helper");
        write_store(
            &root,
            &ProviderStore {
                active_id: "grok".to_string(),
                providers: vec![
                    ProviderStore::default().providers[0].clone(),
                    Provider {
                        id: "grok".to_string(),
                        name: "Grok".to_string(),
                        kind: ProviderKind::ApiKey,
                        model: "grok-4.6".to_string(),
                        base_url: "https://api.x.ai/v1".to_string(),
                        wire_api: "responses".to_string(),
                        api_key: "sk-live".to_string(),
                        compat: "xai".to_string(),
                        model_mappings: Vec::new(),
                        models: Vec::new(),
                        catalog_models: Vec::new(),
                        usage_page_url: String::new(),
                    },
                ],
            },
        )
        .expect("store");
        std::fs::write(
            codex.path().join("config.toml"),
            "model_provider = \"grok\"\nmodel = \"grok-4.6\"\n\n[model_providers.grok]\nname = \"Grok\"\n",
        )
        .expect("seed config");
        let error = delete_provider(&root, "grok", codex.path()).expect_err("active");
        assert!(error
            .to_string()
            .contains("active provider cannot be deleted"));
        assert_eq!(read_store(&root).expect("store").active_id, "grok");
    }

    #[test]
    fn model_mappings_are_saved_and_applied() {
        let temp = tempdir().expect("temp");
        let root = temp.path().join(".codex-helper");
        let (store, saved_id) = upsert_provider(
            &root,
            &json!({
                "name": "Grok",
                "kind": "apiKey",
                "baseUrl": "https://api.x.ai/v1",
                "model": "grok-4.6",
                "apiKey": "sk-live",
                "modelMappings": [{ "source": "gpt-5.6-sol", "target": "grok-4.6" }]
            }),
        )
        .expect("upsert");
        assert_eq!(saved_id, "grok");
        let grok = store
            .providers
            .iter()
            .find(|item| item.id == "grok")
            .unwrap();
        assert_eq!(grok.model_mappings[0].source, "gpt-5.6-sol");
        let mut body = json!({ "model": "gpt-5.6-sol" });
        assert!(apply_provider_model_mappings(
            &mut body,
            &grok.model_mappings
        ));
        assert_eq!(body["model"], "grok-4.6");
    }

    #[test]
    fn unmatched_model_rewrites_to_default() {
        let provider = Provider {
            model: "deepseek-v4-flash".to_string(),
            models: vec!["deepseek-chat".to_string()],
            ..Provider::default()
        };
        let allowed = provider_allowed_models(&provider);
        let mut unknown = json!({ "model": "gpt-5.6-luna" });
        assert!(rewrite_unmatched_request_model(
            &mut unknown,
            &provider.model,
            &allowed
        ));
        assert_eq!(unknown["model"], "deepseek-v4-flash");

        let mut catalog = json!({ "model": "deepseek-chat" });
        assert!(!rewrite_unmatched_request_model(
            &mut catalog,
            &provider.model,
            &allowed
        ));
        assert_eq!(catalog["model"], "deepseek-chat");

        let mut missing = json!({ "input": [] });
        assert!(rewrite_unmatched_request_model(
            &mut missing,
            &provider.model,
            &allowed
        ));
        assert_eq!(missing["model"], "deepseek-v4-flash");
    }

    #[test]
    fn upsert_device_oauth_ignores_leftover_cliproxy_base_url() {
        let temp = tempdir().expect("temp");
        let root = temp.path().join(".codex-helper");
        let (store, _) = upsert_provider(
            &root,
            &json!({
                "name": "GitHub Copilot",
                "authMode": "github_copilot",
                "baseUrl": "http://127.0.0.1:8317/v1",
                "wireApi": "responses"
            }),
        )
        .expect("upsert");
        let provider = store
            .providers
            .iter()
            .find(|item| item.id == "github-copilot")
            .unwrap();
        assert_eq!(provider.base_url, "https://api.githubcopilot.com");
        assert_eq!(provider.wire_api, "chat");
        assert_eq!(provider.compat, "github-copilot");
    }

    #[test]
    fn upsert_device_oauth_does_not_require_api_key() {
        let temp = tempdir().expect("temp");
        let root = temp.path().join(".codex-helper");
        let (store, saved_id) = upsert_provider(
            &root,
            &json!({
                "name": "Copilot",
                "authMode": "github_copilot"
            }),
        )
        .expect("upsert");
        assert_eq!(saved_id, "copilot");
        let provider = store
            .providers
            .iter()
            .find(|item| item.id == "copilot")
            .unwrap();
        assert_eq!(provider.compat, "github-copilot");
        assert_eq!(provider.base_url, "https://api.githubcopilot.com");
        assert_eq!(provider.wire_api, "chat");
        assert!(provider.api_key.is_empty());
    }

    #[test]
    fn upsert_rejects_cliproxy_auth_mode() {
        let temp = tempdir().expect("temp");
        let root = temp.path().join(".codex-helper");
        let error = upsert_provider(
            &root,
            &json!({
                "name": "Proxy",
                "authMode": "oauthProxy",
                "baseUrl": "http://127.0.0.1:8317/v1"
            }),
        )
        .expect_err("cliproxy");
        assert!(error.to_string().contains("CLIProxyAPI"));
    }

    #[test]
    fn upsert_saves_custom_usage_page_url() {
        let temp = tempdir().expect("temp");
        let root = temp.path().join(".codex-helper");
        let (store, saved_id) = upsert_provider(
            &root,
            &json!({
                "name": "Custom",
                "baseUrl": "https://api.example.com/v1",
                "model": "custom-model",
                "usagePageUrl": "https://status.example.com/usage"
            }),
        )
        .expect("upsert");
        assert_eq!(saved_id, "custom");
        let provider = store
            .providers
            .iter()
            .find(|item| item.id == "custom")
            .unwrap();
        assert_eq!(provider.usage_page_url, "https://status.example.com/usage");
    }

    #[test]
    fn upsert_rejects_non_http_usage_page_url() {
        let temp = tempdir().expect("temp");
        let root = temp.path().join(".codex-helper");
        let error = upsert_provider(
            &root,
            &json!({
                "name": "Custom",
                "baseUrl": "https://api.example.com/v1",
                "usagePageUrl": "javascript:alert(1)"
            }),
        )
        .expect_err("usage url");
        assert!(error.to_string().contains("http or https"));
    }

    #[cfg(unix)]
    #[test]
    fn write_store_sets_secret_file_permissions() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempdir().expect("temp");
        let mut store = ProviderStore::default();
        store.providers.push(sample_api_provider("grok", "Grok"));
        store.providers.last_mut().unwrap().api_key = "sk-test".to_string();
        write_store(dir.path(), &store).unwrap();
        let mode = fs::metadata(providers_path(dir.path()))
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o600);
    }

    #[test]
    fn upsert_omitted_models_keeps_existing_list() {
        let temp = tempdir().expect("temp");
        let root = temp.path().join(".codex-helper");
        write_store(
            &root,
            &ProviderStore {
                active_id: OFFICIAL_PROVIDER_ID.to_string(),
                providers: vec![
                    ProviderStore::default().providers[0].clone(),
                    Provider {
                        id: "grok".to_string(),
                        name: "Grok".to_string(),
                        kind: ProviderKind::ApiKey,
                        model: "grok-4.6".to_string(),
                        base_url: "https://api.x.ai/v1".to_string(),
                        wire_api: "responses".to_string(),
                        api_key: "sk-live".to_string(),
                        models: vec!["grok-4.5".to_string()],
                        ..Provider::default()
                    },
                ],
            },
        )
        .expect("store");
        let (store, _) = upsert_provider(
            &root,
            &json!({
                "id": "grok",
                "name": "Grok",
                "kind": "apiKey",
                "baseUrl": "https://api.x.ai/v1",
                "model": "grok-4.6",
                "apiKey": "********"
            }),
        )
        .expect("upsert");
        let grok = store
            .providers
            .iter()
            .find(|item| item.id == "grok")
            .unwrap();
        assert_eq!(grok.models, vec!["grok-4.5".to_string()]);
    }

    #[test]
    fn upsert_api_key_provider_requires_model() {
        let temp = tempdir().expect("temp");
        let root = temp.path().join(".codex-helper");
        let error = upsert_provider(
            &root,
            &json!({
                "name": "DeepSeek",
                "baseUrl": "https://api.deepseek.com/v1",
                "apiKey": "sk-live"
            }),
        )
        .expect_err("model");
        assert!(error.to_string().contains("Provider model is required"));
    }

    #[test]
    fn activate_device_oauth_requires_sign_in() {
        let helper = tempdir().expect("helper");
        let codex = tempdir().expect("codex");
        let root = helper.path().join(".codex-helper");
        write_store(
            &root,
            &ProviderStore {
                active_id: "official".to_string(),
                providers: vec![
                    ProviderStore::default().providers[0].clone(),
                    Provider {
                        id: "copilot".to_string(),
                        name: "Copilot".to_string(),
                        kind: ProviderKind::ApiKey,
                        model: "gpt-5".to_string(),
                        base_url: "https://api.githubcopilot.com".to_string(),
                        wire_api: "chat".to_string(),
                        api_key: String::new(),
                        compat: "github-copilot".to_string(),
                        model_mappings: Vec::new(),
                        models: Vec::new(),
                        catalog_models: Vec::new(),
                        usage_page_url: String::new(),
                    },
                ],
            },
        )
        .expect("store");
        let error = activate_provider(&root, "copilot", "http://127.0.0.1:3721/v1", codex.path())
            .expect_err("signed out");
        assert!(error.to_string().contains("not signed in"));
    }

    #[test]
    fn activate_between_third_party_providers_asks_for_new_conversation() {
        let helper = tempdir().expect("helper");
        let codex = tempdir().expect("codex");
        let root = helper.path().join(".codex-helper");
        write_store(
            &root,
            &ProviderStore {
                active_id: "grok".to_string(),
                providers: vec![
                    ProviderStore::default().providers[0].clone(),
                    sample_api_provider("grok", "Grok"),
                    sample_api_provider("deepseek", "DeepSeek"),
                ],
            },
        )
        .expect("store");
        let (store, refresh) =
            activate_provider(&root, "deepseek", "http://127.0.0.1:3721/v1", codex.path())
                .expect("activate");
        assert_eq!(store.active_id, "deepseek");
        assert_eq!(refresh, LiveRefresh::NewConversation);
    }
}
