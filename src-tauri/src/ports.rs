use std::collections::BTreeMap;
use std::net::TcpListener;
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::process::{Child, Command};
use tokio::time::sleep;

use crate::zed::SshTarget;

const PORT_FORWARD_START_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PortForwardRequest {
    pub host_id: String,
    pub remote_path: String,
    pub remote_port: u16,
    pub local_port: u16,
    pub source: PortForwardSource,
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
    let remote_port = parse_port(
        payload.get("remotePort").unwrap_or(&Value::Null),
        "remotePort",
    )?;
    let local_port = parse_port(
        payload.get("localPort").unwrap_or(&Value::Null),
        "localPort",
    )?;
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
        remote_port,
        local_port,
        source,
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

pub fn tunnel_id(request: &PortForwardRequest) -> String {
    format!(
        "{}:{}:{}:{}",
        request.host_id, request.remote_path, request.remote_port, request.local_port
    )
}

pub fn local_port_available(port: u16) -> bool {
    TcpListener::bind(("127.0.0.1", port)).is_ok()
}

fn ssh_target_arg(target: &SshTarget) -> String {
    let user_prefix = if target.user.trim().is_empty() {
        String::new()
    } else {
        format!("{}@", target.user.trim())
    };
    format!("{user_prefix}{}", target.host)
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
        tunnels.retain(|_, tunnel| matches!(tunnel.child.try_wait(), Ok(None)));
        let items = tunnels
            .iter()
            .map(|(id, tunnel)| {
                json!({
                    "id": id,
                    "status": "active",
                    "hostId": tunnel.request.host_id,
                    "remotePath": tunnel.request.remote_path,
                    "remotePort": tunnel.request.remote_port,
                    "localPort": tunnel.request.local_port,
                    "localUrl": format!("http://127.0.0.1:{}", tunnel.request.local_port),
                    "source": tunnel.request.source,
                })
            })
            .collect::<Vec<_>>();
        json!({ "status": "ok", "ports": items })
    }

    pub async fn start(&self, request: PortForwardRequest, target: SshTarget) -> Value {
        if !local_port_available(request.local_port) {
            return json!({
                "status": "failed",
                "message": format!("Local port {} is already in use", request.local_port),
            });
        }

        let mut command = Command::new(&self.ssh_program);
        command.arg("-N");
        command.arg("-L");
        command.arg(format!(
            "127.0.0.1:{}:127.0.0.1:{}",
            request.local_port, request.remote_port
        ));
        if let Some(port) = target.port {
            command.arg("-p").arg(port.to_string());
        }
        command.arg(ssh_target_arg(&target));
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
        self.tunnels
            .lock()
            .expect("port tunnel registry poisoned")
            .insert(id.clone(), ManagedTunnel { request, child });
        json!({ "status": "ok", "id": id, "localUrl": local_url })
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
            remote_port: 5173,
            local_port: 15173,
            source: PortForwardSource::Manual,
        };

        assert_eq!(
            tunnel_id(&request),
            "remote-ssh-codex-managed:box:/srv/app:5173:15173"
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
            "remotePort": 5173,
            "localPort": 15173,
            "source": "manual"
        }))
        .expect("valid request");

        assert_eq!(request.host_id, "remote-ssh-codex-managed:box");
        assert_eq!(request.remote_path, "/srv/app");
        assert_eq!(request.remote_port, 5173);
        assert_eq!(request.local_port, 15173);
        assert_eq!(request.source, PortForwardSource::Manual);
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

    #[tokio::test]
    async fn start_rejects_tunnel_when_ssh_exits_immediately() {
        let manager = PortForwardManager::with_ssh_program("/bin/false");
        let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).expect("bind free port");
        let local_port = listener.local_addr().expect("local addr").port();
        drop(listener);
        let request = PortForwardRequest {
            host_id: "remote-ssh-codex-managed:box".to_string(),
            remote_path: "/srv/app".to_string(),
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
}
