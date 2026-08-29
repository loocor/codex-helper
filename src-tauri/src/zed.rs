use std::env;
use std::fs;
use std::net::Ipv6Addr;
use std::path::{Path, PathBuf};

use serde::Serialize;
use serde_json::{json, Value};

#[derive(Debug)]
pub enum ZedRemoteError {
    Validation(&'static str),
    StateRead(std::io::Error),
    StateParse(serde_json::Error),
}

impl std::fmt::Display for ZedRemoteError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Validation(message) => f.write_str(message),
            Self::StateRead(_) => f.write_str("Cannot read Codex remote connection state"),
            Self::StateParse(_) => f.write_str("Cannot parse Codex remote connection state"),
        }
    }
}

impl std::error::Error for ZedRemoteError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::StateRead(error) => Some(error),
            Self::StateParse(error) => Some(error),
            Self::Validation(_) => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SshTarget {
    pub user: String,
    pub host: String,
    pub port: Option<u16>,
}

fn home_dir() -> Option<PathBuf> {
    env::var_os("HOME")
        .or_else(|| env::var_os("USERPROFILE"))
        .map(PathBuf::from)
}

fn string_value(value: Option<&Value>) -> String {
    match value {
        Some(Value::String(value)) => value.trim().to_string(),
        Some(Value::Number(value)) => value.to_string(),
        _ => String::new(),
    }
}

pub fn split_ssh_authority(value: &str) -> Result<(String, String, Option<u16>), ZedRemoteError> {
    let mut authority = value.trim();
    if authority.is_empty() {
        return Ok((String::new(), String::new(), None));
    }
    let mut user = "";
    if let Some(index) = authority.rfind('@') {
        user = &authority[..index];
        authority = &authority[index + 1..];
    }

    if authority.starts_with('[') {
        if let Some(close_index) = authority.find(']') {
            let host = authority[..=close_index].trim().to_string();
            let suffix = &authority[close_index + 1..];
            let port = if let Some(raw_port) = suffix.strip_prefix(':') {
                parse_port_str(raw_port)?
            } else {
                None
            };
            return Ok((user.trim().to_string(), host, port));
        }
        return Ok((user.trim().to_string(), authority.trim().to_string(), None));
    }

    if authority.matches(':').count() == 1 {
        let (host, raw_port) = authority.rsplit_once(':').unwrap_or((authority, ""));
        if raw_port.chars().all(|ch| ch.is_ascii_digit()) && !raw_port.is_empty() {
            return Ok((
                user.trim().to_string(),
                host.trim().to_string(),
                parse_port_str(raw_port)?,
            ));
        }
    }
    Ok((user.trim().to_string(), authority.trim().to_string(), None))
}

fn parse_port_value(value: Option<&Value>) -> Result<Option<u16>, ZedRemoteError> {
    match value {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(value)) if value.trim().is_empty() => Ok(None),
        Some(Value::String(value)) => parse_port_str(value.trim()),
        Some(Value::Number(value)) => {
            let port = value
                .as_u64()
                .ok_or(ZedRemoteError::Validation("Invalid SSH port"))?;
            u16::try_from(port)
                .ok()
                .filter(|port| *port >= 1)
                .ok_or(ZedRemoteError::Validation("Invalid SSH port"))
                .map(Some)
        }
        _ => Err(ZedRemoteError::Validation("Invalid SSH port")),
    }
}

fn parse_port_str(value: &str) -> Result<Option<u16>, ZedRemoteError> {
    if value.is_empty() {
        return Ok(None);
    }
    if !value.chars().all(|ch| ch.is_ascii_digit()) {
        return Err(ZedRemoteError::Validation("Invalid SSH port"));
    }
    let port: u16 = value
        .parse()
        .map_err(|_| ZedRemoteError::Validation("Invalid SSH port"))?;
    if port == 0 {
        return Err(ZedRemoteError::Validation("Invalid SSH port"));
    }
    Ok(Some(port))
}

