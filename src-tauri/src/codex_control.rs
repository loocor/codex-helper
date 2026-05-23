use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::Mutex;

use crate::bridge::install_bridge;
use crate::cdp::{browser_websocket_url, reload_codex_page, wait_for_codex_target};
use crate::debug_port::{resolve_debug_port, DebugPortMode, PREFERRED_DEBUG_PORT};
use crate::launcher::{ensure_codex_launched_with_debug_port, quit_codex, resolve_codex_app_path};
use crate::logging::DiagnosticLogger;
use crate::ports::PortForwardManager;
use crate::routes::BridgeContext;
use crate::runtime::build_runtime_bundle;
use crate::state_dir::StateDir;

pub struct LaunchContext {
    pub debug_port: u16,
    pub app_path: PathBuf,
    pub state_dir: StateDir,
    pub logger: Arc<DiagnosticLogger>,
    pub port_manager: PortForwardManager,
}

pub struct CodexController {
    ctx: Mutex<Option<LaunchContext>>,
    busy: Mutex<()>,
}

impl CodexController {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            ctx: Mutex::new(None),
            busy: Mutex::new(()),
        })
    }

    pub async fn initial_launch(&self, port_manager: PortForwardManager) -> anyhow::Result<()> {
        let _busy = self.busy.lock().await;
        let state_dir = StateDir::init()?;
        let logger = Arc::new(DiagnosticLogger::new(state_dir.logs_dir.clone()));
        let app_path = resolve_codex_app_path(None)?;
        let resolved = resolve_debug_port(PREFERRED_DEBUG_PORT).await?;
        let debug_port = resolved.port;
        logger.append(
            "launcher.starting",
            serde_json::json!({ "debugPort": debug_port, "preferred": PREFERRED_DEBUG_PORT }),
        )?;
        logger.append(
            "launcher.codex_app_resolved",
            serde_json::json!({ "appPath": app_path }),
        )?;
        logger.append(
            "launcher.debug_port_resolved",
            serde_json::json!({
                "debugPort": debug_port,
                "mode": match resolved.mode {
                    DebugPortMode::Attach => "attach",
                    DebugPortMode::Launch => "launch",
                },
            }),
        )?;
        let attach_only = matches!(resolved.mode, DebugPortMode::Attach);
        ensure_codex_launched_with_debug_port(
            &app_path,
            debug_port,
            attach_only,
            resolved.port_hold,
        )
        .await?;
        logger.append(
            if attach_only {
                "launcher.attached"
            } else {
                "launcher.codex_started"
            },
            serde_json::json!({ "debugPort": debug_port }),
        )?;
        let launch_ctx = LaunchContext {
            debug_port,
            app_path,
            state_dir,
            logger,
            port_manager,
        };
        connect_and_inject(&launch_ctx).await?;
        *self.ctx.lock().await = Some(launch_ctx);
        Ok(())
    }

    pub async fn quit_codex(&self) -> anyhow::Result<()> {
        let _busy = self.busy.lock().await;
        let ctx = self.ctx.lock().await;
        let Some(ctx) = ctx.as_ref() else {
            anyhow::bail!("Codex Helper has not finished launching yet");
        };
        ctx.logger
            .append("tray.quit_codex", serde_json::json!({}))?;
        quit_codex(Duration::from_secs(15)).await
    }

    pub async fn reload_codex(&self) -> anyhow::Result<()> {
        let _busy = self.busy.lock().await;
        let ctx = self.ctx.lock().await;
        let Some(ctx) = ctx.as_ref() else {
            anyhow::bail!("Codex Helper has not finished launching yet");
        };
        ctx.logger
            .append("tray.reload_codex", serde_json::json!({ "debugPort": ctx.debug_port }))?;
        reload_codex_page(ctx.debug_port).await
    }

    pub async fn restart_codex(&self) -> anyhow::Result<()> {
        let _busy = self.busy.lock().await;
        let ctx = {
            let guard = self.ctx.lock().await;
            let Some(ctx) = guard.as_ref() else {
                anyhow::bail!("Codex Helper has not finished launching yet");
            };
            LaunchContext {
                debug_port: ctx.debug_port,
                app_path: ctx.app_path.clone(),
                state_dir: ctx.state_dir.clone(),
                logger: ctx.logger.clone(),
                port_manager: ctx.port_manager.clone(),
            }
        };
        ctx.logger.append(
            "tray.restart_codex",
            serde_json::json!({ "debugPort": ctx.debug_port }),
        )?;
        quit_codex(Duration::from_secs(15)).await?;
        tokio::time::sleep(Duration::from_millis(500)).await;
        ensure_codex_launched_with_debug_port(&ctx.app_path, ctx.debug_port, false, None).await?;
        ctx.logger.append(
            "launcher.codex_restarted",
            serde_json::json!({ "debugPort": ctx.debug_port }),
        )?;
        connect_and_inject(&ctx).await
    }
}

async fn connect_and_inject(ctx: &LaunchContext) -> anyhow::Result<()> {
    let target = wait_for_codex_target(ctx.debug_port).await?;
    let target_id = target.id.clone();
    ctx.logger.append(
        "cdp.target_selected",
        serde_json::json!({
            "id": target.id,
            "title": target.title,
            "url": target.url,
        }),
    )?;
    let websocket_url = browser_websocket_url(ctx.debug_port).await?;
    let runtime_scripts = build_runtime_bundle(&ctx.state_dir, &ctx.logger)?;
    let bridge_ctx = BridgeContext {
        state_dir: ctx.state_dir.clone(),
        logger: ctx.logger.clone(),
        debug_port: ctx.debug_port,
        port_manager: ctx.port_manager.clone(),
    };
    install_bridge(&websocket_url, &target_id, bridge_ctx, runtime_scripts).await?;
    ctx.logger.append("bridge.injected", serde_json::json!({}))?;
    Ok(())
}
