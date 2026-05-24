# Port Forward Lifecycle Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build safe, reliable, real-time SSH remote port forwarding for CodexHelper, scoped to the active Codex conversation and visible in Pinned Summary.

**Architecture:** CodexHelper must stop treating Terminal DOM and Summary DOM as authoritative. The primary context source is Codex's structured thread execution target: active `conversationId`, `hostId`, `cwd`, and `hostConfig.kind`. Remote listening ports are discovered through the active remote host and workspace, forwarded through managed SSH tunnels, reconciled continuously, and rendered into the existing Pinned Summary panel.

**Tech Stack:** Bun runtime injection (`runtime/*.js`), Bun bridge backend (`src/*.ts`), Tauri/Rust bridge (`src-tauri/src/*.rs`), OpenSSH, Bun tests, Rust unit tests.

---

## Evidence Baseline

From `/Applications/Codex.app/Contents/Resources/app.asar`, extracted to `/tmp/codex-app-asar.noX4Ex`:

- `use-active-conversation-id-CJemFmqQ.js` parses `/local/:conversationId`, `/remote/:conversationId`, and `/hotkey-window/thread/:conversationId`.
- `use-webview-execution-target-B7RRBzs9.js` computes `{ cwd, hostId, hostConfig }`.
- `thread-context-DyfT5Vx-.js` computes route-scoped thread `{ cwd, hostId }`.
- `thread-context-inputs-DcllWVDq.js` reads `host_config`, `remote_connections`, `remote_control_connections`, `REMOTE_PROJECTS`, `ACTIVE_REMOTE_PROJECT_ID`, and `THREAD_PROJECT_ASSIGNMENTS`.
- `use-is-remote-host-CaeHryrK.js` treats a host as remote when `hostConfig.kind !== "local"`.
- `terminal-service-BsiZiRKt.js` maps terminal sessions to conversations and stores terminal snapshots, but this should be a secondary source after structured thread context.
- Codex's own remote ChatGPT login tunnel uses `ssh -N -L` with `ExitOnForwardFailure=yes`, `BatchMode=yes`, `ControlMaster=no`, `ControlPath=none`, `ConnectTimeout`, `ServerAliveInterval`, and `ServerAliveCountMax`.

Current CodexHelper state:

- `runtime/ports.js` still uses sidebar/Summary DOM fallback for context and terminal text parsing as one discovery path.
- `src/ports.ts` and `src-tauri/src/ports.rs` already have `/ports/discover`, `/ports/forward`, `/ports/list`, `/ports/stop`.
- Remote discovery runs `lsof` over SSH and filters ports whose process cwd is inside `remotePath`.
- Existing UI is pinned-summary-only and already removes itself when port forwarding is disabled or the session is not remote.

## File Structure

- Modify `runtime/ports.js`
  - Own active conversation context resolution.
  - Prefer structured React/Codex state over DOM fallback.
  - Reconcile discovered ports and tunnel lifecycle.
  - Render Pinned Summary state.

- Modify `runtime/constants.js`
  - Add state for structured context cache and resolver diagnostics if needed.

- Modify `runtime/_test-port-detection.test.js`
  - Test active route parsing, structured remote context extraction, resolver precedence, and local-session blocking.

- Modify `runtime/_test-ports-panel.test.js`
  - Test Pinned Summary behavior for remote context, disabled settings, active/stopped/unreachable rows.

- Modify `src/ports.ts`
  - Make SSH tunnel start options match Codex's proven tunnel options.
  - Keep strict host resolution and port validation.
  - Improve lifecycle list/stop behavior where tests expose gaps.

- Modify `src/ports.test.ts`
  - Test SSH args, automatic local port allocation, start failure behavior, and tunnel registry reconciliation.

- Modify `src-tauri/src/ports.rs`
  - Keep Tauri behavior aligned with Bun backend.
  - Mirror SSH args and lifecycle behavior.

- Modify `src-tauri/src/routes.rs`
  - Only if route payloads need new fields or status responses.

- Optional create `docs/port-forwarding-architecture.md`
  - Only after implementation is stable, document the signal hierarchy and lifecycle model.

---

### Task 1: Structured Active Conversation Context

**Files:**
- Modify: `runtime/ports.js`
- Modify: `runtime/constants.js`
- Modify: `runtime/_test-port-detection.test.js`

- [ ] **Step 1: Write failing tests for route parsing**

Add tests that define the expected route parser behavior:

```js
test("active conversation id parser accepts Codex local and remote routes", () => {
  const parseActiveConversationIdFromPath = loadFunction(
    "parseActiveConversationIdFromPath",
  );

  expect(parseActiveConversationIdFromPath("/local/019e-thread")).toBe("019e-thread");
  expect(parseActiveConversationIdFromPath("/remote/019e-thread")).toBe("019e-thread");
  expect(parseActiveConversationIdFromPath("/hotkey-window/thread/019e-thread")).toBe("019e-thread");
  expect(parseActiveConversationIdFromPath("/settings")).toBe("");
});
```

- [ ] **Step 2: Run the failing test**

Run:

```bash
bun test runtime/_test-port-detection.test.js
```

