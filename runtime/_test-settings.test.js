import { readFileSync } from "node:fs";
import { join } from "node:path";
import { expect, test } from "bun:test";
import { buildRuntimeBundle, buildSettingsBundle } from "./bundle.ts";

const source = buildRuntimeBundle();
const settingsSource = buildSettingsBundle();
const nativeSettingsSource = readFileSync(
  join(import.meta.dir, "native-settings.js"),
  "utf8",
);
function extractFunction(name) {
  const marker = `function ${name}(`;
  const start = source.indexOf(marker);
  if (start === -1) throw new Error(`${name} not found`);
  const asyncPrefixStart = start - "async ".length;
  const functionStart =
    asyncPrefixStart >= 0 &&
    source.slice(asyncPrefixStart, start) === "async "
      ? asyncPrefixStart
      : start;
  const braceStart = source.indexOf("{", start);
  let depth = 0;
  let quote = "";
  let escaped = false;
  let lineComment = false;
  let blockComment = false;
  for (let index = braceStart; index < source.length; index += 1) {
    const char = source[index];
    const next = source[index + 1] || "";
    if (lineComment) {
      if (char === "\n") lineComment = false;
      continue;
    }
    if (blockComment) {
      if (char === "*" && next === "/") {
        blockComment = false;
        index += 1;
      }
      continue;
    }
    if (quote) {
      if (escaped) {
        escaped = false;
        continue;
      }
      if (char === "\\") {
        escaped = true;
        continue;
      }
      if (char === quote) quote = "";
      continue;
    }
    if (char === "/" && next === "/") {
      lineComment = true;
      index += 1;
      continue;
    }
    if (char === "/" && next === "*") {
      blockComment = true;
      index += 1;
      continue;
    }
    if (char === "\"" || char === "'" || char === "`") {
      quote = char;
      continue;
    }
    if (char === "{") depth += 1;
    if (char === "}") depth -= 1;
    if (depth === 0) return source.slice(functionStart, index + 1);
  }
  throw new Error(`${name} closing brace not found`);
}

test("chatgpt runtime does not inject helper settings into Codex Settings", () => {
  expect(source).not.toContain("function installNativeHelperSettingsGroup(");
  expect(source).not.toContain("function findCodexSettingsSidebar(");
  expect(source).not.toContain("function ensureCodexNativeSettingsOpen(");
  expect(source).not.toContain("function openNativeHelperSettingsFromApp(");
  expect(source).toContain("function openHelperSettingsFromRuntime(");
  expect(source).toContain('bridge("/settings/open"');
});

test("helper settings follow the OS color scheme", () => {
  expect(settingsSource).toContain("color-scheme: light dark");
  expect(settingsSource).toContain("background: Canvas");
  expect(settingsSource).toContain("color: CanvasText");
});

test("helper settings live in a standalone window bundle", () => {
  expect(settingsSource).toContain("function startHelperSettingsApp(");
  expect(settingsSource).toContain("helper-settings-shell");
  expect(settingsSource).toContain("helper-settings-nav");
  expect(settingsSource).toContain("id=\"helper-settings-content\"");
  expect(settingsSource).toContain("function openNativeHelperSettingsPage(");
  expect(settingsSource).not.toContain("function findCodexSettingsSidebar(");
  expect(settingsSource).not.toContain("function installNativeHelperSettingsGroup(");
});

test("settings page exposes port forwarding policy switches", () => {
  const templatePlaceholder = (name) => `$\{${name}}`;
  const helperToggleBinding = templatePlaceholder("helperToggleAttribute");
  const descKeyBinding = templatePlaceholder("descKey");
  const toggleKeyBinding = templatePlaceholder("toggleKey");
  expect(settingsSource).toContain("Enable port forwarding");
  expect(settingsSource).toContain('${helperSettingsSectionAttribute}="${sectionId}"');
  expect(settingsSource).toContain('nativeSettingsGroupSection("Port forwarding"');
  expect(settingsSource).toContain('"port-forwarding")');
  expect(source).toContain("function focusHelperSettingsSection(");
  expect(settingsSource).toContain(`data-codex-helper-setting-desc="${descKeyBinding}"`);
  expect(settingsSource).toContain(`${helperToggleBinding}="${toggleKeyBinding}"`);
  expect(settingsSource).toContain(
    'nativeSettingsSwitchRow("Enable port forwarding", "Detect and forward ports from agent sessions.", "portForwardingEnabled"',
  );
  expect(settingsSource).toContain(
    'nativeSettingsSwitchRow("Auto-forward detected web ports", "Open forwarded web URLs when a common dev port is detected.", "portAutoForwardWeb"',
  );
  expect(settingsSource).toContain(
    'nativeSettingsSwitchRow("Use the same local port by default", "Bind forwarded ports to the same local port number when possible.", "portSameLocalPort"',
  );
});

