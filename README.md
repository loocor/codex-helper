# Codex Helper

[中文说明](README_CN.md)

Codex Helper is a lightweight local launcher for Codex desktop that adds local settings, session tools, diagnostics, and port-forwarding controls without modifying the installed Codex app.

It starts Codex through `CodexHelper.app`, injects a small runtime through the browser debugging interface, and keeps all Helper-owned state under `~/.codex-helper`.

## Features

- Launch Codex with Helper runtime enhancements enabled.
- Run silently as a macOS menu bar helper after launch.
- Add a Helper section inside Codex Settings for configuration and diagnostics.
- Show backend status, runtime logs, and bridge diagnostics from Codex Settings.
- Restore Codex's native Zed open option for remote contexts.
- Add opt-in session actions for Markdown export, session deletion, and moving sessions between workspaces.
- Keep deleted-session backups available for restore from Codex Settings.
- Detect and forward ports from remote Codex sessions.
- Open DevTools for inspecting injected runtime behavior.

## Design

Codex Helper uses external runtime injection instead of patching Codex app files. The Tauri app starts Codex with a local debugging endpoint, selects the Codex renderer target, installs a small bridge, and loads local enhancement scripts.

The default app launch is silent. Codex Helper does not show a separate manager window during normal use. After injection succeeds, configuration appears inside Codex Settings as a Helper section with pages for General, Deleted Sessions, Logs, and About.

This keeps Codex's installed application bundle unchanged. Removing Codex Helper only removes the launcher, `~/.codex-helper`, and local enhancement files.

## Safety Model

Codex Helper keeps its runtime and state outside the Codex application bundle.

- It does not patch `app.asar`.
- It stores Helper-owned logs, config, backups, and state under `~/.codex-helper`.
- Session deletion creates a local backup under `~/.codex-helper/backups` before modifying local session data.
- Feature switches are opt-in and stored in Helper config.

## Local State

Codex Helper owns one state directory:

```text
~/.codex-helper/
  logs/
  backups/
  config.json
  state.json
```

Failures are written to `~/.codex-helper/logs/codex-helper.jsonl`.

## Development

```bash
bun install
bun run check
bun run launch
env RUSTC_WRAPPER= cargo test --manifest-path src-tauri/Cargo.toml
bun run build:app
```

The Tauri backend currently targets macOS `/Applications/Codex.app` by default.

Bun is used for development scripts in this repository. A built `CodexHelper.app` runs as a Rust/Tauri application and does not require Bun at runtime; injected scripts execute inside the Codex renderer.

`bun run build:app` runs `scripts/build-macos-dmg.sh` and writes `dist/macos/CodexHelper-<version>-macos-<arch>.dmg`.

## Releases

Push a `v*` tag to build signed, notarized DMGs via GitHub Actions. See [docs/release-guide.md](docs/release-guide.md) for secrets and local signing.

## Acknowledgements

This project learned from two Codex desktop enhancement projects:

- [BigPizzaV3/CodexPlusPlus](https://github.com/BigPizzaV3/CodexPlusPlus) inspired the dynamic injection approach, and Codex Helper references parts of its implementation model while keeping the installed Codex app unchanged.
- [b-nnett/codex-plusplus](https://github.com/b-nnett/codex-plusplus) inspired the UI integration direction and User Script ideas (not yet developed in Codex Helper). Its implementation patches the Codex app directly, while Codex Helper currently uses dynamic injection, so that area remains intentionally separate.
