// Helper Settings shared commands and state refresh
function helperSettingsRoots() {
  return [helperNativeSettingsRoot].filter(
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
  const [backend, scripts, settings, zed, log] = await Promise.all([
    bridge("/backend/status"),
    bridge("/runtime/user-scripts"),
    bridge("/settings/get"),
    bridge("/zed-remote/status"),
    bridge("/diagnostics/read-latest"),
  ]);
  setHelperText("[data-codex-helper-backend]", resultText(backend));
  applySettings(settings);
  renderLoadedScripts(scripts);
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
          "No user scripts found.",
        ),
      );
      continue;
    }
    for (const script of scriptList) {
      list.appendChild(createCompactListRow(script, script));
    }
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
    for (const input of root.querySelectorAll(`[${helperNumberAttribute}]`)) {
      if (!(input instanceof HTMLInputElement)) continue;
      const key = input.getAttribute(helperNumberAttribute) || "";
      if (Number.isInteger(settings[key])) input.value = String(settings[key]);
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

async function handleHelperNumberInput(input) {
  const key = input.getAttribute(helperNumberAttribute) || "";
  if (!key) return;
  const value = Number(input.value);
  if (!Number.isInteger(value)) throw new Error(`Settings value for ${key} must be an integer`);
  if (value < 1 || value > 20) {
    const message = `Settings value for ${key} must be between 1 and 20`;
    setHelperText("[data-codex-helper-backend]", message);
    logDiagnostic("settings_update_failed", { key, message });
    applySettings({ status: "ok", settings: featureSettings });
    return;
  }
  input.disabled = true;
  const result = await bridge("/settings/set", { [key]: value });
  input.disabled = false;
  if (result?.status !== "ok") {
    setHelperText(
      "[data-codex-helper-backend]",
      result?.message || "Settings update failed",
    );
    logDiagnostic("settings_update_failed", { key, result });
    applySettings({ status: "ok", settings: featureSettings });
    return;
  }
  applySettings(result);
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
