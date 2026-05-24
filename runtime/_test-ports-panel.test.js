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

test("pinned ports section owns its disclosure interaction", () => {
  expect(source).toContain("function installPortForwardPinnedDisclosure(");
  expect(source).toContain("togglePortForwardPinnedDisclosure(section)");
  expect(source).toContain("installPortForwardPinnedDisclosure(existing)");
  expect(source).toContain("installPortForwardPinnedDisclosure(section)");
  expect(source).toContain("data-codex-helper-ports-disclosure-installed");
});

test("pinned ports disclosure icon points right when collapsed", () => {
  expect(source).toContain("function setPortForwardPinnedIconDirection(");
  expect(source).toContain('icon.style.transform = collapsed ? "rotate(-90deg)" : ""');
  expect(source).toContain("setPortForwardPinnedIconDirection(section, collapsed)");
});

test("pinned ports summary repopulates rows even when snapshot is unchanged", () => {
  expect(source).toContain("function portForwardPinnedSectionHasRows(");
  expect(source).toContain("const section = ensurePortsPinnedSection(card);");
  expect(source).toContain(
    "snapshot === pinnedPortsLastSnapshot && portForwardPinnedSectionHasRows(section)",
  );
});

test("pinned ports summary can render from an empty Sources section template", () => {
  expect(source).toContain("function fallbackSummaryRowTemplate(");
  expect(source).toContain("fallbackSummaryRowTemplate(sources)");
  expect(source).toContain("row.textContent = text");
});

test("pinned ports rows include lifecycle status labels", () => {
  expect(source).toContain("function portStatusLabel(");
  expect(source).toContain('return `${remotePort} → ${localPort} · ${status}`;');
  expect(source).toContain('"unreachable"');
  expect(source).toContain('"starting"');
});

test("pinned ports rows expose hover actions and a mapping menu", () => {
  expect(source).toContain("function installPortForwardRowActions(");
  expect(source).toContain("function createPortRowActionButton(");
  expect(source).toContain("function openPortForwardRowMenu(");
  expect(source).toContain("data-codex-helper-port-row");
  expect(source).toContain("codex-helper-port-row-actions");
  expect(source).toContain("Edit mapping record");
  expect(source).toContain("Delete mapping record");
});

test("pinned ports mapping menu edits and deletes managed records", () => {
  expect(source).toContain('command === "edit-mapping"');
  expect(source).toContain('command === "delete-mapping"');
  expect(source).toContain("function portEntryFromCommandButton(");
  expect(source).toContain("async function editPortMapping(");
  expect(source).toContain("async function deletePortMapping(");
  expect(source).toContain('bridge("/ports/forward"');
  expect(source).toContain('bridge("/ports/stop"');
});

test("deleted pinned port mappings suppress rediscovery for the session", () => {
  expect(source).toContain("const suppressedPortMappings = new Set();");
  expect(source).toContain("function suppressPortMapping(");
  expect(source).toContain("function portMappingIsSuppressed(");
  expect(source).toContain("suppressedPortMappings.clear();");
  expect(source).toContain("portMappingIsSuppressed(context, remotePort)");
});

test("pinned ports row template falls back when Sources has no rows", () => {
  const findSummaryRowTemplate = source.slice(
    source.indexOf("function findSummaryRowTemplate("),
    source.indexOf("function findSummaryIconRowTemplate("),
  );

  expect(findSummaryRowTemplate).toContain("host.querySelectorAll");
  expect(findSummaryRowTemplate).toContain("helperPortsPinnedAttribute");
  expect(findSummaryRowTemplate).toContain("summary-panel-row-accessory");
});

test("detected lifecycle state overrides active tunnel rows in summary", () => {
  expect(source).toContain("const current = rows.get(id);");
  expect(source).toContain("status: entry.status || current.status");
  expect(source).toContain("message: entry.message || current.message || \"\"");
});

test("port forwarding disabled removes pinned summary UI", () => {
  expect(source).toContain("function removePortsPinnedSummaryUi(");
  expect(source).toContain("if (!featureSettings.portForwardingEnabled)");
  expect(source).toContain("removePortsPinnedSummaryUi();");
  expect(source).toContain("stopPortScanLoop();");
});

test("non-remote sessions do not render pinned port forwarding UI", () => {
  const maintainPortsPanelNow = source.slice(
    source.indexOf("function maintainPortsPanelNow("),
    source.indexOf("function activePortsForCurrentSession("),
  );
  const refreshPortsPanelIfVisible = source.slice(
    source.indexOf("async function refreshPortsPanelIfVisible("),
    source.indexOf("async function handlePortCommand("),
  );

  expect(maintainPortsPanelNow).toContain("!portForwardingUiAvailable()");
  expect(refreshPortsPanelIfVisible).toContain("!portForwardingUiAvailable()");
  expect(source).toContain("function portForwardingUiAvailable()");
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

test("ports panel updates when the active sidebar thread changes", () => {
  expect(source).toContain("attributeFilter:");
  expect(source).toContain('"data-app-action-sidebar-thread-active"');
  expect(source).toContain('"data-app-action-sidebar-thread-host-id"');
  expect(source).toContain('"data-app-action-sidebar-thread-kind"');
  expect(source).toContain('"data-app-action-sidebar-thread-id"');
});
