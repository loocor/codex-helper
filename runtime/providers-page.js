// Provider list and add/edit subpages
let providerDialogRoot = null;
let providerCache = [];
let providerActiveId = "";
let providerFetchedModels = [];
let providerModelsFetchedThisSession = false;
let providerOauthPollTimer = null;
let providerOauthDeviceCode = "";
let providerDragSuppressClick = false;

const PROVIDER_AUTH_DEFAULTS = {
  apiKey: { name: "", baseUrl: "", wireApi: "responses", model: "", usagePageUrl: "" },
  github_copilot: {
    name: "GitHub Copilot",
    baseUrl: "https://api.githubcopilot.com",
    wireApi: "chat",
    model: "",
    usagePageUrl: "https://github.com/settings/copilot",
  },
  xai_oauth: {
    name: "xAI Grok",
    baseUrl: "https://api.x.ai/v1",
    wireApi: "responses",
    model: "",
    usagePageUrl: "https://grok.com/?_s=usage",
  },
};

const PROVIDER_PRESETS = {
  custom: { name: "", baseUrl: "", wireApi: "responses", model: "", usagePageUrl: "" },
  deepseek: {
    name: "DeepSeek",
    baseUrl: "https://api.deepseek.com/v1",
    wireApi: "chat",
    model: "deepseek-chat",
    usagePageUrl: "https://platform.deepseek.com/usage",
  },
  kimi: {
    name: "Kimi",
    baseUrl: "https://api.moonshot.cn/v1",
    wireApi: "chat",
    model: "kimi-k2.5",
    usagePageUrl: "https://platform.moonshot.cn/console",
  },
  minimax: {
    name: "MiniMax",
    baseUrl: "https://api.minimaxi.com/v1",
    wireApi: "responses",
    model: "MiniMax-M3",
    usagePageUrl: "https://platform.minimaxi.com",
  },
  dashscope: {
    name: "DashScope",
    baseUrl: "https://dashscope.aliyuncs.com/compatible-mode/v1",
    wireApi: "responses",
    model: "qwen3-coder-plus",
    usagePageUrl: "https://bailian.console.aliyun.com",
  },
};

const MASKED_API_KEY = "********";
const CHATGPT_MODEL_OPTIONS = ["gpt-5.6-sol", "gpt-5.6-luna", "gpt-5.6-codex"];
const CODEX_REASONING_LEVELS = [
  "none",
  "minimal",
  "low",
  "medium",
  "high",
  "xhigh",
  "max",
  "ultra",
];

const CONTEXT_WINDOW_OTHERS = "others";
const CONTEXT_WINDOW_PRESETS = [
  { value: 8192, label: "8K" },
  { value: 16384, label: "16K" },
  { value: 32768, label: "32K" },
  { value: 65536, label: "64K" },
  { value: 128000, label: "128K" },
  { value: 200000, label: "200K" },
  { value: 256000, label: "256K" },
  { value: 512000, label: "512K" },
  { value: 1000000, label: "1M" },
  { value: 2000000, label: "2M" },
];

function stopProviderOauthPoll() {
  if (providerOauthPollTimer) {
    clearTimeout(providerOauthPollTimer);
    providerOauthPollTimer = null;
  }
  providerOauthDeviceCode = "";
}

function closeProviderDialog(options = {}) {
  stopProviderOauthPoll();
  if (!options.skipDraft) persistProviderDraft();
  if (providerDialogRoot?.isConnected) providerDialogRoot.remove();
  providerDialogRoot = null;
}

function providerDraftKey(mode, id) {
  return `codex-helper-provider-draft:${mode || "new"}:${id || "new"}`;
}

function persistProviderDraft() {
  if (!(providerDialogRoot instanceof HTMLElement)) return;
  syncUsageUrlOpenButton();
  const mode = providerDialogRoot.getAttribute("data-codex-helper-provider-mode") || "new";
  if (mode === "details") return;
  const id = providerDialogRoot.getAttribute("data-codex-helper-provider-id") || "new";
  try {
    sessionStorage.setItem(
      providerDraftKey(mode, id),
      JSON.stringify({
        ...providerDialogPayload(),
        fetchedModels: providerFetchedModels,
        modelsFetched: providerModelsFetchedThisSession,
      }),
    );
  } catch (_error) {}
}

function readProviderDraft(mode, id) {
  try {
    const raw = sessionStorage.getItem(providerDraftKey(mode, id));
    return raw ? JSON.parse(raw) : null;
  } catch (_error) {
    return null;
  }
}

function clearProviderDraft(mode, id) {
  try {
    sessionStorage.removeItem(providerDraftKey(mode, id));
  } catch (_error) {}
}

function sortProviders(providers) {
  const official = [];
  const others = [];
  for (const provider of providers) {
    if (provider?.id === "official") official.push(provider);
    else others.push(provider);
  }
  return [...official, ...others];
}

function providerById(id) {
  return providerCache.find((item) => item.id === id) || null;
}

function providerAuthMode(provider) {
  const compat = String(provider?.compat || "").toLowerCase();
  if (compat === "github-copilot" || compat === "github_copilot") return "github_copilot";
  if (compat === "xai-oauth" || compat === "xai_oauth") return "xai_oauth";
  return "apiKey";
}

function providerAuthLabel(provider) {
  if (provider?.id === "official") return "Official ChatGPT login";
  const mode = providerAuthMode(provider);
  if (mode === "github_copilot") return "GitHub Copilot";
  if (mode === "xai_oauth") return "xAI Grok";
  return provider?.model || provider?.baseUrl || "API key";
}

function setProviderStatus(message) {
  setHelperText("[data-codex-helper-providers-status]", message);
}

function providerLiveRefreshMessage(verb, name, refresh) {
  const label = name || "provider";
  if (refresh === "restart_desktop") {
    return `${verb} ${label}. Helper is already using it. Restart ChatGPT desktop so login and the model picker refresh.`;
  }
  if (refresh === "new_conversation") {
    return `${verb} ${label}. Helper is already using it. Start a new ChatGPT conversation to pick it up.`;
  }
  return `${verb} ${label}.`;
}

function logProviderEvent(event, detail) {
  const safe = { ...detail };
  delete safe.apiKey;
  delete safe.deviceCode;
  logDiagnostic(event, safe);
}

function renderProviders(result) {
  const providers = sortProviders(
    Array.isArray(result?.providers) ? result.providers : [],
  );
  providerCache = providers;
  providerActiveId = typeof result?.activeId === "string" ? result.activeId : "";
  const statusText =
    result?.status === "ok"
      ? `${providers.length} provider${providers.length === 1 ? "" : "s"}`
      : resultText(result);
  setProviderStatus(statusText);
  const lists = helperSettingsRoots()
    .map((root) => root.querySelector("[data-codex-helper-providers-list]"))
    .filter((panel) => panel instanceof HTMLElement);
  for (const list of lists) {
    list.textContent = "";
    if (result?.status !== "ok") {
      list.appendChild(createScrollEmptyMessage(statusText));
      continue;
    }
    for (const provider of providers) {
      list.appendChild(createProviderListRow(provider, provider.id === providerActiveId));
    }
  }
  void refreshActiveProviderUsage();
}

function createProviderListRow(provider, active) {
  const row = document.createElement("div");
  row.className = "codex-helper-provider-row";
  row.setAttribute("data-codex-helper-provider-id", provider.id);

  const handle = createProviderDragHandle(row, provider);
  const label = document.createElement("div");
  label.className = "codex-helper-provider-row-copy";
  const name = document.createElement("div");
  name.className = "codex-helper-provider-row-name";
  name.textContent = provider.name || provider.id;
  const meta = document.createElement("div");
  meta.className = "codex-helper-provider-row-meta";
  meta.textContent = providerAuthLabel(provider);
  label.appendChild(name);
  label.appendChild(meta);

  const toggle = document.createElement("label");
  toggle.className = "codex-helper-switch";
  toggle.setAttribute(helperCommandAttribute, "activate-provider");
  toggle.setAttribute("data-codex-helper-provider-id", provider.id);
  toggle.setAttribute("aria-label", `Use ${provider.name || provider.id}`);
  const checkbox = document.createElement("input");
  checkbox.type = "checkbox";
  checkbox.setAttribute("role", "switch");
  checkbox.checked = active;
  const track = document.createElement("span");
  track.className = "codex-helper-switch-track";
  track.setAttribute("aria-hidden", "true");
  const thumb = document.createElement("span");
  thumb.className = "codex-helper-switch-thumb";
  track.appendChild(thumb);
  toggle.appendChild(checkbox);
  toggle.appendChild(track);

  row.appendChild(handle);
  row.appendChild(createProviderUsagePie(provider));
  row.appendChild(label);
  row.appendChild(toggle);
  if (provider.id !== "official") {
    row.classList.add("codex-helper-provider-row-openable");
    row.setAttribute(helperCommandAttribute, "provider-open");
    row.setAttribute("aria-label", `Edit ${provider.name || provider.id}`);
    const chevron = document.createElement("span");
    chevron.className = "codex-helper-provider-chevron";
    chevron.setAttribute("aria-hidden", "true");
    chevron.innerHTML = nativeSettingsStandardIconSvg("chevron-right");
    row.appendChild(chevron);
  }
  return row;
}

