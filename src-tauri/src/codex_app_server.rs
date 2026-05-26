use std::io::{BufRead, BufReader, Read, Write};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::time::Duration;

use serde_json::{json, Value};

use crate::zed::SshTarget;

const GENERATED_TITLE_TIMEOUT_SECS: u64 = 120;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForkedThread {
    pub session_id: String,
    pub warning: Option<String>,
}

#[derive(Debug, Clone)]
pub struct CodexAppServerClient {
    codex_command: String,
    remote_target: Option<SshTarget>,
}

pub trait ThreadForker {
    fn fork_thread_to_workspace(
        &self,
        thread_id: &str,
        target_cwd: &str,
        name: &str,
    ) -> anyhow::Result<ForkedThread>;

    fn fork_rollout_to_workspace(
        &self,
        rollout_path: &Path,
        target_cwd: &str,
        name: &str,
    ) -> anyhow::Result<ForkedThread>;
}

impl Default for CodexAppServerClient {
    fn default() -> Self {
        Self::new("codex")
    }
}

impl CodexAppServerClient {
    pub fn new(command: impl Into<String>) -> Self {
        Self {
            codex_command: command.into(),
            remote_target: None,
        }
    }

    pub fn remote(target: SshTarget) -> Self {
        Self {
            codex_command: "codex".to_string(),
            remote_target: Some(target),
        }
    }

    fn app_server_command(&self) -> Command {
        if let Some(target) = &self.remote_target {
            let mut command = Command::new("ssh");
            if let Some(port) = target.port {
                command.arg("-p").arg(port.to_string());
            }
            command
                .arg(ssh_target_arg(target))
                .arg(remote_app_server_command(&self.codex_command));
            return command;
        }
        let mut command = Command::new(&self.codex_command);
        command.arg("app-server").arg("--listen").arg("stdio://");
        command
    }

