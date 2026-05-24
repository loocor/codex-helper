import { expect, test } from "bun:test";
import { buildRuntimeBundle } from "./bundle.ts";

const source = buildRuntimeBundle();
const portsSource = await Bun.file(new URL("./ports.js", import.meta.url)).text();

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
  expect(source).toContain('return `${remotePort} ↔ ${localPort} · ${status}`;');
  expect(source).toContain('"unreachable"');
  expect(source).toContain('"starting"');
});

test("pinned ports rows expose an overflow mapping menu", () => {
  expect(source).toContain("function installPortForwardRowActions(");
  expect(source).toContain("function createNativePortRowActionButton(");
  expect(source).toContain("function createPortRowActionButton(");
  expect(source).toContain("function summaryActionRowTemplate(");
  expect(source).toContain("function summaryAccessoryTemplate(");
  expect(source).toContain("function createPortRowActionsAccessory(");
  expect(source).toContain("function openPortForwardRowMenu(");
  expect(source).toContain("data-codex-helper-port-row");
  expect(source).toContain("codex-helper-port-row-actions");
  expect(source).toContain("Port mapping actions");
  expect(source).toContain('"ellipsis"');
  expect(source).toContain("createNativePortRowActionButton(host, entry)");
  expect(source).toContain("nativeButton.setAttribute(helperPortCommandAttribute");
  expect(source).not.toContain('createPortRowActionButton("edit"');
  expect(source).not.toContain('createPortRowActionButton("delete"');
});

test("pinned ports section clones Environment summary list structure", () => {
  expect(source).toContain("function findEnvironmentSummarySection(");
  expect(source).toContain("function installPortForwardSectionSettings(");
  const ensureSection = source.slice(
    source.indexOf("function ensurePortsPinnedSection("),
    source.indexOf("function portForwardingUiAvailable("),
  );
  expect(ensureSection).toContain("findEnvironmentSummarySection(host)");
  expect(ensureSection).toContain("installPortForwardSectionSettings(section, host)");
  expect(ensureSection).toContain("const sectionTemplate = environment || sources");
  const findSummaryRowTemplate = source.slice(
    source.indexOf("function findSummaryRowTemplate("),
    source.indexOf("function fallbackSummaryRowTemplate("),
  );
  expect(findSummaryRowTemplate).toContain("findEnvironmentSummarySection(host)");
  expect(findSummaryRowTemplate).toContain("summary-panel-row-accessory");
});

test("pinned port forward settings button opens Helper Settings dialog", () => {
  expect(source).toContain('helperPortCommandAttribute, "show-settings-menu"');
  expect(source).toContain("function openPortForwardSettingsMenu(");
  expect(source).toContain("createPortSettingsToggleMenuItem(");
  expect(source).toContain("portForwardSettingsAnchorButton");
  expect(source).toContain("portSameLocalPort");
  expect(source).toContain("portAutoForwardWeb");
  expect(source).toContain('createPortMenuItem("open-settings", "Helper Settings"');
  expect(source).toContain('showHelperSettingsDialog({ focusSection: "port-forwarding" })');
  expect(source).toContain("data-codex-helper-port-settings-button");
});

test("pinned port icons prefer native summary panel svg templates", () => {
  expect(source).toContain("function cloneSummaryPanelIcon(");
  expect(source).toContain('svg[class*="wifi" i]');
  expect(source).toContain('svg[class*="copy" i]');
});

test("pinned ports row actions keep native accessory visible while menu is open", () => {
  expect(source).toContain('data-codex-helper-port-row-menu-open="true"');
  expect(source).toContain("portForwardMenuAnchorRow");
  expect(source).toContain("accessory.replaceChildren(button)");
  expect(source).not.toContain("padding-right: 52px");
  expect(source).not.toContain("right: 20px");
});

test("pinned ports rows render an icon and clickable local port", () => {
  expect(source).toContain("function createPortForwardIcon(");
  expect(source).toContain("function ensurePortForwardRowIcon(");
  expect(source).toContain("function setPortRowContent(");
  expect(source).toContain("function localUrlForPortEntry(");
  expect(source).toContain("codex-helper-port-row-leading-icon");
  expect(source).toContain("codex-helper-port-local-url");
  expect(source).toContain("data-codex-helper-port-local-url");
  expect(source).toContain('"open-local-url-system"');
  expect(source).toContain("Copy local address");
  expect(source).toContain('bridge("/url/open-external"');
});

test("runtime removes duplicate document listeners on reinjection", () => {
  expect(source).toContain("function removeHelperRuntimeEventListeners(");
  expect(source).toContain("function installHelperRuntimeEventListeners(");
  expect(source).toContain("document.removeEventListener(\"click\", onHelperRuntimeClick, true)");
  expect(source).toContain("closePortForwardRowMenu();");
});

test("pinned ports empty state keeps the overflow menu available", () => {
  expect(source).toContain("function installPortForwardEmptyActions(");
  expect(source).toContain("data-codex-helper-port-empty-row");
  expect(source).toContain("createCurrentSessionPortCommandSource(context)");
  expect(source).toContain("installPortForwardEmptyActions(emptyRow, context, host)");
  expect(source).toContain("Add port mapping record");
});

test("pinned ports mapping menu supports add edit and confirmed delete", () => {
  expect(source).toContain('command === "add-mapping"');
  expect(source).toContain('command === "edit-mapping"');
  expect(source).toContain('command === "delete-mapping"');
  expect(source).toContain("async function addPortMapping(");
  expect(source).toContain("async function editPortMapping(");
  expect(source).toContain("async function deletePortMapping(");
  expect(source).toContain("async function requestPortMappingInput(");
  expect(source).toContain("async function confirmPortMappingDelete(");
  expect(source).toContain("codex-helper-port-dialog-port-row");
  expect(source).toContain("codex-helper-port-dialog-arrow");
  expect(source).toContain("↔");
  expect(source).toContain("flex: 1 1 0");
  expect(source).toContain("width: 100%");
  expect(source).toContain("box-sizing: border-box");
  expect(portsSource).not.toContain("window.prompt(");
  expect(portsSource).not.toContain("window.confirm(");
  expect(source).toContain('bridge("/ports/forward"');
  expect(source).toContain('bridge("/ports/stop"');
  expect(source).toContain("Edit mapping record");
  expect(source).toContain("Delete mapping record");
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
