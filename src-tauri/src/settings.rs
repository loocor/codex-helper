use std::fs;
use std::path::Path;

use anyhow::Context;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", default)]
pub struct HelperSettings {
    pub port_forwarding_enabled: bool,
    pub port_auto_forward_web: bool,
    pub port_same_local_port: bool,
    pub hide_usage_limit_banner_enabled: bool,
    pub launch_at_login_enabled: bool,
}

impl Default for HelperSettings {
    fn default() -> Self {
        Self {
            port_forwarding_enabled: false,
            port_auto_forward_web: true,
            port_same_local_port: true,
            hide_usage_limit_banner_enabled: false,
            launch_at_login_enabled: false,
        }
    }
}

const LEGACY_SETTINGS_KEYS: &[&str] = &[
    "sessionDeleteEnabled",
    "autoRenameMenuEnabled",
    "markdownExportEnabled",
    "sessionMoveEnabled",
    "markdownFriendlyFilenameEnabled",
    "autoNamingMinChars",
    "autoNamingMaxChars",
    "autoNamingMinWords",
    "autoNamingMaxWords",
];

pub fn ensure_settings_file(path: &Path) -> anyhow::Result<HelperSettings> {
    if path.exists() {
        return read_settings(path);
    }
    let settings = HelperSettings::default();
    write_settings(path, &settings)?;
    Ok(settings)
}

pub fn read_settings(path: &Path) -> anyhow::Result<HelperSettings> {
    let contents =
        fs::read_to_string(path).with_context(|| format!("Failed to read {}", path.display()))?;
    let value: Value = serde_json::from_str(&contents)
        .with_context(|| format!("Failed to parse {}", path.display()))?;
    settings_from_value(&value)
        .map_err(|error| anyhow::anyhow!("Failed to parse {}: {error}", path.display()))
}

fn settings_from_value(value: &Value) -> anyhow::Result<HelperSettings> {
    let object = value
        .as_object()
        .ok_or_else(|| anyhow::anyhow!("Settings file must contain a JSON object"))?;
    let mut settings = HelperSettings::default();
    for (key, value) in object {
        if apply_setting_value(&mut settings, key, value)? {
            continue;
        }
        if !LEGACY_SETTINGS_KEYS.contains(&key.as_str()) {
            anyhow::bail!("Unknown settings key: {key}");
        }
    }
    Ok(settings)
}

pub fn update_settings(path: &Path, payload: &Value) -> anyhow::Result<HelperSettings> {
    let mut settings = read_settings(path)?;
    let object = payload
        .as_object()
        .ok_or_else(|| anyhow::anyhow!("Settings payload must be an object"))?;

    for (key, value) in object {
        if !apply_setting_value(&mut settings, key, value)? {
            return Err(anyhow::anyhow!("Unknown settings key: {key}"));
        }
    }
    write_settings(path, &settings)?;
    Ok(settings)
}

fn apply_setting_value(
    settings: &mut HelperSettings,
    key: &str,
    value: &Value,
) -> anyhow::Result<bool> {
    match key {
        "portForwardingEnabled" => settings.port_forwarding_enabled = bool_setting(key, value)?,
        "portAutoForwardWeb" => settings.port_auto_forward_web = bool_setting(key, value)?,
        "portSameLocalPort" => settings.port_same_local_port = bool_setting(key, value)?,
        "hideUsageLimitBannerEnabled" => {
            settings.hide_usage_limit_banner_enabled = bool_setting(key, value)?
        }
        "launchAtLoginEnabled" => settings.launch_at_login_enabled = bool_setting(key, value)?,
        _ => return Ok(false),
    }
    Ok(true)
}

fn bool_setting(key: &str, value: &Value) -> anyhow::Result<bool> {
    value
        .as_bool()
        .ok_or_else(|| anyhow::anyhow!("Settings value for {key} must be a boolean"))
}

