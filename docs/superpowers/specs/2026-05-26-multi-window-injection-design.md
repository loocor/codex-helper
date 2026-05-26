# Codex Helper Multi-Window Injection Design

## Goal

Codex Helper should support multiple open Codex windows by injecting Helper runtime capabilities into every injectable Codex page target and by making every bridge request carry an explicit caller identity. Helper should no longer rely on a single selected Codex target or infer the active window from CDP target order.

## Product Shape

Users should be able to open more than one Codex window and use Helper features from whichever Codex window they are interacting with. The Helper Settings entry, session context actions, port forwarding UI, diagnostics, and local bridge calls should behave as local enhancements inside each injected Codex window.

Target-specific commands should operate on the window that initiated the request. Global commands should stay global and make that scope clear. Session-level commands should continue to use business identifiers such as session id, host id, remote project path, and thread id.

## Current Behavior

The current backend chooses one Codex page target and injects only that target:

- `src-tauri/src/cdp.rs` exposes `pick_codex_page_target`, which returns one target.
- `src-tauri/src/codex_control.rs` calls `wait_for_codex_target` and passes one `target_id` to the bridge installer.
- `src-tauri/src/bridge.rs` attaches one CDP session to that target and installs one runtime binding pump.
- `src-tauri/src/routes.rs` route handlers such as `/devtools/open` re-run target selection instead of using the window that made the bridge request.

This is acceptable for a single Codex window but becomes ambiguous when more than one Codex page target exists.

## Definitions

Codex Helper should distinguish three identity layers:

- `targetId`: the CDP page target id. This identifies a Codex renderer target.
- `helperInstanceId`: a Helper-generated id for one runtime installation inside one target.
- Business identity: session-level fields such as `sessionId`, `hostId`, `remotePath`, and `threadId`.

These identities must not be conflated. `targetId` is for window-scoped behavior. `helperInstanceId` is for injection lifecycle and diagnostics. Business identity is for export, fork, port forwarding, and remote project actions.

## Non-Goals

This design does not add a new standalone window manager UI.

This design does not modify Codex application files or rely on private native hooks.

This design does not make user scripts singleton processes. User scripts remain per injected Codex page.

This design does not require perfect OS-level frontmost-window detection. Helper can infer active Helper instances from runtime focus and activity events.

This design does not change the security boundary of existing bridge routes. Routes remain allowlisted local Helper capabilities.

## Architecture

### Target Discovery

`src-tauri/src/cdp.rs` should add a multi-target API:

```text
codex_page_targets(targets: &[CdpTarget]) -> Vec<CdpTarget>
```

The filter should accept only targets that:

- have `target_type == "page"`;
- have a non-empty `web_socket_debugger_url`;
- match Codex by title or URL using the existing Codex target criteria.

The existing single-target picker can remain for transitional commands and tests, but multi-window injection should use the new list API.

### Injection Registry

`CodexController` should own an injection registry keyed by `targetId`:

```text
HashMap<String, InjectedTarget>
```

Each `InjectedTarget` should store:

- `target_id`;
- `helper_instance_id`;
- last known `title`;
- last known `url`;
- `last_ready_at`;
- `last_seen_at`;
- the binding pump task handle or cancellation handle.

The controller should expose a synchronization operation:

```text
sync_injected_targets()
```

The operation should:

1. Query current CDP targets.
2. Filter Codex page targets.
3. Inject any Codex target that is not already registered.
4. Keep existing registered targets whose CDP target still exists.
5. Prune destroyed targets and cancel their binding pumps.
6. Log the count of discovered, injected, retained, and pruned targets.

Initial launch, Open Codex, Restart Codex, and explicit reinjection actions should call `sync_injected_targets` instead of injecting a single target.

### Target Event Watcher

The first implementation can be polling-based through `sync_injected_targets`. A later incremental improvement should add a long-lived browser-level CDP watcher using target discovery events.

The watcher should:

- observe new page targets;
- inject new Codex page targets;
- update title and URL when target metadata changes;
- remove registry entries when targets are destroyed.

The watcher should not replace the sync operation. Sync remains the recovery path after missed events, watcher restarts, or CDP reconnects.

## Bridge Envelope

`build_bridge_script` should accept caller metadata and embed it into every bridge request. The renderer-facing `bridge(path, payload)` helper can stay simple; the low-level bridge function should add the caller envelope automatically.