test("settings updates refresh port forwarding panel visibility", () => {
  expect(source).toContain("function applySettings(");
  expect(source).toContain("maintainPortsPanel();");
  expect(source).toContain("maintainUsageLimitBanner();");
  expect(source).toContain("if (featureSettings.portForwardingEnabled) schedulePortScan();");
});

test("settings page exposes provider management", () => {
  expect(settingsSource).toContain('id: "providers"');
  expect(settingsSource).toContain('label: "Providers"');
  expect(settingsSource).toContain('bridge("/providers/list")');
  expect(settingsSource).toContain('bridge("/providers/save"');
  expect(settingsSource).toContain('bridge("/providers/activate"');
  expect(settingsSource).toContain("function providerLiveRefreshMessage(");
  expect(settingsSource).toContain(
    "Helper is already using it. Start a new ChatGPT conversation to pick it up.",
  );
  expect(settingsSource).toContain(
    "Helper is already using it. Restart ChatGPT desktop so login and the model picker refresh.",
  );
  expect(settingsSource).not.toContain(
    "Restart ChatGPT desktop if it does not pick up the change.",
  );
  expect(settingsSource).toContain('bridge("/providers/delete"');
  expect(settingsSource).toContain('bridge("/providers/models"');
  expect(settingsSource).toContain("new-provider");
  expect(settingsSource).toContain("codex-helper-provider-add-button");
  expect(settingsSource).toContain("provider-fetch-models");
  expect(settingsSource).toContain("Fetch Models");
  expect(settingsSource).toContain("Add Model");
  expect(settingsSource).toContain("Catalog");
  expect(settingsSource).toContain("if (model) model.value = preset.model");
  expect(settingsSource).not.toContain("if (model && !model.value.trim()) model.value = preset.model");
  expect(settingsSource).toContain("Menu Display Name");
  expect(settingsSource).toContain("Actual Request Model");
  expect(settingsSource).toContain("Context Window");
  expect(settingsSource).toContain("Reasoning Levels");
  expect(settingsSource).toContain("catalogModels");
  expect(settingsSource).toContain("provider-catalog-add");
  expect(settingsSource).toContain("provider-select-model");
  expect(settingsSource).toContain("Re-sign in");
  expect(settingsSource).toContain("DeepSeek");
  expect(settingsSource).toContain("Kimi");
  expect(settingsSource).toContain("Usage URL");
  expect(settingsSource).toContain('data-codex-helper-provider-field="usagePageUrl"');
  expect(settingsSource).toContain("https://api.deepseek.com/v1");
  expect(settingsSource).toContain("https://platform.deepseek.com/usage");
  expect(settingsSource).toContain("https://api.moonshot.cn/v1");
  expect(settingsSource).toContain("https://platform.moonshot.cn/console");
  expect(settingsSource).toContain("MiniMax");
  expect(settingsSource).toContain("DashScope");
  expect(settingsSource).toContain("https://api.minimaxi.com/v1");
  expect(settingsSource).toContain("https://platform.minimaxi.com");
  expect(settingsSource).toContain("https://dashscope.aliyuncs.com/compatible-mode/v1");
  expect(settingsSource).toContain("https://bailian.console.aliyun.com");
  expect(settingsSource).toContain("MiniMax-M3");
  expect(settingsSource).toContain("qwen3-coder-plus");
  expect(settingsSource).toContain("GitHub Copilot");
  expect(settingsSource).not.toContain(">Add mapping");
  expect(settingsSource).not.toContain("provider-test");
  expect(settingsSource).not.toContain("provider-details");
  expect(settingsSource).toContain("provider-open");
  expect(settingsSource).toContain("/providers/reorder");
  expect(settingsSource).toContain("codex-helper-provider-drag-handle");
  expect(settingsSource).toContain("grip-vertical");
  expect(settingsSource).toContain("startProviderReorder");
  expect(settingsSource).toContain("pointerdown");
  expect(settingsSource).toContain("codex-helper-provider-reorder-ghost");
  expect(settingsSource).toContain("codex-helper-provider-reorder-layer");
  expect(settingsSource).not.toContain("localeCompare");
  expect(settingsSource).toContain("provider-edit");
  expect(settingsSource).toContain("github_copilot");
  expect(settingsSource).toContain("xai_oauth");
  expect(settingsSource).toContain("https://api.githubcopilot.com");
  expect(settingsSource).toContain("https://api.x.ai/v1");
  expect(settingsSource).toContain("isDeviceOauthMode(authMode) ? defaults.baseUrl");
  expect(settingsSource).toContain("provider-oauth-start");
  expect(settingsSource).toContain("provider-toggle-api-key");
  expect(settingsSource).toContain("provider-open-usage-url");
  expect(settingsSource).toContain("/providers/secret");
  expect(settingsSource.indexOf("Wire API")).toBeLessThan(settingsSource.indexOf("Default model"));
  expect(settingsSource).toContain("codex-helper-provider-field-row");
  expect(settingsSource).toContain("codex-helper-provider-field-label");
  expect(settingsSource).toContain("codex-helper-provider-reasoning-chip");
  expect(settingsSource).toContain("line-height: 30px");
  expect(settingsSource).not.toContain("codex-helper-provider-reasoning-menu");
  expect(settingsSource).not.toContain("provider-reasoning-toggle");
  expect(settingsSource).toContain("grid-template-columns: 132px minmax(0, 1fr)");
  expect(settingsSource).toContain('stroke-width="2.4"');
  expect(settingsSource).toContain("secret-toggle.is-revealed");
  expect(settingsSource).toContain("codex-helper-provider-api-only");
  expect(settingsSource).toContain('providerDialogRoot.setAttribute("data-auth-mode", authMode)');
  expect(settingsSource).toContain("presetLabel.hidden = oauthMode");
  expect(settingsSource).toContain(
    '[data-codex-helper-provider-dialog][data-auth-mode="xai_oauth"] .codex-helper-provider-api-only',
  );
  expect(settingsSource).toContain("codex-helper-provider-mapping-body");
  expect(settingsSource).toContain("margin-left: 148px");
  expect(settingsSource).toContain("flex-wrap: nowrap");
  expect(settingsSource).toContain("min-width: 328px");
  expect(settingsSource).toContain("overflow: visible");
  expect(settingsSource).not.toContain("codex-helper-provider-mapping-hint");
  expect(settingsSource).not.toContain("Generates Codex model_catalog_json");
  expect(settingsSource).toContain("codex-helper-provider-catalog");
  expect(settingsSource).toContain("display: contents");
  expect(settingsSource).toContain(
    "minmax(0, 1fr) minmax(0, 1fr) 132px minmax(328px, max-content) 32px",
  );
  expect(settingsSource).not.toContain("2.2fr");
  expect(settingsSource).toContain("width: max-content");
  expect(settingsSource).toContain("CONTEXT_WINDOW_PRESETS");
  expect(settingsSource).toContain('label: "128K"');
  expect(settingsSource).toContain('label: "512K"');
  expect(settingsSource).toContain("function catalogContextSelect(");
  expect(settingsSource).toContain("function parseContextWindowK(");
  expect(settingsSource).toContain("function commitCatalogContextCustom(");
  expect(settingsSource).toContain("Math.round(k * 1000)");
  expect(settingsSource).toContain('empty.textContent = "Default"');
  expect(settingsSource).toContain('others.textContent = "Others"');
  expect(settingsSource).toContain("data-codex-helper-catalog-context-custom");
  expect(settingsSource).toContain("data-codex-helper-catalog-context-custom-option");
  expect(settingsSource).toContain("codex-helper-provider-catalog-context-unit");
  expect(settingsSource).toContain('input.placeholder = "500"');
  expect(settingsSource).toContain("Custom context window in k");
  expect(settingsSource).not.toContain('input.placeholder = "tokens"');
  expect(settingsSource).not.toContain('empty.textContent = "Select"');
  expect(settingsSource).not.toContain("codex-helper-provider-radio");
  expect(settingsSource).toContain('toggle.className = "codex-helper-switch"');
  expect(settingsSource).toContain('checkbox.setAttribute("role", "switch")');
  expect(settingsSource).toContain('data-codex-helper-backend data-status="loading"');
  expect(settingsSource).toContain('[data-codex-helper-backend][data-status="ok"]');
  expect(settingsSource).toContain("light-dark(rgb(21, 128, 61), rgb(74, 222, 128))");
  expect(settingsSource).toContain('node.setAttribute("data-status", status || "error")');
  expect(settingsSource).toContain('font-weight: 400');
  expect(settingsSource).toContain("[data-codex-helper-provider-dialog] > .codex-helper-provider-dialog-actions");
  expect(settingsSource).toContain("padding: 10px 32px");
  expect(settingsSource).toContain("data-tauri-drag-region");
  expect(settingsSource).toContain("helper-settings-nav-drag");
  expect(settingsSource).toContain("helper-settings-content:has([data-codex-helper-provider-dialog])");
  expect(settingsSource).toContain("helper-settings-back");
  expect(settingsSource).toContain('nativeSettingsStandardIconSvg("chevron-right")');
  expect(settingsSource).toContain('nativeSettingsStandardIconSvg("chevron-left")');
  expect(settingsSource).not.toContain("function openProviderMenu(");
  expect(settingsSource).not.toContain("data-codex-helper-provider-menu");
  expect(settingsSource).not.toContain("min-width: min(920px, calc(100vw - 48px))");
  expect(settingsSource).not.toContain(
    "[data-codex-helper-provider-dialog] {\n        position: fixed",
  );
  expect(settingsSource).not.toContain("oauthProxy");
  expect(settingsSource).not.toContain("CLIProxyAPI");
  expect(settingsSource).toContain("The active provider cannot be deleted");
  expect(settingsSource).toContain("provider-dialog-save");
  expect(settingsSource).toContain("providers.saved");
  expect(settingsSource).not.toContain("Reset usage");
  expect(settingsSource).toContain("createProviderUsagePie");
  expect(settingsSource).toContain("codex-helper-provider-usage-pie");
  expect(settingsSource).toContain("open-provider-usage");
  expect(settingsSource).not.toContain("createProviderUsageLine");
  expect(settingsSource).not.toContain('link.textContent = "Usage"');
});

