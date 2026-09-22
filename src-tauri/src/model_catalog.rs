use std::collections::HashSet;
use std::path::Path;

use anyhow::Context;
use serde_json::{json, Value};
use toml_edit::{value, DocumentMut};

use crate::codex_live::set_secret_file_permissions;
use crate::providers::{
    catalog_model_slug, provider_effort_aliases, provider_is_mimo, CatalogModel, Provider,
};

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

pub fn apply_mixed_catalog(
    codex_home: &Path,
    document: &mut DocumentMut,
    providers: &[&Provider],
) -> anyhow::Result<()> {
    let catalog = build_mixed_catalog(providers, code_mode_host_available())?;
    let path = catalog_path(codex_home);
    let contents = format!("{}\n", serde_json::to_string_pretty(&catalog)?);
    std::fs::write(&path, contents)
        .with_context(|| format!("Failed to write {}", path.display()))?;
    set_secret_file_permissions(&path)?;
    document["model_catalog_json"] = value(HELPER_CATALOG_FILENAME);
    Ok(())
}

/// Codex only honors `tool_mode = code_mode_only` when this host binary exists.
/// Missing host fails closed and hides the model's tools.
pub fn code_mode_host_available() -> bool {
    [
        "/Applications/ChatGPT.app/Contents/Resources/codex-code-mode-host",
        "/Applications/Codex.app/Contents/Resources/codex-code-mode-host",
    ]
    .iter()
    .any(|path| Path::new(path).is_file())
}

fn is_helper_catalog_pointer(path: &str) -> bool {
    std::path::Path::new(path)
        .file_name()
        .and_then(|name| name.to_str())
        == Some(HELPER_CATALOG_FILENAME)
}

#[cfg(test)]
fn build_provider_catalog(provider: &Provider) -> anyhow::Result<Value> {
    build_mixed_catalog(&[provider], code_mode_host_available())
}

fn build_mixed_catalog(providers: &[&Provider], code_mode_host: bool) -> anyhow::Result<Value> {
    CatalogBuilder::new(providers, code_mode_host).build()
}

/// Accumulates one deduplicated catalog entry per selected provider model.
/// `providers` is the full selected mix, needed to detect slug collisions.
struct CatalogBuilder<'a> {
    providers: &'a [&'a Provider],
    code_mode_host: bool,
    models: Vec<Value>,
    seen: HashSet<String>,
}

impl<'a> CatalogBuilder<'a> {
    fn new(providers: &'a [&'a Provider], code_mode_host: bool) -> Self {
        Self {
            providers,
            code_mode_host,
            models: Vec::new(),
            seen: HashSet::new(),
        }
    }

    fn build(mut self) -> anyhow::Result<Value> {
        for provider in self.providers {
            self.append_provider(provider);
        }
        if self.models.is_empty() {
            anyhow::bail!("Provider model is required to build a Codex catalog");
        }
        Ok(json!({ "models": self.models }))
    }

    fn append_provider(&mut self, provider: &Provider) {
        let effort_aliases = provider_effort_aliases(provider);
        let prefix = provider
            .prefix_model_names
            .then_some(provider.name.trim())
            .filter(|name| !name.is_empty());
        if !provider.catalog_models.is_empty() {
            for spec in &provider.catalog_models {
                let slug = spec.model.trim();
                if slug.is_empty() {
                    continue;
                }
                let entry = native_catalog_entry(
                    slug,
                    self.models.len(),
                    Some(spec),
                    false,
                    effort_aliases,
                );
                self.push_model(provider, slug, prefix, entry, Some(spec));
            }
            return;
        }
        let chat_safe = provider.wire_api.trim().eq_ignore_ascii_case("chat");
        for slug in catalog_slugs(provider) {
            let entry =
                native_catalog_entry(&slug, self.models.len(), None, chat_safe, effort_aliases);
            self.push_model(provider, &slug, prefix, entry, None);
        }
    }

    /// Namespaces colliding slugs, optionally prefixes the display name, and
    /// skips entries that duplicate an already-added catalog slug.
    fn push_model(
        &mut self,
        provider: &Provider,
        upstream_slug: &str,
        prefix: Option<&str>,
        entry: Value,
        spec: Option<&CatalogModel>,
    ) {
        let catalog_slug = catalog_model_slug(self.providers, &provider.id, upstream_slug);
        if !self.seen.insert(catalog_slug.to_ascii_lowercase()) {
            return;
        }
        let mut entry =
            finish_catalog_entry(provider, entry, upstream_slug, spec, self.code_mode_host);
        if let Some(object) = entry.as_object_mut() {
            object.insert("slug".to_string(), json!(catalog_slug));
            if let Some(name) = prefix {
                let display = object
                    .get("display_name")
                    .and_then(Value::as_str)
                    .unwrap_or(upstream_slug);
                let label = format!("{name} / ");
                if !display
                    .to_ascii_lowercase()
                    .starts_with(&label.to_ascii_lowercase())
                {
                    let prefixed = format!("{label}{display}");
                    object.insert("display_name".to_string(), json!(prefixed));
                    object.insert("description".to_string(), json!(prefixed));
                }
            }
        }
        self.models.push(entry);
    }
}

