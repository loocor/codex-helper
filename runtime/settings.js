// Helper Settings page content and commands
// biome-ignore-all lint/correctness/noUnusedVariables: called from bootstrap.js, sessions.js, and native-settings.js in the bundled runtime
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
  const sectionLinkIcon = `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M18 13v6a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h6"></path><polyline points="15 3 21 3 21 9"></polyline><line x1="10" y1="14" x2="21" y2="3"></line></svg>`;
  const sectionHeading = (title, command, ariaLabel) => `
            <div class="codex-helper-settings-section-heading">
              <div class="codex-helper-settings-section-title text-sm font-medium text-token-text-primary">${title}</div>
              <button type="button" class="codex-helper-settings-section-link" ${helperCommandAttribute}="${command}" aria-label="${ariaLabel}">${sectionLinkIcon}</button>
            </div>`;
  const sectionToolbar = (statusAttr, command, buttonLabel = "Refresh") => `
            <div class="flex items-center justify-between gap-4 p-3">
              <div class="text-token-text-secondary min-w-0 flex-1 truncate text-sm"${statusAttr ? ` ${statusAttr}` : ""}>Loading</div>
              <button type="button" class="${helperActionClass}" ${helperCommandAttribute}="${command}">${buttonLabel}</button>
            </div>`;
  const actionRow = (title, detail, command, buttonLabel, detailAttr = "") => `
            <div class="flex items-center justify-between gap-4 p-3">
              <div class="flex min-w-0 flex-col gap-1">
                <div class="min-w-0 text-sm text-token-text-primary">${title}</div>
                <div class="text-token-text-secondary min-w-0 text-sm"${detailAttr ? ` ${detailAttr}` : ""}>${detail}</div>
              </div>
              <button type="button" class="${helperActionClass}" ${helperCommandAttribute}="${command}">${buttonLabel}</button>
            </div>`;
  const externalLinkRow = (title, description, url, linkLabel) => `
            <div class="flex items-center justify-between gap-4 p-3">
              <div class="flex min-w-0 flex-col gap-1">
                <div class="min-w-0 text-sm text-token-text-primary">${title}</div>
                <div class="text-token-text-secondary min-w-0 text-sm">${description}</div>
              </div>
              <a href="${url}" target="_blank" rel="noopener noreferrer" class="${helperActionClass} codex-helper-external-link">${linkLabel}</a>
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
            ${actionRow("Open in Zed", "Loading", "refresh", "Refresh", "data-codex-helper-zed-status")}
            ${actionRow("DevTools", "Open Chrome DevTools for this Codex window.", "open-devtools", "Open")}
            `)}
          </section>
          <section class="codex-helper-settings-section flex flex-col gap-1.5">
            <div class="codex-helper-settings-section-title text-sm font-medium text-token-text-primary">Sessions</div>
            ${settingsPanel(`
            ${switchRow("Delete sessions", "Show delete controls in the session list context menu.", "sessionDeleteEnabled", "sessionDeleteEnabled", "Delete sessions")}
            ${switchRow("Markdown export", "Export conversations as Markdown from the session menu.", "markdownExportEnabled", "markdownExportEnabled", "Markdown export")}
            ${switchRow("Move sessions", "Move sessions between projects from the sidebar context menu.", "sessionMoveEnabled", "sessionMoveEnabled", "Move sessions")}
            `)}
          </section>
          <section class="codex-helper-settings-section flex flex-col gap-1.5" ${helperSettingsSectionAttribute}="port-forwarding">
            <div class="codex-helper-settings-section-title text-sm font-medium text-token-text-primary">Port forwarding</div>
            ${settingsPanel(`
            ${switchRow("Enable port forwarding", "Detect and forward ports from agent sessions.", "portForwardingEnabled", "portForwardingEnabled", "Enable port forwarding")}
            ${switchRow("Auto-forward detected web ports", "Open forwarded web URLs when a common dev port is detected.", "portAutoForwardWeb", "portAutoForwardWeb", "Auto-forward detected web ports")}
            ${switchRow("Use the same local port by default", "Bind forwarded ports to the same local port number when possible.", "portSameLocalPort", "portSameLocalPort", "Use the same local port by default")}
            `)}
          </section>
          <section class="codex-helper-settings-section flex flex-col gap-1.5">
            ${sectionHeading("Loaded scripts", "open-scripts-dir", "Open scripts folder")}
            ${settingsPanel(`
            ${sectionToolbar("data-codex-helper-scripts-status", "refresh")}
            <div class="codex-helper-settings-scroll" data-codex-helper-scripts-list></div>
            `)}
          </section>
          <section class="codex-helper-settings-section flex flex-col gap-1.5">
            ${sectionHeading("Deleted chats", "open-backups-dir", "Open deleted chats folder")}
            ${settingsPanel(`
            ${sectionToolbar("data-codex-helper-backups-status", "refresh")}
            <div class="codex-helper-chat-search">
              <input class="codex-helper-chat-search-input" data-codex-helper-deleted-chat-search type="search" placeholder="Search deleted chats" autocomplete="off" spellcheck="false" aria-label="Search deleted chats">
            </div>
            <div class="codex-helper-settings-scroll" data-codex-helper-backups></div>
            `)}
          </section>
          <section class="codex-helper-settings-section flex flex-col gap-1.5">
            ${sectionHeading("Log files", "open-logs-dir", "Open logs folder")}
            ${settingsPanel(`
            ${sectionToolbar("data-codex-helper-log-path", "refresh")}
            <pre class="codex-helper-settings-scroll text-token-text-secondary min-w-0 text-xs" data-codex-helper-log>Loading</pre>
            `)}
          </section>
          <section class="codex-helper-settings-section flex flex-col gap-1.5">
            <div class="codex-helper-settings-section-title text-sm font-medium text-token-text-primary">About</div>
            ${settingsPanel(`
            ${externalLinkRow("Project repository", "Source code, issues, and releases on GitHub.", helperRepoUrl, "Open")}
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
  return [helperPageRoot, helperNativeSettingsRoot].filter(
    (root) => root instanceof HTMLElement && root.isConnected,
  );
}

function focusHelperSettingsSection(sectionId) {
  const selector = `[${helperSettingsSectionAttribute}="${sectionId}"]`;
  for (const root of helperSettingsRoots()) {
    const section = root.querySelector(selector);
    if (!(section instanceof HTMLElement)) continue;
    const scrollParent = root;
    if (scrollParent instanceof HTMLElement) {
      const parentTop = scrollParent.getBoundingClientRect().top;
      const sectionTop = section.getBoundingClientRect().top;
      scrollParent.scrollTo({
        top: Math.max(0, scrollParent.scrollTop + sectionTop - parentTop - 12),
        behavior: "smooth",
      });
    } else {
      section.scrollIntoView({ block: "nearest", behavior: "smooth" });
    }
    return true;
  }
  return false;
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
  renderLoadedScripts(scripts);
  await renderDeletedSessionBackupsAfterRefresh(backups);
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
    "[data-codex-helper-log-status]",
    log?.status === "ok" ? "Latest diagnostic log" : resultText(log),
  );
  setHelperText(
    "[data-codex-helper-log]",
    log?.contents || "No diagnostic records yet",
  );
}

async function renderDeletedSessionBackupsAfterRefresh(result) {
  const activeDeletedSearchInput = activeDeletedChatSearchInput();
  if (activeDeletedSearchInput) {
    await runDeletedChatSearch(activeDeletedSearchInput);
    return;
  }
  renderDeletedSessionBackups(result);
}

function activeDeletedChatSearchInput() {
  for (const root of helperSettingsRoots()) {
    const input = root.querySelector("[data-codex-helper-deleted-chat-search]");
    if (
      input instanceof HTMLInputElement &&
      String(input.value || "").trim()
    ) {
      return input;
    }
  }
  return null;
}

function renderLoadedScripts(result) {
  const scriptList = Array.isArray(result?.scripts) ? result.scripts : [];
  const statusText =
    result?.status === "ok"
      ? scriptList.length
        ? `${scriptList.length} script${scriptList.length === 1 ? "" : "s"} loaded`
        : "No user scripts found"
      : resultText(result);
  setHelperText("[data-codex-helper-scripts-status]", statusText);
  setHelperText(
    "[data-codex-helper-scripts-path]",
    result?.path || "Scripts path unavailable",
  );
  const lists = helperSettingsRoots()
    .map((root) => root.querySelector("[data-codex-helper-scripts-list]"))
    .filter((panel) => panel instanceof HTMLElement);
  for (const list of lists) {
    list.textContent = "";
    if (result?.status !== "ok") {
      list.appendChild(createScrollEmptyMessage(statusText));
      continue;
    }
    if (scriptList.length === 0) {
      list.appendChild(
        createScrollEmptyMessage(
          list.closest(`[${helperNativeSettingsPageAttribute}]`)
            ? "No user scripts found."
            : "No user scripts found in ~/.codex-helper/scripts.",
        ),
      );
      continue;
    }
    for (const script of scriptList) {
      list.appendChild(createCompactListRow(script, script));
    }
  }
}

function renderDeletedSessionBackups(result) {
  deletedChatBackupsResult = result;
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
    setHelperText("[data-codex-helper-backups-status]", resultText(result));
    panel.appendChild(createScrollEmptyMessage(resultText(result)));
    return;
  }
  const backups = Array.isArray(result.backups) ? result.backups : [];
  setHelperText(
    "[data-codex-helper-backups-path]",
    result?.backups_path || "Backups path unavailable",
  );
  setHelperText(
    "[data-codex-helper-backups-status]",
    backups.length
      ? `${backups.length} deleted chat${backups.length === 1 ? "" : "s"}`
      : "No deleted chats",
  );
  if (backups.length === 0) {
    panel.appendChild(
      createScrollEmptyMessage("Deleted chat backups will appear here."),
    );
    return;
  }
  for (const backup of backups.slice(0, DELETED_CHAT_BACKUP_RENDER_LIMIT)) {
    panel.appendChild(createCompactBackupRow(backup, restoreButtonForChat(backup)));
  }
}

function createScrollEmptyMessage(message) {
  const node = document.createElement("div");
  node.className = "codex-helper-settings-scroll-empty";
  node.textContent = message;
  return node;
}

function createCompactListRow(label, title) {
  const row = document.createElement("div");
  row.className = "codex-helper-settings-compact-row";
  const text = document.createElement("div");
  text.className = "codex-helper-settings-compact-text";
  text.textContent = label;
  if (title) text.title = title;
  row.appendChild(text);
  return row;
}

function createCompactBackupRow(backup, control) {
  const row = document.createElement("div");
  row.className = "codex-helper-settings-compact-row";
  const text = document.createElement("div");
  text.className = "codex-helper-settings-compact-text";
  const title = backup.title || backup.session_id || "Untitled session";
  const summary = backupSummaryLine(backup);
  const titleNode = document.createElement("div");
  titleNode.className = "codex-helper-settings-compact-title";
  titleNode.textContent = title;
  const meta = document.createElement("div");
  meta.className = "codex-helper-settings-compact-meta";
  meta.textContent = summary || backup.session_id || "";
  text.title = backupDetail(backup) || [title, summary].filter(Boolean).join(" · ");
  text.appendChild(titleNode);
  if (meta.textContent) text.appendChild(meta);
  row.appendChild(text);
  if (control instanceof HTMLElement) {
    if (control.tagName === "BUTTON") control.className = helperActionClass;
    row.appendChild(control);
  }
  return row;
}

function restoreButtonForChat(chat) {
  if (!chat?.token) return null;
  const button = document.createElement("button");
  button.type = "button";
  button.textContent = "Restore";
  button.setAttribute("data-codex-helper-restore-token", chat.token || "");
  return button;
}

function backupSummaryLine(backup) {
  const parts = [];
  if (backup.time || backup.deleted_at) {
    parts.push(formatDateTime(backup.time || backup.deleted_at));
  }
  if (backup.cwd) parts.push(displayProjectName(backup.cwd));
  return parts.filter(Boolean).join(" · ");
}


function backupDetail(backup) {
  const parts = [];
  if (backup.time) parts.push(formatDateTime(backup.time));
  if (backup.deleted_at) parts.push(`Deleted ${formatDateTime(backup.deleted_at)}`);
  if (backup.cwd) parts.push(backup.cwd);
  if (backup.session_id) parts.push(backup.session_id);
  return parts.filter(Boolean).join(" · ");
}

async function searchChats(scope, query) {
  // The bridge route also handles scope === "archived" (see
  // src-tauri/src/chat_search.rs), but no runtime caller emits it today —
  // Archived chats search is intentionally backend-only until the UI is
  // reintroduced. Keep the `scope` argument so re-enabling stays trivial.
  return bridge("/chats/search", { scope, query });
}

function scheduleDeletedChatSearch(input) {
  if (deletedChatSearchTimer) clearTimeout(deletedChatSearchTimer);
  deletedChatSearchTimer = window.setTimeout(() => {
    deletedChatSearchTimer = 0;
    runDeletedChatSearch(input).catch((error) => {
      setHelperText("[data-codex-helper-backups-status]", error?.message || String(error));
      logDiagnostic("deleted_chat_search_failed", {
        error: error?.message || String(error),
      });
    });
  }, 250);
}

async function runDeletedChatSearch(input) {
  const query = String(input?.value || "").trim();
  const requestId = ++deletedChatSearchRequestId;
  if (!query) {
    if (deletedChatBackupsResult) renderDeletedSessionBackups(deletedChatBackupsResult);
    return;
  }
  setHelperText("[data-codex-helper-backups-status]", "Searching deleted chats");
  const result = await searchChats("deleted", query);
  if (requestId !== deletedChatSearchRequestId) return;
  renderDeletedChatSearchResults(result);
}

function renderDeletedChatSearchResults(result) {
  const panels = helperSettingsRoots()
    .map((root) => root.querySelector("[data-codex-helper-backups]"))
    .filter((panel) => panel instanceof HTMLElement);
  for (const panel of panels) {
    panel.textContent = "";
    if (result?.status !== "ok") {
      const message = resultText(result);
      setHelperText("[data-codex-helper-backups-status]", message);
      panel.appendChild(createScrollEmptyMessage(message));
      continue;
    }
    const matches = Array.isArray(result.matches) ? result.matches : [];
    setHelperText(
      "[data-codex-helper-backups-status]",
      matches.length
        ? `${matches.length} deleted chat match${matches.length === 1 ? "" : "es"}`
        : "No deleted chat matches",
    );
    if (matches.length === 0) {
      panel.appendChild(createScrollEmptyMessage("No deleted chats match this search."));
      continue;
    }
    for (const match of matches) {
      panel.appendChild(createCompactBackupRow(match, restoreButtonForChat(match)));
    }
  }
}

function removeArchivedChatsSearchArtifacts() {
  // Defensive sweep for any leftover Archived chats search UI created by
  // earlier runtime builds. Returns the number of unique nodes removed so
  // callers (the MutationObserver in bootstrap.js) can decide whether to
  // keep running the sweep.
  const removed = new Set();
  for (const node of document.querySelectorAll(
    "[data-codex-helper-archived-chat-search], [data-codex-helper-archived-chat-results]",
  )) {
    if (!(node instanceof HTMLElement)) continue;
    const container = node.closest(".codex-helper-chat-search");
    const target = container instanceof HTMLElement ? container : node;
    if (removed.has(target)) continue;
    removed.add(target);
    target.remove();
  }
  return removed.size;
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
  const wasPortForwardingEnabled = featureSettings.portForwardingEnabled;
  const settings = result.settings || {};
  featureSettings = {
    ...featureSettings,
    ...settings,
  };
  featureSettingsLoaded = true;
  for (const root of helperSettingsRoots()) {
    for (const input of root.querySelectorAll(`[${helperToggleAttribute}]`)) {
      if (!(input instanceof HTMLInputElement)) continue;
      const key = input.getAttribute(helperToggleAttribute) || "";
      input.checked = settings[key] === true;
    }
  }
  maintainPortsPanel();
  if (featureSettings.portForwardingEnabled) schedulePortScan();
  else if (wasPortForwardingEnabled) handlePortForwardingDisabled();
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
    "open-scripts-dir": "/scripts/reveal",
    "open-backups-dir": "/backups/reveal",
    "open-log-file": "/diagnostics/reveal-log",
    "open-logs-dir": "/logs/reveal",
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
    featureSettingsLoaded = true;
  }
  maintainPortsPanel();
  if (featureSettings.portForwardingEnabled) schedulePortScan();
  return featureSettings;
}
