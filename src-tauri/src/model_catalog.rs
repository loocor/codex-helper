use std::collections::HashSet;
use std::path::Path;

use anyhow::Context;
use serde_json::{json, Value};
use toml_edit::{value, DocumentMut};

use crate::codex_live::set_secret_file_permissions;
use crate::providers::{CatalogModel, Provider};

pub const HELPER_CATALOG_FILENAME: &str = "codex-helper-model-catalog.json";

pub fn catalog_path(codex_home: &Path) -> std::path::PathBuf {
    codex_home.join(HELPER_CATALOG_FILENAME)
}

pub fn clear_helper_catalog(codex_home: &Path, document: &mut DocumentMut) -> anyhow::Result<()> {
    if document
        .get("model_catalog_json")
        .and_then(|item| item.as_str())
        .is_some_and(is_helper_catalog_pointer)
    {
        document.remove("model_catalog_json");
    }
    let path = catalog_path(codex_home);
    if path.exists() {
        std::fs::remove_file(&path)
            .with_context(|| format!("Failed to remove {}", path.display()))?;
    }
    Ok(())
}

pub fn apply_provider_catalog(
    codex_home: &Path,
    document: &mut DocumentMut,
    provider: &Provider,
) -> anyhow::Result<()> {
    let catalog = build_provider_catalog(provider)?;
    let path = catalog_path(codex_home);
    let contents = format!("{}\n", serde_json::to_string_pretty(&catalog)?);
    std::fs::write(&path, contents)
        .with_context(|| format!("Failed to write {}", path.display()))?;
    set_secret_file_permissions(&path)?;
    document["model_catalog_json"] = value(HELPER_CATALOG_FILENAME);
    Ok(())
}

fn is_helper_catalog_pointer(path: &str) -> bool {
    std::path::Path::new(path)
        .file_name()
        .and_then(|name| name.to_str())
        == Some(HELPER_CATALOG_FILENAME)
}

fn build_provider_catalog(provider: &Provider) -> anyhow::Result<Value> {
    let mut models = Vec::new();
    let mut seen = HashSet::new();
    if !provider.catalog_models.is_empty() {
        for (index, spec) in provider.catalog_models.iter().enumerate() {
            let slug = spec.model.trim();
            if slug.is_empty() || !seen.insert(slug.to_string()) {
                continue;
            }
            models.push(native_catalog_entry(slug, index, Some(spec), false));
        }
    } else {
        let chat_safe = provider.wire_api.trim().eq_ignore_ascii_case("chat");
        for slug in catalog_slugs(provider) {
            if !seen.insert(slug.clone()) {
                continue;
            }
            models.push(native_catalog_entry(&slug, models.len(), None, chat_safe));
        }
    }
    if models.is_empty() {
        anyhow::bail!("Provider model is required to build a Codex catalog");
    }
    Ok(json!({ "models": models }))
}

fn catalog_slugs(provider: &Provider) -> Vec<String> {
    let mut slugs = Vec::new();
    let mut seen = HashSet::new();
    for slug in
        std::iter::once(provider.model.as_str()).chain(provider.models.iter().map(String::as_str))
    {
        let slug = slug.trim();
        if slug.is_empty() || !seen.insert(slug.to_string()) {
            continue;
        }
        slugs.push(slug.to_string());
    }
    slugs
}

const REASONING_LEVEL_DESCRIPTIONS: &[(&str, &str)] = &[
    ("none", "Disable Thinking"),
    ("minimal", "Minimal reasoning"),
    ("low", "Fast responses with lighter reasoning"),
    (
        "medium",
        "Balances speed and reasoning depth for everyday tasks",
    ),
    ("high", "Greater reasoning depth for complex problems"),
    ("xhigh", "Extra high reasoning depth for complex problems"),
    ("max", "Maximum reasoning depth for the hardest problems"),
    ("ultra", "Ultra reasoning depth"),
];

fn native_catalog_entry(
    slug: &str,
    priority: usize,
    spec: Option<&CatalogModel>,
    chat_safe: bool,
) -> Value {
    let mut entry: Value = serde_json::from_str(include_str!(
        "../resources/codex_native_responses_template.json"
    ))
    .expect("bundled native responses template must be valid JSON");
    let display = spec
        .map(|entry| entry.display_name.trim())
        .filter(|name| !name.is_empty())
        .unwrap_or(slug)
        .to_string();
    if let Some(object) = entry.as_object_mut() {
        object.insert("slug".to_string(), json!(slug));
        object.insert("display_name".to_string(), json!(display));
        object.insert("description".to_string(), json!(display));
        object.insert("priority".to_string(), json!(1000 + priority));
        if let Some(window) = spec
            .and_then(|entry| entry.context_window)
            .filter(|value| *value > 0)
        {
            object.insert("context_window".to_string(), json!(window));
            object.insert("max_context_window".to_string(), json!(window));
        }
        if let Some(spec) = spec {
            apply_reasoning_levels(object, spec);
        } else if chat_safe {
            clear_reasoning_levels(object);
        }
    }
    entry
}

fn clear_reasoning_levels(entry: &mut serde_json::Map<String, Value>) {
    entry.insert(
        "supported_reasoning_levels".to_string(),
        json!([{ "effort": "none", "description": "Disable Thinking" }]),
    );
    entry.insert("default_reasoning_level".to_string(), json!("none"));
}