Bridge payloads should become:

```json
{
  "id": "1",
  "path": "/devtools/open",
  "payload": {},
  "caller": {
    "targetId": "CDP_TARGET_ID",
    "helperInstanceId": "HELPER_INSTANCE_ID",
    "href": "app://-/index.html",
    "hasFocus": true,
    "visibilityState": "visible"
  }
}
```

The Rust bridge layer should parse this envelope into:

```text
BridgeRequest {
  id: String,
  path: String,
  payload: Value,
  caller: BridgeCaller,
}

BridgeCaller {
  target_id: String,
  helper_instance_id: String,
  href: String,
  has_focus: bool,
  visibility_state: String,
}
```

Malformed caller data should fail the request explicitly for target-scoped routes. Global routes may still run if caller data is missing, but they should log that the request was caller-less.

## Route Scope

Routes should be grouped by scope.

### Target-Scoped Routes

These routes must use caller identity:

- `/devtools/open`;
- current-window reload routes added in the future;
- runtime readiness and activity routes.

`/devtools/open` should open DevTools for `caller.targetId`. It should not call `pick_codex_page_target`.

### Global Routes

These routes remain global:

- `/backend/status`;
- `/runtime/user-scripts`;
- `/settings/get`;
- `/settings/set`;
- `/diagnostics/read-latest`;
- `/diagnostics/reveal-log`;
- `/logs/reveal`;
- `/scripts/reveal`;
- `/state/reveal`;
- `/url/open-external`;
- `/zed-remote/status`;
- `/projects/remote-list`;
- `/zed-remote/resolve-host`;
- `/zed-remote/fallback-request`;
- `/zed-remote/open`.

Global routes may record caller metadata for diagnostics, but their behavior should not depend on the caller target.

### Session-Scoped Routes

These routes should continue to use explicit business payload fields:

- `/export-markdown`;
- `/fork-thread-project`;
- `/ports/discover`;
- `/ports/forward`;
- `/ports/stop`.

The caller target should be recorded for diagnostics and ownership decisions, but the authoritative session identity remains `sessionId`, `hostId`, `remotePath`, and `threadId`.

## Runtime Activity

The runtime should report readiness after successful installation:

```text
event: runtime.ready
detail: targetId, helperInstanceId, href, hasFocus, visibilityState
```

The runtime should also report activity when focus or visibility changes:

```text
path: /runtime/activity
payload: targetId, helperInstanceId, href, hasFocus, visibilityState
```

The backend should use this to maintain:

- `last_active_target_id`;
- `last_active_helper_instance_id`;
- activity timestamps for diagnostics and ownership decisions.

This is an application-level active Helper signal, not a guaranteed OS frontmost-window detector.

## Port Forwarding Ownership

Port tunnels remain global because they are local machine resources. Auto-discovery and stale cleanup must not run with equal authority in every injected window.

The design should use an owner model:

- The active Helper instance may run remote discovery and auto-forwarding.
- Non-active Helper instances may display global tunnel state through `/ports/list`.
- Stale cleanup and duplicate auto-tunnel cleanup may only be performed by the active owner for that session context.
- Manual user actions from any window may still request explicit forward or stop operations.

The owner identity should be:

```text
ownerHelperInstanceId
ownerTargetId
sessionKey = hostId + remotePath + threadId
```

Ownership should be time-bounded. If the active owner stops heartbeating or loses focus, another focused Helper instance can become owner for the same session key.

This prevents one background Codex window from stopping a tunnel that another active Codex window is still using.

## User Scripts

User scripts should continue to run per injected Codex page. This should be documented in the Settings surface or diagnostics text when user scripts are listed.

Helper should not attempt to make user scripts singleton-global. If a user script needs singleton behavior, it should implement its own external coordination or avoid global side effects.

The existing runtime cleanup hook remains important because repeated injection into the same target should replace Helper event listeners, timers, observers, and UI roots instead of stacking duplicates.

## Diagnostics

Diagnostics should include caller identity for every bridge request where available:

- `targetId`;
- `helperInstanceId`;
- `href`;
- `hasFocus`;
- `visibilityState`;
- route path;
- result status.

Injection diagnostics should include:

- target discovery count;
- Codex page target count;
- injected target count;
- retained target count;
- pruned target count;
- per-target injection failure details.

This makes multi-window failures debuggable without requiring users to know CDP target ids.

## Error Handling

