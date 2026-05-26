---
title: Codex Helper Project Instructions
scope: project
description: Local development guidance for the Codex Helper launcher.
---

# Project Instructions

## Language

Use English for source code, comments, commit messages, filenames, and user-facing project documentation. Localized README translations, such as `README_CN.md`, may use their target language when the primary README remains English.

## Package Manager

Use Bun for all JavaScript and TypeScript tasks.

- Install dependencies: `bun install`
- Run scripts: `bun run <script>`
- Run one-off commands: `bunx --bun <cmd>`
- Run TypeScript files: `bun <file.ts>`

Do not add npm, pnpm, or yarn lockfiles.

## Repository Layout

- `src-tauri/`: packaged app shell (Rust/Tauri) — launch Codex, CDP, bridge routes, session storage
- `src/` (`launch.ts`, `bridge.ts`, `cdp.ts`, …): Bun dev launcher only; not injected into Codex
- `runtime/*.js`: injectable runtime source (ordered by `runtime/index.json`; tests use `_test-*.test.js`)
- Standalone runtime scripts: any `runtime/*.js` not listed in `index.json` (for example `zed-open.js`)
- `dist/bundle.js`: bundled injectable output (`bun run bundle:runtime`, gitignored)

Do not merge `src/` and `src-tauri/`; they are different languages and build targets.

The dev launcher auto-selects a Codex CDP port: it attaches to an existing debug port in the `9229`–`9260` range, or launches Codex on the first free port in that range (falling back to an ephemeral port). Override with `--debug-port` only when needed. `CODEX_HELPER_DEBUG_PORT` is set to the chosen port for bridge routes.

## Scope

Codex Helper is a small external launcher and runtime injector for Codex desktop.

The project should:

- Start Codex through an external launcher.
- Inject local runtime code through the browser debugging interface.
- Keep Codex application files unchanged.
- Keep the runtime small and auditable.

## Implementation Style

- Prefer explicit errors over silent fallback behavior.
- Keep launcher, CDP, bridge, and runtime concerns separate.
- Keep local tweak APIs small and allowlisted.
- Avoid bundled remote services, remote recommendations, and automatic remote script installation.
