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
  expect(source).toContain("Port Forwarding");
  expect(source).toContain('data-codex-helper-setting-status="portForwardingEnabled"');
  expect(source).toContain('data-codex-helper-setting-toggle="portForwardingEnabled"');
  expect(source).toContain('data-codex-helper-setting-status="portAutoForwardWeb"');
  expect(source).toContain('data-codex-helper-setting-toggle="portAutoForwardWeb"');
  expect(source).toContain('data-codex-helper-setting-status="portSameLocalPort"');
  expect(source).toContain('data-codex-helper-setting-toggle="portSameLocalPort"');
});
