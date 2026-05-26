# Multi-Window Injection Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement multi-window Codex Helper injection from `docs/superpowers/specs/2026-05-26-multi-window-injection-design.md`.

**Architecture:** Add a multi-target CDP discovery API, carry caller identity through the bridge envelope, and replace single-target injection with a controller-owned injection registry. Target-scoped routes use caller target identity, while global and session-scoped routes keep their existing business semantics.

**Tech Stack:** Rust/Tauri backend, Chrome DevTools Protocol over WebSocket, injected browser JavaScript runtime, Bun runtime tests.

---

## File Structure

- Modify `src-tauri/src/cdp.rs`: add `codex_page_targets`, target lookup helpers, and focused tests.
- Modify `src-tauri/src/bridge.rs`: add `BridgeCaller`, `BridgeRequest`, caller-aware bridge script generation, and request parsing tests.
- Modify `src-tauri/src/routes.rs`: make bridge request routing accept caller identity and make `/devtools/open` target-aware.
- Modify `src-tauri/src/codex_control.rs`: add injection registry and sync all Codex targets.
- Modify `runtime/core.js`: keep public `bridge(path, payload)` simple while relying on the injected bridge envelope.
- Modify `runtime/bootstrap.js`: send runtime readiness and activity diagnostics.
- Modify `runtime/ports.js`: gate automatic discovery and cleanup behind active window ownership.
- Modify runtime tests under `runtime/_test-*.test.js`: verify caller identity and active-window port behavior.

## Task 1: Multi-Target CDP Discovery

**Files:**
- Modify: `src-tauri/src/cdp.rs`

- [x] **Step 1: Write failing tests**

Add tests in `src-tauri/src/cdp.rs`:

```rust
#[test]
fn cdp_returns_all_codex_page_targets() {
    let targets = vec![
        target("one", "page", "Codex", Some("ws://one")),
        target("two", "page", "Codex", Some("ws://two")),
        target("worker", "worker", "Codex", Some("ws://worker")),
        target("missing", "page", "Codex", None),
    ];

    let selected = codex_page_targets(&targets);

    assert_eq!(
        selected.iter().map(|target| target.id.as_str()).collect::<Vec<_>>(),
        vec!["one", "two"]
    );
}
```

- [x] **Step 2: Verify red**

Run: `cargo test cdp_returns_all_codex_page_targets`

Expected: compile failure because `codex_page_targets` is not defined.

- [x] **Step 3: Implement discovery helper**

Add `pub fn codex_page_targets(targets: &[CdpTarget]) -> Vec<CdpTarget>` using the existing Codex title/URL criteria and websocket requirement.

- [x] **Step 4: Verify green**

Run: `cargo test cdp_returns_all_codex_page_targets`

Expected: test passes.

## Task 2: Bridge Caller Envelope

**Files:**
- Modify: `src-tauri/src/bridge.rs`

- [x] **Step 1: Write failing tests**

Add tests proving:

```rust
let script = build_bridge_script(
    "codexHelperBridgeV1",
    &BridgeCaller::new_for_target("target-1", "instance-1"),
);
assert!(script.contains("\"targetId\":\"target-1\""));
assert!(script.contains("\"helperInstanceId\":\"instance-1\""));
```

Add a parsing test for:

```json
{"id":"1","path":"/devtools/open","payload":{},"caller":{"targetId":"target-1","helperInstanceId":"instance-1","href":"app://-/index.html","hasFocus":true,"visibilityState":"visible"}}
```

- [x] **Step 2: Verify red**

Run: `cargo test bridge_script_includes_caller_identity bridge_request_parses_caller_identity`

Expected: compile failure because caller types and parser are not defined.

- [x] **Step 3: Implement caller types and parser**

Add `BridgeCaller`, `BridgeRequest`, `parse_bridge_request`, and update `build_bridge_script` to include caller metadata automatically.

- [x] **Step 4: Verify green**

Run: `cargo test bridge_script_includes_caller_identity bridge_request_parses_caller_identity`

Expected: tests pass.

## Task 3: Caller-Aware Routes

