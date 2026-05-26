use std::fs;
use std::path::Path;

use anyhow::Context;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", default)]
pub struct HelperSettings {
    pub markdown_export_enabled: bool,
    pub session_move_enabled: bool,
    pub auto_rename_menu_enabled: bool,
    pub markdown_friendly_filename_enabled: bool,
    pub auto_naming_min_chars: u8,
    pub auto_naming_max_chars: u8,
    pub port_forwarding_enabled: bool,
    pub port_auto_forward_web: bool,
    pub port_same_local_port: bool,
}

impl Default for HelperSettings {
    fn default() -> Self {
        Self {
            markdown_export_enabled: false,
            session_move_enabled: false,
            auto_rename_menu_enabled: false,
            markdown_friendly_filename_enabled: true,
            auto_naming_min_chars: 4,
            auto_naming_max_chars: 10,
            port_forwarding_enabled: false,
            port_auto_forward_web: true,
            port_same_local_port: true,
        }
    }
}

const LEGACY_SETTINGS_KEYS: &[&str] = &["sessionDeleteEnabled"];

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
        match key.as_str() {
            "markdownExportEnabled" => settings.markdown_export_enabled = bool_setting(key, value)?,
            "sessionMoveEnabled" => settings.session_move_enabled = bool_setting(key, value)?,
            "autoRenameMenuEnabled" => {
                settings.auto_rename_menu_enabled = bool_setting(key, value)?
            }
            "markdownFriendlyFilenameEnabled" => {
                settings.markdown_friendly_filename_enabled = bool_setting(key, value)?
            }
            "autoNamingMinChars" => {
                settings.auto_naming_min_chars = char_count_setting(key, value)?
            }
            "autoNamingMaxChars" => {
                settings.auto_naming_max_chars = char_count_setting(key, value)?
            }
            "autoNamingMinWords" => {
                settings.auto_naming_min_chars = char_count_setting("autoNamingMinChars", value)?
            }
            "autoNamingMaxWords" => {
                settings.auto_naming_max_chars = char_count_setting("autoNamingMaxChars", value)?
            }
            "portForwardingEnabled" => settings.port_forwarding_enabled = bool_setting(key, value)?,
            "portAutoForwardWeb" => settings.port_auto_forward_web = bool_setting(key, value)?,
            "portSameLocalPort" => settings.port_same_local_port = bool_setting(key, value)?,
            key if LEGACY_SETTINGS_KEYS.contains(&key) => {}
            _ => anyhow::bail!("Unknown settings key: {key}"),
        }
    }
    validate_auto_naming_range(&settings)?;
    Ok(settings)
}

pub fn update_settings(path: &Path, payload: &Value) -> anyhow::Result<HelperSettings> {
    let mut settings = read_settings(path)?;
    let object = payload
        .as_object()
        .ok_or_else(|| anyhow::anyhow!("Settings payload must be an object"))?;

    for (key, value) in object {
        match key.as_str() {
            "markdownExportEnabled" => settings.markdown_export_enabled = bool_setting(key, value)?,
            "sessionMoveEnabled" => settings.session_move_enabled = bool_setting(key, value)?,
            "autoRenameMenuEnabled" => {
                settings.auto_rename_menu_enabled = bool_setting(key, value)?
            }
            "markdownFriendlyFilenameEnabled" => {
                settings.markdown_friendly_filename_enabled = bool_setting(key, value)?
            }
            "autoNamingMinChars" => {
                settings.auto_naming_min_chars = char_count_setting(key, value)?
            }
            "autoNamingMaxChars" => {
                settings.auto_naming_max_chars = char_count_setting(key, value)?
            }
            "autoNamingMinWords" => {
                settings.auto_naming_min_chars = char_count_setting("autoNamingMinChars", value)?
            }
            "autoNamingMaxWords" => {
                settings.auto_naming_max_chars = char_count_setting("autoNamingMaxChars", value)?
            }
            "portForwardingEnabled" => settings.port_forwarding_enabled = bool_setting(key, value)?,
            "portAutoForwardWeb" => settings.port_auto_forward_web = bool_setting(key, value)?,
            "portSameLocalPort" => settings.port_same_local_port = bool_setting(key, value)?,
            _ => return Err(anyhow::anyhow!("Unknown settings key: {key}")),
        }
    }
    validate_auto_naming_range(&settings)?;

    write_settings(path, &settings)?;
    Ok(settings)
}