const PROVIDER_REORDER_THRESHOLD = 4;
let providerReorderSession = null;

function createProviderDragHandle(row, provider) {
  if (provider.id === "official") {
    const spacer = document.createElement("span");
    spacer.className = "codex-helper-provider-drag-handle";
    spacer.setAttribute("aria-hidden", "true");
    return spacer;
  }
  const handle = document.createElement("button");
  handle.type = "button";
  handle.className = "codex-helper-provider-drag-handle";
  handle.setAttribute("aria-label", `Reorder ${provider.name || provider.id}`);
  handle.setAttribute("title", "Drag to reorder");
  handle.innerHTML = nativeSettingsStandardIconSvg("grip-vertical");
  handle.addEventListener("click", (event) => {
    event.preventDefault();
    event.stopPropagation();
  });
  handle.addEventListener("dragstart", (event) => {
    event.preventDefault();
  });
  handle.addEventListener("pointerdown", (event) => {
    startProviderReorder(event, row, provider, handle);
  });
  return handle;
}

function startProviderReorder(event, row, provider, handle) {
  if (event.button !== 0 || provider.id === "official") return;
  event.preventDefault();
  event.stopPropagation();
  const list = row.closest("[data-codex-helper-providers-list]");
  if (!(list instanceof HTMLElement)) return;
  cancelProviderReorder();
  providerReorderSession = {
    fromId: provider.id,
    row,
    list,
    handle,
    pointerId: event.pointerId,
    startX: event.clientX,
    startY: event.clientY,
    offsetX: event.clientX - row.getBoundingClientRect().left,
    offsetY: event.clientY - row.getBoundingClientRect().top,
    active: false,
    layer: null,
    ghost: null,
    targetId: "",
    after: false,
  };
  try {
    handle.setPointerCapture(event.pointerId);
  } catch (_error) {}
  handle.addEventListener("pointermove", onProviderReorderPointerMove);
  handle.addEventListener("pointerup", onProviderReorderPointerUp);
  handle.addEventListener("pointercancel", onProviderReorderPointerUp);
}

function onProviderReorderPointerMove(event) {
  const session = providerReorderSession;
  if (!session || event.pointerId !== session.pointerId) return;
  event.preventDefault();
  const dx = event.clientX - session.startX;
  const dy = event.clientY - session.startY;
  if (!session.active) {
    if (dx * dx + dy * dy < PROVIDER_REORDER_THRESHOLD * PROVIDER_REORDER_THRESHOLD) return;
    activateProviderReorder(session);
  }
  moveProviderReorderGhost(session, event.clientX, event.clientY);
  updateProviderReorderTarget(session, event.clientY);
}

function activateProviderReorder(session) {
  session.active = true;
  providerDragSuppressClick = true;
  session.row.setAttribute("data-dragging", "true");
  document.documentElement.setAttribute("data-codex-helper-provider-reordering", "true");
  const ghost = session.row.cloneNode(true);
  ghost.removeAttribute("data-codex-helper-command");
  ghost.removeAttribute("data-dragging");
  ghost.removeAttribute("data-drop-edge");
  ghost.classList.add("codex-helper-provider-reorder-ghost");
  ghost.setAttribute("aria-hidden", "true");
  for (const node of ghost.querySelectorAll("input, button")) {
    node.tabIndex = -1;
    node.disabled = true;
  }
  const layer = document.createElement("div");
  layer.className = "codex-helper-provider-reorder-layer";
  layer.setAttribute(helperNativeSettingsPageAttribute, "providers");
  layer.setAttribute("aria-hidden", "true");
  layer.appendChild(ghost);
  document.body.appendChild(layer);
  session.layer = layer;
  session.ghost = ghost;
}

function moveProviderReorderGhost(session, x, y) {
  if (!(session.ghost instanceof HTMLElement)) return;
  const rect = session.row.getBoundingClientRect();
  session.ghost.style.width = `${rect.width}px`;
  session.ghost.style.height = `${rect.height}px`;
  session.ghost.style.left = `${Math.round(x - session.offsetX)}px`;
  session.ghost.style.top = `${Math.round(y - session.offsetY)}px`;
}

function updateProviderReorderTarget(session, clientY) {
  const rows = [...session.list.querySelectorAll(".codex-helper-provider-row")];
  if (rows.length === 0) return;
  let target = rows[rows.length - 1];
  let after = true;
  for (const row of rows) {
    const rect = row.getBoundingClientRect();
    if (clientY < rect.top + rect.height / 2) {
      target = row;
      after = false;
      break;
    }
    target = row;
    after = true;
  }
  const targetId = target.getAttribute("data-codex-helper-provider-id") || "";
  if (targetId === "official") after = true;
  session.targetId = targetId;
  session.after = after;
  setProviderDropEdge(target, after);
}

function onProviderReorderPointerUp(event) {
  const session = providerReorderSession;
  if (!session || event.pointerId !== session.pointerId) return;
  const { fromId, targetId, after, active } = session;
  cancelProviderReorder();
  if (!active) return;
  void persistProviderOrder(fromId, targetId, after).catch((error) => {
    setHelperText(
      "[data-codex-helper-backend]",
      error?.message || String(error),
    );
    logProviderEvent("providers.reorder_failed", {
      fromId,
      targetId,
      error: error?.message || String(error),
    });
  });
  setTimeout(() => {
    providerDragSuppressClick = false;
  }, 0);
}

function cancelProviderReorder() {
  const session = providerReorderSession;
  providerReorderSession = null;
  if (!session) return;
  session.handle.removeEventListener("pointermove", onProviderReorderPointerMove);
  session.handle.removeEventListener("pointerup", onProviderReorderPointerUp);
  session.handle.removeEventListener("pointercancel", onProviderReorderPointerUp);
  try {
    session.handle.releasePointerCapture(session.pointerId);
  } catch (_error) {}
  session.row.removeAttribute("data-dragging");
  session.layer?.remove();
  session.ghost?.remove();
  document.documentElement.removeAttribute("data-codex-helper-provider-reordering");
  clearProviderDropEdges();
}

function setProviderDropEdge(row, after) {
  for (const node of document.querySelectorAll("[data-codex-helper-provider-id][data-drop-edge]")) {
    if (node !== row) node.removeAttribute("data-drop-edge");
  }
  row.setAttribute("data-drop-edge", after ? "after" : "before");
}

function clearProviderDropEdges() {
  for (const node of document.querySelectorAll("[data-codex-helper-provider-id][data-drop-edge]")) {
    node.removeAttribute("data-drop-edge");
  }
}

async function persistProviderOrder(fromId, targetId, after) {
  if (!fromId || fromId === "official" || fromId === targetId) return;
  const others = providerCache
    .filter((provider) => provider.id && provider.id !== "official")
    .map((provider) => provider.id);
  const fromIndex = others.indexOf(fromId);
  if (fromIndex < 0) return;
  others.splice(fromIndex, 1);
  if (targetId === "official") {
    others.unshift(fromId);
  } else {
    let toIndex = others.indexOf(targetId);
    if (toIndex < 0) return;
    if (after) toIndex += 1;
    others.splice(toIndex, 0, fromId);
  }
  const current = providerCache
    .filter((provider) => provider.id && provider.id !== "official")
    .map((provider) => provider.id);
  if (others.every((id, index) => id === current[index])) return;
  const result = await bridge("/providers/reorder", { ids: others });
  if (result?.status !== "ok") {
    throw new Error(result?.message || "Failed to reorder providers");
  }
  logProviderEvent("providers.reordered", { ids: others });
  renderProviders(result);
}