Missing caller identity for a target-scoped route should return a failed response with a clear message.

Unknown target ids should return a failed response instead of falling back to the first Codex target.

Per-target injection failure should not abort injection for other Codex targets. The sync operation should return an aggregate status and log per-target failures.

If no Codex page targets exist, launch should continue to report the existing explicit no-target error.

If CDP target listing fails, sync should surface the error and avoid clearing the registry until a later successful query can confirm target destruction.

## Testing Strategy

Unit tests should cover:

- `codex_page_targets` returns all matching Codex page targets and rejects non-page or missing-websocket targets.
- Existing single-target picker behavior remains stable until all callers migrate.
- Bridge script includes caller metadata in every request.
- Bridge request parsing rejects missing caller data for target-scoped routes.
- `/devtools/open` uses caller target id.
- Injection sync injects missing targets, keeps existing targets, and prunes destroyed targets.
- Injection sync continues after one target fails to inject.
- Port ownership allows only active owners to run auto-discovery cleanup.

Runtime tests should cover:

- `bridge(path, payload)` remains the public runtime helper.
- The low-level bridge envelope includes `targetId`, `helperInstanceId`, `href`, `hasFocus`, and `visibilityState`.
- Runtime cleanup still removes event listeners, observers, timers, and temporary UI state before reinjection.
- Non-active window port scanning does not auto-forward or stop stale tunnels.

Manual validation should cover:

- Open two Codex windows and confirm both show Helper Settings.
- Open DevTools from each window and confirm the correct window target is selected.
- Run session actions from both windows and confirm each action uses the clicked row/session payload.
- Use remote port forwarding from one active remote window and confirm a background window does not stop its tunnel.
- Close one Codex window and confirm the target registry prunes only that target.

## Implementation Phases

### Phase 1: Multi-Target Discovery

Add `codex_page_targets` and tests. Keep `pick_codex_page_target` for existing single-target callers.

### Phase 2: Bridge Caller Identity

Add `BridgeCaller` and `BridgeRequest` parsing. Update `build_bridge_script` so every request includes caller metadata.

### Phase 3: Injection Registry

Replace single-target injection in `CodexController` with `sync_injected_targets`. Initial implementation can use list-and-sync without a target event watcher.

### Phase 4: Caller-Aware Routes

Convert `/devtools/open` to operate on `caller.targetId`. Add explicit failures for missing or unknown caller targets.

### Phase 5: Runtime Activity and Port Ownership

Add runtime readiness and activity reporting. Gate automatic port discovery, auto-forwarding, and stale cleanup behind the active owner model.

### Phase 6: Optional Target Watcher

Add browser-level target event watching as a responsiveness improvement after the sync-based path is stable.

## Review Notes

The design intentionally favors full Codex page injection over active-window-only injection. Full injection ensures the active window already has Helper features when the user interacts with it. Caller identity then makes target-scoped actions precise.

The design also keeps global and session-scoped features stable. It does not move business identity into CDP target identity, and it does not make port tunnels window-local.

## Self-Review

- Empty-marker scan: no incomplete requirements remain.
- Consistency check: `targetId` is used only for CDP/window identity, `helperInstanceId` is used only for injection lifecycle, and session fields remain business identity.
- Scope check: the design is focused on multi-window injection and target identity. It does not include unrelated UI redesign or new remote services.
- Ambiguity check: target-scoped routes must use caller identity; global routes may log caller identity; session-scoped routes use explicit business payload fields.

## Implementation Review

The current implementation covers the core sync-based path:

- CDP target discovery can return all injectable Codex page targets.
- Bridge requests carry `targetId`, `helperInstanceId`, `href`, focus state, and visibility state.
- `/devtools/open` resolves DevTools for the caller target instead of selecting the first Codex target.
- `CodexController` owns an injection registry and syncs newly discovered Codex targets.
- The injection registry owns binding pump task handles and aborts them when targets are pruned.
- Runtime focus and visibility events report Helper activity.
- Automatic port discovery, auto-forwarding, and stale cleanup are gated to the focused Helper window.

The remaining production-hardening items are intentionally separate:

- Add browser-level CDP target event watching after the polling sync path is stable.
- Promote runtime activity from last-observed diagnostics to a session-keyed ownership lease if port ownership needs backend enforcement beyond the current focused-window gate.
- Add a diagnostic surface for the last active Helper target if users need support visibility.
