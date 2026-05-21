---
title: Codex Helper Project Instructions
scope: project
description: Local development guidance for the Codex Helper launcher.
---

# Project Instructions

## Language

Use English for source code, comments, commit messages, filenames, and user-facing project documentation.

## Package Manager

Use Bun for all JavaScript and TypeScript tasks.

- Install dependencies: `bun install`
- Run scripts: `bun run <script>`
- Run one-off commands: `bunx --bun <cmd>`
- Run TypeScript files: `bun <file.ts>`

Do not add npm, pnpm, or yarn lockfiles.

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
