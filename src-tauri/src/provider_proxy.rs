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

use crate::deepseek_sanitize::{
    apply_deepseek_responses_request_compat, rewrite_deepseek_native_json_bytes,
    rewrite_deepseek_native_sse_block, DeepSeekRestoreMap,
};
use crate::endpoint;
use crate::provider_oauth::{
    copilot_request_headers, oauth_bearer_token, oauth_kind_from_provider, OAuthKind,
};
use crate::providers::{
    apply_provider_model_mappings, provider_allowed_models, provider_device_oauth_kind,
    provider_needs_deepseek_responses_sanitize, provider_needs_xai_compat, read_store,
    rewrite_unmatched_request_model, Provider, ProviderKind, ProviderStore,
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
}

fn rewrite_native_sse_block(block: &str, restore: &NativeRestore) -> Bytes {
    match restore {
        NativeRestore::None => Bytes::from(format!("{block}\n\n")),
        NativeRestore::Xai(map) => rewrite_xai_native_sse_block(block, map),
        NativeRestore::DeepSeek(map) => rewrite_deepseek_native_sse_block(block, map),
    }
}

fn rewrite_native_json_bytes(bytes: &[u8], restore: &NativeRestore) -> Vec<u8> {
    match restore {
        NativeRestore::None => bytes.to_vec(),
        NativeRestore::Xai(map) => rewrite_xai_native_json_bytes(bytes, map),
        NativeRestore::DeepSeek(map) => rewrite_deepseek_native_json_bytes(bytes, map),
    }
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
}

impl ProviderProxy {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(ProxyState {
                port: 0,
                store: ProviderStore::default(),
                state_root: None,
                bind_error: None,
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
        let provider = self
            .active_provider()?
            .ok_or_else(|| anyhow::anyhow!("No active provider"))?;
        if provider.id == "official" || provider.kind == ProviderKind::Oauth {
            anyhow::bail!("Official ChatGPT login does not use the Helper provider proxy");
        }
        let upstream = provider.base_url.trim().trim_end_matches('/');
        if upstream.is_empty() {
            anyhow::bail!("Active provider has no base URL");
        }
        let path = request
            .uri()
            .path_and_query()
            .map(|value| value.as_str().to_string())
            .unwrap_or_else(|| "/".to_string());
        let oauth_kind = provider_device_oauth_kind(&provider);
        let upstream = oauth_kind
            .map(|kind| kind.default_base_url().to_string())
            .unwrap_or_else(|| upstream.to_string());
        let url = join_provider_upstream_url_for(oauth_kind, &upstream, &path);
        let method = request.method().clone();
        let headers = request.headers().clone();
        self.authorize_request(&headers, &provider)?;
        let raw_body = request.collect().await?.to_bytes();
        let mut body = raw_body.to_vec();
        let xai_compat = provider_needs_xai_compat(&provider);
        let deepseek_sanitize = provider_needs_deepseek_responses_sanitize(&provider);
        let responses_path = is_responses_path(&path);
        let xai_request = xai_compat && responses_path;
        let deepseek_request = deepseek_sanitize && responses_path;
        let rewrite = method == hyper::Method::POST && is_llm_path(&path);
        let mut restore = NativeRestore::None;
        if rewrite {
            let mut json_body = serde_json::from_slice::<Value>(&body)
                .context("Provider request is not valid JSON")?;
            apply_provider_model_mappings(&mut json_body, &provider.model_mappings);
            rewrite_unmatched_request_model(
                &mut json_body,
                &provider.model,
                &provider_allowed_models(&provider),
            );
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
            body = serde_json::to_vec(&json_body)?;
        }
        let client = reqwest::Client::builder()
            .no_proxy()
            .build()
            .context("Failed to build provider proxy client")?;
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
        let response = upstream_request
            .body(body)
            .send()
            .await
            .context("Provider upstream request failed")?;
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
        if !matches!(restore, NativeRestore::None) && status.is_success() {
            if is_sse {
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
                                    if tx
                                        .send(Ok(rewrite_native_sse_block(&block, &restore)))
                                        .is_err()
                                    {
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
                        let _ = tx.send(Ok(rewrite_native_sse_block(&buffer, &restore)));
                    }
                });
                let rx_stream = futures_util::stream::unfold(rx, |mut rx| async move {
                    rx.recv().await.map(|item| (item, rx))
                })
                .map(|item| item.map(Frame::data));
                return Ok(builder.body(BodyExt::boxed(StreamBody::new(rx_stream)))?);
            }
            let body_bytes = response
                .bytes()
                .await
                .context("Failed to read provider upstream body")?;
            let rewritten = rewrite_native_json_bytes(&body_bytes, &restore);
            return Ok(builder.body(bytes_body(rewritten))?);
        }
        let stream = response
            .bytes_stream()
            .map_err(|error| io::Error::other(error.to_string()))
            .map_ok(Frame::data);
        Ok(builder.body(BodyExt::boxed(StreamBody::new(stream)))?)
    }
}

fn bytes_body(bytes: impl Into<Bytes>) -> ProxyBody {
    Full::new(bytes.into())
        .map_err(|infallible: Infallible| match infallible {})
        .boxed()
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

    fn authorize_request(
        &self,
        headers: &hyper::HeaderMap,
        provider: &Provider,
    ) -> anyhow::Result<()> {
        let store = match self.state_root() {
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
        if let Err(message) = endpoint::authorize_bearer(&store, bearer, provider) {
            anyhow::bail!("Unauthorized: {message}");
        }
        Ok(())
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
    Ok(collect_model_ids(&body))
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
            if let Some(id) = item.get("id").and_then(Value::as_str).map(str::trim) {
                if !id.is_empty() {
                    ids.push(id.to_string());
                }
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
        join_provider_upstream_url_for,
    };
    use crate::provider_oauth::OAuthKind;
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
                usage_page_url: String::new(),
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
                usage_page_url: String::new(),
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
                usage_page_url: String::new(),
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
                usage_page_url: String::new(),
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
                usage_page_url: String::new(),
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
}

static PROVIDER_PROXY: OnceLock<ProviderProxy> = OnceLock::new();

pub fn global_provider_proxy() -> ProviderProxy {
    PROVIDER_PROXY.get_or_init(ProviderProxy::new).clone()
}
