use std::collections::BTreeMap;
use std::net::TcpListener;
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::process::{Child, Command};
use tokio::time::{sleep, timeout};

use crate::zed::SshTarget;

const PORT_FORWARD_START_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PortForwardRequest {
    pub host_id: String,
    pub remote_path: String,
    #[serde(default)]
    pub thread_id: String,
    pub remote_port: u16,
    pub local_port: u16,
    pub source: PortForwardSource,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PortDiscoveryRequest {
    pub host_id: String,
    pub remote_path: String,
    #[serde(default)]
    pub thread_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteListeningPort {
    pub remote_port: u16,
    pub pid: u32,
    pub command: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PortForwardSource {
    Auto,
    Manual,
}

pub fn parse_port(value: &Value, field: &'static str) -> Result<u16, String> {
    let Some(port) = value.as_u64() else {
        return Err(format!("{field} must be a number"));
    };
    u16::try_from(port)
        .ok()
        .filter(|port| *port >= 1)
        .ok_or_else(|| format!("{field} must be between 1 and 65535"))
}

fn parse_local_port(value: &Value) -> Result<u16, String> {
    if value.as_u64() == Some(0) {
        return Ok(0);
    }
    parse_port(value, "localPort")
}

pub fn request_from_payload(payload: &Value) -> Result<PortForwardRequest, String> {
    let host_id = payload
        .get("hostId")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_string();
    if host_id.is_empty() {
        return Err("Remote host id is required".to_string());
    }
    let remote_path = payload
        .get("remotePath")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_string();
    if !remote_path.starts_with('/') {
        return Err("Remote path is required".to_string());
    }
    let thread_id = payload
        .get("threadId")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_string();
    if thread_id.is_empty() {
        return Err("Thread id is required".to_string());
    }
    let remote_port = parse_port(
        payload.get("remotePort").unwrap_or(&Value::Null),
        "remotePort",
    )?;
    let local_port = parse_local_port(payload.get("localPort").unwrap_or(&Value::Null))?;
    let source = match payload
        .get("source")
        .and_then(Value::as_str)
        .unwrap_or("manual")
    {
        "auto" => PortForwardSource::Auto,
        "manual" => PortForwardSource::Manual,
        _ => return Err("source must be auto or manual".to_string()),
    };
    Ok(PortForwardRequest {
        host_id,
        remote_path,
        thread_id,
        remote_port,
        local_port,
        source,
    })
}

pub fn discovery_request_from_payload(payload: &Value) -> Result<PortDiscoveryRequest, String> {
    let host_id = payload
        .get("hostId")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_string();
    if host_id.is_empty() {
        return Err("Remote host id is required".to_string());
    }
    let remote_path = payload
        .get("remotePath")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_string();
    if !remote_path.starts_with('/') {
        return Err("Remote path is required".to_string());
    }
    let thread_id = payload
        .get("threadId")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_string();
    if thread_id.is_empty() {
        return Err("Thread id is required".to_string());
    }
    Ok(PortDiscoveryRequest {
        host_id,
        remote_path,
        thread_id,
    })
}

#[derive(Clone)]
pub struct PortForwardManager {
    tunnels: Arc<Mutex<BTreeMap<String, ManagedTunnel>>>,
    ssh_program: String,
}

struct ManagedTunnel {
    request: PortForwardRequest,
    child: Child,
}

struct ManagedTunnelSnapshot {
    id: String,
    local_port: u16,
}

pub fn tunnel_id(request: &PortForwardRequest) -> String {
    format!(
        "{}:{}:{}:{}:{}",
        request.host_id,
        request.remote_path,
        request.thread_id,
        request.remote_port,
        request.local_port
    )
}

pub fn local_port_available(port: u16) -> bool {
    TcpListener::bind(("127.0.0.1", port)).is_ok()
}

pub fn requested_local_port_available(port: u16) -> bool {
    port > 0 && local_port_available(port)
}

fn allocate_free_local_port() -> Result<u16, String> {
    TcpListener::bind(("127.0.0.1", 0))
        .and_then(|listener| listener.local_addr())
        .map(|addr| addr.port())
        .map_err(|error| error.to_string())
}

fn ssh_target_arg(target: &SshTarget) -> String {
    let user_prefix = if target.user.trim().is_empty() {
        String::new()
    } else {
        format!("{}@", target.user.trim())
    };
    format!("{user_prefix}{}", target.host)
}

fn same_remote_port_request(left: &PortForwardRequest, right: &PortForwardRequest) -> bool {
    left.host_id == right.host_id
        && left.remote_path == right.remote_path
        && left.thread_id == right.thread_id
        && left.remote_port == right.remote_port
}

fn tunnel_response(snapshot: ManagedTunnelSnapshot) -> Value {
    json!({
        "status": "ok",
        "id": snapshot.id,
        "localPort": snapshot.local_port,
        "localUrl": format!("http://127.0.0.1:{}", snapshot.local_port),
    })
}

fn build_ssh_args(request: &PortForwardRequest, target: &SshTarget) -> Vec<String> {
    let mut args = vec![
        "-N".to_string(),
        "-o".to_string(),
        "ExitOnForwardFailure=yes".to_string(),
    ];
    args.extend(build_ssh_base_options());
    args.extend([
        "-L".to_string(),
        format!(
            "127.0.0.1:{}:127.0.0.1:{}",
            request.local_port, request.remote_port
        ),
    ]);
    if let Some(port) = target.port {
        args.push("-p".to_string());
        args.push(port.to_string());
    }
    args.push("--".to_string());
    args.push(ssh_target_arg(target));
    args
}

fn build_ssh_base_options() -> Vec<String> {
    vec![
        "-o".to_string(),
        "BatchMode=yes".to_string(),
        "-o".to_string(),
        "ControlMaster=no".to_string(),
        "-o".to_string(),
        "ControlPath=none".to_string(),
        "-o".to_string(),
        "ConnectTimeout=10".to_string(),
        "-o".to_string(),
        "ServerAliveInterval=15".to_string(),
        "-o".to_string(),
        "ServerAliveCountMax=4".to_string(),
    ]
}

fn build_ssh_base_args(target: &SshTarget) -> Vec<String> {
    let mut args = build_ssh_base_options();
    if let Some(port) = target.port {
        args.push("-p".to_string());
        args.push(port.to_string());
    }
    args.push("--".to_string());
    args.push(ssh_target_arg(target));
    args
}

fn remote_discovery_script() -> &'static str {
    r#"
set -eu
if ! command -v lsof >/dev/null 2>&1; then
  echo "Remote lsof is required for port discovery" >&2
  exit 127
fi
for pid in $(lsof -nP -iTCP -sTCP:LISTEN -t 2>/dev/null | sort -u); do
  command=$(ps -p "$pid" -o comm= 2>/dev/null | sed 's/^[[:space:]]*//;s/[[:space:]]*$//')
  cwd=$(lsof -a -p "$pid" -d cwd -Fn 2>/dev/null | sed -n 's/^n//p' | head -n 1)
  lsof -nP -a -p "$pid" -iTCP -sTCP:LISTEN -Fn 2>/dev/null | sed -n 's/^n//p' | while IFS= read -r address; do
    printf '%s\t%s\t%s\t%s\n' "$pid" "$command" "$cwd" "$address"
  done
done
"#
}

fn parse_port_from_address(address: &str) -> Option<u16> {
    let raw_port = address.trim().rsplit(':').next()?.trim();
    let port = raw_port.parse::<u16>().ok()?;
    if port == 0 {
        return None;
    }
    Some(port)
}

fn path_inside_workspace(path: &str, workspace: &str) -> bool {
    let path = path.trim_end_matches('/');
    let workspace = workspace.trim_end_matches('/');
    if workspace.is_empty() || workspace == "/" {
        return false;
    }
    path == workspace || path.starts_with(&format!("{workspace}/"))
}

pub fn parse_remote_listening_ports(output: &str, remote_path: &str) -> Vec<RemoteListeningPort> {
    let mut ports = BTreeMap::new();
    for line in output.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let mut fields = line.split('\t');
        let pid = fields.next().and_then(|value| value.parse::<u32>().ok());
        let command = fields.next().unwrap_or("").to_string();
        let cwd = fields.next().unwrap_or("");
        let address = fields.next().unwrap_or("");
        if !path_inside_workspace(cwd, remote_path) {
            continue;
        }
        let Some(pid) = pid else {
            continue;
        };
        let Some(remote_port) = parse_port_from_address(address) else {
            continue;
        };
        if remote_port < 1024 {
            continue;
        }
        ports.entry(remote_port).or_insert(RemoteListeningPort {
            remote_port,
            pid,
            command,
        });
    }
    ports.into_values().collect()
}

pub async fn discover_remote_listening_ports(
    request: &PortDiscoveryRequest,
    target: &SshTarget,
) -> Result<Vec<RemoteListeningPort>, String> {
    let mut command = Command::new("ssh");
    command.args(build_ssh_base_args(target));
    command.arg("sh").arg("-lc").arg(remote_discovery_script());
    command.stdin(Stdio::null());
    command.kill_on_drop(true);
    let output = timeout(Duration::from_secs(5), command.output())
        .await
        .map_err(|_| "Remote port discovery timed out".to_string())?
        .map_err(|error| error.to_string())?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(if stderr.is_empty() {
            format!("Remote port discovery exited with status {}", output.status)
        } else {
            stderr
        });
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    Ok(parse_remote_listening_ports(&stdout, &request.remote_path))
}

impl PortForwardManager {
    pub fn new() -> Self {
        Self::default()
    }

    #[cfg(test)]
    fn with_ssh_program(program: &str) -> Self {
        Self {
            tunnels: Arc::new(Mutex::new(BTreeMap::new())),
            ssh_program: program.to_string(),
        }
    }

    pub async fn list(&self) -> Value {
        let mut tunnels = self.tunnels.lock().expect("port tunnel registry poisoned");
        prune_exited_tunnels(&mut tunnels);
        let items = tunnels
            .iter()
            .map(|(id, tunnel)| {
                json!({
                    "id": id,
                    "status": "active",
                    "hostId": tunnel.request.host_id,
                    "remotePath": tunnel.request.remote_path,
                    "threadId": tunnel.request.thread_id,
                    "remotePort": tunnel.request.remote_port,
                    "localPort": tunnel.request.local_port,
                    "localUrl": format!("http://127.0.0.1:{}", tunnel.request.local_port),
                    "source": tunnel.request.source,
                })
            })
            .collect::<Vec<_>>();
        json!({ "status": "ok", "ports": items })
    }

    pub async fn start(&self, mut request: PortForwardRequest, target: SshTarget) -> Value {
        if let Some(snapshot) = self.reusable_tunnel(&request) {
            return tunnel_response(snapshot);
        }

        request.local_port = if requested_local_port_available(request.local_port) {
            request.local_port
        } else {
            match allocate_free_local_port() {
                Ok(port) => port,
                Err(message) => return json!({ "status": "failed", "message": message }),
            }
        };

        if let Some(snapshot) = self.reusable_tunnel(&request) {
            return tunnel_response(snapshot);
        }

        let mut command = Command::new(&self.ssh_program);
        command.args(build_ssh_args(&request, &target));
        command.stdin(Stdio::null());
        command.stdout(Stdio::null());
        command.stderr(Stdio::null());

        let mut child = match command.spawn() {
            Ok(child) => child,
            Err(error) => return json!({ "status": "failed", "message": error.to_string() }),
        };

        if let Err(message) = wait_for_tunnel_start(&mut child, request.local_port).await {
            let _ = child.kill().await;
            return json!({ "status": "failed", "message": message });
        }

        let id = tunnel_id(&request);
        let local_url = format!("http://127.0.0.1:{}", request.local_port);
        let local_port = request.local_port;
        self.tunnels
            .lock()
            .expect("port tunnel registry poisoned")
            .insert(id.clone(), ManagedTunnel { request, child });
        json!({ "status": "ok", "id": id, "localPort": local_port, "localUrl": local_url })
    }

    pub async fn stop(&self, id: &str) -> Value {
        let tunnel = self
            .tunnels
            .lock()
            .expect("port tunnel registry poisoned")
            .remove(id);
        let Some(mut tunnel) = tunnel else {
            return json!({ "status": "failed", "message": "Port tunnel not found" });
        };
        let _ = tunnel.child.kill().await;
        json!({ "status": "ok", "id": id })
    }

    pub fn stop_all(&self) {
        let tunnels =
            std::mem::take(&mut *self.tunnels.lock().expect("port tunnel registry poisoned"));
        for (_, mut tunnel) in tunnels {
            let _ = tunnel.child.start_kill();
        }
    }

    fn reusable_tunnel(&self, request: &PortForwardRequest) -> Option<ManagedTunnelSnapshot> {
        let mut tunnels = self.tunnels.lock().expect("port tunnel registry poisoned");
        prune_exited_tunnels(&mut tunnels);
        let exact_id = if request.local_port > 0 {
            Some(tunnel_id(request))
        } else {
            None
        };
        tunnels
            .iter()
            .find(|(id, tunnel)| {
                exact_id.as_ref().is_some_and(|exact_id| exact_id == *id)
                    || (request.source == PortForwardSource::Auto
                        && same_remote_port_request(request, &tunnel.request))
            })
            .map(|(id, tunnel)| ManagedTunnelSnapshot {
                id: id.clone(),
                local_port: tunnel.request.local_port,
            })
    }
}

fn prune_exited_tunnels(tunnels: &mut BTreeMap<String, ManagedTunnel>) {
    tunnels.retain(|_, tunnel| matches!(tunnel.child.try_wait(), Ok(None)));
}

impl Default for PortForwardManager {
    fn default() -> Self {
        Self {
            tunnels: Arc::new(Mutex::new(BTreeMap::new())),
            ssh_program: "ssh".to_string(),
        }
    }
}

async fn wait_for_tunnel_start(child: &mut Child, local_port: u16) -> Result<(), String> {
    let deadline = Instant::now() + PORT_FORWARD_START_TIMEOUT;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                return Err(format!(
                    "SSH tunnel exited before forwarding started: {status}"
                ));
            }
            Ok(None) => {
                if !local_port_available(local_port) {
                    return Ok(());
                }
            }
            Err(error) => return Err(error.to_string()),
        }
        if Instant::now() >= deadline {
            return Err("SSH tunnel did not start listening on the local port".to_string());
        }
        sleep(Duration::from_millis(50)).await;
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn parse_port_accepts_valid_port() {
        assert_eq!(parse_port(&json!(5173), "remotePort").unwrap(), 5173);
    }

    #[test]
    fn parse_port_rejects_zero() {
        assert_eq!(
            parse_port(&json!(0), "remotePort").unwrap_err(),
            "remotePort must be between 1 and 65535"
        );
    }

    #[test]
    fn parse_port_rejects_out_of_range() {
        assert_eq!(
            parse_port(&json!(70000), "localPort").unwrap_err(),
            "localPort must be between 1 and 65535"
        );
    }

    #[test]
    fn tunnel_id_includes_context_and_ports() {
        let request = PortForwardRequest {
            host_id: "remote-ssh-codex-managed:box".to_string(),
            remote_path: "/srv/app".to_string(),
            thread_id: "thread-1".to_string(),
            remote_port: 5173,
            local_port: 15173,
            source: PortForwardSource::Manual,
        };

        assert_eq!(
            tunnel_id(&request),
            "remote-ssh-codex-managed:box:/srv/app:thread-1:5173:15173"
        );
    }

    #[test]
    fn local_port_available_reports_bound_ports() {
        let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).expect("bind free port");
        let port = listener.local_addr().expect("local addr").port();

        assert!(!local_port_available(port));
        drop(listener);
        assert!(local_port_available(port));
    }

    #[test]
    fn request_from_payload_accepts_valid_manual_request() {
        let request = request_from_payload(&json!({
            "hostId": "remote-ssh-codex-managed:box",
            "remotePath": "/srv/app",
            "threadId": "thread-1",
            "remotePort": 5173,
            "localPort": 15173,
            "source": "manual"
        }))
        .expect("valid request");

        assert_eq!(request.host_id, "remote-ssh-codex-managed:box");
        assert_eq!(request.remote_path, "/srv/app");
        assert_eq!(request.thread_id, "thread-1");
        assert_eq!(request.remote_port, 5173);
        assert_eq!(request.local_port, 15173);
        assert_eq!(request.source, PortForwardSource::Manual);
    }

    #[test]
    fn request_from_payload_accepts_zero_as_automatic_local_port() {
        let request = request_from_payload(&json!({
            "hostId": "remote-ssh-codex-managed:box",
            "remotePath": "/srv/app",
            "threadId": "thread-1",
            "remotePort": 5173,
            "localPort": 0,
            "source": "auto"
        }))
        .expect("valid request");

        assert_eq!(request.local_port, 0);
    }

    #[test]
    fn request_from_payload_rejects_missing_host_id() {
        assert_eq!(
            request_from_payload(&json!({
                "remotePort": 5173,
                "localPort": 5173,
                "source": "auto"
            }))
            .unwrap_err(),
            "Remote host id is required"
        );
    }

    #[test]
    fn request_from_payload_requires_scoped_remote_path_and_thread() {
        assert_eq!(
            request_from_payload(&json!({
                "hostId": "remote-ssh-codex-managed:box",
                "remotePort": 5173,
                "localPort": 5173,
                "source": "auto"
            }))
            .unwrap_err(),
            "Remote path is required"
        );
        assert_eq!(
            request_from_payload(&json!({
                "hostId": "remote-ssh-codex-managed:box",
                "remotePath": "/srv/app",
                "remotePort": 5173,
                "localPort": 5173,
                "source": "auto"
            }))
            .unwrap_err(),
            "Thread id is required"
        );
    }

    #[test]
    fn discovery_request_from_payload_requires_scoped_remote_path_and_thread() {
        assert_eq!(
            discovery_request_from_payload(&json!({
                "hostId": "remote-ssh-codex-managed:box"
            }))
            .unwrap_err(),
            "Remote path is required"
        );
        assert_eq!(
            discovery_request_from_payload(&json!({
                "hostId": "remote-ssh-codex-managed:box",
                "remotePath": "/srv/app"
            }))
            .unwrap_err(),
            "Thread id is required"
        );
    }

    #[test]
    fn build_ssh_args_separates_options_from_target() {
        let request = PortForwardRequest {
            host_id: "remote-ssh-codex-managed:box".to_string(),
            remote_path: "/srv/app".to_string(),
            thread_id: "thread-1".to_string(),
            remote_port: 5173,
            local_port: 15173,
            source: PortForwardSource::Manual,
        };
        let target = SshTarget {
            user: String::new(),
            host: "-oProxyCommand=bad".to_string(),
            port: None,
        };

        let args = build_ssh_args(&request, &target);

        assert_eq!(args[args.len() - 2], "--");
        assert_eq!(args[args.len() - 1], "-oProxyCommand=bad");
    }

    #[test]
    fn build_ssh_args_fails_fast_and_keeps_tunnel_alive_predictably() {
        let request = PortForwardRequest {
            host_id: "remote-ssh-codex-managed:box".to_string(),
            remote_path: "/srv/app".to_string(),
            thread_id: "thread-1".to_string(),
            remote_port: 5173,
            local_port: 15173,
            source: PortForwardSource::Manual,
        };
        let target = SshTarget {
            user: String::new(),
            host: "box".to_string(),
            port: None,
        };

        let args = build_ssh_args(&request, &target);

        assert!(args.contains(&"ExitOnForwardFailure=yes".to_string()));
        assert!(args.contains(&"BatchMode=yes".to_string()));
        assert!(args.contains(&"ControlMaster=no".to_string()));
        assert!(args.contains(&"ControlPath=none".to_string()));
        assert!(args.contains(&"ServerAliveInterval=15".to_string()));
        assert!(args.contains(&"ServerAliveCountMax=4".to_string()));
    }

    #[test]
    fn parse_remote_listening_ports_includes_custom_workspace_ports() {
        let ports = parse_remote_listening_ports(
            "123\tvite\t/Volumes/External/GitHub/CodexHelper\t127.0.0.1:5173\n456\tpython\t/Volumes/External/GitHub/CodexHelper/api\t*:8000\n789\tpostgres\t/usr/local/var\t127.0.0.1:5432",
            "/Volumes/External/GitHub/CodexHelper",
        );

        assert_eq!(
            ports,
            vec![
                RemoteListeningPort {
                    remote_port: 5173,
                    pid: 123,
                    command: "vite".to_string(),
                },
                RemoteListeningPort {
                    remote_port: 8000,
                    pid: 456,
                    command: "python".to_string(),
                },
            ]
        );
    }

    #[test]
    fn parse_remote_listening_ports_removes_ports_outside_workspace() {
        assert!(parse_remote_listening_ports(
            "789\tpostgres\t/usr/local/var\t127.0.0.1:5432",
            "/Volumes/External/GitHub/CodexHelper",
        )
        .is_empty());
    }

    #[test]
    fn parse_remote_listening_ports_rejects_root_workspace_expansion() {
        assert!(parse_remote_listening_ports(
            "123\tvite\t/Volumes/External/GitHub/CodexHelper\t127.0.0.1:5173",
            "/",
        )
        .is_empty());
    }

    #[tokio::test]
    async fn start_rejects_tunnel_when_ssh_exits_immediately() {
        let manager = PortForwardManager::with_ssh_program("/bin/false");
        let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).expect("bind free port");
        let local_port = listener.local_addr().expect("local addr").port();
        drop(listener);
        let request = PortForwardRequest {
            host_id: "remote-ssh-codex-managed:box".to_string(),
            remote_path: "/srv/app".to_string(),
            thread_id: "thread-1".to_string(),
            remote_port: 5173,
            local_port,
            source: PortForwardSource::Manual,
        };
        let target = SshTarget {
            user: String::new(),
            host: "example.invalid".to_string(),
            port: None,
        };

        let result = manager.start(request, target).await;
        let list = manager.list().await;

        assert_eq!(result["status"], "failed");
        assert_eq!(list["ports"].as_array().expect("ports").len(), 0);
    }

    #[tokio::test]
    async fn auto_start_reuses_existing_remote_port_tunnel() {
        let manager = PortForwardManager::with_ssh_program("/bin/false");
        let existing = PortForwardRequest {
            host_id: "remote-ssh-codex-managed:box".to_string(),
            remote_path: "/srv/app".to_string(),
            thread_id: "thread-1".to_string(),
            remote_port: 5173,
            local_port: 15173,
            source: PortForwardSource::Manual,
        };
        let existing_id = tunnel_id(&existing);
        let child = Command::new("sleep")
            .arg("30")
            .spawn()
            .expect("spawn sleep");
        manager
            .tunnels
            .lock()
            .expect("port tunnel registry")
            .insert(
                existing_id.clone(),
                ManagedTunnel {
                    request: existing,
                    child,
                },
            );
        let request = PortForwardRequest {
            host_id: "remote-ssh-codex-managed:box".to_string(),
            remote_path: "/srv/app".to_string(),
            thread_id: "thread-1".to_string(),
            remote_port: 5173,
            local_port: 0,
            source: PortForwardSource::Auto,
        };
        let target = SshTarget {
            user: String::new(),
            host: "example.invalid".to_string(),
            port: None,
        };

        let result = manager.start(request, target).await;

        assert_eq!(result["status"], "ok");
        assert_eq!(result["id"], existing_id);
        assert_eq!(result["localPort"], 15173);
        manager.stop_all();
    }
}