    fn run_requests(&self, requests: &[Value]) -> anyhow::Result<String> {
        let mut child = self
            .app_server_command()
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| anyhow::anyhow!("Codex app-server stdout is unavailable"))?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| anyhow::anyhow!("Codex app-server stderr is unavailable"))?;
        let stderr_thread = std::thread::spawn(move || {
            let mut output = String::new();
            let mut reader = BufReader::new(stderr);
            let _ = reader.read_to_string(&mut output);
            output
        });
        let (line_tx, line_rx) = mpsc::channel();
        let stdout_thread = std::thread::spawn(move || {
            let reader = BufReader::new(stdout);
            for line in reader.lines().map_while(Result::ok) {
                if line_tx.send(line).is_err() {
                    break;
                }
            }
        });

        let mut output = String::new();
        let mut stdin = child
            .stdin
            .take()
            .ok_or_else(|| anyhow::anyhow!("Codex app-server stdin is unavailable"))?;
        let mut request_result = Ok(());
        for request in requests {
            if let Err(error) = send_json_line(&mut stdin, request) {
                request_result = Err(error);
                break;
            }
            if request.get("method").and_then(Value::as_str) == Some("initialize") {
                match recv_json_rpc_response_by_id(&line_rx, &mut output, 1)
                    .and_then(|response| json_rpc_response_error(&response))
                {
                    Ok(()) => {}
                    Err(error) => {
                        request_result = Err(error);
                        break;
                    }
                }
                if let Err(error) = send_json_line(&mut stdin, &build_initialized_notification()) {
                    request_result = Err(error);
                    break;
                }
                continue;
            }
            if let Some(id) = request.get("id").and_then(Value::as_i64) {
                match recv_json_rpc_response_by_id(&line_rx, &mut output, id)
                    .and_then(|response| json_rpc_response_error(&response))
                {
                    Ok(()) => {}
                    Err(error) => {
                        request_result = Err(error);
                        break;
                    }
                }
            }
        }
        drop(stdin);
        if request_result.is_err() {
            let _ = child.kill();
        }
        let status = child.wait()?;
        let _ = stdout_thread.join();
        let stderr = stderr_thread.join().unwrap_or_default();
        if let Err(error) = request_result {
            let stderr = stderr.trim().to_string();
            if !status.success() && !stderr.is_empty() {
                anyhow::bail!("{stderr}");
            }
            return Err(error);
        }
        if !status.success() {
            let stderr = stderr.trim().to_string();
            anyhow::bail!(
                "{}",
                if stderr.is_empty() {
                    format!("Codex app-server failed with status {status}")
                } else {
                    stderr
                }
            );
        }
        Ok(output)
    }

    fn remote_rollout_path(&self, rollout_path: &Path) -> anyhow::Result<String> {
        let target = self
            .remote_target
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Remote target is required"))?;
        let filename = format!(
            "{}.jsonl",
            sanitize_remote_filename(
                rollout_path
                    .file_stem()
                    .and_then(|value| value.to_str())
                    .unwrap_or("session")
            )
        );
        let script = format!(
            "remote_dir=\"$HOME/.codex-helper/fork-imports\"; mkdir -p \"$remote_dir\"; remote_file=$(mktemp \"$remote_dir/{filename}.XXXXXX\"); cat > \"$remote_file\"; printf '%s\\n' \"$remote_file\""
        );
        let mut command = Command::new("ssh");
        if let Some(port) = target.port {
            command.arg("-p").arg(port.to_string());
        }
        let remote_command = format!("sh -lc {}", shell_quote(&script));
        let mut child = command
            .arg(ssh_target_arg(target))
            .arg(remote_command)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        {
            let stdin = child
                .stdin
                .as_mut()
                .ok_or_else(|| anyhow::anyhow!("SSH stdin is unavailable"))?;
            let mut source = std::fs::File::open(rollout_path)?;
            std::io::copy(&mut source, stdin)?;
        }
        let output = child.wait_with_output()?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            anyhow::bail!(
                "{}",
                if stderr.is_empty() {
                    format!("Remote rollout upload failed with status {}", output.status)
                } else {
                    stderr
                }
            );
        }
        let remote_path = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if remote_path.is_empty() {
            anyhow::bail!("Remote rollout upload did not return a path");
        }
        Ok(remote_path)
    }

    fn remove_remote_file(&self, remote_path: &str) -> anyhow::Result<()> {
        let target = self
            .remote_target
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Remote target is required"))?;
        let script = format!("rm -f -- {}", shell_quote(remote_path));
        let mut command = Command::new("ssh");
        if let Some(port) = target.port {
            command.arg("-p").arg(port.to_string());
        }
        let output = command
            .arg(ssh_target_arg(target))
            .arg(format!("sh -lc {}", shell_quote(&script)))
            .output()?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            anyhow::bail!(
                "{}",
                if stderr.is_empty() {
                    format!(
                        "Remote rollout cleanup failed with status {}",
                        output.status
                    )
                } else {
                    stderr
                }
            );
        }
        Ok(())
    }

    fn rename_thread_best_effort(&self, thread_id: &str, name: &str) -> Option<String> {
        match self.set_thread_name(thread_id, name) {
            Ok(()) => None,
            Err(error) => Some(format!("Conversation forked, but rename failed: {error}")),
        }
    }

    pub fn set_thread_name(&self, thread_id: &str, name: &str) -> anyhow::Result<()> {
        let thread_id = normalize_thread_id(thread_id);
        if thread_id.is_empty() {
            anyhow::bail!("Thread id is empty");
        }
        let name = name.trim();
        if name.is_empty() {
            anyhow::bail!("Thread name is empty");
        }
        let output = self.run_requests(&[
            build_initialize_request(1),
            build_thread_name_request(2, &thread_id, name),
        ])?;
        json_rpc_response_by_id(&output, 2).map(|_| ())
    }

    pub fn generate_thread_name(
        &self,
        thread_id: &str,
        min_chars: u8,
        max_chars: u8,
    ) -> anyhow::Result<String> {
        let thread_id = normalize_thread_id(thread_id);
        if thread_id.is_empty() {
            anyhow::bail!("Thread id is empty");
        }
        let transcript = self.thread_transcript(&thread_id)?;
        self.generate_name_from_transcript(&transcript, min_chars, max_chars)
    }

    fn thread_transcript(&self, thread_id: &str) -> anyhow::Result<String> {
        let output = self.run_requests(&[
            build_initialize_request(1),
            build_thread_read_request(2, thread_id),
        ])?;
        let response = json_rpc_response_by_id(&output, 2)?;
        thread_transcript_from_read_response(&response)
    }

    fn generate_name_from_transcript(
        &self,
        transcript: &str,
        min_chars: u8,
        max_chars: u8,
    ) -> anyhow::Result<String> {
        let prompt = title_generation_prompt(transcript, min_chars, max_chars);
        let mut child = self
            .app_server_command()
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| anyhow::anyhow!("Codex app-server stdout is unavailable"))?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| anyhow::anyhow!("Codex app-server stderr is unavailable"))?;
        let stderr_thread = std::thread::spawn(move || {
            let mut output = String::new();
            let mut reader = BufReader::new(stderr);
            let _ = reader.read_to_string(&mut output);
            output
        });
        let (line_tx, line_rx) = mpsc::channel();
        let stdout_thread = std::thread::spawn(move || {
            let reader = BufReader::new(stdout);
            for line in reader.lines().map_while(Result::ok) {
                if line_tx.send(line).is_err() {
                    break;
                }
            }
        });

        let mut stdin = child
            .stdin
            .take()
            .ok_or_else(|| anyhow::anyhow!("Codex app-server stdin is unavailable"))?;
        let result = (|| -> anyhow::Result<String> {
            let mut output = String::new();
            send_json_line(&mut stdin, &build_initialize_request(1))?;
            recv_json_rpc_response_by_id(&line_rx, &mut output, 1)
                .and_then(|response| json_rpc_response_error(&response))?;
            send_json_line(&mut stdin, &build_initialized_notification())?;
            send_json_line(&mut stdin, &build_title_generation_thread_request(2))?;
            let thread_response = recv_json_rpc_response_by_id(&line_rx, &mut output, 2)?;
            json_rpc_response_error(&thread_response)?;
            let generation_thread_id = thread_response
                .get("result")
                .and_then(|result| result.get("thread"))
                .and_then(|thread| thread.get("id"))
                .and_then(Value::as_str)
                .filter(|value| !value.trim().is_empty())
                .ok_or_else(|| anyhow::anyhow!("Title generation thread id is missing"))?;
            send_json_line(
                &mut stdin,
                &build_title_generation_turn_request(3, generation_thread_id, &prompt),
            )?;
            let turn_response = recv_json_rpc_response_by_id(&line_rx, &mut output, 3)?;
            json_rpc_response_error(&turn_response)?;
            recv_generated_title(&line_rx)
        })();
        drop(stdin);
        if result.is_err() {
            let _ = child.kill();
        }
        let status = child.wait()?;
        let _ = stdout_thread.join();
        let stderr = stderr_thread.join().unwrap_or_default();
        match result {
            Ok(name) => {
                if !status.success() {
                    let stderr = stderr.trim().to_string();
                    anyhow::bail!(
                        "{}",
                        if stderr.is_empty() {
                            format!("Codex title generation failed with status {status}")
                        } else {
                            stderr
                        }
                    );
                }
                Ok(name)
            }
            Err(error) => {
                let stderr = stderr.trim().to_string();
                if !status.success() && !stderr.is_empty() {
                    anyhow::bail!("{stderr}");
                }
                Err(error)
            }
        }
    }
}

