use std::sync::Arc;

use serde_json::{json, Value};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use uuid::Uuid;

use crate::cdp::{connect_cdp_websocket, CdpSession};
use crate::routes::{handle_bridge_request, BridgeContext};

#[derive(Debug, Clone)]
pub struct BridgeServer {
    port: u16,
    token: String,
}

impl BridgeServer {
    pub fn base_url(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }
}

pub async fn start_bridge_server(ctx: BridgeContext) -> anyhow::Result<BridgeServer> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let port = listener.local_addr()?.port();
    let token = Uuid::new_v4().to_string();
    let server = BridgeServer {
        port,
        token: token.clone(),
    };
    let ctx = Arc::new(ctx);
    tokio::spawn(async move {
        while let Ok((stream, _)) = listener.accept().await {
            let ctx = Arc::clone(&ctx);
            let token = token.clone();
            tokio::spawn(async move {
                let _ = handle_http_connection(stream, ctx, token).await;
            });
        }
    });
    Ok(server)
}

pub fn build_bridge_script(base_url: &str, token: &str) -> String {
    format!(
        r#"
(() => {{
  const baseUrl = {base_url};
  const bridgeToken = {token};
  window.__codexHelperBridge = async (path, payload = {{}}) => {{
    const response = await fetch(`${{baseUrl}}${{path}}`, {{
      method: "POST",
      headers: {{
        "content-type": "application/json",
        "x-codex-helper-token": bridgeToken,
      }},
      body: JSON.stringify(payload),
    }});
    if (!response.ok) {{
      return {{ status: "failed", message: `Codex Helper bridge HTTP ${{response.status}}` }};
    }}
    return response.json();
  }};
}})();
"#,
        base_url = serde_json::to_string(base_url).expect("base url json"),
        token = serde_json::to_string(token).expect("token json")
    )
}

pub async fn install_bridge(
    websocket_url: &str,
    server: &BridgeServer,
    runtime_scripts: Vec<String>,
) -> anyhow::Result<()> {
    let socket = connect_cdp_websocket(websocket_url).await?;
    let mut session = CdpSession::new(socket);

    session.send_command(1, "Runtime.enable", json!({})).await?;

    let mut scripts = vec![build_bridge_script(&server.base_url(), &server.token)];
    scripts.extend(runtime_scripts);
    let mut message_id = 2;
    for script in scripts {
        session
            .send_command(
                message_id,
                "Page.addScriptToEvaluateOnNewDocument",
                json!({ "source": script }),
            )
            .await?;
        message_id += 1;
        session
            .send_command(
                message_id,
                "Runtime.evaluate",
                runtime_evaluate_params(&script),
            )
            .await?;
        message_id += 1;
    }

    Ok(())
}

fn runtime_evaluate_params(expression: &str) -> Value {
    json!({
        "expression": expression,
        "awaitPromise": false,
        "allowUnsafeEvalBlockedByCSP": true,
    })
}