fn finish_catalog_entry(
    provider: &Provider,
    mut entry: Value,
    slug: &str,
    spec: Option<&CatalogModel>,
    code_mode_host: bool,
) -> Value {
    if !provider_is_mimo(provider) {
        return entry;
    }
    if let Some(object) = entry.as_object_mut() {
        apply_mimo_catalog_metadata(object, slug, spec, code_mode_host);
    }
    entry
}

/// Wire metadata from Xiaomi's Codex catalog. v2.6 custom tools are rejected
/// unless Codex sends Responses Lite with a freeform apply_patch tool.
/// `code_mode_only` is set only when the code-mode host exists. Official entries
/// also set `multi_agent_version = v2`; Helper leaves that unset because spawned
/// subagent sessions currently drop the task text.
fn apply_mimo_catalog_metadata(
    entry: &mut serde_json::Map<String, Value>,
    slug: &str,
    spec: Option<&CatalogModel>,
    code_mode_host: bool,
) {
    let slug_key = slug.trim().to_ascii_lowercase();
    let v25 = slug_key.contains("v2.5");
    if v25 {
        entry.insert("use_responses_lite".to_string(), json!(false));
    } else {
        entry.insert("use_responses_lite".to_string(), json!(true));
        entry.insert("apply_patch_tool_type".to_string(), json!("freeform"));
        if code_mode_host && slug_key.contains("v2.6") {
            entry.insert("tool_mode".to_string(), json!("code_mode_only"));
        }
    }
    entry.insert("shell_type".to_string(), json!("unified_exec"));
    entry.insert("supports_reasoning_summaries".to_string(), json!(true));
    entry.insert("default_reasoning_summary".to_string(), json!("none"));
    entry.insert("supports_parallel_tool_calls".to_string(), json!(false));
    entry.insert("supports_image_detail_original".to_string(), json!(true));
    entry.insert("supports_experimental_context".to_string(), json!(true));
    entry.insert("supports_search_tool".to_string(), json!(false));
    entry.insert(
        "experimental_supported_tools".to_string(),
        json!(["send_user_message_async", "clock"]),
    );
    entry.insert(
        "truncation_policy".to_string(),
        json!({ "mode": "tokens", "limit": 10000 }),
    );
    if slug_key == "mimo-v2.5-pro" {
        entry.insert("input_modalities".to_string(), json!(["text"]));
    } else {
        entry.insert("input_modalities".to_string(), json!(["text", "image"]));
    }
    if slug_key == "mimo-v2.6-pro" {
        entry.insert("node_repl_auto_review_required".to_string(), json!(true));
    }
    let user_context = spec
        .and_then(|item| item.context_window)
        .filter(|value| *value > 0);
    if user_context.is_none() {
        entry.insert("context_window".to_string(), json!(1_048_576));
        entry.insert("max_context_window".to_string(), json!(1_048_576));
    }
    let user_reasoning = spec.is_some_and(|item| !item.reasoning_levels.is_empty());
    if !user_reasoning {
        let supported: Vec<Value> = REASONING_LEVEL_DESCRIPTIONS
            .iter()
            .filter(|(effort, _)| matches!(*effort, "none" | "low" | "medium" | "high"))
            .map(|(effort, description)| json!({ "effort": *effort, "description": *description }))
            .collect();
        entry.insert("supported_reasoning_levels".to_string(), json!(supported));
        entry.insert("default_reasoning_level".to_string(), json!("low"));
    }
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
    effort_aliases: &[(&'static str, &'static str)],
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
            apply_reasoning_levels(object, spec, effort_aliases);
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

fn catalog_effort_level(level: &str, aliases: &[(&'static str, &'static str)]) -> String {
    aliases
        .iter()
        .find(|(_, provider_level)| provider_level.eq_ignore_ascii_case(level))
        .map(|(codex_level, _)| (*codex_level).to_string())
        .unwrap_or_else(|| level.to_ascii_lowercase())
}

fn apply_reasoning_levels(
    entry: &mut serde_json::Map<String, Value>,
    spec: &CatalogModel,
    effort_aliases: &[(&'static str, &'static str)],
) {
    if spec.reasoning_levels.is_empty() {
        clear_reasoning_levels(entry);
        return;
    }
    let configured: Vec<String> = spec
        .reasoning_levels
        .iter()
        .map(|level| catalog_effort_level(level, effort_aliases))
        .collect();
    let supported: Vec<Value> = REASONING_LEVEL_DESCRIPTIONS
        .iter()
        .filter(|(effort, _)| {
            configured
                .iter()
                .any(|level| level.eq_ignore_ascii_case(effort))
        })
        .map(|(effort, description)| json!({ "effort": *effort, "description": *description }))
        .collect();
    if supported.is_empty() {
        clear_reasoning_levels(entry);
        return;
    }
    let default = catalog_effort_level(spec.default_reasoning_level.trim(), effort_aliases);
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

    #[test]
    fn bigmodel_catalog_maps_provider_max_to_codex_xhigh() {
        let provider = Provider {
            id: "bigmodel".to_string(),
            name: "Z.AI".to_string(),
            kind: ProviderKind::ApiKey,
            model: "GLM-5.3-Flash".to_string(),
            base_url: "https://open.bigmodel.cn/api/v1".to_string(),
            wire_api: "responses".to_string(),
            catalog_models: vec![crate::providers::CatalogModel {
                display_name: "GLM-5.3-Flash".to_string(),
                model: "GLM-5.3-Flash".to_string(),
                context_window: None,
                reasoning_levels: vec!["low".to_string(), "high".to_string(), "max".to_string()],
                default_reasoning_level: "max".to_string(),
            }],
            ..Provider::default()
        };
        let catalog = build_provider_catalog(&provider).expect("catalog");
        let entry = &catalog["models"][0];
        let efforts: Vec<&str> = entry["supported_reasoning_levels"]
            .as_array()
            .expect("levels")
            .iter()
            .filter_map(|item| item.get("effort").and_then(Value::as_str))
            .collect();
        assert_eq!(efforts, vec!["low", "high", "xhigh"]);
        assert_eq!(entry["default_reasoning_level"], "xhigh");
    }

    #[test]
    fn mimo_v26_catalog_enables_responses_lite_without_replacing_codex_instructions() {
        let provider = Provider {
            id: "mimo".to_string(),
            name: "MiMo".to_string(),
            kind: ProviderKind::ApiKey,
            model: "mimo-v2.6-pro".to_string(),
            base_url: "https://api.xiaomimimo.com/v1".to_string(),
            wire_api: "responses".to_string(),
            template: "mimo".to_string(),
            ..Provider::default()
        };
        let catalog = build_mixed_catalog(&[&provider], false).expect("catalog");
        let entry = &catalog["models"][0];
        assert_eq!(entry["slug"], "mimo-v2.6-pro");
        assert_eq!(entry["use_responses_lite"], true);
        assert_eq!(entry["apply_patch_tool_type"], "freeform");
        assert!(entry.get("tool_mode").is_none());
        assert!(entry.get("multi_agent_version").is_none());
        assert_eq!(entry["context_window"], 1_048_576);
        assert_eq!(entry["default_reasoning_level"], "low");
        assert_eq!(entry["node_repl_auto_review_required"], true);
        let efforts: Vec<&str> = entry["supported_reasoning_levels"]
            .as_array()
            .expect("levels")
            .iter()
            .filter_map(|item| item.get("effort").and_then(Value::as_str))
            .collect();
        assert_eq!(efforts, vec!["none", "low", "medium", "high"]);
        assert_eq!(
            entry["base_instructions"].as_str(),
            Some(
                "You are Codex, a coding agent. You and the user share the same workspace and collaborate to achieve the user's goals."
            )
        );
        assert!(entry.get("model_messages").is_none());
    }

    #[test]
    fn mimo_v25_catalog_stays_on_standard_responses() {
        let provider = Provider {
            id: "mimo-plan".to_string(),
            name: "MiMo Token Plan".to_string(),
            kind: ProviderKind::ApiKey,
            model: "mimo-v2.5-pro".to_string(),
            base_url: "https://token-plan-cn.xiaomimimo.com/v1".to_string(),
            wire_api: "responses".to_string(),
            template: "mimo-plan".to_string(),
            catalog_models: vec![crate::providers::CatalogModel {
                display_name: "MiMo-V2.5-Pro".to_string(),
                model: "mimo-v2.5-pro".to_string(),
                context_window: Some(262_144),
                reasoning_levels: vec!["high".to_string()],
                default_reasoning_level: "high".to_string(),
            }],
            ..Provider::default()
        };
        let catalog = build_provider_catalog(&provider).expect("catalog");
        let entry = &catalog["models"][0];
        assert_eq!(entry["use_responses_lite"], false);
        assert!(entry.get("apply_patch_tool_type").is_none());
        assert_eq!(entry["context_window"], 262_144);
        assert_eq!(entry["default_reasoning_level"], "high");
        assert_eq!(entry["input_modalities"], json!(["text"]));
    }

    #[test]
    fn mimo_v26_catalog_uses_code_mode_only_when_host_exists() {
        let provider = Provider {
            id: "mimo".to_string(),
            name: "MiMo".to_string(),
            kind: ProviderKind::ApiKey,
            model: "mimo-v2.6-flash".to_string(),
            base_url: "https://api.xiaomimimo.com/v1".to_string(),
            wire_api: "responses".to_string(),
            template: "mimo".to_string(),
            ..Provider::default()
        };
        let catalog = build_mixed_catalog(&[&provider], true).expect("catalog");
        let entry = &catalog["models"][0];
        assert_eq!(entry["tool_mode"], "code_mode_only");
        assert!(entry.get("multi_agent_version").is_none());
        assert_eq!(entry["use_responses_lite"], true);
    }

    #[test]
    fn mixed_catalog_namespaces_colliding_slugs_without_prefixing_display_names() {
        let grok = Provider {
            id: "grok".to_string(),
            name: "Grok".to_string(),
            kind: ProviderKind::ApiKey,
            model: "shared-model".to_string(),
            models: vec!["grok-only".to_string()],
            ..Provider::default()
        };
        let mimo = Provider {
            id: "mimo".to_string(),
            name: "MiMo".to_string(),
            kind: ProviderKind::ApiKey,
            model: "shared-model".to_string(),
            models: vec!["mimo-v2.6-pro".to_string()],
            base_url: "https://api.xiaomimimo.com/v1".to_string(),
            template: "mimo".to_string(),
            ..Provider::default()
        };
        let catalog = build_mixed_catalog(&[&grok, &mimo], false).expect("catalog");
        let slugs: Vec<&str> = catalog["models"]
            .as_array()
            .expect("models")
            .iter()
            .filter_map(|entry| entry.get("slug").and_then(Value::as_str))
            .collect();
        assert_eq!(
            slugs,
            vec![
                "grok::shared-model",
                "grok-only",
                "mimo::shared-model",
                "mimo-v2.6-pro"
            ]
        );
        let names: Vec<&str> = catalog["models"]
            .as_array()
            .expect("models")
            .iter()
            .filter_map(|entry| entry.get("display_name").and_then(Value::as_str))
            .collect();
        assert_eq!(names[0], "shared-model");
        assert_eq!(names[1], "grok-only");
        assert_eq!(names[3], "mimo-v2.6-pro");
        assert!(catalog["models"][3].get("tool_mode").is_none());
    }

    #[test]
    fn prefix_model_names_adds_the_provider_name_to_that_providers_display_names() {
        let grok = Provider {
            id: "grok".to_string(),
            name: "Grok".to_string(),
            kind: ProviderKind::ApiKey,
            model: "shared-model".to_string(),
            prefix_model_names: true,
            ..Provider::default()
        };
        let copilot = Provider {
            id: "copilot".to_string(),
            name: "Copilot".to_string(),
            kind: ProviderKind::ApiKey,
            model: "shared-model".to_string(),
            ..Provider::default()
        };
        let catalog = build_mixed_catalog(&[&grok, &copilot], false).expect("catalog");
        let names: Vec<&str> = catalog["models"]
            .as_array()
            .expect("models")
            .iter()
            .filter_map(|entry| entry.get("display_name").and_then(Value::as_str))
            .collect();
        assert_eq!(names, vec!["Grok / shared-model", "shared-model"]);
    }

    #[test]
    fn non_mimo_catalog_does_not_enable_responses_lite() {
        let provider = Provider {
            id: "glm".to_string(),
            name: "Zhipu GLM".to_string(),
            kind: ProviderKind::ApiKey,
            model: "glm-5.3".to_string(),
            base_url: "https://open.bigmodel.cn/api/v1".to_string(),
            ..Provider::default()
        };
        let catalog = build_provider_catalog(&provider).expect("catalog");
        assert!(catalog["models"][0].get("use_responses_lite").is_none());
    }
}
