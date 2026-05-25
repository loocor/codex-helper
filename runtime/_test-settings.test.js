import { expect, test } from "bun:test";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { buildRuntimeBundle } from "./bundle.ts";

const source = buildRuntimeBundle();
const nativeSettingsSource = readFileSync(
  join(import.meta.dir, "native-settings.js"),
  "utf8",
);

const templatePlaceholder = (name) => `$\{${name}}`;

test("settings sidebar detection does not scan arbitrary div containers", () => {
  expect(source).not.toContain(
    "document.querySelectorAll(\"aside, nav, [role=\\'navigation\\'], [role=\\'tablist\\'], div\")",
  );
  expect(source).not.toContain(
    "document.querySelectorAll(\"aside, nav, [role='navigation'], [role='tablist'], div\")",
  );
});

test("clickable settings item selector excludes generic div elements", () => {
  expect(source).not.toContain(
    "const selector = \"button, a, [role=\\'button\\'], [role=\\'tab\\'], [role=\\'menuitem\\'], div\";",
  );
  expect(source).not.toContain(
    "const selector = \"button, a, [role='button'], [role='tab'], [role='menuitem'], div\";",
  );
});

test("settings page exposes port forwarding policy switches", () => {
  const helperToggleBinding = templatePlaceholder("helperToggleAttribute");
  const descKeyBinding = templatePlaceholder("descKey");
  const toggleKeyBinding = templatePlaceholder("toggleKey");
  const sectionAttrBinding = templatePlaceholder(
    "helperSettingsSectionAttribute",
  );
  expect(source).toContain("Enable port forwarding");
  expect(source).toContain(`${sectionAttrBinding}="port-forwarding"`);
  expect(source).toContain("function focusHelperSettingsSection(");
  expect(source).toContain(
    `data-codex-helper-setting-desc="${descKeyBinding}"`,
  );
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
  expect(source).toContain(
    "if (featureSettings.portForwardingEnabled) schedulePortScan();",
  );
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
  expect(source).toContain(
    'codex-helper-settings-section-title text-sm font-medium text-token-text-primary">Basic</div>',
  );
  expect(source).toContain(
    'codex-helper-settings-section-title text-sm font-medium text-token-text-primary">Sessions</div>',
  );
  expect(source).toContain(
    'codex-helper-settings-section-title text-sm font-medium text-token-text-primary">Port forwarding</div>',
  );
  expect(source).toContain('sectionHeading("Loaded scripts"');
  expect(source).toContain('sectionHeading("Deleted chats"');
  expect(source).not.toContain('sectionHeading("Deleted sessions"');
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
  expect(source).toContain("codex-helper-chat-search-input");
  expect(source).toContain("data-codex-helper-deleted-chat-search");
  expect(source).toContain("removeArchivedChatsSearchArtifacts");
  expect(source).not.toContain("function installArchivedChatsSearch(");
  expect(source).not.toContain("function scheduleArchivedChatSearch(");
  expect(source).not.toContain("function runArchivedChatSearch(");
  expect(source).not.toContain("function renderArchivedChatSearchResults(");
  expect(source).not.toContain("function createArchivedChatResultRow(");
  expect(source).not.toContain("function findArchivedChatsHeading(");
  expect(source).not.toContain("function archivedChatsListPanel(");
  expect(source).not.toContain("function archivedChatsViewLooksRemote(");
  expect(source).not.toContain("function activeCodexSidebarHostLabel(");
  expect(source).not.toContain("function archivedChatsRootLooksRemote(");
  expect(source).not.toContain("codex-helper-chat-search-archived");
  // Strings unique to the (removed) UI injection paths. The cleanup helper's
  // selector legitimately references the data attributes, so we guard on the
  // user-visible label and the HTML construction instead.
  expect(source).not.toContain("Search archived chats");
  expect(source).not.toContain('aria-label="Search archived chats"');
  expect(source).not.toContain(
    '<input class="codex-helper-chat-search-input" data-codex-helper-archived-chat-search',
  );
  expect(source).not.toContain(
    "querySelectorAll(\"h1, h2, h3, [role='heading'], div\")",
  );
  expect(source).toContain("searchChats");
  expect(source).toContain('bridge("/chats/search"');
  expect(source).toContain("codex-helper-settings-compact-title");
  expect(source).toContain("codex-helper-settings-compact-meta");
  expect(source).toContain('move: "Move Session"');
  expect(source).toContain('const order = ["export", "move", "delete"]');
  expect(source).toContain("helperSessionMenuIcon");
  expect(source).toContain("confirmMoveSession");
  expect(source).toContain("isRemoteProjectPath");
  expect(source).not.toContain('">Other</div>');
});