Expected: fail because `parseActiveConversationIdFromPath` does not exist.

- [ ] **Step 3: Implement minimal route parser**

Add to `runtime/ports.js`:

```js
function parseActiveConversationIdFromPath(pathname = window.location.pathname) {
  const match = String(pathname || "").match(
    /^\/(?:local|remote|hotkey-window\/thread)\/([^/?#]+)/,
  );
  return match ? decodeURIComponent(match[1]) : "";
}
```

- [ ] **Step 4: Write failing tests for structured context extraction**

Add a fake fiber tree test that contains:

```js
{
  memoizedProps: {
    value: {
      cwd: "/Volumes/External/GitHub/MCPMate/admin",
      hostId: "remote-ssh-codex-managed:MacMini",
      hostConfig: {
        id: "remote-ssh-codex-managed:MacMini",
        kind: "ssh",
        display_name: "MacMini",
      },
    },
  },
}
```

Expected result:

```js
{
  hostId: "remote-ssh-codex-managed:MacMini",
  path: "/Volumes/External/GitHub/MCPMate/admin",
  threadId: "019e-thread",
  kind: "ssh",
  isRemote: true,
  source: "codex-structured-context",
}
```

- [ ] **Step 5: Implement structured context resolver**

Add functions:

```js
function normalizeStructuredExecutionTarget(value, conversationId) {
  if (!value || typeof value !== "object") return null;
  const hostConfig = value.hostConfig;
  const hostId =
    normalizeRemoteHostId(value.hostId || hostConfig?.id || "");
  const cwd = typeof value.cwd === "string" && value.cwd.startsWith("/")
    ? value.cwd
    : "";
  const kind = typeof hostConfig?.kind === "string" ? hostConfig.kind : "";
  if (!hostId || !cwd || kind === "local") return null;
  return {
    hostId,
    path: cwd,
    threadId: conversationId || "",
    kind,
    isRemote: true,
    source: "codex-structured-context",
  };
}
```

Traverse React fibers from `window.__codexRoot?._internalRoot?.current`, looking at `memoizedProps`, `memoizedState`, `pendingProps`, and `return/child/sibling` nodes. Keep traversal bounded to avoid runtime risk.

- [ ] **Step 6: Verify tests**

Run:

```bash
bun test runtime/_test-port-detection.test.js
```

Expected: pass.

---

### Task 2: Resolver Precedence and Safe Fallbacks

**Files:**
- Modify: `runtime/ports.js`
- Modify: `runtime/_test-port-detection.test.js`

- [ ] **Step 1: Write failing tests for precedence**

Expected order:

1. structured Codex context
2. cached structured context for same visible thread
3. existing sidebar DOM context
4. Pinned Summary Remote + bridge fallback request
5. no context

Test that a local structured context blocks a stale remote Summary row.

- [ ] **Step 2: Run failing tests**

Run:

```bash
bun test runtime/_test-port-detection.test.js
```

Expected: fail because resolver still trusts Summary fallback too early.

- [ ] **Step 3: Update `currentRemoteForwardingContext`**

Make it first call `structuredRemoteForwardingContext()`. If that returns a local context signal, clear `resolvedRemoteForwardingContext` and return not-remote.

- [ ] **Step 4: Keep DOM fallback explicit**

Do not delete DOM fallback yet. Rename its role mentally and in diagnostics as fallback, not source of truth.

- [ ] **Step 5: Verify**

Run:

```bash
bun test runtime/_test-port-detection.test.js runtime/_test-ports-panel.test.js
```

Expected: pass.

---

### Task 3: SSH Tunnel Correctness

**Files:**
- Modify: `src/ports.ts`
- Modify: `src/ports.test.ts`
- Modify: `src-tauri/src/ports.rs`

- [ ] **Step 1: Write failing SSH argument tests**

Expected tunnel args include:

```txt
-N
-L 127.0.0.1:<local>:127.0.0.1:<remote>
-o ExitOnForwardFailure=yes
-o BatchMode=yes
-o ControlMaster=no
-o ControlPath=none
-o ConnectTimeout=10
-o ServerAliveInterval=15
-o ServerAliveCountMax=4
```

Keep target separation behavior verified. On this machine `OpenSSH_10.2p1` accepts `--`, so retain it unless a failing test or compatibility evidence says otherwise.

- [ ] **Step 2: Run failing tests**

Run:

```bash
bun test src/ports.test.ts
```

Expected: fail because `buildSshArgs` lacks those options.

- [ ] **Step 3: Implement Bun backend args**

Update `buildSshArgs` in `src/ports.ts` to match the proven Codex native login tunnel options.

- [ ] **Step 4: Mirror in Rust**

Update `build_ssh_args` in `src-tauri/src/ports.rs` with the same option set.

- [ ] **Step 5: Add Rust tests**

Add or update tests in `src-tauri/src/ports.rs` so Rust verifies the same options.

- [ ] **Step 6: Verify**

Run:

```bash
bun test src/ports.test.ts
rustfmt --edition 2021 --check src-tauri/src/ports.rs src-tauri/src/routes.rs
(cd src-tauri && cargo test --lib)
```

