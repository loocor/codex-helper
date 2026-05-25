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
  expect(source).toContain("pendingSessionMenuContext = null;\n    if (sessionContextMenuMapRestore) sessionContextMenuMapRestore();");
  expect(source).toContain("handleSessionAction(action, context.row, context.ref)");
  expect(source).not.toContain("buildHelperSessionNativeMenuItems");
  expect(source).not.toContain("buildCodexSessionNativeMenuItems");
  expect(source).not.toContain("forwardSessionMenuAction");
  expect(source).not.toContain("showExtendedSessionContextMenu");
  expect(source).not.toContain("bridge.showContextMenu =");
});

test("session context menu hook is scoped to a pending right click", () => {
  expect(source).toContain("installSessionContextMenuBridge();\n    pendingSessionMenuContext = {");
  expect(source).toContain("const openedAt = Date.now();");
  expect(source).toContain("pendingSessionMenuContext?.openedAt === openedAt");
  expect(source).toContain("if (sessionContextMenuMapRestore) sessionContextMenuMapRestore();");
  expect(source).not.toContain("installSessionContextMenuBridge();\ninstallHelperStyles();");
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
