# Codex Helper

Codex Helper is a lightweight local enhancement launcher for Codex desktop.

It starts Codex through an external launcher and injects local enhancements at runtime through the browser debugging interface. It does not modify Codex application files, so it can be removed without repairing or reinstalling Codex.

The first goal is a small, inspectable app that opens Codex, installs a local runtime bridge, and loads local tweaks without requiring a separate runtime after the app is built.

## Features

- Launch Codex with runtime enhancements enabled from `CodexHelper.app`
- Inject local renderer runtime code without patching `app.asar`
- Load local tweaks from the project runtime directory
- Open DevTools for inspecting injected runtime behavior
- Keep the Codex application bundle unchanged
- Store local logs, backups, config, state, and user scripts under `~/.codex-helper`
- Configure tweaks from the injected `Codex Helper` page inside Codex Settings
- Run silently as a macOS menu bar helper after launch

Initial built-in tweaks:

- Show diagnostics for launch, bridge, runtime, and user-script failures
- Load local user scripts from `~/.codex-helper/scripts`
- Restore Codex's native Zed open option for remote contexts without adding a separate button
- Persist Settings switches for session deletion, Markdown export, and session movement

Planned built-in tweaks:

- Export conversations to Markdown
- Delete local conversation records with backup support
- Move conversations between Chat and Project contexts

## Design

Codex Helper uses external runtime injection instead of patching Codex app files. The Tauri app starts Codex with a local debugging endpoint, selects the Codex renderer target, installs a small bridge, and loads local enhancement scripts.

The default app launch is silent. Codex Helper does not show a separate manager window during normal use. After injection succeeds, the configuration surface appears inside Codex Settings as a single `Codex Helper` sidebar item.

This keeps Codex's installed application bundle unchanged. Removing Codex Helper only removes the launcher, `~/.codex-helper`, and local enhancement files.

## Local State

Codex Helper owns one state directory:

```text
~/.codex-helper/
  logs/
  backups/
  scripts/
  config.json
  state.json
```

Failures are written to `~/.codex-helper/logs/codex-helper.jsonl`. Destructive tweaks must write backups under `~/.codex-helper/backups` before modifying Codex data.

## Development

```bash
bun install
bun run check
bun run launch
env RUSTC_WRAPPER= cargo test --manifest-path src-tauri/Cargo.toml
bun run build:app
```

The Tauri backend currently targets macOS `/Applications/Codex.app` by default.

`Bun` is used for development scripts in this repository. A built `CodexHelper.app` runs as a Rust/Tauri application and does not require Bun at runtime; injected scripts execute inside the Codex renderer.

`bun run build:app` builds the Rust release binary, stages `dist/macos/stage/CodexHelper.app`, adds the standard `/Applications` DMG shortcut, copies the installed Codex icon from `/Applications/Codex.app/Contents/Resources/icon.icns`, ad-hoc signs the app, and writes `dist/macos/CodexHelper-<version>-macos-<arch>.dmg`.

If local Rust builds fail with `sccache: error: Operation not permitted`, run Cargo with `RUSTC_WRAPPER=` as shown above.

## Acknowledgements

This project learned from the dynamic launcher approach used by BigPizzaV3/CodexPlusPlus and the tweak-oriented user experience explored by b-nnett/codex-plusplus.