function createProviderUsagePie(provider) {
  const clickable = Boolean(provider.usagePageUrl);
  const pie = document.createElement(clickable ? "button" : "span");
  pie.className = "codex-helper-provider-usage-pie";
  pie.setAttribute("data-codex-helper-provider-usage", "");
  pie.setAttribute("data-codex-helper-provider-id", provider.id);
  if (clickable) {
    pie.type = "button";
    pie.setAttribute(helperCommandAttribute, "open-provider-usage");
  } else {
    pie.setAttribute("aria-hidden", "true");
  }
  pie.innerHTML =
    '<svg viewBox="0 0 28 28" aria-hidden="true">' +
    '<circle class="codex-helper-provider-usage-track" cx="14" cy="14" r="10"></circle>' +
    '<circle class="codex-helper-provider-usage-fill" cx="14" cy="14" r="10"></circle>' +
    "</svg>";
  setProviderUsagePie(pie, {
    percent: null,
    tooltip: clickable ? "Open usage page" : "",
  });
  return pie;
}

function usagePercentFromResult(result) {
  if (result?.status !== "ok" || result.usedPercent == null || result.usedPercent === "") {
    return null;
  }
  const value = Number(result.usedPercent);
  if (!Number.isFinite(value)) return null;
  return Math.min(100, Math.max(0, value));
}

function setProviderUsagePie(node, { percent, tooltip, checking = false }) {
  if (percent == null) {
    node.style.removeProperty("--usage-percent");
    node.removeAttribute("data-has-usage");
  } else {
    node.style.setProperty("--usage-percent", String(percent));
    node.setAttribute("data-has-usage", "true");
  }
  if (tooltip) {
    node.setAttribute("title", tooltip);
    if (node.getAttribute("aria-hidden") !== "true") {
      node.setAttribute("aria-label", tooltip);
    }
  } else {
    node.removeAttribute("title");
    if (node.getAttribute("aria-hidden") !== "true") {
      node.setAttribute("aria-label", "Usage");
    }
  }
  node.setAttribute("aria-busy", checking ? "true" : "false");
}

async function refreshActiveProviderUsage() {
  if (!providerActiveId) return;
  const nodes = helperSettingsRoots()
    .map((root) =>
      root.querySelector(
        `[data-codex-helper-provider-id="${CSS.escape(providerActiveId)}"] [data-codex-helper-provider-usage]`,
      ),
    )
    .filter((node) => node instanceof HTMLElement);
  if (nodes.length === 0) return;
  for (const node of nodes) {
    setProviderUsagePie(node, {
      percent: null,
      tooltip: "Checking usage…",
      checking: true,
    });
  }
  const result = await bridge("/providers/usage", { id: providerActiveId });
  for (const node of nodes) {
    const clickable = node.getAttribute(helperCommandAttribute) === "open-provider-usage";
    if (result?.status === "failed") {
      setProviderUsagePie(node, {
        percent: null,
        tooltip: result.message || "Usage query failed",
      });
      continue;
    }
    const percent = usagePercentFromResult(result);
    const tooltip =
      result?.summary ||
      (clickable ? "Open usage page" : "Usage unavailable");
    setProviderUsagePie(node, { percent, tooltip });
  }
}

function providerFormBackButton() {
  return `<button type="button" class="helper-settings-back" ${helperCommandAttribute}="provider-dialog-cancel">${nativeSettingsStandardIconSvg("chevron-left")}<span>Back</span></button>`;
}

function returnToProvidersList(result) {
  closeProviderDialog({ skipDraft: true });
  const host = helperSettingsContentHost();
  if (!(host instanceof HTMLElement)) {
    throw new Error("Helper Settings content host not found");
  }
  renderNativeHelperSettingsPage(host, "providers");
  if (result) renderProviders(result);
  else {
    refreshHelperPage().catch((error) => {
      setProviderStatus(error?.message || String(error));
      logDiagnostic("settings_refresh_failed", {
        surface: "providers-list",
        error: error?.message || String(error),
      });
    });
  }
}

function dialogField(name) {
  return providerDialogRoot?.querySelector(`[data-codex-helper-provider-field="${name}"]`);
}

function dialogFieldValue(name) {
  return (dialogField(name)?.value || "").trim();
}

function isDeviceOauthMode(mode) {
  return mode === "github_copilot" || mode === "xai_oauth";
}

function collectCatalogModels() {
  const models = [];
  const rows = providerDialogRoot?.querySelectorAll("[data-codex-helper-catalog-row]") || [];
  for (const row of rows) {
    const model = (row.querySelector("[data-codex-helper-catalog-model]")?.value || "").trim();
    if (!model) continue;
    const displayName = (row.querySelector("[data-codex-helper-catalog-display]")?.value || "").trim();
    let reasoning = [];
    try {
      const parsed = JSON.parse(row.getAttribute("data-reasoning-levels") || "[]");
      reasoning = Array.isArray(parsed) ? parsed : [];
    } catch (_error) {
      reasoning = [];
    }
    const defaultReasoningLevel = row.getAttribute("data-default-reasoning-level") || "";
    models.push({
      model,
      displayName: displayName || model,
      contextWindow: catalogContextValue(row),
      reasoningLevels: Array.isArray(reasoning) ? reasoning : [],
      defaultReasoningLevel,
    });
  }
  return models;
}

function existingProviderMappings() {
  const id = providerDialogRoot?.getAttribute("data-codex-helper-provider-id") || "";
  const provider = providerById(id);
  return Array.isArray(provider?.modelMappings) ? provider.modelMappings : [];
}

function providerDialogPayload() {
  const catalogModels = collectCatalogModels();
  const authMode = dialogFieldValue("authMode") || "apiKey";
  const defaults = PROVIDER_AUTH_DEFAULTS[authMode] || PROVIDER_AUTH_DEFAULTS.apiKey;
  const payload = {
    name: dialogFieldValue("name"),
    kind: "apiKey",
    authMode,
    baseUrl: isDeviceOauthMode(authMode) ? defaults.baseUrl : dialogFieldValue("baseUrl") || defaults.baseUrl,
    usagePageUrl: dialogFieldValue("usagePageUrl"),
    wireApi: isDeviceOauthMode(authMode)
      ? defaults.wireApi
      : dialogFieldValue("wireApi") || defaults.wireApi || "responses",
    model: dialogFieldValue("model"),
    apiKey: dialogFieldValue("apiKey"),
    preset: dialogFieldValue("preset") || "custom",
    compat: authMode === "apiKey" ? "" : authMode,
    modelMappings: existingProviderMappings(),
    catalogModels,
  };
  if (providerModelsFetchedThisSession) {
    payload.models = providerFetchedModels;
  }
  const id = providerDialogRoot?.getAttribute("data-codex-helper-provider-id") || "";
  if (id && id !== "official") payload.id = id;
  return payload;
}

function selectedReasoningLevels(row) {
  try {
    const parsed = JSON.parse(row.getAttribute("data-reasoning-levels") || "[]");
    return CODEX_REASONING_LEVELS.filter((level) => parsed.includes(level));
  } catch (_error) {
    return [];
  }
}

function syncReasoningChips(row) {
  const selected = selectedReasoningLevels(row);
  for (const chip of row.querySelectorAll("[data-reasoning-level]")) {
    const on = selected.includes(chip.getAttribute("data-reasoning-level"));
    chip.setAttribute("aria-pressed", on ? "true" : "false");
  }
}

function toggleReasoningLevel(row, level) {
  const selected = selectedReasoningLevels(row);
  const next = CODEX_REASONING_LEVELS.filter((item) =>
    item === level ? !selected.includes(item) : selected.includes(item),
  );
  row.setAttribute("data-reasoning-levels", JSON.stringify(next));
  const currentDefault = row.getAttribute("data-default-reasoning-level");
  if (currentDefault && !next.includes(currentDefault)) {
    row.setAttribute("data-default-reasoning-level", "");
  }
  syncReasoningChips(row);
  persistProviderDraft();
}

function parseContextWindow(raw) {
  const value = Number.parseInt(String(raw || "").replace(/[_\s,]/g, ""), 10);
  return Number.isInteger(value) && value > 0 ? value : undefined;
}

function parseContextWindowK(raw) {
  const text = String(raw || "").trim().replace(/[_\s,]/g, "");
  if (!text) return undefined;
  const match = text.match(/^(\d+(?:\.\d+)?)k?$/i);
  if (!match) return undefined;
  const k = Number(match[1]);
  if (!Number.isFinite(k) || k <= 0) return undefined;
  const tokens = Math.round(k * 1000);
  return tokens > 0 ? tokens : undefined;
}