test("settings page exposes usage-limit overlay hide switch", () => {
  expect(settingsSource).toContain('nativeSettingsGroupSection("Interface"');
  expect(settingsSource).toContain("Hide usage-limit overlay");
  expect(settingsSource).toContain("This does not reset or bypass account limits.");
  expect(settingsSource).toContain('"hideUsageLimitBannerEnabled"');
  expect(source).toContain("hideUsageLimitBannerEnabled: false");
});

test("settings page exposes start at login switch", () => {
  expect(settingsSource).toContain('nativeSettingsGroupSection("Startup"');
  expect(settingsSource).toContain("Start at login");
  expect(settingsSource).toContain("Open Codex Helper when you log in to this Mac.");
  expect(settingsSource).toContain('"launchAtLoginEnabled"');
  expect(source).toContain("launchAtLoginEnabled: false");
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
  expect(settingsSource).toContain('nativeSettingsGroupSection("Integrations"');
  expect(settingsSource).not.toContain('nativeSettingsGroupSection("Session actions"');
  expect(settingsSource).toContain('nativeSettingsGroupSection("Interface"');
  expect(settingsSource).not.toContain('nativeSettingsGroupSection("Chat titles"');
  expect(settingsSource).toContain('nativeSettingsGroupSection("Port forwarding"');
  expect(settingsSource).not.toContain('>Basic</div>');
  expect(settingsSource).not.toContain('sectionHeading("Loaded scripts"');
  expect(settingsSource).not.toContain('sectionHeading("Log files"');
  expect(settingsSource).toContain("https://github.com/loocor/codex-helper");
  expect(settingsSource).toContain("function nativeSettingsAboutPageContent(");
  expect(settingsSource).not.toContain('nativeSettingsActionRow("Open in Zed"');
  expect(settingsSource).toContain("open-scripts-dir");
  expect(settingsSource).toContain("open-logs-dir");
  expect(settingsSource).not.toContain("Helper directory");
  expect(settingsSource).toContain("codex-helper-settings-scroll");
  expect(source).not.toContain('forkRemoteProject: "Fork into remote project..."');
  expect(source).not.toContain('autoRename: "Regenerate chat title"');
  expect(source).not.toContain('bridge("/auto-rename-chat"');
  expect(source).not.toContain("autoNamingRangePayload()");
  expect(source).not.toContain('move: "Move Session"');
  expect(source).not.toContain('copy: "Copy Session"');
  expect(source).not.toContain("helperSessionMenuIcon");
  expect(source).not.toContain("confirmForkSessionAction");
  expect(source).not.toContain('">Other</div>');
});

