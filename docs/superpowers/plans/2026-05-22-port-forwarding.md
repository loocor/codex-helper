# Codex Helper Port Forwarding Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Stabilize Codex Helper's injected Settings UI, then add VS Code-style SSH remote port forwarding with Settings policies, a bottom-panel Ports view, Terminal web-port detection, and helper-managed SSH tunnels.

**Architecture:** First harden renderer DOM surface detection so injected UI cannot mix into Codex General settings. Then split port forwarding into small backend and renderer units: Rust owns settings persistence, SSH context resolution, local port checks, and tunnel child processes; the injected renderer owns Terminal URL detection and the Ports panel UI.

**Tech Stack:** Bun, TypeScript syntax checks, plain injected browser JavaScript, Rust/Tauri, Tokio child processes, system `ssh`, existing Codex Helper CDP bridge.

---

## File Structure

- Modify `runtime/renderer.js`: keep the runtime entrypoint, but add smaller pure helper functions for Settings surface detection, Terminal port parsing, and Ports panel rendering.
- Modify `src-tauri/src/settings.rs`: add persisted port forwarding settings with explicit allowlisted keys.
- Modify `src-tauri/src/routes.rs`: add allowlisted `/ports/*` bridge routes.
- Create `src-tauri/src/ports.rs`: own port validation, tunnel request models, local port availability checks, tunnel registry, and SSH child process lifecycle.
- Modify `src-tauri/src/lib.rs`: register the `ports` module.
- Modify `src-tauri/src/app.rs`: create and pass shared tunnel state into bridge context, and stop tunnels during helper shutdown.
- Modify `src-tauri/src/zed.rs`: expose reusable SSH target resolution functions without renaming the existing Zed behavior.
- Test with Rust unit tests in `ports.rs`, `settings.rs`, `zed.rs`, and lightweight renderer syntax checks.

## Task 1: Harden Settings Injection

**Files:**
- Modify: `runtime/renderer.js`

- [ ] **Step 1: Extract pure Settings surface helpers**

Create helper functions near the existing Settings functions:

```javascript
function isSettingsContainerCandidate(candidate) {
  if (!(candidate instanceof HTMLElement)) return false;
  if (candidate.querySelector(`[${helperPageAttribute}]`)) return false;
  const rect = candidate.getBoundingClientRect();
  if (rect.width <= 120 || rect.width >= 520) return false;
  const labels = visibleSettingsLabels(candidate);
  return labels.length >= 5;
}

function visibleSettingsLabels(root) {
  return settingsLabels.filter((label) =>
    Array.from(root.querySelectorAll("button, a, [role='button'], [role='tab'], [role='menuitem']"))
      .some((node) => node instanceof HTMLElement && exactText(node, label) && isVisibleElement(node)),
  );
}

function isVisibleElement(node) {
  const rect = node.getBoundingClientRect();
  return rect.width > 0 && rect.height > 0;
}
```

- [ ] **Step 2: Stop treating arbitrary divs as clickable settings items**

Change `findSettingsSidebar` and `findClickableSettingsItem` so sidebar candidates may include layout containers, but clickable items only include explicit interactive elements:

```javascript
function findSettingsSidebar() {
  const candidates = Array.from(
    document.querySelectorAll("aside, nav, [role='navigation'], [role='tablist']"),
  );
  return candidates.find(isSettingsContainerCandidate) || null;
}

function findClickableSettingsItem(sidebar, label) {
  const selector = "button, a, [role='button'], [role='tab'], [role='menuitem']";
  return Array.from(sidebar.querySelectorAll(selector)).find((node) => {
    if (!(node instanceof HTMLElement)) return false;
    if (node.closest(`[${helperEntryAttribute}]`)) return false;
    if (!exactText(node, label)) return false;
    const rect = node.getBoundingClientRect();
    return rect.width > 80 && rect.height > 18;
  });
}
```

- [ ] **Step 3: Add content root validation before rendering**