function formatContextWindowK(tokens) {
  if (!Number.isInteger(tokens) || tokens <= 0) return "";
  if (tokens % 1000 === 0) return `${tokens / 1000}k`;
  return `${tokens / 1000}k`;
}

function formatContextWindowKInput(tokens) {
  if (!Number.isInteger(tokens) || tokens <= 0) return "";
  if (tokens % 1000 === 0) return String(tokens / 1000);
  return String(tokens / 1000);
}

function isContextWindowPreset(value) {
  return CONTEXT_WINDOW_PRESETS.some((item) => item.value === value);
}

function catalogContextValue(row) {
  const select = row.querySelector("[data-codex-helper-catalog-context-preset]");
  const custom = row.querySelector("[data-codex-helper-catalog-context-custom]");
  const preset = (select?.value || "").trim();
  if (preset === CONTEXT_WINDOW_OTHERS) {
    return parseContextWindowK(custom?.value);
  }
  return parseContextWindow(preset);
}

function customContextOption(select) {
  return select.querySelector("[data-codex-helper-catalog-context-custom-option]");
}

function ensureCustomContextOption(select, tokens) {
  const option = customContextOption(select);
  if (!Number.isInteger(tokens) || tokens <= 0 || isContextWindowPreset(tokens)) {
    option?.remove();
    return;
  }
  let next = option;
  if (!(next instanceof HTMLOptionElement)) {
    next = document.createElement("option");
    next.setAttribute("data-codex-helper-catalog-context-custom-option", "true");
    const others = select.querySelector(`option[value="${CONTEXT_WINDOW_OTHERS}"]`);
    select.insertBefore(next, others);
  }
  next.value = String(tokens);
  next.textContent = formatContextWindowK(tokens);
}

function syncCatalogContextCustom(host) {
  const select = host.querySelector("[data-codex-helper-catalog-context-preset]");
  const input = host.querySelector("[data-codex-helper-catalog-context-custom]");
  const unit = host.querySelector("[data-codex-helper-catalog-context-unit]");
  if (!(select instanceof HTMLSelectElement) || !(input instanceof HTMLInputElement)) return;
  const isOthers = select.value === CONTEXT_WINDOW_OTHERS;
  select.hidden = isOthers;
  input.hidden = !isOthers;
  if (unit) unit.hidden = !isOthers;
  host.classList.toggle("is-custom", isOthers);
}

function commitCatalogContextCustom(host) {
  const select = host.querySelector("[data-codex-helper-catalog-context-preset]");
  const input = host.querySelector("[data-codex-helper-catalog-context-custom]");
  if (!(select instanceof HTMLSelectElement) || !(input instanceof HTMLInputElement)) return;
  if (select.value !== CONTEXT_WINDOW_OTHERS) return;
  const tokens = parseContextWindowK(input.value);
  if (!tokens) {
    const previous = host.getAttribute("data-previous-context") || "";
    select.value = previous === CONTEXT_WINDOW_OTHERS ? "" : previous;
  } else if (isContextWindowPreset(tokens)) {
    ensureCustomContextOption(select);
    select.value = String(tokens);
    input.value = "";
  } else {
    ensureCustomContextOption(select, tokens);
    select.value = String(tokens);
    input.value = formatContextWindowKInput(tokens);
  }
  syncCatalogContextCustom(host);
}

function catalogContextSelect(selected) {
  const host = document.createElement("div");
  host.className = "codex-helper-provider-catalog-context";
  const select = document.createElement("select");
  select.setAttribute("data-codex-helper-catalog-context-preset", "true");
  select.setAttribute("aria-label", "Context window");
  const empty = document.createElement("option");
  empty.value = "";
  empty.textContent = "Default";
  select.appendChild(empty);
  for (const item of CONTEXT_WINDOW_PRESETS) {
    const option = document.createElement("option");
    option.value = String(item.value);
    option.textContent = item.label;
    select.appendChild(option);
  }
  const others = document.createElement("option");
  others.value = CONTEXT_WINDOW_OTHERS;
  others.textContent = "Others";
  select.appendChild(others);
  const input = document.createElement("input");
  input.type = "text";
  input.inputMode = "decimal";
  input.autocomplete = "off";
  input.placeholder = "500";
  input.setAttribute("data-codex-helper-catalog-context-custom", "true");
  input.setAttribute("aria-label", "Custom context window in k");
  const unit = document.createElement("span");
  unit.className = "codex-helper-provider-catalog-context-unit";
  unit.setAttribute("data-codex-helper-catalog-context-unit", "true");
  unit.textContent = "k";
  const selectedValue = parseContextWindow(selected);
  if (selectedValue && isContextWindowPreset(selectedValue)) {
    select.value = String(selectedValue);
  } else if (selectedValue) {
    ensureCustomContextOption(select, selectedValue);
    select.value = String(selectedValue);
    input.value = formatContextWindowKInput(selectedValue);
  }
  host.appendChild(select);
  host.appendChild(input);
  host.appendChild(unit);
  select.addEventListener("focus", () => {
    host.setAttribute("data-previous-context", select.value);
  });
  select.addEventListener("change", () => {
    if (select.value === CONTEXT_WINDOW_OTHERS) {
      const customTokens = parseContextWindow(customContextOption(select)?.value);
      if (!input.value && customTokens) {
        input.value = formatContextWindowKInput(customTokens);
      }
      syncCatalogContextCustom(host);
      input.focus();
      input.select();
      return;
    }
    syncCatalogContextCustom(host);
  });
  input.addEventListener("keydown", (event) => {
    if (event.key === "Enter") {
      event.preventDefault();
      input.blur();
    }
    if (event.key === "Escape") {
      event.preventDefault();
      input.value = "";
      commitCatalogContextCustom(host);
      select.focus();
    }
  });
  input.addEventListener("blur", () => {
    commitCatalogContextCustom(host);
  });
  syncCatalogContextCustom(host);
  return host;
}

function addCatalogRow(entry = {}) {
  const host = providerDialogRoot?.querySelector("[data-codex-helper-catalog-list]");
  if (!(host instanceof HTMLElement)) return;
  const row = document.createElement("div");
  row.className = "codex-helper-provider-catalog-row";
  row.setAttribute("data-codex-helper-catalog-row", "true");
  const levels = Array.isArray(entry.reasoningLevels)
    ? CODEX_REASONING_LEVELS.filter((level) => entry.reasoningLevels.includes(level))
    : [];
  row.setAttribute("data-reasoning-levels", JSON.stringify(levels));
  row.setAttribute("data-default-reasoning-level", entry.defaultReasoningLevel || "");
  const display = document.createElement("input");
  display.setAttribute("data-codex-helper-catalog-display", "true");
  display.placeholder = "e.g. Grok 4.6";
  display.value = entry.displayName || "";
  const model = document.createElement("input");
  model.setAttribute("data-codex-helper-catalog-model", "true");
  model.setAttribute("list", "codex-helper-fetched-models");
  model.placeholder = "e.g. grok-4.6";
  model.value = entry.model || "";
  const context = catalogContextSelect(entry.contextWindow);
  const reasoning = document.createElement("div");
  reasoning.className = "codex-helper-provider-reasoning";
  reasoning.setAttribute("role", "group");
  reasoning.setAttribute("aria-label", "Reasoning levels");
  for (const level of CODEX_REASONING_LEVELS) {
    const chip = document.createElement("button");
    chip.type = "button";
    chip.className = "codex-helper-provider-reasoning-chip";
    chip.setAttribute("data-reasoning-level", level);
    chip.setAttribute("aria-pressed", levels.includes(level) ? "true" : "false");
    chip.textContent = level;
    chip.addEventListener("click", () => {
      toggleReasoningLevel(row, level);
    });
    reasoning.appendChild(chip);
  }
  const remove = document.createElement("button");
  remove.type = "button";
  remove.className = "codex-helper-provider-mapping-remove";
  remove.setAttribute(helperCommandAttribute, "provider-catalog-remove");
  remove.setAttribute("aria-label", "Remove model");
  remove.innerHTML = nativeSettingsStandardIconSvg("x");
  row.appendChild(display);
  row.appendChild(model);
  row.appendChild(context);
  row.appendChild(reasoning);
  row.appendChild(remove);
  host.appendChild(row);
}

