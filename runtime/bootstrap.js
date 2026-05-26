// Mutation observer, event listeners, and startup hooks
if (typeof window.__codexHelperRuntimeCleanup === "function") {
  try {
    window.__codexHelperRuntimeCleanup();
  } catch (error) {
    console.warn("[Codex Helper] previous runtime cleanup failed", error);
  }
}

function installObserver() {
  if (observerInstalled) return;
  observerInstalled = true;
  const observer = new MutationObserver(() => {
    maintainPortsPanel();
    installNativeHelperSettingsGroup();
    if (helperPageRoot && !helperPageRoot.isConnected) {
      clearHelperSettingsPage();
    }
    if (helperNativeSettingsRoot && !helperNativeSettingsRoot.isConnected) {
      clearNativeHelperSettingsPage();
    }
  });
  observer.observe(document.documentElement, {
    childList: true,
    subtree: true,
    attributes: true,
    attributeFilter: [
      "role",
      "aria-selected",
      "aria-current",
      "data-state",
      "class",
      "data-app-action-sidebar-thread-active",
      "data-app-action-sidebar-thread-host-id",
      "data-app-action-sidebar-thread-kind",
      "data-app-action-sidebar-thread-id",
    ],
  });
  helperRuntimeObserver = observer;
}

function onHelperRuntimeClick(event) {
  const target = event.target;
  if (!(target instanceof Element)) return;
  if (
    portForwardMenuRoot?.isConnected &&
    !target.closest("[data-codex-helper-port-menu]")
  ) {
    closePortForwardRowMenu();
  }
  const nativeSettingsEntry = target.closest(
    `[${helperNativeSettingsEntryAttribute}]`,
  );
  if (nativeSettingsEntry instanceof HTMLElement) {
    event.preventDefault();
    event.stopPropagation();
    event.stopImmediatePropagation();
    const pageId =
      nativeSettingsEntry.getAttribute(helperNativeSettingsEntryAttribute) ||
      "general";
    try {
      openNativeHelperSettingsPage(pageId);
    } catch (error) {
      showHelperToast(error?.message || String(error));
      logDiagnostic("settings_open_failed", {
        page: pageId,
        error: error?.message || String(error),
      });
    }
    return;
  }
  if (helperNativeSettingsRoot && isNativeSettingsNavigationClick(target)) {
    clearNativeHelperSettingsPage();
  }
  const portCommand = target.closest(`[${helperPortCommandAttribute}]`);
  if (portCommand instanceof HTMLElement) {
    event.preventDefault();
    event.stopPropagation();
    event.stopImmediatePropagation();
    handlePortCommand(portCommand).catch((error) => {
      showHelperToast(error?.message || String(error));
      logDiagnostic("ports_command_failed", {
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
}

function onHelperRuntimeContextMenu(event) {
  const target = event.target;
  if (!(target instanceof Element)) return;
  const portLocalUrl = target.closest("[data-codex-helper-port-local-url]");
  if (portLocalUrl instanceof HTMLElement) {
    event.preventDefault();
    event.stopImmediatePropagation();
    openPortLocalUrlMenu(portLocalUrl, event);
    return;
  }
  const row = sessionRowFromTarget(target);
  if (!(row instanceof HTMLElement)) return;
  trackSessionContextMenu(row);
  if (enabledSessionActions().length === 0) return;
  event.preventDefault();
  event.stopImmediatePropagation();
  const ref = sessionRefFromRow(row);
  if (!ref.session_id) return;
  showExtendedSessionContextMenu(row, ref).catch((error) => {
    showHelperToast(error?.message || String(error));
    logDiagnostic("session_menu_open_failed", {
      error: error?.message || String(error),
    });
  });
}

function onHelperRuntimeKeydown(event) {
  if (event.key !== "Escape") return;
  if (portForwardMenuRoot?.isConnected) {
    closePortForwardRowMenu();
    return;
  }
  if (portForwardDialogRoot?.isConnected) {
    closePortForwardDialog();
    return;
  }
}

function onHelperRuntimeChange(event) {
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
}

function removeHelperRuntimeEventListeners() {
  document.removeEventListener("click", onHelperRuntimeClick, true);
  document.removeEventListener("contextmenu", onHelperRuntimeContextMenu, true);
  document.removeEventListener("keydown", onHelperRuntimeKeydown, true);
  document.removeEventListener("change", onHelperRuntimeChange, true);
}

function installHelperRuntimeEventListeners() {
  removeHelperRuntimeEventListeners();
  document.addEventListener("click", onHelperRuntimeClick, true);
  document.addEventListener("contextmenu", onHelperRuntimeContextMenu, true);
  document.addEventListener("keydown", onHelperRuntimeKeydown, true);
  document.addEventListener("change", onHelperRuntimeChange, true);
}

window.__codexHelperRuntimeCleanup = () => {
  if (pendingPortScan) clearTimeout(pendingPortScan);
  if (maintainPortsPanelTimer) clearTimeout(maintainPortsPanelTimer);
  if (refreshPortsPanelTimer) clearTimeout(refreshPortsPanelTimer);
  if (pinnedSummaryHideTimer) clearTimeout(pinnedSummaryHideTimer);
  stopPortScanLoop();
  if (helperRuntimeObserver) helperRuntimeObserver.disconnect();
  closePortForwardRowMenu();
  closePortForwardDialog();
  clearNativeHelperSettingsPage();
  removeHelperRuntimeEventListeners();
  pendingPortScan = 0;
  maintainPortsPanelTimer = 0;
  refreshPortsPanelTimer = 0;
  pinnedSummaryHideTimer = 0;
  observerInstalled = false;
  helperRuntimeObserver = null;
};

installHelperRuntimeEventListeners();
installHelperStyles();
removeLegacyPortsBottomPanelUi();
maintainPortsPanel();
installNativeHelperSettingsGroup();
refreshFeatureSettings().catch((error) => {
  logDiagnostic("settings_feature_refresh_failed", {
    error: error?.message || String(error),
  });
});
installObserver();
