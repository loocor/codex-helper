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

async function openHelperSettingsFromRuntime(pageId = "general") {
  const result = await bridge("/settings/open", { page: pageId || "general" });
  if (result?.status !== "ok") {
    throw new Error(result?.message || "Failed to open Helper Settings");
  }
}

async function refreshHelperPage() {
  if (helperSettingsRoots().length === 0) return;
  const requests = [
    bridge("/backend/status"),
    bridge("/runtime/user-scripts"),
    bridge("/settings/get"),
    bridge("/providers/list"),
  ];
  if (helperNativeSettingsActivePage === "endpoint") {
    requests.push(bridge("/endpoint/get"));
  }
  const [backend, scripts, settings, providers, extra] = await Promise.all(requests);
  setHelperText(
    "[data-codex-helper-backend]",
    resultText(backend),
    backend?.status === "ok" ? "ok" : "error",
  );
  applySettings(settings);
  renderLoadedScripts(scripts);
  renderProviders(providers);
  if (helperNativeSettingsActivePage === "logs") {
    await loadHelperLogs(true);
  }
  if (helperNativeSettingsActivePage === "endpoint") {
    renderEndpoint(extra);
  }
}

const helperLogPager = {
  pageIndex: 0,
  cursors: [""],
  hasMore: false,
  searching: false,
  query: {
    date: "",
    event: "",
    pattern: "",
    regex: false,
  },
};
let helperLogRecords = [];
const HELPER_LOG_SEARCH_DEBOUNCE_MS = 300;
let helperLogSearchTimer = 0;
let helperLogQuerySeq = 0;
const helperLogEventFilters = [
  ["", "All events"],
  ["llm.request", "llm.request"],
  ["launcher.", "launcher"],
  ["runtime.", "runtime"],
  ["injection.", "injection"],
  ["target_watcher.", "target_watcher"],
  ["bridge.request", "bridge.request"],
  ["providers.", "providers"],
  ["codex.", "codex"],
  ["endpoint.", "endpoint"],
];

function cancelHelperLogSearchTimer() {
  if (!helperLogSearchTimer) return;
  clearTimeout(helperLogSearchTimer);
  helperLogSearchTimer = 0;
}

function scheduleHelperLogSearch(onError) {
  cancelHelperLogSearchTimer();
  helperLogSearchTimer = window.setTimeout(() => {
    helperLogSearchTimer = 0;
    loadHelperLogs(true).catch(onError);
  }, HELPER_LOG_SEARCH_DEBOUNCE_MS);
}

function resetHelperLogPager(query) {
  helperLogPager.pageIndex = 0;
  helperLogPager.cursors = [""];
  helperLogPager.hasMore = false;
  helperLogPager.query = {
    date: query?.date || "",
    event: query?.event || "",
    pattern: query?.pattern || "",
    regex: query?.regex === true,
  };
  helperLogPager.searching = Boolean(helperLogPager.query.pattern);
}

function setHelperLogView(view) {
  for (const root of helperSettingsRoots()) {
    if (root.getAttribute(helperNativeSettingsPageAttribute) !== "logs") continue;
    if (view === "detail") {
      root.setAttribute("data-codex-helper-log-view", "detail");
    } else {
      root.removeAttribute("data-codex-helper-log-view");
    }
  }
}

function setHelperLogDetailEvent(event) {
  const label = event ? ` · ${event}` : "";
  for (const root of helperSettingsRoots()) {
    const node = root.querySelector("[data-codex-helper-log-event]");
    if (node instanceof HTMLElement) node.textContent = label;
  }
}

function closeHelperLogDetail() {
  setHelperLogView("list");
  setHelperLogDetailEvent("");
  for (const root of helperSettingsRoots()) {
    const body = root.querySelector("[data-codex-helper-log-detail-body]");
    if (body instanceof HTMLElement) body.textContent = "";
  }
}