**Files:**
- Modify: `src-tauri/src/routes.rs`
- Modify: `src-tauri/src/bridge.rs`

- [x] **Step 1: Write failing tests**

Add route tests proving `/devtools/open` uses caller `targetId` instead of `pick_codex_page_target`.

- [x] **Step 2: Verify red**

Run: `cargo test devtools_open_uses_caller_target`

Expected: compile failure or assertion failure because current route still picks the first target.

- [x] **Step 3: Implement caller-aware route context**

Update the bridge handler to pass `BridgeRequest` into route handling. Add target lookup by caller target id and make unknown target ids fail explicitly.

- [x] **Step 4: Verify green**

Run: `cargo test devtools_open_uses_caller_target`

Expected: test passes.

## Task 4: Injection Registry

**Files:**
- Modify: `src-tauri/src/codex_control.rs`
- Modify: `src-tauri/src/bridge.rs`

- [x] **Step 1: Write failing tests**

Add unit-testable pure functions for registry sync decisions:

```rust
let current = vec!["target-a".to_string(), "target-b".to_string()];
let existing = vec!["target-a".to_string(), "target-old".to_string()];
let plan = plan_injection_sync(&current, &existing);
assert_eq!(plan.inject, vec!["target-b"]);
assert_eq!(plan.retain, vec!["target-a"]);
assert_eq!(plan.prune, vec!["target-old"]);
```

- [x] **Step 2: Verify red**

Run: `cargo test injection_sync_plans_inject_retain_and_prune`

Expected: compile failure because the sync planner is missing.

- [x] **Step 3: Implement registry sync path**

Add sync planning, registry structs, and controller `sync_injected_targets`. Initial launch, Open Codex, and Restart Codex should call sync instead of a single-target inject.

- [x] **Step 4: Verify green**

Run: `cargo test injection_sync_plans_inject_retain_and_prune`

Expected: test passes.

## Task 5: Runtime Activity and Port Ownership

**Files:**
- Modify: `runtime/core.js`
- Modify: `runtime/bootstrap.js`
- Modify: `runtime/ports.js`
- Modify: `runtime/_test-ports-panel.test.js`
- Modify: `runtime/_test-port-detection.test.js`

- [x] **Step 1: Write failing tests**

Add runtime source tests proving:

```js
expect(source).toContain("runtime.ready");
expect(source).toContain("/runtime/activity");
expect(source).toContain("document.hasFocus()");
expect(source).toContain("helperWindowIsPortOwner");
```

- [x] **Step 2: Verify red**

Run: `bun test runtime/_test-ports-panel.test.js runtime/_test-port-detection.test.js`

Expected: tests fail because runtime activity and ownership guards are absent.

- [x] **Step 3: Implement runtime activity and ownership guard**

Send ready/activity diagnostics from bootstrap. Gate automatic port discovery, auto-forwarding, stale cleanup, and duplicate auto cleanup behind `helperWindowIsPortOwner()`.

- [x] **Step 4: Verify green**

Run: `bun test runtime/_test-ports-panel.test.js runtime/_test-port-detection.test.js`

Expected: tests pass.

## Task 6: Final Verification

**Files:**
- Read: `docs/superpowers/specs/2026-05-26-multi-window-injection-design.md`

- [x] **Step 1: Format**

Run: `cargo fmt`

Expected: no output and exit 0.

- [x] **Step 2: Rust tests**

Run: `cargo test`

Expected: all Rust tests pass.

Current evidence: default parallel `cargo test` passes with 82 tests.

- [x] **Step 3: Runtime tests**

Run: `bun test runtime`

Expected: all runtime tests pass.

- [x] **Step 4: Completion audit**

Read the design spec and verify each named requirement has code or test evidence. Keep the goal active if any required item remains incomplete.

## Self-Review

- Spec coverage: all design sections map to at least one task.
- Empty-marker scan: no incomplete requirements remain.
- Type consistency: `targetId`, `helperInstanceId`, `BridgeCaller`, and `BridgeRequest` names are consistent across backend and runtime tasks.
