import { expect, test } from "bun:test";
import { readFileSync } from "node:fs";

const releaseWorkflow = readFileSync(".github/workflows/release.yml", "utf8");
const dmgScript = readFileSync("scripts/build-macos-dmg.sh", "utf8");

test("release build keeps a step-level timeout around notarization", () => {
  expect(releaseWorkflow).toContain("timeout-minutes: 45");
});

test("release notarization uses bounded retry attempts", () => {
  expect(releaseWorkflow).toContain('NOTARY_WAIT_TIMEOUT: "10m"');
  expect(releaseWorkflow).toContain('NOTARY_MAX_ATTEMPTS: "3"');
  expect(dmgScript).toContain('NOTARY_WAIT_TIMEOUT="${NOTARY_WAIT_TIMEOUT:-10m}"');
  expect(dmgScript).toContain('NOTARY_MAX_ATTEMPTS="${NOTARY_MAX_ATTEMPTS:-3}"');
  expect(dmgScript).toContain("submit_notarization() {");
  expect(dmgScript).toContain("while (( attempt <= NOTARY_MAX_ATTEMPTS )); do");
  expect(dmgScript).toContain("Notarization attempt ${attempt}/${NOTARY_MAX_ATTEMPTS}");
  expect(dmgScript).toContain("notarytool submit");
});

test("release stapling runs only after a successful notary submission", () => {
  const firstSubmit = dmgScript.indexOf("submit_notarization");
  const firstStaple = dmgScript.indexOf("xcrun stapler staple");

  expect(firstSubmit).toBeGreaterThan(-1);
  expect(firstStaple).toBeGreaterThan(firstSubmit);
});