Before `renderHelperPage(root)`, ensure the selected root is not a native Settings subgroup:

```javascript
function isValidSettingsContentRoot(root, sidebar) {
  if (!(root instanceof HTMLElement)) return false;
  if (root.closest(`[${helperEntryAttribute}]`)) return false;
  if (root.querySelector(`[${helperEntryAttribute}]`)) return false;
  if (sidebar instanceof HTMLElement && (root.contains(sidebar) || sidebar.contains(root))) return false;
  const rect = root.getBoundingClientRect();
  return rect.width > 520 && rect.height > 420;
}
```

Use this validation in both `findSettingsContentRoot` and `findSettingsContentRootFromSidebar`.

- [ ] **Step 4: Normalize diagnostics**

Use explicit event names for Settings injection failures:

```javascript
logDiagnostic("settings_insertion_failed", { reason: "sidebar_not_found" });
logDiagnostic("settings_content_root_failed", { reason: "content_root_not_found" });
```

- [ ] **Step 5: Verify renderer syntax**

Run:

```bash
bun --check runtime/renderer.js
```

Expected: no output and exit code 0.

## Task 2: Add Port Forwarding Settings Model

**Files:**
- Modify: `src-tauri/src/settings.rs`
- Modify: `runtime/renderer.js`

- [ ] **Step 1: Extend Rust settings struct**

Add these fields to `HelperSettings`:

```rust
pub port_forwarding_enabled: bool,
pub port_auto_forward_web: bool,
pub port_same_local_port: bool,
```

Add defaults:

```rust
port_forwarding_enabled: false,
port_auto_forward_web: true,
port_same_local_port: true,
```

- [ ] **Step 2: Allowlist settings updates**

Add keys in `update_settings`:

```rust
"portForwardingEnabled" => settings.port_forwarding_enabled = enabled,
"portAutoForwardWeb" => settings.port_auto_forward_web = enabled,
"portSameLocalPort" => settings.port_same_local_port = enabled,
```

- [ ] **Step 3: Add settings tests**

Add assertions to the default and update tests:

```rust
assert!(!settings.port_forwarding_enabled);
assert!(settings.port_auto_forward_web);
assert!(settings.port_same_local_port);
```

Update the persistence test payload:

```rust
"portForwardingEnabled": true,
"portAutoForwardWeb": false,
"portSameLocalPort": true,
```

- [ ] **Step 4: Add Settings UI rows**

In `renderHelperPage`, add a `Port Forwarding` section with three switches using the bridge keys:

```html
<h2>Port Forwarding</h2>
<div class="codex-helper-panel">
  <div class="codex-helper-row">
    <div>
      <div class="codex-helper-label">Enable port forwarding</div>
      <div class="codex-helper-detail" data-codex-helper-setting-status="portForwardingEnabled">Loading</div>
    </div>
    <label class="codex-helper-switch" aria-label="Enable port forwarding">
      <input type="checkbox" data-codex-helper-setting-toggle="portForwardingEnabled">
      <span></span>
    </label>
  </div>
  <div class="codex-helper-row">
    <div>
      <div class="codex-helper-label">Auto-forward detected web ports</div>
      <div class="codex-helper-detail" data-codex-helper-setting-status="portAutoForwardWeb">Loading</div>
    </div>
    <label class="codex-helper-switch" aria-label="Auto-forward detected web ports">
      <input type="checkbox" data-codex-helper-setting-toggle="portAutoForwardWeb">
      <span></span>
    </label>
  </div>
  <div class="codex-helper-row">
    <div>
      <div class="codex-helper-label">Use the same local port by default</div>
      <div class="codex-helper-detail" data-codex-helper-setting-status="portSameLocalPort">Loading</div>
    </div>
    <label class="codex-helper-switch" aria-label="Use the same local port by default">
      <input type="checkbox" data-codex-helper-setting-toggle="portSameLocalPort">
      <span></span>
    </label>
  </div>
</div>
```