impl ThreadForker for CodexAppServerClient {
    fn fork_thread_to_workspace(
        &self,
        thread_id: &str,
        target_cwd: &str,
        name: &str,
    ) -> anyhow::Result<ForkedThread> {
        let thread_id = normalize_thread_id(thread_id);
        let target_cwd = target_cwd.trim();
        if target_cwd.is_empty() {
            anyhow::bail!("Target project path is empty");
        }
        let name = name.trim();
        if name.is_empty() {
            anyhow::bail!("Thread name is empty");
        }
        let fork_request = build_thread_fork_request(2, &thread_id, target_cwd);
        let output = self.run_requests(&[build_initialize_request(1), fork_request])?;
        let fork_response = json_rpc_response_by_id(&output, 2)?;
        let forked_thread_id = forked_thread_id_from_response(&fork_response)?;
        let warning = self.rename_thread_best_effort(&forked_thread_id, name);
        Ok(ForkedThread {
            session_id: forked_thread_id,
            warning,
        })
    }

    fn fork_rollout_to_workspace(
        &self,
        rollout_path: &Path,
        target_cwd: &str,
        name: &str,
    ) -> anyhow::Result<ForkedThread> {
        let target_cwd = target_cwd.trim();
        if target_cwd.is_empty() {
            anyhow::bail!("Target project path is empty");
        }
        let name = name.trim();
        if name.is_empty() {
            anyhow::bail!("Thread name is empty");
        }
        let uploaded_remote_path = if self.remote_target.is_some() {
            Some(self.remote_rollout_path(rollout_path)?)
        } else {
            None
        };
        let fork_path = uploaded_remote_path
            .as_deref()
            .map(str::to_string)
            .unwrap_or_else(|| rollout_path.to_string_lossy().into_owned());
        let fork_request = build_thread_fork_from_path_request(2, &fork_path, target_cwd);
        let fork_result = (|| -> anyhow::Result<String> {
            let output = self.run_requests(&[build_initialize_request(1), fork_request])?;
            let fork_response = json_rpc_response_by_id(&output, 2)?;
            forked_thread_id_from_response(&fork_response)
        })();
        let cleanup_warning = uploaded_remote_path.as_deref().and_then(|path| {
            self.remove_remote_file(path)
                .err()
                .map(|error| format!("Remote rollout cleanup failed: {error}"))
        });
        let forked_thread_id = fork_result?;
        let warning = combine_warnings([
            self.rename_thread_best_effort(&forked_thread_id, name),
            cleanup_warning,
        ]);
        Ok(ForkedThread {
            session_id: forked_thread_id,
            warning,
        })
    }
}

