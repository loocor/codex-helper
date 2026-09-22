use std::collections::HashSet;
use std::convert::Infallible;
use std::io;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};

use anyhow::Context;
use bytes::Bytes;
use futures_util::{StreamExt, TryStreamExt};
use http_body_util::{combinators::BoxBody, BodyExt, Full, StreamBody};
use hyper::body::{Frame, Incoming};
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use serde_json::Value;
use tokio::net::TcpListener;

use crate::compat_custom::{
    custom_tool_names_from_request, restore_custom_tool_calls, rewrite_custom_as_function,
    rewrite_custom_input_items,
};
use crate::deepseek_sanitize::{
    apply_deepseek_responses_request_compat, rewrite_deepseek_native_json_bytes,
    rewrite_deepseek_native_sse_block, DeepSeekRestoreMap,
};
use crate::endpoint;
use crate::llm_compat_inventory::{sse_block_is_response_completed, ResponseCompatInventory};
use crate::llm_traffic_log::{redact_header_pairs, PendingLlmLog};
use crate::logging::DiagnosticLogger;
use crate::provider_oauth::{
    copilot_request_headers, oauth_bearer_token, oauth_kind_from_provider, OAuthKind,
};
use crate::providers::{
    apply_provider_effort_aliases, apply_provider_model_mappings, provider_allowed_models,
    provider_device_oauth_kind, provider_effort_aliases,
    provider_needs_deepseek_responses_sanitize, provider_needs_xai_compat, read_store,
    resolve_model_route, rewrite_unmatched_request_model, selected_api_providers, Provider,
    ProviderKind, ProviderStore,
};
use crate::xai_sanitize::{
    append_utf8_safe, apply_xai_native_responses_request_compat, rewrite_xai_native_json_bytes,
    rewrite_xai_native_sse_block, take_sse_block, XaiNativeRestoreMap,
};

#[derive(Clone)]
enum NativeRestore {
    None,
    Xai(XaiNativeRestoreMap),
    DeepSeek(DeepSeekRestoreMap),
    CustomTools(HashSet<String>),
}

fn rewrite_native_sse_block(block: &str, restore: &NativeRestore) -> Bytes {
    match restore {
        NativeRestore::None => Bytes::from(format!("{block}\n\n")),
        NativeRestore::Xai(map) => rewrite_xai_native_sse_block(block, map),
        NativeRestore::DeepSeek(map) => rewrite_deepseek_native_sse_block(block, map),
        NativeRestore::CustomTools(names) => rewrite_custom_tool_sse_block(block, names),
    }
}

fn rewrite_native_json_bytes(bytes: &[u8], restore: &NativeRestore) -> Vec<u8> {
    match restore {
        NativeRestore::None => bytes.to_vec(),
        NativeRestore::Xai(map) => rewrite_xai_native_json_bytes(bytes, map),
        NativeRestore::DeepSeek(map) => rewrite_deepseek_native_json_bytes(bytes, map),
        NativeRestore::CustomTools(names) => rewrite_custom_tool_json_bytes(bytes, names),
    }
}

fn rewrite_custom_tool_json_bytes(bytes: &[u8], names: &HashSet<String>) -> Vec<u8> {
    let Ok(mut value) = serde_json::from_slice::<Value>(bytes) else {
        return bytes.to_vec();
    };
    if !restore_custom_tool_calls(&mut value, names) {
        return bytes.to_vec();
    }
    serde_json::to_vec(&value).unwrap_or_else(|_| bytes.to_vec())
}

fn rewrite_custom_tool_sse_block(block: &str, names: &HashSet<String>) -> Bytes {
    let mut out = String::new();
    for line in block.lines() {
        let Some(payload) = line.strip_prefix("data:") else {
            out.push_str(line);
            out.push('\n');
            continue;
        };
        let payload = payload.trim();
        if payload.is_empty() || payload == "[DONE]" {
            out.push_str(line);
            out.push('\n');
            continue;
        }
        match serde_json::from_str::<Value>(payload) {
            Ok(mut value) => {
                restore_custom_tool_calls(&mut value, names);
                out.push_str("data: ");
                out.push_str(&value.to_string());
                out.push('\n');
            }
            Err(_) => {
                out.push_str(line);
                out.push('\n');
            }
        }
    }
    if out.ends_with('\n') {
        out.pop();
    }
    Bytes::from(format!("{out}\n\n"))
}

fn remember_downgraded_tools(restore: &mut NativeRestore, names: HashSet<String>) {
    match restore {
        NativeRestore::Xai(map) => map.custom_tool_names.extend(names),
        NativeRestore::DeepSeek(map) => map.custom_tool_names.extend(names),
        NativeRestore::CustomTools(existing) => existing.extend(names),
        NativeRestore::None => *restore = NativeRestore::CustomTools(names),
    }
}

fn upstream_error_excerpt(bytes: &[u8]) -> String {
    let text = String::from_utf8_lossy(bytes);
    let message = serde_json::from_slice::<Value>(bytes)
        .ok()
        .and_then(|value| {
            let error = value.get("error")?;
            error
                .as_str()
                .map(str::to_string)
                .or_else(|| error.get("message")?.as_str().map(str::to_string))
        })
        .unwrap_or_else(|| text.to_string());
    let excerpt: String = message.chars().take(180).collect();
    if excerpt.contains("sk-") || excerpt.to_ascii_lowercase().contains("bearer ") {
        return "[redacted]".to_string();
    }
    excerpt
}

type ProxyBody = BoxBody<Bytes, io::Error>;

pub const PROVIDER_PROXY_PORT: u16 = 3721;

#[derive(Clone)]
pub struct ProviderProxy {
    inner: Arc<Mutex<ProxyState>>,
}

struct ProxyState {
    port: u16,
    store: ProviderStore,
    state_root: Option<PathBuf>,
    bind_error: Option<String>,
    logger: Option<Arc<DiagnosticLogger>>,
    log_llm_traffic: bool,
}

