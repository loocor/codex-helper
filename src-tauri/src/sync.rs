use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;
use std::time::{Duration, SystemTime};

use anyhow::Context;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::codex_live::write_secret_file_atomic;
use crate::providers::{self, activate_provider};

const DEFAULT_SSH_PORT: u16 = 22;
const SSH_CONNECT_TIMEOUT_SECS: u64 = 8;
const MASKED_PASSWORD: &str = "********";
const ASKPASS_SCRIPT: &str = "#!/bin/sh\nexec cat \"$(dirname \"$0\")/secret\"\n";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SyncRole {
    Primary,
    Replica,
}

impl Default for SyncRole {
    fn default() -> Self {
        Self::Primary
    }
}

impl SyncRole {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Primary => "primary",
            Self::Replica => "replica",
        }
    }

    fn parse(value: &str) -> anyhow::Result<Self> {
        match value.trim() {
            "primary" => Ok(Self::Primary),
            "replica" => Ok(Self::Replica),
            other => anyhow::bail!("Unknown sync role: {other}"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct SyncPeerStatus {
    pub ok: bool,
    pub at: String,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SyncAuthMethod {
    Identity,
    Password,
}

impl Default for SyncAuthMethod {
    fn default() -> Self {
        Self::Identity
    }
}

impl SyncAuthMethod {
    fn as_str(self) -> &'static str {
        match self {
            Self::Identity => "identity",
            Self::Password => "password",
        }
    }

    fn parse(value: &str) -> anyhow::Result<Self> {
        match value.trim() {
            "identity" => Ok(Self::Identity),
            "password" => Ok(Self::Password),
            other => anyhow::bail!("Unknown auth method: {other}"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", default)]
pub struct SyncPeer {
    pub id: String,
    pub name: String,
    pub host: String,
    pub user: String,
    pub port: u16,
    pub auth_method: SyncAuthMethod,
    pub identity_file: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub password: String,
    pub last_status: Option<SyncPeerStatus>,
}

impl Default for SyncPeer {
    fn default() -> Self {
        Self {
            id: String::new(),
            name: String::new(),
            host: String::new(),
            user: String::new(),
            port: DEFAULT_SSH_PORT,
            auth_method: SyncAuthMethod::Identity,
            identity_file: String::new(),
            password: String::new(),
            last_status: None,
        }
    }
}

struct AskpassGuard {
    dir: PathBuf,
}

impl AskpassGuard {
    fn create(password: &str) -> anyhow::Result<Self> {
        let dir = std::env::temp_dir().join(format!("codex-helper-askpass-{}", random_id()?));
        fs::create_dir(&dir).with_context(|| format!("Failed to create {}", dir.display()))?;
        let guard = Self { dir };
        guard.write_files(password)?;
        Ok(guard)
    }

    fn write_files(&self, password: &str) -> anyhow::Result<()> {
        fs::set_permissions(&self.dir, fs::Permissions::from_mode(0o700))
            .with_context(|| format!("Failed to set permissions on {}", self.dir.display()))?;
        write_secret_file_atomic(&self.dir.join("secret"), format!("{password}\n"))?;
        let script = self.script_path();
        fs::write(&script, ASKPASS_SCRIPT)
            .with_context(|| format!("Failed to write {}", script.display()))?;
        fs::set_permissions(&script, fs::Permissions::from_mode(0o700))
            .with_context(|| format!("Failed to set permissions on {}", script.display()))?;
        Ok(())
    }

    fn script_path(&self) -> PathBuf {
        self.dir.join("askpass")
    }
}

impl Drop for AskpassGuard {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", default)]
pub struct SyncStore {
    pub role: SyncRole,
    pub auto_sync: bool,
    pub peers: Vec<SyncPeer>,
}

impl Default for SyncStore {
    fn default() -> Self {
        Self {
            role: SyncRole::Primary,
            auto_sync: false,
            peers: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct ReplicaWatch {
    last_mtime: Option<SystemTime>,
    last_active_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplicaChange {
    pub active_id: String,
    pub restart_desktop: bool,
}

#[derive(Debug, Clone)]
pub struct SyncTools {
    pub ssh: PathBuf,
    pub rsync: PathBuf,
}

impl Default for SyncTools {
    fn default() -> Self {
        Self {
            ssh: PathBuf::from("ssh"),
            rsync: PathBuf::from("rsync"),
        }
    }
}

pub fn sync_path(state_root: &Path) -> PathBuf {
    state_root.join("sync.json")
}

pub fn read_store(state_root: &Path) -> anyhow::Result<SyncStore> {
    let path = sync_path(state_root);
    if !path.exists() {
        return Ok(SyncStore::default());
    }
    let contents =
        fs::read_to_string(&path).with_context(|| format!("Failed to read {}", path.display()))?;
    serde_json::from_str(&contents).with_context(|| format!("Failed to parse {}", path.display()))
}

pub fn write_store(state_root: &Path, store: &SyncStore) -> anyhow::Result<()> {
    let path = sync_path(state_root);
    let contents = format!("{}\n", serde_json::to_string_pretty(store)?);
    write_secret_file_atomic(&path, contents)
}

pub fn local_computer_name() -> anyhow::Result<String> {
    let output = Command::new("/usr/sbin/scutil")
        .args(["--get", "ComputerName"])
        .output()
        .context("Failed to read the computer name")?;
    if !output.status.success() {
        anyhow::bail!(
            "Failed to read the computer name: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    let name = String::from_utf8(output.stdout).context("Computer name was not UTF-8")?;
    let name = name.trim();
    if name.is_empty() {
        anyhow::bail!("Computer name is empty");
    }
    Ok(name.to_string())
}

pub fn list_response(state_root: &Path) -> Value {
    match list_payload(state_root) {
        Ok(value) => value,
        Err(error) => json!({ "status": "failed", "message": error.to_string() }),
    }
}

fn list_payload(state_root: &Path) -> anyhow::Result<Value> {
    store_list_value(&read_store(state_root)?)
}

fn store_list_value(store: &SyncStore) -> anyhow::Result<Value> {
    Ok(json!({
        "status": "ok",
        "self": {
            "name": local_computer_name()?,
            "role": store.role.as_str(),
            "autoSync": store.auto_sync,
        },
        "peers": store.peers.iter().map(public_peer).collect::<Vec<_>>(),
    }))
}

fn public_peer(peer: &SyncPeer) -> Value {
    json!({
        "id": peer.id,
        "name": peer.name,
        "host": peer.host,
        "user": peer.user,
        "port": peer.port,
        "authMethod": peer.auth_method.as_str(),
        "identityFile": peer.identity_file,
        "password": if peer.password.is_empty() {
            ""
        } else {
            MASKED_PASSWORD
        },
        "lastStatus": peer.last_status,
    })
}

pub fn update_settings(state_root: &Path, payload: &Value) -> anyhow::Result<SyncStore> {
    let mut store = read_store(state_root)?;
    if let Some(role) = payload.get("role").and_then(Value::as_str) {
        store.role = SyncRole::parse(role)?;
    }
    if let Some(auto_sync) = payload.get("autoSync").and_then(Value::as_bool) {
        if auto_sync && store.role != SyncRole::Primary {
            anyhow::bail!("Auto-sync is only available on the primary Mac");
        }
        store.auto_sync = auto_sync;
    }
    if store.role != SyncRole::Primary {
        store.auto_sync = false;
    }
    write_store(state_root, &store)?;
    Ok(store)
}

pub fn upsert_peer(state_root: &Path, payload: &Value) -> anyhow::Result<SyncStore> {
    let mut store = read_store(state_root)?;
    let requested_id = payload
        .get("id")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let existing = requested_id
        .as_ref()
        .and_then(|id| store.peers.iter().find(|item| item.id == *id).cloned());
    let mut peer = peer_from_payload(payload, existing.as_ref())?;
    if let Some(existing) = existing {
        peer.last_status = existing.last_status;
        store.peers.retain(|item| item.id != peer.id);
    }
    store.peers.push(peer);
    write_store(state_root, &store)?;
    Ok(store)
}

pub fn delete_peer(state_root: &Path, payload: &Value) -> anyhow::Result<SyncStore> {
    let id = required_string(payload, "id")?;
    let mut store = read_store(state_root)?;
    let before = store.peers.len();
    store.peers.retain(|peer| peer.id != id);
    if store.peers.len() == before {
        anyhow::bail!("Sync peer not found: {id}");
    }
    write_store(state_root, &store)?;
    Ok(store)
}

pub fn test_peer(state_root: &Path, payload: &Value) -> anyhow::Result<SyncStore> {
    test_peer_with(state_root, payload, &SyncTools::default())
}

pub fn test_peer_with(
    state_root: &Path,
    payload: &Value,
    tools: &SyncTools,
) -> anyhow::Result<SyncStore> {
    let id = required_string(payload, "id")?;
    let mut store = read_store(state_root)?;
    let peer = store
        .peers
        .iter()
        .find(|peer| peer.id == id)
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("Sync peer not found: {id}"))?;
    let result = run_ssh(tools, &peer, &["true"]);
    set_peer_status(
        &mut store,
        &id,
        result
            .as_ref()
            .map(|_| "SSH connection succeeded".to_string())
            .map_err(ToString::to_string),
    );
    write_store(state_root, &store)?;
    result?;
    Ok(store)
}

pub fn push_now(state_root: &Path, payload: &Value) -> anyhow::Result<Value> {
    push_now_with(state_root, payload, &SyncTools::default())
}

pub fn push_now_with(
    state_root: &Path,
    payload: &Value,
    tools: &SyncTools,
) -> anyhow::Result<Value> {
    let store = read_store(state_root)?;
    if store.role != SyncRole::Primary {
        anyhow::bail!("This Mac is a replica. Sync from the primary.");
    }
    if store.peers.is_empty() {
        anyhow::bail!("No sync peers are configured");
    }
    let peer_id = payload
        .get("id")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let ids: Vec<String> = match peer_id {
        Some(id) => {
            if !store.peers.iter().any(|peer| peer.id == id) {
                anyhow::bail!("Sync peer not found: {id}");
            }
            vec![id.to_string()]
        }
        None => store.peers.iter().map(|peer| peer.id.clone()).collect(),
    };
    let mut last = read_store(state_root)?;
    let mut results = Vec::new();
    let mut failed = None;
    for id in ids {
        match push_peer(state_root, tools, &id) {
            Ok(message) => {
                set_peer_status(&mut last, &id, Ok(message.clone()));
                results.push(json!({ "id": id, "status": "ok", "message": message }));
            }
            Err(error) => {
                let message = error.to_string();
                set_peer_status(&mut last, &id, Err(message.clone()));
                results.push(json!({ "id": id, "status": "failed", "message": message }));
                if failed.is_none() {
                    failed = Some(message);
                }
            }
        }
    }
    write_store(state_root, &last)?;
    if let Some(message) = failed {
        anyhow::bail!(message);
    }
    let mut value = store_list_value(&last)?;
    value["results"] = json!(results);
    Ok(value)
}

struct AutoPushJob {
    pending: bool,
    running: bool,
    root: PathBuf,
    tools: SyncTools,
}

/// Coalesced background peer copy. A provider switch schedules this and
/// returns before SSH finishes. A second schedule during a copy pushes again
/// with the latest files instead of starting a parallel copy.
#[derive(Clone)]
pub struct AutoPushQueue {
    inner: Arc<Mutex<AutoPushJob>>,
}

impl AutoPushQueue {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(AutoPushJob {
                pending: false,
                running: false,
                root: PathBuf::new(),
                tools: SyncTools::default(),
            })),
        }
    }

    #[cfg(test)]
    pub fn schedule(&self, state_root: &Path, tools: SyncTools) {
        self.enqueue(state_root, tools);
    }

    fn enqueue(&self, state_root: &Path, tools: SyncTools) {
        let start_worker = {
            let mut job = self.inner.lock().expect("auto push queue poisoned");
            job.pending = true;
            job.root = state_root.to_path_buf();
            job.tools = tools;
            if job.running {
                false
            } else {
                job.running = true;
                true
            }
        };
        if !start_worker {
            return;
        }
        let inner = Arc::clone(&self.inner);
        thread::Builder::new()
            .name("codex-helper-auto-sync".to_string())
            .spawn(move || loop {
                let (root, tools) = {
                    let mut job = inner.lock().expect("auto push queue poisoned");
                    if !job.pending {
                        job.running = false;
                        return;
                    }
                    job.pending = false;
                    (job.root.clone(), job.tools.clone())
                };
                let _ = auto_push_with(&root, &tools);
            })
            .expect("failed to start auto sync");
    }
}

fn auto_sync_enabled(state_root: &Path) -> bool {
    read_store(state_root).ok().is_some_and(|store| {
        store.role == SyncRole::Primary && store.auto_sync && !store.peers.is_empty()
    })
}

fn global_auto_push_queue() -> &'static AutoPushQueue {
    static QUEUE: OnceLock<AutoPushQueue> = OnceLock::new();
    QUEUE.get_or_init(AutoPushQueue::new)
}

pub fn schedule_auto_push(state_root: &Path) {
    if !auto_sync_enabled(state_root) {
        return;
    }
    global_auto_push_queue().enqueue(state_root, SyncTools::default());
}

pub fn auto_push_with(state_root: &Path, tools: &SyncTools) -> Option<Value> {
    let store = read_store(state_root).ok()?;
    if store.role != SyncRole::Primary || !store.auto_sync || store.peers.is_empty() {
        return None;
    }
    Some(match push_now_with(state_root, &json!({}), tools) {
        Ok(value) => value,
        Err(error) => json!({ "status": "failed", "message": error.to_string() }),
    })
}

pub fn replica_tick(
    state_root: &Path,
    watch: &mut ReplicaWatch,
    proxy_url: &str,
) -> anyhow::Result<Option<ReplicaChange>> {
    replica_tick_with(
        state_root,
        watch,
        proxy_url,
        &crate::codex_live::default_codex_home(),
    )
}

fn replica_tick_with(
    state_root: &Path,
    watch: &mut ReplicaWatch,
    proxy_url: &str,
    codex_home: &Path,
) -> anyhow::Result<Option<ReplicaChange>> {
    let sync = read_store(state_root)?;
    if sync.role != SyncRole::Replica {
        return Ok(None);
    }
    let path = providers::providers_path(state_root);
    if !path.exists() {
        return Ok(None);
    }
    let mtime = fs::metadata(&path)
        .and_then(|meta| meta.modified())
        .with_context(|| format!("Failed to read {}", path.display()))?;
    if watch.last_mtime == Some(mtime) {
        return Ok(None);
    }
    let providers = providers::read_store(state_root)?;
    let active_id = providers.active_id.clone();
    let previous = watch.last_active_id.clone();
    activate_provider(state_root, &active_id, proxy_url, codex_home)?;
    let written = fs::metadata(&path).and_then(|meta| meta.modified()).ok();
    watch.last_mtime = written.or(Some(mtime));
    watch.last_active_id = Some(active_id.clone());
    Ok(Some(ReplicaChange {
        restart_desktop: previous
            .as_deref()
            .is_some_and(|previous_id| previous_id != active_id),
        active_id,
    }))
}

fn push_peer(state_root: &Path, tools: &SyncTools, id: &str) -> anyhow::Result<String> {
    let store = read_store(state_root)?;
    let peer = store
        .peers
        .iter()
        .find(|peer| peer.id == id)
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("Sync peer not found: {id}"))?;
    let providers_path = providers::providers_path(state_root);
    if !providers_path.exists() {
        anyhow::bail!("providers.json is missing");
    }
    run_ssh(tools, &peer, &["mkdir", "-p", ".codex-helper/oauth"])?;
    let oauth_dir = state_root.join("oauth");
    if oauth_dir.is_dir() {
        run_rsync(tools, &peer, &oauth_dir, ".codex-helper/oauth/")?;
    }
    run_rsync(
        tools,
        &peer,
        &providers_path,
        ".codex-helper/providers.json",
    )?;
    let active_id = providers::read_store(state_root)?.active_id;
    Ok(format!("Pushed providers and active provider {active_id}"))
}

fn run_ssh(tools: &SyncTools, peer: &SyncPeer, remote: &[&str]) -> anyhow::Result<()> {
    let mut command = Command::new(&tools.ssh);
    command.args(ssh_args(peer)?);
    command.args(remote);
    let _askpass = configure_ssh_process(&mut command, peer)?;
    run_command(&mut command, "ssh")
}

fn run_rsync(
    tools: &SyncTools,
    peer: &SyncPeer,
    source: &Path,
    destination: &str,
) -> anyhow::Result<()> {
    let mut command = Command::new(&tools.rsync);
    command.arg("-a");
    command.arg("-e");
    command.arg(rsync_remote_shell(peer)?);
    command.arg(rsync_source_arg(source));
    command.arg(format!("{}:{destination}", ssh_target(peer)));
    let _askpass = configure_ssh_process(&mut command, peer)?;
    run_command(&mut command, "rsync")
}

fn configure_ssh_process(
    command: &mut Command,
    peer: &SyncPeer,
) -> anyhow::Result<Option<AskpassGuard>> {
    command.stdin(Stdio::null());
    if peer.auth_method != SyncAuthMethod::Password {
        return Ok(None);
    }
    let guard = AskpassGuard::create(ssh_password(peer)?)?;
    command.env("SSH_ASKPASS", guard.script_path());
    command.env("SSH_ASKPASS_REQUIRE", "force");
    command.env("DISPLAY", "dummy:0");
    Ok(Some(guard))
}

fn ssh_password(peer: &SyncPeer) -> anyhow::Result<&str> {
    if peer.password.is_empty() {
        anyhow::bail!("SSH password is required");
    }
    Ok(&peer.password)
}

fn run_command(command: &mut Command, name: &str) -> anyhow::Result<()> {
    let output = command
        .output()
        .with_context(|| format!("Failed to start {name}"))?;
    if output.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let detail = stderr
        .trim()
        .is_empty()
        .then_some(stdout.trim())
        .unwrap_or(stderr.trim());
    if detail.is_empty() {
        anyhow::bail!("{name} failed");
    }
    anyhow::bail!("{name} failed: {detail}")
}

fn ssh_args(peer: &SyncPeer) -> anyhow::Result<Vec<String>> {
    let mut args = ssh_transport_args(peer)?;
    args.push(ssh_target(peer));
    Ok(args)
}

fn ssh_transport_args(peer: &SyncPeer) -> anyhow::Result<Vec<String>> {
    let mut args = match peer.auth_method {
        SyncAuthMethod::Identity => {
            let identity = identity_path(peer)?;
            vec![
                "-i".to_string(),
                identity.display().to_string(),
                "-p".to_string(),
                peer.port.to_string(),
                "-o".to_string(),
                "BatchMode=yes".to_string(),
                "-o".to_string(),
                "IdentitiesOnly=yes".to_string(),
            ]
        }
        SyncAuthMethod::Password => {
            ssh_password(peer)?;
            vec![
                "-p".to_string(),
                peer.port.to_string(),
                "-o".to_string(),
                "PreferredAuthentications=password,keyboard-interactive".to_string(),
                "-o".to_string(),
                "PubkeyAuthentication=no".to_string(),
                "-o".to_string(),
                "NumberOfPasswordPrompts=1".to_string(),
                "-o".to_string(),
                "StrictHostKeyChecking=accept-new".to_string(),
            ]
        }
    };
    args.push("-o".to_string());
    args.push(format!("ConnectTimeout={SSH_CONNECT_TIMEOUT_SECS}"));
    Ok(args)
}

fn rsync_source_arg(source: &Path) -> String {
    let path = source.display().to_string();
    if source.is_dir() && !path.ends_with('/') {
        format!("{path}/")
    } else {
        path
    }
}

fn rsync_remote_shell(peer: &SyncPeer) -> anyhow::Result<String> {
    let mut command = String::from("ssh");
    for arg in ssh_transport_args(peer)? {
        command.push(' ');
        if arg.starts_with('-') {
            command.push_str(&arg);
        } else {
            command.push_str(&shell_single_quote(&arg));
        }
    }
    Ok(command)
}

fn ssh_target(peer: &SyncPeer) -> String {
    format!("{}@{}", peer.user, peer.host)
}

fn identity_path(peer: &SyncPeer) -> anyhow::Result<PathBuf> {
    if peer.identity_file.trim().is_empty() {
        anyhow::bail!("SSH identity file is required");
    }
    let path = expand_user_path(&peer.identity_file)?;
    if !path.is_file() {
        anyhow::bail!("SSH identity file not found: {}", path.display());
    }
    Ok(path)
}

pub fn expand_user_path(value: &str) -> anyhow::Result<PathBuf> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        anyhow::bail!("SSH identity file is required");
    }
    if trimmed == "~" {
        return dirs::home_dir().ok_or_else(|| anyhow::anyhow!("Home directory not found"));
    }
    if let Some(rest) = trimmed.strip_prefix("~/") {
        let home = dirs::home_dir().ok_or_else(|| anyhow::anyhow!("Home directory not found"))?;
        return Ok(home.join(rest));
    }
    Ok(PathBuf::from(trimmed))
}

fn peer_from_payload(payload: &Value, existing: Option<&SyncPeer>) -> anyhow::Result<SyncPeer> {
    let host = required_token(payload, "host")?;
    let user = required_token(payload, "user")?;
    let auth_method = match payload
        .get("authMethod")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        Some(value) => SyncAuthMethod::parse(value)?,
        None => existing
            .map(|peer| peer.auth_method)
            .unwrap_or(SyncAuthMethod::Identity),
    };
    let name = payload
        .get("name")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(host.as_str())
        .to_string();
    let port = match payload.get("port") {
        None => DEFAULT_SSH_PORT,
        Some(value) => value
            .as_u64()
            .and_then(|port| u16::try_from(port).ok())
            .filter(|port| *port >= 1)
            .ok_or_else(|| anyhow::anyhow!("SSH port must be between 1 and 65535"))?,
    };
    let id = payload
        .get("id")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .map(Ok)
        .unwrap_or_else(random_id)?;
    let (identity_file, password) = match auth_method {
        SyncAuthMethod::Identity => {
            let identity_file = required_string(payload, "identityFile")?;
            let _ = expand_user_path(&identity_file)?;
            (identity_file, String::new())
        }
        SyncAuthMethod::Password => {
            let identity_file = payload
                .get("identityFile")
                .and_then(Value::as_str)
                .map(str::trim)
                .unwrap_or("")
                .to_string();
            (identity_file, resolve_password(payload, existing)?)
        }
    };
    Ok(SyncPeer {
        id,
        name,
        host,
        user,
        port,
        auth_method,
        identity_file,
        password,
        last_status: None,
    })
}

fn resolve_password(payload: &Value, existing: Option<&SyncPeer>) -> anyhow::Result<String> {
    let incoming = payload
        .get("password")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim();
    if incoming.is_empty() || incoming == MASKED_PASSWORD {
        if let Some(existing) = existing {
            if !existing.password.is_empty() {
                return Ok(existing.password.clone());
            }
        }
        anyhow::bail!("password is required");
    }
    Ok(incoming.to_string())
}

fn required_string(payload: &Value, key: &str) -> anyhow::Result<String> {
    payload
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .ok_or_else(|| anyhow::anyhow!("{key} is required"))
}

fn required_token(payload: &Value, key: &str) -> anyhow::Result<String> {
    let value = required_string(payload, key)?;
    if value.split_whitespace().nth(1).is_some() {
        anyhow::bail!("{key} must not contain whitespace");
    }
    if value.starts_with('-') {
        anyhow::bail!("{key} is invalid");
    }
    Ok(value)
}

fn set_peer_status(store: &mut SyncStore, id: &str, result: Result<String, String>) {
    let (ok, message) = match result {
        Ok(message) => (true, message),
        Err(message) => (false, message),
    };
    if let Some(peer) = store.peers.iter_mut().find(|peer| peer.id == id) {
        peer.last_status = Some(SyncPeerStatus {
            ok,
            at: chrono::Utc::now().to_rfc3339(),
            message,
        });
    }
}

fn shell_single_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn random_id() -> anyhow::Result<String> {
    let mut bytes = [0u8; 6];
    getrandom::fill(&mut bytes).context("Failed to read random bytes")?;
    Ok(bytes.iter().map(|byte| format!("{byte:02x}")).collect())
}

pub fn replica_poll_interval() -> Duration {
    Duration::from_secs(1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    use tempfile::tempdir;

    fn sample_payload(identity: &Path) -> Value {
        json!({
            "name": "Mini",
            "host": "mini.sgponte",
            "user": "loocor",
            "identityFile": identity.display().to_string(),
        })
    }

    fn write_identity(dir: &Path) -> PathBuf {
        let path = dir.join("id_ed25519");
        fs::write(&path, "test-key\n").unwrap();
        path
    }

    #[test]
    fn default_store_is_primary_without_auto_sync() {
        let store = SyncStore::default();
        assert_eq!(store.role, SyncRole::Primary);
        assert!(!store.auto_sync);
        assert!(store.peers.is_empty());
    }

    #[test]
    fn replica_clears_auto_sync() {
        let dir = tempdir().unwrap();
        write_store(
            dir.path(),
            &SyncStore {
                role: SyncRole::Primary,
                auto_sync: true,
                peers: Vec::new(),
            },
        )
        .unwrap();
        let store = update_settings(dir.path(), &json!({ "role": "replica" })).unwrap();
        assert_eq!(store.role, SyncRole::Replica);
        assert!(!store.auto_sync);
    }

    #[test]
    fn replica_cannot_enable_auto_sync() {
        let dir = tempdir().unwrap();
        write_store(
            dir.path(),
            &SyncStore {
                role: SyncRole::Replica,
                auto_sync: false,
                peers: Vec::new(),
            },
        )
        .unwrap();
        let error = update_settings(dir.path(), &json!({ "autoSync": true })).unwrap_err();
        assert!(error.to_string().contains("primary Mac"));
    }

    #[test]
    fn upsert_peer_requires_identity_file() {
        let dir = tempdir().unwrap();
        let error = upsert_peer(
            dir.path(),
            &json!({
                "host": "mini.sgponte",
                "user": "loocor",
            }),
        )
        .unwrap_err();
        assert!(error.to_string().contains("identityFile"));
    }

    #[test]
    fn upsert_peer_password_mode_skips_identity_file() {
        let dir = tempdir().unwrap();
        let store = upsert_peer(
            dir.path(),
            &json!({
                "name": "Mini",
                "host": "mini.sgponte",
                "user": "loocor",
                "authMethod": "password",
                "password": "s3cret",
            }),
        )
        .unwrap();
        assert_eq!(store.peers[0].auth_method, SyncAuthMethod::Password);
        assert_eq!(store.peers[0].password, "s3cret");
        assert!(store.peers[0].identity_file.is_empty());
    }

    #[test]
    fn upsert_peer_keeps_password_when_masked() {
        let dir = tempdir().unwrap();
        let store = upsert_peer(
            dir.path(),
            &json!({
                "host": "mini.sgponte",
                "user": "loocor",
                "authMethod": "password",
                "password": "s3cret",
            }),
        )
        .unwrap();
        let id = store.peers[0].id.clone();
        let store = upsert_peer(
            dir.path(),
            &json!({
                "id": id,
                "host": "mini.sgponte",
                "user": "loocor",
                "authMethod": "password",
                "password": MASKED_PASSWORD,
            }),
        )
        .unwrap();
        assert_eq!(store.peers[0].password, "s3cret");
    }

    #[test]
    fn public_peer_masks_password() {
        let peer = SyncPeer {
            id: "peer1".to_string(),
            name: "Mini".to_string(),
            host: "mini.sgponte".to_string(),
            user: "loocor".to_string(),
            auth_method: SyncAuthMethod::Password,
            password: "s3cret".to_string(),
            ..SyncPeer::default()
        };
        let value = public_peer(&peer);
        assert_eq!(value["password"], MASKED_PASSWORD);
        assert_eq!(value["authMethod"], "password");
        assert!(!value.to_string().contains("s3cret"));
    }

    #[test]
    fn upsert_peer_rejects_whitespace_host() {
        let dir = tempdir().unwrap();
        let identity = write_identity(dir.path());
        let error = upsert_peer(
            dir.path(),
            &json!({
                "host": "mini host",
                "user": "loocor",
                "identityFile": identity.display().to_string(),
            }),
        )
        .unwrap_err();
        assert!(error.to_string().contains("whitespace"));
    }

    #[test]
    fn expand_user_path_expands_tilde() {
        let home = dirs::home_dir().unwrap();
        assert_eq!(
            expand_user_path("~/.ssh/mac.mini_rsa").unwrap(),
            home.join(".ssh/mac.mini_rsa")
        );
    }

    #[test]
    fn rsync_remote_shell_requires_identity_file() {
        let peer = SyncPeer {
            identity_file: "/tmp/id file".to_string(),
            user: "loocor".to_string(),
            host: "mini.sgponte".to_string(),
            port: 22,
            ..SyncPeer::default()
        };
        let shell = rsync_remote_shell(&peer);
        let error = shell.unwrap_err().to_string();
        assert!(error.contains("SSH identity file not found"));
    }

    #[test]
    fn rsync_source_arg_adds_trailing_slash_for_directories() {
        let dir = tempdir().unwrap();
        let folder = dir.path().join("oauth");
        fs::create_dir(&folder).unwrap();
        assert!(rsync_source_arg(&folder).ends_with('/'));
        let file = dir.path().join("providers.json");
        fs::write(
            &file, "{}
",
        )
        .unwrap();
        assert!(!rsync_source_arg(&file).ends_with('/'));
    }

    #[test]
    fn rsync_remote_shell_password_skips_identity() {
        let peer = SyncPeer {
            auth_method: SyncAuthMethod::Password,
            password: "s3cret".to_string(),
            user: "loocor".to_string(),
            host: "mini.sgponte".to_string(),
            port: 22,
            ..SyncPeer::default()
        };
        let shell = rsync_remote_shell(&peer).unwrap();
        assert!(!shell.contains("-i "));
        assert!(!shell.contains("BatchMode=yes"));
        assert!(shell.contains("PreferredAuthentications=password,keyboard-interactive"));
        assert!(shell.contains("StrictHostKeyChecking=accept-new"));
        assert!(!shell.contains("s3cret"));
    }

    #[test]
    fn password_ssh_args_do_not_use_batch_mode() {
        let peer = SyncPeer {
            auth_method: SyncAuthMethod::Password,
            password: "s3cret".to_string(),
            user: "loocor".to_string(),
            host: "mini.sgponte".to_string(),
            port: 22,
            ..SyncPeer::default()
        };
        let joined = ssh_transport_args(&peer).unwrap().join(" ");
        assert!(!joined.contains("BatchMode=yes"));
        assert!(joined.contains("StrictHostKeyChecking=accept-new"));
        assert!(!joined.contains("s3cret"));
    }

    #[test]
    fn replica_push_is_rejected() {
        let dir = tempdir().unwrap();
        write_store(
            dir.path(),
            &SyncStore {
                role: SyncRole::Replica,
                auto_sync: false,
                peers: vec![SyncPeer {
                    id: "peer1".to_string(),
                    host: "mini.sgponte".to_string(),
                    user: "loocor".to_string(),
                    identity_file: "/tmp/missing".to_string(),
                    ..SyncPeer::default()
                }],
            },
        )
        .unwrap();
        let error = push_now(dir.path(), &json!({})).unwrap_err();
        assert!(error.to_string().contains("replica"));
    }

    #[test]
    fn auto_push_skips_when_disabled() {
        let dir = tempdir().unwrap();
        write_store(dir.path(), &SyncStore::default()).unwrap();
        assert!(auto_push_with(dir.path(), &SyncTools::default()).is_none());
    }

    #[test]
    fn test_peer_uses_injected_ssh() {
        let dir = tempdir().unwrap();
        let identity = write_identity(dir.path());
        let store = upsert_peer(dir.path(), &sample_payload(&identity)).unwrap();
        let id = store.peers[0].id.clone();
        let tools = SyncTools {
            ssh: PathBuf::from("/usr/bin/true"),
            rsync: PathBuf::from("/usr/bin/true"),
        };
        test_peer_with(dir.path(), &json!({ "id": id }), &tools).unwrap();
        let stored = read_store(dir.path()).unwrap();
        assert!(stored.peers[0].last_status.as_ref().unwrap().ok);
    }

    #[test]
    fn test_peer_missing_identity_fails() {
        let dir = tempdir().unwrap();
        write_store(
            dir.path(),
            &SyncStore {
                role: SyncRole::Primary,
                auto_sync: false,
                peers: vec![SyncPeer {
                    id: "peer1".to_string(),
                    host: "mini.sgponte".to_string(),
                    user: "loocor".to_string(),
                    identity_file: dir.path().join("missing").display().to_string(),
                    ..SyncPeer::default()
                }],
            },
        )
        .unwrap();
        let error = test_peer(dir.path(), &json!({ "id": "peer1" })).unwrap_err();
        assert!(error.to_string().contains("not found"));
    }

    #[test]
    fn test_peer_password_mode_uses_injected_ssh() {
        let dir = tempdir().unwrap();
        let store = upsert_peer(
            dir.path(),
            &json!({
                "host": "mini.sgponte",
                "user": "loocor",
                "authMethod": "password",
                "password": "s3cret",
            }),
        )
        .unwrap();
        let id = store.peers[0].id.clone();
        let tools = SyncTools {
            ssh: PathBuf::from("/usr/bin/true"),
            rsync: PathBuf::from("/usr/bin/true"),
        };
        test_peer_with(dir.path(), &json!({ "id": id }), &tools).unwrap();
        let stored = read_store(dir.path()).unwrap();
        assert!(stored.peers[0].last_status.as_ref().unwrap().ok);
    }

    #[test]
    fn test_peer_password_missing_fails() {
        let dir = tempdir().unwrap();
        write_store(
            dir.path(),
            &SyncStore {
                role: SyncRole::Primary,
                auto_sync: false,
                peers: vec![SyncPeer {
                    id: "peer1".to_string(),
                    host: "mini.sgponte".to_string(),
                    user: "loocor".to_string(),
                    auth_method: SyncAuthMethod::Password,
                    ..SyncPeer::default()
                }],
            },
        )
        .unwrap();
        let error = test_peer(dir.path(), &json!({ "id": "peer1" })).unwrap_err();
        assert!(error.to_string().contains("password"));
    }

    #[test]
    fn replica_tick_ignores_primary() {
        let dir = tempdir().unwrap();
        write_store(dir.path(), &SyncStore::default()).unwrap();
        let mut watch = ReplicaWatch::default();
        let change = replica_tick(dir.path(), &mut watch, "http://127.0.0.1:3721/v1").unwrap();
        assert!(change.is_none());
    }

    #[test]
    fn replica_tick_applies_when_active_provider_unchanged() {
        let dir = tempdir().unwrap();
        write_store(
            dir.path(),
            &SyncStore {
                role: SyncRole::Replica,
                auto_sync: false,
                peers: Vec::new(),
            },
        )
        .unwrap();
        providers::write_store(dir.path(), &providers::ProviderStore::default()).unwrap();
        let codex_home = dir.path().join("codex-home");
        fs::create_dir(&codex_home).unwrap();
        let mut watch = ReplicaWatch::default();
        let first = replica_tick_with(
            dir.path(),
            &mut watch,
            "http://127.0.0.1:3721/v1",
            &codex_home,
        )
        .unwrap()
        .expect("first apply");
        assert_eq!(first.active_id, "official");
        assert!(!first.restart_desktop);

        let unchanged = replica_tick_with(
            dir.path(),
            &mut watch,
            "http://127.0.0.1:3721/v1",
            &codex_home,
        )
        .unwrap();
        assert!(unchanged.is_none());

        let providers_path = providers::providers_path(dir.path());
        let contents = fs::read_to_string(&providers_path).unwrap();
        fs::write(&providers_path, contents).unwrap();
        let second = replica_tick_with(
            dir.path(),
            &mut watch,
            "http://127.0.0.1:3721/v1",
            &codex_home,
        )
        .unwrap()
        .expect("same-id content change");
        assert_eq!(second.active_id, "official");
        assert!(!second.restart_desktop);
    }

    #[test]
    fn schedule_auto_push_returns_before_remote_copy_finishes() {
        let dir = tempdir().unwrap();
        let hits = dir.path().join("hits");
        let tool = dir.path().join("slow-tool");
        fs::write(
            &tool,
            format!(
                "#!/bin/sh\nprintf 'x\\n' >> {}\nsleep 0.8\n",
                shell_single_quote(&hits.display().to_string())
            ),
        )
        .unwrap();
        let mut permissions = fs::metadata(&tool).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&tool, permissions).unwrap();
        let store = upsert_peer(
            dir.path(),
            &json!({
                "host": "mini.sgponte",
                "user": "loocor",
                "authMethod": "password",
                "password": "s3cret",
            }),
        )
        .unwrap();
        write_store(
            dir.path(),
            &SyncStore {
                auto_sync: true,
                ..store
            },
        )
        .unwrap();
        fs::write(providers::providers_path(dir.path()), "{}\n").unwrap();

        let started = std::time::Instant::now();
        AutoPushQueue::new().schedule(
            dir.path(),
            SyncTools {
                ssh: tool.clone(),
                rsync: tool,
            },
        );
        let elapsed = started.elapsed();
        assert!(
            elapsed < std::time::Duration::from_millis(250),
            "auto push blocked the caller for {elapsed:?}"
        );

        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(4);
        while std::time::Instant::now() < deadline && !hits.exists() {
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        assert!(hits.exists(), "background push never started");
    }

    #[test]
    fn write_store_sets_secret_permissions() {
        let dir = tempdir().unwrap();
        write_store(dir.path(), &SyncStore::default()).unwrap();
        let mode = fs::metadata(sync_path(dir.path()))
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o600);
    }
}