fn combine_warnings(warnings: impl IntoIterator<Item = Option<String>>) -> Option<String> {
    let warnings = warnings
        .into_iter()
        .flatten()
        .filter(|warning| !warning.trim().is_empty())
        .collect::<Vec<_>>();
    if warnings.is_empty() {
        None
    } else {
        Some(warnings.join("; "))
    }
}

fn ssh_target_arg(target: &SshTarget) -> String {
    let user_prefix = if target.user.trim().is_empty() {
        String::new()
    } else {
        format!("{}@", target.user.trim())
    };
    format!("{user_prefix}{}", target.host)
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn remote_app_server_command(codex_command: &str) -> String {
    let app_server_command = format!(
        "{} app-server --listen stdio://",
        shell_quote(codex_command)
    );
    let quoted_app_server_command = shell_quote(&app_server_command);
    format!(
        "if [ -n \"$SHELL\" ] && [ -x \"$SHELL\" ]; then exec \"$SHELL\" -lc {quoted_app_server_command}; elif command -v zsh >/dev/null 2>&1; then exec zsh -lc {quoted_app_server_command}; elif command -v bash >/dev/null 2>&1; then exec bash -lc {quoted_app_server_command}; else exec sh -lc {quoted_app_server_command}; fi"
    )
}

fn sanitize_remote_filename(value: &str) -> String {
    let sanitized = value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.') {
                ch
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .to_string();
    if sanitized.is_empty() {
        "session".to_string()
    } else {
        sanitized
    }
}

pub fn normalize_thread_id(thread_id: &str) -> String {
    thread_id
        .trim()
        .strip_prefix("local:")
        .or_else(|| thread_id.trim().strip_prefix("remote:"))
        .unwrap_or(thread_id.trim())
        .to_string()
}

pub fn build_initialize_request(id: i64) -> Value {
    json!({
        "id": id,
        "method": "initialize",
        "params": {
            "capabilities": {
                "experimentalApi": true,
                "optOutNotificationMethods": [
                    "thread/started",
                    "thread/status/changed",
                    "thread/name/updated"
                ]
            },
            "clientInfo": {
                "name": "codex-helper",
                "version": env!("CARGO_PKG_VERSION")
            }
        }
    })
}

pub fn build_initialized_notification() -> Value {
    json!({
        "method": "initialized"
    })
}

pub fn build_thread_fork_request(id: i64, thread_id: &str, cwd: &str) -> Value {
    json!({
        "id": id,
        "method": "thread/fork",
        "params": {
            "threadId": thread_id,
            "cwd": cwd,
            "ephemeral": false
        }
    })
}

pub fn build_thread_fork_from_path_request(id: i64, path: &str, cwd: &str) -> Value {
    json!({
        "id": id,
        "method": "thread/fork",
        "params": {
            "threadId": "",
            "path": path,
            "cwd": cwd,
            "ephemeral": false
        }
    })
}

pub fn build_thread_name_request(id: i64, thread_id: &str, name: &str) -> Value {
    json!({
        "id": id,
        "method": "thread/name/set",
        "params": {
            "threadId": thread_id,
            "name": name
        }
    })
}

pub fn build_thread_read_request(id: i64, thread_id: &str) -> Value {
    json!({
        "id": id,
        "method": "thread/read",
        "params": {
            "threadId": thread_id,
            "includeTurns": true
        }
    })
}

fn build_title_generation_thread_request(id: i64) -> Value {
    json!({
        "id": id,
        "method": "thread/start",
        "params": {
            "cwd": "/tmp",
            "ephemeral": true,
            "approvalPolicy": "never",
            "sandbox": "read-only",
            "baseInstructions": "You generate concise, friendly, accurate chat titles. Respond with only the title."
        }
    })
}

fn build_title_generation_turn_request(id: i64, thread_id: &str, prompt: &str) -> Value {
    json!({
        "id": id,
        "method": "turn/start",
        "params": {
            "threadId": thread_id,
            "input": [{
                "type": "text",
                "text": prompt,
                "text_elements": []
            }],
            "approvalPolicy": "never",
            "sandboxPolicy": {
                "type": "readOnly",
                "networkAccess": false
            }
        }
    })
}

fn title_generation_prompt(transcript: &str, min_chars: u8, max_chars: u8) -> String {
    let transcript = compact_title_transcript(transcript);
    format!(
        "Generate one friendly, accurate title for this Codex chat.\n\
Rules:\n\
- Output only the title, with no quotes or punctuation wrapper.\n\
- Prefer Chinese when the transcript is mainly Chinese; use English when it is mainly English.\n\
- Target {min_chars}-{max_chars} Chinese characters, or no more than 5 English words.\n\
- Avoid generic prompt prefixes such as \"Check the\", \"Help me\", or \"Please\".\n\
- Capture the actual topic, project, product, or task.\n\n\
Transcript:\n{transcript}"
    )
}

fn compact_title_transcript(transcript: &str) -> String {
    let cleaned = transcript
        .replace(['\r', '\t'], " ")
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join("\n");
    let chars = cleaned.chars().collect::<Vec<_>>();
    if chars.len() <= 6000 {
        return cleaned;
    }
    let head = chars.iter().take(3000).collect::<String>();
    let tail = chars
        .iter()
        .skip(chars.len().saturating_sub(3000))
        .collect::<String>();
    format!("{head}\n...\n{tail}")
}

fn thread_transcript_from_read_response(response: &Value) -> anyhow::Result<String> {
    let turns = response
        .get("result")
        .and_then(|result| result.get("thread"))
        .and_then(|thread| thread.get("turns"))
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow::anyhow!("thread/read response missing thread turns"))?;
    let mut messages = Vec::new();
    for turn in turns {
        let Some(items) = turn.get("items").and_then(Value::as_array) else {
            continue;
        };
        for item in items {
            match item.get("type").and_then(Value::as_str) {
                Some("userMessage") => {
                    let text = user_message_text(item);
                    if !text.trim().is_empty() {
                        messages.push(format!("User: {}", collapse_prompt_text(&text)));
                    }
                }
                Some("agentMessage") => {
                    let text = item.get("text").and_then(Value::as_str).unwrap_or_default();
                    if !text.trim().is_empty() {
                        messages.push(format!("Assistant: {}", collapse_prompt_text(text)));
                    }
                }
                _ => {}
            }
        }
    }
    let transcript = messages.join("\n");
    if transcript.trim().is_empty() {
        anyhow::bail!("Thread transcript is empty");
    }
    Ok(transcript)
}