impl ProviderProxy {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(ProxyState {
                port: 0,
                store: ProviderStore::default(),
                state_root: None,
                bind_error: None,
                logger: None,
                log_llm_traffic: false,
            })),
        }
    }

    pub fn port(&self) -> u16 {
        self.inner.lock().expect("provider proxy lock").port
    }

    pub fn set_state_root(&self, root: PathBuf) {
        self.inner.lock().expect("provider proxy lock").state_root = Some(root);
    }

    pub fn base_url(&self) -> anyhow::Result<String> {
        let state = self.inner.lock().expect("provider proxy lock");
        if state.port != 0 {
            return Ok(format!("http://127.0.0.1:{}/v1", state.port));
        }
        anyhow::bail!(state.bind_error.clone().unwrap_or_else(|| format!(
            "Provider proxy is not listening on 127.0.0.1:{PROVIDER_PROXY_PORT}"
        )))
    }

    pub fn set_store(&self, store: ProviderStore) {
        self.inner.lock().expect("provider proxy lock").store = store;
    }

    pub fn set_logger(&self, logger: Arc<DiagnosticLogger>) {
        self.inner.lock().expect("provider proxy lock").logger = Some(logger);
    }

    pub fn set_log_llm_traffic(&self, enabled: bool) {
        self.inner
            .lock()
            .expect("provider proxy lock")
            .log_llm_traffic = enabled;
    }

    pub fn active_provider(&self) -> anyhow::Result<Option<Provider>> {
        let (state_root, fallback) = {
            let state = self.inner.lock().expect("provider proxy lock");
            (state.state_root.clone(), state.store.clone())
        };
        let store = if let Some(state_root) = state_root {
            read_store(&state_root)?
        } else {
            fallback
        };
        Ok(store
            .providers
            .iter()
            .find(|provider| provider.id == store.active_id)
            .cloned())
    }

    pub async fn bind_and_serve(&self) -> anyhow::Result<u16> {
        self.bind_on(PROVIDER_PROXY_PORT).await
    }

    async fn bind_on(&self, port: u16) -> anyhow::Result<u16> {
        let listener = match TcpListener::bind(SocketAddr::from(([127, 0, 0, 1], port))).await {
            Ok(listener) => listener,
            Err(error) => {
                self.inner.lock().expect("provider proxy lock").bind_error = Some(format!(
                    "Failed to bind provider proxy on 127.0.0.1:{port}: {error}"
                ));
                return Err(error.into());
            }
        };
        let port = listener.local_addr()?.port();
        {
            let mut state = self.inner.lock().expect("provider proxy lock");
            state.port = port;
            state.bind_error = None;
        }
        let proxy = self.clone();
        tokio::spawn(async move {
            loop {
                let Ok((stream, _)) = listener.accept().await else {
                    continue;
                };
                let proxy = proxy.clone();
                tokio::spawn(async move {
                    let io = TokioIo::new(stream);
                    let service = service_fn(move |request| {
                        let proxy = proxy.clone();
                        async move { Ok::<_, Infallible>(proxy.handle(request).await) }
                    });
                    let _ = http1::Builder::new().serve_connection(io, service).await;
                });
            }
        });
        Ok(port)
    }

    async fn handle(&self, request: Request<Incoming>) -> Response<ProxyBody> {
        match self.forward(request).await {
            Ok(response) => response,
            Err(error) => {
                let message = error.to_string();
                let status = if message.starts_with("Unauthorized:") {
                    StatusCode::UNAUTHORIZED
                } else {
                    StatusCode::BAD_GATEWAY
                };
                Response::builder()
                    .status(status)
                    .header(hyper::header::CONTENT_TYPE, "application/json")
                    .body(bytes_body(
                        serde_json::json!({ "error": message }).to_string(),
                    ))
                    .unwrap_or_else(|_| Response::new(bytes_body("{}")))
            }
        }
    }

    async fn forward(&self, request: Request<Incoming>) -> anyhow::Result<Response<ProxyBody>> {
        let path = request
            .uri()
            .path_and_query()
            .map(|value| value.as_str().to_string())
            .unwrap_or_else(|| "/".to_string());
        let method = request.method().clone();
        let headers = request.headers().clone();
        let raw_body = request.collect().await?.to_bytes();
        let store = self.current_store()?;
        let mut provider = store
            .providers
            .iter()
            .find(|item| item.id == store.active_id)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("No active provider"))?;
        let mut routed_upstream = None;
        if method == hyper::Method::POST && is_llm_path(&path) {
            if let Ok(json_body) = serde_json::from_slice::<Value>(&raw_body) {
                if let Some(model) = json_body.get("model").and_then(Value::as_str) {
                    if let Some((routed, route)) = resolve_model_route(&store, model) {
                        provider = routed.clone();
                        routed_upstream = Some(route.upstream_model);
                    }
                }
            }
        }
        if provider.id == "official" || provider.kind == ProviderKind::Oauth {
            anyhow::bail!("Official ChatGPT login does not use the Helper provider proxy");
        }
        self.authorize_request(&headers, &provider, &store)?;
        let upstream = provider.base_url.trim().trim_end_matches('/');
        if upstream.is_empty() {
            anyhow::bail!("Active provider has no base URL");
        }
        let oauth_kind = provider_device_oauth_kind(&provider);
        let upstream = oauth_kind
            .map(|kind| kind.default_base_url().to_string())
            .unwrap_or_else(|| upstream.to_string());
        let url = join_provider_upstream_url_for(oauth_kind, &upstream, &path);
        let mut body = raw_body.to_vec();
        let xai_compat = provider_needs_xai_compat(&provider);
        let deepseek_sanitize = provider_needs_deepseek_responses_sanitize(&provider);
        let responses_path = is_responses_path(&path);
        let xai_request = xai_compat && responses_path;
        let deepseek_request = deepseek_sanitize && responses_path;
        let rewrite = method == hyper::Method::POST && is_llm_path(&path);
        let mut restore = NativeRestore::None;
        let mut retry_json = None;
        if rewrite {
            let mut json_body = serde_json::from_slice::<Value>(&body)
                .context("Provider request is not valid JSON")?;
            let mapped = apply_provider_model_mappings(&mut json_body, &provider.model_mappings);
            if !mapped {
                if let Some(upstream_model) = &routed_upstream {
                    if let Some(object) = json_body.as_object_mut() {
                        object.insert("model".to_string(), Value::String(upstream_model.clone()));
                    }
                } else {
                    rewrite_unmatched_request_model(
                        &mut json_body,
                        &provider.model,
                        &provider_allowed_models(&provider),
                    );
                }
            }
            apply_provider_effort_aliases(&mut json_body, provider_effort_aliases(&provider));
            if xai_request {
                restore = NativeRestore::Xai(apply_xai_native_responses_request_compat(
                    &mut json_body,
                    Some(provider.model.as_str()).filter(|model| !model.is_empty()),
                    &provider_allowed_models(&provider),
                ));
            }
            if deepseek_request {
                restore = NativeRestore::DeepSeek(apply_deepseek_responses_request_compat(
                    &mut json_body,
                ));
            }
            retry_json = Some(json_body.clone());
            body = serde_json::to_vec(&json_body)?;
        }
        let mut pending_log = self.pending_llm_log(&path, &method, &provider.id, &headers, &body);
        let client = reqwest::Client::builder()
            .no_proxy()
            .build()
            .context("Failed to build provider proxy client")?;
        let mut response = self
            .send_upstream(
                &client,
                method.clone(),
                &url,
                &headers,
                oauth_kind,
                &provider,
                body.clone(),
            )
            .await?;
        let first_status = response.status();
        if responses_path && matches!(first_status.as_u16(), 400 | 422) {
            if let Some(json_body) = retry_json.as_mut() {
                let names = custom_tool_names_from_request(json_body, &HashSet::new());
                if !names.is_empty() {
                    let error_bytes = response.bytes().await.unwrap_or_default();
                    let excerpt = upstream_error_excerpt(&error_bytes);
                    let changed = rewrite_custom_as_function(json_body, &HashSet::new())
                        | rewrite_custom_input_items(json_body, &HashSet::new());
                    if !changed {
                        return Ok(Response::builder()
                            .status(first_status)
                            .header(hyper::header::CONTENT_TYPE, "application/json")
                            .body(bytes_body(error_bytes))?);
                    }
                    if let Some(log) = pending_log.take() {
                        log.succeed(
                            first_status.as_u16(),
                            false,
                            serde_json::json!({}),
                            error_bytes.len(),
                            None,
                        );
                    }
                    remember_downgraded_tools(&mut restore, names);
                    body = serde_json::to_vec(&*json_body)?;
                    pending_log =
                        self.pending_llm_log(&path, &method, &provider.id, &headers, &body);
                    let retry = match self
                        .send_upstream(
                            &client,
                            method.clone(),
                            &url,
                            &headers,
                            oauth_kind,
                            &provider,
                            body,
                        )
                        .await
                    {
                        Ok(retry) => retry,
                        Err(error) => {
                            self.log_protocol_downgrade(
                                &provider.id,
                                routed_upstream
                                    .as_deref()
                                    .unwrap_or(provider.model.as_str()),
                                first_status.as_u16(),
                                0,
                                &excerpt,
                            );
                            if let Some(log) = pending_log {
                                log.fail(
                                    &format!("Provider upstream request failed: {error}"),
                                    None,
                                );
                            }
                            return Err(error).context("Provider upstream request failed");
                        }
                    };
                    self.log_protocol_downgrade(
                        &provider.id,
                        routed_upstream
                            .as_deref()
                            .unwrap_or(provider.model.as_str()),
                        first_status.as_u16(),
                        retry.status().as_u16(),
                        &excerpt,
                    );
                    response = retry;
                }
            }
        }
        let status = response.status();
        let response_headers = response.headers().clone();
        let is_sse = response_headers
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .is_some_and(|value| value.to_ascii_lowercase().contains("text/event-stream"));
        let mut builder = Response::builder().status(status);
        for (name, value) in response_headers.iter() {
            if matches!(
                name.as_str(),
                "connection" | "transfer-encoding" | "content-length"
            ) {
                continue;
            }
            builder = builder.header(name.as_str(), value);
        }
        let logged_headers = pending_log
            .as_ref()
            .map(|_| redact_response_headers(&response_headers))
            .unwrap_or(Value::Null);
        if !matches!(restore, NativeRestore::None) && status.is_success() {
            if is_sse {
                let inventory = Arc::new(Mutex::new(ResponseCompatInventory::default()));
                let rewrite_inventory = inventory.clone();
                let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<Result<Bytes, io::Error>>();
                tokio::spawn(async move {
                    let mut buffer = String::new();
                    let mut remainder = Vec::new();
                    let mut stream = response.bytes_stream();
                    while let Some(chunk) = stream.next().await {
                        match chunk {
                            Ok(bytes) => {
                                append_utf8_safe(&mut buffer, &mut remainder, &bytes);
                                while let Some(block) = take_sse_block(&mut buffer) {
                                    if block.trim().is_empty() {
                                        continue;
                                    }
                                    if !send_inspected_sse_block(
                                        &tx,
                                        &block,
                                        &restore,
                                        &rewrite_inventory,
                                    ) {
                                        return;
                                    }
                                }
                            }
                            Err(error) => {
                                let _ = tx.send(Err(io::Error::other(error.to_string())));
                                return;
                            }
                        }
                    }
                    if !remainder.is_empty() {
                        buffer.push_str(&String::from_utf8_lossy(&remainder));
                    }
                    if !buffer.trim().is_empty() {
                        let _ =
                            send_inspected_sse_block(&tx, &buffer, &restore, &rewrite_inventory);
                    }
                });
                let log_inventory = pending_log.as_ref().map(|_| inventory);
                return Ok(builder.body(outgoing_body(
                    receiver_byte_stream(rx),
                    pending_log,
                    status.as_u16(),
                    true,
                    logged_headers,
                    log_inventory,
                    false,
                ))?);
            }
            let body_bytes = response
                .bytes()
                .await
                .context("Failed to read provider upstream body")?;
            let rewritten = rewrite_native_json_bytes(&body_bytes, &restore);
            let inventory = pending_log.as_ref().map(|_| {
                let mut inventory = ResponseCompatInventory::default();
                inventory.observe_json_pair_bytes(&body_bytes, &rewritten);
                inventory
            });
            return Ok(builder.body(logged_bytes_body(
                rewritten,
                pending_log,
                status.as_u16(),
                logged_headers,
                inventory,
            ))?);
        }
        let inventory = pending_log
            .as_ref()
            .map(|_| Arc::new(Mutex::new(ResponseCompatInventory::default())));
        let inspect_stream = inventory.is_some();
        let stream = response
            .bytes_stream()
            .map_err(|error| io::Error::other(error.to_string()));
        Ok(builder.body(outgoing_body(
            stream,
            pending_log,
            status.as_u16(),
            is_sse,
            logged_headers,
            inventory,
            inspect_stream,
        ))?)
    }
}

fn bytes_body(bytes: impl Into<Bytes>) -> ProxyBody {
    Full::new(bytes.into())
        .map_err(|infallible: Infallible| match infallible {})
        .boxed()
}

fn boxed_bytes_stream<S>(stream: S) -> ProxyBody
where
    S: futures_util::Stream<Item = Result<Bytes, io::Error>> + Send + Sync + 'static,
{
    BodyExt::boxed(StreamBody::new(stream.map_ok(Frame::data)))
}

fn receiver_byte_stream(
    rx: tokio::sync::mpsc::UnboundedReceiver<Result<Bytes, io::Error>>,
) -> impl futures_util::Stream<Item = Result<Bytes, io::Error>> + Send + Sync + 'static {
    futures_util::stream::unfold(rx, |mut rx| async move {
        rx.recv().await.map(|item| (item, rx))
    })
}

fn redact_response_headers(headers: &reqwest::header::HeaderMap) -> Value {
    redact_header_pairs(
        headers
            .iter()
            .filter_map(|(name, value)| value.to_str().ok().map(|value| (name.as_str(), value))),
    )
}

