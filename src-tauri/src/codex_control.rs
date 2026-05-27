use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use tokio::process::Child;
use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};

use crate::bridge::{install_bridge, restore_injected_runtime, BridgeCaller};
use crate::cdp::{
    browser_websocket_url, close_browser, codex_injectable_page_targets, connect_cdp_websocket,
    find_existing_codex_debug_port, has_codex_cdp_target, list_browser_targets,
    wait_for_codex_targets, wait_for_debug_port_to_close,
};
use crate::debug_port::{debug_port_scan_candidates, resolve_debug_port, PREFERRED_DEBUG_PORT};
use crate::launcher::{ensure_codex_launched_with_debug_port, resolve_codex_app_path};
use crate::logging::DiagnosticLogger;
use crate::ports::PortForwardManager;
use crate::routes::{BridgeContext, RuntimeActivity};
use crate::runtime::build_runtime_bundle;
use crate::state_dir::StateDir;

#[derive(Clone)]
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
    target_watcher: Mutex<Option<JoinHandle<()>>>,
    managed_codex_process: Mutex<Option<Child>>,
    managed_codex_online: Mutex<bool>,
    injection_sync_busy: Mutex<()>,
    runtime_activity: RuntimeActivity,
    busy: Mutex<()>,
}

struct InjectedTarget {
    target_id: String,
    helper_instance_id: String,
    title: Option<String>,
    url: Option<String>,
    script_identifiers: Vec<String>,
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
const TARGET_WATCHER_RECONNECT: Duration = Duration::from_secs(1);
const TARGET_WATCHER_MAX_RECONNECT: Duration = Duration::from_secs(30);
const TARGET_WATCHER_DISCONNECT_PROBE_LIMIT: usize = 3;
const TARGET_DISCOVERY_TIMEOUT: Duration = Duration::from_secs(5);
const TARGET_DISCOVERY_COMMAND_ID: u64 = 1;
const TARGET_EVENT_DEBOUNCE: Duration = Duration::from_millis(200);
const CODEX_RECOVERY_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(5);
type BrowserCdpSocket = WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>;

impl CodexController {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            ctx: Mutex::new(None),
            injected_targets: Mutex::new(HashMap::new()),
            target_watcher: Mutex::new(None),
            managed_codex_process: Mutex::new(None),
            managed_codex_online: Mutex::new(false),
            injection_sync_busy: Mutex::new(()),
            runtime_activity: RuntimeActivity::default(),
            busy: Mutex::new(()),
        })
    }

    pub async fn initial_launch(
        self: &Arc<Self>,
        port_manager: PortForwardManager,
    ) -> anyhow::Result<()> {
        let _busy = self.busy.lock().await;
        let state_dir = StateDir::init()?;
        let logger = Arc::new(DiagnosticLogger::new(state_dir.logs_dir.clone()));
        let app_path = resolve_codex_app_path(None)?;
        logger.append(
            "launcher.starting",
            serde_json::json!({ "preferred": PREFERRED_DEBUG_PORT }),
        )?;
        logger.append(
            "launcher.codex_app_resolved",
            serde_json::json!({ "appPath": app_path }),
        )?;
        if let Some(debug_port) =
            find_existing_codex_debug_port(debug_port_scan_candidates(PREFERRED_DEBUG_PORT)).await
        {
            let launch_ctx = LaunchContext {
                debug_port,
                app_path,
                state_dir,
                logger,
                port_manager,
            };
            return self.attach_to_existing_codex(launch_ctx).await;
        }
        let resolved = resolve_debug_port(PREFERRED_DEBUG_PORT).await?;
        let debug_port = resolved.port;
        let launch_ctx = LaunchContext {
            debug_port,
            app_path,
            state_dir,
            logger,
            port_manager,
        };
        self.launch_new_codex(launch_ctx, resolved.port_hold).await
    }

    async fn attach_to_existing_codex(
        self: &Arc<Self>,
        launch_ctx: LaunchContext,
    ) -> anyhow::Result<()> {
        launch_ctx.logger.append(
            "launcher.debug_port_resolved",
            serde_json::json!({
                "debugPort": launch_ctx.debug_port,
                "mode": "attach",
            }),
        )?;
        wait_for_codex_targets_ready(&launch_ctx, "attach-existing").await?;
        self.sync_injected_targets(&launch_ctx).await?;
        *self.ctx.lock().await = Some(launch_ctx);
        self.start_target_watcher().await;
        Ok(())
    }

    async fn launch_new_codex(
        self: &Arc<Self>,
        launch_ctx: LaunchContext,
        port_hold: Option<std::net::TcpListener>,
    ) -> anyhow::Result<()> {
        let debug_port = launch_ctx.debug_port;
        launch_ctx.logger.append(
            "launcher.debug_port_resolved",
            serde_json::json!({
                "debugPort": debug_port,
                "mode": "launch",
            }),
        )?;
        let child = ensure_codex_launched_with_debug_port(
            &launch_ctx.app_path,
            debug_port,
            false,
            port_hold,
        )
        .await?;
        self.set_managed_codex_process(child).await;
        launch_ctx.logger.append(
            "launcher.codex_started",
            serde_json::json!({ "debugPort": debug_port }),
        )?;
        wait_for_codex_targets_ready(&launch_ctx, "initial-launch").await?;
        self.sync_injected_targets(&launch_ctx).await?;
        *self.ctx.lock().await = Some(launch_ctx);
        self.start_target_watcher().await;
        Ok(())
    }

    pub async fn recover_codex_launch(
        self: &Arc<Self>,
        port_manager: PortForwardManager,
    ) -> anyhow::Result<()> {
        let _busy = self.busy.lock().await;
        let state_dir = StateDir::init()?;
        let logger = Arc::new(DiagnosticLogger::new(state_dir.logs_dir.clone()));
        let app_path = resolve_codex_app_path(None)?;
        logger.append(
            "launcher.recovery_starting",
            serde_json::json!({ "preferred": PREFERRED_DEBUG_PORT }),
        )?;
        self.close_existing_codex_debug_ports(&logger).await?;
        let resolved = resolve_debug_port(PREFERRED_DEBUG_PORT).await?;
        let launch_ctx = LaunchContext {
            debug_port: resolved.port,
            app_path,
            state_dir,
            logger,
            port_manager,
        };
        self.launch_new_codex(launch_ctx, resolved.port_hold).await
    }

    pub async fn prepare_helper_shutdown(&self) -> anyhow::Result<()> {
        let _busy = self.busy.lock().await;
        if let Some(ctx) = self.current_context_if_ready().await {
            self.cleanup_managed_codex(&ctx, "helper-shutdown", true)
                .await?;
        } else {
            self.abort_target_watcher().await;
            self.disconnect_injected_targets(None).await;
        }
        Ok(())
    }

    pub async fn has_connected_codex_instance(&self) -> bool {
        let Some(ctx) = self.current_context_if_ready().await else {
            return false;
        };
        has_codex_cdp_target(ctx.debug_port).await
    }

    async fn sync_injected_targets(&self, ctx: &LaunchContext) -> anyhow::Result<()> {
        let _sync_busy = self.injection_sync_busy.lock().await;
        let targets = list_browser_targets(ctx.debug_port).await?;
        let codex_targets = codex_injectable_page_targets(&targets);
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
        let mut newly_injected_target_ids = Vec::new();
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
                let installed_bridge = match install_bridge(
                    &websocket_url,
                    &target.id,
                    caller,
                    bridge_ctx.clone(),
                    runtime_scripts.clone(),
                )
                .await
                {
                    Ok(installed_bridge) => installed_bridge,
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
                newly_injected_target_ids.push(target.id.clone());
                let timestamp = unix_time_millis();
                self.injected_targets.lock().await.insert(
                    target.id.clone(),
                    InjectedTarget {
                        target_id: target.id.clone(),
                        helper_instance_id,
                        title: target.title.clone(),
                        url: target.url.clone(),
                        script_identifiers: installed_bridge.script_identifiers,
                        last_ready_at: timestamp,
                        last_seen_at: timestamp,
                        binding_task: installed_bridge.binding_task,
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
        if should_log_injection_sync(injected_count, plan.prune.len(), injection_failures.len()) {
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
        }
        if !injection_failures.is_empty() {
            let mut injected_targets = self.injected_targets.lock().await;
            for target_id in newly_injected_target_ids {
                if let Some(injected) = injected_targets.remove(&target_id) {
                    injected.binding_task.abort();
                }
            }
            anyhow::bail!(
                "Codex target injection failed for {} target(s)",
                injection_failures.len()
            );
        }
        if current_ids.is_empty() {
            anyhow::bail!("No injectable Codex page target found");
        }
        *self.managed_codex_online.lock().await = true;
        Ok(())
    }

    async fn current_context_if_ready(&self) -> Option<LaunchContext> {
        self.ctx.lock().await.as_ref().cloned()
    }

    async fn cleanup_managed_codex(
        &self,
        ctx: &LaunchContext,
        reason: &str,
        stop_watcher: bool,
    ) -> anyhow::Result<()> {
        let was_online = {
            let mut online = self.managed_codex_online.lock().await;
            let was_online = *online;
            *online = false;
            was_online
        };
        ctx.port_manager.stop_all();
        let disconnected_targets = self.disconnect_injected_targets(Some(ctx)).await;
        let aborted_watcher = if stop_watcher {
            self.abort_target_watcher().await
        } else {
            false
        };
        if was_online || disconnected_targets > 0 || aborted_watcher {
            ctx.logger.append(
                "codex.cleaned_up",
                serde_json::json!({
                    "reason": reason,
                    "wasOnline": was_online,
                    "disconnectedTargets": disconnected_targets,
                    "abortedWatcher": aborted_watcher,
                }),
            )?;
        }
        Ok(())
    }

    async fn close_existing_codex_debug_ports(
        &self,
        logger: &DiagnosticLogger,
    ) -> anyhow::Result<()> {
        for port in debug_port_scan_candidates(PREFERRED_DEBUG_PORT) {
            if !has_codex_cdp_target(port).await {
                continue;
            }
            logger.append(
                "launcher.recovery_closing_codex",
                serde_json::json!({ "debugPort": port }),
            )?;
            let _ = close_browser(port).await;
            if let Err(error) =
                wait_for_debug_port_to_close(port, CODEX_RECOVERY_SHUTDOWN_TIMEOUT).await
            {
                logger.append(
                    "launcher.recovery_close_failed",
                    serde_json::json!({
                        "debugPort": port,
                        "error": error.to_string(),
                    }),
                )?;
                anyhow::bail!(
                    "Codex debug port {port} did not close during recovery. Quit Codex manually and start Codex Helper again."
                );
            }
        }
        Ok(())
    }

    async fn disconnect_injected_targets(&self, ctx: Option<&LaunchContext>) -> usize {
        let mut injected_targets = self.injected_targets.lock().await;
        let count = injected_targets.len();
        let drained_targets = injected_targets
            .drain()
            .map(|(_, target)| target)
            .collect::<Vec<_>>();
        drop(injected_targets);
        let restore_websocket_url = match ctx {
            Some(ctx) => match browser_websocket_url(ctx.debug_port).await {
                Ok(url) => Some(url),
                Err(error) => {
                    let _ = ctx.logger.append(
                        "injection.restore_failed",
                        serde_json::json!({
                            "error": error.to_string(),
                        }),
                    );
                    None
                }
            },
            None => None,
        };
        for target in drained_targets {
            if let (Some(ctx), Some(websocket_url)) = (ctx, restore_websocket_url.as_deref()) {
                if let Err(error) = restore_injected_runtime(
                    websocket_url,
                    &target.target_id,
                    &target.script_identifiers,
                )
                .await
                {
                    let _ = ctx.logger.append(
                        "injection.restore_failed",
                        serde_json::json!({
                            "targetId": target.target_id,
                            "error": error.to_string(),
                        }),
                    );
                }
            }
            target.binding_task.abort();
        }
        count
    }

    async fn set_managed_codex_process(&self, child: Option<Child>) {
        let mut managed_process = self.managed_codex_process.lock().await;
        if let Some(mut existing_child) = managed_process.take() {
            let _ = existing_child.start_kill();
        }
        *managed_process = child;
    }

    async fn abort_target_watcher(&self) -> bool {
        let mut watcher = self.target_watcher.lock().await;
        let Some(task) = watcher.take() else {
            return false;
        };
        task.abort();
        true
    }

    async fn start_target_watcher(self: &Arc<Self>) {
        let mut watcher = self.target_watcher.lock().await;
        if watcher.as_ref().is_some_and(|task| !task.is_finished()) {
            return;
        }
        let controller = self.clone();
        *watcher = Some(tokio::spawn(async move {
            controller.target_watcher_loop().await;
        }));
    }

    async fn target_watcher_loop(self: Arc<Self>) {
        let mut reconnect_delay = TARGET_WATCHER_RECONNECT;
        let mut disconnect_probe_misses = 0usize;
        loop {
            let ctx = {
                let guard = self.ctx.lock().await;
                guard.as_ref().cloned()
            };
            let Some(ctx) = ctx else {
                tokio::time::sleep(reconnect_delay).await;
                reconnect_delay = next_target_watcher_reconnect_delay(reconnect_delay);
                continue;
            };
            if let Err(error) = self.run_target_watcher_once(ctx.clone()).await {
                let was_online = *self.managed_codex_online.lock().await;
                if was_online {
                    let _ = ctx.logger.append(
                        "target_watcher.failed",
                        serde_json::json!({ "error": error.to_string() }),
                    );
                }
                let target_present = has_codex_cdp_target(ctx.debug_port).await;
                let (next_misses, should_cleanup) =
                    target_watcher_disconnect_probe(disconnect_probe_misses, target_present);
                disconnect_probe_misses = next_misses;
                if should_cleanup {
                    let _ = self
                        .cleanup_managed_codex(&ctx, "codex-disconnected", false)
                        .await;
                }
                tokio::time::sleep(reconnect_delay).await;
                reconnect_delay = next_target_watcher_reconnect_delay(reconnect_delay);
                continue;
            }
            disconnect_probe_misses = 0;
            reconnect_delay = TARGET_WATCHER_RECONNECT;
            tokio::time::sleep(reconnect_delay).await;
        }
    }

    async fn run_target_watcher_once(&self, ctx: LaunchContext) -> anyhow::Result<()> {
        let websocket_url = browser_websocket_url(ctx.debug_port).await?;
        let mut socket = connect_cdp_websocket(&websocket_url).await?;
        futures_util::SinkExt::send(
            &mut socket,
            Message::Text(
                serde_json::json!({
                    "id": TARGET_DISCOVERY_COMMAND_ID,
                    "method": "Target.setDiscoverTargets",
                    "params": { "discover": true, "filter": [{}] },
                })
                .to_string()
                .into(),
            ),
        )
        .await?;

        tokio::time::timeout(TARGET_DISCOVERY_TIMEOUT, async {
            loop {
                let Some(message) = futures_util::StreamExt::next(&mut socket).await else {
                    anyhow::bail!("Target.setDiscoverTargets socket closed before response");
                };
                let message = message?;
                let value = cdp_message_value(message)?;
                if let Some(method) = value.get("method").and_then(serde_json::Value::as_str) {
                    if is_target_discovery_event(method) {
                        continue;
                    }
                    continue;
                }
                if value.get("id").and_then(serde_json::Value::as_u64)
                    == Some(TARGET_DISCOVERY_COMMAND_ID)
                {
                    if let Some(error) = value.get("error") {
                        anyhow::bail!("Target.setDiscoverTargets failed: {error}");
                    }
                    return Ok(());
                }
            }
        })
        .await
        .map_err(|_| {
            anyhow::anyhow!(
                "Target.setDiscoverTargets timed out after {}s",
                TARGET_DISCOVERY_TIMEOUT.as_secs()
            )
        })??;

        ctx.logger
            .append("target_watcher.ready", serde_json::json!({}))?;
        self.sync_injected_targets(&ctx).await?;
        while let Some(message) = futures_util::StreamExt::next(&mut socket).await {
            let value = cdp_message_value(message?)?;
            let Some(method) = value.get("method").and_then(serde_json::Value::as_str) else {
                continue;
            };
            if !is_target_discovery_event(method) {
                continue;
            }
            drain_target_discovery_events(&mut socket, TARGET_EVENT_DEBOUNCE).await?;
            self.sync_injected_targets(&ctx).await?;
        }
        anyhow::bail!("Target discovery socket closed")
    }
}

async fn drain_target_discovery_events(
    socket: &mut BrowserCdpSocket,
    quiet_period: Duration,
) -> anyhow::Result<()> {
    loop {
        match tokio::time::timeout(quiet_period, futures_util::StreamExt::next(socket)).await {
            Ok(Some(message)) => {
                let value = cdp_message_value(message?)?;
                let Some(method) = value.get("method").and_then(serde_json::Value::as_str) else {
                    continue;
                };
                if is_target_discovery_event(method) {
                    continue;
                }
            }
            Ok(None) => anyhow::bail!("Target discovery socket closed"),
            Err(_) => return Ok(()),
        }
    }
}

fn cdp_message_value(message: Message) -> anyhow::Result<serde_json::Value> {
    match message {
        Message::Text(text) => Ok(serde_json::from_str(&text)?),
        Message::Binary(bytes) => Ok(serde_json::from_slice(&bytes)?),
        _ => Ok(serde_json::json!({})),
    }
}

fn is_target_discovery_event(method: &str) -> bool {
    matches!(
        method,
        "Target.targetCreated" | "Target.targetInfoChanged" | "Target.targetDestroyed"
    )
}

fn should_log_injection_sync(
    injected_count: usize,
    pruned_count: usize,
    failure_count: usize,
) -> bool {
    injected_count > 0 || pruned_count > 0 || failure_count > 0
}

fn next_target_watcher_reconnect_delay(current: Duration) -> Duration {
    current
        .checked_mul(2)
        .unwrap_or(TARGET_WATCHER_MAX_RECONNECT)
        .min(TARGET_WATCHER_MAX_RECONNECT)
}

fn target_watcher_disconnect_probe(current_misses: usize, target_present: bool) -> (usize, bool) {
    if target_present {
        return (0, false);
    }
    let misses = current_misses.saturating_add(1);
    (misses, misses >= TARGET_WATCHER_DISCONNECT_PROBE_LIMIT)
}

async fn wait_for_codex_targets_ready(ctx: &LaunchContext, reason: &str) -> anyhow::Result<()> {
    let targets = wait_for_codex_targets(ctx.debug_port, Duration::from_secs(60))
        .await
        .map_err(|error| {
            let _ = ctx.logger.append(
                "launcher.codex_targets_wait_failed",
                serde_json::json!({
                    "debugPort": ctx.debug_port,
                    "reason": reason,
                    "error": error.to_string(),
                }),
            );
            error
        })?;
    ctx.logger.append(
        "launcher.codex_targets_ready",
        serde_json::json!({
            "debugPort": ctx.debug_port,
            "reason": reason,
            "targetIds": targets.iter().map(|target| target.id.clone()).collect::<Vec<_>>(),
        }),
    )?;
    Ok(())
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

    #[test]
    fn injection_sync_logging_only_records_changes_or_failures() {
        assert!(!should_log_injection_sync(0, 0, 0));
        assert!(should_log_injection_sync(1, 0, 0));
        assert!(should_log_injection_sync(0, 1, 0));
        assert!(should_log_injection_sync(0, 0, 1));
    }

    #[test]
    fn target_watcher_reconnect_delay_caps_at_maximum() {
        assert_eq!(
            next_target_watcher_reconnect_delay(Duration::from_secs(1)),
            Duration::from_secs(2)
        );
        assert_eq!(
            next_target_watcher_reconnect_delay(Duration::from_secs(20)),
            TARGET_WATCHER_MAX_RECONNECT
        );
        assert_eq!(
            next_target_watcher_reconnect_delay(Duration::MAX),
            TARGET_WATCHER_MAX_RECONNECT
        );
    }

    #[test]
    fn target_watcher_requires_repeated_misses_before_cleanup() {
        assert_eq!(target_watcher_disconnect_probe(0, false), (1, false));
        assert_eq!(target_watcher_disconnect_probe(1, false), (2, false));
        assert_eq!(target_watcher_disconnect_probe(2, false), (3, true));
    }

    #[test]
    fn target_watcher_probe_resets_when_target_is_present() {
        assert_eq!(target_watcher_disconnect_probe(2, true), (0, false));
    }

    #[tokio::test]
    async fn controller_starts_with_managed_codex_offline() {
        let controller = CodexController::new();

        assert!(!*controller.managed_codex_online.lock().await);
    }

    #[tokio::test]
    async fn controller_without_context_has_no_connected_codex_instance() {
        let controller = CodexController::new();

        assert!(!controller.has_connected_codex_instance().await);
    }
}
