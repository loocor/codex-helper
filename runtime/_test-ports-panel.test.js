import { expect, test } from "bun:test";
import { buildRuntimeBundle } from "./bundle.ts";

const source = buildRuntimeBundle();

test("runtime does not install bottom panel ports UI", () => {
  expect(source).not.toContain("function findBottomPanelLayout()");
  expect(source).not.toContain("function installPortsEntry()");
  expect(source).not.toContain("function installPortsHomeCard()");
  expect(source).not.toContain("function renderPortsPanel(");
  expect(source).not.toContain("function activatePortsView(");
  expect(source).not.toContain("codex-helper-ports-table");
  expect(source).toContain("function removeLegacyPortsBottomPanelUi()");
});

test("ports UI uses allowlisted bridge routes", () => {
  expect(source).toContain('bridge("/ports/list"');
  expect(source).toContain('bridge("/ports/forward"');
  expect(source).toContain('bridge("/ports/stop"');
});

test("ports render only in pinned summary card", () => {
  expect(source).toContain("function findPinnedSummaryCard()");
  expect(source).toContain("function findSourcesSummarySection(");
  expect(source).toContain("function populatePortForwardList(");
  expect(source).toContain("function createSummaryRowFromTemplate(");
  expect(source).toContain("function renderPortsPinnedSummary(");
  expect(source).toContain("data-codex-helper-ports-pinned");
  expect(source).toContain("function removeLegacyPortsBottomPanelUi()");
  expect(source).toContain("function maintainPortsPanel()");
  expect(source).not.toContain("codex-helper-ports-pinned-heading");
});

test("ports detection uses session sidebar state and debounced pinned UI", () => {
  expect(source).toContain("function sessionContextFromDom()");
  expect(source).toContain("data-app-action-sidebar-thread-active");
  expect(source).toContain("function buildPinnedPortsSnapshot(");
  expect(source).toContain("function scheduleRefreshPortsPanel()");
  expect(source).toContain("function scanTerminalWebPorts()");
  expect(source).toContain("function terminalTextForPortScan()");
  expect(source).not.toContain("function ensureBottomPanelOpen()");
  expect(source).not.toContain("Open a terminal session to detect");
});
