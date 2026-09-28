//! Local inbound API keys for the Helper provider proxy.

use std::fs;
use std::path::Path;

use anyhow::Context;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::codex_live::write_secret_file_atomic;
use crate::provider_oauth::HELPER_OAUTH_LIVE_TOKEN;
use crate::providers::{provider_is_official, selected_catalog_models, Provider, ProviderStore};

const KEY_ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", default)]
pub struct EndpointKey {
    pub id: String,
    pub name: String,
    pub secret: String,
    pub created_at: String,
}

impl Default for EndpointKey {
    fn default() -> Self {
        Self {
            id: String::new(),
            name: String::new(),
            secret: String::new(),
            created_at: String::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct EndpointStore {
    pub keys: Vec<EndpointKey>,
}

pub fn endpoint_path(state_root: &Path) -> std::path::PathBuf {
    state_root.join("endpoint.json")
}

pub fn read_store(state_root: &Path) -> anyhow::Result<EndpointStore> {
    let path = endpoint_path(state_root);
    if !path.exists() {
        return Ok(EndpointStore::default());
    }
    let contents =
        fs::read_to_string(&path).with_context(|| format!("Failed to read {}", path.display()))?;
    serde_json::from_str(&contents).with_context(|| format!("Failed to parse {}", path.display()))
}

pub fn write_store(state_root: &Path, store: &EndpointStore) -> anyhow::Result<()> {
    let path = endpoint_path(state_root);
    let contents = format!("{}\n", serde_json::to_string_pretty(store)?);
    write_secret_file_atomic(&path, contents)
}

/// Models listed here are the whole selected provider mix, not just the
/// active provider: the proxy routes each request by model across every
/// selected provider, so Endpoint clients must see the same set.
pub fn list_response(store: &EndpointStore, base_url: &str, providers: &ProviderStore) -> Value {
    let models: Vec<String> = selected_catalog_models(providers)
        .into_iter()
        .map(|(slug, _)| slug)
        .collect();
    let active = providers
        .providers
        .iter()
        .find(|provider| provider.id == providers.active_id);
    let official = models.is_empty() && active.is_some_and(provider_is_official);
    json!({
        "status": "ok",
        "baseUrl": base_url,
        "keys": store.keys,
        "officialActive": official,
        "models": models,
    })
}

pub fn create_key(state_root: &Path, payload: &Value) -> anyhow::Result<EndpointStore> {
    let name = payload
        .get("name")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("Untitled")
        .to_string();
    let length = match payload.get("length") {
        None => 32usize,
        Some(value) => value
            .as_u64()
            .filter(|length| *length == 32)
            .map(|length| length as usize)
            .ok_or_else(|| anyhow::anyhow!("API key length must be 32"))?,
    };
    let mut store = read_store(state_root)?;
    store.keys.push(EndpointKey {
        id: random_id()?,
        name,
        secret: generate_secret(length)?,
        created_at: chrono::Utc::now().to_rfc3339(),
    });
    write_store(state_root, &store)?;
    Ok(store)
}

pub fn delete_key(state_root: &Path, payload: &Value) -> anyhow::Result<EndpointStore> {
    let id = payload
        .get("id")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow::anyhow!("API key id is required"))?;
    let mut store = read_store(state_root)?;
    let before = store.keys.len();
    store.keys.retain(|key| key.id != id);
    if store.keys.len() == before {
        anyhow::bail!("API key not found: {id}");
    }
    write_store(state_root, &store)?;
    Ok(store)
}

pub fn authorize_bearer(
    store: &EndpointStore,
    bearer: Option<&str>,
    provider: &Provider,
) -> Result<(), String> {
    if store.keys.is_empty() {
        return Ok(());
    }
    let Some(token) = bearer.map(str::trim).filter(|value| !value.is_empty()) else {
        return Err("Endpoint API key is required".to_string());
    };
    if store.keys.iter().any(|key| key.secret == token) {
        return Ok(());
    }
    if token == HELPER_OAUTH_LIVE_TOKEN {
        return Ok(());
    }
    if !provider.api_key.trim().is_empty() && token == provider.api_key {
        return Ok(());
    }
    Err("Invalid Endpoint API key".to_string())
}

fn generate_secret(length: usize) -> anyhow::Result<String> {
    let bytes = random_bytes(length)?;
    Ok(bytes
        .into_iter()
        .map(|byte| KEY_ALPHABET[byte as usize % KEY_ALPHABET.len()] as char)
        .collect())
}

fn random_id() -> anyhow::Result<String> {
    let bytes = random_bytes(6)?;
    Ok(bytes.iter().map(|byte| format!("{byte:02x}")).collect())
}

fn random_bytes(length: usize) -> anyhow::Result<Vec<u8>> {
    let mut bytes = vec![0u8; length];
    getrandom::fill(&mut bytes).context("Failed to read random bytes")?;
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[cfg(unix)]
    #[test]
    fn write_store_sets_secret_file_permissions() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempdir().expect("temp");
        let store = EndpointStore {
            keys: vec![EndpointKey {
                id: "1".to_string(),
                name: "Local".to_string(),
                secret: "abc".to_string(),
                created_at: "now".to_string(),
            }],
        };
        write_store(dir.path(), &store).unwrap();
        let mode = fs::metadata(endpoint_path(dir.path()))
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o600);
    }

