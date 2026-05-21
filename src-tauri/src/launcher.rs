use std::path::{Path, PathBuf};
use std::process::Stdio;

use tokio::process::{Child, Command};

pub const DEFAULT_CODEX_APP_PATH: &str = "/Applications/Codex.app";
pub const DEFAULT_DEBUG_PORT: u16 = 9229;

pub fn resolve_codex_app_path(explicit_path: Option<&Path>) -> anyhow::Result<PathBuf> {
    let candidate = explicit_path
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from(DEFAULT_CODEX_APP_PATH));
    if !candidate.exists() {
        anyhow::bail!("Codex app not found: {}", candidate.display());
    }
    Ok(candidate)
}

pub fn build_macos_open_command(app_path: &Path, debug_port: u16) -> Vec<String> {
    vec![
        "open".to_string(),
        "-W".to_string(),
        "-a".to_string(),
        app_path.to_string_lossy().to_string(),
        "--args".to_string(),
        format!("--remote-debugging-port={debug_port}"),
    ]
}

pub async fn launch_codex(app_path: &Path, debug_port: u16) -> anyhow::Result<Child> {
    let command = build_macos_open_command(app_path, debug_port);
    let executable = command
        .first()
        .ok_or_else(|| anyhow::anyhow!("Codex launch command is empty"))?;
    let child = Command::new(executable)
        .args(&command[1..])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| anyhow::anyhow!("failed to launch Codex app: {error}"))?;
    Ok(child)
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;

    #[test]
    fn launcher_macos_open_command_waits_for_app_exit() {
        let command = build_macos_open_command(Path::new("/Applications/Codex.app"), 9229);

        assert_eq!(command[0], "open");
        assert!(command.contains(&"-W".to_string()));
        assert!(command.contains(&"-a".to_string()));
        assert!(command.contains(&"--args".to_string()));
        assert!(command.contains(&"--remote-debugging-port=9229".to_string()));
    }

    #[test]
    fn launcher_reports_missing_codex_app_path() {
        let missing = Path::new("/tmp/codex-helper-missing/Codex.app");

        let error = resolve_codex_app_path(Some(missing)).unwrap_err();

        assert_eq!(
            error.to_string(),
            "Codex app not found: /tmp/codex-helper-missing/Codex.app"
        );
    }
}