test("deleted chats full list keeps a bounded render cap", () => {
  expect(source).toContain("DELETED_CHAT_BACKUP_RENDER_LIMIT");
  expect(source).toContain("backups.slice(0, DELETED_CHAT_BACKUP_RENDER_LIMIT)");
});

test("deleted chats refresh preserves active search results", () => {
  expect(source).toContain("function activeDeletedChatSearchInput(");
  expect(source).toContain("runDeletedChatSearch(activeDeletedSearchInput)");
  expect(source).not.toContain("renderDeletedSessionBackups(backups);");
});

test("account menu no longer exposes helper settings dialog entry", () => {
  expect(source).toContain("Helper Settings");
  expect(source).not.toContain("data-codex-helper-account-settings-entry");
  expect(source).not.toContain("data-codex-helper-settings-dialog");
  expect(source).not.toContain("function showHelperSettingsDialog(");
  expect(source).not.toContain("installAccountSettingsMenuItems");
});

test("native settings exposes a dedicated Helper group", () => {
  expect(source).toContain("data-codex-helper-native-settings-group");
  expect(source).toContain("data-codex-helper-native-settings-entry");
  expect(source).toContain("data-codex-helper-native-settings-page");
  expect(source).toContain('label: "User Scripts"');
  expect(source).toContain("hidden: true");
  expect(source).toContain('label: "Deleted chats"');
  expect(source).toContain('label: "Logs"');
  expect(source).toContain('label: "About"');
  expect(source).toContain("installNativeHelperSettingsGroup");
  expect(source).toContain("heading-base text-token-text-primary");
  expect(source).toContain("codex-helper-native-settings-page-description");
});

test("native settings pages follow worktree-style sparse list layout", () => {
  expect(source).toContain("nativeSettingsPathHeader");
  expect(source).toContain("nativeSettingsListFooter");
  expect(source).toContain("codex-helper-native-settings-icon-button");
  expect(source).toContain("data-codex-helper-scripts-path");
  expect(source).toContain("data-codex-helper-backups-path");
  expect(source).toContain("data-codex-helper-log-status");
  expect(source).toContain("codex-helper-native-settings-log-panel");
  expect(source).toContain('"open-log-file": "/diagnostics/reveal-log"');
  expect(source).not.toContain("codex-helper-native-settings-list-status");
  expect(source).not.toContain('nativeSettingsSection("User Scripts"');
  expect(source).not.toContain('nativeSettingsSection("Deleted Sessions"');
  expect(source).not.toContain('label: "Deleted Sessions"');
  expect(source).not.toContain('nativeSettingsSection("Logs"');
});

test("native settings sidebar uses contextual helper icons", () => {
  expect(source).toContain("nativeSettingsStandardIconSvg");
  expect(source).toContain("setNativeSettingsEntryIcon");
  expect(source).toContain("codex-helper-native-settings-sidebar-icon");
  expect(source).toContain(`data-lucide="${templatePlaceholder("iconName")}"`);
  expect(source).toContain('standardIconName: "sliders-horizontal"');
  expect(source).toContain('standardIconName: "file-code-2"');
  expect(source).toContain('standardIconName: "trash-2"');
  expect(source).toContain('standardIconName: "scroll-text"');
  expect(source).toContain('standardIconName: "info"');
  expect(source).toContain('case "external-link"');
});

test("native settings about page is independent from general", () => {
  expect(source).toContain('pageId === "about"');
  expect(source).toContain("Codex Helper");
  expect(source).toContain("Last updated");
  expect(source).toContain("A local runtime helper for Codex settings");
  expect(source).toContain("Project repository");
  expect(source).not.toContain(
    'nativeSettingsExternalLinkRow(\n        "Project repository"',
  );
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
  expect(source).toContain("clearNativeHelperSettingsPage");
  expect(source).toContain("stashNativeSettingsContent");
  expect(source).toContain("restoreNativeSettingsContent");
  expect(source).toContain("findNativeSettingsContentRoot");
  expect(source).toContain("findNativeSettingsScrollContentRoot");
});

