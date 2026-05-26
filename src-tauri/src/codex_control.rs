use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use tokio::sync::Mutex;
use tokio::task::JoinHandle;

use crate::bridge::{install_bridge, BridgeCaller};
use crate::cdp::{browser_websocket_url, codex_page_targets, list_targets, reload_codex_page};
use crate::debug_port::{resolve_debug_port, DebugPortMode, PREFERRED_DEBUG_PORT};
use crate::launcher::{ensure_codex_launched_with_debug_port, quit_codex, resolve_codex_app_path};
use crate::logging::DiagnosticLogger;
use crate::ports::PortForwardManager;
use crate::routes::{BridgeContext, RuntimeActivity};
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
    injected_targets: Mutex<HashMap<String, InjectedTarget>>,
    runtime_activity: RuntimeActivity,
    busy: Mutex<()>,
}

struct InjectedTarget {
    target_id: String,
    helper_instance_id: String,
    title: Option<String>,
    url: Option<String>,
    last_ready_at: u128,
    last_seen_at: u128,
    binding_task: JoinHandle<()>,
}

#[derive(Debug, PartialEq, Eq)]
struct InjectionSyncPlan {
    inject: Vec<String>,
    retain: Vec<String>,
    prune: Vec<String>,
}

static NEXT_HELPER_INSTANCE_ID: AtomicU64 = AtomicU64::new(0);

impl CodexController {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            ctx: Mutex::new(None),
            injected_targets: Mutex::new(HashMap::new()),
            runtime_activity: RuntimeActivity::default(),
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
        self.sync_injected_targets(&launch_ctx).await?;
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

    pub async fn open_codex(&self) -> anyhow::Result<()> {
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
            "tray.open_codex",
            serde_json::json!({ "debugPort": ctx.debug_port }),
        )?;
        ensure_codex_launched_with_debug_port(&ctx.app_path, ctx.debug_port, false, None).await?;
        self.sync_injected_targets(&ctx).await
    }

    pub async fn reload_codex(&self) -> anyhow::Result<()> {
        let _busy = self.busy.lock().await;
        let ctx = self.ctx.lock().await;
        let Some(ctx) = ctx.as_ref() else {
            anyhow::bail!("Codex Helper has not finished launching yet");
        };
        ctx.logger.append(
            "tray.reload_codex",
            serde_json::json!({ "debugPort": ctx.debug_port }),
        )?;
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
        self.sync_injected_targets(&ctx).await
    }

    async fn sync_injected_targets(&self, ctx: &LaunchContext) -> anyhow::Result<()> {
        let targets = list_targets(ctx.debug_port).await?;
        let codex_targets = codex_page_targets(&targets);
        let current_ids = codex_targets
            .iter()
            .map(|target| target.id.clone())
            .collect::<Vec<_>>();
        let existing_ids = {
            let mut injected_targets = self.injected_targets.lock().await;
            injected_targets.retain(|_, target| !target.binding_task.is_finished());
            injected_targets.keys().cloned().collect::<Vec<_>>()
        };
        let plan = plan_injection_sync(&current_ids, &existing_ids);
        let mut injection_failures = Vec::new();
        let mut injected_count = 0usize;

        if !plan.inject.is_empty() {
            let websocket_url = browser_websocket_url(ctx.debug_port).await?;
            let runtime_scripts = build_runtime_bundle(&ctx.state_dir, &ctx.logger)?;
            let bridge_ctx = BridgeContext {
                state_dir: ctx.state_dir.clone(),
                logger: ctx.logger.clone(),
                debug_port: ctx.debug_port,
                port_manager: ctx.port_manager.clone(),
                runtime_activity: self.runtime_activity.clone(),
            };
            for target_id in &plan.inject {
                let target = codex_targets
                    .iter()
                    .find(|target| &target.id == target_id)
                    .ok_or_else(|| {
                        anyhow::anyhow!("Codex target disappeared before injection: {target_id}")
                    })?;
                let helper_instance_id = next_helper_instance_id();
                let caller = BridgeCaller::new_for_target(&target.id, &helper_instance_id);
                let binding_task = match install_bridge(
                    &websocket_url,
                    &target.id,
                    caller,
                    bridge_ctx.clone(),
                    runtime_scripts.clone(),
                )
                .await
                {
                    Ok(binding_task) => binding_task,
                    Err(error) => {
                        injection_failures.push(serde_json::json!({
                            "targetId": target.id.clone(),
                            "title": target.title.clone(),
                            "url": target.url.clone(),
                            "error": error.to_string(),
                        }));
                        continue;
                    }
                };
                injected_count += 1;
                let timestamp = unix_time_millis();
                self.injected_targets.lock().await.insert(
                    target.id.clone(),
                    InjectedTarget {
                        target_id: target.id.clone(),
                        helper_instance_id,
                        title: target.title.clone(),
                        url: target.url.clone(),
                        last_ready_at: timestamp,
                        last_seen_at: timestamp,
                        binding_task,
                    },
                );
            }
        }

        {
            let mut injected_targets = self.injected_targets.lock().await;
            for target_id in &plan.retain {
                if let Some(target) = codex_targets.iter().find(|target| &target.id == target_id) {
                    if let Some(injected) = injected_targets.get_mut(target_id) {
                        injected.title = target.title.clone();
                        injected.url = target.url.clone();
                        injected.last_seen_at = unix_time_millis();
                    }
                }
            }
            for target_id in &plan.prune {
                if let Some(injected) = injected_targets.remove(target_id) {
                    injected.binding_task.abort();
                }
            }
        }
        let injected_target_details = self
            .injected_targets
            .lock()
            .await
            .values()
            .map(|target| {
                serde_json::json!({
                    "targetId": target.target_id.clone(),
                    "helperInstanceId": target.helper_instance_id.clone(),
                    "title": target.title.clone(),
                    "url": target.url.clone(),
                    "lastReadyAt": target.last_ready_at,
                    "lastSeenAt": target.last_seen_at,
                })
            })
            .collect::<Vec<_>>();

        ctx.logger.append(
            "injection.targets_synced",
            serde_json::json!({
                "discovered": targets.len(),
                "codexTargets": current_ids.len(),
                "injected": injected_count,
                "retained": plan.retain.len(),
                "pruned": plan.prune.len(),
                "failures": injection_failures,
                "targets": injected_target_details,
            }),
        )?;
        if injected_count == 0 && plan.retain.is_empty() && !injection_failures.is_empty() {
            anyhow::bail!(
                "Codex target injection failed for {} target(s)",
                injection_failures.len()
            );
        }
        if current_ids.is_empty() {
            anyhow::bail!("No injectable Codex page target found");
        }
        Ok(())
    }
}