- [ ] **Step 5: Run tests**

Run:

```bash
env RUSTC_WRAPPER= cargo test --manifest-path src-tauri/Cargo.toml settings
bun --check runtime/renderer.js
```

Expected: settings tests pass and renderer syntax check exits 0.

## Task 3: Backend Port Models and Validation

**Files:**
- Create: `src-tauri/src/ports.rs`
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 1: Add module registration**

Add to `src-tauri/src/lib.rs`:

```rust
mod ports;
```

- [ ] **Step 2: Create port request and status types**

Create `src-tauri/src/ports.rs` with:

```rust
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

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
```

- [ ] **Step 3: Add validation tests**

Add tests in `ports.rs`:

```rust
#[cfg(test)]
mod tests {
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
}
```

- [ ] **Step 4: Run tests**

Run:

```bash
env RUSTC_WRAPPER= cargo test --manifest-path src-tauri/Cargo.toml ports
```

Expected: all `ports` tests pass.

## Task 4: Backend Tunnel Manager

**Files:**
- Modify: `src-tauri/src/ports.rs`
- Modify: `src-tauri/src/routes.rs`
- Modify: `src-tauri/src/app.rs`

- [ ] **Step 1: Add tunnel registry**

Extend `ports.rs`:

```rust
use std::collections::BTreeMap;
use std::net::TcpListener;
use std::process::Stdio;
use std::sync::Arc;

use tokio::process::{Child, Command};
use tokio::sync::Mutex;

use crate::zed::SshTarget;

#[derive(Clone, Default)]
pub struct PortForwardManager {
    tunnels: Arc<Mutex<BTreeMap<String, ManagedTunnel>>>,
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
```

- [ ] **Step 2: Build SSH target argument**

Add:

```rust
fn ssh_target_arg(target: &SshTarget) -> String {
    let user_prefix = if target.user.trim().is_empty() {
        String::new()
    } else {
        format!("{}@", target.user.trim())
    };
    format!("{user_prefix}{}", target.host)
}
```

Use the SSH port as `-p <port>` only when present.

- [ ] **Step 3: Add start and stop methods**

Add methods:

```rust
impl PortForwardManager {
    pub async fn list(&self) -> Value {
        let tunnels = self.tunnels.lock().await;
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

    pub async fn stop(&self, id: &str) -> Value {
        let mut tunnels = self.tunnels.lock().await;
        let Some(mut tunnel) = tunnels.remove(id) else {
            return json!({ "status": "failed", "message": "Port tunnel not found" });
        };
        let _ = tunnel.child.kill().await;
        json!({ "status": "ok", "id": id })
    }
}
```

- [ ] **Step 4: Add `start` method**

Add:

```rust
impl PortForwardManager {
    pub async fn start(&self, request: PortForwardRequest, target: SshTarget) -> Value {
        if !local_port_available(request.local_port) {
            return json!({
                "status": "failed",
                "message": format!("Local port {} is already in use", request.local_port)
            });
        }

        let mut command = Command::new("ssh");
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
        command.stderr(Stdio::piped());

        let child = match command.spawn() {
            Ok(child) => child,
            Err(error) => return json!({ "status": "failed", "message": error.to_string() }),
        };

        let id = tunnel_id(&request);
        let local_url = format!("http://127.0.0.1:{}", request.local_port);
        self.tunnels.lock().await.insert(id.clone(), ManagedTunnel { request, child });
        json!({ "status": "ok", "id": id, "localUrl": local_url })
    }
}
```

- [ ] **Step 5: Wire manager into bridge context**

Add `pub port_manager: PortForwardManager` to `BridgeContext` in `routes.rs`.

Create the manager in `launch_on_startup` and pass it into `BridgeContext`.

- [ ] **Step 6: Add shutdown cleanup**

Keep a clone of `PortForwardManager` in `app.rs` and stop all tracked tunnels when Tauri exits. If direct async cleanup in the event handler is awkward, add a synchronous best-effort method using `child.start_kill()` and call it before exit.