test("number settings validate the configured character range before saving", () => {
  expect(source).toContain("if (value < 1 || value > 20)");
  expect(source).toContain(
    "Settings value for ${key} must be between 1 and 20",
  );
  expect(source).toContain('logDiagnostic("settings_update_failed", { key, message })');
  expect(source).toContain('applySettings({ status: "ok", settings: featureSettings })');
});

test("account menu no longer exposes helper settings dialog entry", () => {
  expect(source).toContain("Helper Settings");
  expect(source).not.toContain("data-codex-helper-account-settings-entry");
  expect(source).not.toContain("data-codex-helper-settings-dialog");
  expect(source).not.toContain("function showHelperSettingsDialog(");
  expect(source).not.toContain("installAccountSettingsMenuItems");
});

test("helper settings window exposes the settings pages", () => {
  expect(settingsSource).toContain("data-codex-helper-native-settings-entry");
  expect(settingsSource).toContain("data-codex-helper-native-settings-page");
  expect(settingsSource).toContain('label: "User Scripts"');
  expect(settingsSource).toContain("hidden: true");
  expect(settingsSource).not.toContain('label: "Deleted Sessions"');
  expect(settingsSource).toContain('label: "Endpoint"');
  expect(settingsSource).toContain('label: "Logs"');
  expect(settingsSource).toContain('label: "About"');
  expect(settingsSource).toContain("helper-settings-page-title");
  expect(settingsSource).toContain("helper-settings-page-description");
  expect(source).not.toContain("installNativeHelperSettingsGroup");
});

