import { readFileSync } from "node:fs";
import { join } from "node:path";
import { expect, test } from "bun:test";
import { buildRuntimeBundle } from "./bundle.ts";

const source = buildRuntimeBundle();
const nativeSettingsSource = readFileSync(
  join(import.meta.dir, "native-settings.js"),
  "utf8",
);

function extractFunction(name) {
  const marker = `function ${name}(`;
  const start = source.indexOf(marker);
  if (start === -1) throw new Error(`${name} not found`);
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
    if (depth === 0) return source.slice(start, index + 1);
  }
  throw new Error(`${name} closing brace not found`);
}

function loadForkProjectHelpers(document) {
  return new Function(
    "document",
    "HTMLElement",
    [
      extractFunction("displayProjectName"),
      extractFunction("normalizeWorkspacePath"),
      extractFunction("sessionRemoteHostId"),
      extractFunction("isRemoteProjectPath"),
      extractFunction("remoteProjectMetadataById"),
      extractFunction("projectsSection"),
      extractFunction("nativeProjectTargets"),
      extractFunction("sessionProjectContext"),
      extractFunction("forkActionTargetPredicate"),
      extractFunction("forkTargetsForAction"),
      extractFunction("enabledForkSessionActions"),
      extractFunction("forkedSessionPath"),
      extractFunction("codexAppServerHostId"),
      extractFunction("codexThreadId"),
      "return { nativeProjectTargets, forkTargetsForAction, enabledForkSessionActions, forkedSessionPath, sessionProjectContext, codexAppServerHostId, codexThreadId };",
    ].join("\n"),
  )(document, document.Element);
}

function fakeProjectDocument(projects, selectedPath = "") {
  class Element {
    constructor(attrs, closestAttrs = null) {
      this.attrs = attrs;
      this.closestElement = closestAttrs ? new Element(closestAttrs) : null;
      this.textContent = "";
    }

    getAttribute(name) {
      return this.attrs[name] || null;
    }

    closest(selector) {
      if (
        selector === "[data-app-action-sidebar-project-list-id]" &&
        this.closestElement
      ) {
        return this.closestElement;
      }
      return null;
    }
  }

  return {
    Element,
    querySelector(selector) {
      if (selector === "[data-app-action-sidebar-project-list-id]" && selectedPath) {
        return new Element({ "data-app-action-sidebar-project-list-id": selectedPath });
      }
      return null;
    },
    querySelectorAll(selector) {
      if (selector !== "[data-app-action-sidebar-project-row]") return [];
      return projects.map((project) => new Element(project));
    },
  };
}

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
  expect(source).toContain('${helperSettingsSectionAttribute}="port-forwarding"');
  expect(source).toContain("function focusHelperSettingsSection(");
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
  expect(source).toContain('codex-helper-settings-section-title text-sm font-medium text-token-text-primary">Auto naming</div>');
  expect(source).toContain('codex-helper-settings-section-title text-sm font-medium text-token-text-primary">Sessions</div>');
  expect(source).toContain('codex-helper-settings-section-title text-sm font-medium text-token-text-primary">Port forwarding</div>');
  expect(source).toContain('sectionHeading("Loaded scripts"');
  expect(source).toContain('sectionHeading("Log files"');
  expect(source).toContain("https://github.com/loocor/codex-helper");
  expect(source).toContain('externalLinkRow("Project repository"');
  expect(source).toContain('actionRow("Open in Zed"');
  expect(source).toContain("sectionHeading");
  expect(source).toContain("open-scripts-dir");
  expect(source).toContain("open-logs-dir");
  expect(source).toContain("codex-helper-settings-section-link");
  expect(source).not.toContain("Helper directory");
  expect(source).toContain("codex-helper-settings-scroll");
  expect(source).toContain('forkRemoteProject: "Fork into Remote Project..."');
  expect(source).toContain('forkLocalProject: "Fork into Local Project..."');
  expect(source).toContain('forkAnotherProject: "Fork into Another Project..."');
  expect(source).toContain('const order = ["autoRename", "export", "fork"]');
  expect(source).toContain('autoRename: "Regenerate chat title"');
  expect(source).toContain('bridge("/auto-rename-chat"');
  expect(source).toContain('logDiagnostic("auto_rename_chat_succeeded"');
  expect(source).toContain('logDiagnostic("auto_rename_chat_failed"');
  expect(source).toContain("await setSidebarConversationTitleForHost(");
  expect(source).toContain("autoNamingRangePayload()");
  expect(source).not.toContain('move: "Move Session"');
  expect(source).not.toContain('copy: "Copy Session"');
  expect(source).not.toContain('const order = ["export", "copy", "move", "delete"]');
  expect(source).toContain("helperSessionMenuIcon");
  expect(source).toContain("confirmForkSessionAction");
  expect(source).toContain("isRemoteProjectPath");
  expect(source).not.toContain('">Other</div>');
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
  expect(source).not.toContain('label: "Deleted Sessions"');
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
  expect(source).toContain("data-codex-helper-log-status");
  expect(source).toContain("codex-helper-native-settings-log-panel");
  expect(source).toContain('"open-log-file": "/diagnostics/reveal-log"');
  expect(source).not.toContain("codex-helper-native-settings-list-status");
  expect(source).not.toContain('nativeSettingsSection("User Scripts"');
  expect(source).not.toContain('nativeSettingsSection("Deleted Sessions"');
  expect(source).not.toContain('nativeSettingsSection("Logs"');
});