fn next_helper_instance_id() -> String {
    let id = NEXT_HELPER_INSTANCE_ID.fetch_add(1, Ordering::Relaxed) + 1;
    format!("helper-{id}")
}

fn unix_time_millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default()
}

fn plan_injection_sync(current: &[String], existing: &[String]) -> InjectionSyncPlan {
    let current_set = current.iter().collect::<HashSet<_>>();
    let existing_set = existing.iter().collect::<HashSet<_>>();
    InjectionSyncPlan {
        inject: current
            .iter()
            .filter(|target_id| !existing_set.contains(target_id))
            .cloned()
            .collect(),
        retain: current
            .iter()
            .filter(|target_id| existing_set.contains(target_id))
            .cloned()
            .collect(),
        prune: existing
            .iter()
            .filter(|target_id| !current_set.contains(target_id))
            .cloned()
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn injection_sync_plans_inject_retain_and_prune() {
        let current = vec!["target-a".to_string(), "target-b".to_string()];
        let existing = vec!["target-a".to_string(), "target-old".to_string()];

        let plan = plan_injection_sync(&current, &existing);

        assert_eq!(plan.inject, vec!["target-b"]);
        assert_eq!(plan.retain, vec!["target-a"]);
        assert_eq!(plan.prune, vec!["target-old"]);
    }

    #[test]
    fn injection_sync_plans_prune_when_no_targets_remain() {
        let current = Vec::new();
        let existing = vec!["target-old".to_string()];

        let plan = plan_injection_sync(&current, &existing);

        assert!(plan.inject.is_empty());
        assert!(plan.retain.is_empty());
        assert_eq!(plan.prune, vec!["target-old"]);
    }
}