- [ ] **Step 7: Run tests**

Run:

```bash
env RUSTC_WRAPPER= cargo test --manifest-path src-tauri/Cargo.toml ports
```

Expected: all `ports` tests pass.

## Task 5: Bridge Routes for Ports

**Files:**
- Modify: `src-tauri/src/routes.rs`
- Modify: `src-tauri/src/ports.rs`
- Modify: `src-tauri/src/zed.rs`

- [ ] **Step 1: Expose SSH target resolution for ports**

In `zed.rs`, make `resolve_ssh_target_for_host_id` public:

```rust
pub fn resolve_ssh_target_for_host_id(
    host_id: &str,
    state_path: Option<&Path>,
) -> Result<SshTarget, ZedRemoteError> {
```

Keep existing callers unchanged.

- [ ] **Step 2: Parse forward route payload**

Add to `ports.rs`:

```rust
pub fn request_from_payload(payload: &Value) -> Result<PortForwardRequest, String> {
    let host_id = payload.get("hostId").and_then(Value::as_str).unwrap_or("").trim().to_string();
    if host_id.is_empty() {
        return Err("Remote host id is required".to_string());
    }
    let remote_path = payload.get("remotePath").and_then(Value::as_str).unwrap_or("").trim().to_string();
    let remote_port = parse_port(payload.get("remotePort").unwrap_or(&Value::Null), "remotePort")?;
    let local_port = parse_port(payload.get("localPort").unwrap_or(&Value::Null), "localPort")?;
    let source = match payload.get("source").and_then(Value::as_str).unwrap_or("manual") {
        "auto" => PortForwardSource::Auto,
        "manual" => PortForwardSource::Manual,
        _ => return Err("source must be auto or manual".to_string()),
    };
    Ok(PortForwardRequest { host_id, remote_path, remote_port, local_port, source })
}
```

- [ ] **Step 3: Add routes**

In `handle_bridge_request`:

```rust
"/ports/list" => ctx.port_manager.list().await,
"/ports/forward" => match crate::ports::request_from_payload(&payload) {
    Ok(request) => match crate::zed::resolve_ssh_target_for_host_id(&request.host_id, None) {
        Ok(target) => ctx.port_manager.start(request, target).await,
        Err(error) => json!({ "status": "failed", "message": error.to_string() }),
    },
    Err(message) => json!({ "status": "failed", "message": message }),
},
"/ports/stop" => {
    let id = payload.get("id").and_then(Value::as_str).unwrap_or("");
    ctx.port_manager.stop(id).await
},
```

- [ ] **Step 4: Run route tests**

Add a unit test for unknown route behavior remaining unchanged and run:

```bash
env RUSTC_WRAPPER= cargo test --manifest-path src-tauri/Cargo.toml routes ports zed
```

Expected: relevant tests pass.

## Task 6: Terminal Web-Port Detection

**Files:**
- Modify: `runtime/renderer.js`

- [ ] **Step 1: Add parser**

Add pure parser functions:

```javascript
function parseWebPortsFromText(text) {
  const ports = new Map();
  const pattern = /\bhttps?:\/\/(?:localhost|127\.0\.0\.1|0\.0\.0\.0|\[::1\]):([0-9]{1,5})(?:[/?#][^\s"'<>]*)?/gi;
  for (const match of text.matchAll(pattern)) {
    const port = Number(match[1]);
    if (Number.isInteger(port) && port >= 1 && port <= 65535) {
      ports.set(port, { port, url: match[0] });
    }
  }
  return Array.from(ports.values());
}
```

- [ ] **Step 2: Add remote context for ports**

Reuse `remoteContextFromDom()` and return `hostId` plus `path`. If either value is missing, create a detected row but do not call `/ports/forward`.

- [ ] **Step 3: Observe Terminal text conservatively**

