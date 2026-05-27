use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use tokio::process::{Child, Command};

use crate::cdp;
use crate::proxy_env::loopback_no_proxy_value;

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

pub fn codex_executable_path(app_path: &Path) -> PathBuf {
    if app_path
        .extension()
        .and_then(|extension| extension.to_str())
        == Some("app")
    {
        return app_path.join("Contents").join("MacOS").join("Codex");
    }
    app_path.to_path_buf()
}

pub fn codex_debug_args(debug_port: u16) -> Vec<String> {
    vec![
        format!("--remote-debugging-port={debug_port}"),
        "--remote-debugging-address=127.0.0.1".to_string(),
        format!("--remote-allow-origins=http://127.0.0.1:{debug_port}"),
    ]
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CodexLaunchCommand {
    program: PathBuf,
    args: Vec<String>,
    keeps_child: bool,
}

fn codex_launch_command(app_path: &Path, debug_port: u16) -> CodexLaunchCommand {
    codex_launch_command_for_platform(app_path, debug_port, std::env::consts::OS)
}

fn codex_launch_command_for_platform(
    app_path: &Path,
    debug_port: u16,
    platform: &str,
) -> CodexLaunchCommand {
    let debug_args = codex_debug_args(debug_port);
    if platform == "macos" && is_app_bundle_path(app_path) {
        let mut args = vec![
            "-na".to_string(),
            app_path.to_string_lossy().into_owned(),
            "--args".to_string(),
        ];
        args.extend(debug_args);
        return CodexLaunchCommand {
            program: PathBuf::from("open"),
            args,
            keeps_child: false,
        };
    }
    CodexLaunchCommand {
        program: codex_executable_path(app_path),
        args: debug_args,
        keeps_child: true,
    }
}

fn is_app_bundle_path(path: &Path) -> bool {
    path.extension().and_then(|extension| extension.to_str()) == Some("app")
}

pub async fn ensure_codex_launched_with_debug_port(
    app_path: &Path,
    debug_port: u16,
    attach_only: bool,
    port_hold: Option<std::net::TcpListener>,
) -> anyhow::Result<Option<Child>> {
    let mut port_hold = port_hold;
    if attach_only {
        if cdp::has_codex_cdp_target(debug_port).await {
            return Ok(None);
        }
        anyhow::bail!(
            "Codex CDP is not ready on port {debug_port}. Start Codex with remote debugging on that port or let Codex Helper auto-select."
        );
    }
    if port_hold.is_none() && cdp::is_debug_port_ready(debug_port).await {
        if cdp::has_codex_cdp_target(debug_port).await {
            anyhow::bail!(
                "Debug port {debug_port} already exposes Codex CDP. Use an explicit debug port only when you intend to attach to that existing Codex, or let Codex Helper launch a managed Codex on a random port."
            );
        }
        anyhow::bail!(
            "Debug port {debug_port} exposes a browser CDP endpoint but not Codex. Stop the other app or let Codex Helper auto-select another port."
        );
    }

    let launch_command = codex_launch_command(app_path, debug_port);
    if !launch_command.program.exists() && launch_command.program != PathBuf::from("open") {
        anyhow::bail!(
            "Codex executable not found: {}",
            launch_command.program.display()
        );
    }

    drop(port_hold.take());
    let no_proxy = loopback_no_proxy_value();
    let mut command = Command::new(&launch_command.program);
    command
        .args(&launch_command.args)
        .env("NO_PROXY", &no_proxy)
        .env("no_proxy", &no_proxy)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let child = command
        .spawn()
        .map_err(|error| anyhow::anyhow!("failed to launch Codex executable: {error}"))?;
    let mut child = child;
    if let Err(error) = cdp::wait_for_debug_port(debug_port, Duration::from_secs(60)).await {
        let _ = child.kill().await;
        return Err(error);
    }
    if !launch_command.keeps_child {
        return Ok(None);
    }
    Ok(Some(child))
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;

    #[test]
    fn launcher_codex_debug_args_include_remote_debugging_port() {
        let args = codex_debug_args(9229);

        assert!(args.contains(&"--remote-debugging-port=9229".to_string()));
        assert!(args.contains(&"--remote-debugging-address=127.0.0.1".to_string()));
        assert!(args.contains(&"--remote-allow-origins=http://127.0.0.1:9229".to_string()));
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

    #[test]
    fn launcher_resolves_macos_bundle_executable_path() {
        let executable = codex_executable_path(Path::new("/Applications/Codex.app"));

        assert_eq!(
            executable,
            PathBuf::from("/Applications/Codex.app/Contents/MacOS/Codex")
        );
    }

    #[test]
    fn launcher_preserves_direct_executable_path() {
        let executable = codex_executable_path(Path::new("/usr/local/bin/codex"));

        assert_eq!(executable, PathBuf::from("/usr/local/bin/codex"));
    }

    #[test]
    fn launcher_uses_launchservices_for_macos_app_bundles() {
        let command =
            codex_launch_command_for_platform(Path::new("/Applications/Codex.app"), 9229, "macos");

        assert_eq!(command.program, PathBuf::from("open"));
        assert_eq!(
            command.args,
            vec![
                "-na",
                "/Applications/Codex.app",
                "--args",
                "--remote-debugging-port=9229",
                "--remote-debugging-address=127.0.0.1",
                "--remote-allow-origins=http://127.0.0.1:9229",
            ]
        );
        assert!(!command.keeps_child);
    }

    #[test]
    fn launcher_starts_direct_executables_without_launchservices() {
        let command =
            codex_launch_command_for_platform(Path::new("/usr/local/bin/codex"), 9229, "linux");

        assert_eq!(command.program, PathBuf::from("/usr/local/bin/codex"));
        assert_eq!(
            command.args,
            vec![
                "--remote-debugging-port=9229",
                "--remote-debugging-address=127.0.0.1",
                "--remote-allow-origins=http://127.0.0.1:9229",
            ]
        );
        assert!(command.keeps_child);
    }
}