pub fn write_settings(path: &Path, settings: &HelperSettings) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create {}", parent.display()))?;
    }
    let contents = format!("{}\n", serde_json::to_string_pretty(settings)?);
    fs::write(path, contents).with_context(|| format!("Failed to write {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_settings_keep_port_forwarding_disabled() {
        let settings = HelperSettings::default();

        assert!(!settings.port_forwarding_enabled);
        assert!(settings.port_auto_forward_web);
        assert!(settings.port_same_local_port);
        assert!(!settings.hide_usage_limit_banner_enabled);
        assert!(!settings.launch_at_login_enabled);
    }

    #[test]
    fn read_settings_accepts_legacy_files_without_port_forwarding_keys() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let path = temp_dir.path().join("config.json");
        fs::write(
            &path,
            r#"{
  "markdownExportEnabled": false,
  "sessionMoveEnabled": true
}
"#,
        )
        .expect("legacy settings");

        let settings = read_settings(&path).expect("legacy settings should load");

        assert!(!settings.port_forwarding_enabled);
        assert!(settings.port_auto_forward_web);
        assert!(settings.port_same_local_port);
    }

    #[test]
    fn read_settings_accepts_settings_with_known_removed_keys() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let path = temp_dir.path().join("config.json");
        fs::write(
            &path,
            r#"{
  "markdownExportEnabled": true,
  "sessionDeleteEnabled": true,
  "autoRenameMenuEnabled": true,
  "markdownFriendlyFilenameEnabled": true,
  "autoNamingMinChars": 8,
  "autoNamingMaxChars": 12
}
"#,
        )
        .expect("legacy settings");

        let settings = read_settings(&path).expect("legacy settings should load");

        assert!(!settings.port_forwarding_enabled);
        assert!(!settings.hide_usage_limit_banner_enabled);
    }

    #[test]
    fn read_settings_rejects_unknown_keys() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let path = temp_dir.path().join("config.json");
        fs::write(&path, r#"{ "unknownSetting": true }"#).expect("settings");

        let error = read_settings(&path).expect_err("unknown setting should fail");

        assert!(error.to_string().contains("Unknown settings key"));
    }

    #[test]
    fn read_settings_rejects_invalid_value_types() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let path = temp_dir.path().join("config.json");
        fs::write(&path, r#"{ "portForwardingEnabled": "yes" }"#).expect("settings");

        let error = read_settings(&path).expect_err("invalid setting should fail");

        assert!(error.to_string().contains("must be a boolean"));
    }

    #[test]
    fn update_settings_persists_known_switches() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let path = temp_dir.path().join("config.json");
        ensure_settings_file(&path).expect("initial settings");

        let settings = update_settings(
            &path,
            &serde_json::json!({
                "portForwardingEnabled": true,
                "portAutoForwardWeb": false,
                "portSameLocalPort": true,
            }),
        )
        .expect("updated settings");
        let persisted = read_settings(&path).expect("persisted settings");

        assert!(settings.port_forwarding_enabled);
        assert!(!settings.port_auto_forward_web);
        assert!(settings.port_same_local_port);
        assert!(!settings.hide_usage_limit_banner_enabled);
        assert!(!settings.launch_at_login_enabled);
        assert_eq!(settings, persisted);
    }

    #[test]
    fn update_settings_enables_usage_limit_banner_hide() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let path = temp_dir.path().join("config.json");
        ensure_settings_file(&path).expect("initial settings");

        let settings = update_settings(
            &path,
            &serde_json::json!({
                "hideUsageLimitBannerEnabled": true
            }),
        )
        .expect("updated settings");
        let persisted = read_settings(&path).expect("persisted settings");

        assert!(settings.hide_usage_limit_banner_enabled);
        assert_eq!(settings, persisted);
    }

    #[test]
    fn update_settings_enables_launch_at_login() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let path = temp_dir.path().join("config.json");
        ensure_settings_file(&path).expect("initial settings");

        let settings = update_settings(
            &path,
            &serde_json::json!({
                "launchAtLoginEnabled": true
            }),
        )
        .expect("updated settings");
        let persisted = read_settings(&path).expect("persisted settings");

        assert!(settings.launch_at_login_enabled);
        assert_eq!(settings, persisted);
    }

    #[test]
    fn update_settings_rejects_unknown_keys() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let path = temp_dir.path().join("config.json");
        ensure_settings_file(&path).expect("initial settings");

        let error = update_settings(&path, &serde_json::json!({ "providerSyncEnabled": true }))
            .expect_err("unknown setting should fail");

        assert!(error.to_string().contains("Unknown settings key"));
    }

    #[test]
    fn update_settings_rejects_removed_session_keys() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let path = temp_dir.path().join("config.json");
        ensure_settings_file(&path).expect("initial settings");

        let error = update_settings(&path, &serde_json::json!({ "markdownExportEnabled": true }))
            .expect_err("removed setting should fail on update");

        assert!(error.to_string().contains("Unknown settings key"));
    }
}