In the existing MutationObserver callback, throttle a scan:

```javascript
let pendingPortScan = 0;

function schedulePortScan() {
  if (pendingPortScan) return;
  pendingPortScan = window.setTimeout(() => {
    pendingPortScan = 0;
    scanTerminalWebPorts();
  }, 500);
}
```

Call `schedulePortScan()` from the observer callback.

- [ ] **Step 4: Add detected port registry**

Add:

```javascript
const detectedPorts = new Map();

function portKey(context, remotePort, localPort) {
  return [context.hostId || "unknown", context.path || "", remotePort, localPort || remotePort].join(":");
}
```

- [ ] **Step 5: Auto-forward only when settings allow it**

In `scanTerminalWebPorts`, read `featureSettings.portForwardingEnabled` and `featureSettings.portAutoForwardWeb`. Only call `/ports/forward` when both are true and `context.hostId` is present.

- [ ] **Step 6: Verify renderer syntax**

Run:

```bash
bun --check runtime/renderer.js
```

Expected: no output and exit code 0.

## Task 7: Bottom Ports Panel UI

**Files:**
- Modify: `runtime/renderer.js`

- [ ] **Step 1: Locate bottom panel card container**

Add a conservative locator that only returns a container if it includes a visible `Terminal` control:

```javascript
function findBottomPanelPicker() {
  const controls = Array.from(document.querySelectorAll("button, [role='button'], [role='tab']"));
  const terminal = controls.find((node) => node instanceof HTMLElement && exactText(node, "Terminal") && isVisibleElement(node));
  return terminal?.parentElement || null;
}
```

- [ ] **Step 2: Install Ports entry**

Clone the Terminal control, replace text with `Ports`, mark it with `data-codex-helper-ports-entry`, and attach a click handler that calls `showPortsPanel()`.

- [ ] **Step 3: Render Ports panel**

Add a helper-owned panel root with rows from `/ports/list` and local detected candidates:

```javascript
async function showPortsPanel() {
  const result = await bridge("/ports/list");
  renderPortsPanel(result?.ports || []);
}
```

Each row should include `Open`, `Copy URL`, and `Stop` buttons when a local URL exists.

- [ ] **Step 4: Add actions**

Use:

```javascript
window.open(localUrl, "_blank", "noopener,noreferrer");
navigator.clipboard.writeText(localUrl);
bridge("/ports/stop", { id });
```

If clipboard fails, show a visible row error and log `ports_copy_failed`.

- [ ] **Step 5: Verify renderer syntax**

Run:

```bash
bun --check runtime/renderer.js
```

Expected: no output and exit code 0.

## Task 8: Full Verification

**Files:**
- Verify repository-wide behavior.

- [ ] **Step 1: TypeScript check**

Run:

```bash
bun run check
```

Expected: `tsc --noEmit` exits 0.

- [ ] **Step 2: Renderer syntax check**

Run:

```bash
bun --check runtime/renderer.js
```

Expected: no output and exit code 0.

- [ ] **Step 3: Rust tests**

Run:

```bash
env RUSTC_WRAPPER= cargo test --manifest-path src-tauri/Cargo.toml
```

Expected: all tests pass.

- [ ] **Step 4: Manual Codex validation**

Launch Codex Helper, then validate:

```text
Settings > Codex Helper opens without mixing into General.
Native Settings entries clear the helper page.
Bottom panel shows Ports next to Terminal.
Remote Terminal web URL creates a detected or active Ports row.
Active row supports Open, Copy URL, and Stop.
Quitting Codex Helper stops helper-managed SSH tunnels.
```

## Self-Review Notes

- Spec coverage: Settings hardening, Settings policies, Ports panel, Terminal detection, SSH context, tunnel lifecycle, and verification are all mapped to tasks.
- Placeholder scan: no deferred implementation markers are intentionally left.
- Scope check: the persistent mapping table, random-port policy, SSH credential management, and remote process scanning remain out of scope.