test("native settings pages follow worktree-style sparse list layout", () => {
  expect(settingsSource).toContain("nativeSettingsPathHeader");
  expect(settingsSource).toContain("nativeSettingsListFooter");
  expect(settingsSource).toContain("codex-helper-native-settings-icon-button");
  expect(settingsSource).toContain("data-codex-helper-scripts-path");
  expect(settingsSource).toContain("data-codex-helper-log-status");
  expect(settingsSource).toContain("codex-helper-native-settings-log-panel");
  expect(settingsSource).toContain("data-codex-helper-log-list");
  expect(settingsSource).toContain('endpoint-generate"');
  expect(settingsSource).not.toContain("endpoint-generate-16");
  expect(settingsSource).not.toContain("Generate 16");
  expect(settingsSource).not.toContain("Generate 32");
  expect(settingsSource).toContain('nativeSettingsStandardIconSvg("dices")');
  expect(settingsSource).toContain("endpoint-copy-base");
  expect(settingsSource).toContain("endpoint-copy-model");
  expect(settingsSource).toContain("codex-helper-endpoint-models");
  expect(settingsSource).toContain("codex-helper-endpoint-model-tag");
  expect(settingsSource).toContain("data-codex-helper-endpoint-models");
  expect(settingsSource).toContain('bridge("/endpoint/get")');
  expect(settingsSource).toContain("result?.proxyError");
  expect(settingsSource).not.toContain(
    'result?.baseUrl || "http://127.0.0.1:3721/v1"',
  );
  expect(source).toContain('"open-log-file": "/diagnostics/reveal-log"');
  expect(settingsSource).not.toContain("codex-helper-native-settings-list-status");
  expect(settingsSource).not.toContain('nativeSettingsSection("User Scripts"');
  expect(settingsSource).not.toContain('nativeSettingsSection("Deleted Sessions"');
  expect(settingsSource).not.toContain('nativeSettingsSection("Logs"');
});

