use std::fs;
use std::path::PathBuf;

use crate::settings;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StateDir {
    pub root: PathBuf,
    pub logs_dir: PathBuf,
    pub scripts_dir: PathBuf,
    pub config_path: PathBuf,
    pub state_path: PathBuf,
}

impl StateDir {
    pub fn init() -> anyhow::Result<Self> {
        let root = default_root()?;
        Self::init_at(root)
    }

    pub fn init_at(root: PathBuf) -> anyhow::Result<Self> {
        let state_dir = Self::from_root(root);
        fs::create_dir_all(&state_dir.logs_dir)?;
        fs::create_dir_all(&state_dir.scripts_dir)?;
        settings::ensure_settings_file(&state_dir.config_path)?;
        Ok(state_dir)
    }

    fn from_root(root: PathBuf) -> Self {
        Self {
            logs_dir: root.join("logs"),
            scripts_dir: root.join("scripts"),
            config_path: root.join("config.json"),
            state_path: root.join("state.json"),
            root,
        }
    }
}

fn default_root() -> anyhow::Result<PathBuf> {
    let home = dirs::home_dir().ok_or_else(|| anyhow::anyhow!("Home directory not found"))?;
    Ok(home.join(".codex-helper"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_dir_creates_expected_directories() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let root = temp_dir.path().join(".codex-helper");

        let state_dir = StateDir::init_at(root.clone()).expect("state dir");

        assert_eq!(state_dir.root, root);
        assert!(state_dir.logs_dir.is_dir());
        assert!(state_dir.scripts_dir.is_dir());
        assert_eq!(state_dir.config_path, state_dir.root.join("config.json"));
        assert_eq!(state_dir.state_path, state_dir.root.join("state.json"));
    }

    #[test]
    fn state_dir_creates_default_settings() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let root = temp_dir.path().join(".codex-helper");

        let state_dir = StateDir::init_at(root).expect("state dir");
        let settings = settings::read_settings(&state_dir.config_path).expect("settings");

        assert!(!settings.markdown_export_enabled);
        assert!(!settings.session_move_enabled);
    }
}
