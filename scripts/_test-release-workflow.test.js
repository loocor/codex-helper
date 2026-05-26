import { expect, test } from "bun:test";
import { readFileSync } from "node:fs";
import { join, resolve } from "node:path";

const repoRoot = resolve(import.meta.dir, "..");
const ciWorkflow = readFileSync(join(repoRoot, ".github/workflows/ci.yml"), "utf8");
const releaseWorkflow = readFileSync(join(repoRoot, ".github/workflows/release.yml"), "utf8");
const dmgScript = readFileSync(join(repoRoot, "scripts/build-macos-dmg.sh"), "utf8");

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
  const apiKeySubmit = dmgScript.indexOf("submit_notarization --key");
  const appleIdSubmit = dmgScript.indexOf("submit_notarization --apple-id");
  const firstStaple = dmgScript.indexOf("xcrun stapler staple");
  const secondStaple = dmgScript.indexOf("xcrun stapler staple", firstStaple + 1);

  expect(apiKeySubmit).toBeGreaterThan(-1);
  expect(appleIdSubmit).toBeGreaterThan(-1);
  expect(firstStaple).toBeGreaterThan(apiKeySubmit);
  expect(secondStaple).toBeGreaterThan(appleIdSubmit);
});

test("release notarization preserves failed submit status", () => {
  expect(dmgScript).toContain("else\n      status=$?");
  expect(dmgScript).not.toContain("fi\n    status=$?");
});

test("release publishing generates GitHub release notes", () => {
  expect(releaseWorkflow).toContain("generate_release_notes: true");
});

test("ci macos packaging signs notarizes and uploads the dmg", () => {
  expect(ciWorkflow).toContain("if: github.event_name == 'push' || github.event.pull_request.head.repo.full_name == github.repository");
  expect(ciWorkflow).toContain("Validate macOS signing/notarization secrets");
  expect(ciWorkflow).toContain("Import Developer ID certificate");
  expect(ciWorkflow).toContain("Prepare notary API key file");
  expect(ciWorkflow).toContain('REQUIRE_SIGNING: "1"');
  expect(ciWorkflow).toContain('REQUIRE_NOTARIZE: "1"');
  expect(ciWorkflow).toContain('NOTARY_WAIT_TIMEOUT: "10m"');
  expect(ciWorkflow).toContain('NOTARY_MAX_ATTEMPTS: "3"');
  expect(ciWorkflow).toContain("actions/upload-artifact@v4");
  expect(ciWorkflow).not.toContain("SKIP_NOTARIZE=1");
});
