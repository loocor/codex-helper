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

function replaySessionContextMenu(event, target) {
  target.dispatchEvent(
    new MouseEvent("contextmenu", {
      bubbles: true,
      cancelable: true,
      view: window,
      clientX: event.clientX,
      clientY: event.clientY,
      screenX: event.screenX,
      screenY: event.screenY,
      button: event.button,
      buttons: event.buttons,
      ctrlKey: event.ctrlKey,
      shiftKey: event.shiftKey,
      altKey: event.altKey,
      metaKey: event.metaKey,
    }),
  );
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
  if (!sessionContextMenuReady() && !sessionContextMenuReplayInFlight) {
    event.preventDefault();
    event.stopImmediatePropagation();
    sessionContextMenuReplayInFlight = true;
    prepareSessionContextMenu()
      .catch((error) => {
        logDiagnostic("session_menu_prepare_failed", {
          error: error?.message || String(error),
        });
      })
      .finally(() => {
        try {
          if (target.isConnected) replaySessionContextMenu(event, target);
        } finally {
          sessionContextMenuReplayInFlight = false;
        }
      });
    return;
  }
  trackSessionContextMenu(row);
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
  if (target.hasAttribute(helperNumberAttribute)) {
    event.preventDefault();
    event.stopPropagation();
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

function runtimeActivityKey(detail) {
  return [
    detail?.targetId || "",
    detail?.helperInstanceId || "",
    detail?.href || "",
    detail?.hasFocus ? "focused" : "blurred",
    detail?.visibilityState || "",
  ].join("\n");
}

function reportHelperRuntimeActivity() {
  const detail = helperRuntimeActivityDetail();
  const key = runtimeActivityKey(detail);
  const now = Date.now();
  if (
    key === lastRuntimeActivityKey &&
    now - lastRuntimeActivityAt < RUNTIME_ACTIVITY_REPORT_MIN_INTERVAL_MS
  ) {
    return;
  }
  lastRuntimeActivityKey = key;
  lastRuntimeActivityAt = now;
  bridge("/runtime/activity", detail).catch((error) => {
    console.warn("[Codex Helper] runtime activity report failed", error);
  });
}

function onHelperRuntimeActivity() {
  reportHelperRuntimeActivity();
}

function removeHelperRuntimeEventListeners() {
  document.removeEventListener("click", onHelperRuntimeClick, true);
  document.removeEventListener("contextmenu", onHelperRuntimeContextMenu, true);
  document.removeEventListener("keydown", onHelperRuntimeKeydown, true);
  document.removeEventListener("change", onHelperRuntimeChange, true);
  window.removeEventListener("focus", onHelperRuntimeActivity, true);
  window.removeEventListener("blur", onHelperRuntimeActivity, true);
  document.removeEventListener("visibilitychange", onHelperRuntimeActivity, true);
}

function installHelperRuntimeEventListeners() {
  removeHelperRuntimeEventListeners();
  document.addEventListener("click", onHelperRuntimeClick, true);
  document.addEventListener("contextmenu", onHelperRuntimeContextMenu, true);
  document.addEventListener("keydown", onHelperRuntimeKeydown, true);
  document.addEventListener("change", onHelperRuntimeChange, true);
  window.addEventListener("focus", onHelperRuntimeActivity, true);
  window.addEventListener("blur", onHelperRuntimeActivity, true);
  document.addEventListener("visibilitychange", onHelperRuntimeActivity, true);
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
  if (sessionContextMenuMapRestore) sessionContextMenuMapRestore();
  removeHelperRuntimeEventListeners();
  pendingPortScan = 0;
  maintainPortsPanelTimer = 0;
  refreshPortsPanelTimer = 0;
  pinnedSummaryHideTimer = 0;
  lastRuntimeActivityKey = "";
  lastRuntimeActivityAt = 0;
  lastRemotePortSyncStartedAt = 0;
  lastRemotePortSyncSessionKey = "";
  cachedRemoteProjectMetadata = [];
  cachedRemoteProjectMetadataLoaded = false;
  observerInstalled = false;
  helperRuntimeObserver = null;
};

installHelperRuntimeEventListeners();
installHelperStyles();
removeLegacyPortsBottomPanelUi();
maintainPortsPanel();
installNativeHelperSettingsGroup();
logDiagnostic("runtime.ready", helperRuntimeActivityDetail());
reportHelperRuntimeActivity();
refreshFeatureSettings().catch((error) => {
  logDiagnostic("settings_feature_refresh_failed", {
    error: error?.message || String(error),
  });
});
installObserver();
