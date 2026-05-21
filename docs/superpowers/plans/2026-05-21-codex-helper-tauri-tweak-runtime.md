# Codex Helper Tauri Tweak Runtime Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the first Codex Helper foundation: a silent Tauri app that starts Codex, writes local state under `~/.codex-helper`, injects a bridge/runtime through CDP, and exposes configuration inside Codex Settings as a single `Codex Helper` sidebar item.

**Architecture:** Keep the packaged app in `src-tauri/`, keep injected JavaScript under `runtime/`, and keep the existing TypeScript launcher as a prototype/reference until the Rust path is verified. The Rust backend owns launch, state, logging, CDP, and bridge routing; the renderer runtime owns Codex DOM integration.

**Tech Stack:** Bun for repository development commands only, Tauri v2 for the macOS app shell, Rust async backend with `tokio`, `reqwest`, and `tokio-tungstenite`, plain injected JavaScript runtime inside Codex.

---

## File Structure

- Create `src-tauri/tauri.conf.json`: Tauri app metadata, hidden default window, bundle identifiers.
- Create `src-tauri/Cargo.toml`: Rust dependencies and binary metadata.
- Create `src-tauri/build.rs`: Tauri build hook.
- Create `src-tauri/src/main.rs`: app entry point.
- Create `src-tauri/src/app.rs`: Tauri builder and launch-on-startup flow.
- Create `src-tauri/src/state_dir.rs`: `~/.codex-helper` paths and initialization.
- Create `src-tauri/src/logging.rs`: append-only JSONL diagnostic logging.
- Create `src-tauri/src/launcher.rs`: Codex app resolution and macOS launch command construction.
- Create `src-tauri/src/cdp.rs`: target query, target selection, websocket command support.
- Create `src-tauri/src/bridge.rs`: CDP binding installation and runtime bundle injection.
- Create `src-tauri/src/runtime.rs`: built-in runtime and user script bundle loading.
- Create `src-tauri/src/routes.rs`: allowlisted bridge routes.
- Modify `package.json`: add Tauri scripts using Bun.
- Modify `runtime/renderer.js`: replace floating panel with `Codex Helper` Settings injection.
- Modify `README.md`: document silent app launch and `~/.codex-helper`.

---

### Task 1: Tauri Shell and State Directory

**Files:**
- Create: `src-tauri/tauri.conf.json`
- Create: `src-tauri/Cargo.toml`
- Create: `src-tauri/build.rs`
- Create: `src-tauri/src/main.rs`
- Create: `src-tauri/src/app.rs`
- Create: `src-tauri/src/state_dir.rs`
- Create: `src-tauri/src/logging.rs`
- Modify: `package.json`

- [ ] **Step 1: Create Tauri config and Rust package files**

Create a hidden-window Tauri app named `CodexHelper`. `tauri.conf.json` must have `"create": false` for the main window.

- [ ] **Step 2: Implement state directory initialization**

`StateDir::init()` must create exactly these directories:

```text
~/.codex-helper
~/.codex-helper/logs
~/.codex-helper/backups
~/.codex-helper/scripts
```

It must also expose paths for `config.json` and `state.json`.

- [ ] **Step 3: Implement append-only diagnostics**

`DiagnosticLogger::append(event, detail)` writes JSONL records under `~/.codex-helper/logs/codex-helper.jsonl`.

Each record must include:

```json
{"timestamp":"...","event":"...","detail":{}}
```

- [ ] **Step 4: Add Rust tests**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml
```

Expected: state directory and logging tests pass.

---

### Task 2: Launcher Backend

**Files:**
- Create: `src-tauri/src/launcher.rs`
- Modify: `src-tauri/src/app.rs`

- [ ] **Step 1: Implement Codex app resolution**

Use `/Applications/Codex.app` as the explicit first version path. If it does not exist, return:

```text
Codex app not found: /Applications/Codex.app
```

- [ ] **Step 2: Implement macOS launch command construction**

The command must be:

```text
open -W -a /Applications/Codex.app --args --remote-debugging-port=<port>
```

- [ ] **Step 3: Launch Codex from Tauri setup**

On startup, initialize the state directory, write `launcher.starting`, launch Codex, then proceed to CDP injection.

- [ ] **Step 4: Add launcher tests**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml launcher
```

Expected: command construction includes `-W`, `-a`, `--args`, and `--remote-debugging-port=9229`.

---

### Task 3: CDP Target Selection and Bridge Injection

**Files:**
- Create: `src-tauri/src/cdp.rs`
- Create: `src-tauri/src/bridge.rs`
- Create: `src-tauri/src/runtime.rs`
- Create: `src-tauri/src/routes.rs`
- Modify: `src-tauri/src/app.rs`

- [ ] **Step 1: Implement CDP target query and selection**

`pick_codex_page_target` must select a `page` target with `webSocketDebuggerUrl`. Prefer targets whose title or URL contains `codex`; otherwise select the first injectable page. If no target exists, return:

```text
No injectable Codex page target found
```

- [ ] **Step 2: Implement bridge script**

The bridge must expose:

```javascript
window.__codexHelperBridge(path, payload)
```

It must use a CDP binding named:

```text
codexHelperBridge
```

- [ ] **Step 3: Install bridge and runtime bundle**

Use `Runtime.addBinding`, `Page.addScriptToEvaluateOnNewDocument`, and immediate `Runtime.evaluate`. Runtime bundle order is:

1. bridge script
2. `runtime/renderer.js`
3. user scripts from `~/.codex-helper/scripts`

- [ ] **Step 4: Add CDP and bridge tests**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml
```

Expected: target selection, bridge script content, and runtime bundle ordering tests pass.

---

### Task 4: Codex Settings Runtime UI

**Files:**
- Modify: `runtime/renderer.js`

- [ ] **Step 1: Replace floating UI**

Remove the `#codex-helper-root` floating button/panel. The runtime should not create visible UI unless it finds a Codex Settings insertion point.

- [ ] **Step 2: Add `Codex Helper` Settings item installer**

The runtime should insert one `Codex Helper` item into a Settings sidebar-like element. The first version supports representative selectors:

```javascript
[role="tablist"], nav, aside
```

If no insertion point exists, dispatch a diagnostic event with:

```text
settings_insertion_failed
```

- [ ] **Step 3: Add diagnostics and user scripts content**

The injected settings page must include sections for:

```text
User Scripts
Diagnostics
```

- [ ] **Step 4: Run JavaScript syntax checks**

Run:

```bash
bun --check runtime/renderer.js
```

Expected: runtime file passes syntax checks.

---

### Task 5: Documentation and Baseline Verification

**Files:**
- Modify: `README.md`
- Modify: `docs/references.md`

- [ ] **Step 1: Update README**

Document that `CodexHelper.app` is silent by default, stores local state in `~/.codex-helper`, and configures tweaks from Codex Settings after injection.

- [ ] **Step 2: Update references**

Clarify that CodexMan is a reference for launch/CDP mechanics only, not product scope.

- [ ] **Step 3: Run available verification**

Run:

```bash
bun run check
cargo test --manifest-path src-tauri/Cargo.toml
```

Expected: TypeScript check passes; Rust tests pass if Tauri/Cargo dependencies are available in the local environment.
