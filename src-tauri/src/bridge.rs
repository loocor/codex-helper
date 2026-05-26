use std::collections::{HashMap, VecDeque};
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::task::JoinHandle;
use tokio_tungstenite::tungstenite::Message;

use crate::cdp::connect_cdp_websocket;
use crate::routes::{handle_bridge_request, BridgeContext};

const BRIDGE_BINDING_NAME: &str = "codexHelperBridgeV1";
const CDP_COMMAND_TIMEOUT: Duration = Duration::from_secs(5);

type BridgeHandler = Arc<
    dyn Fn(BridgeRequest) -> Pin<Box<dyn Future<Output = anyhow::Result<Value>> + Send>>
        + Send
        + Sync,
>;

static NEXT_MESSAGE_ID: AtomicU64 = AtomicU64::new(100);

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct BridgeCaller {
    pub target_id: String,
    pub helper_instance_id: String,
    #[serde(default)]
    pub href: String,
    #[serde(default)]
    pub has_focus: bool,
    #[serde(default)]
    pub visibility_state: String,
}

impl BridgeCaller {
    pub fn new_for_target(target_id: &str, helper_instance_id: &str) -> Self {
        Self {
            target_id: target_id.to_string(),
            helper_instance_id: helper_instance_id.to_string(),
            href: String::new(),
            has_focus: false,
            visibility_state: String::new(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct BridgeRequest {
    pub id: String,
    pub path: String,
    #[serde(default = "empty_payload")]
    pub payload: Value,
    #[serde(default)]
    pub caller: BridgeCaller,
}

fn empty_payload() -> Value {
    json!({})
}

fn parse_bridge_request(payload_text: &str) -> anyhow::Result<BridgeRequest> {
    Ok(serde_json::from_str(payload_text)?)
}

pub fn build_bridge_script(binding_name: &str, caller: &BridgeCaller) -> String {
    let caller_json = serde_json::to_string(caller).expect("bridge caller metadata must serialize");
    format!(
        r#"
(() => {{
  window.__codexHelperCallbacks = new Map();
  window.__codexHelperSeq = 0;
  window.__codexHelperCallerBase = {caller_json};
  window.__codexHelperCaller = () => ({{
    ...window.__codexHelperCallerBase,
    href: window.location.href,
    hasFocus: document.hasFocus(),
    visibilityState: document.visibilityState || "",
  }});
  window.__codexHelperResolve = (id, result) => {{
    const callback = window.__codexHelperCallbacks.get(id);
    if (!callback) return;
    window.__codexHelperCallbacks.delete(id);
    callback.resolve(result);
  }};
  window.__codexHelperReject = (id, message) => {{
    const callback = window.__codexHelperCallbacks.get(id);
    if (!callback) return;
    window.__codexHelperCallbacks.delete(id);
    callback.resolve({{ status: "failed", message }});
  }};
  window.__codexHelperBridge = (path, payload = {{}}) => new Promise((resolve) => {{
    const id = String(++window.__codexHelperSeq);
    window.__codexHelperCallbacks.set(id, {{ resolve }});
    window.{binding_name}(JSON.stringify({{ id, path, payload, caller: window.__codexHelperCaller() }}));
  }});
}})();
"#
    )
}

pub async fn install_bridge(
    websocket_url: &str,
    target_id: &str,
    caller: BridgeCaller,
    ctx: BridgeContext,
    runtime_scripts: Vec<String>,
) -> anyhow::Result<JoinHandle<()>> {
    let handler = bridge_handler(ctx);
    let socket = connect_cdp_websocket(websocket_url).await?;
    let mut session = BindingCdpSession::new(socket).with_handler(handler);
    let attached = session
        .send_command(
            1,
            "Target.attachToTarget",
            json!({ "targetId": target_id, "flatten": true }),
        )
        .await?;
    let session_id = attached
        .get("result")
        .and_then(|result| result.get("sessionId"))
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("CDP attach response did not include sessionId"))?
        .to_string();
    session = session.with_session_id(session_id);

    session.send_command(2, "Runtime.enable", json!({})).await?;
    session
        .send_command(
            3,
            "Runtime.removeBinding",
            json!({ "name": BRIDGE_BINDING_NAME }),
        )
        .await?;
    session
        .send_command(
            4,
            "Runtime.addBinding",
            json!({ "name": BRIDGE_BINDING_NAME }),
        )
        .await?;

    let bridge_script = build_bridge_script(BRIDGE_BINDING_NAME, &caller);
    session
        .send_command(
            5,
            "Page.addScriptToEvaluateOnNewDocument",
            json!({ "source": bridge_script }),
        )
        .await?;
    session
        .send_command(
            6,
            "Runtime.evaluate",
            runtime_evaluate_params(&bridge_script),
        )
        .await?;

    for script in runtime_scripts {
        let message_id = next_message_id();
        session
            .send_command(
                message_id,
                "Page.addScriptToEvaluateOnNewDocument",
                json!({ "source": script }),
            )
            .await?;
        let message_id = next_message_id();
        session
            .send_command(
                message_id,
                "Runtime.evaluate",
                runtime_evaluate_params(&script),
            )
            .await?;
    }

    let binding_task = tokio::spawn(async move {
        loop {
            if session.drain_binding_queue().await.is_err() {
                break;
            }
            match session.next_message().await {
                Ok(Some(_)) => {}
                Ok(None) | Err(_) => break,
            }
        }
    });

    Ok(binding_task)
}

fn bridge_handler(ctx: BridgeContext) -> BridgeHandler {
    Arc::new(move |request| {
        let ctx = ctx.clone();
        Box::pin(async move { Ok(handle_bridge_request(ctx, request).await) })
    })
}

fn runtime_evaluate_params(expression: &str) -> Value {
    json!({
        "expression": expression,
        "awaitPromise": false,
        "allowUnsafeEvalBlockedByCSP": true,
    })
}

fn cdp_command(id: u64, method: &str, params: Value, session_id: Option<&str>) -> Value {
    let mut command = json!({ "id": id, "method": method, "params": params });
    if let Some(session_id) = session_id {
        command["sessionId"] = json!(session_id);
    }
    command
}

fn resolve_bridge_expression(request_id: &str, result: &Value) -> anyhow::Result<String> {
    Ok(format!(
        "window.__codexHelperResolve({}, {})",
        serde_json::to_string(request_id)?,
        serde_json::to_string(result)?,
    ))
}

fn reject_bridge_expression(request_id: &str, message: &str) -> anyhow::Result<String> {
    Ok(format!(
        "window.__codexHelperReject({}, {})",
        serde_json::to_string(request_id)?,
        serde_json::to_string(message)?,
    ))
}

struct BindingCdpSession<S> {
    socket: S,
    responses: HashMap<u64, Value>,
    binding_calls: VecDeque<Value>,
    handler: Option<BridgeHandler>,
    session_id: Option<String>,
}

impl<S> BindingCdpSession<S>
where
    S: SinkExt<Message>
        + StreamExt<Item = Result<Message, tokio_tungstenite::tungstenite::Error>>
        + Unpin
        + Send,
    <S as futures_util::Sink<Message>>::Error: std::error::Error + Send + Sync + 'static,
{
    fn new(socket: S) -> Self {
        Self {
            socket,
            responses: HashMap::new(),
            binding_calls: VecDeque::new(),
            handler: None,
            session_id: None,
        }
    }

    fn with_handler(mut self, handler: BridgeHandler) -> Self {
        self.handler = Some(handler);
        self
    }

    fn with_session_id(mut self, session_id: String) -> Self {
        self.session_id = Some(session_id);
        self
    }

    async fn send_command(
        &mut self,
        id: u64,
        method: &str,
        params: Value,
    ) -> anyhow::Result<Value> {
        self.socket
            .send(Message::Text(
                cdp_command(id, method, params, self.session_id.as_deref())
                    .to_string()
                    .into(),
            ))
            .await?;
        tokio::time::timeout(CDP_COMMAND_TIMEOUT, self.wait_for_response(id, method))
            .await
            .map_err(|_| {
                anyhow::anyhow!(
                    "timed out waiting for CDP command {method} after {}s",
                    CDP_COMMAND_TIMEOUT.as_secs()
                )
            })?
    }

    async fn send_command_without_wait(
        &mut self,
        id: u64,
        method: &str,
        params: Value,
    ) -> anyhow::Result<()> {
        self.socket
            .send(Message::Text(
                cdp_command(id, method, params, self.session_id.as_deref())
                    .to_string()
                    .into(),
            ))
            .await?;
        Ok(())
    }

    async fn wait_for_response(&mut self, id: u64, method: &str) -> anyhow::Result<Value> {
        loop {
            if let Some(response) = self.responses.remove(&id) {
                if let Some(error) = response.get("error") {
                    anyhow::bail!("CDP command {method} failed: {error}");
                }
                return Ok(response);
            }
            let Some(message) = self.next_message().await? else {
                anyhow::bail!("CDP command {method} closed before response");
            };
            if let Some(response_id) = message.get("id").and_then(Value::as_u64) {
                if response_id == id {
                    if let Some(error) = message.get("error") {
                        anyhow::bail!("CDP command {method} failed: {error}");
                    }
                    return Ok(message);
                }
                self.responses.insert(response_id, message);
            }
        }
    }

    async fn next_message(&mut self) -> anyhow::Result<Option<Value>> {
        let Some(message) = self.socket.next().await else {
            return Ok(None);
        };
        let message = message?;
        let value = match message {
            Message::Text(text) => serde_json::from_str(&text)?,
            Message::Binary(bytes) => serde_json::from_slice(&bytes)?,
            _ => json!({}),
        };
        if value.get("method").and_then(Value::as_str) == Some("Runtime.bindingCalled") {
            self.binding_calls.push_back(value.clone());
        }
        Ok(Some(value))
    }

    async fn drain_binding_queue(&mut self) -> anyhow::Result<()> {
        while let Some(message) = self.binding_calls.pop_front() {
            self.route_binding_call(message).await?;
        }
        Ok(())
    }

    fn route_binding_call(
        &mut self,
        message: Value,
    ) -> Pin<Box<dyn Future<Output = anyhow::Result<()>> + Send + '_>> {
        Box::pin(async move {
            let Some(handler) = self.handler.clone() else {
                return Ok(());
            };
            let Some(payload_text) = message
                .get("params")
                .and_then(|params| params.get("payload"))
                .and_then(Value::as_str)
            else {
                return Ok(());
            };

            let request = match parse_bridge_request(payload_text) {
                Ok(parsed) => parsed,
                Err(error) => {
                    if let Some(request_id) = extract_string_field(payload_text, "id") {
                        self.reject_bridge_request(
                            &request_id,
                            &format!("failed to parse bridge payload: {error}"),
                        )
                        .await?;
                    }
                    return Ok(());
                }
            };
            self.route_parsed_binding_call(&handler, request).await
        })
    }

    async fn route_parsed_binding_call(
        &mut self,
        handler: &BridgeHandler,
        request: BridgeRequest,
    ) -> anyhow::Result<()> {
        match handler(request.clone()).await {
            Ok(result) => self.resolve_bridge_request(&request.id, &result).await?,
            Err(error) => {
                self.reject_bridge_request(&request.id, &error.to_string())
                    .await?
            }
        }
        Ok(())
    }

    async fn resolve_bridge_request(
        &mut self,
        request_id: &str,
        result: &Value,
    ) -> anyhow::Result<()> {
        let expression = resolve_bridge_expression(request_id, result)?;
        self.send_command_without_wait(
            next_message_id(),
            "Runtime.evaluate",
            runtime_evaluate_params(&expression),
        )
        .await
    }

    async fn reject_bridge_request(
        &mut self,
        request_id: &str,
        message: &str,
    ) -> anyhow::Result<()> {
        let expression = reject_bridge_expression(request_id, message)?;
        self.send_command_without_wait(
            next_message_id(),
            "Runtime.evaluate",
            runtime_evaluate_params(&expression),
        )
        .await
    }
}

fn extract_string_field(input: &str, field: &str) -> Option<String> {
    let needle = format!("\"{field}\"");
    let mut index = input.find(&needle)? + needle.len();
    let bytes = input.as_bytes();

    while matches!(bytes.get(index), Some(b' ' | b'\n' | b'\r' | b'\t')) {
        index += 1;
    }
    if bytes.get(index) != Some(&b':') {
        return None;
    }
    index += 1;
    while matches!(bytes.get(index), Some(b' ' | b'\n' | b'\r' | b'\t')) {
        index += 1;
    }
    if bytes.get(index) != Some(&b'"') {
        return None;
    }
    index += 1;

    let mut output = String::new();
    let mut escaped = false;
    for ch in input[index..].chars() {
        if escaped {
            output.push(ch);
            escaped = false;
            continue;
        }
        match ch {
            '\\' => escaped = true,
            '"' => return Some(output),
            _ => output.push(ch),
        }
    }
    None
}

fn next_message_id() -> u64 {
    NEXT_MESSAGE_ID.fetch_add(1, Ordering::Relaxed) + 1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bridge_script_defines_cdp_binding_bridge() {
        let script = build_bridge_script(
            "codexHelperBridgeV1",
            &BridgeCaller::new_for_target("target-1", "instance-1"),
        );

        assert!(script.contains("window.__codexHelperBridge"));
        assert!(script.contains("window.codexHelperBridgeV1"));
        assert!(script.contains("window.__codexHelperResolve"));
        assert!(script.contains("window.__codexHelperReject"));
    }

    #[test]
    fn bridge_script_includes_caller_identity() {
        let script = build_bridge_script(
            "codexHelperBridgeV1",
            &BridgeCaller::new_for_target("target-1", "instance-1"),
        );

        assert!(script.contains("\"targetId\":\"target-1\""));
        assert!(script.contains("\"helperInstanceId\":\"instance-1\""));
        assert!(script.contains("document.hasFocus()"));
        assert!(script.contains("document.visibilityState"));
    }

    #[test]
    fn bridge_request_parses_caller_identity() {
        let request = parse_bridge_request(
            r#"{"id":"1","path":"/devtools/open","payload":{},"caller":{"targetId":"target-1","helperInstanceId":"instance-1","href":"app://-/index.html","hasFocus":true,"visibilityState":"visible"}}"#,
        )
        .expect("request");

        assert_eq!(request.id, "1");
        assert_eq!(request.path, "/devtools/open");
        assert_eq!(request.caller.target_id, "target-1");
        assert_eq!(request.caller.helper_instance_id, "instance-1");
        assert_eq!(request.caller.href, "app://-/index.html");
        assert!(request.caller.has_focus);
        assert_eq!(request.caller.visibility_state, "visible");
    }

    #[test]
    fn bridge_request_allows_missing_caller_for_global_routes() {
        let request = parse_bridge_request(r#"{"id":"1","path":"/backend/status","payload":{}}"#)
            .expect("request");

        assert_eq!(request.path, "/backend/status");
        assert_eq!(request.caller, BridgeCaller::default());
    }

    #[test]
    fn resolve_bridge_expression_serializes_result() {
        let expression =
            resolve_bridge_expression("request-1", &json!({"status": "ok"})).expect("expression");

        assert_eq!(
            expression,
            "window.__codexHelperResolve(\"request-1\", {\"status\":\"ok\"})"
        );
    }

    #[test]
    fn reject_bridge_expression_serializes_message() {
        let expression = reject_bridge_expression("request-1", "bad value").expect("expression");

        assert_eq!(
            expression,
            "window.__codexHelperReject(\"request-1\", \"bad value\")"
        );
    }

    #[test]
    fn extract_string_field_reads_escaped_value() {
        let id = extract_string_field(r#"{"id":"request\"1","payload":false}"#, "id");

        assert_eq!(id.as_deref(), Some("request\"1"));
    }

    #[test]
    fn cdp_command_includes_flattened_session_id() {
        let command = cdp_command(
            42,
            "Runtime.evaluate",
            json!({ "expression": "1 + 1" }),
            Some("session-1"),
        );

        assert_eq!(
            command,
            json!({
                "id": 42,
                "sessionId": "session-1",
                "method": "Runtime.evaluate",
                "params": { "expression": "1 + 1" },
            })
        );
    }
}