test("native settings content root lookup does not require a minimum viewport size", () => {
  expect(source).not.toContain("rect.width > 520");
  expect(source).not.toContain("rect.height > 360");
  expect(source).not.toContain("rect.width <= 520");
  expect(source).not.toContain("rect.height <= 360");

  class FakeElement {
    constructor(rect, options = {}) {
      this.rect = rect;
      this.className = options.className || "";
      this.style = options.style || {};
      this.children = [];
      this.parentElement = null;
      this.queryResults = options.queryResults || [];
    }

    append(...children) {
      for (const child of children) {
        child.parentElement = this;
        this.children.push(child);
      }
    }

    contains(node) {
      return (
        node === this || this.children.some((child) => child.contains(node))
      );
    }

    closest() {
      return null;
    }

    querySelector() {
      return null;
    }

    querySelectorAll() {
      return this.queryResults;
    }

    getBoundingClientRect() {
      return this.rect;
    }
  }

  const sidebar = new FakeElement({
    left: 0,
    top: 0,
    width: 240,
    height: 240,
    right: 240,
  });
  const compactContent = new FakeElement({
    left: 248,
    top: 0,
    width: 320,
    height: 240,
    right: 568,
  });
  const compactScrollRoot = new FakeElement(
    {
      left: 260,
      top: 12,
      width: 300,
      height: 180,
      right: 560,
    },
    { className: "scrollbar-stable", style: { overflowY: "auto" } },
  );
  compactContent.queryResults = [compactScrollRoot];
  const layout = new FakeElement({
    left: 0,
    top: 0,
    width: 568,
    height: 240,
    right: 568,
  });
  const body = new FakeElement({
    left: 0,
    top: 0,
    width: 568,
    height: 240,
    right: 568,
  });
  layout.append(sidebar, compactContent);
  body.append(layout);

  const factory = new Function(
    "HTMLElement",
    "document",
    "getComputedStyle",
    `
      const helperNativeSettingsGroupAttribute = "data-codex-helper-native-settings-group";
      const helperNativeSettingsEntryAttribute = "data-codex-helper-native-settings-entry";
      function isVisibleElement(node) {
        const rect = node.getBoundingClientRect();
        return rect.width > 0 && rect.height > 0;
      }
      ${nativeSettingsSource}
      return { findNativeSettingsContentRoot };
    `,
  );
  const { findNativeSettingsContentRoot } = factory(
    FakeElement,
    { body },
    (node) => node.style,
  );

  expect(findNativeSettingsContentRoot(sidebar)).toBe(compactScrollRoot);
});

function extractRuntimeFunction(bundleSource, name) {
  const marker = `function ${name}(`;
  const start = bundleSource.indexOf(marker);
  if (start < 0) {
    throw new Error(`function ${name} not found in bundle source`);
  }
  let depth = 0;
  let inString = null;
  let escaped = false;
  for (let i = start; i < bundleSource.length; i += 1) {
    const ch = bundleSource[i];
    if (escaped) {
      escaped = false;
      continue;
    }
    if (inString) {
      if (ch === "\\") {
        escaped = true;
      } else if (ch === inString) {
        inString = null;
      }
      continue;
    }
    if (ch === '"' || ch === "'" || ch === "`") {
      inString = ch;
      continue;
    }
    if (ch === "{") {
      depth += 1;
    } else if (ch === "}") {
      depth -= 1;
      if (depth === 0) {
        return bundleSource.slice(start, i + 1);
      }
    }
  }
  throw new Error(`function ${name} brace-match did not terminate`);
}

class FakeRuntimeNode {
  constructor(tag, options = {}) {
    this.tag = tag;
    this.attrs = options.attrs || {};
    this.className = options.className || "";
    this.children = [];
    this.parentNode = null;
  }

  append(child) {
    child.parentNode = this;
    this.children.push(child);
    return child;
  }

  walk(visit) {
    visit(this);
    for (const child of this.children) child.walk(visit);
  }

  matches(selector) {
    const sel = selector.trim();
    if (sel.startsWith("[") && sel.endsWith("]")) {
      return sel.slice(1, -1) in this.attrs;
    }
    if (sel.startsWith(".")) {
      const cls = sel.slice(1);
      return (this.className || "").split(/\s+/).includes(cls);
    }
    return false;
  }

  closest(selector) {
    const parts = selector
      .split(",")
      .map((s) => s.trim())
      .filter(Boolean);
    let cur = this;
    while (cur) {
      if (parts.some((p) => cur.matches(p))) return cur;
      cur = cur.parentNode;
    }
    return null;
  }

  remove() {
    if (!this.parentNode) return;
    const idx = this.parentNode.children.indexOf(this);
    if (idx >= 0) this.parentNode.children.splice(idx, 1);
    this.parentNode = null;
  }
}

function buildFakeDocument(root) {
  return {
    querySelectorAll(selector) {
      const parts = selector
        .split(",")
        .map((s) => s.trim())
        .filter(Boolean);
      const results = [];
      root.walk((node) => {
        if (parts.some((p) => node.matches(p))) results.push(node);
      });
      return results;
    },
  };
}