fn logged_bytes_body(
    bytes: Vec<u8>,
    pending: Option<PendingLlmLog>,
    status: u16,
    response_headers: Value,
    inventory: Option<ResponseCompatInventory>,
) -> ProxyBody {
    if let Some(pending) = pending {
        pending.succeed(
            status,
            false,
            response_headers,
            bytes.len(),
            inventory.as_ref(),
        );
    }
    bytes_body(bytes)
}

const REASONING_ONLY_COMPLETED: &str = "upstream completed with reasoning only";

fn send_inspected_sse_block(
    tx: &tokio::sync::mpsc::UnboundedSender<Result<Bytes, io::Error>>,
    block: &str,
    restore: &NativeRestore,
    inventory: &Arc<Mutex<ResponseCompatInventory>>,
) -> bool {
    let rewritten = rewrite_inspected_sse_block(block, restore, Some(inventory));
    if should_fail_reasoning_only_completed(inventory, block) {
        let _ = tx.send(Err(io::Error::other(REASONING_ONLY_COMPLETED)));
        return false;
    }
    tx.send(Ok(rewritten)).is_ok()
}

fn should_fail_reasoning_only_completed(
    inventory: &Arc<Mutex<ResponseCompatInventory>>,
    block: &str,
) -> bool {
    sse_block_is_response_completed(block)
        && lock_compat_inventory(inventory).has_reasoning_without_output()
}

fn rewrite_inspected_sse_block(
    block: &str,
    restore: &NativeRestore,
    inventory: Option<&Arc<Mutex<ResponseCompatInventory>>>,
) -> Bytes {
    let rewritten = rewrite_native_sse_block(block, restore);
    if let Some(inventory) = inventory {
        observe_rewritten_sse_blocks(block, &rewritten, inventory);
    }
    rewritten
}

fn observe_rewritten_sse_blocks(
    before_block: &str,
    rewritten: &Bytes,
    inventory: &Arc<Mutex<ResponseCompatInventory>>,
) {
    let mut buffer = String::from_utf8_lossy(rewritten).into_owned();
    if !buffer.is_empty() && !buffer.ends_with("\n\n") && !buffer.ends_with("\r\n\r\n") {
        buffer.push_str("\n\n");
    }
    let mut first = true;
    let mut saw_block = false;
    while let Some(piece) = take_sse_block(&mut buffer) {
        if piece.trim().is_empty() {
            continue;
        }
        saw_block = true;
        let mut lock = lock_compat_inventory(inventory);
        if first {
            lock.observe_sse_pair(before_block, piece.as_bytes());
            first = false;
        } else {
            lock.observe_sse_block(&piece);
        }
    }
    if !saw_block {
        lock_compat_inventory(inventory).observe_sse_pair(before_block, rewritten);
    }
}

fn lock_compat_inventory(
    inventory: &Arc<Mutex<ResponseCompatInventory>>,
) -> std::sync::MutexGuard<'_, ResponseCompatInventory> {
    inventory
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn snapshot_compat_inventory(
    inventory: &Option<Arc<Mutex<ResponseCompatInventory>>>,
) -> Option<ResponseCompatInventory> {
    inventory
        .as_ref()
        .map(|inventory| lock_compat_inventory(inventory).clone())
}

fn succeed_pending(
    pending: PendingLlmLog,
    status: u16,
    sse: bool,
    response_headers: Value,
    response_bytes: usize,
    inventory: &Option<Arc<Mutex<ResponseCompatInventory>>>,
) {
    let snapshot = snapshot_compat_inventory(inventory);
    pending.succeed(
        status,
        sse,
        response_headers,
        response_bytes,
        snapshot.as_ref(),
    );
}

fn fail_pending(
    pending: PendingLlmLog,
    error: &str,
    inventory: &Option<Arc<Mutex<ResponseCompatInventory>>>,
) {
    let snapshot = snapshot_compat_inventory(inventory);
    pending.fail(error, snapshot.as_ref());
}

struct StreamInventory {
    sse: bool,
    sse_buffer: String,
    remainder: Vec<u8>,
    json_buf: Option<Vec<u8>>,
}

impl StreamInventory {
    fn new(sse: bool) -> Self {
        Self {
            sse,
            sse_buffer: String::new(),
            remainder: Vec::new(),
            json_buf: if sse { None } else { Some(Vec::new()) },
        }
    }

    fn observe_chunk(
        &mut self,
        inventory: &Arc<Mutex<ResponseCompatInventory>>,
        bytes: &[u8],
    ) -> bool {
        if self.sse {
            append_utf8_safe(&mut self.sse_buffer, &mut self.remainder, bytes);
            while let Some(block) = take_sse_block(&mut self.sse_buffer) {
                if block.trim().is_empty() {
                    continue;
                }
                let mut lock = lock_compat_inventory(inventory);
                lock.observe_sse_block(&block);
                if sse_block_is_response_completed(&block) && lock.has_reasoning_without_output() {
                    return true;
                }
            }
            return false;
        }
        if let Some(buf) = &mut self.json_buf {
            buf.extend_from_slice(bytes);
        }
        false
    }

    fn finish(mut self, inventory: &Arc<Mutex<ResponseCompatInventory>>) -> bool {
        if self.sse {
            if !self.remainder.is_empty() {
                self.sse_buffer
                    .push_str(&String::from_utf8_lossy(&self.remainder));
            }
            if !self.sse_buffer.trim().is_empty() {
                let mut lock = lock_compat_inventory(inventory);
                lock.observe_sse_block(&self.sse_buffer);
                return sse_block_is_response_completed(&self.sse_buffer)
                    && lock.has_reasoning_without_output();
            }
            return false;
        }
        if let Some(buf) = self.json_buf {
            lock_compat_inventory(inventory).observe_json_bytes(&buf);
        }
        false
    }
}

fn outgoing_body<S>(
    stream: S,
    pending: Option<PendingLlmLog>,
    status: u16,
    sse: bool,
    response_headers: Value,
    inventory: Option<Arc<Mutex<ResponseCompatInventory>>>,
    inspect_stream: bool,
) -> ProxyBody
where
    S: futures_util::Stream<Item = Result<Bytes, io::Error>> + Send + Sync + 'static,
{
    let Some(pending) = pending else {
        return boxed_bytes_stream(stream);
    };
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<Result<Bytes, io::Error>>();
    tokio::spawn(async move {
        let mut stream = std::pin::pin!(stream);
        let mut response_bytes = 0usize;
        let mut inspector = inspect_stream.then(|| StreamInventory::new(sse));
        while let Some(item) = stream.next().await {
            match item {
                Ok(bytes) => {
                    response_bytes += bytes.len();
                    if let (Some(inspector), Some(current)) = (&mut inspector, &inventory) {
                        if inspector.observe_chunk(current, &bytes) {
                            fail_pending(pending, REASONING_ONLY_COMPLETED, &inventory);
                            let _ = tx.send(Err(io::Error::other(REASONING_ONLY_COMPLETED)));
                            return;
                        }
                    }
                    if tx.send(Ok(bytes)).is_err() {
                        fail_pending(
                            pending,
                            "client disconnected before response completed",
                            &inventory,
                        );
                        return;
                    }
                }
                Err(error) => {
                    let message = error.to_string();
                    let _ = tx.send(Err(error));
                    fail_pending(pending, &message, &inventory);
                    return;
                }
            }
        }
        if let (Some(inspector), Some(current)) = (inspector, &inventory) {
            if inspector.finish(current) {
                fail_pending(pending, REASONING_ONLY_COMPLETED, &inventory);
                let _ = tx.send(Err(io::Error::other(REASONING_ONLY_COMPLETED)));
                return;
            }
        }
        succeed_pending(
            pending,
            status,
            sse,
            response_headers,
            response_bytes,
            &inventory,
        );
    });
    boxed_bytes_stream(receiver_byte_stream(rx))
}

impl ProviderProxy {
    fn state_root(&self) -> anyhow::Result<std::path::PathBuf> {
        self.inner
            .lock()
            .expect("provider proxy lock")
            .state_root
            .clone()
            .ok_or_else(|| anyhow::anyhow!("Helper state dir is unavailable"))
    }

    fn pending_llm_log(
        &self,
        path: &str,
        method: &hyper::Method,
        provider_id: &str,
        headers: &hyper::HeaderMap,
        body: &[u8],
    ) -> Option<PendingLlmLog> {
        if !is_llm_path(path) {
            return None;
        }
        let (logger, enabled) = {
            let state = self.inner.lock().expect("provider proxy lock");
            (state.logger.clone(), state.log_llm_traffic)
        };
        if !enabled {
            return None;
        }
        let Some(logger) = logger else {
            eprintln!("LLM traffic logging is enabled but no diagnostic logger is configured");
            return None;
        };
        Some(PendingLlmLog::start(
            logger,
            path.split('?').next().unwrap_or(path).to_string(),
            method.as_str().to_string(),
            provider_id.to_string(),
            headers,
            body,
        ))
    }