function openHelperLogDetail(index) {
  if (!Number.isInteger(index) || index < 0 || index >= helperLogRecords.length) {
    throw new Error("Log record is not on the current page");
  }
  const record = helperLogRecords[index];
  for (const root of helperSettingsRoots()) {
    const body = root.querySelector("[data-codex-helper-log-detail-body]");
    if (!(body instanceof HTMLElement)) continue;
    body.replaceChildren(createLogDetailContent(record));
    body.scrollTop = 0;
  }
  setHelperLogDetailEvent(record?.event || "event");
  setHelperLogView("detail");
}

function helperLogQueryFromDom() {
  const root = helperSettingsRoots()[0];
  const dateNode = root?.querySelector("[data-codex-helper-log-date]");
  const eventNode = root?.querySelector("[data-codex-helper-log-event-filter]");
  const searchNode = root?.querySelector("[data-codex-helper-log-search]");
  const regexNode = root?.querySelector("[data-codex-helper-log-regex]");
  return {
    date: dateNode instanceof HTMLSelectElement ? dateNode.value.trim() : "",
    event: eventNode instanceof HTMLSelectElement ? eventNode.value.trim() : "",
    pattern: searchNode instanceof HTMLInputElement ? searchNode.value.trim() : "",
    regex: regexNode instanceof HTMLInputElement ? regexNode.checked : false,
  };
}

async function loadHelperLogs(resetPager = false) {
  cancelHelperLogSearchTimer();
  closeHelperLogDetail();
  if (resetPager) resetHelperLogPager(helperLogQueryFromDom());
  const seq = ++helperLogQuerySeq;
  const query = helperLogPager.query;
  const cursor = helperLogPager.cursors[helperLogPager.pageIndex] || "";
  const payload = { limit: 50 };
  if (query.date) payload.date = query.date;
  if (query.event) payload.event = query.event;
  if (cursor) payload.cursor = cursor;
  let result;
  if (query.pattern) {
    result = await bridge("/diagnostics/search", {
      ...payload,
      pattern: query.pattern,
      regex: query.regex,
    });
  } else {
    result = await bridge("/diagnostics/list", payload);
  }
  if (seq !== helperLogQuerySeq) return;
  if (result?.status === "ok") {
    helperLogPager.hasMore = result.hasMore === true;
    if (result.cursor) {
      helperLogPager.cursors[helperLogPager.pageIndex + 1] = result.cursor;
    }
  } else {
    helperLogPager.hasMore = false;
  }
  renderLogRecords(result);
}

function helperLogDateLabel(value) {
  if (!value) return "All dates";
  if (value === "unparsed") return "Unparsed";
  return value;
}

function helperLogEventFilterLabel(value) {
  const found = helperLogEventFilters.find(([key]) => key === value);
  if (found) return found[1];
  if (!value) return "All events";
  return value.endsWith(".") ? value.slice(0, -1) : value;
}

function helperLogEventCovered(event, filters) {
  return filters.some((value) => {
    if (!value) return false;
    if (value.endsWith(".")) return event.startsWith(value);
    return event === value;
  });
}

function syncLogEventOptions(records) {
  const extras = [];
  const seen = new Set(helperLogEventFilters.map(([value]) => value));
  for (const record of Array.isArray(records) ? records : []) {
    const event = typeof record?.event === "string" ? record.event.trim() : "";
    if (!event || helperLogEventCovered(event, [...seen])) continue;
    seen.add(event);
    extras.push(event);
  }
  extras.sort();
  const values = helperLogEventFilters.concat(extras.map((event) => [event, event]));
  for (const root of helperSettingsRoots()) {
    const select = root.querySelector("[data-codex-helper-log-event-filter]");
    if (!(select instanceof HTMLSelectElement)) continue;
    const current = select.value;
    if (current && !values.some(([value]) => value === current)) {
      values.push([current, helperLogEventFilterLabel(current)]);
    }
    select.textContent = "";
    for (const [value, label] of values) {
      const option = document.createElement("option");
      option.value = value;
      option.textContent = label;
      select.appendChild(option);
    }
    select.value = values.some(([value]) => value === current) ? current : "";
  }
}

