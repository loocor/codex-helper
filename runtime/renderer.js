(() => {
  if (typeof document === "undefined") return;

  const helperEntryAttribute = "data-codex-helper-settings-entry";
  const helperContentHostAttribute = "data-codex-helper-content-host";
  const helperPageAttribute = "data-codex-helper-settings-page";
  const helperDialogPageAttribute = "data-codex-helper-settings-dialog-page";
  const helperCommandAttribute = "data-codex-helper-command";
  const helperToggleAttribute = "data-codex-helper-setting-toggle";
  const helperContextMenuAttribute = "data-codex-helper-session-menu";
  const helperToastAttribute = "data-codex-helper-toast";
  const helperAccountSettingsEntryAttribute =
    "data-codex-helper-account-settings-entry";
  const helperSettingsDialogAttribute = "data-codex-helper-settings-dialog";
  const helperPortsEntryAttribute = "data-codex-helper-ports-entry";
  const helperPortsPanelAttribute = "data-codex-helper-ports-panel";
  const helperPortCommandAttribute = "data-codex-helper-port-command";
  const helperSettingsPanelId = "codex-helper-settings-panel";
  const helperActionClass =
    "codex-helper-action border-token-border user-select-none no-drag cursor-interaction flex shrink-0 items-center gap-1 border whitespace-nowrap rounded-lg px-2 py-1 text-sm text-token-foreground bg-token-foreground/5 enabled:hover:bg-token-foreground/10";
  const helperPanelClass =
    "codex-helper-panel flex flex-col divide-y-[0.5px] divide-token-border overflow-hidden rounded-lg border border-token-border";
  let observerInstalled = false;
  let helperPageRoot = null;
  let helperDialogRoot = null;
  let helperContentStash = null;
  let pendingSessionMenuContext = null;
  let pendingPortScan = 0;
  const detectedPorts = new Map();
  let featureSettings = {
    sessionDeleteEnabled: false,
    markdownExportEnabled: false,
    sessionMoveEnabled: false,
    portForwardingEnabled: false,
    portAutoForwardWeb: true,
    portSameLocalPort: true,
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

  function isVisibleElement(node) {
    const rect = node.getBoundingClientRect();
    return rect.width > 0 && rect.height > 0;
  }

  function restoreNativeSettingsPanels() {
    for (const node of document.querySelectorAll(
      "[data-codex-helper-native-hidden='true']",
    )) {
      if (!(node instanceof HTMLElement)) continue;
      node.hidden = false;
      node.removeAttribute("aria-hidden");
      node.removeAttribute("data-codex-helper-native-hidden");
    }
    const helperPanel = document.getElementById(helperSettingsPanelId);
    if (helperPanel instanceof HTMLElement) {
      helperPanel.remove();
    }
  }

  function installHelperStyles() {
    let style = document.getElementById("codex-helper-runtime-style");
    if (!(style instanceof HTMLStyleElement)) {
      style = document.createElement("style");
      style.id = "codex-helper-runtime-style";
      document.head.appendChild(style);
    }
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
      [${helperContentHostAttribute}][data-codex-helper-active="true"] {
        min-height: 0 !important;
        overflow: auto !important;
      }
      [${helperPageAttribute}] {
        display: flex;
        flex-direction: column;
        border-top: 0.5px solid var(--color-token-border, rgba(26, 28, 31, 0.12));
        padding-top: var(--padding-panel, 20px);
        color: inherit;
      }
      [${helperPageAttribute}] .codex-helper-panel {
        background-color: var(--color-background-panel, var(--color-token-bg-fog));
      }
      [${helperPageAttribute}] [data-codex-helper-backups] {
        display: contents;
      }
      [${helperPageAttribute}] .codex-helper-action {
        border-color: transparent;
      }
      [${helperPageAttribute}] .codex-helper-switch {
        position: relative;
      }
      [${helperPageAttribute}] .codex-helper-switch input {
        position: absolute;
        width: 1px;
        height: 1px;
        opacity: 0;
        pointer-events: none;
      }
      [${helperPageAttribute}] .codex-helper-switch input:focus-visible + span {
        outline: 2px solid var(--color-token-focus-border, rgb(48, 145, 255));
        outline-offset: 2px;
      }
      [${helperPageAttribute}] .codex-helper-switch input:checked + span {
        background-color: var(--color-token-charts-blue, rgb(48, 145, 255));
      }
      [${helperPageAttribute}] .codex-helper-switch input:checked + span > span {
        transform: translateX(14px);
      }
      [${helperPageAttribute}] pre[data-codex-helper-log] {
        max-height: 260px;
        overflow: auto;
        margin: 0;
        padding: 12px;
        white-space: pre-wrap;
        word-break: break-word;
        font-size: 12px;
        line-height: 1.45;
      }
      [${helperDialogPageAttribute}] {
        display: flex;
        flex-direction: column;
        color: inherit;
      }
      [${helperDialogPageAttribute}] .codex-helper-panel {
        background-color: var(--color-background-panel, var(--color-token-bg-fog));
      }
      [${helperDialogPageAttribute}] [data-codex-helper-backups] {
        display: contents;
      }
      [${helperPageAttribute}] .codex-helper-settings-section-title,
      [${helperDialogPageAttribute}] .codex-helper-settings-section-title {
        padding: 0 2px;
      }
      [${helperDialogPageAttribute}] .codex-helper-action {
        border-color: transparent;
      }
      [${helperDialogPageAttribute}] .codex-helper-switch {
        position: relative;
      }
      [${helperDialogPageAttribute}] .codex-helper-switch input {
        position: absolute;
        width: 1px;
        height: 1px;
        opacity: 0;
        pointer-events: none;
      }
      [${helperDialogPageAttribute}] .codex-helper-switch input:focus-visible + span {
        outline: 2px solid var(--color-token-focus-border, rgb(48, 145, 255));
        outline-offset: 2px;
      }
      [${helperDialogPageAttribute}] .codex-helper-switch input:checked + span {
        background-color: var(--color-token-charts-blue, rgb(48, 145, 255));
      }
      [${helperDialogPageAttribute}] .codex-helper-switch input:checked + span > span {
        transform: translateX(14px);
      }
      [${helperDialogPageAttribute}] pre[data-codex-helper-log] {
        max-height: 260px;
        overflow: auto;
        margin: 0;
        padding: 12px;
        white-space: pre-wrap;
        word-break: break-word;
        font-size: 12px;
        line-height: 1.45;
      }
      [${helperSettingsDialogAttribute}] {
        position: fixed;
        inset: 0;
        z-index: 2147483646;
        display: grid;
        place-items: center;
        padding: 24px;
        background: rgba(0, 0, 0, 0.28);
        color: CanvasText;
      }
      [${helperSettingsDialogAttribute}] .codex-helper-settings-dialog-panel {
        width: min(760px, calc(100vw - 48px));
        max-height: min(820px, calc(100vh - 48px));
        display: flex;
        flex-direction: column;
        overflow: hidden;
        border-radius: 12px;
        border: 1px solid color-mix(in srgb, currentColor 12%, transparent);
        background: Canvas;
        box-shadow: 0 24px 80px color-mix(in srgb, black 28%, transparent);
      }
      [${helperSettingsDialogAttribute}] .codex-helper-settings-dialog-header {
        display: flex;
        align-items: center;
        justify-content: space-between;
        gap: 16px;
        padding: 16px 18px;
        border-bottom: 1px solid color-mix(in srgb, currentColor 10%, transparent);
      }
      [${helperSettingsDialogAttribute}] .codex-helper-settings-dialog-title {
        font-size: 16px;
        font-weight: 600;
      }
      [${helperSettingsDialogAttribute}] .codex-helper-settings-dialog-close {
        display: inline-flex;
        align-items: center;
        justify-content: center;
        width: 28px;
        height: 28px;
        border: 0;
        border-radius: 8px;
        padding: 0;
        background: transparent;
        color: inherit;
        cursor: pointer;
      }
      [${helperSettingsDialogAttribute}] .codex-helper-settings-dialog-close:hover {
        background: color-mix(in srgb, currentColor 8%, transparent);
      }
      [${helperSettingsDialogAttribute}] .codex-helper-settings-dialog-close:focus-visible {
        outline: 2px solid var(--color-token-focus-border, rgb(48, 145, 255));
        outline-offset: 2px;
      }
      [${helperSettingsDialogAttribute}] .codex-helper-settings-dialog-close svg {
        width: 16px;
        height: 16px;
      }
      [${helperSettingsDialogAttribute}] .codex-helper-settings-dialog-body {
        overflow: auto;
        padding: 18px;
      }
      [${helperSettingsDialogAttribute}] [${helperDialogPageAttribute}] {
        border-top: 0;
        padding-top: 0;
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
      [${helperPortsPanelAttribute}] {
        margin: 10px;
        padding: 10px;
        border: 1px solid color-mix(in srgb, currentColor 12%, transparent);
        border-radius: 8px;
        background: color-mix(in srgb, Canvas 96%, currentColor 4%);
        color: CanvasText;
        font-size: 13px;
      }
      [${helperPortsPanelAttribute}] .codex-helper-port-toolbar {
        display: flex;
        justify-content: space-between;
        gap: 10px;
        align-items: center;
        margin-bottom: 8px;
      }
      [${helperPortsPanelAttribute}] .codex-helper-port-row {
        display: grid;
        grid-template-columns: 86px minmax(0, 1fr) auto;
        gap: 10px;
        align-items: center;
        min-height: 34px;
        border-top: 1px solid color-mix(in srgb, currentColor 9%, transparent);
        padding: 7px 0;
      }
      [${helperPortsPanelAttribute}] .codex-helper-port-row:first-of-type {
        border-top: 0;
      }
      [${helperPortsPanelAttribute}] .codex-helper-port-actions {
        display: inline-flex;
        gap: 6px;
      }
      [${helperPortsPanelAttribute}] button {
        border: 0;
        border-radius: 7px;
        padding: 5px 8px;
        background: color-mix(in srgb, currentColor 10%, transparent);
        color: inherit;
        font: inherit;
        cursor: pointer;
      }
    `;
  }

  function setEntryActive(active) {
    void active;
  }

  function renderHelperPage(host, options = {}) {
    const replaceHost = options.replaceHost === true;
    const hidePageHeader = options.hidePageHeader === true;
    const insertAfter =
      options.insertAfter instanceof HTMLElement ? options.insertAfter : null;
    const pageAttribute = options.pageAttribute || helperPageAttribute;
    host.querySelectorAll(`[${pageAttribute}]`).forEach((node) => {
      node.remove();
    });
    if (replaceHost && host.id !== helperSettingsPanelId) {
      stashHostContent(host);
    } else if (replaceHost) {
      restoreStashedContent();
    }
    if (replaceHost) {
      host.setAttribute(helperContentHostAttribute, "true");
      host.setAttribute("data-codex-helper-active", "true");
    }
    const page = document.createElement("section");
    page.setAttribute(pageAttribute, "true");
    page.className = "flex flex-col";
    const panelStyle =
      'style="background-color: var(--color-background-panel, var(--color-token-bg-fog));"';
    const switchRow = (title, description, descKey, toggleKey, ariaLabel) => `
            <div class="flex items-center justify-between gap-4 p-3">
              <div class="flex min-w-0 flex-col gap-1">
                <div class="min-w-0 text-sm text-token-text-primary">${title}</div>
                <div class="text-token-text-secondary min-w-0 text-sm" data-codex-helper-setting-desc="${descKey}">${description}</div>
              </div>
              <label class="codex-helper-switch inline-flex shrink-0 items-center" aria-label="${ariaLabel}">
                <input type="checkbox" ${helperToggleAttribute}="${toggleKey}">
                <span class="relative inline-flex h-5 w-8 shrink-0 items-center rounded-full bg-token-foreground/20 transition-colors duration-200 ease-out"><span class="h-4 w-4 translate-x-[2px] rounded-full border border-[color:var(--gray-0)] bg-[color:var(--gray-0)] shadow-sm transition-transform duration-200 ease-out"></span></span>
              </label>
            </div>`;
    const actionRow = (title, detail, command, buttonLabel, detailAttr = "") => `
            <div class="flex items-center justify-between gap-4 p-3">
              <div class="flex min-w-0 flex-col gap-1">
                <div class="min-w-0 text-sm text-token-text-primary">${title}</div>
                <div class="text-token-text-secondary min-w-0 text-sm"${detailAttr ? ` ${detailAttr}` : ""}>${detail}</div>
              </div>
              <button type="button" class="${helperActionClass}" ${helperCommandAttribute}="${command}">${buttonLabel}</button>
            </div>`;
    const settingsPanel = (rows) =>
      `<div class="${helperPanelClass}" ${panelStyle}>${rows}</div>`;
    page.innerHTML = `
        ${hidePageHeader
        ? ""
        : `<div class="flex h-toolbar items-center justify-between gap-2 px-0 py-0">
          <div class="flex min-w-0 flex-1 flex-col gap-1">
            <div class="text-base font-medium text-token-text-primary">Codex Helper</div>
          </div>
        </div>`
      }
        <div class="flex flex-col gap-4">
          <section class="codex-helper-settings-section flex flex-col gap-1.5">
            <div class="codex-helper-settings-section-title text-sm font-medium text-token-text-primary">Basic</div>
            ${settingsPanel(`
            ${actionRow("Backend", "Loading", "refresh", "Refresh", "data-codex-helper-backend")}
            ${actionRow("DevTools", "Open Chrome DevTools for this Codex window.", "open-devtools", "Open")}
            ${actionRow("Helper directory", "~/.codex-helper stores settings, logs, and scripts.", "open-state-dir", "Open")}
            `)}
          </section>
          <section class="codex-helper-settings-section flex flex-col gap-1.5">
            <div class="codex-helper-settings-section-title text-sm font-medium text-token-text-primary">Sessions</div>
            ${settingsPanel(`
            ${switchRow("Delete sessions", "Show delete controls in the session list context menu.", "sessionDeleteEnabled", "sessionDeleteEnabled", "Delete sessions")}
            ${switchRow("Markdown export", "Export conversations as Markdown from the session menu.", "markdownExportEnabled", "markdownExportEnabled", "Markdown export")}
            ${switchRow("Move sessions", "Allow reordering sessions in the sidebar.", "sessionMoveEnabled", "sessionMoveEnabled", "Move sessions")}
            `)}
          </section>
          <section class="codex-helper-settings-section flex flex-col gap-1.5">
            <div class="codex-helper-settings-section-title text-sm font-medium text-token-text-primary">Port forwarding</div>
            ${settingsPanel(`
            ${switchRow("Enable port forwarding", "Detect and forward ports from agent sessions.", "portForwardingEnabled", "portForwardingEnabled", "Enable port forwarding")}
            ${switchRow("Auto-forward detected web ports", "Open forwarded web URLs when a common dev port is detected.", "portAutoForwardWeb", "portAutoForwardWeb", "Auto-forward detected web ports")}
            ${switchRow("Use the same local port by default", "Bind forwarded ports to the same local port number when possible.", "portSameLocalPort", "portSameLocalPort", "Use the same local port by default")}
            `)}
          </section>
          <section class="codex-helper-settings-section flex flex-col gap-1.5">
            <div class="codex-helper-settings-section-title text-sm font-medium text-token-text-primary">Other</div>
            ${settingsPanel(`
            ${actionRow("Open in Zed", "Loading", "refresh", "Refresh", "data-codex-helper-zed-status")}
            ${actionRow("Loaded scripts", "Loading", "refresh", "Refresh", "data-codex-helper-scripts")}
            <div data-codex-helper-backups></div>
            ${actionRow("Log file", "Loading", "open-log", "Open", "data-codex-helper-log-path")}
            <pre class="text-token-text-secondary min-w-0 px-3 pb-3 text-xs" data-codex-helper-log>Loading</pre>
            `)}
          </section>
        </div>
    `;
    if (insertAfter && insertAfter.parentElement === host) {
      insertAfter.insertAdjacentElement("afterend", page);
    } else {
      host.appendChild(page);
    }
    return page;
  }

  function stashHostContent(host) {
    if (!(host instanceof HTMLElement)) return;
    if (helperContentStash?.host === host) return;
    restoreStashedContent();

    const marker = document.createComment("codex-helper-content-stash");
    const fragment = document.createDocumentFragment();
    host.insertBefore(marker, host.firstChild);
    for (const node of Array.from(host.childNodes)) {
      if (node === marker) continue;
      if (node instanceof HTMLElement && node.hasAttribute(helperPageAttribute))
        continue;
      fragment.appendChild(node);
    }
    helperContentStash = { host, marker, fragment };
  }

  function restoreStashedContent() {
    if (!helperContentStash) return;
    const { host, marker, fragment } = helperContentStash;
    if (
      host instanceof HTMLElement &&
      marker instanceof Comment &&
      marker.parentNode === host
    ) {
      host.insertBefore(fragment, marker.nextSibling);
      marker.remove();
    }
    helperContentStash = null;
  }

  function clearHelperSettingsPage() {
    for (const node of document.querySelectorAll(`[${helperPageAttribute}]`)) {
      if (node.closest(`[${helperSettingsDialogAttribute}]`)) continue;
      node.remove();
    }
    restoreNativeSettingsPanels();
    restoreStashedContent();
    for (const node of document.querySelectorAll(
      `[${helperContentHostAttribute}]`,
    )) {
      node.removeAttribute("data-codex-helper-active");
      node.removeAttribute(helperContentHostAttribute);
    }
    helperContentHost = null;
    helperPageRoot = null;
    setEntryActive(false);
  }

  function helperSettingsRoots() {
    return [helperPageRoot, helperDialogRoot].filter(
      (root) => root instanceof HTMLElement && root.isConnected,
    );
  }

  async function refreshHelperPage() {
    if (helperSettingsRoots().length === 0) return;
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
    setHelperText(
      "[data-codex-helper-log-path]",
      log?.path || "Log path unavailable",
    );
    setHelperText(
      "[data-codex-helper-log]",
      log?.contents || "No diagnostic records yet",
    );
  }

  function renderDeletedSessionBackups(result) {
    const panels = helperSettingsRoots()
      .map((root) => root.querySelector("[data-codex-helper-backups]"))
      .filter((panel) => panel instanceof HTMLElement);
    if (panels.length === 0) return;
    for (const panel of panels) {
      renderDeletedSessionBackupsPanel(panel, result);
    }
  }

  function renderDeletedSessionBackupsPanel(panel, result) {
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
        createSettingsRow(
          "No deleted sessions",
          "Deleted session backups will appear here.",
          null,
        ),
      );
      return;
    }
    for (const backup of backups.slice(0, 20)) {
      const button = document.createElement("button");
      button.type = "button";
      button.textContent = "Restore";
      button.setAttribute(
        "data-codex-helper-restore-token",
        backup.token || "",
      );
      panel.appendChild(
        createSettingsRow(
          backup.title || backup.session_id || "Untitled session",
          backupDetail(backup),
          button,
        ),
      );
    }
  }

  function createSettingsRow(label, detail, control) {
    const row = document.createElement("div");
    row.className = "flex items-center justify-between gap-4 p-3";
    const body = document.createElement("div");
    body.className = "flex min-w-0 flex-col gap-1";
    const labelNode = document.createElement("div");
    labelNode.className = "min-w-0 text-sm text-token-text-primary";
    labelNode.textContent = label;
    const detailNode = document.createElement("div");
    detailNode.className = "text-token-text-secondary min-w-0 text-sm";
    detailNode.textContent = detail;
    body.append(labelNode, detailNode);
    row.appendChild(body);
    if (control instanceof HTMLElement) {
      if (control.tagName === "BUTTON") control.className = helperActionClass;
      row.appendChild(control);
    }
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
      for (const root of helperSettingsRoots()) {
        for (const node of root.querySelectorAll(
          `[data-codex-helper-setting-desc]`,
        )) {
          node.textContent = message;
        }
      }
      return;
    }
    const settings = result.settings || {};
    featureSettings = {
      ...featureSettings,
      ...settings,
    };
    for (const root of helperSettingsRoots()) {
      for (const input of root.querySelectorAll(`[${helperToggleAttribute}]`)) {
        if (!(input instanceof HTMLInputElement)) continue;
        const key = input.getAttribute(helperToggleAttribute) || "";
        input.checked = settings[key] === true;
      }
    }
  }

  function resultText(result) {
    if (!result) return "No response";
    if (result.status === "ok") return result.message || "Connected";
    return result.message || "Request failed";
  }

  function setHelperText(selector, value) {
    for (const root of helperSettingsRoots()) {
      const node = root.querySelector(selector);
      if (node) node.textContent = value;
    }
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
      setHelperText(
        "[data-codex-helper-backend]",
        result?.message || "Command failed",
      );
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
      setHelperText(
        "[data-codex-helper-backend]",
        result?.message || "Settings update failed",
      );
      logDiagnostic("settings_update_failed", { key, result });
      return;
    }
    applySettings(result);
  }

  async function handleRestoreBackup(button) {
    const undoToken =
      button.getAttribute("data-codex-helper-restore-token") || "";
    if (!undoToken) return;
    button.setAttribute("disabled", "true");
    const result = await bridge("/backups/restore", { undo_token: undoToken });
    button.removeAttribute("disabled");
    if (result?.status !== "undone") {
      setHelperText(
        "[data-codex-helper-backend]",
        result?.message || "Restore failed",
      );
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
    const href =
      row.getAttribute("href") ||
      row.querySelector("a")?.getAttribute("href") ||
      "";
    const idMatch =
      href.match(/(?:session|conversation|thread)[=/:-]([A-Za-z0-9_.-]+)/i) ||
      href.match(/([A-Za-z0-9_-]{8,})$/);
    const sessionId =
      row.getAttribute("data-app-action-sidebar-thread-id") ||
      row.getAttribute("data-session-id") ||
      idMatch?.[1] ||
      "";
    const titleNode = row.querySelector(
      "[data-thread-title], .truncate.select-none, .truncate.text-base",
    );
    const rawTitle =
      titleNode?.textContent || row.textContent || "Untitled session";
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
        logDiagnostic("session_menu_settings_failed", {
          error: error?.message || String(error),
        });
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
    if (!(context.row instanceof HTMLElement) || !context.row.isConnected)
      return false;
    const rect = menu.getBoundingClientRect();
    if (rect.width <= 0 || rect.height <= 0) return false;
    const margin = 90;
    const containsClick =
      context.x >= rect.left - margin &&
      context.x <= rect.right + margin &&
      context.y >= rect.top - margin &&
      context.y <= rect.bottom + margin;
    const nearOrigin =
      Math.abs(rect.left - context.x) < 180 &&
      Math.abs(rect.top - context.y) < 180;
    return containsClick || nearOrigin;
  }

  function removeHelperSessionMenuItems(menu) {
    menu.querySelectorAll(`[${helperContextMenuAttribute}]`).forEach((node) => {
      node.remove();
    });
  }

  function createSessionMenuSeparator(menu) {
    const nativeSeparator = menu.querySelector('[role="separator"]');
    const separator =
      nativeSeparator instanceof HTMLElement
        ? nativeSeparator.cloneNode(true)
        : document.createElement("div");
    if (!(separator instanceof HTMLElement))
      return document.createElement("div");
    separator.setAttribute(helperContextMenuAttribute, "separator");
    separator.setAttribute("role", "separator");
    if (!separator.className)
      separator.className = "codex-helper-menu-separator";
    return separator;
  }

  function menuItemTemplate(menu) {
    return Array.from(menu.querySelectorAll('[role="menuitem"]')).find(
      (item) => {
        if (!(item instanceof HTMLElement)) return false;
        if (item.hasAttribute(helperContextMenuAttribute)) return false;
        const rect = item.getBoundingClientRect();
        return rect.width > 0 && rect.height > 0 && textOf(item);
      },
    );
  }

  function setSessionMenuItemLabel(item, label) {
    const textNodes = [];
    const walker = document.createTreeWalker(item, NodeFilter.SHOW_TEXT);
    while (walker.nextNode()) {
      if ((walker.currentNode.nodeValue || "").trim())
        textNodes.push(walker.currentNode);
    }
    if (textNodes.length > 0) {
      textNodes[0].nodeValue = label;
      for (let index = 1; index < textNodes.length; index += 1)
        textNodes[index].nodeValue = "";
      return;
    }
    const labelNode = document.createElement("span");
    labelNode.textContent = label;
    item.appendChild(labelNode);
  }

  function createSessionActionMenuItem(menu, action, label) {
    const template = menuItemTemplate(menu);
    const item =
      template instanceof HTMLElement
        ? template.cloneNode(true)
        : document.createElement("div");
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
          logDiagnostic("session_action_failed", {
            action,
            error: error?.message || String(error),
          });
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
    if (icon.getAttribute("class"))
      svg.setAttribute("class", icon.getAttribute("class"));
    const paths =
      {
        delete: [
          "M3 6h18",
          "M8 6V4h8v2",
          "M10 11v6",
          "M14 11v6",
          "M6 6l1 15h10l1-15",
        ],
        export: ["M12 3v12", "M7 10l5 5 5-5", "M5 21h14"],
        move: ["M5 9l-3 3 3 3", "M19 9l3 3-3 3", "M2 12h20"],
      }[action] || [];
    for (const d of paths) {
      const path = document.createElementNS(
        "http://www.w3.org/2000/svg",
        "path",
      );
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
      menu.getAttribute("data-codex-helper-session-menu-context") ===
      contextId &&
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
      if (menu instanceof HTMLElement && installSessionContextMenu(menu))
        return;
    }
  }

  function rowHref(row) {
    const href =
      row.getAttribute("href") ||
      row.querySelector("a[href]")?.getAttribute("href") ||
      "";
    if (!href) return "";
    try {
      return new URL(href, window.location.href).href;
    } catch (_) {
      return href;
    }
  }

  function isCurrentSessionRow(row, ref) {
    const activeNode = row.closest(
      '[aria-current="page"], [aria-selected="true"], [data-state="active"]',
    );
    if (activeNode instanceof HTMLElement) return true;
    const href = rowHref(row);
    if (href && href === window.location.href) return true;
    return !!ref.session_id && window.location.href.includes(ref.session_id);
  }

  function visibleSessionRows(root = document) {
    return Array.from(
      root.querySelectorAll("[data-app-action-sidebar-thread-id]"),
    ).filter((candidate) => {
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
    const inSameContainer = visibleSessionRows(
      nearestSessionRowContainer(deletedRow) || document,
    ).filter((row) => row !== deletedRow);
    const allRows = visibleSessionRows(document).filter(
      (row) => row !== deletedRow,
    );
    const candidates = inSameContainer.length ? inSameContainer : allRows;
    if (candidates.length === 0) return null;
    const allBeforeDelete = visibleSessionRows(document);
    const deletedIndex = allBeforeDelete.indexOf(deletedRow);
    if (deletedIndex >= 0) {
      const after = candidates.find(
        (row) => allBeforeDelete.indexOf(row) > deletedIndex,
      );
      if (after) return after;
      const before = [...candidates]
        .reverse()
        .find((row) => allBeforeDelete.indexOf(row) < deletedIndex);
      if (before) return before;
    }
    return candidates[0] || null;
  }

  function clickElement(element) {
    if (!(element instanceof HTMLElement)) return false;
    element.dispatchEvent(
      new MouseEvent("pointerdown", { bubbles: true, cancelable: true }),
    );
    element.dispatchEvent(
      new MouseEvent("mousedown", { bubbles: true, cancelable: true }),
    );
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
    const control = Array.from(
      document.querySelectorAll("a, button, [role='button']"),
    ).find((node) => {
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
      const replacementRow = deletingCurrentSession
        ? findReplacementSessionRow(row)
        : null;
      const result = await bridge("/delete", ref);
      if (
        result?.status === "server_deleted" ||
        result?.status === "local_deleted"
      ) {
        row.remove();
        if (deletingCurrentSession)
          navigateAfterDeletedCurrentSession(replacementRow);
        showHelperToast(result.message || "Deleted", result.undo_token);
        return;
      }
      throw new Error(result?.message || "Delete failed");
    }
    if (action === "export") {
      const result = await bridge("/export-markdown", ref);
      if (
        result?.status !== "exported" ||
        typeof result.markdown !== "string" ||
        !result.filename
      ) {
        throw new Error(result?.message || "Export failed");
      }
      downloadMarkdown(result.filename, result.markdown);
      showHelperToast(result.message || "Exported");
      return;
    }
    if (action === "move") {
      const target = window.prompt("Move session to project path");
      if (!target) return;
      const result = await bridge("/move-thread-workspace", {
        ...ref,
        target_cwd: target,
      });
      if (result?.status !== "moved")
        throw new Error(result?.message || "Move failed");
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
    document.querySelectorAll(`[${helperToastAttribute}]`).forEach((node) => {
      node.remove();
    });
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
      return (
        aria === "true" ||
        state === "active" ||
        /selected|active/.test(className)
      );
    });
    return active?.value || visible[0]?.value || "";
  }

  function remoteContextFromDom() {
    const hostId = selectedAttributeValue(
      "data-app-action-sidebar-thread-host-id",
    );
    const projectPath = selectedAttributeValue(
      "data-app-action-sidebar-project-list-id",
    );
    return {
      hostId: hostId && hostId !== "local" ? hostId : "",
      path: projectPath.startsWith("/") ? projectPath : "",
    };
  }

  function parseWebPortsFromText(text) {
    const ports = new Map();
    const pattern =
      /\bhttps?:\/\/(?:localhost|127\.0\.0\.1|0\.0\.0\.0|\[::1\]):([0-9]{1,5})(?:[/?#][^\s"'<>]*)?/gi;
    for (const match of text.matchAll(pattern)) {
      const port = Number(match[1]);
      if (Number.isInteger(port) && port >= 1 && port <= 65535) {
        ports.set(port, { port, url: match[0] });
      }
    }
    return Array.from(ports.values());
  }

  function portKey(context, remotePort, localPort) {
    const localPortKey =
      Number.isInteger(localPort) && localPort > 0 ? localPort : "custom";
    return [
      context.hostId || "unknown",
      context.path || "",
      remotePort,
      localPortKey,
    ].join(":");
  }

  function schedulePortScan() {
    if (pendingPortScan) return;
    pendingPortScan = window.setTimeout(() => {
      pendingPortScan = 0;
      scanTerminalWebPorts();
    }, 500);
  }

  function findTerminalPortScanRoots() {
    const selector = [
      ".xterm",
      "[class*='xterm' i]",
      "[data-testid*='terminal' i]",
      "[data-test*='terminal' i]",
      "[aria-label*='terminal' i]",
      "[data-panel-id*='terminal' i]",
      "[data-codex-terminal]",
    ].join(",");
    const roots = [];
    for (const node of document.querySelectorAll(selector)) {
      if (!(node instanceof HTMLElement)) continue;
      if (!isVisibleElement(node)) continue;
      if (
        node.closest(
          `[${helperPageAttribute}], [${helperPortsPanelAttribute}], [${helperToastAttribute}], [${helperContextMenuAttribute}]`,
        )
      ) {
        continue;
      }
      if (roots.some((root) => root.contains(node))) continue;
      for (let index = roots.length - 1; index >= 0; index -= 1) {
        if (node.contains(roots[index])) roots.splice(index, 1);
      }
      roots.push(node);
    }
    return roots;
  }

  function terminalTextForPortScan() {
    const roots = findTerminalPortScanRoots();
    const parts = [];
    for (const root of roots) {
      const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT);
      while (walker.nextNode()) {
        const parent = walker.currentNode.parentElement;
        if (
          parent?.closest(
            `[${helperPageAttribute}], [${helperPortsPanelAttribute}], [${helperToastAttribute}], [${helperContextMenuAttribute}]`,
          )
        ) {
          continue;
        }
        const text = (walker.currentNode.nodeValue || "").trim();
        if (text) parts.push(text);
      }
    }
    return parts.join("\n");
  }

  function localPortForDetectedPort(remotePort) {
    return featureSettings.portSameLocalPort ? remotePort : 0;
  }

  function shouldAutoForwardDetectedPort(entry, context) {
    return Boolean(
      featureSettings.portAutoForwardWeb && context.hostId && entry.localPort,
    );
  }

  function scanTerminalWebPorts() {
    if (!featureSettings.portForwardingEnabled) return;
    const context = remoteContextFromDom();
    const text = terminalTextForPortScan();
    for (const candidate of parseWebPortsFromText(text)) {
      const localPort = localPortForDetectedPort(candidate.port);
      const key = portKey(context, candidate.port, localPort);
      const existing = detectedPorts.get(key);
      if (existing?.status === "forwarding" || existing?.status === "active")
        continue;
      const entry = {
        key,
        hostId: context.hostId,
        remotePath: context.path,
        remotePort: candidate.port,
        localPort,
        url: candidate.url,
        status: "detected",
      };
      detectedPorts.set(key, entry);
      if (shouldAutoForwardDetectedPort(entry, context)) {
        forwardDetectedPort(entry).catch((error) => {
          entry.status = "failed";
          entry.message = error?.message || String(error);
          logDiagnostic("ports_auto_forward_failed", {
            error: entry.message,
            remotePort: entry.remotePort,
          });
        });
      }
    }
  }

  async function forwardDetectedPort(entry, source = "auto") {
    entry.status = "forwarding";
    const result = await bridge("/ports/forward", {
      hostId: entry.hostId,
      remotePath: entry.remotePath,
      remotePort: entry.remotePort,
      localPort: entry.localPort,
      source,
    });
    if (result?.status !== "ok") {
      entry.status = "failed";
      entry.message = result?.message || "Port forwarding failed";
      logDiagnostic("ports_auto_forward_failed", {
        result,
        remotePort: entry.remotePort,
      });
      return;
    }
    entry.status = "active";
    entry.id = result.id;
    entry.localUrl = result.localUrl;
    showHelperToast(
      `Forwarded remote port ${entry.remotePort} to localhost:${entry.localPort}`,
    );
    if (document.querySelector(`[${helperPortsPanelAttribute}]`))
      showPortsPanel();
  }

  function findBottomPanelPicker() {
    const controls = Array.from(
      document.querySelectorAll("button, [role='button'], [role='tab']"),
    );
    const terminal = controls.find(
      (node) =>
        node instanceof HTMLElement &&
        exactText(node, "Terminal") &&
        isVisibleElement(node),
    );
    return terminal?.parentElement || null;
  }

  function installPortsEntry() {
    if (document.querySelector(`[${helperPortsEntryAttribute}]`)) return true;
    const picker = findBottomPanelPicker();
    if (!(picker instanceof HTMLElement)) return false;
    const terminal = Array.from(
      picker.querySelectorAll("button, [role='button'], [role='tab']"),
    ).find(
      (node) => node instanceof HTMLElement && exactText(node, "Terminal"),
    );
    const entry =
      terminal instanceof HTMLElement
        ? terminal.cloneNode(true)
        : document.createElement("button");
    if (!(entry instanceof HTMLElement)) return false;
    entry.setAttribute(helperPortsEntryAttribute, "true");
    entry.removeAttribute("aria-selected");
    entry.removeAttribute("data-state");
    replaceTextNodes(entry, textOf(entry), "Ports");
    if (!textOf(entry).includes("Ports")) entry.textContent = "Ports";
    entry.addEventListener(
      "click",
      (event) => {
        event.preventDefault();
        event.stopPropagation();
        showPortsPanel();
      },
      true,
    );
    picker.appendChild(entry);
    return true;
  }

  function mergedPortRows(activePorts) {
    const rows = new Map();
    for (const port of activePorts) {
      rows.set(
        port.id ||
        `${port.hostId}:${port.remotePath}:${port.remotePort}:${port.localPort}`,
        port,
      );
    }
    for (const entry of detectedPorts.values()) {
      const id = entry.id || entry.key;
      if (!rows.has(id)) rows.set(id, entry);
    }
    return Array.from(rows.values()).sort(
      (a, b) => (a.remotePort || 0) - (b.remotePort || 0),
    );
  }

  async function showPortsPanel() {
    installHelperStyles();
    const result = await bridge("/ports/list");
    renderPortsPanel(
      result?.status === "ok" && Array.isArray(result.ports)
        ? result.ports
        : [],
    );
  }

  function renderPortsPanel(activePorts) {
    document
      .querySelectorAll(`[${helperPortsPanelAttribute}]`)
      .forEach((node) => {
        node.remove();
      });
    const picker = findBottomPanelPicker();
    const host = picker?.parentElement || document.body;
    const panel = document.createElement("section");
    panel.setAttribute(helperPortsPanelAttribute, "true");
    const rows = mergedPortRows(activePorts);
    panel.innerHTML = `
      <div class="codex-helper-port-toolbar">
        <strong>Ports</strong>
        <button type="button" ${helperPortCommandAttribute}="manual">Forward Port</button>
      </div>
    `;
    if (rows.length === 0) {
      const empty = document.createElement("div");
      empty.className = "codex-helper-port-row";
      empty.textContent = "No forwarded ports";
      panel.appendChild(empty);
    } else {
      for (const row of rows) panel.appendChild(createPortRow(row));
    }
    host.appendChild(panel);
  }

  function createPortRow(row) {
    const item = document.createElement("div");
    item.className = "codex-helper-port-row";
    const status = row.status || "detected";
    const localUrl =
      row.localUrl ||
      (row.localPort ? `http://127.0.0.1:${row.localPort}` : "");
    item.innerHTML = `
      <div>${status}</div>
      <div>${row.remotePort || ""}${localUrl ? ` -> ${localUrl}` : ""}</div>
      <div class="codex-helper-port-actions"></div>
    `;
    const actions = item.querySelector(".codex-helper-port-actions");
    if (actions instanceof HTMLElement) {
      if (localUrl && status === "active") {
        actions.appendChild(createPortAction("open", "Open", row.id, localUrl));
        actions.appendChild(
          createPortAction("copy", "Copy URL", row.id, localUrl),
        );
        actions.appendChild(createPortAction("stop", "Stop", row.id, localUrl));
      } else {
        actions.appendChild(
          createPortAction("forward", "Forward", row.key || row.id, ""),
        );
      }
    }
    return item;
  }

  function createPortAction(command, label, id, localUrl) {
    const button = document.createElement("button");
    button.type = "button";
    button.textContent = label;
    button.setAttribute(helperPortCommandAttribute, command);
    if (id) button.setAttribute("data-codex-helper-port-id", id);
    if (localUrl) button.setAttribute("data-codex-helper-port-url", localUrl);
    return button;
  }

  async function handlePortCommand(button) {
    const command = button.getAttribute(helperPortCommandAttribute) || "";
    const id = button.getAttribute("data-codex-helper-port-id") || "";
    const localUrl = button.getAttribute("data-codex-helper-port-url") || "";
    if (command === "open" && localUrl) {
      window.open(localUrl, "_blank", "noopener,noreferrer");
      return;
    }
    if (command === "copy" && localUrl) {
      await navigator.clipboard.writeText(localUrl);
      showHelperToast("Copied port URL");
      return;
    }
    if (command === "stop" && id) {
      const result = await bridge("/ports/stop", { id });
      if (result?.status !== "ok")
        throw new Error(result?.message || "Stop failed");
      showPortsPanel();
      return;
    }
    if (command === "forward") {
      const entry = detectedPorts.get(id);
      if (entry) {
        const localPort =
          entry.localPort ||
          Number(window.prompt("Local port", String(entry.remotePort)));
        if (!Number.isInteger(localPort) || localPort < 1 || localPort > 65535)
          return;
        const previousKey = entry.key;
        entry.localPort = localPort;
        entry.key = portKey(
          { hostId: entry.hostId, path: entry.remotePath },
          entry.remotePort,
          localPort,
        );
        if (previousKey !== entry.key) {
          detectedPorts.delete(previousKey);
          detectedPorts.set(entry.key, entry);
        }
        await forwardDetectedPort(entry, "manual");
      }
      return;
    }
    if (command === "manual") {
      const context = remoteContextFromDom();
      const remotePort = Number(window.prompt("Remote port"));
      if (!Number.isInteger(remotePort) || remotePort < 1 || remotePort > 65535)
        return;
      const localPort = Number(window.prompt("Local port", String(remotePort)));
      if (!Number.isInteger(localPort) || localPort < 1 || localPort > 65535)
        return;
      await forwardDetectedPort(
        {
          key: portKey(context, remotePort, localPort),
          hostId: context.hostId,
          remotePath: context.path,
          remotePort,
          localPort,
          status: "detected",
        },
        "manual",
      );
    }
  }

  function isAccountSettingsMenu(menu) {
    if (!(menu instanceof HTMLElement) || menu.getAttribute("role") !== "menu")
      return false;
    if (!isVisibleElement(menu)) return false;
    const text = textOf(menu);
    return (
      text.includes("Personal account") &&
      text.includes("Settings") &&
      text.includes("Usage remaining") &&
      text.includes("Log out")
    );
  }

  function findAccountSettingsItem(menu) {
    return Array.from(menu.querySelectorAll('[role="menuitem"]')).find(
      (item) =>
        item instanceof HTMLElement &&
        exactText(item, "Settings") &&
        isVisibleElement(item),
    );
  }

  function installAccountSettingsMenuEntry(menu) {
    if (!isAccountSettingsMenu(menu)) return false;
    if (menu.querySelector(`[${helperAccountSettingsEntryAttribute}]`))
      return true;
    const settingsItem = findAccountSettingsItem(menu);
    if (!(settingsItem instanceof HTMLElement)) return false;
    const item = settingsItem.cloneNode(true);
    if (!(item instanceof HTMLElement)) return false;
    item.setAttribute(helperAccountSettingsEntryAttribute, "true");
    item.setAttribute("role", "menuitem");
    item.setAttribute("tabindex", "-1");
    item.removeAttribute("id");
    item.removeAttribute("aria-disabled");
    item.removeAttribute("data-highlighted");
    item.removeAttribute("data-state");
    setSessionMenuItemLabel(item, "Helper Settings");
    item.addEventListener(
      "click",
      (event) => {
        event.preventDefault();
        event.stopPropagation();
        closeOpenMenus();
        showHelperSettingsDialog();
      },
      true,
    );
    settingsItem.insertAdjacentElement("afterend", item);
    return true;
  }

  function installAccountSettingsMenuItems() {
    for (const menu of document.querySelectorAll('[role="menu"]')) {
      if (menu instanceof HTMLElement && installAccountSettingsMenuEntry(menu))
        return true;
    }
    return false;
  }

  function closeHelperSettingsDialog() {
    document
      .querySelectorAll(`[${helperSettingsDialogAttribute}]`)
      .forEach((node) => {
        node.remove();
      });
    helperDialogRoot = null;
  }

  function showHelperSettingsDialog() {
    installHelperStyles();
    closeHelperSettingsDialog();
    const dialog = document.createElement("div");
    dialog.setAttribute(helperSettingsDialogAttribute, "true");
    dialog.setAttribute("role", "dialog");
    dialog.setAttribute("aria-modal", "true");
    dialog.setAttribute("aria-label", "Helper Settings");
    dialog.innerHTML = `
      <div class="codex-helper-settings-dialog-panel">
        <div class="codex-helper-settings-dialog-header">
          <div class="codex-helper-settings-dialog-title">Helper Settings</div>
          <button type="button" class="codex-helper-settings-dialog-close" data-codex-helper-dialog-close aria-label="Close">
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
              <path d="M18 6 6 18"></path>
              <path d="m6 6 12 12"></path>
            </svg>
          </button>
        </div>
        <div class="codex-helper-settings-dialog-body"></div>
      </div>
    `;
    dialog.addEventListener(
      "click",
      (event) => {
        if (
          event.target === dialog ||
          event.target?.closest?.("[data-codex-helper-dialog-close]")
        ) {
          event.preventDefault();
          event.stopPropagation();
          closeHelperSettingsDialog();
        }
      },
      true,
    );
    const body = dialog.querySelector(".codex-helper-settings-dialog-body");
    if (!(body instanceof HTMLElement)) return;
    document.body.appendChild(dialog);
    helperDialogRoot = renderHelperPage(body, {
      replaceHost: false,
      hidePageHeader: true,
      pageAttribute: helperDialogPageAttribute,
    });
    refreshHelperPage().catch((error) => {
      setHelperText(
        "[data-codex-helper-backend]",
        error?.message || String(error),
      );
      logDiagnostic("settings_refresh_failed", {
        error: error?.message || String(error),
      });
    });
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

  function installObserver() {
    if (observerInstalled) return;
    observerInstalled = true;
    const observer = new MutationObserver(() => {
      installPortsEntry();
      installAccountSettingsMenuItems();
      installSessionContextMenuItems();
      schedulePortScan();
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
      const accountSettingsEntry = target.closest(
        `[${helperAccountSettingsEntryAttribute}]`,
      );
      if (accountSettingsEntry instanceof HTMLElement) {
        event.preventDefault();
        event.stopPropagation();
        closeOpenMenus();
        showHelperSettingsDialog();
        return;
      }
      const portsEntry = target.closest(`[${helperPortsEntryAttribute}]`);
      if (portsEntry instanceof HTMLElement) {
        event.preventDefault();
        event.stopPropagation();
        showPortsPanel();
        return;
      }
      const portCommand = target.closest(`[${helperPortCommandAttribute}]`);
      if (portCommand instanceof HTMLElement) {
        event.preventDefault();
        event.stopPropagation();
        handlePortCommand(portCommand).catch((error) => {
          showHelperToast(error?.message || String(error));
          logDiagnostic("ports_command_failed", {
            error: error?.message || String(error),
          });
        });
        return;
      }
      const restoreButton = target.closest("[data-codex-helper-restore-token]");
      if (restoreButton instanceof HTMLElement) {
        event.preventDefault();
        event.stopPropagation();
        handleRestoreBackup(restoreButton).catch((error) => {
          restoreButton.removeAttribute("disabled");
          setHelperText(
            "[data-codex-helper-backend]",
            error?.message || String(error),
          );
          logDiagnostic("backup_restore_failed", {
            error: error?.message || String(error),
          });
        });
        return;
      }
      const command = target.closest(`[${helperCommandAttribute}]`);
      if (!(command instanceof HTMLElement)) return;
      event.preventDefault();
      event.stopPropagation();
      handleHelperCommand(
        command.getAttribute(helperCommandAttribute) || "",
      ).catch((error) => {
        setHelperText(
          "[data-codex-helper-backend]",
          error?.message || String(error),
        );
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
    "keydown",
    (event) => {
      if (event.key !== "Escape") return;
      if (!document.querySelector(`[${helperSettingsDialogAttribute}]`)) return;
      closeHelperSettingsDialog();
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
        setHelperText(
          "[data-codex-helper-backend]",
          error?.message || String(error),
        );
        logDiagnostic("settings_update_failed", {
          key: target.getAttribute(helperToggleAttribute),
          error: error?.message || String(error),
        });
      });
    },
    true,
  );

  installHelperStyles();
  installPortsEntry();
  installAccountSettingsMenuItems();
  refreshFeatureSettings().catch((error) => {
    logDiagnostic("settings_feature_refresh_failed", {
      error: error?.message || String(error),
    });
  });
  installObserver();
})();
