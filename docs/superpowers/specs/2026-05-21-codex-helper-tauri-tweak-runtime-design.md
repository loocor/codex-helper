# Codex Helper Tauri Tweak Runtime Design

## Purpose

Codex Helper is a local enhancement launcher for Codex desktop. It exists to start Codex through an external app, inject a small local runtime through CDP, and expose a focused set of local tweaks without modifying the Codex application bundle.

The project name defines the product boundary: Codex Helper should stay small, local, auditable, and free of provider routing, ads, update systems, installers, or a separate manager UI.

## Goals

- Provide a double-clickable `CodexHelper.app` entry point.
- Launch Codex desktop with a local CDP debugging endpoint.
- Inject a bridge and renderer runtime into the selected Codex page target.
- Store all Codex Helper state under `~/.codex-helper`.
- Add a single `Codex Helper` settings sidebar item inside Codex Settings after successful injection.
- Support a small local tweak runtime that can load built-in tweaks and user scripts.
- Make launch, injection, script loading, and route failures visible through logs and diagnostics.

## Non-Goals

- No provider, relay, API key, or model routing features.
- No ads, recommendations, sponsors, or remote content feeds.
- No updater, installer manager, shortcut repair, or release distribution workflow in the first version.
- No standalone manager window for normal use.
- No writes to Codex application files, `app.asar`, app bundle resources, or injected native libraries.
- No silent fallback behavior. Failures must be logged and surfaced.
- No Bun runtime dependency for the built app. Bun is a development tool only.

## User Experience

The default user action is double-clicking `CodexHelper.app`.

On normal launch, Codex Helper should not show its own app window. It starts Codex, injects the runtime, and leaves the user inside Codex. After injection succeeds, Codex Settings includes a new sidebar item named `Codex Helper`.

`Codex Helper` is the configuration surface for built-in tweaks, user scripts, and diagnostics. It should feel like a native part of Codex Settings rather than a floating overlay.

If Codex launch or injection fails before the runtime can render diagnostics inside Codex, the failure is written to `~/.codex-helper/logs`. A future manual diagnostic command may open or reveal that directory, but the first implementation only needs stable file logging.

## State Directory

Codex Helper owns one local state directory:

```text
~/.codex-helper/
  logs/
  scripts/
  config.json
  state.json
```

`logs/` stores launch, CDP, bridge, renderer, and user-script failures.

`scripts/` stores user-provided JavaScript files loaded by the tweak runtime.

`config.json` stores explicit user settings. Missing or invalid config is an error once config loading is implemented; code must not silently substitute unrelated settings.

`state.json` stores runtime state that is not user-authored, such as last launch status or last injected target metadata.

## Architecture

Codex Helper has four boundaries:

1. Tauri app shell
   - Packaged as `CodexHelper.app`.
   - Starts hidden or without a normal window by default.
   - Owns app lifecycle and invokes the Rust launcher backend.

2. Launcher backend
   - Resolves the Codex app path.
   - Starts Codex with `--remote-debugging-port`.
   - Waits for a CDP page target.
   - Installs the bridge and runtime bundle.
   - Writes logs and state under `~/.codex-helper`.

3. Bridge
   - Uses CDP `Runtime.addBinding` for renderer-to-backend calls.
   - Uses `Page.addScriptToEvaluateOnNewDocument` and immediate `Runtime.evaluate` for runtime installation.
   - Routes only allowlisted local paths such as diagnostics, user-script inventory, Zed open, Markdown export, and project-level session fork.

4. Renderer runtime
   - Runs inside Codex.
   - Installs the `Codex Helper` Settings sidebar item.
   - Loads built-in tweaks and user scripts.
   - Shows diagnostics from backend logs and runtime events.

## Initial Tweaks

The first implementation should keep built-in tweaks limited:

- Diagnostics: show bridge status, last injection status, and recent logged failures.
- User Scripts: list scripts from `~/.codex-helper/scripts` and reload the runtime.

Zed support is in scope, but it should restore Codex's existing Zed entry in native open-target menus for remote contexts instead of adding a separate Codex Helper button or path-guessing action.

Markdown export is in scope for the product but should be implemented after the launcher, bridge, runtime, and Settings UI are stable. Session deletion is intentionally out of scope for Helper because Codex owns archive, unarchive, and the archived chat lifecycle across local and remote hosts.

## Codex Settings Injection

The runtime should avoid a generic floating panel. It should find the Codex Settings surface and add a single `Codex Helper` sidebar item when Settings is present.

If Settings is not mounted yet, the runtime should observe DOM changes and install the group when the Settings UI appears.

If Codex changes its DOM and no compatible insertion point is found, the runtime should log an explicit `settings_insertion_failed` diagnostic. It should not create an unrelated fallback UI unless the user explicitly asks for one later.

## Error Handling

Failures should be explicit:

- Missing Codex app path: fail launch and log the path.
- CDP endpoint unavailable: fail launch after bounded retries and log the port.
- No injectable page target: fail injection and log target metadata.
- Bridge install failure: fail injection and log the CDP method error.
- User script read or evaluation failure: keep the core runtime running, mark that script failed, and show the failure in diagnostics.
- Settings insertion failure: log and show in diagnostics if the diagnostics surface is available.

## Testing Strategy

The first implementation should verify:

- State directory creation is deterministic.
- Launcher command construction includes the remote debugging argument.
- CDP target selection rejects missing websocket URLs.
- Bridge script defines the expected binding and callback globals.
- Runtime installer can create a `Codex Helper` item in a representative Settings DOM fixture.
- Runtime installer logs a clear failure when no Settings insertion point exists.

Runtime UI behavior should be validated with lightweight DOM tests before attempting a real Codex launch.

## References

CodexMan is a reference for external launcher and CDP injection mechanics, not a scope template. Codex Helper should borrow only the launch, CDP bridge, injection, and selected tweak-context ideas. It should not inherit Provider, ads, updater, installer, or manager systems.
