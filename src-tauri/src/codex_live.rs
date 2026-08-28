use std::fs;
use std::path::{Path, PathBuf};

use anyhow::Context;
use serde_json::Value;
use toml_edit::{value, DocumentMut, Item, Table};

pub(crate) fn set_secret_file_permissions(path: &Path) -> anyhow::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))
            .with_context(|| format!("Failed to set permissions on {}", path.display()))?;
    }
    let _ = path;
    Ok(())
}

pub fn default_codex_home() -> PathBuf {
    std::env::var_os("CODEX_HOME")
        .map(PathBuf::from)
        .or_else(|| dirs::home_dir().map(|home| home.join(".codex")))
        .unwrap_or_else(|| PathBuf::from(".codex"))
}

pub fn config_path(codex_home: &Path) -> PathBuf {
    codex_home.join("config.toml")
}

pub fn auth_path(codex_home: &Path) -> PathBuf {
    codex_home.join("auth.json")
}

pub fn auth_has_oauth_login(auth: &Value) -> bool {
    let tokens = auth.get("tokens").unwrap_or(auth);
    ["access_token", "refresh_token", "id_token"]
        .into_iter()
        .any(|key| {
            tokens
                .get(key)
                .and_then(Value::as_str)
                .is_some_and(|token| !token.trim().is_empty())
        })
}

pub fn read_auth(codex_home: &Path) -> anyhow::Result<Option<Value>> {
    let path = auth_path(codex_home);
    if !path.exists() {
        return Ok(None);
    }
    let contents =
        fs::read_to_string(&path).with_context(|| format!("Failed to read {}", path.display()))?;
    let value: Value = serde_json::from_str(&contents)
        .with_context(|| format!("Failed to parse {}", path.display()))?;
    Ok(Some(value))
}

pub fn read_config_document(codex_home: &Path) -> anyhow::Result<DocumentMut> {
    let path = config_path(codex_home);
    if !path.exists() {
        return Ok(DocumentMut::new());
    }
    let contents =
        fs::read_to_string(&path).with_context(|| format!("Failed to read {}", path.display()))?;
    contents
        .parse::<DocumentMut>()
        .with_context(|| format!("Failed to parse {}", path.display()))
}

pub fn write_config_atomic(codex_home: &Path, document: &DocumentMut) -> anyhow::Result<()> {
    fs::create_dir_all(codex_home)
        .with_context(|| format!("Failed to create {}", codex_home.display()))?;
    let path = config_path(codex_home);
    let temp = path.with_extension("toml.helper-tmp");
    fs::write(&temp, document.to_string())
        .with_context(|| format!("Failed to write {}", temp.display()))?;
    set_secret_file_permissions(&temp)?;
    fs::rename(&temp, &path).with_context(|| format!("Failed to replace {}", path.display()))?;
    set_secret_file_permissions(&path)?;
    Ok(())
}

pub struct LiveProviderWrite<'a> {
    pub id: &'a str,
    pub name: &'a str,
    pub model: &'a str,
    pub base_url: &'a str,
    pub wire_api: &'a str,
    pub api_key: Option<&'a str>,
    pub preserve_openai_login: bool,
    pub previous_id: Option<&'a str>,
}

pub const UNIFIED_SESSION_PROVIDER_ID: &str = "custom";
/// Placeholder for Helper's local proxy. Codex 0.149 short-circuits request
/// auth on `experimental_bearer_token` so ChatGPT tokens are not sent to a
/// third-party route. The proxy replaces this with the real provider key.
pub const HELPER_MANAGED_BEARER: &str = "HELPER_MANAGED";
const HELPER_OWNED_PROVIDER_IDS: &[&str] = &["helper", "deepseek", "grok"];

fn ensure_file_auth_store(document: &mut DocumentMut) {
    if document.get("cli_auth_credentials_store").is_none() {
        document["cli_auth_credentials_store"] = value("file");
    }
}

fn ensure_model_providers_table(document: &mut DocumentMut) -> &mut Table {
    if !document.contains_key("model_providers") {
        document["model_providers"] = Item::Table(Table::new());
    }
    document["model_providers"]
        .as_table_mut()
        .expect("model_providers table")
}