pub fn validate_ssh_host(host: &str) -> Result<String, ZedRemoteError> {
    let host = host.trim();
    if host.is_empty() {
        return Err(ZedRemoteError::Validation(
            "Cannot determine remote SSH host for this file",
        ));
    }
    if host
        .chars()
        .any(|ch| ch.is_control() || ch.is_whitespace() || matches!(ch, '/' | '?' | '#' | '@'))
    {
        return Err(ZedRemoteError::Validation("Invalid SSH host"));
    }
    if host.starts_with('[') || host.ends_with(']') {
        if !(host.starts_with('[') && host.ends_with(']')) {
            return Err(ZedRemoteError::Validation("Invalid SSH host"));
        }
        host[1..host.len() - 1]
            .parse::<Ipv6Addr>()
            .map_err(|_| ZedRemoteError::Validation("Invalid SSH host"))?;
        return Ok(host.to_string());
    }
    if host.contains('[') || host.contains(']') {
        return Err(ZedRemoteError::Validation("Invalid SSH host"));
    }
    Ok(host.to_string())
}

pub fn target_from_payload(payload: &Value) -> Result<SshTarget, ZedRemoteError> {
    let ssh = payload.get("ssh").and_then(Value::as_object);
    let raw_host = ssh
        .map(|ssh| {
            string_value(ssh.get("host"))
                .or_else_nonempty(|| string_value(ssh.get("hostname")))
                .or_else_nonempty(|| string_value(ssh.get("hostName")))
        })
        .unwrap_or_default();
    let (authority_user, authority_host, authority_port) = split_ssh_authority(&raw_host)?;
    let user = ssh
        .map(|ssh| {
            string_value(ssh.get("user")).or_else_nonempty(|| string_value(ssh.get("username")))
        })
        .unwrap_or_default()
        .or_else_nonempty(|| authority_user.clone());
    let host = validate_ssh_host(&authority_host)?;
    let port = match ssh.and_then(|ssh| ssh.get("port")) {
        Some(Value::Null) | None => authority_port,
        Some(Value::String(value)) if value.trim().is_empty() => authority_port,
        value => parse_port_value(value)?,
    };
    Ok(SshTarget { user, host, port })
}

pub fn codex_global_state_path() -> PathBuf {
    env::var_os("CODEX_HOME")
        .map(PathBuf::from)
        .or_else(|| home_dir().map(|home| home.join(".codex")))
        .unwrap_or_else(|| PathBuf::from(".codex"))
        .join(".codex-global-state.json")
}

pub fn target_from_managed_remote_connection(
    connection: &serde_json::Map<String, Value>,
) -> Result<SshTarget, ZedRemoteError> {
    let ssh_host = string_value(connection.get("sshHost"))
        .or_else_nonempty(|| string_value(connection.get("hostname")));
    let ssh_alias = string_value(connection.get("sshAlias"))
        .or_else_nonempty(|| string_value(connection.get("alias")));
    let (authority_user, authority_host, authority_port) = split_ssh_authority(&ssh_host)?;
    let host = authority_host.or_else_nonempty(|| ssh_alias.clone());
    let user = string_value(connection.get("sshUser"))
        .or_else_nonempty(|| string_value(connection.get("user")))
        .or_else_nonempty(|| authority_user.clone());
    let port = match connection.get("sshPort") {
        Some(Value::Null) | None => authority_port,
        Some(Value::String(value)) if value.trim().is_empty() => authority_port,
        value => parse_port_value(value)?,
    };
    Ok(SshTarget {
        user,
        host: validate_ssh_host(&host)?,
        port,
    })
}

