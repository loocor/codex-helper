// Mutation observer, event listeners, and startup hooks
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
removeLegacyPortsBottomPanelUi();
maintainPortsPanel();
installAccountSettingsMenuItems();
refreshFeatureSettings().catch((error) => {
  logDiagnostic("settings_feature_refresh_failed", {
    error: error?.message || String(error),
  });
});
installObserver();