test("removeArchivedChatsSearchArtifacts strips stale archived UI without touching deleted chats", () => {
  const fnSrc = extractRuntimeFunction(
    source,
    "removeArchivedChatsSearchArtifacts",
  );
  const factory = new Function(
    "document",
    "HTMLElement",
    `${fnSrc}\nreturn removeArchivedChatsSearchArtifacts;`,
  );

  const root = new FakeRuntimeNode("ROOT");
  const archivedContainer = root.append(
    new FakeRuntimeNode("div", { className: "codex-helper-chat-search" }),
  );
  archivedContainer.append(
    new FakeRuntimeNode("input", {
      attrs: { "data-codex-helper-archived-chat-search": "" },
    }),
  );
  archivedContainer.append(
    new FakeRuntimeNode("div", {
      attrs: { "data-codex-helper-archived-chat-results": "" },
    }),
  );
  const orphanInput = root.append(
    new FakeRuntimeNode("input", {
      attrs: { "data-codex-helper-archived-chat-search": "" },
    }),
  );
  const deletedInput = root.append(
    new FakeRuntimeNode("input", {
      attrs: { "data-codex-helper-deleted-chat-search": "" },
    }),
  );

  const cleanup = factory(buildFakeDocument(root), FakeRuntimeNode);
  const removed = cleanup();

  // One wrapped container + one orphan input = two unique nodes removed.
  expect(removed).toBe(2);
  expect(archivedContainer.parentNode).toBeNull();
  expect(orphanInput.parentNode).toBeNull();
  // Deleted chats search input must never be touched by this helper.
  expect(deletedInput.parentNode).toBe(root);
  // A second sweep on a clean DOM should report zero work.
  expect(cleanup()).toBe(0);
});

test("native settings open failures surface explicit errors", () => {
  expect(source).toContain(
    'throw new Error("Native Settings sidebar not found")',
  );
  expect(source).toContain(
    'throw new Error("Native Settings content root not found")',
  );
  expect(source).toContain(
    'throw new Error("Helper settings group could not be installed")',
  );
  expect(source).toContain('logDiagnostic("settings_open_failed"');
  expect(source).not.toContain(
    'if (!openNativeHelperSettingsPage(pageId || "general"))',
  );
});

test("native settings opener can use an existing Settings menu item or trigger candidates", () => {
  expect(source).toContain("function nativeSettingsMenuTriggerCandidates(");
  expect(source).toContain("function nativeSettingsMenuTriggerScore(");
  expect(source).toContain("function isNativeSettingsAccountMenu(");
  expect(source).toContain("function nativeSettingsAccountMenuCandidates(");
  expect(source).toContain(
    "const existingMenuItem = findNativeSettingsMenuItem()",
  );
  expect(source).toContain(
    "for (const trigger of nativeSettingsMenuTriggerCandidates())",
  );
  expect(source).toContain('node.hasAttribute("aria-haspopup")');
  expect(source).toContain('label.includes("account")');
  expect(source).toContain('label.includes("profile")');
  expect(source).toContain('text.includes("Usage")');
  expect(source).toContain('text.includes("Log out")');
  expect(source).toContain("closeNativeSettingsCandidateMenus()");
});

test("standalone helper settings dialog is not bundled", () => {
  expect(source).not.toContain("function showHelperSettingsDialog(");
  expect(source).not.toContain("helperDialogRoot = renderHelperPage(body,");
  expect(source).not.toContain("pageAttribute: helperDialogPageAttribute");
  expect(source).not.toContain(
    "helperDialogRoot = renderNativeHelperSettingsPage",
  );
});

test("startup does not eagerly mount inline General settings page", () => {
  expect(source).not.toContain("showHelperSettingsPage({ refresh: true })");
  expect(source).not.toContain("showHelperSettingsPage({ refresh: false })");
});

test("session context menu extends Codex native electronBridge menu", () => {
  expect(source).toContain("installSessionContextMenuBridge");
  expect(source).toContain("sessionContextMenuMapRestore");
  expect(source).toContain("appendHelperSessionMenuItems");
  expect(source).toContain("Array.prototype.map");
  expect(source).toContain("buildHelperSessionMenuModelItems");
  expect(source).toContain("openProjectMoveMenu");
  expect(source).toContain("nativeProjectTargets");
  expect(source).toContain("helperSessionMenuIcon");
  expect(source).toContain("Move Session");
  expect(source).toContain("codex-helper-session-");
  expect(source).toContain("trackSessionContextMenu(row)");
  expect(source).not.toContain("installSessionContextMenuItems");
  expect(source).not.toContain("installElectronContextMenuHook");
  expect(source).not.toContain("promptMoveTargetPath");
});
