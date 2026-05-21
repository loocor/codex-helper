(() => {
  if (typeof document === "undefined") return;

  const helperEntryAttribute = "data-codex-helper-settings-entry";
  const helperContentHostAttribute = "data-codex-helper-content-host";
  const helperPageAttribute = "data-codex-helper-settings-page";
  const helperCommandAttribute = "data-codex-helper-command";
  const helperToggleAttribute = "data-codex-helper-setting-toggle";
  const helperContextMenuAttribute = "data-codex-helper-session-menu";
  const helperToastAttribute = "data-codex-helper-toast";
  const helperZedAttribute = "data-codex-helper-zed-menu-item";
  const settingsLabels = [
    "General",
    "Appearance",
    "Connections",
    "Git",
    "Usage & billing",
    "Configuration",
    "Personalization",
    "Keyboard shortcuts",
    "MCP servers",
    "Hooks",
    "Browser",
    "Computer use",
    "Environments",
    "Worktrees",
    "Archived chats",
  ];
  let observerInstalled = false;
  let settingsFailureLogged = false;
  let helperContentHost = null;
  let helperPageRoot = null;
  let pendingSessionMenuContext = null;
  let featureSettings = {
    sessionDeleteEnabled: false,
    markdownExportEnabled: false,
    sessionMoveEnabled: false,
  };

  function bridge(path, payload = {}) {
    if (typeof window.__codexHelperBridge !== "function") {
      return Promise.resolve({
        status: "failed",
        message: "Codex Helper bridge is not installed",
      });
    }
    return window.__codexHelperBridge(path, payload);
  }

  function logDiagnostic(event, detail = {}) {
    bridge("/diagnostics/log", {
      event,
      detail,
      href: window.location.href,
    }).catch((error) => {
      console.warn("[Codex Helper] diagnostic log failed", error);
    });
  }

  function textOf(node) {
    return (node?.textContent || "").replace(/\s+/g, " ").trim();
  }

  function exactText(node, value) {
    return textOf(node) === value;
  }

  function replaceTextNodes(node, from, to) {
    const walker = document.createTreeWalker(node, NodeFilter.SHOW_TEXT);
    const textNodes = [];
    while (walker.nextNode()) textNodes.push(walker.currentNode);
    for (const textNode of textNodes) {
      if ((textNode.nodeValue || "").trim() === from) {
        textNode.nodeValue = (textNode.nodeValue || "").replace(from, to);
      }
    }
  }

  function isSettingsSidebar(candidate) {
    if (!(candidate instanceof HTMLElement)) return false;
    if (candidate.querySelector(`[${helperPageAttribute}]`)) return false;
    const text = textOf(candidate);
    const matchedLabels = settingsLabels.filter((label) => text.includes(label));
    const rect = candidate.getBoundingClientRect();
    return matchedLabels.length >= 5 && rect.width > 120 && rect.width < 520;
  }

  function findSettingsSidebar() {
    const candidates = Array.from(
      document.querySelectorAll("aside, nav, [role='navigation'], [role='tablist'], div"),
    );
    return candidates.find(isSettingsSidebar) || null;
  }

  function findClickableSettingsItem(sidebar, label) {
    const selector = "button, a, [role='button'], [role='tab'], [role='menuitem'], div";
    return Array.from(sidebar.querySelectorAll(selector)).find((node) => {
      if (!(node instanceof HTMLElement)) return false;
      if (node.closest(`[${helperEntryAttribute}]`)) return false;
      if (!exactText(node, label)) return false;
      const rect = node.getBoundingClientRect();
      return rect.width > 80 && rect.height > 18;
    });
  }

  function createFallbackEntry() {
    const button = document.createElement("button");
    button.type = "button";
    button.textContent = "Codex Helper";
    button.style.cssText = [
      "display:flex",
      "align-items:center",
      "width:100%",
      "border:0",
      "background:transparent",
      "color:inherit",
      "font:inherit",
      "text-align:left",
      "padding:7px 10px",
      "border-radius:8px",
      "cursor:pointer",
    ].join(";");
    return button;
  }

  function createSettingsEntry(sidebar) {
    const template =
      findClickableSettingsItem(sidebar, "Appearance") ||
      findClickableSettingsItem(sidebar, "General");
    const entry = template ? template.cloneNode(true) : createFallbackEntry();
    if (!(entry instanceof HTMLElement)) return createFallbackEntry();
    entry.setAttribute(helperEntryAttribute, "true");
    entry.setAttribute("data-active", "false");
    entry.removeAttribute("aria-selected");
    entry.removeAttribute("data-state");
    replaceTextNodes(entry, textOf(template || entry), "Codex Helper");
    if (!textOf(entry).includes("Codex Helper")) entry.textContent = "Codex Helper";
    entry.addEventListener(
      "click",
      (event) => {
        event.preventDefault();
        event.stopPropagation();
        showHelperSettingsPage();
      },
      true,
    );
    return entry;
  }

  function installHelperStyles() {
    if (document.getElementById("codex-helper-runtime-style")) return;
    const style = document.createElement("style");
    style.id = "codex-helper-runtime-style";
    style.textContent = `
      [${helperEntryAttribute}][data-active="true"] {
        background: color-mix(in srgb, currentColor 10%, transparent) !important;
      }
      [data-codex-helper-muted-selected="true"] {
        background: transparent !important;
        box-shadow: none !important;
      }
      [${helperContentHostAttribute}][data-codex-helper-active="true"] > :not([${helperPageAttribute}]) {
        display: none !important;
      }
      [${helperPageAttribute}] {
        box-sizing: border-box;
        width: min(100%, 900px);
        margin: 0 auto;
        padding: 70px 0 48px;
        color: inherit;
      }
      [${helperPageAttribute}] h1 {
        margin: 0 0 48px;
        font-size: 24px;
        font-weight: 650;
        letter-spacing: 0;
      }
      [${helperPageAttribute}] h2 {
        margin: 32px 0 14px;
        font-size: 15px;
        font-weight: 650;
        letter-spacing: 0;
      }
      [${helperPageAttribute}] .codex-helper-panel {
        border: 1px solid color-mix(in srgb, currentColor 12%, transparent);
        border-radius: 8px;
        overflow: hidden;
        background: color-mix(in srgb, currentColor 2%, transparent);
      }
      [${helperPageAttribute}] .codex-helper-row {
        display: grid;
        grid-template-columns: minmax(0, 1fr) auto;
        gap: 18px;
        align-items: center;
        min-height: 58px;
        padding: 13px 16px;
        border-top: 1px solid color-mix(in srgb, currentColor 9%, transparent);
      }
      [${helperPageAttribute}] .codex-helper-row:first-child {
        border-top: 0;
      }
      [${helperPageAttribute}] .codex-helper-label {
        font-size: 14px;
        font-weight: 560;
        line-height: 1.35;
      }
      [${helperPageAttribute}] .codex-helper-detail {
        margin-top: 3px;
        color: color-mix(in srgb, currentColor 58%, transparent);
        font-size: 13px;
        line-height: 1.38;
      }
      [${helperPageAttribute}] button {
        border: 0;
        border-radius: 8px;
        padding: 7px 11px;
        background: color-mix(in srgb, currentColor 8%, transparent);
        color: inherit;
        font: inherit;
        font-size: 13px;
        cursor: pointer;
      }
      [${helperPageAttribute}] button:hover {
        background: color-mix(in srgb, currentColor 13%, transparent);
      }
      [${helperPageAttribute}] .codex-helper-switch {
        position: relative;
        display: inline-flex;
        align-items: center;
        width: 38px;
        height: 22px;
      }
      [${helperPageAttribute}] .codex-helper-switch input {
        position: absolute;
        inset: 0;
        opacity: 0;
        cursor: pointer;
      }
      [${helperPageAttribute}] .codex-helper-switch span {
        width: 38px;
        height: 22px;
        border-radius: 999px;
        background: color-mix(in srgb, currentColor 12%, transparent);
        transition: background 120ms ease;
      }
      [${helperPageAttribute}] .codex-helper-switch span::after {
        content: "";
        position: absolute;
        top: 3px;
        left: 3px;
        width: 16px;
        height: 16px;
        border-radius: 50%;
        background: white;
        box-shadow: 0 1px 3px color-mix(in srgb, black 22%, transparent);
        transition: transform 120ms ease;
      }
      [${helperPageAttribute}] .codex-helper-switch input:checked + span {
        background: rgb(48, 145, 255);
      }
      [${helperPageAttribute}] .codex-helper-switch input:checked + span::after {
        transform: translateX(16px);
      }
      [${helperPageAttribute}] pre {
        max-height: 260px;
        overflow: auto;
        margin: 0;
        padding: 14px 16px;
        white-space: pre-wrap;
        word-break: break-word;
        font-size: 12px;
        line-height: 1.45;
        color: color-mix(in srgb, currentColor 72%, transparent);
      }
      [${helperZedAttribute}] {
        cursor: pointer;
      }
      [${helperContextMenuAttribute}] .codex-helper-menu-separator {
        height: 1px;
        margin: 5px 2px;
        background: color-mix(in srgb, currentColor 12%, transparent);
      }
      [${helperToastAttribute}] {
        position: fixed;
        right: 24px;
        bottom: 24px;
        z-index: 2147483647;
        max-width: min(420px, calc(100vw - 48px));
        border-radius: 10px;
        padding: 10px 12px;
        background: color-mix(in srgb, Canvas 96%, currentColor 4%);
        color: CanvasText;
        border: 1px solid color-mix(in srgb, currentColor 12%, transparent);
        box-shadow: 0 12px 34px color-mix(in srgb, black 18%, transparent);
        font-size: 13px;
      }
      [${helperToastAttribute}] button {
        margin-left: 10px;
        border: 0;
        border-radius: 7px;
        padding: 5px 8px;
        background: color-mix(in srgb, currentColor 10%, transparent);
        color: inherit;
        font: inherit;
        cursor: pointer;
      }
    `;
    document.head.appendChild(style);
  }

  function installSettingsEntry() {
    installHelperStyles();
    if (document.querySelector(`[${helperEntryAttribute}]`)) return true;
    const sidebar = findSettingsSidebar();
    if (!sidebar) {
      if (!settingsFailureLogged) {
        settingsFailureLogged = true;
        logDiagnostic("settings_sidebar_not_found", {});
      }
      return false;
    }
    const entry = createSettingsEntry(sidebar);
    const usageBilling = findClickableSettingsItem(sidebar, "Usage & billing");
    if (usageBilling?.parentElement) {
      usageBilling.parentElement.insertAdjacentElement("afterend", entry);
    } else {
      sidebar.appendChild(entry);
    }
    logDiagnostic("settings_entry_injected", {});
    return true;
  }

  function findSettingsContentRoot() {
    const heading = Array.from(document.querySelectorAll("h1, h2")).find((node) =>
      settingsLabels.some((label) => exactText(node, label)),
    );
    if (heading instanceof HTMLElement) {
      let node = heading;
      let selected = null;
      while (node.parentElement && node.parentElement !== document.body) {
        const parent = node.parentElement;
        const rect = parent.getBoundingClientRect();
        if (rect.left > 340 && rect.width > 360 && rect.height > 420) selected = parent;
        node = parent;
      }
      if (selected) return selected;
    }
    return findSettingsContentRootFromSidebar();
  }

  function findSettingsContentRootFromSidebar() {
    const sidebar = findSettingsSidebar();
    if (!(sidebar instanceof HTMLElement)) return null;
    const sidebarRect = sidebar.getBoundingClientRect();
    const candidates = Array.from(document.querySelectorAll("main, section, article, div"))
      .filter((node) => node instanceof HTMLElement)
      .map((node) => {
        const rect = node.getBoundingClientRect();
        return {
          node,
          rect,
          text: textOf(node),
          area: rect.width * rect.height,
        };
      })
      .filter((item) => {
        if (item.node.closest(`[${helperEntryAttribute}]`)) return false;
        if (item.node.querySelector(`[${helperEntryAttribute}]`)) return false;
        return (
          item.rect.left >= sidebarRect.right - 24 &&
          item.rect.width > 520 &&
          item.rect.height > 420 &&
          item.text.length > 20 &&
          ((item.text.includes("General") && item.text.includes("Work mode")) ||
            (item.text.includes("Work mode") && item.text.includes("Default permissions")) ||
            (item.text.includes("Default open destination") && item.text.includes("Language")) ||
            settingsLabels.some((label) => item.text.startsWith(label)))
        );
      })
      .sort((a, b) => b.area - a.area);
    return candidates[0]?.node || null;
  }

  function setEntryActive(active) {
    const entry = document.querySelector(`[${helperEntryAttribute}]`);
    if (entry instanceof HTMLElement) entry.setAttribute("data-active", active ? "true" : "false");
    if (active) {
      muteNativeSettingsSelection();
    } else {
      clearNativeSettingsSelectionMute();
    }
  }

  function clearNativeSettingsSelectionMute() {
    for (const node of document.querySelectorAll("[data-codex-helper-muted-selected]")) {
      node.removeAttribute("data-codex-helper-muted-selected");
    }
  }

  function muteNativeSettingsSelection() {
    clearNativeSettingsSelectionMute();
    const sidebar = findSettingsSidebar();
    if (!sidebar) return;
    for (const label of settingsLabels) {
      const item = findClickableSettingsItem(sidebar, label);
      if (item instanceof HTMLElement) {
        item.setAttribute("data-codex-helper-muted-selected", "true");
      }
    }
  }

  function renderHelperPage(host) {
    host.querySelectorAll(`[${helperPageAttribute}]`).forEach((node) => node.remove());
    host.setAttribute(helperContentHostAttribute, "true");
    host.setAttribute("data-codex-helper-active", "true");
    const page = document.createElement("section");
    page.setAttribute(helperPageAttribute, "true");
    page.innerHTML = `
        <h1>Codex Helper</h1>

        <h2>Runtime</h2>
        <div class="codex-helper-panel">
          <div class="codex-helper-row">
            <div>
              <div class="codex-helper-label">Backend</div>
              <div class="codex-helper-detail" data-codex-helper-backend>Loading</div>
            </div>
            <button type="button" ${helperCommandAttribute}="refresh">Refresh</button>
          </div>
          <div class="codex-helper-row">
            <div>
              <div class="codex-helper-label">DevTools</div>
              <div class="codex-helper-detail">Available</div>
            </div>
            <button type="button" ${helperCommandAttribute}="open-devtools">Open</button>
          </div>
          <div class="codex-helper-row">
            <div>
              <div class="codex-helper-label">Helper directory</div>
              <div class="codex-helper-detail">~/.codex-helper</div>
            </div>
            <button type="button" ${helperCommandAttribute}="open-state-dir">Open</button>
          </div>
        </div>

        <h2>Session Tools</h2>
        <div class="codex-helper-panel">
          <div class="codex-helper-row">
            <div>
              <div class="codex-helper-label">Delete sessions</div>
              <div class="codex-helper-detail" data-codex-helper-setting-status="sessionDeleteEnabled">Loading</div>
            </div>
            <label class="codex-helper-switch" aria-label="Delete sessions">
              <input type="checkbox" ${helperToggleAttribute}="sessionDeleteEnabled">
              <span></span>
            </label>
          </div>
          <div class="codex-helper-row">
            <div>
              <div class="codex-helper-label">Markdown export</div>
              <div class="codex-helper-detail" data-codex-helper-setting-status="markdownExportEnabled">Loading</div>
            </div>
            <label class="codex-helper-switch" aria-label="Markdown export">
              <input type="checkbox" ${helperToggleAttribute}="markdownExportEnabled">
              <span></span>
            </label>
          </div>
          <div class="codex-helper-row">
            <div>
              <div class="codex-helper-label">Move sessions</div>
              <div class="codex-helper-detail" data-codex-helper-setting-status="sessionMoveEnabled">Loading</div>
            </div>
            <label class="codex-helper-switch" aria-label="Move sessions">
              <input type="checkbox" ${helperToggleAttribute}="sessionMoveEnabled">
              <span></span>
            </label>
          </div>
        </div>

        <h2>Deleted Sessions</h2>
        <div class="codex-helper-panel" data-codex-helper-backups>
          <div class="codex-helper-row">
            <div>
              <div class="codex-helper-label">Deleted session backups</div>
              <div class="codex-helper-detail">Loading</div>
            </div>
          </div>
        </div>

        <h2>Zed</h2>
        <div class="codex-helper-panel">
          <div class="codex-helper-row">
            <div>
              <div class="codex-helper-label">Remote open target</div>
              <div class="codex-helper-detail" data-codex-helper-zed-status>Loading</div>
            </div>
            <button type="button" ${helperCommandAttribute}="refresh">Refresh</button>
          </div>
        </div>

        <h2>User Scripts</h2>
        <div class="codex-helper-panel">
          <div class="codex-helper-row">
            <div>
              <div class="codex-helper-label">Loaded scripts</div>
              <div class="codex-helper-detail" data-codex-helper-scripts>Loading</div>
            </div>
            <button type="button" ${helperCommandAttribute}="refresh">Refresh</button>
          </div>
        </div>

        <h2>Diagnostics</h2>
        <div class="codex-helper-panel">
          <div class="codex-helper-row">
            <div>
              <div class="codex-helper-label">Log file</div>
              <div class="codex-helper-detail" data-codex-helper-log-path>Loading</div>
            </div>
            <button type="button" ${helperCommandAttribute}="open-log">Open</button>
          </div>
          <pre data-codex-helper-log>Loading</pre>
        </div>
    `;
    host.appendChild(page);
    return page;
  }

  function clearHelperSettingsPage() {
    helperPageRoot?.remove();
    if (helperContentHost instanceof HTMLElement) {
      helperContentHost.removeAttribute("data-codex-helper-active");
      helperContentHost.removeAttribute(helperContentHostAttribute);
    }
    helperContentHost = null;
    helperPageRoot = null;
    setEntryActive(false);
  }

  async function refreshHelperPage() {
    if (!helperPageRoot?.isConnected) return;
    const [backend, scripts, settings, backups, zed, log] = await Promise.all([
      bridge("/backend/status"),
      bridge("/runtime/user-scripts"),
      bridge("/settings/get"),
      bridge("/backups/list"),
      bridge("/zed-remote/status"),
      bridge("/diagnostics/read-latest"),
    ]);
    setHelperText("[data-codex-helper-backend]", resultText(backend));
    applySettings(settings);
    const scriptList = Array.isArray(scripts?.scripts) ? scripts.scripts : [];
    setHelperText(
      "[data-codex-helper-scripts]",
      scripts?.status === "ok"
        ? scriptList.length
          ? scriptList.join(", ")
          : "No user scripts found"
        : resultText(scripts),
    );
    renderDeletedSessionBackups(backups);
    setHelperText(
      "[data-codex-helper-zed-status]",
      zed?.status === "ok"
        ? zed.zedAppFound || zed.zedCliFound
          ? `Ready${zed.zedAppPath ? `: ${zed.zedAppPath}` : ""}${zed.zedCliPath ? ` (${zed.zedCliPath})` : ""}`
          : "Zed is not installed or not available on PATH"
        : resultText(zed),
    );
    setHelperText("[data-codex-helper-log-path]", log?.path || "Log path unavailable");
    setHelperText("[data-codex-helper-log]", log?.contents || "No diagnostic records yet");
  }

  function renderDeletedSessionBackups(result) {
    const panel = helperPageRoot?.querySelector("[data-codex-helper-backups]");
    if (!(panel instanceof HTMLElement)) return;
    panel.textContent = "";
    if (result?.status !== "ok") {
      panel.appendChild(
        createSettingsRow("Deleted session backups", resultText(result), null),
      );
      return;
    }
    const backups = Array.isArray(result.backups) ? result.backups : [];
    if (backups.length === 0) {
      panel.appendChild(
        createSettingsRow("No deleted sessions", "Deleted session backups will appear here.", null),
      );
      return;
    }
    for (const backup of backups.slice(0, 20)) {
      const button = document.createElement("button");
      button.type = "button";
      button.textContent = "Restore";
      button.setAttribute("data-codex-helper-restore-token", backup.token || "");
      panel.appendChild(createSettingsRow(backup.title || backup.session_id || "Untitled session", backupDetail(backup), button));
    }
  }

  function createSettingsRow(label, detail, control) {
    const row = document.createElement("div");
    row.className = "codex-helper-row";
    const body = document.createElement("div");
    const labelNode = document.createElement("div");
    labelNode.className = "codex-helper-label";
    labelNode.textContent = label;
    const detailNode = document.createElement("div");
    detailNode.className = "codex-helper-detail";
    detailNode.textContent = detail;
    body.append(labelNode, detailNode);
    row.appendChild(body);
    if (control instanceof HTMLElement) row.appendChild(control);
    return row;
  }

  function backupDetail(backup) {
    const parts = [];
    if (backup.deleted_at) parts.push(formatDateTime(backup.deleted_at));
    if (backup.cwd) parts.push(backup.cwd);
    if (backup.session_id) parts.push(backup.session_id);
    return parts.filter(Boolean).join(" · ");
  }

  function formatDateTime(value) {
    const date = new Date(value);
    if (Number.isNaN(date.getTime())) return value;
    return date.toLocaleString(undefined, {
      year: "numeric",
      month: "short",
      day: "numeric",
      hour: "numeric",
      minute: "2-digit",
    });
  }

  function applySettings(result) {
    if (result?.status !== "ok") {
      const message = resultText(result);
      for (const node of helperPageRoot?.querySelectorAll(`[data-codex-helper-setting-status]`) ||
        []) {
        node.textContent = message;
      }
      return;
    }
    const settings = result.settings || {};
    featureSettings = {
      ...featureSettings,
      ...settings,
    };
    for (const input of helperPageRoot?.querySelectorAll(`[${helperToggleAttribute}]`) || []) {
      if (!(input instanceof HTMLInputElement)) continue;
      const key = input.getAttribute(helperToggleAttribute) || "";
      input.checked = settings[key] === true;
      const status = helperPageRoot.querySelector(`[data-codex-helper-setting-status="${key}"]`);
      if (status) status.textContent = input.checked ? "Enabled" : "Disabled";
    }
  }

  function resultText(result) {
    if (!result) return "No response";
    if (result.status === "ok") return result.message || "Connected";
    return result.message || "Request failed";
  }

  function setHelperText(selector, value) {
    const node = helperPageRoot?.querySelector(selector);
    if (node) node.textContent = value;
  }

  function showHelperSettingsPage() {
    const root = findSettingsContentRoot();
    if (!root) {
      logDiagnostic("settings_page_root_not_found", {});
      return;
    }
    setEntryActive(true);
    helperContentHost = root;
    helperPageRoot = renderHelperPage(root);
    refreshHelperPage().catch((error) => {
      setHelperText("[data-codex-helper-backend]", error?.message || String(error));
      logDiagnostic("settings_refresh_failed", { error: error?.message || String(error) });
    });
  }

  async function handleHelperCommand(command) {
    const route = {
      "open-devtools": "/devtools/open",
      "open-state-dir": "/state/reveal",
      "open-log": "/diagnostics/reveal-log",
    }[command];
    if (command === "refresh") {
      await refreshHelperPage();
      return;
    }
    if (!route) return;
    const result = await bridge(route);
    if (result?.status !== "ok") {
      setHelperText("[data-codex-helper-backend]", result?.message || "Command failed");
      logDiagnostic("settings_command_failed", { command, result });
    }
  }

  async function handleHelperToggle(input) {
    const key = input.getAttribute(helperToggleAttribute) || "";
    if (!key) return;
    input.disabled = true;
    const result = await bridge("/settings/set", { [key]: input.checked });
    input.disabled = false;
    if (result?.status !== "ok") {
      input.checked = !input.checked;
      setHelperText("[data-codex-helper-backend]", result?.message || "Settings update failed");
      logDiagnostic("settings_update_failed", { key, result });
      return;
    }
    applySettings(result);
  }

  async function handleRestoreBackup(button) {
    const undoToken = button.getAttribute("data-codex-helper-restore-token") || "";
    if (!undoToken) return;
    button.setAttribute("disabled", "true");
    const result = await bridge("/backups/restore", { undo_token: undoToken });
    button.removeAttribute("disabled");
    if (result?.status !== "undone") {
      setHelperText("[data-codex-helper-backend]", result?.message || "Restore failed");
      logDiagnostic("backup_restore_failed", { result });
      return;
    }
    showHelperToast(result.message || "Session restored");
    await refreshHelperPage();
  }

  async function refreshFeatureSettings() {
    const result = await bridge("/settings/get");
    if (result?.status === "ok" && result.settings) {
      featureSettings = {
        ...featureSettings,
        ...result.settings,
      };
    }
    return featureSettings;
  }

  function enabledSessionActions() {
    const actions = [];
    if (featureSettings.sessionDeleteEnabled) actions.push("delete");
    if (featureSettings.markdownExportEnabled) actions.push("export");
    if (featureSettings.sessionMoveEnabled) actions.push("move");
    return actions;
  }

  function sessionRowFromTarget(target) {
    return target?.closest?.("[data-app-action-sidebar-thread-id]") || null;
  }

  function sessionRefFromRow(row) {
    const href = row.getAttribute("href") || row.querySelector("a")?.getAttribute("href") || "";
    const idMatch =
      href.match(/(?:session|conversation|thread)[=/:-]([A-Za-z0-9_.-]+)/i) ||
      href.match(/([A-Za-z0-9_-]{8,})$/);
    const sessionId =
      row.getAttribute("data-app-action-sidebar-thread-id") ||
      row.getAttribute("data-session-id") ||
      (idMatch && idMatch[1]) ||
      "";
    const titleNode = row.querySelector("[data-thread-title], .truncate.select-none, .truncate.text-base");
    const rawTitle = titleNode?.textContent || row.textContent || "Untitled session";
    const title = rawTitle
      .replace(/\s*(Delete|Export|Move)(\s*(Delete|Export|Move))*$/g, "")
      .trim()
      .slice(0, 160);
    return { session_id: sessionId, title: title || "Untitled session" };
  }

  function trackSessionContextMenu(row, event) {
    const ref = sessionRefFromRow(row);
    if (!ref.session_id) return;
    pendingSessionMenuContext = {
      row,
      ref,
      x: event.clientX,
      y: event.clientY,
      openedAt: Date.now(),
    };
    refreshFeatureSettings()
      .then(() => installSessionContextMenuItems())
      .catch((error) => {
        logDiagnostic("session_menu_settings_failed", { error: error?.message || String(error) });
      });
    setTimeout(() => {
      if (pendingSessionMenuContext?.ref?.session_id === ref.session_id) {
        pendingSessionMenuContext = null;
      }
    }, 2500);
  }

  function sessionMenuMatchesPendingContext(menu) {
    const context = pendingSessionMenuContext;
    if (!context) return false;
    if (Date.now() - context.openedAt > 2500) return false;
    if (!(context.row instanceof HTMLElement) || !context.row.isConnected) return false;
    const rect = menu.getBoundingClientRect();
    if (rect.width <= 0 || rect.height <= 0) return false;
    const margin = 90;
    const containsClick =
      context.x >= rect.left - margin &&
      context.x <= rect.right + margin &&
      context.y >= rect.top - margin &&
      context.y <= rect.bottom + margin;
    const nearOrigin = Math.abs(rect.left - context.x) < 180 && Math.abs(rect.top - context.y) < 180;
    return containsClick || nearOrigin;
  }

  function removeHelperSessionMenuItems(menu) {
    menu.querySelectorAll(`[${helperContextMenuAttribute}]`).forEach((node) => node.remove());
  }

  function createSessionMenuSeparator(menu) {
    const nativeSeparator = menu.querySelector('[role="separator"]');
    const separator = nativeSeparator instanceof HTMLElement ? nativeSeparator.cloneNode(true) : document.createElement("div");
    if (!(separator instanceof HTMLElement)) return document.createElement("div");
    separator.setAttribute(helperContextMenuAttribute, "separator");
    separator.setAttribute("role", "separator");
    if (!separator.className) separator.className = "codex-helper-menu-separator";
    return separator;
  }

  function menuItemTemplate(menu) {
    return Array.from(menu.querySelectorAll('[role="menuitem"]')).find((item) => {
      if (!(item instanceof HTMLElement)) return false;
      if (item.hasAttribute(helperContextMenuAttribute)) return false;
      const rect = item.getBoundingClientRect();
      return rect.width > 0 && rect.height > 0 && textOf(item);
    });
  }

  function setSessionMenuItemLabel(item, label) {
    const textNodes = [];
    const walker = document.createTreeWalker(item, NodeFilter.SHOW_TEXT);
    while (walker.nextNode()) {
      if ((walker.currentNode.nodeValue || "").trim()) textNodes.push(walker.currentNode);
    }
    if (textNodes.length > 0) {
      textNodes[0].nodeValue = label;
      for (let index = 1; index < textNodes.length; index += 1) textNodes[index].nodeValue = "";
      return;
    }
    const labelNode = document.createElement("span");
    labelNode.textContent = label;
    item.appendChild(labelNode);
  }

  function createSessionActionMenuItem(menu, action, label) {
    const template = menuItemTemplate(menu);
    const item = template instanceof HTMLElement ? template.cloneNode(true) : document.createElement("div");
    if (!(item instanceof HTMLElement)) return null;
    const context = pendingSessionMenuContext;
    if (!context?.ref) return null;
    item.setAttribute(helperContextMenuAttribute, "item");
    item.setAttribute("role", "menuitem");
    item.setAttribute("tabindex", "-1");
    item.setAttribute("data-codex-helper-session-action", action);
    item.removeAttribute("id");
    item.removeAttribute("aria-disabled");
    item.removeAttribute("data-highlighted");
    item.removeAttribute("data-state");
    replaceSessionMenuItemIcon(item, action);
    setSessionMenuItemLabel(item, label);
    item.addEventListener(
      "click",
      (event) => {
        event.preventDefault();
        event.stopPropagation();
        closeOpenMenus();
        pendingSessionMenuContext = null;
        handleSessionAction(action, context.row, context.ref).catch((error) => {
          showHelperToast(error?.message || String(error));
          logDiagnostic("session_action_failed", { action, error: error?.message || String(error) });
        });
      },
      true,
    );
    item.addEventListener(
      "keydown",
      (event) => {
        if (!["Enter", " "].includes(event.key)) return;
        item.click();
      },
      true,
    );
    return item;
  }

  function replaceSessionMenuItemIcon(item, action) {
    const icon = item.querySelector("svg, img");
    if (!(icon instanceof Element)) return;
    const svg = document.createElementNS("http://www.w3.org/2000/svg", "svg");
    svg.setAttribute("viewBox", "0 0 24 24");
    svg.setAttribute("fill", "none");
    svg.setAttribute("stroke", "currentColor");
    svg.setAttribute("stroke-width", "2");
    svg.setAttribute("stroke-linecap", "round");
    svg.setAttribute("stroke-linejoin", "round");
    svg.setAttribute("aria-hidden", "true");
    if (icon.getAttribute("class")) svg.setAttribute("class", icon.getAttribute("class"));
    const paths = {
      delete: ["M3 6h18", "M8 6V4h8v2", "M10 11v6", "M14 11v6", "M6 6l1 15h10l1-15"],
      export: ["M12 3v12", "M7 10l5 5 5-5", "M5 21h14"],
      move: ["M5 9l-3 3 3 3", "M19 9l3 3-3 3", "M2 12h20"],
    }[action] || [];
    for (const d of paths) {
      const path = document.createElementNS("http://www.w3.org/2000/svg", "path");
      path.setAttribute("d", d);
      svg.appendChild(path);
    }
    icon.replaceWith(svg);
  }

  function installSessionContextMenu(menu) {
    const actions = enabledSessionActions();
    if (!sessionMenuMatchesPendingContext(menu)) return false;
    const context = pendingSessionMenuContext;
    const contextId = `${context.ref.session_id}:${context.openedAt}`;
    if (
      menu.getAttribute("data-codex-helper-session-menu-context") === contextId &&
      menu.querySelector(`[${helperContextMenuAttribute}="item"]`)
    ) {
      return true;
    }
    removeHelperSessionMenuItems(menu);
    if (actions.length === 0) return false;
    menu.setAttribute("data-codex-helper-session-menu-context", contextId);
    const labels = {
      delete: "Delete",
      export: "Export Markdown",
      move: "Move",
    };
    menu.appendChild(createSessionMenuSeparator(menu));
    for (const action of actions) {
      const item = createSessionActionMenuItem(menu, action, labels[action]);
      if (item) menu.appendChild(item);
    }
    return true;
  }

  function installSessionContextMenuItems() {
    if (!pendingSessionMenuContext) return;
    const menus = Array.from(document.querySelectorAll("[role='menu']"));
    for (const menu of menus) {
      if (menu instanceof HTMLElement && installSessionContextMenu(menu)) return;
    }
  }

  function rowHref(row) {
    const href = row.getAttribute("href") || row.querySelector("a[href]")?.getAttribute("href") || "";
    if (!href) return "";
    try {
      return new URL(href, window.location.href).href;
    } catch (_) {
      return href;
    }
  }

  function isCurrentSessionRow(row, ref) {
    const activeNode = row.closest('[aria-current="page"], [aria-selected="true"], [data-state="active"]');
    if (activeNode instanceof HTMLElement) return true;
    const href = rowHref(row);
    if (href && href === window.location.href) return true;
    return !!ref.session_id && window.location.href.includes(ref.session_id);
  }

  function visibleSessionRows(root = document) {
    return Array.from(root.querySelectorAll("[data-app-action-sidebar-thread-id]")).filter((candidate) => {
      if (!(candidate instanceof HTMLElement)) return false;
      const rect = candidate.getBoundingClientRect();
      return rect.width > 0 && rect.height > 0;
    });
  }

  function nearestSessionRowContainer(row) {
    let node = row.parentElement;
    while (node && node !== document.body) {
      if (visibleSessionRows(node).length > 1) return node;
      node = node.parentElement;
    }
    return null;
  }

  function findReplacementSessionRow(deletedRow) {
    const inSameContainer = visibleSessionRows(nearestSessionRowContainer(deletedRow) || document).filter(
      (row) => row !== deletedRow,
    );
    const allRows = visibleSessionRows(document).filter((row) => row !== deletedRow);
    const candidates = inSameContainer.length ? inSameContainer : allRows;
    if (candidates.length === 0) return null;
    const allBeforeDelete = visibleSessionRows(document);
    const deletedIndex = allBeforeDelete.indexOf(deletedRow);
    if (deletedIndex >= 0) {
      const after = candidates.find((row) => allBeforeDelete.indexOf(row) > deletedIndex);
      if (after) return after;
      const before = [...candidates].reverse().find((row) => allBeforeDelete.indexOf(row) < deletedIndex);
      if (before) return before;
    }
    return candidates[0] || null;
  }

  function clickElement(element) {
    if (!(element instanceof HTMLElement)) return false;
    element.dispatchEvent(new MouseEvent("pointerdown", { bubbles: true, cancelable: true }));
    element.dispatchEvent(new MouseEvent("mousedown", { bubbles: true, cancelable: true }));
    element.click();
    return true;
  }

  function navigateToSessionRow(row) {
    if (!(row instanceof HTMLElement)) return false;
    const link = row.matches("a[href]") ? row : row.querySelector("a[href]");
    if (clickElement(link instanceof HTMLElement ? link : row)) return true;
    const href = rowHref(row);
    if (!href) return false;
    window.location.assign(href);
    return true;
  }

  function openNewChat() {
    const control = Array.from(document.querySelectorAll("a, button, [role='button']")).find((node) => {
      if (!(node instanceof HTMLElement)) return false;
      const rect = node.getBoundingClientRect();
      return rect.width > 0 && rect.height > 0 && exactText(node, "New chat");
    });
    return clickElement(control instanceof HTMLElement ? control : null);
  }

  function navigateAfterDeletedCurrentSession(replacementRow) {
    window.setTimeout(() => {
      if (navigateToSessionRow(replacementRow)) return;
      if (openNewChat()) return;
      logDiagnostic("delete_navigation_target_not_found", {});
    }, 0);
  }

  async function handleSessionAction(action, row, ref) {
    if (action === "delete") {
      if (!window.confirm(`Delete "${ref.title || ref.session_id}"?`)) return;
      const deletingCurrentSession = isCurrentSessionRow(row, ref);
      const replacementRow = deletingCurrentSession ? findReplacementSessionRow(row) : null;
      const result = await bridge("/delete", ref);
      if (result?.status === "server_deleted" || result?.status === "local_deleted") {
        row.remove();
        if (deletingCurrentSession) navigateAfterDeletedCurrentSession(replacementRow);
        showHelperToast(result.message || "Deleted", result.undo_token);
        return;
      }
      throw new Error(result?.message || "Delete failed");
    }
    if (action === "export") {
      const result = await bridge("/export-markdown", ref);
      if (result?.status !== "exported" || typeof result.markdown !== "string" || !result.filename) {
        throw new Error(result?.message || "Export failed");
      }
      downloadMarkdown(result.filename, result.markdown);
      showHelperToast(result.message || "Exported");
      return;
    }
    if (action === "move") {
      const target = window.prompt("Move session to project path");
      if (!target) return;
      const result = await bridge("/move-thread-workspace", { ...ref, target_cwd: target });
      if (result?.status !== "moved") throw new Error(result?.message || "Move failed");
      showHelperToast(result.message || "Moved");
      return;
    }
  }

  function downloadMarkdown(filename, markdown) {
    const blob = new Blob([markdown], { type: "text/markdown;charset=utf-8" });
    const url = URL.createObjectURL(blob);
    const link = document.createElement("a");
    link.href = url;
    link.download = filename;
    document.body.appendChild(link);
    link.click();
    link.remove();
    setTimeout(() => URL.revokeObjectURL(url), 1000);
  }

  function showHelperToast(message, undoToken) {
    document.querySelectorAll(`[${helperToastAttribute}]`).forEach((node) => node.remove());
    const toast = document.createElement("div");
    toast.setAttribute(helperToastAttribute, "true");
    toast.textContent = message;
    if (undoToken) {
      const undo = document.createElement("button");
      undo.type = "button";
      undo.textContent = "Undo";
      undo.addEventListener("click", async () => {
        const result = await bridge("/undo", { undo_token: undoToken });
        toast.textContent = result?.message || "Undo complete";
      });
      toast.appendChild(undo);
    }
    document.body.appendChild(toast);
    setTimeout(() => toast.remove(), 8000);
  }

  function selectedAttributeValue(attribute) {
    const nodes = Array.from(document.querySelectorAll(`[${attribute}]`));
    const visible = nodes
      .filter((node) => node instanceof HTMLElement)
      .map((node) => ({
        node,
        rect: node.getBoundingClientRect(),
        value: node.getAttribute(attribute) || "",
        text: textOf(node),
      }))
      .filter((item) => item.rect.width > 0 && item.rect.height > 0);
    const active = visible.find((item) => {
      const aria = item.node.getAttribute("aria-selected");
      const state = item.node.getAttribute("data-state");
      const className = String(item.node.className || "");
      return aria === "true" || state === "active" || /selected|active/.test(className);
    });
    return (active || visible[0] || {}).value || "";
  }

  function remoteContextFromDom() {
    const hostId = selectedAttributeValue("data-app-action-sidebar-thread-host-id");
    const projectPath = selectedAttributeValue("data-app-action-sidebar-project-list-id");
    return {
      hostId: hostId && hostId !== "local" ? hostId : "",
      path: projectPath.startsWith("/") ? projectPath : "",
    };
  }

  async function zedOpenRequestFromContext() {
    const context = remoteContextFromDom();
    if (context.hostId && context.path) {
      const resolved = await bridge("/zed-remote/resolve-host", { hostId: context.hostId });
      if (resolved?.status !== "ok") return resolved;
      return {
        status: "ok",
        request: {
          hostId: context.hostId,
          ssh: resolved.ssh,
          path: context.path,
        },
      };
    }
    return bridge("/zed-remote/fallback-request", {});
  }

  async function openCurrentRemoteInZed(menuItem) {
    menuItem.setAttribute("aria-disabled", "true");
    const originalText = menuItem.textContent;
    menuItem.dataset.codexHelperOriginalText = originalText || "Zed";
    if (originalText === "Zed") replaceTextNodes(menuItem, "Zed", "Opening Zed");
    const requestResult = await zedOpenRequestFromContext();
    if (requestResult?.status !== "ok") {
      throw new Error(requestResult?.message || "Cannot build Zed remote open request");
    }
    const openResult = await bridge("/zed-remote/open", requestResult.request);
    if (openResult?.status !== "ok") {
      throw new Error(openResult?.message || "Cannot open Zed remote target");
    }
    logDiagnostic("zed_remote_opened", {
      url: openResult.url,
      hostId: requestResult.request?.hostId,
      path: requestResult.request?.path,
    });
    replaceTextNodes(menuItem, "Opening Zed", "Zed");
    menuItem.removeAttribute("aria-disabled");
    closeOpenMenus();
  }

  function closeOpenMenus() {
    document.dispatchEvent(
      new KeyboardEvent("keydown", {
        key: "Escape",
        code: "Escape",
        keyCode: 27,
        which: 27,
        bubbles: true,
        cancelable: true,
      }),
    );
    window.dispatchEvent(
      new KeyboardEvent("keydown", {
        key: "Escape",
        code: "Escape",
        keyCode: 27,
        which: 27,
        bubbles: true,
        cancelable: true,
      }),
    );
  }

  function installZedMenuItem(menu) {
    if (!(menu instanceof HTMLElement)) return false;
    if (menu.querySelector(`[${helperZedAttribute}]`)) return true;
    if (!textOf(menu).includes("Cursor") || textOf(menu).includes("Zed")) return false;
    const cursorItem = Array.from(menu.querySelectorAll("[role='menuitem'], div, button")).find(
      (node) => node instanceof HTMLElement && exactText(node, "Cursor"),
    );
    if (!(cursorItem instanceof HTMLElement)) return false;

    const item = cursorItem.cloneNode(true);
    if (!(item instanceof HTMLElement)) return false;
    item.setAttribute(helperZedAttribute, "true");
    item.setAttribute("role", "menuitem");
    item.removeAttribute("data-highlighted");
    replaceTextNodes(item, "Cursor", "Zed");
    const image = item.querySelector("img");
    if (image instanceof HTMLImageElement) {
      image.src = "apps/zed.png";
      image.alt = "Zed";
    }
    item.addEventListener(
      "click",
      (event) => {
        event.preventDefault();
        event.stopPropagation();
        openCurrentRemoteInZed(item).catch((error) => {
          item.removeAttribute("aria-disabled");
          logDiagnostic("zed_remote_open_failed", { error: error?.message || String(error) });
          item.textContent = `Zed failed: ${error?.message || String(error)}`;
          console.error("[Codex Helper] Zed remote open failed", error);
        });
      },
      true,
    );
    cursorItem.insertAdjacentElement("afterend", item);
    logDiagnostic("zed_menu_item_injected", {});
    return true;
  }

  function installZedMenuItems() {
    const menus = Array.from(document.querySelectorAll("[role='menu']"));
    for (const menu of menus) installZedMenuItem(menu);
  }

  function installObserver() {
    if (observerInstalled) return;
    observerInstalled = true;
    const observer = new MutationObserver(() => {
      installSettingsEntry();
      installZedMenuItems();
      installSessionContextMenuItems();
      if (helperPageRoot && !helperPageRoot.isConnected) {
        clearHelperSettingsPage();
      }
    });
    observer.observe(document.documentElement, {
      childList: true,
      subtree: true,
      attributes: true,
      attributeFilter: ["role", "aria-selected", "data-state", "class"],
    });
  }

  document.addEventListener(
    "click",
    (event) => {
      const target = event.target;
      if (!(target instanceof Element)) return;
      const helperEntry = target.closest(`[${helperEntryAttribute}]`);
      if (helperEntry instanceof HTMLElement) {
        event.preventDefault();
        event.stopPropagation();
        showHelperSettingsPage();
        return;
      }
      const restoreButton = target.closest("[data-codex-helper-restore-token]");
      if (restoreButton instanceof HTMLElement) {
        event.preventDefault();
        event.stopPropagation();
        handleRestoreBackup(restoreButton).catch((error) => {
          restoreButton.removeAttribute("disabled");
          setHelperText("[data-codex-helper-backend]", error?.message || String(error));
          logDiagnostic("backup_restore_failed", { error: error?.message || String(error) });
        });
        return;
      }
      if (target.closest("aside, nav, [role='navigation'], [role='tablist']")) {
        const item = target.closest("button, a, [role='button'], [role='tab'], div");
        if (item instanceof HTMLElement && settingsLabels.some((label) => exactText(item, label))) {
          clearHelperSettingsPage();
        }
      }
      const command = target.closest(`[${helperCommandAttribute}]`);
      if (!(command instanceof HTMLElement)) return;
      event.preventDefault();
      event.stopPropagation();
      handleHelperCommand(command.getAttribute(helperCommandAttribute) || "").catch((error) => {
        setHelperText("[data-codex-helper-backend]", error?.message || String(error));
        logDiagnostic("settings_command_failed", {
          command: command.getAttribute(helperCommandAttribute),
          error: error?.message || String(error),
        });
      });
    },
    true,
  );

  document.addEventListener(
    "contextmenu",
    (event) => {
      const target = event.target;
      if (!(target instanceof Element)) return;
      const row = sessionRowFromTarget(target);
      if (!(row instanceof HTMLElement)) return;
      trackSessionContextMenu(row, event);
    },
    true,
  );

  document.addEventListener(
    "change",
    (event) => {
      const target = event.target;
      if (!(target instanceof HTMLInputElement)) return;
      if (!target.hasAttribute(helperToggleAttribute)) return;
      event.preventDefault();
      event.stopPropagation();
      handleHelperToggle(target).catch((error) => {
        target.checked = !target.checked;
        target.disabled = false;
        setHelperText("[data-codex-helper-backend]", error?.message || String(error));
        logDiagnostic("settings_update_failed", {
          key: target.getAttribute(helperToggleAttribute),
          error: error?.message || String(error),
        });
      });
    },
    true,
  );

  installSettingsEntry();
  installZedMenuItems();
  refreshFeatureSettings().catch((error) => {
    logDiagnostic("settings_feature_refresh_failed", { error: error?.message || String(error) });
  });
  installObserver();
})();