fn restore_unified_official_custom_table(providers: &mut Table) {
    let table = providers
        .entry(UNIFIED_SESSION_PROVIDER_ID)
        .or_insert(Item::Table(Table::new()));
    let table = table.as_table_mut().expect("custom provider table");
    table["name"] = value("OpenAI");
    table["requires_openai_auth"] = value(true);
    table["supports_websockets"] = value(true);
    table["wire_api"] = value("responses");
    table.remove("base_url");
    table.remove("experimental_bearer_token");
    table.remove("env_key");
}

fn remove_stale_provider_table(providers: &mut Table, provider_id: Option<&str>) {
    let Some(provider_id) = provider_id.map(str::trim).filter(|id| !id.is_empty()) else {
        return;
    };
    if provider_id == UNIFIED_SESSION_PROVIDER_ID || provider_id == "official" {
        return;
    }
    providers.remove(provider_id);
}

fn prune_helper_owned_provider_tables(providers: &mut Table) {
    for provider_id in HELPER_OWNED_PROVIDER_IDS {
        providers.remove(*provider_id);
    }
}

pub fn apply_official_provider(
    document: &mut DocumentMut,
    previous_id: Option<&str>,
    previous_model: Option<&str>,
) {
    ensure_file_auth_store(document);
    document["model_provider"] = value(UNIFIED_SESSION_PROVIDER_ID);
    if let Some(previous_model) = previous_model
        .map(str::trim)
        .filter(|model| !model.is_empty())
    {
        if document
            .get("model")
            .and_then(Item::as_value)
            .and_then(|value| value.as_str())
            == Some(previous_model)
        {
            document.remove("model");
        }
    }
    let providers = ensure_model_providers_table(document);
    prune_helper_owned_provider_tables(providers);
    remove_stale_provider_table(providers, previous_id);
    restore_unified_official_custom_table(providers);
}