test("native settings switches stay visible when unchecked", () => {
  expect(source).toContain("codex-helper-switch-track");
  expect(source).toContain("codex-helper-switch-thumb");
  expect(source).toContain("transform: translateX(12px)");
  expect(settingsSource).toContain("helper-settings-page-title");
  expect(source).toContain("inset-inline: 1rem");
});

test("native settings sidebar uses contextual helper icons", () => {
  expect(settingsSource).toContain("nativeSettingsStandardIconSvg");
  expect(settingsSource).toContain("codex-helper-native-settings-sidebar-icon");
  expect(settingsSource).toContain('data-lucide="${iconName}"');
  expect(settingsSource).toContain('standardIconName: "sliders-horizontal"');
  expect(settingsSource).toContain('standardIconName: "file-code-2"');
  expect(settingsSource).toContain('standardIconName: "radio"');
  expect(settingsSource).toContain('standardIconName: "scroll-text"');
  expect(settingsSource).toContain('standardIconName: "info"');
  expect(settingsSource).toContain('case "external-link"');
  expect(settingsSource).toContain('case "chevron-right"');
  expect(settingsSource).toContain('case "chevron-left"');
  expect(settingsSource).not.toContain("setNativeSettingsEntryIcon");
});

test("native settings about page is independent from general", () => {
  expect(settingsSource).toContain('pageId === "about"');
  expect(settingsSource).toContain("Codex Helper");
  expect(settingsSource).toContain("Last updated");
  expect(settingsSource).toContain("A local runtime helper for Codex settings");
  expect(settingsSource).toContain("Project repository");
  expect(settingsSource).not.toContain('nativeSettingsExternalLinkRow(\n        "Project repository"');
});

test("runtime bundle injects the helper build date at build time", () => {
  const previous = process.env.CODEX_HELPER_BUILD_DATE;
  try {
    process.env.CODEX_HELPER_BUILD_DATE = "May 26, 2026";
    const bundled = buildRuntimeBundle();

    expect(bundled).toContain('const helperBuildDate = "May 26, 2026";');
    expect(bundled).not.toContain("__CODEX_HELPER_BUILD_DATE__");
  } finally {
    if (previous === undefined) delete process.env.CODEX_HELPER_BUILD_DATE;
    else process.env.CODEX_HELPER_BUILD_DATE = previous;
  }
});

test("runtime bundle escapes the injected helper build date", () => {
  const previous = process.env.CODEX_HELPER_BUILD_DATE;
  try {
    process.env.CODEX_HELPER_BUILD_DATE = 'May "26", 2026';
    const bundled = buildRuntimeBundle();

    expect(bundled).toContain('const helperBuildDate = "May \\"26\\", 2026";');
  } finally {
    if (previous === undefined) delete process.env.CODEX_HELPER_BUILD_DATE;
    else process.env.CODEX_HELPER_BUILD_DATE = previous;
  }
});

test("native settings surface has independent ownership markers", () => {
  expect(source).toContain("helperNativeSettingsPageAttribute");
  expect(source).toContain("helperNativeSettingsGroupAttribute");
  expect(source).toContain("helperNativeSettingsContentHostAttribute");
  expect(settingsSource).toContain("function helperSettingsContentHost(");
  expect(settingsSource).not.toContain("function stashNativeSettingsContent(");
  expect(settingsSource).not.toContain("function findNativeSettingsContentRoot(");
  expect(nativeSettingsSource).toContain("function openNativeHelperSettingsPage(");
});

test("helper settings window reports a missing content host", () => {
  expect(settingsSource).toContain('throw new Error("Helper Settings content host not found")');
  expect(settingsSource).toContain('throw new Error("Helper Settings app host not found")');
  expect(source).not.toContain('throw new Error("Native Settings sidebar not found")');
  expect(source).not.toContain('logDiagnostic("settings_open_failed"');
});

