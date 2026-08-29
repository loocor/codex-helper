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
    maintainUsageLimitBanner();
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

function onHelperRuntimeContextMenu(event) {
  const target = event.target;
  if (!(target instanceof Element)) return;
  const portLocalUrl = target.closest("[data-codex-helper-port-local-url]");
  if (portLocalUrl instanceof HTMLElement) {
    event.preventDefault();
    event.stopImmediatePropagation();
    openPortLocalUrlMenu(portLocalUrl, event);
  }
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
  if (maintainUsageLimitBannerTimer) clearTimeout(maintainUsageLimitBannerTimer);
  if (refreshPortsPanelTimer) clearTimeout(refreshPortsPanelTimer);
  if (pinnedSummaryHideTimer) clearTimeout(pinnedSummaryHideTimer);
  stopPortScanLoop();
  if (helperRuntimeObserver) helperRuntimeObserver.disconnect();
  closePortForwardRowMenu();
  closePortForwardDialog();
  removeHelperRuntimeEventListeners();
  pendingPortScan = 0;
  maintainPortsPanelTimer = 0;
  maintainUsageLimitBannerTimer = 0;
  refreshPortsPanelTimer = 0;
  pinnedSummaryHideTimer = 0;
  lastRuntimeActivityKey = "";
  lastRuntimeActivityAt = 0;
  lastRemotePortSyncStartedAt = 0;
  lastRemotePortSyncSessionKey = "";
  observerInstalled = false;
  helperRuntimeObserver = null;
};

function onHelperSettingsApplied(wasPortForwardingEnabled) {
  maintainPortsPanel();
  maintainUsageLimitBanner();
  if (featureSettings.portForwardingEnabled) schedulePortScan();
  else if (wasPortForwardingEnabled) handlePortForwardingDisabled();
}

installHelperRuntimeEventListeners();
installHelperStyles();
removeLegacyPortsBottomPanelUi();
maintainPortsPanel();
maintainUsageLimitBanner();
logDiagnostic("runtime.ready", helperRuntimeActivityDetail());
reportHelperRuntimeActivity();
refreshFeatureSettings().catch((error) => {
  logDiagnostic("settings_feature_refresh_failed", {
    error: error?.message || String(error),
  });
});
installObserver();