function seedCatalogRows(provider, draft) {
  let catalog = [];
  if (Array.isArray(draft?.catalogModels)) {
    catalog = draft.catalogModels;
  } else if (Array.isArray(provider?.catalogModels)) {
    catalog = provider.catalogModels;
  }
  if (catalog.length > 0) {
    catalog.forEach((entry) => addCatalogRow(entry));
    return;
  }
  const slugs = [];
  const seen = new Set();
  for (const slug of [provider?.model, ...(Array.isArray(provider?.models) ? provider.models : [])]) {
    const value = String(slug || "").trim();
    if (!value || seen.has(value)) continue;
    seen.add(value);
    slugs.push(value);
  }
  slugs.forEach((model) => addCatalogRow({ model, displayName: model }));
}

function autoFillableNames() {
  return [
    ...Object.values(PROVIDER_AUTH_DEFAULTS).map((item) => item.name),
    ...Object.values(PROVIDER_PRESETS).map((item) => item.name),
  ].filter(Boolean);
}

function nameLooksAutoFilled(value) {
  const name = (value || "").trim();
  if (!name) return true;
  if (autoFillableNames().includes(name)) return true;
  return /^GitHub Copilot( \(.*\))?$/.test(name) || /^xAI Grok( \(.*\))?$/.test(name);
}

function applyAutoName() {
  const name = dialogField("name");
  if (!(name instanceof HTMLInputElement) || !nameLooksAutoFilled(name.value)) return;
  const authMode = dialogFieldValue("authMode") || "apiKey";
  if (authMode === "github_copilot") {
    const login = providerDialogRoot?.getAttribute("data-oauth-login") || "";
    name.value = login ? `GitHub Copilot (${login})` : "GitHub Copilot";
    return;
  }
  if (authMode === "xai_oauth") {
    const login = providerDialogRoot?.getAttribute("data-oauth-login") || "";
    name.value = login ? `xAI Grok (${login})` : "xAI Grok";
    return;
  }
  const preset = PROVIDER_PRESETS[dialogFieldValue("preset")] || PROVIDER_PRESETS.custom;
  name.value = preset.name;
}

function applyPresetFields() {
  const preset = PROVIDER_PRESETS[dialogFieldValue("preset")] || PROVIDER_PRESETS.custom;
  const authMode = dialogFieldValue("authMode") || "apiKey";
  if (authMode !== "apiKey" || dialogFieldValue("preset") === "custom") return;
  const base = dialogField("baseUrl");
  const usage = dialogField("usagePageUrl");
  const wire = dialogField("wireApi");
  const model = dialogField("model");
  if (base) base.value = preset.baseUrl;
  if (usage) usage.value = preset.usagePageUrl || "";
  if (wire) wire.value = preset.wireApi;
  if (model) model.value = preset.model;
  applyAutoName();
}

function defaultUsagePageUrl({ authMode, preset, stored }) {
  if ((stored || "").trim()) return stored.trim();
  if (isDeviceOauthMode(authMode)) {
    return PROVIDER_AUTH_DEFAULTS[authMode]?.usagePageUrl || "";
  }
  return PROVIDER_PRESETS[preset]?.usagePageUrl || "";
}

function detectPreset(provider) {
  const url = String(provider?.baseUrl || "").toLowerCase();
  if (url.includes("api.deepseek.com")) return "deepseek";
  if (url.includes("api.moonshot.cn") || url.includes("kimi")) return "kimi";
  if (url.includes("minimaxi.com") || url.includes("minimax.io")) return "minimax";
  if (url.includes("dashscope.aliyuncs.com") || url.includes("bailian.console.aliyun.com")) return "dashscope";
  return "custom";
}

function providerFieldRow(label, controlHtml, options = {}) {
  const classes = ["codex-helper-provider-field-row"];
  if (options.apiOnly) classes.push("codex-helper-provider-api-only");
  const attrs = [];
  if (options.attr) attrs.push(options.attr);
  if (options.hidden) attrs.push("hidden");
  return `
    <div class="${classes.join(" ")}"${attrs.length ? ` ${attrs.join(" ")}` : ""}>
      <span class="codex-helper-provider-field-label">${label}</span>
      ${controlHtml}
    </div>`;
}