fn user_message_text(item: &Value) -> String {
    item.get("content")
        .and_then(Value::as_array)
        .map(|content| {
            content
                .iter()
                .filter(|part| part.get("type").and_then(Value::as_str) == Some("text"))
                .filter_map(|part| part.get("text").and_then(Value::as_str))
                .collect::<Vec<_>>()
                .join("\n")
        })
        .unwrap_or_default()
}

fn collapse_prompt_text(value: &str) -> String {
    value
        .replace(['\r', '\t'], " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
pub fn request_batch(requests: &[Value]) -> anyhow::Result<String> {
    let mut input = String::new();
    for request in requests {
        input.push_str(&serde_json::to_string(request)?);
        input.push('\n');
    }
    Ok(input)
}

fn send_json_line(stdin: &mut impl Write, value: &Value) -> anyhow::Result<()> {
    stdin.write_all(serde_json::to_string(value)?.as_bytes())?;
    stdin.write_all(b"\n")?;
    stdin.flush()?;
    Ok(())
}

pub fn forked_thread_id_from_response(response: &Value) -> anyhow::Result<String> {
    response
        .get("result")
        .and_then(|result| result.get("thread"))
        .and_then(|thread| thread.get("sessionId"))
        .and_then(Value::as_str)
        .map(str::to_string)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| anyhow::anyhow!("thread/fork response did not include thread.sessionId"))
}

fn recv_json_rpc_response_by_id(
    lines: &Receiver<String>,
    output: &mut String,
    id: i64,
) -> anyhow::Result<Value> {
    loop {
        let line = lines.recv_timeout(Duration::from_secs(30)).map_err(|_| {
            anyhow::anyhow!("Codex app-server response missing for request id {id}")
        })?;
        output.push_str(&line);
        output.push('\n');
        let response: Value = serde_json::from_str(line.trim())?;
        if response.get("id").and_then(Value::as_i64) == Some(id) {
            return Ok(response);
        }
    }
}

fn recv_generated_title(lines: &Receiver<String>) -> anyhow::Result<String> {
    let mut latest_title = String::new();
    loop {
        let line = lines
            .recv_timeout(Duration::from_secs(GENERATED_TITLE_TIMEOUT_SECS))
            .map_err(|_| anyhow::anyhow!("Codex title generation timed out"))?;
        let response: Value = serde_json::from_str(line.trim())?;
        if response.get("method").and_then(Value::as_str) == Some("item/completed") {
            if let Some(item) = response.get("params").and_then(|params| params.get("item")) {
                if item.get("type").and_then(Value::as_str) == Some("agentMessage") {
                    latest_title = item
                        .get("text")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string();
                }
            }
        }
        if response.get("method").and_then(Value::as_str) != Some("turn/completed") {
            continue;
        }
        let status = response
            .get("params")
            .and_then(|params| params.get("turn"))
            .and_then(|turn| turn.get("status"))
            .and_then(Value::as_str)
            .unwrap_or_default();
        if status != "completed" {
            anyhow::bail!("Codex title generation turn failed");
        }
        return clean_generated_title(&latest_title);
    }
}

fn clean_generated_title(value: &str) -> anyhow::Result<String> {
    let title = value
        .replace(['\r', '\n', '\t'], " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .trim_matches(|ch: char| {
            ch.is_whitespace()
                || ch == '"'
                || ch == '\''
                || ch == '`'
                || ch == '“'
                || ch == '”'
                || ch == '‘'
                || ch == '’'
        })
        .trim_matches(|ch: char| ch.is_ascii_punctuation())
        .to_string();
    if title.is_empty() {
        anyhow::bail!("Codex generated title is empty");
    }
    Ok(title)
}

