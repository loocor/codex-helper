// Account menu entry and Helper Settings dialog shell
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

function showHelperSettingsDialog(options = {}) {
  const focusSection =
    typeof options.focusSection === "string" ? options.focusSection : "";
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
  refreshHelperPage()
    .then(() => {
      if (focusSection) focusHelperSettingsSection(focusSection);
    })
    .catch((error) => {
      setHelperText(
        "[data-codex-helper-backend]",
        error?.message || String(error),
      );
      logDiagnostic("settings_refresh_failed", {
        error: error?.message || String(error),
      });
    });
}
