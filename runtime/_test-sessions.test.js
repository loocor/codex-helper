import { expect, test } from "bun:test";
import { buildRuntimeBundle } from "./bundle.ts";

const source = buildRuntimeBundle();

test("session context menu patches Codex native menu instead of rebuilding it", () => {
  expect(source).toContain("installSessionContextMenuBridge()");
  expect(source).toContain("sessionContextMenuMapRestore");
  expect(source).toContain("const originalArrayMap = Array.prototype.map");
  expect(source).toContain("appendHelperSessionMenuItems(this)");
  expect(source).toContain("buildHelperSessionMenuModelItems(actions, context)");
  expect(source).toContain("isCodexSessionMenuItemId(item?.id)");
  expect(source).toContain("hasNativeSessionMenuLabels(items)");
  expect(source).toContain("nativeLabel: labels[action] || action");
  expect(source).not.toContain("label: labels[action] || action");
  expect(source).toMatch(
    /pendingSessionMenuContext = null;\s*if \(sessionContextMenuMapRestore\) sessionContextMenuMapRestore\(\);/,
  );
  expect(source).toContain("handleSessionAction(action, context.row, context.ref)");
  expect(source).not.toContain("buildHelperSessionNativeMenuItems");
  expect(source).not.toContain("buildCodexSessionNativeMenuItems");
  expect(source).not.toContain("forwardSessionMenuAction");
  expect(source).not.toContain("showExtendedSessionContextMenu");
  expect(source).not.toContain("bridge.showContextMenu =");
});

test("session context menu hook is scoped to a pending right click", () => {
  expect(source).toMatch(
    /installSessionContextMenuBridge\(\);\s*pendingSessionMenuContext = \{/,
  );
  expect(source).toContain("const openedAt = Date.now();");
  expect(source).toContain("pendingSessionMenuContext?.openedAt === openedAt");
  expect(source).toContain("if (sessionContextMenuMapRestore) sessionContextMenuMapRestore();");
  expect(source).not.toMatch(
    /installSessionContextMenuBridge\(\);\s*installHelperStyles\(\);/,
  );
});

test("session context menu waits for initial settings before replaying", () => {
  expect(source).toContain("function replaySessionContextMenu(event, target)");
  expect(source).toContain("!featureSettingsLoaded");
  expect(source).toContain("sessionContextMenuReplayInFlight");
  expect(source).toContain("event.preventDefault()");
  expect(source).toContain("event.stopImmediatePropagation()");
  expect(source).toMatch(/refreshFeatureSettings\(\)[\s\S]*replaySessionContextMenu/);
});

test("session context menu map hook restores on terminal paths", () => {
  expect(source).toContain("clearPendingSessionMenuContext()");
  expect(source).toMatch(/hasHelperSessionMenuItem\(items\)[\s\S]*clearPendingSessionMenuContext\(\);/);
  expect(source).toMatch(/try \{[\s\S]*appendHelperSessionMenuItems\(this\);[\s\S]*\} catch \(error\) \{/);
  expect(source).toContain('logDiagnostic("session_menu_patch_failed"');
});

test("session context menu does not reconstruct Codex native action ids", () => {
  expect(source).not.toContain('id: "fork-into-local"');
  expect(source).not.toContain('id: "fork-into-worktree"');
  expect(source).not.toContain('id: "copy-cwd"');
  expect(source).not.toContain('id: "mark-thread-unread"');
  expect(source).not.toContain('id: "open-thread-folder"');
  expect(source).not.toContain('id: "rename-thread"');
  expect(source).not.toContain('id: "archive-thread"');
  expect(source).not.toContain('id: "copy-session-id"');
  expect(source).not.toContain('id: "copy-app-link"');
  expect(source).not.toContain('id: "open-thread-new-window"');
});
