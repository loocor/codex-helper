import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import {
  resolveScreenshotPath,
  writeScreenshot,
} from "./validate-native-settings-helpers.mjs";
import { buildSettingsBundle } from "../runtime/bundle.ts";

const runtimeDir = join(dirname(import.meta.dir), "runtime");
const screenshotPath = resolveScreenshotPath();
const settingsSource = buildSettingsBundle();
const nativeSettingsSource = readFileSync(
  join(runtimeDir, "native-settings.js"),
  "utf8",
);
const settingsAppSource = readFileSync(
  join(runtimeDir, "settings-app.js"),
  "utf8",
);

const expectedPages = [
  { id: "general", label: "General", icon: "sliders-horizontal" },
  { id: "providers", label: "Providers", icon: "plug" },
  { id: "endpoint", label: "Endpoint", icon: "radio" },
  { id: "logs", label: "Logs", icon: "scroll-text" },
  { id: "about", label: "About", icon: "info" },
];
const labels = expectedPages.map((page) => page.label).join("|");
const iconNames = expectedPages.map((page) => page.icon).join("|");
const pageIds = expectedPages.map((page) => page.id);

const failures = [];
if (!settingsAppSource.includes("function startHelperSettingsApp(")) {
  failures.push("settings window host is missing");
}
if (!settingsAppSource.includes("helper-settings-nav")) {
  failures.push("settings window sidebar is missing");
}
if (nativeSettingsSource.includes("function findCodexSettingsSidebar(")) {
  failures.push("Helper settings still look for the Codex Settings sidebar");
}
if (nativeSettingsSource.includes("function installNativeHelperSettingsGroup(")) {
  failures.push("Helper settings still inject a Codex Settings group");
}
if (labels !== "General|Providers|Endpoint|Logs|About") {
  failures.push(`unexpected Helper labels: ${labels}`);
}
if (iconNames !== "sliders-horizontal|plug|radio|scroll-text|info") {
  failures.push(`unexpected Helper icon names: ${iconNames}`);
}
for (const pageId of pageIds) {
  if (!settingsSource.includes(`id: "${pageId}"`)) {
    failures.push(`settings bundle is missing page ${pageId}`);
  }
}
if (!settingsSource.includes("Last updated")) {
  failures.push("About page is missing the update date row");
}
if (!settingsSource.includes("github.com/loocor/codex-helper")) {
  failures.push("About page is missing the repository link");
}
if (!settingsSource.includes("codex-helper-native-settings-log-panel")) {
  failures.push("Logs page is missing the native settings log panel");
}
if (!settingsSource.includes("data-codex-helper-log-list")) {
  failures.push("Logs page is missing the summary list");
}
if (!settingsSource.includes('label: "Endpoint"')) {
  failures.push("Endpoint page is missing");
}

if (failures.length > 0) {
  console.error(failures.join("\n"));
  process.exit(1);
}

if (process.env.CODEX_HELPER_VALIDATE_SCREENSHOT) {
  writeScreenshot(
    screenshotPath,
    Buffer.from("settings-window-source-ok").toString("base64"),
  );
}

console.log(
  `Helper Settings window pages: ${labels}; icons: ${iconNames}; screenshot=${screenshotPath}`,
);