function syncLogDateOptions(dates) {
  const values = [""].concat(Array.isArray(dates) ? dates.filter(Boolean) : []);
  for (const root of helperSettingsRoots()) {
    const select = root.querySelector("[data-codex-helper-log-date]");
    if (!(select instanceof HTMLSelectElement)) continue;
    const current = select.value;
    select.textContent = "";
    for (const value of values) {
      const option = document.createElement("option");
      option.value = value;
      option.textContent = helperLogDateLabel(value);
      select.appendChild(option);
    }
    select.value = values.includes(current) ? current : "";
  }
}

function helperLogRecordsFromResponse(log) {
  if (Array.isArray(log?.matches)) return log.matches;
  if (Array.isArray(log?.records)) return log.records;
  return [];
}

function helperLogRecordNoun(count, searching) {
  if (searching) {
    if (count === 1) return "match";
    return "matches";
  }
  if (count === 1) return "event";
  return "events";
}

function helperLogStatusText(log, records) {
  if (log?.status !== "ok") return resultText(log);
  if (records.length === 0) {
    if (helperLogPager.searching) return "No matching log records";
    return "No diagnostic records yet";
  }
  const noun = helperLogRecordNoun(records.length, helperLogPager.searching);
  return `${records.length} ${noun} · page ${helperLogPager.pageIndex + 1}`;
}

function renderLogRecords(log) {
  const records = helperLogRecordsFromResponse(log);
  if (log?.status === "ok") {
    syncLogDateOptions(log.dates);
    syncLogEventOptions(records);
  }
  helperLogRecords = log?.status === "ok" ? records : [];
  const statusText = helperLogStatusText(log, records);
  setHelperText("[data-codex-helper-log-status]", statusText);
  for (const root of helperSettingsRoots()) {
    const prev = root.querySelector("[data-codex-helper-log-prev]");
    const next = root.querySelector("[data-codex-helper-log-next]");
    if (prev instanceof HTMLButtonElement) {
      prev.disabled = helperLogPager.pageIndex <= 0 || log?.status !== "ok";
    }
    if (next instanceof HTMLButtonElement) {
      next.disabled = !helperLogPager.hasMore || log?.status !== "ok";
    }
  }
  const lists = helperSettingsRoots()
    .map((root) => root.querySelector("[data-codex-helper-log-list]"))
    .filter((panel) => panel instanceof HTMLElement);
  for (const list of lists) {
    list.textContent = "";
    if (log?.status !== "ok") {
      list.appendChild(createScrollEmptyMessage(statusText));
      continue;
    }
    if (records.length === 0) {
      list.appendChild(
        createScrollEmptyMessage(
          helperLogPager.searching
            ? "No matching log records."
            : "No diagnostic records yet.",
        ),
      );
      continue;
    }
    for (let index = 0; index < records.length; index += 1) {
      list.appendChild(createLogSummaryRow(records[index], index));
    }
  }
}

function logListMetricParts(record) {
  const detail = record?.detail;
  const detailObject =
    detail && typeof detail === "object" && !Array.isArray(detail) ? detail : null;
  if (!detailObject) {
    if (record?.event === "llm.request") return [];
    return [record?.preview].filter(Boolean);
  }
  const status = formatLogDetailScalar(detailObject.status);
  const duration = formatLogDuration(detailObject.durationMs);
  const model =
    formatLogDetailScalar(detailObject.model) ||
    formatLogDetailScalar(detailObject.providerId);
  const request = formatLogBytes(detailObject.requestBytes);
  const response = formatLogBytes(detailObject.responseBytes);
  const size = request && response ? `${request} → ${response}` : request || response;
  const error =
    formatLogDetailScalar(detailObject.error) ||
    formatLogDetailScalar(detailObject.message);
  const parts = [];
  if (status && status !== "0") parts.push(status);
  for (const value of [
    duration,
    model,
    size,
    formatLogDetailScalar(detailObject.sessionId),
    formatLogDetailScalar(detailObject.turnId),
    error,
  ]) {
    if (value) parts.push(value);
  }
  if (parts.length === 0) {
    const path = formatLogDetailScalar(detailObject.path);
    if (path) parts.push(path);
  }
  if (record?.event !== "llm.request" && record?.preview) parts.push(record.preview);
  return parts;
}