    #[test]
    fn create_key_uses_32_characters() {
        let temp = tempdir().expect("temp");
        let store =
            create_key(temp.path(), &json!({ "name": "Coser", "length": 32 })).expect("create");
        assert_eq!(store.keys.len(), 1);
        assert_eq!(store.keys[0].name, "Coser");
        assert_eq!(store.keys[0].secret.len(), 32);
        assert!(store.keys[0]
            .secret
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric()));
    }

    #[test]
    fn create_key_rejects_16_character_length() {
        let temp = tempdir().expect("temp");
        let error = create_key(temp.path(), &json!({ "length": 16 })).expect_err("16-char keys");
        assert!(error.to_string().contains("API key length must be 32"));
    }

    #[test]
    fn create_key_defaults_to_32() {
        let temp = tempdir().expect("temp");
        let store = create_key(temp.path(), &json!({})).expect("create");
        assert_eq!(store.keys[0].secret.len(), 32);
        assert_eq!(store.keys[0].name, "Untitled");
    }

    #[test]
    fn authorize_allows_any_configured_key() {
        let store = EndpointStore {
            keys: vec![EndpointKey {
                id: "one".to_string(),
                name: "Coser".to_string(),
                secret: "abc".to_string(),
                created_at: String::new(),
            }],
        };
        let provider = Provider::default();
        assert!(authorize_bearer(&store, Some("abc"), &provider).is_ok());
        assert!(authorize_bearer(&store, Some("nope"), &provider).is_err());
        assert!(authorize_bearer(&store, None, &provider).is_err());
    }

    #[test]
    fn authorize_is_open_when_no_keys() {
        let store = EndpointStore::default();
        let provider = Provider::default();
        assert!(authorize_bearer(&store, None, &provider).is_ok());
    }

    #[test]
    fn list_response_keeps_empty_base_url() {
        let response = list_response(&EndpointStore::default(), "", &ProviderStore::default());
        assert_eq!(response["status"], "ok");
        assert_eq!(response["baseUrl"], "");
    }

    #[test]
    fn list_response_marks_official_selection() {
        let response = list_response(&EndpointStore::default(), "", &ProviderStore::default());
        assert_eq!(response["officialActive"], json!(true));
        assert_eq!(response["models"], json!([]));
    }

    #[test]
    fn list_response_includes_models_of_every_selected_provider() {
        let store = EndpointStore::default();
        let providers = ProviderStore {
            active_id: "mimo".to_string(),
            selected_ids: vec!["grok".to_string(), "mimo".to_string()],
            providers: vec![
                Provider {
                    id: "grok".to_string(),
                    model: "grok-4.6".to_string(),
                    models: vec!["grok-4.5".to_string()],
                    ..Provider::default()
                },
                Provider {
                    id: "mimo".to_string(),
                    model: "mimo-v2.6-pro".to_string(),
                    models: vec!["mimo-v2.6-flash".to_string()],
                    ..Provider::default()
                },
                Provider {
                    id: "kimi".to_string(),
                    model: "kimi-k3".to_string(),
                    ..Provider::default()
                },
            ],
        };
        let response = list_response(&store, "http://127.0.0.1:3721/v1", &providers);
        assert_eq!(response["status"], "ok");
        assert_eq!(response["officialActive"], json!(false));
        assert_eq!(
            response["models"],
            json!(["grok-4.6", "grok-4.5", "mimo-v2.6-pro", "mimo-v2.6-flash"])
        );
    }

    #[test]
    fn list_response_namespaces_models_shared_by_selected_providers() {
        let store = EndpointStore::default();
        let providers = ProviderStore {
            active_id: "grok".to_string(),
            selected_ids: vec!["grok".to_string(), "mimo".to_string()],
            providers: vec![
                Provider {
                    id: "grok".to_string(),
                    model: "shared-model".to_string(),
                    ..Provider::default()
                },
                Provider {
                    id: "mimo".to_string(),
                    model: "shared-model".to_string(),
                    ..Provider::default()
                },
            ],
        };
        let response = list_response(&store, "http://127.0.0.1:3721/v1", &providers);
        assert_eq!(
            response["models"],
            json!(["grok::shared-model", "mimo::shared-model"])
        );
    }

    #[test]
    fn authorize_accepts_codex_live_token() {
        let store = EndpointStore {
            keys: vec![EndpointKey {
                id: "one".to_string(),
                name: "Coser".to_string(),
                secret: "abc".to_string(),
                created_at: String::new(),
            }],
        };
        let mut provider = Provider::default();
        provider.api_key = "sk-upstream".to_string();
        assert!(authorize_bearer(&store, Some(HELPER_OAUTH_LIVE_TOKEN), &provider).is_ok());
        assert!(authorize_bearer(&store, Some("sk-upstream"), &provider).is_ok());
    }
}