pub fn apply_api_provider(document: &mut DocumentMut, write: LiveProviderWrite<'_>) {
    ensure_file_auth_store(document);
    document["model_provider"] = value(UNIFIED_SESSION_PROVIDER_ID);
    if !write.model.trim().is_empty() {
        document["model"] = value(write.model);
    }
    let providers = ensure_model_providers_table(document);
    prune_helper_owned_provider_tables(providers);
    remove_stale_provider_table(providers, write.previous_id);
    if write.id != UNIFIED_SESSION_PROVIDER_ID {
        remove_stale_provider_table(providers, Some(write.id));
    }
    let table = providers
        .entry(UNIFIED_SESSION_PROVIDER_ID)
        .or_insert(Item::Table(Table::new()));
    let table = table.as_table_mut().expect("custom provider table");
    table["name"] = value(write.name);
    table["base_url"] = value(write.base_url);
    table["wire_api"] = value(write.wire_api);
    table["requires_openai_auth"] = value(write.preserve_openai_login);
    table["supports_websockets"] = value(false);
    table.remove("env_key");
    // CC Switch 0.149 contract: request auth short-circuits on
    // experimental_bearer_token, while requires_openai_auth drives the
    // Desktop ChatGPT login UX. A real provider key here makes Desktop treat
    // the session as API-key login and show the sign-in wall; a placeholder
    // keeps ChatGPT login visible and lets Helper's proxy inject the real key.
    if write.preserve_openai_login {
        table["experimental_bearer_token"] = value(HELPER_MANAGED_BEARER);
    } else if let Some(api_key) = write.api_key.filter(|key| !key.trim().is_empty()) {
        table["experimental_bearer_token"] = value(api_key);
    } else {
        table.remove("experimental_bearer_token");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn apply_api_provider_preserves_unrelated_tables() {
        let mut document: DocumentMut = r#"
sandbox_mode = "danger-full-access"

[projects."/tmp/demo"]
trust_level = "trusted"
"#
        .parse()
        .expect("toml");
        apply_api_provider(
            &mut document,
            LiveProviderWrite {
                id: "grok",
                name: "Grok",
                model: "grok-4.6",
                base_url: "http://127.0.0.1:3721/v1",
                wire_api: "responses",
                api_key: Some("sk-test"),
                preserve_openai_login: true,
                previous_id: Some("grok-old"),
            },
        );
        let rendered = document.to_string();
        assert!(rendered.contains("model_provider = \"custom\""));
        assert!(rendered.contains("[model_providers.custom]"));
        assert!(!rendered.contains("[model_providers.grok]"));
        assert!(rendered.contains("requires_openai_auth = true"));
        assert!(rendered.contains(&format!(
            "experimental_bearer_token = \"{HELPER_MANAGED_BEARER}\""
        )));
        assert!(!rendered.contains("sk-test"));
        assert!(rendered.contains("cli_auth_credentials_store = \"file\""));
        assert!(rendered.contains("trust_level = \"trusted\""));
        assert!(rendered.contains("sandbox_mode = \"danger-full-access\""));
    }

    #[test]
    fn apply_official_provider_restores_custom_resume_bucket() {
        let mut document: DocumentMut = r#"
model_provider = "grok"
model = "grok-4.6"

[model_providers.grok]
name = "Grok"
base_url = "https://api.x.ai/v1"

[model_providers.custom]
name = "xai"
base_url = "http://127.0.0.1:15721/v1"
experimental_bearer_token = "PROXY_MANAGED"
"#
        .parse()
        .expect("toml");
        apply_official_provider(&mut document, Some("grok"), Some("grok-4.6"));
        let rendered = document.to_string();
        assert!(rendered.contains("model_provider = \"custom\""));
        assert!(!rendered.contains("model = \"grok-4.6\""));
        assert!(!rendered.contains("[model_providers.grok]"));
        assert!(rendered.contains("[model_providers.custom]"));
        assert!(rendered.contains("name = \"OpenAI\""));
        assert!(rendered.contains("supports_websockets = true"));
        assert!(!rendered.contains("PROXY_MANAGED"));
        assert!(!rendered.contains("127.0.0.1:15721"));
    }

    #[test]
    fn auth_detects_oauth_tokens() {
        let auth = serde_json::json!({
            "tokens": { "access_token": "tok", "refresh_token": "ref" }
        });
        assert!(auth_has_oauth_login(&auth));
        let key_only = serde_json::json!({ "OPENAI_API_KEY": "sk-test" });
        assert!(!auth_has_oauth_login(&key_only));
    }

    #[test]
    fn apply_api_provider_removes_previous_provider_table() {
        let mut document: DocumentMut = r#"
model_provider = "grok"

[model_providers.grok]
name = "Grok"
experimental_bearer_token = "sk-grok"
"#
        .parse()
        .expect("toml");
        apply_api_provider(
            &mut document,
            LiveProviderWrite {
                id: "deepseek",
                name: "DeepSeek",
                model: "deepseek-chat",
                base_url: "https://api.deepseek.com/v1",
                wire_api: "responses",
                api_key: Some("sk-ds"),
                preserve_openai_login: true,
                previous_id: Some("grok"),
            },
        );
        let rendered = document.to_string();
        assert!(rendered.contains("model_provider = \"custom\""));
        assert!(rendered.contains("[model_providers.custom]"));
        assert!(!rendered.contains("[model_providers.grok]"));
        assert!(!rendered.contains("[model_providers.deepseek]"));
        assert!(!rendered.contains("sk-grok"));
        assert!(rendered.contains("name = \"DeepSeek\""));
        assert!(rendered.contains(&format!(
            "experimental_bearer_token = \"{HELPER_MANAGED_BEARER}\""
        )));
        assert!(!rendered.contains("sk-ds"));
    }

    #[test]
    fn apply_api_provider_writes_bearer_without_chatgpt_login() {
        let mut document = DocumentMut::new();
        apply_api_provider(
            &mut document,
            LiveProviderWrite {
                id: "deepseek",
                name: "DeepSeek",
                model: "deepseek-chat",
                base_url: "http://127.0.0.1:3721/v1",
                wire_api: "responses",
                api_key: Some("sk-ds"),
                preserve_openai_login: false,
                previous_id: None,
            },
        );
        let rendered = document.to_string();
        assert!(rendered.contains("requires_openai_auth = false"));
        assert!(rendered.contains("experimental_bearer_token = \"sk-ds\""));
    }
}