pub fn json_rpc_response_by_id(output: &str, id: i64) -> anyhow::Result<Value> {
    for line in output
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
    {
        let response: Value = serde_json::from_str(line)?;
        if response.get("id").and_then(Value::as_i64) != Some(id) {
            continue;
        }
        if let Some(error) = response.get("error") {
            let message = error
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or("Codex app-server request failed");
            anyhow::bail!("{message}");
        }
        return Ok(response);
    }
    anyhow::bail!("Codex app-server response missing for request id {id}")
}

fn json_rpc_response_error(response: &Value) -> anyhow::Result<()> {
    if let Some(error) = response.get("error") {
        let message = error
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or("Codex app-server request failed");
        anyhow::bail!("{message}");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::ThreadForker;
    use serde_json::json;
    use std::fs;

    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn fork_request_uses_official_thread_fork_method() {
        let request = super::build_thread_fork_request(2, "thread-1", "/tmp/project");

        assert_eq!(
            request,
            json!({
                "id": 2,
                "method": "thread/fork",
                "params": {
                    "threadId": "thread-1",
                    "cwd": "/tmp/project",
                    "ephemeral": false
                }
            })
        );
    }

    #[test]
    fn fork_from_path_request_uses_official_thread_fork_method() {
        let request =
            super::build_thread_fork_from_path_request(2, "/tmp/source.jsonl", "/tmp/project");

        assert_eq!(
            request,
            json!({
                "id": 2,
                "method": "thread/fork",
                "params": {
                    "threadId": "",
                    "path": "/tmp/source.jsonl",
                    "cwd": "/tmp/project",
                    "ephemeral": false
                }
            })
        );
    }

    #[test]
    fn initialized_notification_uses_official_method() {
        assert_eq!(
            super::build_initialized_notification(),
            json!({ "method": "initialized" })
        );
    }

    #[test]
    fn remote_filename_sanitizer_removes_shell_sensitive_characters() {
        assert_eq!(
            super::sanitize_remote_filename("rollout-../bad name"),
            "rollout-..-bad-name"
        );
        assert_eq!(super::sanitize_remote_filename("///"), "session");
    }

    #[test]
    fn remote_app_server_command_uses_login_shell_path() {
        let command = super::remote_app_server_command("codex");

        assert!(command.contains("\"$SHELL\" -lc"));
        assert!(command.contains("app-server --listen stdio://"));
        assert!(!command.contains("ssh"));
    }

    #[test]
    fn rename_request_uses_official_thread_name_set_method() {
        let request = super::build_thread_name_request(3, "thread-2", "Readable name");

        assert_eq!(
            request,
            json!({
                "id": 3,
                "method": "thread/name/set",
                "params": {
                    "threadId": "thread-2",
                    "name": "Readable name"
                }
            })
        );
    }

    #[test]
    fn generated_title_prompt_uses_transcript_only() {
        let prompt = super::title_generation_prompt(
            "User: Check the current codebase.\nAssistant: Confirmed Admin preset API wiring.",
            4,
            10,
        );

        assert!(prompt.contains("Transcript:"));
        assert!(prompt.contains("Confirmed Admin preset API wiring"));
        assert!(!prompt.contains("Existing title"));
    }

    #[test]
    fn generated_title_timeout_matches_bridge_budget() {
        assert_eq!(super::GENERATED_TITLE_TIMEOUT_SECS, 120);
    }

    #[test]
    fn extracts_forked_thread_id_from_response() {
        let response = json!({
            "id": 2,
            "result": {
                "thread": {
                    "sessionId": "thread-2",
                    "preview": "Readable name"
                }
            }
        });

        assert_eq!(
            super::forked_thread_id_from_response(&response).expect("thread id"),
            "thread-2"
        );
    }

    #[test]
    fn finds_matching_response_by_request_id() {
        let output = r#"{"method":"thread/name/updated","params":{"threadId":"thread-2"}}
{"id":3,"result":{}}
"#;

        assert_eq!(
            super::json_rpc_response_by_id(output, 3).expect("response"),
            json!({"id": 3, "result": {}})
        );
    }

    #[test]
    fn surfaces_json_rpc_error_for_matching_request_id() {
        let output =
            r#"{"id":3,"error":{"code":-32602,"message":"thread name must not be empty"}}"#;

        let error = super::json_rpc_response_by_id(output, 3).expect_err("rpc error");

        assert!(error.to_string().contains("thread name must not be empty"));
    }

    #[test]
    fn request_batch_is_newline_delimited_json() {
        let input = super::request_batch(&[
            super::build_initialize_request(1),
            super::build_thread_name_request(2, "thread-2", "Readable name"),
        ])
        .expect("batch");

        assert_eq!(
            input,
            "{\"id\":1,\"method\":\"initialize\",\"params\":{\"capabilities\":{\"experimentalApi\":true,\"optOutNotificationMethods\":[\"thread/started\",\"thread/status/changed\",\"thread/name/updated\"]},\"clientInfo\":{\"name\":\"codex-helper\",\"version\":\"0.1.0\"}}}\n{\"id\":2,\"method\":\"thread/name/set\",\"params\":{\"name\":\"Readable name\",\"threadId\":\"thread-2\"}}\n"
        );
    }

    #[test]
    #[cfg(unix)]
    fn fork_client_keeps_app_server_connection_open_until_response() {
        let dir = std::env::temp_dir().join(format!(
            "codex-helper-fake-app-server-{}",
            std::process::id()
        ));
        fs::create_dir_all(&dir).expect("temp dir");
        let script = dir.join("codex-fake");
        fs::write(
            &script,
            r#"#!/bin/sh
initialized=0
while IFS= read -r line; do
  case "$line" in
    *'"method":"initialize"'*)
      printf '%s\n' '{"id":1,"result":{"userAgent":"fake","codexHome":"/tmp","platformFamily":"unix","platformOs":"macos"}}'
      ;;
    *'"method":"initialized"'*)
      initialized=1
      ;;
    *'"method":"thread/fork"'*)
      if [ "$initialized" = 1 ]; then
        printf '%s\n' '{"id":2,"result":{"thread":{"sessionId":"forked-thread"}}}'
      fi
      ;;
    *'"method":"thread/name/set"'*)
      if [ "$initialized" = 1 ]; then
        printf '%s\n' '{"id":2,"result":{}}'
      fi
      ;;
  esac
