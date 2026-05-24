import { expect, test } from "bun:test";
import { buildRuntimeBundle } from "./bundle.ts";

const source = buildRuntimeBundle();

test("settings sidebar detection does not scan arbitrary div containers", () => {
  expect(source).not.toContain(
    'document.querySelectorAll("aside, nav, [role=\\\'navigation\\\'], [role=\\\'tablist\\\'], div")',
  );
  expect(source).not.toContain(
    'document.querySelectorAll("aside, nav, [role=\'navigation\'], [role=\'tablist\'], div")',
  );
});

test("clickable settings item selector excludes generic div elements", () => {
  expect(source).not.toContain(
    'const selector = "button, a, [role=\\\'button\\\'], [role=\\\'tab\\\'], [role=\\\'menuitem\\\'], div";',
  );
  expect(source).not.toContain(
    'const selector = "button, a, [role=\'button\'], [role=\'tab\'], [role=\'menuitem\'], div";',
  );
});

test("settings page exposes port forwarding policy switches", () => {
  const templatePlaceholder = (name) => `$\{${name}}`;
  const helperToggleBinding = templatePlaceholder("helperToggleAttribute");
  const descKeyBinding = templatePlaceholder("descKey");
  const toggleKeyBinding = templatePlaceholder("toggleKey");
  expect(source).toContain("Enable port forwarding");
  expect(source).toContain(`data-codex-helper-setting-desc="${descKeyBinding}"`);
  expect(source).toContain(`${helperToggleBinding}="${toggleKeyBinding}"`);
  expect(source).toContain(
    'switchRow("Enable port forwarding", "Detect and forward ports from agent sessions.", "portForwardingEnabled"',
  );
  expect(source).toContain(
    'switchRow("Auto-forward detected web ports", "Open forwarded web URLs when a common dev port is detected.", "portAutoForwardWeb"',
  );
  expect(source).toContain(
    'switchRow("Use the same local port by default", "Bind forwarded ports to the same local port number when possible.", "portSameLocalPort"',
  );
});

test("settings updates refresh port forwarding panel visibility", () => {
  expect(source).toContain("function applySettings(");
  expect(source).toContain("maintainPortsPanel();");
  expect(source).toContain("if (featureSettings.portForwardingEnabled) schedulePortScan();");
});

test("disabling port forwarding stops managed tunnels", () => {
  expect(source).toContain("function handlePortForwardingDisabled(");
  expect(source).toContain("function stopAllManagedPortForwards(");
  expect(source).toContain('bridge("/ports/list"');
  expect(source).toContain('bridge("/ports/stop"');
  expect(source).toContain("detectedPorts.clear();");
  expect(source).toContain("portDiscoveryStates.clear();");
});

test("settings page groups options by feature area", () => {
  expect(source).toContain('codex-helper-settings-section-title text-sm font-medium text-token-text-primary">Basic</div>');
  expect(source).toContain('codex-helper-settings-section-title text-sm font-medium text-token-text-primary">Sessions</div>');
  expect(source).toContain('codex-helper-settings-section-title text-sm font-medium text-token-text-primary">Port forwarding</div>');
  expect(source).toContain('sectionHeading("Loaded scripts"');
  expect(source).toContain('sectionHeading("Deleted sessions"');
  expect(source).toContain('sectionHeading("Log files"');
  expect(source).toContain("https://github.com/loocor/codex-helper");
  expect(source).toContain('externalLinkRow("Project repository"');
  expect(source).toContain('actionRow("Open in Zed"');
  expect(source).toContain("sectionHeading");
  expect(source).toContain("open-scripts-dir");
  expect(source).toContain("open-backups-dir");
  expect(source).toContain("open-logs-dir");
  expect(source).toContain("codex-helper-settings-section-link");
  expect(source).not.toContain("Helper directory");
  expect(source).toContain("codex-helper-settings-scroll");
  expect(source).toContain("createCompactBackupRow");
  expect(source).toContain('move: "Move Session"');
  expect(source).toContain('const order = ["export", "move", "delete"]');
  expect(source).toContain("helperSessionMenuIcon");
  expect(source).toContain("confirmMoveSession");
  expect(source).toContain("isRemoteProjectPath");
  expect(source).not.toContain('">Other</div>');
});

test("settings dialog close control uses icon button", () => {
  expect(source).toContain('data-codex-helper-dialog-close aria-label="Close"');
  expect(source).toContain('codex-helper-settings-dialog-close');
  expect(source).not.toContain("data-codex-helper-dialog-close>Close</button>");
});

test("account menu exposes helper settings dialog entry", () => {
  expect(source).toContain("Helper Settings");
  expect(source).toContain("data-codex-helper-account-settings-entry");
  expect(source).toContain("data-codex-helper-settings-dialog");
});

test("startup does not eagerly mount inline General settings page", () => {
  expect(source).not.toContain("showHelperSettingsPage({ refresh: true })");
  expect(source).not.toContain("showHelperSettingsPage({ refresh: false })");
});

test("session context menu extends Codex native electronBridge menu", () => {
  expect(source).toContain("showExtendedSessionContextMenu");
  expect(source).toContain("buildCodexSessionNativeMenuItems");
  expect(source).toContain("openProjectMoveMenu");
  expect(source).toContain("nativeProjectTargets");
  expect(source).toContain("helperSessionMenuIcon");
  expect(source).toContain("Move Session");
  expect(source).toContain("window.electronBridge");
  expect(source).toContain("open-thread-new-window");
  expect(source).toContain("codex-helper-session-");
  expect(source).toContain("stopImmediatePropagation");
  expect(source).not.toContain("installSessionContextMenuItems");
  expect(source).not.toContain("installElectronContextMenuHook");
  expect(source).not.toContain("promptMoveTargetPath");
});