function createLogSummaryRow(record, index) {
  const row = document.createElement("button");
  row.type = "button";
  row.className = "codex-helper-settings-compact-row";
  row.setAttribute(helperCommandAttribute, "logs-open");
  row.setAttribute("data-codex-helper-log-index", String(index));
  const text = document.createElement("div");
  text.className = "codex-helper-settings-compact-text";
  const event = document.createElement("div");
  event.className = "codex-helper-settings-row-title";
  event.textContent = record?.event || "event";
  text.appendChild(event);
  const metrics = logListMetricParts(record).join(" · ");
  if (metrics) {
    const summary = document.createElement("div");
    summary.className = "codex-helper-settings-row-description";
    summary.textContent = ` · ${metrics}`;
    text.appendChild(summary);
  }
  row.appendChild(text);
  const time = formatLogTimestamp(record?.timestamp);
  if (time) {
    const meta = document.createElement("div");
    meta.className = "codex-helper-settings-compact-meta";
    meta.textContent = time;
    row.appendChild(meta);
  }
  return row;
}

function hasLogSection(value) {
  return value !== undefined && value !== null;
}

function formatLogCaller(caller) {
  if (typeof caller === "string") return caller;
  if (caller && typeof caller === "object") {
    return [caller.targetId, caller.href].filter(Boolean).join(" · ") || JSON.stringify(caller);
  }
  return JSON.stringify(caller);
}

function formatLogJson(value) {
  return JSON.stringify(value, null, 2);
}

function formatLogDetailScalar(value) {
  if (typeof value === "string") return value;
  if (typeof value === "number" && Number.isFinite(value)) return String(value);
  if (typeof value === "boolean") return value ? "true" : "false";
  return "";
}

function pushLogDetailField(rows, label, value) {
  const text = formatLogDetailScalar(value);
  if (text) rows.push([label, text]);
}

function logDetailHeaders(detailObject, side) {
  const headers = detailObject?.[side]?.headers;
  if (!headers || typeof headers !== "object" || Array.isArray(headers)) return null;
  return headers;
}

function logDetailHeaderValue(headers, name) {
  if (!headers) return "";
  const wanted = name.toLowerCase();
  for (const [key, value] of Object.entries(headers)) {
    if (key.toLowerCase() === wanted) return formatLogDetailScalar(value);
  }
  return "";
}

function formatLogBytes(value) {
  if (typeof value !== "number" || !Number.isFinite(value) || value < 0) return "";
  if (value < 1024) return `${Math.round(value)} B`;
  const kb = value / 1024;
  if (kb < 1024) return `${kb >= 100 ? kb.toFixed(0) : kb.toFixed(1)} KB`;
  const mb = kb / 1024;
  return `${mb >= 10 ? mb.toFixed(1) : mb.toFixed(2)} MB`;
}

function formatLogCount(value) {
  if (value === "" || value === undefined || value === null) return "";
  const number = typeof value === "number" ? value : Number(value);
  if (!Number.isFinite(number)) return "";
  if (Math.abs(number) >= 1_000_000) {
    const millions = number / 1_000_000;
    const text = Number.isInteger(millions)
      ? String(millions)
      : millions.toFixed(1).replace(/\.0$/, "");
    return `${text}M`;
  }
  return Math.round(number).toLocaleString("en-US");
}

function formatLogPair(remaining, limit, unit = "") {
  const left = formatLogCount(remaining);
  const right = formatLogCount(limit);
  if (!left && !right) return "";
  const pair = left && right ? `${left} / ${right}` : left || right;
  return unit ? `${pair} ${unit}` : pair;
}

