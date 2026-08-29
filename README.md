# Codex Helper

[Chinese README](README_CN.md)

Codex Helper is a lightweight local enhancement launcher for Codex desktop.

It focuses on a small set of local and remote workflow gaps while keeping the Codex desktop experience familiar.

## Features

- **Remote port forwarding**: detect and forward web ports from Codex SSH sessions so remote dev servers can be opened locally.
- **Helper Settings**: configure Helper from a standalone window opened from the menu bar, with a UI that stays close to Codex.
- **Usage-limit overlay hide**: optionally hide the *You're out of Codex and Work usage* card. This is visual only and does not reset or bypass account limits.
- **Provider management**: switch ChatGPT desktop between Official ChatGPT login, API keys (DeepSeek, Kimi, MiniMax, DashScope, or custom), GitHub Copilot, and xAI Grok. Helper writes `~/.codex/config.toml` and a native-style model catalog. Copilot and Grok OAuth use device-code flows and store tokens in `~/.codex-helper/oauth/`.
- **Local provider proxy**: non-Official traffic goes through `127.0.0.1:3721` (`/v1/responses` and `/v1/chat/completions`). Helper injects the upstream key, sanitizes ChatGPT-desktop tool payloads, and can expose named local Endpoint keys for other agents.

## Characteristics

- **Dynamic injection**: Codex Helper keeps Codex application files unchanged, so the enhancement layer is reversible.
- **Focused scope**: it supplements uncovered workflows instead of duplicating Codex-native capabilities or adding ambiguous actions.
- **Codex-like interaction**: controls follow Codex visual patterns. Helper Settings lives in its own window so Codex Settings stays untouched.

## Local State

Codex Helper owns one state directory:

```text
~/.codex-helper/
  logs/
  scripts/
  config.json
  providers.json
  endpoint.json
  oauth/
  state.json
```

Diagnostics are written to dated JSONL files under `~/.codex-helper/logs/` (`codex-helper-YYYY-MM-DD.jsonl`, local date). Helper Settings → Logs can page through them, search their contents, and open a record to inspect its stored detail. A Diagnostics setting (off by default) can also record API-provider LLM request metadata from the local Helper proxy (secrets stripped from headers, plus a short user-message preview). Request and response bodies are not stored. Official ChatGPT login traffic is not included.

## Development

```bash
bun install
bun run check
bun run launch
env RUSTC_WRAPPER= cargo test --manifest-path src-tauri/Cargo.toml
bun run build:app
```

The Tauri backend currently targets macOS `/Applications/ChatGPT.app` by default.

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
