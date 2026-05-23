use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use crate::cdp;

pub const DEFAULT_CODEX_APP_PATH: &str = "/Applications/Codex.app";
pub fn resolve_codex_app_path(explicit_path: Option<&Path>) -> anyhow::Result<PathBuf> {
    let candidate = explicit_path
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from(DEFAULT_CODEX_APP_PATH));
    if !candidate.exists() {
        anyhow::bail!("Codex app not found: {}", candidate.display());
    }
    Ok(candidate)
}

pub fn codex_debug_args(debug_port: u16) -> Vec<String> {
    vec![
        format!("--remote-debugging-port={debug_port}"),
        format!("--remote-allow-origins=http://127.0.0.1:{debug_port}"),
    ]
}

pub fn process_command(pid: u32) -> anyhow::Result<String> {
    let output = std::process::Command::new("ps")
        .args(["-p", &pid.to_string(), "-o", "command="])
        .output()
        .map_err(|error| anyhow::anyhow!("ps failed for pid {pid}: {error}"))?;
    if !output.status.success() {
        anyhow::bail!(
            "ps failed for pid {pid} with status {}: {}",
            output
                .status
                .code()
                .map_or_else(|| "signal".to_string(), |code| code.to_string()),
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

pub fn is_killable_port_blocker(command: &str) -> bool {
    let normalized = command.to_lowercase();
    normalized.contains("codex.app")
        || normalized.split_whitespace().any(|word| word == "codex")
        || normalized.contains("/codex.app/")
}

pub fn listen_pids_on_port(port: u16) -> anyhow::Result<Vec<u32>> {
    let listen_arg = format!("-tiTCP:{port}");
    let output = std::process::Command::new("lsof")
        .args([&listen_arg, "-sTCP:LISTEN"])
        .output()
        .map_err(|error| anyhow::anyhow!("lsof failed for port {port}: {error}"))?;
    if !output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        if stdout.is_empty() && stderr.is_empty() {
            return Ok(Vec::new());
        }
        anyhow::bail!(
            "lsof failed for port {port} with status {}: {}",
            output
                .status
                .code()
                .map_or_else(|| "signal".to_string(), |code| code.to_string()),
            if stderr.is_empty() { stdout } else { stderr }
        );
    }
    Ok(String::from_utf8_lossy(&output.stdout)
        .split_whitespace()
        .filter_map(|value| value.parse::<u32>().ok())
        .collect())
}

pub fn is_codex_running() -> bool {
    std::process::Command::new("pgrep")
        .args(["-x", "Codex"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

pub async fn quit_codex(timeout: Duration) -> anyhow::Result<()> {
    let _ = std::process::Command::new("osascript")
        .args(["-e", r#"tell application "Codex" to quit"#])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    let started_at = std::time::Instant::now();
    while started_at.elapsed() < timeout {
        if !is_codex_running() {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
    anyhow::bail!("Timed out waiting for Codex to quit")
}

pub async fn release_blocked_debug_port(debug_port: u16) -> anyhow::Result<()> {
    if cdp::is_debug_port_ready(debug_port).await {
        if cdp::has_codex_cdp_target(debug_port).await {
            return Ok(());
        }
        anyhow::bail!(
            "Debug port {debug_port} exposes a browser CDP endpoint but not Codex. Stop the other app or let Codex Helper auto-select another port."
        );
    }
    let initial_pids = listen_pids_on_port(debug_port)?;
    if initial_pids.is_empty() {
        return Ok(());
    }

    let mut killable_pids = Vec::new();
    for pid in &initial_pids {
        if is_killable_port_blocker(&process_command(*pid)?) {
            killable_pids.push(*pid);
        }
    }
    let blocked_by: Vec<String> = initial_pids
        .iter()
        .filter(|pid| !killable_pids.contains(pid))
        .map(|pid| process_command(*pid).map(|command| format!("{pid}:{command}")))
        .collect::<anyhow::Result<Vec<_>>>()?;
    if !blocked_by.is_empty() {
        anyhow::bail!(
            "Debug port {debug_port} is blocked by a non-Codex process: {}. Stop it manually or let Codex Helper auto-select a port.",
            blocked_by.join("; ")
        );
    }

    for pid in &killable_pids {
        let _ = std::process::Command::new("kill")
            .arg(pid.to_string())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
    tokio::time::sleep(Duration::from_millis(750)).await;
    if cdp::is_debug_port_ready(debug_port).await {
        return Ok(());
    }
    for pid in listen_pids_on_port(debug_port)? {
        if is_killable_port_blocker(&process_command(pid)?) {
            let _ = std::process::Command::new("kill")
                .args(["-9", &pid.to_string()])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
        }
    }
    tokio::time::sleep(Duration::from_millis(500)).await;
    if !listen_pids_on_port(debug_port)?.is_empty() {
        anyhow::bail!("Debug port {debug_port} is still blocked after releasing Codex listeners");
    }
    Ok(())
}

pub async fn ensure_codex_launched_with_debug_port(
    app_path: &Path,
    debug_port: u16,
    attach_only: bool,
    port_hold: Option<std::net::TcpListener>,
) -> anyhow::Result<()> {
    let _port_hold = port_hold;
    if attach_only {
        if cdp::has_codex_cdp_target(debug_port).await {
            return Ok(());
        }
        anyhow::bail!(
            "Codex CDP is not ready on port {debug_port}. Start Codex with remote debugging on that port or let Codex Helper auto-select."
        );
    }
    if cdp::is_debug_port_ready(debug_port).await {
        return Ok(());
    }
    release_blocked_debug_port(debug_port).await?;
    if is_codex_running() {
        quit_codex(Duration::from_secs(15)).await?;
        tokio::time::sleep(Duration::from_millis(500)).await;
        release_blocked_debug_port(debug_port).await?;
    }
    let app_path_string = app_path.to_string_lossy().to_string();
    let mut open_args = vec!["-na".to_string(), app_path_string, "--args".to_string()];
    open_args.extend(codex_debug_args(debug_port));
    std::process::Command::new("open")
        .args(open_args)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| anyhow::anyhow!("failed to open Codex app: {error}"))?;
    cdp::wait_for_debug_port(debug_port, Duration::from_secs(60)).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;

    #[test]
    fn launcher_codex_debug_args_include_remote_debugging_port() {
        let args = codex_debug_args(9229);

        assert!(args.contains(&"--remote-debugging-port=9229".to_string()));
        assert!(args.contains(&"--remote-allow-origins=http://127.0.0.1:9229".to_string()));
    }

    #[test]
    fn launcher_identifies_codex_port_blockers() {
        assert!(is_killable_port_blocker(
            "/Applications/Codex.app/Contents/MacOS/Codex"
        ));
        assert!(!is_killable_port_blocker(
            "/System/Library/PrivateFrameworks/SkyComputerUseService"
        ));
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