pub fn resolve_ssh_target_from_global_state(
    state: &Value,
    host_id: &str,
) -> Result<SshTarget, ZedRemoteError> {
    if host_id.is_empty() {
        return Err(ZedRemoteError::Validation("Remote host id is required"));
    }
    let connections = state
        .get("codex-managed-remote-connections")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    for connection in connections {
        let Some(connection) = connection.as_object() else {
            continue;
        };
        if string_value(connection.get("hostId")) != host_id {
            continue;
        }
        return target_from_managed_remote_connection(connection);
    }
    Err(ZedRemoteError::Validation(
        "Cannot resolve remote SSH host for this file",
    ))
}

pub fn resolve_ssh_target_for_host_id(
    host_id: &str,
    state_path: Option<&Path>,
) -> Result<SshTarget, ZedRemoteError> {
    let path = state_path
        .map(Path::to_path_buf)
        .unwrap_or_else(codex_global_state_path);
    let data = fs::read_to_string(path).map_err(ZedRemoteError::StateRead)?;
    let state: Value = serde_json::from_str(&data).map_err(ZedRemoteError::StateParse)?;
    resolve_ssh_target_from_global_state(&state, host_id)
}

pub fn ordered_remote_projects_from_global_state(state: &Value) -> Vec<Value> {
    let projects = state
        .get("remote-projects")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter(|project| project.as_object().is_some())
        .collect::<Vec<_>>();
    let project_order = state
        .get("project-order")
        .and_then(Value::as_array)
        .map(|order| {
            order
                .iter()
                .map(|item| string_value(Some(item)))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let mut ordered = Vec::new();
    for project_id in project_order {
        if let Some(project) = projects
            .iter()
            .find(|project| string_value(project.get("id")) == project_id)
        {
            ordered.push(project.clone());
        }
    }
    let ordered_ids = ordered
        .iter()
        .map(|project| string_value(project.get("id")))
        .collect::<std::collections::HashSet<_>>();
    ordered.extend(
        projects
            .into_iter()
            .filter(|project| !ordered_ids.contains(&string_value(project.get("id")))),
    );
    ordered
}

pub fn fallback_open_request_from_global_state(state: &Value) -> Result<Value, ZedRemoteError> {
    let selected_host_id = string_value(state.get("selected-remote-host-id"));
    let selected_project = ordered_remote_projects_from_global_state(state)
        .into_iter()
        .find(|project| {
            let project_host_id = string_value(project.get("hostId"));
            let remote_path = string_value(project.get("remotePath"));
            (selected_host_id.is_empty() || project_host_id == selected_host_id)
                && remote_path.starts_with('/')
        })
        .ok_or(ZedRemoteError::Validation(
            "Cannot determine remote workspace",
        ))?;
    let host_id =
        selected_host_id.or_else_nonempty(|| string_value(selected_project.get("hostId")));
    if host_id.is_empty() {
        return Err(ZedRemoteError::Validation("Remote host id is required"));
    }
    let target = resolve_ssh_target_from_global_state(state, &host_id)?;
    Ok(json!({
        "hostId": host_id,
        "ssh": { "user": target.user, "host": target.host, "port": target.port },
        "path": string_value(selected_project.get("remotePath")),
    }))
}

pub fn fallback_open_request_response(_payload: &Value) -> Value {
    let path = codex_global_state_path();
    let result = fs::read_to_string(path)
        .map_err(ZedRemoteError::StateRead)
        .and_then(|data| serde_json::from_str::<Value>(&data).map_err(ZedRemoteError::StateParse))
        .and_then(|state| fallback_open_request_from_global_state(&state));
    match result {
        Ok(request) => json!({ "status": "ok", "request": request }),
        Err(error) => json!({ "status": "failed", "message": error.to_string() }),
    }
}

pub fn resolve_ssh_target_response(payload: &Value) -> Value {
    let host_id = string_value(payload.get("hostId"));
    if host_id.is_empty() {
        return json!({"status": "failed", "message": "Remote host id is required"});
    }
    match resolve_ssh_target_for_host_id(&host_id, None) {
        Ok(target) => json!({
            "status": "ok",
            "ssh": { "user": target.user, "host": target.host, "port": target.port },
        }),
        Err(error) => json!({ "status": "failed", "message": error.to_string() }),
    }
}

trait NonEmptyStringExt {
    fn or_else_nonempty<F>(self, fallback: F) -> String
    where
        F: FnOnce() -> String;
}

impl NonEmptyStringExt for String {
    fn or_else_nonempty<F>(self, fallback: F) -> String
    where
        F: FnOnce() -> String,
    {
        if self.is_empty() {
            fallback()
        } else {
            self
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn target_from_payload_splits_codex_managed_authority() {
        let target =
            target_from_payload(&json!({"ssh": {"host": "longnv@192.168.100.31"}})).unwrap();

        assert_eq!(
            target,
            SshTarget {
                user: "longnv".to_string(),
                host: "192.168.100.31".to_string(),
                port: None,
            }
        );
    }

    #[test]
    fn resolve_ssh_target_from_global_state_for_codex_managed_connection() {
        let state = json!({
            "codex-managed-remote-connections": [{
                "hostId": "remote-ssh-codex-managed:remote",
                "displayName": "remote",
                "source": "codex-managed",
                "hostname": "longnv@192.168.100.31",
                "sshPort": null,
            }]
        });

        let target =
            resolve_ssh_target_from_global_state(&state, "remote-ssh-codex-managed:remote")
                .unwrap();

        assert_eq!(
            target,
            SshTarget {
                user: "longnv".to_string(),
                host: "192.168.100.31".to_string(),
                port: None,
            }
        );
    }

    #[test]
    fn fallback_open_request_from_global_state_uses_selected_remote_project() {
        let state = json!({
            "selected-remote-host-id": "remote-ssh-codex-managed:remote",
            "codex-managed-remote-connections": [{
                "hostId": "remote-ssh-codex-managed:remote",
                "hostname": "longnv@192.168.100.31",
                "sshPort": null,
            }],
            "remote-projects": [{
                "id": "032e652b-7956-4e6e-83bd-b29f456c6c3d",
                "hostId": "remote-ssh-codex-managed:remote",
                "remotePath": "/Users/longnv/bin/repo/sealos-skills",
                "label": "sealos-skills",
            }],
            "project-order": ["032e652b-7956-4e6e-83bd-b29f456c6c3d"],
        });

        let request = fallback_open_request_from_global_state(&state).unwrap();

        assert_eq!(
            request,
            json!({
                "hostId": "remote-ssh-codex-managed:remote",
                "ssh": {"user": "longnv", "host": "192.168.100.31", "port": null},
                "path": "/Users/longnv/bin/repo/sealos-skills",
            })
        );
    }

    #[test]
    fn fallback_open_request_from_global_state_prefers_project_order_for_selected_host() {
        let state = json!({
            "selected-remote-host-id": "remote-ssh-codex-managed:remote",
            "codex-managed-remote-connections": [{
                "hostId": "remote-ssh-codex-managed:remote",
                "hostname": "longnv@192.168.100.31",
            }],
            "remote-projects": [
                {"id": "old", "hostId": "remote-ssh-codex-managed:remote", "remotePath": "/Users/longnv/bin/repo/old"},
                {"id": "current", "hostId": "remote-ssh-codex-managed:remote", "remotePath": "/Users/longnv/bin/repo/current"},
                {"id": "other-host", "hostId": "remote-ssh-codex-managed:other", "remotePath": "/srv/other"}
            ],
            "project-order": ["other-host", "current", "old"],
        });

        let request = fallback_open_request_from_global_state(&state).unwrap();

        assert_eq!(request["hostId"], "remote-ssh-codex-managed:remote");
        assert_eq!(request["path"], "/Users/longnv/bin/repo/current");
    }

    #[test]
    fn resolve_ssh_target_response_reports_missing_host_id() {
        let result = resolve_ssh_target_response(&json!({"hostId": ""}));

        assert_eq!(
            result,
            json!({"status": "failed", "message": "Remote host id is required"})
        );
    }
}