fn bool_setting(key: &str, value: &Value) -> anyhow::Result<bool> {
    value
        .as_bool()
        .ok_or_else(|| anyhow::anyhow!("Settings value for {key} must be a boolean"))
}

fn char_count_setting(key: &str, value: &Value) -> anyhow::Result<u8> {
    let count = value
        .as_u64()
        .ok_or_else(|| anyhow::anyhow!("Settings value for {key} must be an integer"))?;
    if !(1..=20).contains(&count) {
        anyhow::bail!("Settings value for {key} must be between 1 and 20");
    }
    Ok(count as u8)
}

fn validate_auto_naming_range(settings: &HelperSettings) -> anyhow::Result<()> {
    if settings.auto_naming_min_chars > settings.auto_naming_max_chars {
        anyhow::bail!("autoNamingMinChars must be less than or equal to autoNamingMaxChars");
    }
    Ok(())
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
    fn default_settings_disable_session_tools() {
        let settings = HelperSettings::default();

        assert!(!settings.markdown_export_enabled);
        assert!(!settings.session_move_enabled);
        assert!(!settings.port_forwarding_enabled);
        assert!(settings.port_auto_forward_web);
        assert!(settings.port_same_local_port);
        assert!(!settings.auto_rename_menu_enabled);
        assert!(settings.markdown_friendly_filename_enabled);
        assert_eq!(settings.auto_naming_min_chars, 4);
        assert_eq!(settings.auto_naming_max_chars, 10);
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

        assert!(!settings.markdown_export_enabled);
        assert!(settings.session_move_enabled);
        assert!(!settings.port_forwarding_enabled);
        assert!(settings.port_auto_forward_web);
        assert!(settings.port_same_local_port);
    }

    #[test]
    fn read_settings_ignores_known_removed_keys() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let path = temp_dir.path().join("config.json");
        fs::write(
            &path,
            r#"{
  "markdownExportEnabled": true,
  "sessionDeleteEnabled": true
}
"#,
        )
        .expect("legacy settings");

        let settings = read_settings(&path).expect("legacy settings should load");

        assert!(settings.markdown_export_enabled);
        assert!(!settings.session_move_enabled);
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
        fs::write(&path, r#"{ "markdownExportEnabled": "yes" }"#).expect("settings");

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
                "markdownExportEnabled": true,
                "portForwardingEnabled": true,
                "portAutoForwardWeb": false,
                "portSameLocalPort": true,
                "autoRenameMenuEnabled": true,
                "markdownFriendlyFilenameEnabled": false,
                "autoNamingMinChars": 3,
                "autoNamingMaxChars": 7,
            }),
        )
        .expect("updated settings");
        let persisted = read_settings(&path).expect("persisted settings");

        assert!(settings.markdown_export_enabled);
        assert!(!settings.session_move_enabled);
        assert!(settings.port_forwarding_enabled);
        assert!(!settings.port_auto_forward_web);
        assert!(settings.port_same_local_port);
        assert!(settings.auto_rename_menu_enabled);
        assert!(!settings.markdown_friendly_filename_enabled);
        assert_eq!(settings.auto_naming_min_chars, 3);
        assert_eq!(settings.auto_naming_max_chars, 7);
        assert_eq!(settings, persisted);
    }

    #[test]
    fn update_settings_rejects_invalid_auto_naming_range() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let path = temp_dir.path().join("config.json");
        ensure_settings_file(&path).expect("initial settings");

        let error = update_settings(
            &path,
            &serde_json::json!({
                "autoNamingMinChars": 9,
                "autoNamingMaxChars": 4
            }),
        )
        .expect_err("invalid range should fail");

        assert!(error.to_string().contains("autoNamingMinChars"));
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
}