test("native settings sidebar uses contextual helper icons", () => {
  expect(source).toContain("nativeSettingsStandardIconSvg");
  expect(source).toContain("setNativeSettingsEntryIcon");
  expect(source).toContain("codex-helper-native-settings-sidebar-icon");
  expect(source).toContain('data-lucide="${iconName}"');
  expect(source).toContain('standardIconName: "sliders-horizontal"');
  expect(source).toContain('standardIconName: "file-code-2"');
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
  expect(source).not.toContain('nativeSettingsExternalLinkRow(\n        "Project repository"');
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
      return node === this || this.children.some((child) => child.contains(node));
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

test("native settings open failures surface explicit errors", () => {
  expect(source).toContain('throw new Error("Native Settings sidebar not found")');
  expect(source).toContain('throw new Error("Native Settings content root not found")');
  expect(source).toContain('throw new Error("Helper settings group could not be installed")');
  expect(source).toContain('logDiagnostic("settings_open_failed"');
  expect(source).not.toContain('if (!openNativeHelperSettingsPage(pageId || "general"))');
});

test("native settings opener can use an existing Settings menu item or trigger candidates", () => {
  expect(source).toContain("function nativeSettingsMenuTriggerCandidates(");
  expect(source).toContain("function nativeSettingsMenuTriggerScore(");
  expect(source).toContain("function isNativeSettingsAccountMenu(");
  expect(source).toContain("function nativeSettingsAccountMenuCandidates(");
  expect(source).toContain("const existingMenuItem = findNativeSettingsMenuItem()");
  expect(source).toContain("for (const trigger of nativeSettingsMenuTriggerCandidates())");
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
  expect(source).not.toContain("helperDialogRoot = renderNativeHelperSettingsPage");
});

test("startup does not eagerly mount inline General settings page", () => {
  expect(source).not.toContain("showHelperSettingsPage({ refresh: true })");
  expect(source).not.toContain("showHelperSettingsPage({ refresh: false })");
});

test("session context menu extends Codex native electronBridge menu", () => {
  expect(source).toContain("showExtendedSessionContextMenu");
  expect(source).toContain("buildCodexSessionNativeMenuItems");
  expect(source).toContain("openProjectForkMenu");
  expect(source).toContain("navigateAfterFork(result, target)");
  expect(source).toContain("Regenerate chat title");
  expect(source).toContain("markdown_friendly_filename_succeeded");
  expect(source).toContain("markdown_friendly_filename_failed");
  expect(source).toContain("showHelperToast(result.warning || result.message || \"Forked\")");
  expect(source).toContain("window.location.assign(path)");
  expect(source).toContain("nativeProjectTargets");
  expect(source).toContain("helperSessionMenuIcon");
  expect(source).toContain("Fork into Another Project...");
  expect(source).not.toContain("Move Session");
  expect(source).toContain("window.electronBridge");
  expect(source).toContain("open-thread-new-window");
  expect(source).toContain("loadRemoteProjectMetadataOrEmpty");
  expect(source).toContain('logDiagnostic("remote_project_metadata_unavailable"');
  expect(source).toContain("codex-helper-session-");
  expect(source).toContain("stopImmediatePropagation");
  expect(source).not.toContain('id: "mark-thread-unread"');
  expect(source).not.toContain('id: "fork-into-local"');
  expect(source).not.toContain('id: "fork-into-worktree"');
  expect(source).not.toContain("installSessionContextMenuItems");
  expect(source).not.toContain("installElectronContextMenuHook");
  expect(source).not.toContain("promptMoveTargetPath");
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

test("fork project actions filter local session targets by side", () => {
  const document = fakeProjectDocument(
    [
      {
        "data-app-action-sidebar-project-id": "/repo/current",
        "data-app-action-sidebar-project-label": "current",
      },
      {
        "data-app-action-sidebar-project-id": "/repo/other",
        "data-app-action-sidebar-project-label": "other",
      },
      {
        "data-app-action-sidebar-project-id": "/srv/remote",
        "data-app-action-sidebar-project-label": "remote",
        "data-app-action-sidebar-project-host-id": "remote-ssh-codex-managed:box",
      },
      {
        "data-app-action-sidebar-project-id": "019e5587-1ab5-7eb2-a3cd-b5f481ef9639",
        "data-app-action-sidebar-project-label": "unknown remote",
      },
    ],
    "/repo/current",
  );
  const helpers = loadForkProjectHelpers(document);
  const row = new document.Element({
    "data-app-action-sidebar-thread-id": "local:thread-1",
    "data-app-action-sidebar-thread-cwd": "/repo/current",
  });

  expect(helpers.enabledForkSessionActions(row)).toEqual([
    "forkRemoteProject",
    "forkAnotherProject",
  ]);
  expect(helpers.forkTargetsForAction("forkRemoteProject", row).map((target) => target.path)).toEqual([
    "/srv/remote",
  ]);
  expect(helpers.forkTargetsForAction("forkAnotherProject", row).map((target) => target.path)).toEqual([
    "/repo/other",
  ]);
});

test("fork project context uses the session row project ancestor", () => {
  const document = fakeProjectDocument(
    [
      {
        "data-app-action-sidebar-project-id": "/repo/codmate",
        "data-app-action-sidebar-project-label": "CodMate",
      },
      {
        "data-app-action-sidebar-project-id": "/repo/current",
        "data-app-action-sidebar-project-label": "current",
      },
    ],
    "/repo/codmate",
  );
  const helpers = loadForkProjectHelpers(document);
  const row = new document.Element(
    {
      "data-app-action-sidebar-thread-id": "local:thread-1",
    },
    {
      "data-app-action-sidebar-project-list-id": "/repo/current",
    },
  );

  expect(helpers.forkTargetsForAction("forkAnotherProject", row).map((target) => target.path)).toEqual([
    "/repo/codmate",
  ]);
});

test("fork project actions resolve remote project ids from metadata", () => {
  const document = fakeProjectDocument([
    {
      "data-app-action-sidebar-project-id": "/repo/local",
      "data-app-action-sidebar-project-label": "local",
    },
    {
      "data-app-action-sidebar-project-id": "remote-project-1",
      "data-app-action-sidebar-project-label": "CodMate",
    },
    {
      "data-app-action-sidebar-project-id": "remote-project-2",
      "data-app-action-sidebar-project-label": "MCPMate",
    },
  ]);
  const remoteProjects = [
    {
      id: "remote-project-1",
      hostId: "remote-ssh-codex-managed:box",
      remotePath: "/srv/codmate",
      label: "CodMate",
    },
    {
      id: "remote-project-2",
      hostId: "remote-ssh-codex-managed:box",
      remotePath: "/srv/mcpmate",
      label: "MCPMate",
    },
  ];
  const helpers = loadForkProjectHelpers(document);
  const localRow = new document.Element({
    "data-app-action-sidebar-thread-id": "local:thread-1",
    "data-app-action-sidebar-thread-cwd": "/repo/local",
  });
  const remoteRow = new document.Element(
    {
      "data-app-action-sidebar-thread-id": "local:thread-2",
      "data-app-action-sidebar-thread-host-id": "remote-ssh-codex-managed:box",
    },
    {
      "data-app-action-sidebar-project-list-id": "remote-project-1",
    },
  );

  expect(
    helpers.forkTargetsForAction("forkRemoteProject", localRow, remoteProjects),
  ).toEqual([
    {
      path: "/srv/codmate",
      label: "CodMate (Remote)",
      remote: true,
      hostId: "remote-ssh-codex-managed:box",
    },
    {
      path: "/srv/mcpmate",
      label: "MCPMate (Remote)",
      remote: true,
      hostId: "remote-ssh-codex-managed:box",
    },
  ]);
  expect(
    helpers.forkTargetsForAction("forkAnotherProject", remoteRow, remoteProjects).map(
      (target) => target.path,
    ),
  ).toEqual(["/srv/mcpmate"]);
});

test("fork project actions keep remote same-side targets on the same host", () => {
  const document = fakeProjectDocument(
    [
      {
        "data-app-action-sidebar-project-id": "/repo/local",
        "data-app-action-sidebar-project-label": "local",
      },
      {
        "data-app-action-sidebar-project-id": "/srv/current",
        "data-app-action-sidebar-project-label": "current",
        "data-app-action-sidebar-project-host-id": "remote-ssh-codex-managed:box",
      },
      {
        "data-app-action-sidebar-project-id": "/srv/other",
        "data-app-action-sidebar-project-label": "other",
        "data-app-action-sidebar-project-host-id": "remote-ssh-codex-managed:box",
      },
      {
        "data-app-action-sidebar-project-id": "/srv/other-host",
        "data-app-action-sidebar-project-label": "other-host",
        "data-app-action-sidebar-project-host-id": "remote-ssh-codex-managed:other",
      },
    ],
    "/srv/current",
  );
  const helpers = loadForkProjectHelpers(document);
  const row = new document.Element({
    "data-app-action-sidebar-thread-id": "remote:thread-1",
    "data-app-action-sidebar-thread-cwd": "/srv/current",
    "data-app-action-sidebar-thread-host-id": "remote-ssh-codex-managed:box",
  });

  expect(helpers.enabledForkSessionActions(row)).toEqual([
    "forkLocalProject",
    "forkAnotherProject",
  ]);
  expect(helpers.forkTargetsForAction("forkLocalProject", row).map((target) => target.path)).toEqual([
    "/repo/local",
  ]);
  expect(helpers.forkTargetsForAction("forkAnotherProject", row).map((target) => target.path)).toEqual([
    "/srv/other",
  ]);
});

test("fork success navigates only for local forked sessions", () => {
  const helpers = loadForkProjectHelpers(fakeProjectDocument([]));

  expect(
    helpers.forkedSessionPath(
      { new_session_id: "local:019e5f6d-9b04-78c1-8d4c-9c33774967a9" },
      { path: "/repo/target", hostId: "" },
    ),
  ).toBe("/local/019e5f6d-9b04-78c1-8d4c-9c33774967a9");
  expect(
    helpers.forkedSessionPath(
      { new_session_id: "019e5f6d-9b04-78c1-8d4c-9c33774967a9" },
      { path: "/srv/target", hostId: "remote-ssh-codex-managed:box" },
    ),
  ).toBe("");
});

test("fork success refreshes sidebar through Codex recent conversations manager", () => {
  expect(source).toContain("await refreshSidebarAfterFork(target)");
  expect(source).toContain(
    'await manager.refreshRecentConversations({ sortKey: "updated_at" })',
  );
  expect(source).toContain('"sidebar_refresh_manager_missing"');
});

test("auto rename updates Codex sidebar manager before refreshing", () => {
  expect(source).toContain("function codexThreadId(sessionId)");
  expect(source).toContain(
    "async function setSidebarConversationTitleForHost(hostId, sessionId, title)",
  );
  expect(source).not.toContain('manager.sendRequest("thread/name/set"');
  expect(source).toContain("manager.applyThreadTitleUpdateAndNotify({");
  expect(source).toContain('"sidebar_title_update_failed"');
  expect(source).toContain("await setSidebarConversationTitleForHost(");
  expect(source).toContain("await refreshSidebarConversationsForHost(context.hostId)");
});

test("auto rename preserves remote host context for sidebar title updates", () => {
  const document = fakeProjectDocument([], "/srv/current");
  const helpers = loadForkProjectHelpers(document);
  const row = new document.Element({
    "data-app-action-sidebar-thread-id": "remote:thread-1",
    "data-app-action-sidebar-thread-cwd": "/srv/current",
    "data-app-action-sidebar-thread-host-id": "remote-ssh-codex-managed:box",
  });

  expect(helpers.sessionProjectContext(row)).toEqual({
    hostId: "remote-ssh-codex-managed:box",
    remote: true,
    path: "/srv/current",
  });
  expect(helpers.codexAppServerHostId("remote-ssh-codex-managed:box")).toBe(
    "remote-ssh-codex-managed:box",
  );
  expect(helpers.codexThreadId("remote:thread-1")).toBe("thread-1");
  expect(source).toContain("host_id: context.hostId");
  expect(source).toContain(
    "setSidebarConversationTitleForHost(\n        context.hostId",
  );
});

test("codex app-server helpers normalize host ids", () => {
  const helpers = loadForkProjectHelpers(fakeProjectDocument([]));

  expect(helpers.codexAppServerHostId("")).toBe("local");
  expect(helpers.codexAppServerHostId("local")).toBe("local");
  expect(helpers.codexAppServerHostId("remote-ssh-codex-managed:box")).toBe(
    "remote-ssh-codex-managed:box",
  );
});