function openProviderDialog(mode, provider) {
  closeProviderDialog();
  const host = helperSettingsContentHost();
  if (!(host instanceof HTMLElement)) {
    throw new Error("Helper Settings content host not found");
  }
  const title = mode === "edit" ? "Edit provider" : "Add provider";
  const dialog = document.createElement("section");
  dialog.setAttribute(helperNativeSettingsPageAttribute, "providers");
  dialog.className = "codex-helper-native-settings-page helper-settings-page";
  dialog.setAttribute("data-codex-helper-provider-dialog", "true");
  dialog.setAttribute("data-codex-helper-provider-mode", mode);
  if (provider?.id) dialog.setAttribute("data-codex-helper-provider-id", provider.id);
  dialog.innerHTML = `
    <div class="helper-settings-page-inner">
      <header class="helper-settings-page-header">
        ${providerFormBackButton()}
        <h1 class="helper-settings-page-title"></h1>
      </header>
      <div class="codex-helper-provider-dialog-body">
        ${providerFieldRow("Name", `<input data-codex-helper-provider-field="name">`)}
        ${providerFieldRow(
          "Auth",
          `<select data-codex-helper-provider-field="authMode">
            <option value="apiKey">API key</option>
            <option value="github_copilot">GitHub Copilot</option>
            <option value="xai_oauth">xAI Grok</option>
          </select>`,
        )}
        ${providerFieldRow(
          "Template",
          `<select data-codex-helper-provider-field="preset">
            <option value="custom">Custom</option>
            <option value="deepseek">DeepSeek</option>
            <option value="kimi">Kimi</option>
            <option value="minimax">MiniMax</option>
            <option value="dashscope">DashScope</option>
          </select>`,
          { apiOnly: true, attr: "data-codex-helper-provider-preset-label" },
        )}
        <p class="codex-helper-provider-auth-hint" data-codex-helper-provider-auth-hint></p>
        ${providerFieldRow(
          "Account",
          `<div class="codex-helper-provider-oauth">
            <div class="codex-helper-provider-oauth-row">
              <div class="codex-helper-provider-oauth-status" data-codex-helper-provider-oauth-status></div>
              <button type="button" class="codex-helper-provider-oauth-action" ${helperCommandAttribute}="provider-oauth-start">Sign in</button>
            </div>
            <div class="codex-helper-provider-oauth-code" data-codex-helper-provider-oauth-code hidden></div>
          </div>`,
          { attr: "data-codex-helper-provider-oauth", hidden: true },
        )}
        ${providerFieldRow(
          "Base URL",
          `<input data-codex-helper-provider-field="baseUrl" placeholder="https://api.example.com/v1">`,
          { apiOnly: true, attr: "data-codex-helper-provider-base-label" },
        )}
        ${providerFieldRow(
          "Usage URL",
          `<span class="codex-helper-provider-secret-row">
            <input data-codex-helper-provider-field="usagePageUrl" placeholder="https://platform.example.com/usage">
            <button type="button" class="codex-helper-provider-secret-toggle" ${helperCommandAttribute}="provider-open-usage-url" aria-label="Open usage URL" title="Open usage URL">${nativeSettingsStandardIconSvg("external-link")}</button>
          </span>`,
        )}
        ${providerFieldRow(
          "API key",
          `<span class="codex-helper-provider-secret-row">
            <input data-codex-helper-provider-field="apiKey" type="password" placeholder="${MASKED_API_KEY}" autocomplete="off">
            <button type="button" class="codex-helper-provider-secret-toggle" ${helperCommandAttribute}="provider-toggle-api-key" aria-label="Show API key" title="Show API key">${nativeSettingsStandardIconSvg("eye")}</button>
          </span>`,
          { apiOnly: true, attr: "data-codex-helper-provider-key-label" },
        )}
        ${providerFieldRow(
          "Wire API",
          `<select data-codex-helper-provider-field="wireApi">
            <option value="responses">responses</option>
            <option value="chat">chat</option>
          </select>`,
          { apiOnly: true, attr: "data-codex-helper-provider-wire-label" },
        )}
        ${providerFieldRow(
          "Default model",
          `<input data-codex-helper-provider-field="model" list="codex-helper-fetched-models" placeholder="Select after fetching models">`,
          { attr: "data-codex-helper-provider-model-label" },
        )}
        <div class="codex-helper-provider-mapping-block">
          <div class="codex-helper-provider-mapping-header">
            <span class="codex-helper-provider-field-label">Catalog</span>
            <div class="codex-helper-provider-mapping-actions">
              <button type="button" class="codex-helper-provider-mapping-fetch" ${helperCommandAttribute}="provider-fetch-models">Fetch Models</button>
              <button type="button" class="codex-helper-provider-mapping-add" ${helperCommandAttribute}="provider-catalog-add">+ Add Model</button>
            </div>
          </div>
          <div class="codex-helper-provider-mapping-body">
            <div class="codex-helper-provider-fetch-error" data-codex-helper-provider-fetch-error></div>
            <div class="codex-helper-provider-fetched" data-codex-helper-fetched-list hidden></div>
            <div class="codex-helper-provider-catalog">
              <div class="codex-helper-provider-catalog-columns">
                <span>Menu Display Name</span>
                <span>Actual Request Model</span>
                <span>Context Window</span>
                <span>Reasoning Levels</span>
                <span></span>
              </div>
              <div data-codex-helper-catalog-list></div>
            </div>
          </div>
        </div>
        <datalist id="codex-helper-fetched-models"></datalist>
        <div class="codex-helper-provider-dialog-error" data-codex-helper-provider-dialog-error></div>
      </div>
    </div>
    <div class="codex-helper-provider-dialog-actions">
      ${mode === "edit" ? `<button type="button" class="codex-helper-provider-delete" ${helperCommandAttribute}="provider-delete" data-codex-helper-provider-id="${provider?.id || ""}">Delete</button>` : ""}
      <span class="codex-helper-provider-dialog-spacer"></span>
      <button type="button" ${helperCommandAttribute}="provider-dialog-save">Save</button>
    </div>
  `;
  dialog.querySelector(".helper-settings-page-title").textContent = title;
  host.replaceChildren(dialog);
  helperNativeSettingsRoot = dialog;
  helperNativeSettingsContentHost = host;
  helperNativeSettingsActivePage = "providers";
  updateNativeSettingsActiveEntry("providers");
  providerDialogRoot = dialog;
  const deleteButton = dialog.querySelector(`[${helperCommandAttribute}="provider-delete"]`);
  if (deleteButton instanceof HTMLButtonElement && provider?.id === providerActiveId) {
    deleteButton.disabled = true;
    deleteButton.title = "The active provider cannot be deleted";
  }
  const draft = readProviderDraft(mode, provider?.id || "new");
  const authMode = draft?.authMode || providerAuthMode(provider);
  const oauthDefaults = PROVIDER_AUTH_DEFAULTS[authMode] || PROVIDER_AUTH_DEFAULTS.apiKey;
  const fields = {
    name: draft?.name || provider?.name || "",
    authMode,
    preset: draft?.preset || detectPreset(provider),
    baseUrl: isDeviceOauthMode(authMode)
      ? oauthDefaults.baseUrl
      : draft?.baseUrl || provider?.baseUrl || "",
    usagePageUrl: defaultUsagePageUrl({
      authMode,
      preset: draft?.preset || detectPreset(provider),
      stored: draft?.usagePageUrl || provider?.usagePageUrl || "",
    }),
    wireApi: isDeviceOauthMode(authMode)
      ? oauthDefaults.wireApi
      : draft?.wireApi || provider?.wireApi || "responses",
    model: draft?.model || provider?.model || "",
    apiKey: draft?.apiKey || provider?.apiKey || "",
  };
  for (const [name, value] of Object.entries(fields)) {
    const node = dialogField(name);
    if (node) node.value = value;
  }
  seedCatalogRows(provider, draft);
  providerFetchedModels = Array.isArray(draft?.fetchedModels) ? draft.fetchedModels : [];
  providerModelsFetchedThisSession = draft?.modelsFetched === true;
  renderFetchedModelOptions();
  updateProviderAuthHint({ applyDefaults: mode === "new" && !draft });
  applyAutoName();
  refreshProviderOauthStatus();
  dialog.addEventListener("input", persistProviderDraft);
  dialog.addEventListener("change", (event) => {
    const field = event.target?.getAttribute?.("data-codex-helper-provider-field");
    if (field === "authMode") {
      updateProviderAuthHint({ applyDefaults: true });
      applyAutoName();
      refreshProviderOauthStatus();
    }
    if (field === "preset") {
      applyPresetFields();
      persistProviderDraft();
    }
    persistProviderDraft();
  });
}

function setProviderDialogError(message) {
  const node = providerDialogRoot?.querySelector("[data-codex-helper-provider-dialog-error]");
  if (node) node.textContent = message || "";
}

function setProviderOauthStatus(message, userCode = "") {
  const status = providerDialogRoot?.querySelector("[data-codex-helper-provider-oauth-status]");
  const code = providerDialogRoot?.querySelector("[data-codex-helper-provider-oauth-code]");
  if (status) status.textContent = message || "";
  if (code) {
    code.textContent = userCode || "";
    code.hidden = !userCode;
  }
}

function confirmProviderDelete(provider) {
  closeProviderDialog({ skipDraft: true });
  const host = helperSettingsContentHost();
  if (!(host instanceof HTMLElement)) {
    throw new Error("Helper Settings content host not found");
  }
  const dialog = document.createElement("section");
  dialog.setAttribute(helperNativeSettingsPageAttribute, "providers");
  dialog.className = "codex-helper-native-settings-page helper-settings-page";
  dialog.setAttribute("data-codex-helper-provider-dialog", "true");
  dialog.setAttribute("data-codex-helper-provider-mode", "delete");
  dialog.setAttribute("data-codex-helper-provider-id", provider.id);
  dialog.innerHTML = `
    <div class="helper-settings-page-inner">
      <header class="helper-settings-page-header">
        ${providerFormBackButton()}
        <h1 class="helper-settings-page-title">Delete provider?</h1>
      </header>
      <div class="helper-settings-page-body codex-helper-provider-dialog-panel">
        <div class="codex-helper-provider-dialog-message"></div>
        <div class="codex-helper-provider-dialog-actions">
          <span class="codex-helper-provider-dialog-spacer"></span>
          <button type="button" ${helperCommandAttribute}="provider-delete-confirm" data-codex-helper-provider-id="${provider.id}">Delete</button>
        </div>
      </div>
    </div>
  `;
  dialog.querySelector(".codex-helper-provider-dialog-message").textContent =
    `${provider.name || provider.id} will be removed from Helper. This does not change ChatGPT until you activate another provider.`;
  host.replaceChildren(dialog);
  helperNativeSettingsRoot = dialog;
  helperNativeSettingsContentHost = host;
  helperNativeSettingsActivePage = "providers";
  updateNativeSettingsActiveEntry("providers");
  providerDialogRoot = dialog;
}

function renderFetchedModelOptions() {
  const list = providerDialogRoot?.querySelector("#codex-helper-fetched-models");
  if (list instanceof HTMLDataListElement) {
    list.textContent = "";
    for (const id of providerFetchedModels) {
      const option = document.createElement("option");
      option.value = id;
      list.appendChild(option);
    }
  }
  const host = providerDialogRoot?.querySelector("[data-codex-helper-fetched-list]");
  if (!(host instanceof HTMLElement)) return;
  host.textContent = "";
  if (providerFetchedModels.length === 0) {
    host.hidden = true;
    return;
  }
  host.hidden = false;
  const current = dialogFieldValue("model");
  for (const id of providerFetchedModels) {
    const button = document.createElement("button");
    button.type = "button";
    button.className = "codex-helper-provider-fetched-model";
    button.setAttribute(helperCommandAttribute, "provider-select-model");
    button.setAttribute("data-codex-helper-model-id", id);
    button.setAttribute("aria-pressed", current === id ? "true" : "false");
    button.textContent = id;
    host.appendChild(button);
  }
}

function setProviderFetchError(message) {
  const node = providerDialogRoot?.querySelector("[data-codex-helper-provider-fetch-error]");
  if (node) node.textContent = message || "";
}

