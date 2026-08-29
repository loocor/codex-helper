// Standalone Helper Settings window host
function helperSettingsShellCss() {
  return `
    html {
      color-scheme: light dark;
    }
    html, body {
      margin: 0;
      height: 100%;
      background: Canvas;
      color: CanvasText;
      font: 13px/1.45 -apple-system, BlinkMacSystemFont, "SF Pro Text", system-ui, sans-serif;
    }
    #helper-settings-app {
      height: 100%;
    }
    .helper-settings-shell {
      position: relative;
      display: flex;
      height: 100%;
      min-height: 0;
    }
    .helper-settings-titlebar {
      position: absolute;
      top: 0;
      left: 0;
      right: 0;
      height: 52px;
      z-index: 2;
    }
    .helper-settings-nav {
      box-sizing: border-box;
      position: relative;
      width: 196px;
      flex: 0 0 196px;
      display: flex;
      flex-direction: column;
      gap: 4px;
      padding: 0 10px 16px;
      border-right: 1px solid color-mix(in srgb, CanvasText 12%, transparent);
      background: color-mix(in srgb, CanvasText 4%, Canvas);
    }
    .helper-settings-nav-drag {
      flex: 0 0 52px;
      height: 52px;
      margin: 0 -10px;
    }
    .helper-settings-nav-title {
      padding: 4px 10px 10px;
      font-size: 11px;
      font-weight: 600;
      letter-spacing: 0.04em;
      text-transform: uppercase;
      color: color-mix(in srgb, CanvasText 55%, transparent);
    }
    .helper-settings-nav-item {
      display: flex;
      align-items: center;
      gap: 8px;
      width: 100%;
      border: 0;
      border-radius: 8px;
      padding: 8px 10px;
      background: transparent;
      color: inherit;
      font: inherit;
      text-align: left;
      cursor: pointer;
    }
    .helper-settings-nav-item:hover,
    .helper-settings-nav-item:focus-visible {
      background: color-mix(in srgb, CanvasText 8%, transparent);
      outline: none;
    }
    .helper-settings-nav-item[data-active="true"] {
      background: color-mix(in srgb, CanvasText 12%, transparent);
    }
    .helper-settings-nav-item svg {
      width: 16px;
      height: 16px;
      flex: 0 0 16px;
    }
    .helper-settings-content {
      flex: 1 1 auto;
      min-width: 0;
      min-height: 0;
      overflow: auto;
    }
    .helper-settings-content:has([data-codex-helper-provider-dialog]) {
      overflow: hidden;
      display: flex;
      flex-direction: column;
    }
    .helper-settings-content:has([data-codex-helper-provider-dialog]) > [data-codex-helper-provider-dialog] {
      flex: 1 1 auto;
      min-height: 0;
    }
    .helper-settings-page {
      box-sizing: border-box;
      min-height: 100%;
      padding: 52px 32px 32px;
    }
    [data-codex-helper-provider-dialog] {
      padding: 0;
    }
    .helper-settings-page-inner,
    .helper-settings-page-body,
    .codex-helper-settings-section {
      display: flex;
      flex-direction: column;
      min-width: 0;
    }
    .helper-settings-page-inner,
    .helper-settings-page-body {
      gap: 24px;
    }
    .codex-helper-settings-section {
      gap: 8px;
    }
    .helper-settings-page-header {
      display: flex;
      flex-direction: column;
      gap: 6px;
    }
    .helper-settings-back {
      display: inline-flex;
      align-items: center;
      gap: 2px;
      align-self: flex-start;
      margin: 0 0 4px -6px;
      border: 0;
      padding: 4px 8px 4px 2px;
      background: transparent;
      color: inherit;
      font: inherit;
      font-size: 13px;
      cursor: pointer;
      border-radius: 8px;
    }
    .helper-settings-back:hover,
    .helper-settings-back:focus-visible {
      background: color-mix(in srgb, CanvasText 8%, transparent);
      outline: none;
    }
    .helper-settings-back svg {
      width: 16px;
      height: 16px;
    }
    .helper-settings-page-title {
      margin: 0;
      font-size: 22px;
      font-weight: 600;
      letter-spacing: -0.02em;
    }
    .helper-settings-page-description,
    .codex-helper-settings-row-description,
    .codex-helper-native-settings-list-footer {
      margin: 0;
      color: color-mix(in srgb, CanvasText 62%, transparent);
    }
    .helper-settings-page-description {
      font-size: 13px;
      line-height: 1.45;
    }
    .codex-helper-settings-section-title,
    .codex-helper-settings-row-title,
    .codex-helper-native-settings-about-name {
      font-weight: 600;
    }
    .codex-helper-settings-row {
      display: flex;
      align-items: center;
      justify-content: space-between;
      gap: 24px;
      padding: 12px 16px;
    }
    .codex-helper-settings-row-copy,
    .helper-settings-about-copy {
      display: flex;
      flex-direction: column;
      gap: 2px;
      min-width: 0;
      flex: 1 1 auto;
    }
    .codex-helper-settings-row-description {
      font-size: 12px;
      line-height: 1.35;
    }
    [data-codex-helper-backend] {
      display: inline-flex;
      align-items: center;
      gap: 6px;
      font-weight: 500;
    }
    [data-codex-helper-backend]::before {
      content: "";
      width: 8px;
      height: 8px;
      border-radius: 999px;
      background: currentColor;
      flex: 0 0 auto;
    }
    [data-codex-helper-backend][data-status="ok"] {
      color: light-dark(rgb(21, 128, 61), rgb(74, 222, 128));
    }
    [data-codex-helper-backend][data-status="error"] {
      color: light-dark(rgb(185, 28, 28), rgb(248, 113, 113));
    }
    [data-codex-helper-backend][data-status="loading"] {
      color: color-mix(in srgb, CanvasText 62%, transparent);
    }
    .codex-helper-panel {
      display: flex;
      flex-direction: column;
      overflow: hidden;
      border: 1px solid color-mix(in srgb, CanvasText 12%, transparent);
      border-radius: 12px;
      background: color-mix(in srgb, CanvasText 3%, Canvas);
    }
    .codex-helper-action {
      display: inline-flex;
      align-items: center;
      flex-shrink: 0;
      border: 1px solid color-mix(in srgb, CanvasText 16%, transparent);
      border-radius: 8px;
      padding: 5px 10px;
      background: color-mix(in srgb, CanvasText 6%, Canvas);
      color: inherit;
      font: inherit;
      cursor: pointer;
    }
    .codex-helper-action:hover,
    .codex-helper-action:focus-visible {
      background: color-mix(in srgb, CanvasText 10%, Canvas);
      outline: none;
    }
    .helper-settings-truncate {
      overflow: hidden;
      text-overflow: ellipsis;
      white-space: nowrap;
    }
    .codex-helper-switch {
      position: relative;
      display: inline-flex;
      flex-shrink: 0;
      cursor: pointer;
    }
    [${helperNativeSettingsPageAttribute}="logs"] .helper-settings-page-inner,
    [${helperNativeSettingsPageAttribute}="logs"] .helper-settings-page-body,
    [${helperNativeSettingsPageAttribute}="logs"] .codex-helper-native-settings-log-section {
      flex: 1 1 auto;
      min-height: 0;
      height: 100%;
    }
    [${helperNativeSettingsPageAttribute}="logs"] {
      height: 100%;
      min-height: 0;
    }
  `;
}