    fn authorize_request(
        &self,
        headers: &hyper::HeaderMap,
        provider: &Provider,
        providers: &ProviderStore,
    ) -> anyhow::Result<()> {
        let endpoint_store = match self.state_root() {
            Ok(root) => endpoint::read_store(&root)?,
            Err(_) => endpoint::EndpointStore::default(),
        };
        let bearer = headers
            .get(hyper::header::AUTHORIZATION)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| {
                value
                    .strip_prefix("Bearer ")
                    .or_else(|| value.strip_prefix("bearer "))
            });
        let active = providers
            .providers
            .iter()
            .find(|item| item.id == providers.active_id);
        let mut candidates = selected_api_providers(providers).into_iter().chain(active);
        if endpoint::authorize_bearer(&endpoint_store, bearer, provider).is_ok()
            || candidates.any(|candidate| {
                endpoint::authorize_bearer(&endpoint_store, bearer, candidate).is_ok()
            })
        {
            return Ok(());
        }
        let message = endpoint::authorize_bearer(&endpoint_store, bearer, provider)
            .err()
            .unwrap_or_else(|| "Invalid Endpoint API key".to_string());
        anyhow::bail!("Unauthorized: {message}");
    }

    fn current_store(&self) -> anyhow::Result<ProviderStore> {
        let (state_root, fallback) = {
            let state = self.inner.lock().expect("provider proxy lock");
            (state.state_root.clone(), state.store.clone())
        };
        if let Some(state_root) = state_root {
            return read_store(&state_root);
        }
        Ok(fallback)
    }

    fn log_protocol_downgrade(
        &self,
        provider_id: &str,
        model: &str,
        status: u16,
        retry_status: u16,
        excerpt: &str,
    ) {
        let logger = {
            let state = self.inner.lock().expect("provider proxy lock");
            state.logger.clone()
        };
        let Some(logger) = logger else {
            return;
        };
        let _ = logger.append(
            "provider.protocol_downgrade",
            serde_json::json!({
                "providerId": provider_id,
                "model": model,
                "trigger": "upstream_4xx_custom_tools",
                "status": status,
                "shapeBefore": "custom",
                "shapeAfter": "function",
                "retryStatus": retry_status,
                "errorExcerpt": excerpt,
            }),
        );
    }

    async fn send_upstream(
        &self,
        client: &reqwest::Client,
        method: hyper::Method,
        url: &str,
        headers: &hyper::HeaderMap,
        oauth_kind: Option<OAuthKind>,
        provider: &Provider,
        body: Vec<u8>,
    ) -> anyhow::Result<reqwest::Response> {
        let mut upstream_request = client.request(method, url);
        let skip_copilot_headers = oauth_kind == Some(OAuthKind::GithubCopilot);
        for (name, value) in headers.iter() {
            if matches!(
                name.as_str(),
                "host" | "content-length" | "authorization" | "connection" | "transfer-encoding"
            ) {
                continue;
            }
            if skip_copilot_headers
                && matches!(
                    name.as_str(),
                    "user-agent"
                        | "editor-version"
                        | "editor-plugin-version"
                        | "copilot-integration-id"
                        | "x-github-api-version"
                )
            {
                continue;
            }
            if let Ok(value) = value.to_str() {
                upstream_request = upstream_request.header(name.as_str(), value);
            }
        }
        if skip_copilot_headers {
            for (name, value) in copilot_request_headers() {
                upstream_request = upstream_request.header(name, value);
            }
        }
        let bearer = if let Some(kind) = oauth_kind {
            let state_root = self.state_root()?;
            oauth_bearer_token(&state_root, kind).await?
        } else if !provider.api_key.trim().is_empty() {
            provider.api_key.clone()
        } else {
            anyhow::bail!("Provider API key is required");
        };
        upstream_request =
            upstream_request.header("Authorization", authorization_header_value(&bearer));
        upstream_request
            .body(body)
            .send()
            .await
            .context("Provider upstream request failed")
    }
}