function selectFetchedModel(id) {
  const node = dialogField("model");
  if (node) node.value = id;
  const rows = [...(providerDialogRoot?.querySelectorAll("[data-codex-helper-catalog-row]") || [])];
  const empty = rows.find((row) => !(row.querySelector("[data-codex-helper-catalog-model]")?.value || "").trim());
  if (empty) {
    const model = empty.querySelector("[data-codex-helper-catalog-model]");
    const display = empty.querySelector("[data-codex-helper-catalog-display]");
    if (model) model.value = id;
    if (display && !display.value.trim()) display.value = id;
  } else if (!rows.some((row) => (row.querySelector("[data-codex-helper-catalog-model]")?.value || "").trim() === id)) {
    addCatalogRow({ model: id, displayName: id });
  }
  persistProviderDraft();
  renderFetchedModelOptions();
}

function updateProviderAuthHint(options = {}) {
  const hint = providerDialogRoot?.querySelector("[data-codex-helper-provider-auth-hint]");
  const oauth = providerDialogRoot?.querySelector("[data-codex-helper-provider-oauth]");
  const oauthMode = isDeviceOauthMode(dialogFieldValue("authMode") || "apiKey");
  const keyLabel = providerDialogRoot?.querySelector("[data-codex-helper-provider-key-label]");
  const baseLabel = providerDialogRoot?.querySelector("[data-codex-helper-provider-base-label]");
  const wireLabel = providerDialogRoot?.querySelector("[data-codex-helper-provider-wire-label]");
  const presetLabel = providerDialogRoot?.querySelector("[data-codex-helper-provider-preset-label]");
  const base = dialogField("baseUrl");
  const usage = dialogField("usagePageUrl");
  const wire = dialogField("wireApi");
  const authMode = dialogFieldValue("authMode") || "apiKey";
  const defaults = PROVIDER_AUTH_DEFAULTS[authMode] || PROVIDER_AUTH_DEFAULTS.apiKey;
  if (providerDialogRoot) providerDialogRoot.setAttribute("data-auth-mode", authMode);
  if (hint) {
    if (authMode === "github_copilot") {
      hint.textContent = "Sign in with GitHub Copilot. Fetch models after sign-in, then click a model to set it as default.";
    } else if (authMode === "xai_oauth") {
      hint.textContent = "Sign in with xAI Grok. Fetch models after sign-in, then click a model to set it as default.";
    } else {
      hint.textContent = "Use an OpenAI-compatible API key. Choose a template or fill the endpoint, then fetch models.";
    }
  }
  if (oauth) oauth.hidden = !oauthMode;
  if (keyLabel) keyLabel.hidden = oauthMode;
  if (baseLabel) baseLabel.hidden = oauthMode;
  if (wireLabel) wireLabel.hidden = oauthMode;
  if (presetLabel) presetLabel.hidden = oauthMode;
  const knownUsageUrls = [
    ...Object.values(PROVIDER_AUTH_DEFAULTS).map((item) => item.usagePageUrl),
    ...Object.values(PROVIDER_PRESETS).map((item) => item.usagePageUrl),
  ].filter(Boolean);
  if (oauthMode) {
    if (base) base.value = defaults.baseUrl;
    if (wire) wire.value = defaults.wireApi;
    if (usage && (!usage.value.trim() || knownUsageUrls.includes(usage.value.trim()))) {
      usage.value = defaults.usagePageUrl || "";
    }
    applyAutoName();
  } else if (options.applyDefaults) {
    if (base && (!base.value.trim() || Object.values(PROVIDER_AUTH_DEFAULTS).some((item) => item.baseUrl === base.value.trim()) || Object.values(PROVIDER_PRESETS).some((item) => item.baseUrl && item.baseUrl === base.value.trim()))) {
      base.value = defaults.baseUrl;
    }
    if (usage && (!usage.value.trim() || knownUsageUrls.includes(usage.value.trim()))) {
      const preset = PROVIDER_PRESETS[dialogFieldValue("preset")] || PROVIDER_PRESETS.custom;
      usage.value = preset.usagePageUrl || "";
    }
    if (wire) wire.value = defaults.wireApi;
    applyAutoName();
  } else if (base && !base.value.trim() && defaults.baseUrl) {
    base.value = defaults.baseUrl;
  }
  syncUsageUrlOpenButton();
}

function updateOauthAction(signedIn, login = "") {
  const button = providerDialogRoot?.querySelector(`[${helperCommandAttribute}="provider-oauth-start"]`);
  if (providerDialogRoot) {
    if (signedIn && login) providerDialogRoot.setAttribute("data-oauth-login", login);
    else providerDialogRoot.removeAttribute("data-oauth-login");
  }
  if (button) {
    button.textContent = signedIn ? "Re-sign in" : "Sign in";
    button.classList.toggle("codex-helper-provider-oauth-secondary", Boolean(signedIn));
  }
  applyAutoName();
}

async function refreshProviderOauthStatus() {
  const authMode = dialogFieldValue("authMode");
  if (!isDeviceOauthMode(authMode) || !providerDialogRoot) return;
  const result = await bridge("/providers/oauth/status", { kind: authMode });
  if (!providerDialogRoot) return;
  if (result?.status !== "ok") {
    setProviderOauthStatus(resultText(result));
    updateOauthAction(false);
    return;
  }
  if (result.signedIn) {
    setProviderOauthStatus(`Signed in${result.login ? ` as ${result.login}` : ""}`);
    updateOauthAction(true, result.login || "");
  } else {
    setProviderOauthStatus("Not signed in");
    updateOauthAction(false);
  }
}

function scheduleProviderOauthPoll(kind, deviceCode, intervalSec) {
  stopProviderOauthPoll();
  providerOauthDeviceCode = deviceCode;
  const delay = Math.max(3, Number(intervalSec) || 5) * 1000;
  const tick = async () => {
    if (!providerDialogRoot || providerOauthDeviceCode !== deviceCode) return;
    const result = await bridge("/providers/oauth/poll", { kind, deviceCode });
    if (!providerDialogRoot || providerOauthDeviceCode !== deviceCode) return;
    if (result?.status === "ok") {
      setProviderOauthStatus(`Signed in${result.login ? ` as ${result.login}` : ""}`);
      updateOauthAction(true, result.login || "");
      setProviderFetchError("");
      logProviderEvent("providers.oauth_ok", { kind, login: result.login });
      providerOauthDeviceCode = "";
      persistProviderDraft();
      await handleProviderCommand("provider-fetch-models");
      return;
    }
    if (result?.status === "pending") {
      providerOauthPollTimer = setTimeout(tick, delay);
      return;
    }
    setProviderDialogError(resultText(result) || `Sign-in ${result?.status || "failed"}`);
    logProviderEvent("providers.oauth_failed", {
      kind,
      status: result?.status,
      message: result?.message,
    });
    providerOauthDeviceCode = "";
  };
  providerOauthPollTimer = setTimeout(tick, delay);
}

function isMaskedApiKey(value) {
  return value === MASKED_API_KEY;
}

function syncUsageUrlOpenButton() {
  const button = providerDialogRoot?.querySelector(
    `[${helperCommandAttribute}="provider-open-usage-url"]`,
  );
  if (!(button instanceof HTMLButtonElement)) return;
  button.disabled = !dialogFieldValue("usagePageUrl").trim();
}

async function openProviderUsageUrl() {
  const url = dialogFieldValue("usagePageUrl").trim();
  if (!url) {
    throw new Error("Usage URL is required");
  }
  const result = await bridge("/providers/usage/open", { url });
  if (result?.status !== "ok") {
    throw new Error(result?.message || "Failed to open usage page");
  }
}

async function toggleProviderApiKeyVisibility() {
  const input = dialogField("apiKey");
  const button = providerDialogRoot?.querySelector(`[${helperCommandAttribute}="provider-toggle-api-key"]`);
  if (!(input instanceof HTMLInputElement) || !(button instanceof HTMLElement)) return;
  const reveal = input.type !== "text";
  if (reveal && isMaskedApiKey(input.value.trim())) {
    const id = providerDialogRoot?.getAttribute("data-codex-helper-provider-id") || "";
    if (id && id !== "official") {
      const result = await bridge("/providers/secret", { id });
      if (result?.status !== "ok") {
        throw new Error(result?.message || "Failed to reveal API key");
      }
      input.value = typeof result.apiKey === "string" ? result.apiKey : "";
    }
  }
  input.type = reveal ? "text" : "password";
  const label = reveal ? "Hide API key" : "Show API key";
  button.setAttribute("aria-label", label);
  button.setAttribute("title", label);
  button.setAttribute("data-secret-visible", reveal ? "true" : "false");
  button.classList.toggle("is-revealed", reveal);
  button.innerHTML = nativeSettingsStandardIconSvg(reveal ? "eye-off" : "eye");
}

