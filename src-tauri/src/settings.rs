use std::fs;
use std::path::Path;

use anyhow::Context;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields, default)]
pub struct HelperSettings {
    pub session_delete_enabled: bool,
    pub markdown_export_enabled: bool,
    pub session_move_enabled: bool,
    pub port_forwarding_enabled: bool,
    pub port_auto_forward_web: bool,
    pub port_same_local_port: bool,
}

impl Default for HelperSettings {
    fn default() -> Self {
        Self {
            session_delete_enabled: false,
            markdown_export_enabled: false,
            session_move_enabled: false,
            port_forwarding_enabled: false,
            port_auto_forward_web: true,
            port_same_local_port: true,
        }
    }
}

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
    serde_json::from_str(&contents).with_context(|| format!("Failed to parse {}", path.display()))
}

pub fn update_settings(path: &Path, payload: &Value) -> anyhow::Result<HelperSettings> {
    let mut settings = read_settings(path)?;
    let object = payload
        .as_object()
        .ok_or_else(|| anyhow::anyhow!("Settings payload must be an object"))?;

    for (key, value) in object {
        let enabled = value
            .as_bool()
            .ok_or_else(|| anyhow::anyhow!("Settings value for {key} must be a boolean"))?;
        match key.as_str() {
            "sessionDeleteEnabled" => settings.session_delete_enabled = enabled,
            "markdownExportEnabled" => settings.markdown_export_enabled = enabled,
            "sessionMoveEnabled" => settings.session_move_enabled = enabled,
            "portForwardingEnabled" => settings.port_forwarding_enabled = enabled,
            "portAutoForwardWeb" => settings.port_auto_forward_web = enabled,
            "portSameLocalPort" => settings.port_same_local_port = enabled,
            _ => return Err(anyhow::anyhow!("Unknown settings key: {key}")),
        }
    }

    write_settings(path, &settings)?;
    Ok(settings)
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

        assert!(!settings.session_delete_enabled);
        assert!(!settings.markdown_export_enabled);
        assert!(!settings.session_move_enabled);
        assert!(!settings.port_forwarding_enabled);
        assert!(settings.port_auto_forward_web);
        assert!(settings.port_same_local_port);
    }

    #[test]
    fn read_settings_accepts_legacy_files_without_port_forwarding_keys() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let path = temp_dir.path().join("config.json");
        fs::write(
            &path,
            r#"{
  "sessionDeleteEnabled": true,
  "markdownExportEnabled": false,
  "sessionMoveEnabled": true
}
"#,
        )
        .expect("legacy settings");

        let settings = read_settings(&path).expect("legacy settings should load");

        assert!(settings.session_delete_enabled);
        assert!(!settings.markdown_export_enabled);
        assert!(settings.session_move_enabled);
        assert!(!settings.port_forwarding_enabled);
        assert!(settings.port_auto_forward_web);
        assert!(settings.port_same_local_port);
    }

    #[test]
    fn update_settings_persists_known_switches() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let path = temp_dir.path().join("config.json");
        ensure_settings_file(&path).expect("initial settings");

        let settings = update_settings(
            &path,
            &serde_json::json!({
                "sessionDeleteEnabled": true,
                "markdownExportEnabled": true,
                "portForwardingEnabled": true,
                "portAutoForwardWeb": false,
                "portSameLocalPort": true,
            }),
        )
        .expect("updated settings");
        let persisted = read_settings(&path).expect("persisted settings");

        assert!(settings.session_delete_enabled);
        assert!(settings.markdown_export_enabled);
        assert!(!settings.session_move_enabled);
        assert!(settings.port_forwarding_enabled);
        assert!(!settings.port_auto_forward_web);
        assert!(settings.port_same_local_port);
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
}