fn apply_reasoning_levels(entry: &mut serde_json::Map<String, Value>, spec: &CatalogModel) {
    if spec.reasoning_levels.is_empty() {
        clear_reasoning_levels(entry);
        return;
    }
    let supported: Vec<Value> = REASONING_LEVEL_DESCRIPTIONS
        .iter()
        .filter(|(effort, _)| {
            spec.reasoning_levels
                .iter()
                .any(|level| level.eq_ignore_ascii_case(effort))
        })
        .map(|(effort, description)| json!({ "effort": *effort, "description": *description }))
        .collect();
    if supported.is_empty() {
        clear_reasoning_levels(entry);
        return;
    }
    let default = spec.default_reasoning_level.trim().to_ascii_lowercase();
    let default = supported
        .iter()
        .find_map(|entry| {
            entry
                .get("effort")
                .and_then(Value::as_str)
                .filter(|effort| effort.eq_ignore_ascii_case(&default))
        })
        .or_else(|| {
            entry
                .get("default_reasoning_level")
                .and_then(Value::as_str)
                .filter(|effort| {
                    supported
                        .iter()
                        .any(|item| item.get("effort").and_then(Value::as_str) == Some(*effort))
                })
        })
        .or_else(|| {
            supported
                .last()
                .and_then(|item| item.get("effort").and_then(Value::as_str))
        })
        .unwrap_or("high")
        .to_string();
    entry.insert("supported_reasoning_levels".to_string(), json!(supported));
    entry.insert("default_reasoning_level".to_string(), json!(default));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::{Provider, ProviderKind};

    #[test]
    fn deepseek_catalog_uses_native_template_for_configured_models_only() {
        let provider = Provider {
            id: "deepseek".to_string(),
            name: "Deepseek".to_string(),
            kind: ProviderKind::ApiKey,
            model: "deepseek-chat".to_string(),
            base_url: "https://api.deepseek.com".to_string(),
            wire_api: "chat".to_string(),
            models: vec!["deepseek-v4-flash".to_string()],
            ..Provider::default()
        };
        let catalog = build_provider_catalog(&provider).expect("catalog");
        let models = catalog["models"].as_array().expect("models");
        let slugs: Vec<&str> = models
            .iter()
            .filter_map(|entry| entry.get("slug").and_then(Value::as_str))
            .collect();
        assert_eq!(slugs, vec!["deepseek-chat", "deepseek-v4-flash"]);
        assert!(!slugs.contains(&"deepseek-v4-pro"));
        let instructions = models[0]
            .get("base_instructions")
            .and_then(Value::as_str)
            .expect("base_instructions");
        assert_eq!(
            instructions,
            "You are Codex, a coding agent. You and the user share the same workspace and collaborate to achieve the user's goals."
        );
        assert!(models[0].get("apply_patch_tool_type").is_none());
        assert!(models[0].get("model_messages").is_none());
        assert_eq!(models[0]["default_reasoning_level"], "none");
        let efforts: Vec<&str> = models[0]["supported_reasoning_levels"]
            .as_array()
            .expect("levels")
            .iter()
            .filter_map(|item| item.get("effort").and_then(Value::as_str))
            .collect();
        assert_eq!(efforts, vec!["none"]);
    }

    #[test]
    fn empty_catalog_reasoning_does_not_inherit_template_thinking() {
        let provider = Provider {
            id: "kimi".to_string(),
            name: "Kimi".to_string(),
            kind: ProviderKind::ApiKey,
            model: "kimi-k2.5".to_string(),
            base_url: "https://api.moonshot.cn/v1".to_string(),
            wire_api: "chat".to_string(),
            catalog_models: vec![crate::providers::CatalogModel {
                display_name: "Kimi K2.5".to_string(),
                model: "kimi-k2.5".to_string(),
                context_window: None,
                reasoning_levels: Vec::new(),
                default_reasoning_level: String::new(),
            }],
            ..Provider::default()
        };
        let catalog = build_provider_catalog(&provider).expect("catalog");
        let entry = &catalog["models"][0];
        assert_eq!(entry["default_reasoning_level"], "none");
        let efforts: Vec<&str> = entry["supported_reasoning_levels"]
            .as_array()
            .expect("levels")
            .iter()
            .filter_map(|item| item.get("effort").and_then(Value::as_str))
            .collect();
        assert_eq!(efforts, vec!["none"]);
    }

    #[test]
    fn catalog_models_are_written_exactly_as_listed() {
        let provider = Provider {
            id: "grok".to_string(),
            name: "Grok".to_string(),
            kind: ProviderKind::ApiKey,
            model: "grok-4.6".to_string(),
            base_url: "https://api.x.ai/v1".to_string(),
            catalog_models: vec![crate::providers::CatalogModel {
                display_name: "Grok 4.6".to_string(),
                model: "grok-4.6".to_string(),
                context_window: Some(500_000),
                reasoning_levels: vec![
                    "low".to_string(),
                    "medium".to_string(),
                    "high".to_string(),
                    "xhigh".to_string(),
                ],
                default_reasoning_level: "high".to_string(),
            }],
            ..Provider::default()
        };
        let catalog = build_provider_catalog(&provider).expect("catalog");
        let entry = &catalog["models"][0];
        assert_eq!(entry["slug"], "grok-4.6");
        assert_eq!(entry["display_name"], "Grok 4.6");
        assert_eq!(entry["context_window"], 500_000);
        let efforts: Vec<&str> = entry["supported_reasoning_levels"]
            .as_array()
            .expect("levels")
            .iter()
            .filter_map(|item| item.get("effort").and_then(Value::as_str))
            .collect();
        assert_eq!(efforts, vec!["low", "medium", "high", "xhigh"]);
        assert_eq!(entry["default_reasoning_level"], "high");
        assert_eq!(catalog["models"].as_array().map(Vec::len), Some(1));
    }
}
