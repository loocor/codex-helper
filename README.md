# Codex Helper

[Chinese README](README_CN.md)

Codex Helper is a lightweight local enhancement launcher for Codex desktop.

It starts or attaches to Codex through a local CDP debugging session, injects a
small runtime into Codex windows, and adds a focused set of workflow tools while
leaving the Codex application bundle unchanged.

## Features

- **Menu bar launcher**: run Helper as a menu bar app that starts Codex when
  needed, attaches to existing CDP-enabled Codex windows when possible, and
  exposes a single `Quit Codex Helper` action. When Helper is connected to
  Codex, quitting warns that Helper features will stop while Codex windows keep
  running.
- **Multi-window injection**: keep Helper features injected across multiple
  Codex windows and route window-scoped actions, such as DevTools, to the Codex
  window that invoked them.
- **Reversible runtime cleanup**: remove Helper runtime hooks before Helper
  exits so Codex session context menus continue to work after Helper is gone.
- **Codex Settings integration**: add a native-looking `Helper` section to Codex
  Settings with `General`, `Logs`, and `About` pages.
- **Session context menu actions**: add optional session actions to Codex's
  existing sidebar context menu without replacing native items:
  - `Export Markdown`
  - `Regenerate chat title`
  - `Fork into local`
  - `Fork into remote`
  - `Fork into another project`
- **Native fork preservation**: keep Codex's built-in fork actions, including
  `Fork into same worktree`, `Fork into new worktree`, and `Fork into local`.
- **Markdown export**: export a conversation to Markdown for sharing, review, or
  archiving outside Codex. Helper can use Codex-generated names for friendlier
  Markdown filenames.
- **Chat title regeneration**: regenerate a chat title from the session menu,
  with configurable minimum and maximum generated title length.
- **Project-level session fork**: fork a conversation between local and remote
  projects, or into another project on the same side, without manually copying
  session files.
- **Remote port forwarding**: detect web ports from Codex SSH sessions, forward
  them to localhost, show forwarded ports in the conversation overview area, and
  provide menu actions for opening, copying, editing, or stopping mappings.
- **Zed integration**: open remote Codex project contexts in Zed through a
  native-feeling action when Zed is installed.
- **Diagnostics**: inspect recent Helper bridge and runtime logs from the
  injected Codex Settings page, and open DevTools for the active Codex window.

## Characteristics

- **Dynamic injection**: Helper uses CDP hooks and runtime scripts instead of
  modifying Codex application files.
- **Focused scope**: it supplements uncovered workflows instead of duplicating
  Codex-native capabilities or adding ambiguous actions.
- **Codex-like interaction**: controls are placed in existing Codex surfaces and
  follow the app's visual and interaction patterns.
- **Local-first operation**: state, logs, settings, injected scripts, and bridge
  routes stay local to the machine.
- **Best-effort multi-window support**: Helper tracks CDP targets and keeps
  injected features available across Codex windows, but it does not try to
  replace Codex's own window or session ownership model.

## Settings

Helper settings live inside Codex Settings under the `Helper` group.

The `General` page contains:

- **Integrations**: backend status, Zed availability, and DevTools for the
  current Codex window.
- **Session actions**: toggles for Markdown export and Helper fork actions.
- **Chat titles**: title regeneration, friendly Markdown filenames, and title
  length controls.
- **Port forwarding**: port detection, automatic web-port forwarding, and local
  port reuse behavior.

The `Logs` page shows the latest Helper diagnostics. The `About` page shows the
build date and repository link.

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

`Bun` is used for development scripts in this repository. A built
`CodexHelper.app` runs as a Rust/Tauri application and does not require Bun at
runtime; injected scripts execute inside the Codex renderer.

`bun run launch` is the development launcher. It mirrors the packaged app as
closely as practical, including CDP target discovery and multi-window injection
sync.

`bun run build:app` runs `scripts/build-macos-dmg.sh` and writes
`dist/macos/CodexHelper-<version>-macos-<arch>.dmg`.

## Releases

Push a `v*` tag to build signed, notarized DMGs via GitHub Actions. See
[docs/release-guide.md](docs/release-guide.md) for secrets and local signing.

App and menu bar icons live in `src-tauri/icons/`:

- `icon.png` — application icon (committed; used for `.app` / DMG and Tauri bundle metadata)
- `tray.png` — menu bar template source (black on transparent; build generates `tray-menu.png`)

If local Rust builds fail with `sccache: error: Operation not permitted`, run
Cargo with `RUSTC_WRAPPER=` as shown above.

## Acknowledgements

This project learned from the dynamic launcher approach used by
BigPizzaV3/CodexPlusPlus and the tweak-oriented user experience explored by
b-nnett/codex-plusplus.