function formatLogDuration(durationMs) {
  if (typeof durationMs !== "number" || !Number.isFinite(durationMs) || durationMs < 0) {
    return "";
  }
  if (durationMs < 1000) return `${Math.round(durationMs)} ms`;
  const seconds = durationMs / 1000;
  if (seconds < 60) return `${seconds.toFixed(1)} s`;
  const minutes = Math.floor(seconds / 60);
  const rest = Math.round(seconds - minutes * 60);
  return `${minutes}m ${rest}s`;
}

function logDetailCfColo(cfRay) {
  if (!cfRay) return "";
  const colo = cfRay.split("-").pop() || "";
  return /^[A-Z]{3}$/.test(colo) ? colo : "";
}

function logDetailFieldRows(record, detailObject) {
  const rows = [];
  pushLogDetailField(rows, "Time", formatLogTimestamp(record?.timestamp));
  if (!detailObject) return rows;
  pushLogDetailField(rows, "Path", detailObject.path);
  pushLogDetailField(rows, "Method", detailObject.method);
  pushLogDetailField(rows, "Status", detailObject.status);
  pushLogDetailField(rows, "Provider", detailObject.providerId);
  pushLogDetailField(rows, "Model", detailObject.model);
  pushLogDetailField(rows, "Duration", formatLogDuration(detailObject.durationMs));
  pushLogDetailField(rows, "Request size", formatLogBytes(detailObject.requestBytes));
  pushLogDetailField(rows, "Response size", formatLogBytes(detailObject.responseBytes));
  if (typeof detailObject.sse === "boolean") {
    pushLogDetailField(rows, "Stream", detailObject.sse ? "SSE" : "no");
  }
  pushLogDetailField(rows, "Session", detailObject.sessionId);
  pushLogDetailField(rows, "Thread", detailObject.threadId);
  pushLogDetailField(rows, "Turn", detailObject.turnId);
  pushLogDetailField(rows, "Request ID", detailObject.requestId);
  pushLogDetailField(rows, "Message", detailObject.message);
  pushLogDetailField(rows, "Error", detailObject.error);
  if (hasLogSection(detailObject.caller)) {
    rows.push(["Caller", formatLogCaller(detailObject.caller)]);
  }
  const responseHeaders = logDetailHeaders(detailObject, "response");
  pushLogDetailField(rows, "Server", logDetailHeaderValue(responseHeaders, "server"));
  pushLogDetailField(
    rows,
    "Cache",
    logDetailHeaderValue(responseHeaders, "cf-cache-status") ||
      logDetailHeaderValue(responseHeaders, "cache-control"),
  );
  pushLogDetailField(
    rows,
    "Edge",
    logDetailCfColo(logDetailHeaderValue(responseHeaders, "cf-ray")),
  );
  pushLogDetailField(
    rows,
    "Rate",
    formatLogPair(
      logDetailHeaderValue(responseHeaders, "x-ratelimit-remaining-requests"),
      logDetailHeaderValue(responseHeaders, "x-ratelimit-limit-requests"),
      "req",
    ),
  );
  pushLogDetailField(
    rows,
    "Tokens",
    formatLogPair(
      logDetailHeaderValue(responseHeaders, "x-ratelimit-remaining-tokens"),
      logDetailHeaderValue(responseHeaders, "x-ratelimit-limit-tokens"),
      "tok",
    ),
  );
  if (!detailObject.requestId) {
    pushLogDetailField(
      rows,
      "Request ID",
      logDetailHeaderValue(responseHeaders, "x-request-id"),
    );
  }
  return rows;
}

function createLogSessionRevealButton(sessionId) {
  const button = document.createElement("button");
  button.type = "button";
  button.className = "codex-helper-native-settings-log-session-reveal";
  button.setAttribute(helperCommandAttribute, "logs-reveal-session");
  button.setAttribute("data-codex-helper-session-id", sessionId);
  button.setAttribute("aria-label", "Show session file");
  button.innerHTML = `${nativeSettingsStandardIconSvg("external-link")}<span>Show session file</span>`;
  return button;
}

