import { expect, test } from "bun:test";
import { readFileSync } from "node:fs";

const source = readFileSync(new URL("./renderer.js", import.meta.url), "utf8");

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

test("settings page groups options by feature area", () => {
  expect(source).toContain('codex-helper-settings-section-title text-sm font-medium text-token-text-primary">Basic</div>');
  expect(source).toContain('codex-helper-settings-section-title text-sm font-medium text-token-text-primary">Sessions</div>');
  expect(source).toContain('codex-helper-settings-section-title text-sm font-medium text-token-text-primary">Port forwarding</div>');
  expect(source).toContain('codex-helper-settings-section-title text-sm font-medium text-token-text-primary">Other</div>');
  expect(source).toContain("Open in Zed");
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