async fn handle_http_connection(
    mut stream: TcpStream,
    ctx: Arc<BridgeContext>,
    token: String,
) -> anyhow::Result<()> {
    let mut buffer = Vec::new();
    let mut chunk = [0_u8; 4096];
    loop {
        let read = stream.read(&mut chunk).await?;
        if read == 0 {
            return Ok(());
        }
        buffer.extend_from_slice(&chunk[..read]);
        if request_complete(&buffer) {
            break;
        }
        if buffer.len() > 1_048_576 {
            write_json_response(
                &mut stream,
                413,
                json!({"status": "failed", "message": "Request too large"}),
            )
            .await?;
            return Ok(());
        }
    }
    let request = parse_http_request(&buffer)?;
    if request.method == "OPTIONS" {
        write_options_response(&mut stream).await?;
        return Ok(());
    }
    if request.method != "POST" {
        write_json_response(
            &mut stream,
            405,
            json!({"status": "failed", "message": "Method not allowed"}),
        )
        .await?;
        return Ok(());
    }
    if request.token.as_deref() != Some(token.as_str()) {
        write_json_response(
            &mut stream,
            403,
            json!({"status": "failed", "message": "Invalid Codex Helper bridge token"}),
        )
        .await?;
        return Ok(());
    }
    let payload = if request.body.trim().is_empty() {
        json!({})
    } else {
        serde_json::from_str(&request.body)?
    };
    let result = handle_bridge_request((*ctx).clone(), &request.path, payload).await;
    write_json_response(&mut stream, 200, result).await?;
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct HttpRequest {
    method: String,
    path: String,
    token: Option<String>,
    body: String,
}

fn request_complete(buffer: &[u8]) -> bool {
    let Some(header_end) = find_header_end(buffer) else {
        return false;
    };
    let header = String::from_utf8_lossy(&buffer[..header_end]);
    let content_length = content_length(&header).unwrap_or(0);
    buffer.len() >= header_end + 4 + content_length
}

fn parse_http_request(buffer: &[u8]) -> anyhow::Result<HttpRequest> {
    let header_end = find_header_end(buffer)
        .ok_or_else(|| anyhow::anyhow!("HTTP header terminator not found"))?;
    let header = String::from_utf8_lossy(&buffer[..header_end]);
    let mut lines = header.lines();
    let request_line = lines
        .next()
        .ok_or_else(|| anyhow::anyhow!("HTTP request line missing"))?;
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or_default().to_string();
    let path = parts.next().unwrap_or_default().to_string();
    if method.is_empty() || path.is_empty() {
        anyhow::bail!("Invalid HTTP request line");
    }
    let token = lines.find_map(|line| {
        let (name, value) = line.split_once(':')?;
        if name.eq_ignore_ascii_case("x-codex-helper-token") {
            Some(value.trim().to_string())
        } else {
            None
        }
    });
    let length = content_length(&header).unwrap_or(0);
    let body_start = header_end + 4;
    let body_end = body_start + length;
    let body = String::from_utf8_lossy(&buffer[body_start..body_end]).to_string();
    Ok(HttpRequest {
        method,
        path,
        token,
        body,
    })
}

fn find_header_end(buffer: &[u8]) -> Option<usize> {
    buffer.windows(4).position(|window| window == b"\r\n\r\n")
}

fn content_length(header: &str) -> Option<usize> {
    header.lines().find_map(|line| {
        let (name, value) = line.split_once(':')?;
        if name.eq_ignore_ascii_case("content-length") {
            value.trim().parse().ok()
        } else {
            None
        }
    })
}

async fn write_options_response(stream: &mut TcpStream) -> anyhow::Result<()> {
    let response = "HTTP/1.1 204 No Content\r\n\
Access-Control-Allow-Origin: *\r\n\
Access-Control-Allow-Methods: POST, OPTIONS\r\n\
Access-Control-Allow-Headers: content-type, x-codex-helper-token\r\n\
Access-Control-Allow-Private-Network: true\r\n\
Content-Length: 0\r\n\r\n";
    stream.write_all(response.as_bytes()).await?;
    Ok(())
}

async fn write_json_response(
    stream: &mut TcpStream,
    status: u16,
    body: Value,
) -> anyhow::Result<()> {
    let status_text = match status {
        200 => "OK",
        403 => "Forbidden",
        405 => "Method Not Allowed",
        413 => "Payload Too Large",
        _ => "Error",
    };
    let body = serde_json::to_string(&body)?;
    let response = format!(
        "HTTP/1.1 {status} {status_text}\r\n\
Access-Control-Allow-Origin: *\r\n\
Access-Control-Allow-Methods: POST, OPTIONS\r\n\
Access-Control-Allow-Headers: content-type, x-codex-helper-token\r\n\
Access-Control-Allow-Private-Network: true\r\n\
Content-Type: application/json\r\n\
Content-Length: {}\r\n\r\n{}",
        body.len(),
        body
    );
    stream.write_all(response.as_bytes()).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bridge_script_defines_expected_globals_and_binding() {
        let script = build_bridge_script("http://127.0.0.1:1234", "secret-token");

        assert!(script.contains("window.__codexHelperBridge"));
        assert!(script.contains("http://127.0.0.1:1234"));
        assert!(script.contains("x-codex-helper-token"));
        assert!(script.contains("secret-token"));
    }

    #[test]
    fn parse_http_request_reads_path_token_and_body() {
        let request = b"POST /backend/status HTTP/1.1\r\ncontent-length: 13\r\nx-codex-helper-token: token-1\r\n\r\n{\"ok\": true }";

        let parsed = parse_http_request(request).expect("request");

        assert_eq!(parsed.method, "POST");
        assert_eq!(parsed.path, "/backend/status");
        assert_eq!(parsed.token.as_deref(), Some("token-1"));
        assert_eq!(parsed.body, "{\"ok\": true }");
    }

    #[test]
    fn request_complete_waits_for_full_body() {
        let partial = b"POST /x HTTP/1.1\r\ncontent-length: 4\r\n\r\nabc";
        let complete = b"POST /x HTTP/1.1\r\ncontent-length: 4\r\n\r\nabcd";

        assert!(!request_complete(partial));
        assert!(request_complete(complete));
    }
}
