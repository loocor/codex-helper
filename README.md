# Codex Helper

[Chinese README](README_CN.md)

Codex Helper is a lightweight local enhancement launcher for Codex desktop.

It focuses on a small set of local and remote workflow gaps while keeping the Codex desktop experience familiar.

## Features

- **Project-level session Fork**: fork a conversation between local and remote projects, or into another project on the same side, without manually copying session files.
- **Markdown export**: export a conversation to Markdown for sharing, review, or archiving outside Codex.
- **Zed for remote projects**: open remote Codex project contexts in Zed through a native-feeling menu action.
- **Remote port forwarding**: detect and forward web ports from Codex SSH sessions so remote dev servers can be opened locally.
- **Native-feeling settings**: configure Helper features inside Codex Settings with a UI that fits the surrounding application.

## Characteristics

- **Dynamic injection**: Codex Helper keeps Codex application files unchanged, so the enhancement layer is reversible.
- **Focused scope**: it supplements uncovered workflows instead of duplicating Codex-native capabilities or adding ambiguous actions.
- **Codex-like interaction**: controls are placed in existing Codex surfaces and follow the app's visual and interaction patterns.

## Local State

Codex Helper owns one state directory:

```text
~/.codex-helper/
  logs/
  scripts/
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

`Bun` is used for development scripts in this repository. A built `CodexHelper.app` runs as a Rust/Tauri application and does not require Bun at runtime; injected scripts execute inside the Codex renderer.

`bun run build:app` runs `scripts/build-macos-dmg.sh` and writes `dist/macos/CodexHelper-<version>-macos-<arch>.dmg`.

## Releases

Push a `v*` tag to build signed, notarized DMGs via GitHub Actions. See [docs/release-guide.md](docs/release-guide.md) for secrets and local signing.

App and menu bar icons live in `src-tauri/icons/`:

- `icon.png` — application icon (committed; used for `.app` / DMG and Tauri bundle metadata)
- `tray.png` — menu bar template source (black on transparent; build generates `tray-menu.png`)

If local Rust builds fail with `sccache: error: Operation not permitted`, run Cargo with `RUSTC_WRAPPER=` as shown above.

## Acknowledgements

This project learned from the dynamic launcher approach used by BigPizzaV3/CodexPlusPlus and the tweak-oriented user experience explored by b-nnett/codex-plusplus.