test("chatgpt runtime does not open Codex Settings to host Helper pages", () => {
  expect(source).not.toContain("function nativeSettingsMenuTriggerCandidates(");
  expect(source).not.toContain("function ensureCodexNativeSettingsOpen(");
  expect(source).not.toContain("closeNativeSettingsCandidateMenus()");
  expect(source).toContain("function openHelperSettingsFromRuntime(");
});

test("standalone helper settings dialog is not bundled", () => {
  expect(source).not.toContain("function showHelperSettingsDialog(");
  expect(source).not.toContain("function renderHelperPage(");
  expect(source).not.toContain("function clearHelperSettingsPage(");
  expect(source).not.toContain("function stashHostContent(");
  expect(source).not.toContain("function restoreStashedContent(");
  expect(source).not.toContain("helperPageAttribute");
  expect(source).not.toContain("helperEntryAttribute");
  expect(source).not.toContain("helperContentHostAttribute");
  expect(source).not.toContain("helperPageRoot");
  expect(source).not.toContain("helperContentHost");
  expect(source).not.toContain("helperContentStash");
  expect(source).not.toContain("helperDialogRoot = renderHelperPage(body,");
  expect(source).not.toContain("pageAttribute: helperDialogPageAttribute");
  expect(source).not.toContain("helperDialogRoot = renderNativeHelperSettingsPage");
  expect(source).toContain("helperNativeSettingsPageAttribute");
});

test("startup does not eagerly mount inline General settings page", () => {
  expect(source).not.toContain("showHelperSettingsPage({ refresh: true })");
  expect(source).not.toContain("showHelperSettingsPage({ refresh: false })");
});

test("chatgpt runtime no longer injects helper session actions", () => {
  expect(source).not.toContain("installSessionContextMenuBridge");
  expect(source).not.toContain("handleSessionAction");
  expect(source).not.toContain("prepareSessionContextMenu");
  expect(source).not.toContain("openProjectForkMenu");
  expect(source).not.toContain('bridge("/export-markdown"');
  expect(source).not.toContain('bridge("/fork-thread-project"');
  expect(source).not.toContain('bridge("/projects/remote-list"');
  expect(source).not.toContain("function showHelperTaskToast(");
  expect(source).not.toContain("data-codex-helper-project-fork");
});

test("helper toast remains available for port forwarding", () => {
  const toast = extractFunction("showHelperToast");
  expect(toast).toContain("codex-helper-toast-spinner");
  expect(toast).toContain("aria-live");
  expect(source).toContain('bridge("/zed-remote/fallback-request"');
  expect(source).not.toContain('bridge("/zed-remote/open"');
  expect(source).not.toContain('bridge("/zed-remote/status"');
});

test("helper no longer exposes its own session delete lifecycle", () => {
  expect(source).not.toContain("sessionDeleteEnabled");
  expect(source).not.toContain("Delete sessions");
  expect(source).not.toContain("Deleted Sessions");
  expect(source).not.toContain('sectionHeading("Deleted sessions"');
  expect(source).not.toContain("createCompactBackupRow");
  expect(source).not.toContain("archiveThreadBeforeHelperDelete");
  expect(source).not.toContain('bridge("/delete"');
  expect(source).not.toContain('bridge("/undo"');
  expect(source).not.toContain('bridge("/backups/list"');
  expect(source).not.toContain('bridge("/backups/restore"');
  expect(source).not.toContain("open-backups-dir");
  expect(source).not.toContain("data-codex-helper-backups-path");
});

test("provider save omits models until fetch and requires a default model", () => {
  expect(settingsSource).toContain("if (providerModelsFetchedThisSession)");
  expect(settingsSource).toContain("payload.models = providerFetchedModels");
  expect(settingsSource).not.toContain("catalogModels[0]?.model");
  expect(settingsSource).toContain('setProviderDialogError("Default model is required")');
  expect(settingsSource).toContain("modelsFetched: providerModelsFetchedThisSession");
  expect(settingsSource).toContain("catch (_error)");
});
