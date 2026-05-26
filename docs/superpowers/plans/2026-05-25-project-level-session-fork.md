# Project-Level Session Fork Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace CodexHelper session Move/Copy UI with project-level Fork actions for local, remote, and same-side project targets.

**Architecture:** Runtime code classifies the selected session and sidebar project targets by local/remote host, then exposes only the applicable Fork actions. Rust bridge routes receive one fork request and call Codex app-server on the target side. Helper does not implement session deletion; users should use Codex archive and archived chat management for that lifecycle.

**Tech Stack:** Bun runtime tests, injected `runtime/*.js`, Rust/Tauri bridge routes, Codex app-server JSON-RPC.

---

### Task 1: Lock Runtime Fork Menu Semantics

**Files:**
- Modify: `runtime/_test-settings.test.js`
- Modify: `runtime/sessions.js`

- [ ] **Step 1: Write failing tests**

Add assertions that the runtime bundle contains the three Fork labels and no longer exposes `Copy Session` or `Move Session` as Helper menu actions.

- [ ] **Step 2: Run the focused runtime test**

Run: `bun test runtime/_test-settings.test.js`

Expected: FAIL until `runtime/sessions.js` is migrated from Copy/Move to Fork.

- [ ] **Step 3: Implement runtime Fork labels and action ids**

Replace the old `copy` / `move` session actions with `forkRemoteProject`, `forkLocalProject`, and `forkAnotherProject`.

- [ ] **Step 4: Re-run the focused runtime test**

Run: `bun test runtime/_test-settings.test.js`

Expected: PASS.

### Task 2: Add Target Filtering

**Files:**
- Modify: `runtime/_test-settings.test.js`
- Modify: `runtime/sessions.js`

- [ ] **Step 1: Write failing tests**

Add tests for target filtering helpers: local sessions can target remote projects and another local project; remote sessions can target local projects and another project on the same remote host.

- [ ] **Step 2: Run the focused runtime test**

Run: `bun test runtime/_test-settings.test.js`

Expected: FAIL until target filtering exists.

- [ ] **Step 3: Implement project target classification**

Classify project targets by `hostId`, remote/local side, and normalized path. Exclude the current project from `Fork into Another Project...`.

- [ ] **Step 4: Re-run the focused runtime test**

Run: `bun test runtime/_test-settings.test.js`

Expected: PASS.

### Task 3: Replace Bridge Contract With Fork Project

**Files:**
- Modify: `src-tauri/src/session_actions.rs`
- Modify: `src-tauri/src/codex_app_server.rs`
- Modify: `src-tauri/src/routes.rs`
- Modify: `src-tauri/src/bridge_cli.rs`
- Modify: `src/rust-bridge.ts`

- [ ] **Step 1: Write failing Rust tests**

Add tests that `/fork-thread-project` returns `status: "forked"`, preserves the source session, and returns the new target thread id.

- [ ] **Step 2: Run the focused Rust tests**

Run: `RUSTC_WRAPPER= cargo test --manifest-path src-tauri/Cargo.toml session_actions`

Expected: FAIL until the new route exists.

- [ ] **Step 3: Implement fork project route**

Add `fork_thread_project_response`, keep Helper delete/restore out of the active surface, and remove Copy/Move response functions from active route handling.

- [ ] **Step 4: Re-run the focused Rust tests**

Run: `RUSTC_WRAPPER= cargo test --manifest-path src-tauri/Cargo.toml session_actions`

Expected: PASS.

### Task 4: Verify No Old Copy/Move Surface Remains

**Files:**
- Modify: `runtime/settings.js`
- Modify: `runtime/native-settings.js`
- Modify: `src-tauri/src/settings.rs`
- Modify: `runtime/constants.js`

- [ ] **Step 1: Write or update assertions**

Assert the settings copy says Fork sessions while preserving the existing `sessionMoveEnabled` key for migration compatibility.

- [ ] **Step 2: Run all runtime tests**

Run: `bun test ./runtime`

Expected: PASS.

- [ ] **Step 3: Run TypeScript check**

Run: `bun run check`

Expected: PASS.

- [ ] **Step 4: Run Rust tests**

Run: `RUSTC_WRAPPER= cargo test --manifest-path src-tauri/Cargo.toml`

Expected: PASS.