done
"#,
        )
        .expect("script");
        let mut permissions = fs::metadata(&script).expect("metadata").permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&script, permissions).expect("chmod");

        let client = super::CodexAppServerClient::new(script.to_string_lossy().to_string());
        let forked = client
            .fork_thread_to_workspace("source-thread", "/tmp/project", "Readable name")
            .expect("forked thread");

        assert_eq!(forked.session_id, "forked-thread");
        assert_eq!(forked.warning, None);
    }

    #[test]
    #[cfg(unix)]
    fn fork_client_reports_rename_failure_as_warning() {
        let dir = std::env::temp_dir().join(format!(
            "codex-helper-fake-app-server-rename-{}",
            std::process::id()
        ));
        fs::create_dir_all(&dir).expect("temp dir");
        let script = dir.join("codex-fake");
        fs::write(
            &script,
            r#"#!/bin/sh
initialized=0
while IFS= read -r line; do
  case "$line" in
    *'"method":"initialize"'*)
      printf '%s\n' '{"id":1,"result":{"userAgent":"fake","codexHome":"/tmp","platformFamily":"unix","platformOs":"macos"}}'
      ;;
    *'"method":"initialized"'*)
      initialized=1
      ;;
    *'"method":"thread/fork"'*)
      if [ "$initialized" = 1 ]; then
        printf '%s\n' '{"id":2,"result":{"thread":{"sessionId":"forked-thread"}}}'
      fi
      ;;
    *'"method":"thread/name/set"'*)
      if [ "$initialized" = 1 ]; then
        printf '%s\n' '{"id":2,"error":{"code":-32603,"message":"rename failed"}}'
      fi
      ;;
  esac
done
"#,
        )
        .expect("script");
        let mut permissions = fs::metadata(&script).expect("metadata").permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&script, permissions).expect("chmod");

        let client = super::CodexAppServerClient::new(script.to_string_lossy().to_string());
        let forked = client
            .fork_thread_to_workspace("source-thread", "/tmp/project", "Readable name")
            .expect("forked thread");

        assert_eq!(forked.session_id, "forked-thread");
        assert!(forked
            .warning
            .expect("rename warning")
            .contains("rename failed"));
    }
}