function installHelperSettingsShellStyles() {
  let style = document.getElementById("codex-helper-settings-shell-style");
  if (!(style instanceof HTMLStyleElement)) {
    style = document.createElement("style");
    style.id = "codex-helper-settings-shell-style";
    document.head.appendChild(style);
  }
  style.textContent = helperSettingsShellCss();
}

function helperSettingsNavButton(page) {
  return `
    <button type="button" class="helper-settings-nav-item" ${helperNativeSettingsEntryAttribute}="${page.id}">
      ${nativeSettingsStandardIconSvg(page.standardIconName, "codex-helper-native-settings-sidebar-icon")}
      <span>${page.label}</span>
    </button>
  `;
}

function renderHelperSettingsShell(root) {
  root.innerHTML = `
    <div class="helper-settings-shell">
      <div class="helper-settings-titlebar" data-tauri-drag-region></div>
      <nav class="helper-settings-nav" aria-label="Settings">
        <div class="helper-settings-nav-drag"></div>
        <div class="helper-settings-nav-title">Settings</div>
        ${nativeHelperSettingsPages().map(helperSettingsNavButton).join("")}
      </nav>
      <main id="helper-settings-content" class="helper-settings-content"></main>
    </div>
  `;
}

function onHelperSettingsClick(event) {
  const target = event.target;
  if (!(target instanceof Element)) return;
  const navEntry = target.closest(
    `.helper-settings-nav [${helperNativeSettingsEntryAttribute}]`,
  );
  if (navEntry instanceof HTMLElement) {
    event.preventDefault();
    const pageId =
      navEntry.getAttribute(helperNativeSettingsEntryAttribute) || "general";
    openNativeHelperSettingsPage(pageId);
    return;
  }
  if (target.closest(".codex-helper-provider-drag-handle")) return;
  if (providerDragSuppressClick) return;
  const command = target.closest(`[${helperCommandAttribute}]`);
  if (!(command instanceof HTMLElement)) return;
  event.preventDefault();
  handleHelperCommand(
    command.getAttribute(helperCommandAttribute) || "",
    command,
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
}

function helperLogQueryError(error) {
  setHelperText(
    "[data-codex-helper-backend]",
    error?.message || String(error),
  );
  logDiagnostic("settings_command_failed", {
    command: "logs-query",
    error: error?.message || String(error),
  });
}

function onHelperSettingsInput(event) {
  const target = event.target;
  if (
    target instanceof HTMLInputElement &&
    target.hasAttribute("data-codex-helper-log-search")
  ) {
    scheduleHelperLogSearch(helperLogQueryError);
  }
}

function onHelperSettingsChange(event) {
  const target = event.target;
  if (
    (target instanceof HTMLSelectElement &&
      (target.hasAttribute("data-codex-helper-log-date") ||
        target.hasAttribute("data-codex-helper-log-event-filter"))) ||
    (target instanceof HTMLInputElement &&
      target.hasAttribute("data-codex-helper-log-regex"))
  ) {
    loadHelperLogs(true).catch(helperLogQueryError);
    return;
  }
  if (!(target instanceof HTMLInputElement)) return;
  if (target.hasAttribute(helperNumberAttribute)) {
    event.preventDefault();
    handleHelperNumberInput(target).catch((error) => {
      target.disabled = false;
      setHelperText(
        "[data-codex-helper-backend]",
        error?.message || String(error),
      );
      logDiagnostic("settings_update_failed", {
        key: target.getAttribute(helperNumberAttribute),
        error: error?.message || String(error),
      });
    });
    return;
  }
  if (!target.hasAttribute(helperToggleAttribute)) return;
  event.preventDefault();
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
}

function onHelperSettingsKeydown(event) {
  if (event.key === "Enter") {
    const target = event.target;
    if (
      target instanceof HTMLInputElement &&
      target.hasAttribute("data-codex-helper-log-search")
    ) {
      event.preventDefault();
      loadHelperLogs(true).catch(helperLogQueryError);
    }
    return;
  }
  if (event.key !== "Escape") return;
  if (helperNativeSettingsRoot?.getAttribute("data-codex-helper-log-view") === "detail") {
    event.preventDefault();
    closeHelperLogDetail();
    return;
  }
  if (!providerDialogRoot?.isConnected) return;
  handleProviderCommand("provider-dialog-cancel").catch((error) => {
    setHelperText(
      "[data-codex-helper-backend]",
      error?.message || String(error),
    );
    logDiagnostic("settings_command_failed", {
      command: "provider-dialog-cancel",
      error: error?.message || String(error),
    });
  });
}

function startHelperSettingsApp() {
  const root = document.getElementById("helper-settings-app");
  if (!(root instanceof HTMLElement)) {
    throw new Error("Helper Settings app host not found");
  }
  installHelperStyles();
  installHelperSettingsShellStyles();
  renderHelperSettingsShell(root);
  document.addEventListener("click", onHelperSettingsClick, true);
  document.addEventListener("input", onHelperSettingsInput, true);
  document.addEventListener("change", onHelperSettingsChange, true);
  document.addEventListener("keydown", onHelperSettingsKeydown, true);
  window.__codexHelperShowSettingsPage = openNativeHelperSettingsPage;
  openNativeHelperSettingsPage(
    window.__codexHelperPendingSettingsPage || "general",
  );
}

if (document.readyState === "loading") {
  document.addEventListener("DOMContentLoaded", startHelperSettingsApp, {
    once: true,
  });
} else {
  startHelperSettingsApp();
}
