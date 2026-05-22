import { expect, test } from "bun:test";
import { readFileSync } from "node:fs";

const source = readFileSync(new URL("./renderer.js", import.meta.url), "utf8");

test("runtime installs a helper-owned Ports bottom panel entry", () => {
  expect(source).toContain("data-codex-helper-ports-entry");
  expect(source).toContain("function findBottomPanelPicker()");
  expect(source).toContain("function installPortsEntry()");
  expect(source).toContain("Ports");
});

test("Ports panel uses allowlisted bridge routes", () => {
  expect(source).toContain('bridge("/ports/list"');
  expect(source).toContain('bridge("/ports/forward"');
  expect(source).toContain('bridge("/ports/stop"');
});