async fn resolve_call_auth(
    state_root: &std::path::Path,
    payload: &Value,
) -> anyhow::Result<(String, String, Option<OAuthKind>)> {
    let mut base_url = payload
        .get("baseUrl")
        .or_else(|| payload.get("base_url"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_string();
    let mut api_key = payload
        .get("apiKey")
        .or_else(|| payload.get("api_key"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_string();
    let mut compat = payload
        .get("compat")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_string();
    let auth_mode = payload
        .get("authMode")
        .or_else(|| payload.get("auth_mode"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim();
    if let Some(id) = payload
        .get("id")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        let store = read_store(state_root)?;
        let provider = store
            .providers
            .iter()
            .find(|provider| provider.id == id)
            .ok_or_else(|| anyhow::anyhow!("Provider not found: {id}"))?;
        if base_url.is_empty() {
            base_url = provider.base_url.clone();
        }
        if api_key.is_empty() || api_key == "********" {
            api_key = provider.api_key.clone();
        }
        if compat.is_empty() {
            compat = provider.compat.clone();
        }
    }
    let oauth_kind = OAuthKind::parse(auth_mode)
        .ok()
        .or_else(|| oauth_kind_from_provider(&compat, &base_url));
    if let Some(kind) = oauth_kind {
        let bearer = oauth_bearer_token(state_root, kind).await?;
        return Ok((kind.default_base_url().to_string(), bearer, Some(kind)));
    }
    if base_url.is_empty() {
        anyhow::bail!("Provider base URL is required");
    }
    if api_key.is_empty() || api_key == "********" {
        anyhow::bail!("Provider API key is required");
    }
    Ok((base_url, api_key, None))
}

fn is_responses_path(path: &str) -> bool {
    let path = path.split('?').next().unwrap_or(path);
    matches!(
        path,
        "/responses" | "/v1/responses" | "/responses/compact" | "/v1/responses/compact"
    )
}

fn is_llm_path(path: &str) -> bool {
    let path = path.split('?').next().unwrap_or(path);
    is_responses_path(path)
        || matches!(
            path,
            "/chat/completions" | "/v1/chat/completions" | "/completions" | "/v1/completions"
        )
}

pub async fn test_provider_connection(
    state_root: &std::path::Path,
    payload: &Value,
) -> anyhow::Result<(u16, String)> {
    let (base_url, bearer, oauth_kind) = resolve_call_auth(state_root, payload).await?;
    let url = join_provider_upstream_url_for(oauth_kind, &base_url, "/models");
    let client = reqwest::Client::builder()
        .no_proxy()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .context("Failed to build provider test client")?;
    let mut request = client.get(&url);
    if oauth_kind == Some(OAuthKind::GithubCopilot) {
        for (name, value) in copilot_request_headers() {
            request = request.header(name, value);
        }
    }
    if !bearer.is_empty() {
        request = request.header("Authorization", authorization_header_value(&bearer));
    }
    let response = request
        .send()
        .await
        .context("Provider test request failed")?;
    let status = response.status().as_u16();
    let text = response.text().await.unwrap_or_default();
    let preview: String = text.chars().take(180).collect();
    Ok((status, preview))
}

pub async fn fetch_provider_models(
    state_root: &std::path::Path,
    payload: &Value,
) -> anyhow::Result<Vec<String>> {
    let (base_url, bearer, oauth_kind) = resolve_call_auth(state_root, payload).await?;
    let url = join_provider_upstream_url_for(oauth_kind, &base_url, "/models");
    let client = reqwest::Client::builder()
        .no_proxy()
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .context("Failed to build provider models client")?;
    let mut request = client.get(&url);
    if oauth_kind == Some(OAuthKind::GithubCopilot) {
        request = request.header("Content-Type", "application/json");
        for (name, value) in copilot_request_headers() {
            request = request.header(name, value);
        }
    }
    if !bearer.is_empty() {
        request = request.header("Authorization", authorization_header_value(&bearer));
    }
    let response = request
        .send()
        .await
        .with_context(|| format!("Provider models request failed for {url}"))?;
    let status = response.status().as_u16();
    let text = response.text().await.unwrap_or_default();
    if !(200..300).contains(&status) {
        let preview: String = text.chars().take(180).collect();
        anyhow::bail!(if preview.is_empty() {
            format!("HTTP {status} from {url}")
        } else {
            format!("HTTP {status} from {url}: {preview}")
        });
    }
    let body: Value = serde_json::from_str(&text)
        .with_context(|| format!("Provider models response from {url} is not JSON"))?;
    let ids = collect_model_ids(&body);
    if ids.is_empty() {
        if let Some(detail) = provider_error_detail(&body) {
            anyhow::bail!("Provider models request for {url} returned no models: {detail}");
        }
    }
    Ok(ids)
}

/// Some gateways wrap failures in HTTP 200 responses (for example bigmodel's
/// `{"code":401,"msg":"...","success":false}`). Surface those payloads instead
/// of reporting an empty model list.
fn provider_error_detail(body: &Value) -> Option<String> {
    if let Some(error) = body.get("error").filter(|value| !value.is_null()) {
        let message = error
            .get("message")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty());
        let code = match error.get("code") {
            Some(Value::String(code)) => Some(code.clone()),
            Some(code) => Some(code.to_string()),
            None => None,
        };
        return Some(match (code, message) {
            (Some(code), Some(message)) => format!("provider error {code}: {message}"),
            (Some(code), None) => format!("provider error {code}"),
            (None, Some(message)) => format!("provider error: {message}"),
            (None, None) => "provider returned an error response".to_string(),
        });
    }
    let success = body.get("success").and_then(Value::as_bool);
    let code = body.get("code").and_then(Value::as_i64);
    let message = body
        .get("msg")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty());
    if success != Some(false) && !code.is_some_and(|value| value != 200) {
        return None;
    }
    Some(match (code, message) {
        (Some(code), Some(message)) => format!("provider error (code {code}): {message}"),
        (Some(code), None) => format!("provider error (code {code})"),
        (None, Some(message)) => format!("provider error: {message}"),
        (None, None) => "provider returned no models".to_string(),
    })
}

fn collect_model_ids(body: &Value) -> Vec<String> {
    let mut ids = Vec::new();
    let lists = [
        body.get("data").and_then(Value::as_array),
        body.get("models").and_then(Value::as_array),
    ];
    for list in lists.into_iter().flatten() {
        for item in list {
            let enabled = item
                .get("model_picker_enabled")
                .and_then(Value::as_bool)
                .unwrap_or(true);
            if !enabled {
                continue;
            }
            // OpenAI-style entries carry `id`; Codex model catalogs (bigmodel
            // /api/v1) identify models by `slug` instead.
            let id = item
                .get("id")
                .and_then(Value::as_str)
                .or_else(|| item.get("slug").and_then(Value::as_str))
                .map(str::trim);
            if let Some(id) = id.filter(|id| !id.is_empty()) {
                ids.push(id.to_string());
            }
        }
    }
    if let Some(list) = body.as_array() {
        for item in list {
            if let Some(id) = item
                .as_str()
                .or_else(|| item.get("id").and_then(Value::as_str))
            {
                let id = id.trim();
                if !id.is_empty() {
                    ids.push(id.to_string());
                }
            }
        }
    }
    ids.sort();
    ids.dedup();
    ids
}

fn authorization_header_value(bearer: &str) -> String {
    let trimmed = bearer.trim();
    if trimmed
        .get(..7)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("bearer "))
    {
        trimmed.to_string()
    } else {
        let mut value = String::from("Bearer ");
        value.push_str(trimmed);
        value
    }
}

fn join_provider_upstream_url_for(
    oauth_kind: Option<OAuthKind>,
    base_url: &str,
    path: &str,
) -> String {
    let path = if oauth_kind == Some(OAuthKind::GithubCopilot) {
        strip_leading_v1(path)
    } else {
        path.to_string()
    };
    join_provider_upstream_url(base_url, &path)
}

fn strip_leading_v1(path: &str) -> String {
    let (path, query) = match path.split_once('?') {
        Some((path, query)) => (path, Some(query)),
        None => (path, None),
    };
    let stripped = if path == "/v1" {
        "/".to_string()
    } else if let Some(rest) = path.strip_prefix("/v1/") {
        format!("/{rest}")
    } else {
        path.to_string()
    };
    match query {
        Some(query) => format!("{stripped}?{query}"),
        None => stripped,
    }
}

pub(crate) fn join_provider_upstream_url(base_url: &str, path: &str) -> String {
    let base = base_url.trim().trim_end_matches('/');
    let path = if path.starts_with('/') {
        path.to_string()
    } else {
        format!("/{path}")
    };
    let mut url = format!("{base}{path}");
    while url.contains("/v1/v1") {
        url = url.replace("/v1/v1", "/v1");
    }
    url
}

#[cfg(test)]
mod tests {
    use super::{
        collect_model_ids, is_llm_path, is_responses_path, join_provider_upstream_url,
        join_provider_upstream_url_for, provider_error_detail, ProviderProxy,
    };
    use crate::provider_oauth::OAuthKind;
    use crate::providers::{Provider, ProviderKind, ProviderStore};
    use serde_json::json;

    #[test]
    fn join_dedups_v1_when_base_and_path_both_include_it() {
        assert_eq!(
            join_provider_upstream_url("https://api.x.ai/v1", "/v1/responses"),
            "https://api.x.ai/v1/responses"
        );
    }

    #[test]
    fn join_keeps_single_v1_when_path_omits_it() {
        assert_eq!(
            join_provider_upstream_url("https://api.x.ai/v1", "/responses"),
            "https://api.x.ai/v1/responses"
        );
    }

    #[test]
    fn join_preserves_query_after_dedup() {
        assert_eq!(
            join_provider_upstream_url("https://api.x.ai/v1", "/v1/responses?stream=true"),
            "https://api.x.ai/v1/responses?stream=true"
        );
    }

    #[test]
    fn collect_model_ids_accepts_codex_catalog_slugs() {
        let body = json!({
            "models": [
                { "slug": "glm-5.3", "display_name": "glm-5.3" },
                { "slug": "glm-5.3-flash", "display_name": "GLM-5.3-Flash" },
                { "id": "extra-openai-model" }
            ]
        });
        assert_eq!(
            collect_model_ids(&body),
            vec![
                "extra-openai-model".to_string(),
                "glm-5.3".to_string(),
                "glm-5.3-flash".to_string()
            ]
        );
    }

    #[test]
    fn error_detail_exposes_http200_wrapped_failures() {
        assert_eq!(
            provider_error_detail(&json!({
                "code": 401,
                "msg": "令牌已过期或验证不正确",
                "success": false
            }))
            .expect("wrapped error"),
            "provider error (code 401): 令牌已过期或验证不正确"
        );
        assert_eq!(
            provider_error_detail(&json!({
                "error": { "code": "authorized_error", "message": "login fail (1004)" }
            }))
            .expect("openai-style error"),
            "provider error authorized_error: login fail (1004)"
        );
        assert!(provider_error_detail(&json!({ "data": [] })).is_none());
        assert!(provider_error_detail(&json!({ "code": 200, "data": [] })).is_none());
        assert!(provider_error_detail(&json!({ "error": null, "data": [] })).is_none());
    }

    #[tokio::test]
    async fn xai_proxy_rewrites_agent_message_before_upstream() {
        use super::*;
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        let mock = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("mock bind");
        let mock_port = mock.local_addr().expect("mock addr").port();
        let captured = tokio::spawn(async move {
            let (mut stream, _) = mock.accept().await.expect("mock accept");
            let mut buf = vec![0u8; 65536];
            let n = stream.read(&mut buf).await.expect("mock read");
            stream
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 2\r\n\r\n{}")
                .await
                .expect("mock write");
            buf[..n].to_vec()
        });

        let proxy = ProviderProxy::new();
        proxy.set_store(ProviderStore {
            active_id: "grok".to_string(),
            selected_ids: Vec::new(),
            providers: vec![Provider {
                id: "grok".to_string(),
                name: "Grok".to_string(),
                kind: ProviderKind::ApiKey,
                model: "grok-4.6".to_string(),
                base_url: format!("http://127.0.0.1:{mock_port}/v1"),
                wire_api: "responses".to_string(),
                api_key: "sk-test".to_string(),
                compat: "xai".to_string(),
                model_mappings: vec![crate::providers::ModelMapping {
                    source: "gpt-5.6-sol".to_string(),
                    target: "grok-4.6".to_string(),
                }],
                models: Vec::new(),
                catalog_models: Vec::new(),
                prefix_model_names: false,
                usage_page_url: String::new(),
                template: String::new(),
                usage_cookie_source: String::new(),
                usage_cookie_header: String::new(),
            }],
        });
        let port = proxy.bind_on(0).await.expect("proxy bind");
        let client = reqwest::Client::builder()
            .no_proxy()
            .build()
            .expect("client");
        let response = client
            .post(format!("http://127.0.0.1:{port}/v1/responses"))
            .json(&serde_json::json!({
                "model": "gpt-5.6-sol",
                "input": [{ "type": "agent_message", "content": "spawn a worker" }]
            }))
            .send()
            .await
            .expect("proxy request");
        assert_eq!(response.status(), 200);
        let raw = captured.await.expect("capture join");
        let text = String::from_utf8_lossy(&raw);
        let body = text.split("\r\n\r\n").nth(1).unwrap_or(text.as_ref());
        assert!(
            body.contains("\"type\":\"message\"") || body.contains("\"type\": \"message\""),
            "upstream body should rewrite agent_message, got {body}"
        );
        assert!(
            !body.contains("agent_message"),
            "upstream body should not keep agent_message, got {body}"
        );
        assert!(
            body.contains("grok-4.6"),
            "unknown SKU should remap to grok-4.6, got {body}"
        );
    }

    #[tokio::test]
    async fn xai_proxy_rewrites_whole_float_function_call_arguments() {
        use super::*;
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        let mock = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("mock bind");
        let mock_port = mock.local_addr().expect("mock addr").port();
        tokio::spawn(async move {
            let (mut stream, _) = mock.accept().await.expect("mock accept");
            let mut buf = vec![0u8; 65536];
            let _ = stream.read(&mut buf).await.expect("mock read");
            let payload = br#"{"output":[{"type":"function_call","name":"write_stdin","arguments":"{\"session_id\":92116.0}"}]}"#;
            let header = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n",
                payload.len()
            );
            stream.write_all(header.as_bytes()).await.expect("hdr");
            stream.write_all(payload).await.expect("body");
        });

        let proxy = ProviderProxy::new();
        proxy.set_store(ProviderStore {
            active_id: "grok".to_string(),
            selected_ids: Vec::new(),
            providers: vec![Provider {
                id: "grok".to_string(),
                name: "Grok".to_string(),
                kind: ProviderKind::ApiKey,
                model: "grok-4.6".to_string(),
                base_url: format!("http://127.0.0.1:{mock_port}/v1"),
                wire_api: "responses".to_string(),
                api_key: "sk-test".to_string(),
                compat: "xai".to_string(),
                model_mappings: Vec::new(),
                models: Vec::new(),
                catalog_models: Vec::new(),
                prefix_model_names: false,
                usage_page_url: String::new(),
                template: String::new(),
                usage_cookie_source: String::new(),
                usage_cookie_header: String::new(),
            }],
        });
        let port = proxy.bind_on(0).await.expect("proxy bind");
        let client = reqwest::Client::builder()
            .no_proxy()
            .build()
            .expect("client");
        let response = client
            .post(format!("http://127.0.0.1:{port}/v1/responses"))
            .json(&serde_json::json!({ "model": "grok-4.6", "input": [] }))
            .send()
            .await
            .expect("proxy request");
        assert_eq!(response.status(), 200);
        let body = response.text().await.expect("body");
        let parsed: serde_json::Value = serde_json::from_str(&body).expect("json");
        let arguments: serde_json::Value = serde_json::from_str(
            parsed["output"][0]["arguments"]
                .as_str()
                .expect("arguments"),
        )
        .expect("arguments json");
        assert_eq!(arguments["session_id"].as_i64(), Some(92116));
        assert!(
            !body.contains("92116.0"),
            "whole-float args should not keep .0, got {body}"
        );
    }

    #[tokio::test]
    async fn deepseek_proxy_rewrites_exec_custom_tool_before_upstream() {
        use super::*;
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        let mock = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("mock bind");
        let mock_port = mock.local_addr().expect("mock addr").port();
        let captured = tokio::spawn(async move {
            let (mut stream, _) = mock.accept().await.expect("mock accept");
            let mut buf = vec![0u8; 65536];
            let n = stream.read(&mut buf).await.expect("mock read");
            stream
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 2\r\n\r\n{}")
                .await
                .expect("mock write");
            buf[..n].to_vec()
        });

        let proxy = ProviderProxy::new();
        proxy.set_store(ProviderStore {
            active_id: "deepseek".to_string(),
            selected_ids: Vec::new(),
            providers: vec![Provider {
                id: "deepseek".to_string(),
                name: "DeepSeek".to_string(),
                kind: ProviderKind::ApiKey,
                model: "deepseek-v4-flash".to_string(),
                base_url: format!("http://127.0.0.1:{mock_port}/v1"),
                wire_api: "responses".to_string(),
                api_key: "sk-test".to_string(),
                compat: String::new(),
                model_mappings: Vec::new(),
                models: Vec::new(),
                catalog_models: Vec::new(),
                prefix_model_names: false,
                usage_page_url: String::new(),
                template: String::new(),
                usage_cookie_source: String::new(),
                usage_cookie_header: String::new(),
            }],
        });
        let port = proxy.bind_on(0).await.expect("proxy bind");
        let client = reqwest::Client::builder()
            .no_proxy()
            .build()
            .expect("client");
        let response = client
            .post(format!("http://127.0.0.1:{port}/v1/responses"))
            .json(&serde_json::json!({
                "model": "deepseek-v4-flash",
                "tools": [
                    { "type": "custom", "name": "exec" },
                    { "type": "custom", "name": "apply_patch" },
                    { "type": "function", "name": "read_file" }
                ]
            }))
            .send()
            .await
            .expect("proxy request");
        assert_eq!(response.status(), 200);
        let raw = captured.await.expect("capture join");
        let text = String::from_utf8_lossy(&raw);
        let body = text.split("\r\n\r\n").nth(1).unwrap_or(text.as_ref());
        let parsed: serde_json::Value = serde_json::from_str(body).expect("upstream json");
        let names: Vec<&str> = parsed["tools"]
            .as_array()
            .expect("tools")
            .iter()
            .map(|tool| {
                tool.get("name")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("")
            })
            .collect();
        let exec = parsed["tools"]
            .as_array()
            .expect("tools")
            .iter()
            .find(|tool| tool.get("name").and_then(Value::as_str) == Some("exec"))
            .expect("exec tool");
        assert_eq!(
            exec["type"], "function",
            "exec must be rewritten, got {body}"
        );
        let apply_patch = parsed["tools"]
            .as_array()
            .expect("tools")
            .iter()
            .find(|tool| tool.get("name").and_then(Value::as_str) == Some("apply_patch"))
            .expect("apply_patch tool");
        assert_eq!(
            apply_patch["type"], "custom",
            "apply_patch must stay native, got {body}"
        );
        assert!(
            names.contains(&"read_file"),
            "function tools must survive, got {body}"
        );
    }

    #[tokio::test]
    async fn deepseek_proxy_rewrites_unknown_model_to_default() {
        use super::*;
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        let mock = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("mock bind");
        let mock_port = mock.local_addr().expect("mock addr").port();
        let captured = tokio::spawn(async move {
            let (mut stream, _) = mock.accept().await.expect("mock accept");
            let mut buf = vec![0u8; 65536];
            let n = stream.read(&mut buf).await.expect("mock read");
            stream
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 2\r\n\r\n{}")
                .await
                .expect("mock write");
            buf[..n].to_vec()
        });

        let proxy = ProviderProxy::new();
        proxy.set_store(ProviderStore {
            active_id: "deepseek".to_string(),
            selected_ids: Vec::new(),
            providers: vec![Provider {
                id: "deepseek".to_string(),
                name: "DeepSeek".to_string(),
                kind: ProviderKind::ApiKey,
                model: "deepseek-v4-flash".to_string(),
                base_url: format!("http://127.0.0.1:{mock_port}/v1"),
                wire_api: "responses".to_string(),
                api_key: "sk-test".to_string(),
                compat: String::new(),
                model_mappings: Vec::new(),
                models: vec!["deepseek-chat".to_string()],
                catalog_models: Vec::new(),
                prefix_model_names: false,
                usage_page_url: String::new(),
                template: String::new(),
                usage_cookie_source: String::new(),
                usage_cookie_header: String::new(),
            }],
        });
        let port = proxy.bind_on(0).await.expect("proxy bind");
        let client = reqwest::Client::builder()
            .no_proxy()
            .build()
            .expect("client");
        let response = client
            .post(format!("http://127.0.0.1:{port}/v1/responses"))
            .json(&serde_json::json!({
                "model": "gpt-5.6-luna",
                "input": []
            }))
            .send()
            .await
            .expect("proxy request");
        assert_eq!(response.status(), 200);
        let raw = captured.await.expect("capture join");
        let text = String::from_utf8_lossy(&raw);
        let body = text.split("\r\n\r\n").nth(1).unwrap_or(text.as_ref());
        assert!(
            body.contains("deepseek-v4-flash"),
            "unknown SKU should remap to default, got {body}"
        );
        assert!(
            !body.contains("gpt-5.6-luna"),
            "unknown SKU should not reach upstream, got {body}"
        );
    }

    #[tokio::test]
    async fn kimi_proxy_rewrites_unknown_model_on_chat_completions() {
        use super::*;
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        let mock = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("mock bind");
        let mock_port = mock.local_addr().expect("mock addr").port();
        let captured = tokio::spawn(async move {
            let (mut stream, _) = mock.accept().await.expect("mock accept");
            let mut buf = vec![0u8; 65536];
            let n = stream.read(&mut buf).await.expect("mock read");
            stream
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 2\r\n\r\n{}")
                .await
                .expect("mock write");
            buf[..n].to_vec()
        });

        let proxy = ProviderProxy::new();
        proxy.set_store(ProviderStore {
            active_id: "kimi".to_string(),
            selected_ids: Vec::new(),
            providers: vec![Provider {
                id: "kimi".to_string(),
                name: "Kimi".to_string(),
                kind: ProviderKind::ApiKey,
                model: "kimi-k2.5".to_string(),
                base_url: format!("http://127.0.0.1:{mock_port}/v1"),
                wire_api: "chat_completions".to_string(),
                api_key: "sk-test".to_string(),
                compat: String::new(),
                model_mappings: Vec::new(),
                models: Vec::new(),
                catalog_models: Vec::new(),
                prefix_model_names: false,
                usage_page_url: String::new(),
                template: String::new(),
                usage_cookie_source: String::new(),
                usage_cookie_header: String::new(),
            }],
        });
        let port = proxy.bind_on(0).await.expect("proxy bind");
        let client = reqwest::Client::builder()
            .no_proxy()
            .build()
            .expect("client");
        let response = client
            .post(format!("http://127.0.0.1:{port}/v1/chat/completions"))
            .json(&serde_json::json!({
                "model": "gpt-5.6-sol",
                "messages": [{ "role": "user", "content": "hi" }]
            }))
            .send()
            .await
            .expect("proxy request");
        assert_eq!(response.status(), 200);
        let raw = captured.await.expect("capture join");
        let text = String::from_utf8_lossy(&raw);
        let body = text.split("\r\n\r\n").nth(1).unwrap_or(text.as_ref());
        assert!(
            body.contains("kimi-k2.5"),
            "unknown SKU should remap to default, got {body}"
        );
        assert!(
            !body.contains("gpt-5.6-sol"),
            "unknown SKU should not reach upstream, got {body}"
        );
    }

    #[tokio::test]
    async fn bigmodel_proxy_rewrites_codex_effort_to_provider_level() {
        let (mock_port, captured) = serve_mock_http("200 OK", b"{}").await;
        let mut provider = test_api_provider("bigmodel", "GLM-5.3-Flash", mock_port);
        provider.wire_api = "responses".to_string();
        provider.template = "bigmodel".to_string();
        let proxy = ProviderProxy::new();
        proxy.set_store(ProviderStore {
            active_id: "bigmodel".to_string(),
            selected_ids: Vec::new(),
            providers: vec![provider],
        });
        let port = proxy.bind_on(0).await.expect("proxy bind");
        let client = reqwest::Client::builder()
            .no_proxy()
            .build()
            .expect("client");
        let response = client
            .post(format!("http://127.0.0.1:{port}/v1/responses"))
            .json(&serde_json::json!({
                "model": "GLM-5.3-Flash",
                "input": "hi",
                "reasoning": { "effort": "xhigh" }
            }))
            .send()
            .await
            .expect("proxy request");
        assert_eq!(response.status(), 200);
        let raw = captured.await.expect("capture join");
        let text = String::from_utf8_lossy(&raw);
        let body = text.split("\r\n\r\n").nth(1).unwrap_or(text.as_ref());
        let upstream: serde_json::Value = serde_json::from_str(body).expect("upstream body");
        assert_eq!(upstream["reasoning"]["effort"], "max");
    }

    #[test]
    fn responses_compat_skips_chat_completions_path() {
        assert!(is_responses_path("/v1/responses"));
        assert!(is_responses_path("/responses"));
        assert!(!is_responses_path("/v1/chat/completions"));
        assert!(is_llm_path("/v1/chat/completions"));
        assert!(is_llm_path("/v1/responses"));
    }

    #[test]
    fn collect_model_ids_reads_data_and_skips_disabled_picker_models() {
        let ids = collect_model_ids(&json!({
            "data": [
                { "id": "gpt-4.1", "model_picker_enabled": true },
                { "id": "hidden", "model_picker_enabled": false },
                { "id": "claude-sonnet-5" }
            ]
        }));
        assert_eq!(ids, vec!["claude-sonnet-5", "gpt-4.1"]);
    }

    async fn wait_for_llm_log(logger: &crate::logging::DiagnosticLogger) -> serde_json::Value {
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(2);
        loop {
            if let Ok(page) = logger.read_latest() {
                if let Some(record) = page
                    .records
                    .iter()
                    .find(|record| record.event == "llm.request")
                {
                    return record.detail.clone();
                }
            }
            if tokio::time::Instant::now() >= deadline {
                panic!("timed out waiting for llm.request log");
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    }

    fn temp_logger() -> (
        tempfile::TempDir,
        std::sync::Arc<crate::logging::DiagnosticLogger>,
    ) {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let logger = std::sync::Arc::new(crate::logging::DiagnosticLogger::new(
            temp_dir.path().join("logs"),
        ));
        (temp_dir, logger)
    }

    fn test_api_provider(id: &str, model: &str, mock_port: u16) -> Provider {
        Provider {
            id: id.to_string(),
            name: id.to_string(),
            kind: ProviderKind::ApiKey,
            model: model.to_string(),
            base_url: format!("http://127.0.0.1:{mock_port}/v1"),
            wire_api: "chat_completions".to_string(),
            api_key: "sk-test".to_string(),
            compat: String::new(),
            model_mappings: Vec::new(),
            models: Vec::new(),
            catalog_models: Vec::new(),
            prefix_model_names: false,
            usage_page_url: String::new(),
            template: String::new(),
            usage_cookie_source: String::new(),
            usage_cookie_header: String::new(),
        }
    }

    async fn serve_mock_http(status: &str, body: &[u8]) -> (u16, tokio::task::JoinHandle<Vec<u8>>) {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        let mock = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("mock bind");
        let mock_port = mock.local_addr().expect("mock addr").port();
        let header = format!(
            "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n",
            body.len()
        );
        let body = body.to_vec();
        let captured = tokio::spawn(async move {
            let (mut stream, _) = mock.accept().await.expect("mock accept");
            let mut buf = vec![0u8; 65536];
            let n = stream.read(&mut buf).await.expect("mock read");
            stream
                .write_all(header.as_bytes())
                .await
                .expect("mock header");
            stream.write_all(&body).await.expect("mock body");
            buf[..n].to_vec()
        });
        (mock_port, captured)
    }

    async fn bind_kimi_proxy(
        logger: &std::sync::Arc<crate::logging::DiagnosticLogger>,
        enabled: bool,
        mock_port: u16,
    ) -> (ProviderProxy, u16) {
        let proxy = ProviderProxy::new();
        proxy.set_logger(logger.clone());
        proxy.set_log_llm_traffic(enabled);
        proxy.set_store(ProviderStore {
            active_id: "kimi".to_string(),
            selected_ids: Vec::new(),
            providers: vec![test_api_provider("kimi", "kimi-k2.5", mock_port)],
        });
        let port = proxy.bind_on(0).await.expect("proxy bind");
        (proxy, port)
    }

    #[tokio::test]
    async fn outgoing_body_records_stream_error() {
        let (_temp_dir, logger) = temp_logger();
        let pending = crate::llm_traffic_log::PendingLlmLog::start(
            logger.clone(),
            "/v1/responses".to_string(),
            "POST".to_string(),
            "kimi".to_string(),
            &hyper::HeaderMap::new(),
            b"{}",
        );
        let stream = futures_util::stream::iter([
            Ok(bytes::Bytes::from_static(b"hello")),
            Err(std::io::Error::other("upstream reset")),
        ]);
        let body = super::outgoing_body(stream, Some(pending), 200, true, json!({}), None, false);
        let collected = http_body_util::BodyExt::collect(body).await;
        assert!(
            collected.is_err(),
            "stream error should surface to the client"
        );
        let detail = wait_for_llm_log(&logger).await;
        assert_eq!(detail["error"], "upstream reset");
        assert_eq!(detail["status"], 0);
        assert_eq!(detail["compat"]["suspect"], true);
        assert_eq!(detail["compat"]["reasons"], json!(["stream_error"]));
        assert!(detail["request"].get("body").is_none());
        assert!(detail["response"].get("body").is_none());
    }

    #[test]
    fn rewrite_inspected_sse_block_records_object_arguments_without_values() {
        use super::{rewrite_inspected_sse_block, NativeRestore};
        use crate::llm_compat_inventory::ResponseCompatInventory;
        use crate::xai_sanitize::XaiNativeRestoreMap;
        use std::sync::{Arc, Mutex};

        let restore = NativeRestore::Xai(XaiNativeRestoreMap::default());
        let inventory = Arc::new(Mutex::new(ResponseCompatInventory::default()));
        let block = r#"data: {"type":"function_call","name":"view_image","call_id":"c1","arguments":{"path":"/secret/cursor.png"}}"#;
        let rewritten = rewrite_inspected_sse_block(block, &restore, Some(&inventory));
        let rewritten_text = String::from_utf8(rewritten.to_vec()).expect("utf8");
        let data = rewritten_text
            .lines()
            .find(|line| line.starts_with("data:"))
            .expect("data line")
            .trim_start_matches("data:")
            .trim();
        let parsed: serde_json::Value = serde_json::from_str(data).expect("json");
        assert_eq!(parsed["name"], "view_image");
        assert!(
            parsed["arguments"].is_string(),
            "object arguments should stringify, got {parsed}"
        );

        let compat = inventory
            .lock()
            .expect("inventory")
            .log_value(200, None)
            .expect("compat");
        let rendered = compat.to_string();
        assert_eq!(compat["functionCalls"][0]["name"], "view_image");
        assert_eq!(compat["functionCalls"][0]["argumentsKind"], "object");
        assert_eq!(compat["functionCalls"][0]["rewritten"], true);
        assert!(compat["reasons"]
            .as_array()
            .expect("reasons")
            .iter()
            .any(|reason| reason == "arguments_object"));
        assert!(!rendered.contains("/secret/cursor.png"));
        assert!(!rendered.contains("cursor.png"));
    }

    #[tokio::test]
    async fn outgoing_body_fails_reasoning_only_completed_sse() {
        let (_temp_dir, logger) = temp_logger();
        let pending = crate::llm_traffic_log::PendingLlmLog::start(
            logger.clone(),
            "/v1/responses".to_string(),
            "POST".to_string(),
            "xai".to_string(),
            &hyper::HeaderMap::new(),
            b"{}",
        );
        let inventory = std::sync::Arc::new(std::sync::Mutex::new(
            crate::llm_compat_inventory::ResponseCompatInventory::default(),
        ));
        let stream = futures_util::stream::iter([
            Ok(bytes::Bytes::from_static(
                b"event: response.output_item.done\ndata: {\"type\":\"reasoning\"}\n\n",
            )),
            Ok(bytes::Bytes::from_static(
                b"event: response.completed\ndata: {\"type\":\"response.completed\"}\n\n",
            )),
        ]);
        let body = super::outgoing_body(
            stream,
            Some(pending),
            200,
            true,
            json!({}),
            Some(inventory),
            true,
        );
        let collected = http_body_util::BodyExt::collect(body).await;
        assert!(
            collected.is_err(),
            "reasoning-only completed SSE should fail the client stream"
        );
        let detail = wait_for_llm_log(&logger).await;
        assert_eq!(detail["error"], super::REASONING_ONLY_COMPLETED);
        assert_eq!(detail["status"], 0);
        assert_eq!(detail["compat"]["suspect"], true);
        let reasons = detail["compat"]["reasons"].as_array().expect("reasons");
        assert!(reasons.iter().any(|reason| reason == "stream_error"));
        assert!(reasons
            .iter()
            .any(|reason| reason == "reasoning_without_output"));
        assert!(detail["request"].get("body").is_none());
        assert!(detail["response"].get("body").is_none());
    }

    #[tokio::test]
    async fn outgoing_body_forwards_reasoning_when_function_call_completed() {
        let (_temp_dir, logger) = temp_logger();
        let pending = crate::llm_traffic_log::PendingLlmLog::start(
            logger.clone(),
            "/v1/responses".to_string(),
            "POST".to_string(),
            "xai".to_string(),
            &hyper::HeaderMap::new(),
            b"{}",
        );
        let inventory = std::sync::Arc::new(std::sync::Mutex::new(
            crate::llm_compat_inventory::ResponseCompatInventory::default(),
        ));
        let stream = futures_util::stream::iter([
            Ok(bytes::Bytes::from_static(
                b"data: {\"type\":\"reasoning\"}\n\n",
            )),
            Ok(bytes::Bytes::from_static(
                b"data: {\"type\":\"function_call\",\"name\":\"exec\",\"arguments\":\"{}\"}\n\n",
            )),
            Ok(bytes::Bytes::from_static(
                b"event: response.completed\ndata: {\"type\":\"response.completed\"}\n\n",
            )),
        ]);
        let body = super::outgoing_body(
            stream,
            Some(pending),
            200,
            true,
            json!({}),
            Some(inventory),
            true,
        );
        let collected = http_body_util::BodyExt::collect(body)
            .await
            .expect("complete stream")
            .to_bytes();
        let text = String::from_utf8(collected.to_vec()).expect("utf8");
        assert!(text.contains("response.completed"));
        let detail = wait_for_llm_log(&logger).await;
        assert!(detail.get("error").is_none());
        assert_eq!(detail["status"], 200);
        assert!(detail["request"].get("body").is_none());
        assert!(detail["response"].get("body").is_none());
    }

    #[tokio::test]
    async fn llm_log_disabled_writes_nothing() {
        let (mock_port, _captured) = serve_mock_http("200 OK", b"{}").await;
        let (_temp_dir, logger) = temp_logger();
        let (_proxy, port) = bind_kimi_proxy(&logger, false, mock_port).await;
        let client = reqwest::Client::builder()
            .no_proxy()
            .build()
            .expect("client");
        let response = client
            .post(format!("http://127.0.0.1:{port}/v1/chat/completions"))
            .header("Authorization", "Bearer inbound-secret")
            .json(&json!({
                "model": "kimi-k2.5",
                "messages": [{ "role": "user", "content": "hi" }]
            }))
            .send()
            .await
            .expect("proxy request");
        assert_eq!(response.status(), 200);
        let _ = response.bytes().await.expect("body");
        tokio::time::sleep(std::time::Duration::from_millis(150)).await;
        let page = logger.read_latest().expect("latest");
        assert!(
            page.records
                .iter()
                .all(|record| record.event != "llm.request"),
            "disabled logging should not write llm.request"
        );
    }

    #[tokio::test]
    async fn llm_log_redacts_secrets_and_records_every_call() {
        let payload = br#"{"id":"chatcmpl-1","api_key":"sk-upstream"}"#;
        let (mock_port, captured) = serve_mock_http("500 Internal Server Error", payload).await;
        let (_temp_dir, logger) = temp_logger();
        let (_proxy, port) = bind_kimi_proxy(&logger, true, mock_port).await;
        let client = reqwest::Client::builder()
            .no_proxy()
            .build()
            .expect("client");
        let response = client
            .post(format!("http://127.0.0.1:{port}/v1/chat/completions"))
            .header("Authorization", "Bearer inbound-secret")
            .json(&json!({
                "model": "kimi-k2.5",
                "api_key": "sk-request",
                "messages": [{ "role": "user", "content": "hi" }]
            }))
            .send()
            .await
            .expect("proxy request");
        assert_eq!(response.status(), 500);
        let _ = response.bytes().await.expect("body");
        let _ = captured.await.expect("capture join");
        let detail = wait_for_llm_log(&logger).await;
        let rendered = detail.to_string();
        assert_eq!(detail["path"], "/v1/chat/completions");
        assert_eq!(detail["method"], "POST");
        assert_eq!(detail["status"], 500);
        assert_eq!(detail["providerId"], "kimi");
        assert_eq!(detail["model"], "kimi-k2.5");
        assert_eq!(detail["userPreview"], "hi");
        assert!(detail["request"].get("body").is_none());
        assert!(detail["response"].get("body").is_none());
        assert!(!rendered.contains("inbound-secret"));
        assert!(!rendered.contains("sk-test"));
        assert!(!rendered.contains("sk-request"));
        assert!(!rendered.contains("sk-upstream"));
        assert!(detail["request"]["headers"].get("authorization").is_none());
    }

    #[tokio::test]
    async fn llm_log_skips_non_llm_paths() {
        let (mock_port, _captured) = serve_mock_http("200 OK", br#"{"data":[]}"#).await;
        let (_temp_dir, logger) = temp_logger();
        let (_proxy, port) = bind_kimi_proxy(&logger, true, mock_port).await;
        let client = reqwest::Client::builder()
            .no_proxy()
            .build()
            .expect("client");
        let response = client
            .get(format!("http://127.0.0.1:{port}/v1/models"))
            .send()
            .await
            .expect("proxy request");
        assert_eq!(response.status(), 200);
        let _ = response.bytes().await.expect("body");
        tokio::time::sleep(std::time::Duration::from_millis(150)).await;
        let page = logger.read_latest().expect("latest");
        assert!(
            page.records
                .iter()
                .all(|record| record.event != "llm.request"),
            "non-LLM paths should not write llm.request"
        );
    }

    #[test]
    fn copilot_join_strips_v1_prefix() {
        assert_eq!(
            join_provider_upstream_url_for(
                Some(OAuthKind::GithubCopilot),
                "https://api.githubcopilot.com",
                "/v1/chat/completions",
            ),
            "https://api.githubcopilot.com/chat/completions"
        );
        assert_eq!(
            join_provider_upstream_url_for(
                Some(OAuthKind::GithubCopilot),
                "https://api.githubcopilot.com",
                "/v1/models",
            ),
            "https://api.githubcopilot.com/models"
        );
    }

    async fn read_http_request(stream: &mut tokio::net::TcpStream) -> Vec<u8> {
        use tokio::io::AsyncReadExt;

        let mut buf = Vec::new();
        let mut tmp = [0u8; 4096];
        loop {
            let n = stream.read(&mut tmp).await.expect("mock read");
            if n == 0 {
                break;
            }
            buf.extend_from_slice(&tmp[..n]);
            let Some(header_end) = buf.windows(4).position(|window| window == b"\r\n\r\n") else {
                continue;
            };
            let header = String::from_utf8_lossy(&buf[..header_end]);
            let length = header
                .lines()
                .find_map(|line| {
                    let rest = line
                        .strip_prefix("Content-Length:")
                        .or_else(|| line.strip_prefix("content-length:"))?;
                    rest.trim().parse::<usize>().ok()
                })
                .unwrap_or(0);
            if buf.len() >= header_end + 4 + length {
                break;
            }
        }
        buf
    }

    fn routed_provider(id: &str, model: &str, key: &str, base_url: &str) -> Provider {
        Provider {
            id: id.to_string(),
            name: id.to_string(),
            kind: ProviderKind::ApiKey,
            model: model.to_string(),
            base_url: base_url.to_string(),
            wire_api: "responses".to_string(),
            api_key: key.to_string(),
            compat: String::new(),
            model_mappings: Vec::new(),
            models: vec![model.to_string()],
            catalog_models: Vec::new(),
            prefix_model_names: false,
            usage_page_url: String::new(),
            template: String::new(),
            usage_cookie_source: String::new(),
            usage_cookie_header: String::new(),
        }
    }

    #[tokio::test]
    async fn proxy_routes_namespaced_model_to_its_provider() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        let mock = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("mock bind");
        let mock_port = mock.local_addr().expect("mock addr").port();
        let captured = tokio::spawn(async move {
            let (mut stream, _) = mock.accept().await.expect("mock accept");
            let mut buf = vec![0u8; 65536];
            let n = stream.read(&mut buf).await.expect("mock read");
            stream
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 2\r\n\r\n{}")
                .await
                .expect("mock write");
            buf[..n].to_vec()
        });
        let base = format!("http://127.0.0.1:{mock_port}/v1");
        let proxy = ProviderProxy::new();
        proxy.set_store(ProviderStore {
            active_id: "grok".to_string(),
            selected_ids: vec!["grok".to_string(), "mimo".to_string()],
            providers: vec![
                routed_provider("grok", "grok-4.7", "sk-grok", &base),
                routed_provider("mimo", "mimo-v2.6-pro", "sk-mimo", &base),
            ],
        });
        let port = proxy.bind_on(0).await.expect("proxy bind");
        let client = reqwest::Client::builder()
            .no_proxy()
            .build()
            .expect("client");
        let response = client
            .post(format!("http://127.0.0.1:{port}/v1/responses"))
            .json(&serde_json::json!({ "model": "mimo::mimo-v2.6-pro", "input": "ping" }))
            .send()
            .await
            .expect("proxy request");
        assert_eq!(response.status(), 200);
        let raw = captured.await.expect("capture join");
        let text = String::from_utf8_lossy(&raw);
        assert!(
            text.contains("Bearer sk-mimo"),
            "routed key missing: {text}"
        );
        assert!(
            text.contains("mimo-v2.6-pro"),
            "upstream model missing: {text}"
        );
        assert!(
            !text.contains("mimo::mimo-v2.6-pro"),
            "namespace leaked: {text}"
        );
    }

    #[tokio::test]
    async fn proxy_retries_custom_tool_rejection_as_function_tools() {
        use tokio::io::AsyncWriteExt;

        let mock = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("mock bind");
        let mock_port = mock.local_addr().expect("mock addr").port();
        let captured = tokio::spawn(async move {
            let mut requests = Vec::new();
            for attempt in 0..2 {
                let (mut stream, _) = mock.accept().await.expect("mock accept");
                let request = read_http_request(&mut stream).await;
                requests.push(request);
                if attempt == 0 {
                    let body = br#"{"error":{"message":"custom tools require MiMo freeform Responses lite mode."}}"#;
                    let header = format!(
                        "HTTP/1.1 400 Bad Request\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n",
                        body.len()
                    );
                    stream
                        .write_all(header.as_bytes())
                        .await
                        .expect("400 header");
                    stream.write_all(body).await.expect("400 body");
                } else {
                    let body = br#"{"output":[{"type":"function_call","name":"exec","call_id":"call_1","arguments":"{\"input\":\"ls\"}"}]}"#;
                    let header = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n",
                        body.len()
                    );
                    stream
                        .write_all(header.as_bytes())
                        .await
                        .expect("200 header");
                    stream.write_all(body).await.expect("200 body");
                }
                stream.shutdown().await.expect("shutdown");
            }
            requests
        });
        let proxy = ProviderProxy::new();
        proxy.set_store(ProviderStore {
            active_id: "mimo".to_string(),
            selected_ids: vec!["mimo".to_string()],
            providers: vec![routed_provider(
                "mimo",
                "mimo-v2.6-flash",
                "sk-mimo",
                &format!("http://127.0.0.1:{mock_port}/v1"),
            )],
        });
        let port = proxy.bind_on(0).await.expect("proxy bind");
        let client = reqwest::Client::builder()
            .no_proxy()
            .build()
            .expect("client");
        let response = client
            .post(format!("http://127.0.0.1:{port}/v1/responses"))
            .json(&serde_json::json!({
                "model": "mimo-v2.6-flash",
                "tools": [{ "type": "custom", "name": "exec" }]
            }))
            .send()
            .await
            .expect("proxy request");
        let status = response.status();
        let body = response.text().await.expect("body");
        assert_eq!(status, 200, "{body}");
        assert!(body.contains("custom_tool_call"), "restore missing: {body}");
        let requests = captured.await.expect("capture join");
        let first = String::from_utf8_lossy(&requests[0]);
        let second = String::from_utf8_lossy(&requests[1]);
        assert!(
            first.contains("\"type\":\"custom\"") || first.contains("\"type\": \"custom\""),
            "{first}"
        );
        let second_body = second.split("\r\n\r\n").nth(1).unwrap_or(second.as_ref());
        let parsed: serde_json::Value = serde_json::from_str(second_body).expect("retry json");
        assert_eq!(parsed["tools"][0]["type"], "function");
        assert_eq!(parsed["tools"][0]["name"], "exec");
    }
}

static PROVIDER_PROXY: OnceLock<ProviderProxy> = OnceLock::new();

pub fn global_provider_proxy() -> ProviderProxy {
    PROVIDER_PROXY.get_or_init(ProviderProxy::new).clone()
}