function createLogDetailField(label, value) {
  const row = document.createElement("div");
  row.className = "codex-helper-native-settings-log-detail-field";
  const title = document.createElement("div");
  title.className = "codex-helper-settings-row-title";
  title.textContent = label;
  const description = document.createElement("div");
  description.className = "codex-helper-settings-row-description";
  description.textContent = value;
  row.append(title, description);
  return row;
}

function createLogDetailJsonSection(value, copyCommand = "") {
  const section = document.createElement("section");
  section.className = "codex-helper-native-settings-log-json-section";
  const pre = document.createElement("pre");
  pre.className = "codex-helper-native-settings-log-json";
  pre.setAttribute("data-codex-helper-log-record", "");
  pre.textContent = formatLogJson(value);
  section.appendChild(pre);
  if (copyCommand) {
    const copy = document.createElement("button");
    copy.type = "button";
    copy.className = "codex-helper-native-settings-icon-button codex-helper-native-settings-log-json-copy";
    copy.setAttribute(helperCommandAttribute, copyCommand);
    copy.setAttribute("aria-label", "Copy record");
    copy.innerHTML = nativeSettingsStandardIconSvg("copy");
    section.appendChild(copy);
  }
  return section;
}

let helperLogSplitRatio = 2 / 7;

function applyLogDetailSplitWidth(split, left) {
  const width = split.getBoundingClientRect().width;
  if (width <= 0) return;
  const min = Math.min(160, Math.max(120, width * 0.12));
  const max = Math.max(min, width * 0.4);
  const leftWidth = Math.min(max, Math.max(min, width * helperLogSplitRatio));
  left.style.flex = `0 0 ${leftWidth}px`;
  left.style.width = `${leftWidth}px`;
}

function bindLogDetailSplitter(split, handle, left) {
  const sync = () => applyLogDetailSplitWidth(split, left);
  sync();
  if (typeof ResizeObserver === "function") {
    const observer = new ResizeObserver(() => {
      if (!split.isConnected) {
        observer.disconnect();
        return;
      }
      sync();
    });
    observer.observe(split);
  }
  handle.addEventListener("pointerdown", (event) => {
    if (event.button !== 0) return;
    event.preventDefault();
    handle.setPointerCapture(event.pointerId);
    handle.dataset.active = "true";
    const onMove = (moveEvent) => {
      const rect = split.getBoundingClientRect();
      if (rect.width <= 0) return;
      helperLogSplitRatio = Math.min(
        0.4,
        Math.max(0.12, (moveEvent.clientX - rect.left) / rect.width),
      );
      applyLogDetailSplitWidth(split, left);
    };
    const onUp = () => {
      handle.dataset.active = "false";
      handle.removeEventListener("pointermove", onMove);
      handle.removeEventListener("pointerup", onUp);
      handle.removeEventListener("pointercancel", onUp);
    };
    handle.addEventListener("pointermove", onMove);
    handle.addEventListener("pointerup", onUp);
    handle.addEventListener("pointercancel", onUp);
  });
}

function createLogDetailRecordPane(record) {
  const recordPane = createLogDetailJsonSection(
    {
      timestamp: record?.timestamp || "",
      event: record?.event || "event",
      detail: record?.detail ?? null,
    },
    "logs-copy-record",
  );
  recordPane.classList.add("codex-helper-native-settings-log-detail-record");
  return recordPane;
}

