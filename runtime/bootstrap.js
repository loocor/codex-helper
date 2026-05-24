// Mutation observer, event listeners, and startup hooks
if (typeof window.__codexHelperRuntimeCleanup === "function") {
  try {
    window.__codexHelperRuntimeCleanup();
  } catch (error) {
    console.warn("[Codex Helper] previous runtime cleanup failed", error);
  }
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
    maintainPortsPanel();
    installAccountSettingsMenuItems();
    if (helperPageRoot && !helperPageRoot.isConnected) {
      clearHelperSettingsPage();
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
  if (!document.querySelector(`[${helperSettingsDialogAttribute}]`)) return;
  closeHelperSettingsDialog();
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
installAccountSettingsMenuItems();
refreshFeatureSettings().catch((error) => {
  logDiagnostic("settings_feature_refresh_failed", {
    error: error?.message || String(error),
  });
});
installObserver();