async function handleProviderCommand(command, source) {
  if (command === "open-provider-usage") {
    const id =
      source?.getAttribute("data-codex-helper-provider-id") || providerActiveId;
    const result = await bridge("/providers/usage/open", { id });
    if (result?.status !== "ok") {
      throw new Error(result?.message || "Failed to open usage page");
    }
    return;
  }
  if (command === "provider-open-usage-url") {
    await openProviderUsageUrl();
    return;
  }
  if (command === "provider-dialog-cancel") {
    const mode = providerDialogRoot?.getAttribute("data-codex-helper-provider-mode") || "";
    const id =
      providerDialogRoot?.getAttribute("data-codex-helper-provider-id") || "";
    if (mode === "delete") {
      const provider = providerById(id);
      closeProviderDialog({ skipDraft: true });
      if (provider) openProviderDialog("edit", provider);
      return;
    }
    closeProviderDialog();
    returnToProvidersList();
    return;
  }
  if (command === "provider-toggle-api-key") {
    await toggleProviderApiKeyVisibility();
    return;
  }
  if (command === "provider-select-model") {
    const id = source?.getAttribute("data-codex-helper-model-id") || "";
    if (id) selectFetchedModel(id);
    return;
  }
  if (command === "provider-catalog-add") {
    addCatalogRow();
    persistProviderDraft();
    return;
  }
  if (command === "provider-catalog-remove") {
    source.closest("[data-codex-helper-catalog-row]")?.remove();
    persistProviderDraft();
    return;
  }
  if (source?.getAttribute("data-codex-helper-provider-field") === "authMode") {
    updateProviderAuthHint({ applyDefaults: true });
    persistProviderDraft();
    refreshProviderOauthStatus();
    return;
  }
  if (command === "provider-oauth-start") {
    const kind = dialogFieldValue("authMode");
    if (!isDeviceOauthMode(kind)) return;
    stopProviderOauthPoll();
    setProviderDialogError("");
    setProviderOauthStatus("Starting sign-in…");
    logProviderEvent("providers.oauth_start_requested", { kind });
    const result = await bridge("/providers/oauth/start", { kind });
    if (!providerDialogRoot) return;
    if (result?.status !== "ok") {
      setProviderOauthStatus("Not signed in");
      setProviderDialogError(resultText(result));
      logProviderEvent("providers.oauth_start_failed", { kind, message: result?.message });
      return;
    }
    const userCode = result.userCode || "";
    const waiting = userCode
      ? `Enter ${userCode} in the browser to continue.`
      : "Complete sign-in in the browser.";
    const browserNote = result.browserOpened === false
      ? ` Browser did not open${result.browserError ? `: ${result.browserError}` : ""}. Open ${result.verificationUri || "the verification URL"} manually.`
      : "";
    setProviderOauthStatus(waiting + browserNote, userCode);
    if (!result.deviceCode) {
      setProviderDialogError("OAuth device code is missing");
      return;
    }
    scheduleProviderOauthPoll(kind, result.deviceCode, result.interval);
    logProviderEvent("providers.oauth_started", {
      kind,
      browserOpened: result.browserOpened,
    });
    return;
  }
  if (command === "provider-fetch-models") {
    const payload = providerDialogRoot?.getAttribute("data-codex-helper-provider-id")
      ? { ...providerDialogPayload(), id: providerDialogRoot.getAttribute("data-codex-helper-provider-id") }
      : providerDialogPayload();
    setProviderFetchError("Fetching models…");
    logProviderEvent("providers.models_requested", { id: payload.id || payload.name, baseUrl: payload.baseUrl });
    const result = await bridge("/providers/models", payload);
    if (result?.status !== "ok") {
      setProviderFetchError(resultText(result));
      logProviderEvent("providers.models_failed", { id: payload.id || payload.name, message: result?.message });
      return;
    }
    providerFetchedModels = Array.isArray(result.models) ? result.models : [];
    providerModelsFetchedThisSession = true;
    const currentModel = dialogFieldValue("model");
    if (!currentModel && providerFetchedModels[0]) {
      const node = dialogField("model");
      if (node) node.value = providerFetchedModels[0];
    }
    renderFetchedModelOptions();
    persistProviderDraft();
    setProviderFetchError(
      providerFetchedModels.length
        ? `Fetched ${providerFetchedModels.length} models. Click one to set it as the default and add it to Catalog.`
        : "No models returned",
    );
    logProviderEvent("providers.models_fetched", { count: providerFetchedModels.length });
    return;
  }
  if (command === "new-provider") {
    openProviderDialog("new", { kind: "apiKey", wireApi: "responses", modelMappings: [] });
    logProviderEvent("providers.dialog_opened", { mode: "new" });
    return;
  }
  const id = source?.getAttribute("data-codex-helper-provider-id") || providerDialogRoot?.getAttribute("data-codex-helper-provider-id") || "";
  if (command === "activate-provider") {
    if (!id) return;
    const input = source instanceof HTMLInputElement
      ? source
      : source?.querySelector?.("input");
    const list = source?.closest?.("[data-codex-helper-providers-list]");
    if (id === providerActiveId) {
      if (input instanceof HTMLInputElement) input.checked = true;
      return;
    }
    if (list instanceof HTMLElement) {
      for (const node of list.querySelectorAll(".codex-helper-switch input")) {
        if (node instanceof HTMLInputElement) node.checked = node === input;
      }
    } else if (input instanceof HTMLInputElement) {
      input.checked = true;
    }
    logProviderEvent("providers.activate_requested", { id });
    const result = await bridge("/providers/activate", { id });
    if (result?.status !== "ok") {
      setProviderStatus(resultText(result));
      logProviderEvent("providers.activate_failed", { id, result });
      await refreshHelperPage();
      return;
    }
    renderProviders(result);
    setProviderStatus(
      providerLiveRefreshMessage("Activated", providerById(id)?.name || id, result.refresh),
    );
    logProviderEvent("providers.activated", {
      id,
      activeId: result.activeId,
      refresh: result.refresh,
    });
    return;
  }
  if (command === "provider-open" || command === "provider-edit") {
    const provider = providerById(id);
    if (!provider || provider.id === "official") return;
    openProviderDialog("edit", provider);
    logProviderEvent("providers.dialog_opened", { mode: "edit", id });
    return;
  }
  if (command === "provider-delete") {
    const provider = providerById(id);
    if (!provider) return;
    if (provider.id === "official" || provider.id === providerActiveId) {
      setProviderStatus("The active provider cannot be deleted");
      logProviderEvent("providers.delete_blocked", { id: provider.id });
      return;
    }
    confirmProviderDelete(provider);
    logProviderEvent("providers.delete_prompted", { id: provider.id });
    return;
  }
  if (command === "provider-delete-confirm") {
    if (!id) return;
    const result = await bridge("/providers/delete", { id });
    if (result?.status !== "ok") {
      returnToProvidersList();
      setProviderStatus(resultText(result));
      logProviderEvent("providers.delete_failed", { id, result });
      return;
    }
    returnToProvidersList(result);
    setProviderStatus("Provider deleted");
    logProviderEvent("providers.deleted", { id });
    return;
  }
  if (command === "provider-dialog-save") {
    const payload = providerDialogPayload();
    if (payload.id === "official") {
      setProviderDialogError("The official ChatGPT provider cannot be overwritten");
      return;
    }
    if (!isDeviceOauthMode(payload.authMode) && !String(payload.model || "").trim()) {
      setProviderDialogError("Default model is required");
      return;
    }
    const result = await bridge("/providers/save", payload);
    if (result?.status !== "ok") {
      setProviderDialogError(resultText(result));
      logProviderEvent("providers.save_failed", { id: payload.id || payload.name, result });
      return;
    }
    const mode = providerDialogRoot?.getAttribute("data-codex-helper-provider-mode") || "new";
    const draftId = providerDialogRoot?.getAttribute("data-codex-helper-provider-id") || "new";
    clearProviderDraft(mode, draftId);
    const savedId = result.savedId || payload.id;
    returnToProvidersList(result);
    setProviderStatus(
      result.refresh
        ? providerLiveRefreshMessage(
            "Saved",
            providerById(savedId)?.name || savedId,
            result.refresh,
          )
        : "Provider saved",
    );
    logProviderEvent("providers.saved", { id: savedId, refresh: result.refresh });
    return;
  }
}