function createLogDetailContent(record) {
  const root = document.createElement("div");
  root.className = "codex-helper-native-settings-log-detail-content";
  const recordPane = createLogDetailRecordPane(record);
  if (record?.event !== "llm.request") {
    root.appendChild(recordPane);
    return root;
  }
  const meta = document.createElement("div");
  meta.className = "codex-helper-native-settings-log-detail-meta";
  const detail = record?.detail;
  const detailObject =
    detail && typeof detail === "object" && !Array.isArray(detail) ? detail : null;
  for (const [label, value] of logDetailFieldRows(record, detailObject)) {
    meta.appendChild(createLogDetailField(label, value));
  }
  const sessionId = formatLogDetailScalar(detailObject?.sessionId);
  if (sessionId) {
    meta.appendChild(createLogSessionRevealButton(sessionId));
  }
  const handle = document.createElement("div");
  handle.className = "codex-helper-native-settings-log-split-handle";
  handle.setAttribute("role", "separator");
  handle.setAttribute("aria-orientation", "vertical");
  handle.setAttribute("aria-label", "Resize log detail panes");
  handle.tabIndex = 0;
  root.append(meta, handle, recordPane);
  bindLogDetailSplitter(root, handle, meta);
  return root;
}

function formatLogTimestamp(value) {
  if (!value) return "";
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return String(value);
  const now = new Date();
  if (date.toDateString() === now.toDateString()) return date.toLocaleTimeString();
  return date.toLocaleString();
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
  if (typeof onHelperSettingsApplied === "function") {
    onHelperSettingsApplied(wasPortForwardingEnabled);
  }
}

function resultText(result) {
  if (!result) return "No response";
  if (result.status === "ok") return result.message || "Connected";
  return result.message || "Request failed";
}

function setHelperText(selector, value, status) {
  for (const root of helperSettingsRoots()) {
    const node = root.querySelector(selector);
    if (!node) continue;
    node.textContent = value;
    if (selector === "[data-codex-helper-backend]") {
      node.setAttribute("data-status", status || "error");
    }
  }
}

async function handleHelperCommand(command, source) {
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
  if (command === "logs-prev") {
    if (helperLogPager.pageIndex <= 0) return;
    helperLogPager.pageIndex -= 1;
    await loadHelperLogs(false);
    return;
  }
  if (command === "logs-next") {
    if (!helperLogPager.hasMore) return;
    if (!helperLogPager.cursors[helperLogPager.pageIndex + 1]) return;
    helperLogPager.pageIndex += 1;
    await loadHelperLogs(false);
    return;
  }
  if (command === "logs-open") {
    const index = Number(source?.getAttribute("data-codex-helper-log-index"));
    openHelperLogDetail(index);
    return;
  }
  if (command === "logs-back") {
    closeHelperLogDetail();
    return;
  }
  if (command === "logs-reveal-session") {
    const sessionId = source?.getAttribute("data-codex-helper-session-id") || "";
    const result = await bridge("/diagnostics/reveal-session", { sessionId });
    if (result?.status !== "ok") {
      throw new Error(result?.message || "Failed to reveal session file");
    }
    return;
  }
  if (command === "logs-copy-record") {
    const scoped =
      source instanceof HTMLElement
        ? source
            .closest(".codex-helper-native-settings-log-json-section")
            ?.querySelector("[data-codex-helper-log-record]")
        : null;
    const pre =
      scoped instanceof HTMLElement
        ? scoped
        : helperSettingsRoots()
            .map((root) => root.querySelector("[data-codex-helper-log-record]"))
            .find((node) => node instanceof HTMLElement);
    if (!(pre instanceof HTMLElement) || !pre.textContent) {
      throw new Error("Log record is not available to copy");
    }
    await navigator.clipboard.writeText(pre.textContent);
    return;
  }
  if (command.includes("provider")) {
    if (typeof handleProviderCommand !== "function") {
      throw new Error("Provider settings are only available in Helper Settings");
    }
    await handleProviderCommand(command, source);
    return;
  }
  if (command.startsWith("endpoint-")) {
    await handleEndpointCommand(command, source);
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
  if (typeof onHelperSettingsApplied === "function") {
    onHelperSettingsApplied(featureSettings.portForwardingEnabled);
  }
  return featureSettings;
}
