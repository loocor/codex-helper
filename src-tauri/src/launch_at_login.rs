use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::Context;

pub const LAUNCH_AGENT_LABEL: &str = "ai.codexhelper.launcher";

pub fn apply_launch_at_login(enabled: bool) -> anyhow::Result<()> {
    #[cfg(target_os = "macos")]
    {
        remove_legacy_launch_agent()?;
        let program = login_program()?;
        if app_bundle_from_exe(&program).is_none() {
            anyhow::bail!("Start at login requires the packaged Codex Helper app");
        }
        smapp_service::set_enabled(enabled)
    }
    #[cfg(not(target_os = "macos"))]
    {
        if enabled {
            anyhow::bail!("Start at login is only supported on macOS");
        }
        Ok(())
    }
}

pub fn app_bundle_from_exe(exe: &Path) -> Option<PathBuf> {
    let macos_dir = exe.parent()?;
    if macos_dir.file_name()? != "MacOS" {
        return None;
    }
    let contents = macos_dir.parent()?;
    if contents.file_name()? != "Contents" {
        return None;
    }
    let app = contents.parent()?;
    if app.extension()? != "app" {
        return None;
    }
    Some(app.to_path_buf())
}

fn login_program() -> anyhow::Result<PathBuf> {
    let exe = std::env::current_exe().context("Failed to resolve Codex Helper executable")?;
    exe.canonicalize()
        .with_context(|| format!("Failed to resolve {}", exe.display()))
}

fn remove_legacy_launch_agent() -> anyhow::Result<()> {
    remove_legacy_launch_agent_in(&launch_agents_dir()?)?;
    bootout_legacy_launch_agent()
}

fn launch_agents_dir() -> anyhow::Result<PathBuf> {
    let home = dirs::home_dir().ok_or_else(|| anyhow::anyhow!("Home directory not found"))?;
    Ok(home.join("Library/LaunchAgents"))
}

pub fn remove_legacy_launch_agent_in(launch_agents_dir: &Path) -> anyhow::Result<()> {
    let plist_path = launch_agents_dir.join(format!("{LAUNCH_AGENT_LABEL}.plist"));
    if plist_path.exists() {
        fs::remove_file(&plist_path)
            .with_context(|| format!("Failed to remove {}", plist_path.display()))?;
    }
    Ok(())
}

fn bootout_legacy_launch_agent() -> anyhow::Result<()> {
    let uid = current_uid()?;
    let label = format!("gui/{uid}/{LAUNCH_AGENT_LABEL}");
    let output = Command::new("/bin/launchctl")
        .args(["bootout", &label])
        .output()
        .context("Failed to unload the previous login item")?;
    if output.status.success() {
        return Ok(());
    }
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    if combined.contains("No such process")
        || combined.contains("Could not find")
        || combined.contains("not found")
    {
        return Ok(());
    }
    anyhow::bail!(
        "Failed to unload the previous login item: {}",
        combined.trim()
    )
}

fn current_uid() -> anyhow::Result<u32> {
    let output = Command::new("/usr/bin/id")
        .args(["-u"])
        .output()
        .context("Failed to resolve user id")?;
    if !output.status.success() {
        anyhow::bail!("Failed to resolve user id");
    }
    String::from_utf8(output.stdout)
        .context("Failed to read user id")?
        .trim()
        .parse()
        .context("Failed to parse user id")
}

#[cfg(target_os = "macos")]
mod smapp_service {
    use std::ffi::CStr;
    use std::os::raw::{c_char, c_int};

    extern "C" {
        fn codex_helper_smapp_set_enabled(enabled: c_int, error_message: *mut *mut c_char)
            -> c_int;
        fn codex_helper_smapp_free(message: *mut c_char);
    }

    pub fn set_enabled(enabled: bool) -> anyhow::Result<()> {
        unsafe {
            let mut error_message: *mut c_char = std::ptr::null_mut();
            let result =
                codex_helper_smapp_set_enabled(if enabled { 1 } else { 0 }, &mut error_message);
            if result == 0 {
                return Ok(());
            }
            let message = if error_message.is_null() {
                "Failed to update login item".to_string()
            } else {
                let owned = CStr::from_ptr(error_message).to_string_lossy().into_owned();
                codex_helper_smapp_free(error_message);
                owned
            };
            anyhow::bail!("{message}")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn app_bundle_from_exe_detects_packaged_binary() {
        let exe = PathBuf::from("/Applications/CodexHelper.app/Contents/MacOS/codex-helper");
        assert_eq!(
            app_bundle_from_exe(&exe),
            Some(PathBuf::from("/Applications/CodexHelper.app"))
        );
    }

    #[test]
    fn app_bundle_from_exe_ignores_unpackaged_binary() {
        let exe = PathBuf::from("/tmp/codex-helper");
        assert_eq!(app_bundle_from_exe(&exe), None);
    }

    #[test]
    fn remove_legacy_launch_agent_deletes_plist() {
        let temp_dir = tempdir().expect("temp dir");
        let agents = temp_dir.path().join("LaunchAgents");
        fs::create_dir_all(&agents).expect("agents dir");
        let plist = agents.join("ai.codexhelper.launcher.plist");
        fs::write(&plist, "legacy").expect("plist");

        remove_legacy_launch_agent_in(&agents).expect("remove");

        assert!(!plist.exists());
    }

    #[test]
    fn remove_legacy_launch_agent_when_missing_succeeds() {
        let temp_dir = tempdir().expect("temp dir");
        let agents = temp_dir.path().join("LaunchAgents");

        remove_legacy_launch_agent_in(&agents).expect("remove missing");
    }
}