Expected: pass.

---

### Task 4: Discovery and Lifecycle Reconciliation

**Files:**
- Modify: `runtime/ports.js`
- Modify: `runtime/_test-port-detection.test.js`
- Modify: `runtime/_test-ports-panel.test.js`
- Modify: `src/ports.ts`
- Modify: `src/ports.test.ts`

- [ ] **Step 1: Write failing tests for service stop**

When `/ports/discover` no longer reports a remote port that has an active tunnel, runtime must:

- mark the row as stopped or remove it from active rows
- call `/ports/stop` with the tunnel id
- update Pinned Summary

- [ ] **Step 2: Write failing tests for tunnel process exit**

When `/ports/list` omits an id that runtime thought was active, runtime must clear `entry.id`, clear `localUrl`, and mark status `stopped`.

- [ ] **Step 3: Run failing tests**

Run:

```bash
bun test runtime/_test-port-detection.test.js runtime/_test-ports-panel.test.js src/ports.test.ts
```

Expected: fail only on newly specified lifecycle behavior.

- [ ] **Step 4: Implement lifecycle reconciliation**

Keep one state machine:

```txt
detected -> starting -> active
active -> stopped       when tunnel disappears or remote service stops
active -> unreachable   when remote discovery fails
unreachable -> active   when discovery sees the port again and tunnel exists
failed -> detected      only after the same remote port is rediscovered
```

- [ ] **Step 5: Verify**

Run:

```bash
bun test runtime/_test-port-detection.test.js runtime/_test-ports-panel.test.js src/ports.test.ts
```

Expected: pass.

---

### Task 5: Pinned Summary UI Contract

**Files:**
- Modify: `runtime/ports.js`
- Modify: `runtime/_test-ports-panel.test.js`

- [ ] **Step 1: Write failing tests for UI visibility**

Expected:

- port forwarding disabled: no Port Forward section
- local structured context: no Port Forward section
- remote structured context with no ports yet: Port Forward section with `No ports detected yet`
- remote unavailable: Port Forward section with `Remote unavailable`

- [ ] **Step 2: Run failing tests**

Run:

```bash
bun test runtime/_test-ports-panel.test.js
```

Expected: fail for any missing UI contract.

- [ ] **Step 3: Implement UI updates**

Keep the section cloned from Codex's existing Summary rows. Do not create a new card. Preserve disclosure state and icon rotation:

```txt
collapsed: chevron right
expanded: chevron down
```

- [ ] **Step 4: Verify**

Run:

```bash
bun test runtime/_test-ports-panel.test.js
```

Expected: pass.

---

### Task 6: Runtime Integration and Bundle Verification

**Files:**
- Modify as needed from Tasks 1-5

- [ ] **Step 1: Run full JS/runtime checks**

Run:

```bash
bun test ./runtime ./src
bun run check
bun run bundle:runtime
git diff --check
```

Expected: all commands exit 0.

- [ ] **Step 2: Run Rust checks**

Run:

```bash
rustfmt --edition 2021 --check src-tauri/src/ports.rs src-tauri/src/routes.rs
(cd src-tauri && cargo test --lib)
```

Expected: all commands exit 0.

- [ ] **Step 3: Inspect bundled output**

Confirm `dist/bundle.js` contains the new resolver functions and no broad `document.body` terminal scan fallback:

```bash
rg "structuredRemoteForwardingContext|parseActiveConversationIdFromPath|textOf\\(document\\.body\\)" dist/bundle.js
```

Expected: resolver symbols found; `textOf(document.body)` not found.

---

### Task 7: Live Verification Without User Sampling

**Files:**
- No planned source edits

- [ ] **Step 1: Use CDP or Computer Use to inspect current Codex UI**

Collect, without changing app files:

- current route path
- active conversation id
- structured context candidate found by injected runtime
- Pinned Summary Remote/Local row text
- whether Port Forward section renders

- [ ] **Step 2: Verify no restart is required for runtime-only changes**

Use the existing CodexHelper dev injection flow. If the running app cannot pick up the bundle, document the exact reason instead of asking the user to sample manually.

- [ ] **Step 3: If a remote session is already open, verify end-to-end**

Expected:

- remote service on port `3000` is discovered
- tunnel starts
- Pinned Summary shows `3000 -> <local> · active`
- browser can reach `http://127.0.0.1:<local>`
- stopping the remote service removes or stops the tunnel within one scan interval

- [ ] **Step 4: If no remote session is open, stop at non-destructive evidence**

Do not create or mutate SSH config. Report that live remote E2E was unavailable and include static/unit verification evidence.

---

## Completion Criteria

- Active remote context comes from Codex structured state when available.
- Local sessions never inherit stale Remote Summary state.
- Auto-forwarding only starts for the active remote conversation and workspace.
- Stopped remote services stop their associated local tunnels.
- SSH disconnection/reconnection updates Pinned Summary status.
- Disabled Port forwarding removes the Pinned Summary UI and stops scanning.
- All JS/runtime tests, TypeScript checks, runtime bundle, Rust tests, and diff whitespace checks pass.
