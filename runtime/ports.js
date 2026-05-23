// Port detection, pinned-summary UI, and forwarding commands
// biome-ignore-all lint/correctness/noUnusedVariables: called from bootstrap.js and settings.js in the bundled runtime
function selectedAttributeValue(attribute) {
  const nodes = Array.from(document.querySelectorAll(`[${attribute}]`));
  const visible = nodes
    .filter((node) => node instanceof HTMLElement)
    .map((node) => ({
      node,
      rect: node.getBoundingClientRect(),
      value: node.getAttribute(attribute) || "",
      text: textOf(node),
    }))
    .filter((item) => item.rect.width > 0 && item.rect.height > 0);
  const active = visible.find((item) => {
    const aria = item.node.getAttribute("aria-selected");
    const state = item.node.getAttribute("data-state");
    const className = String(item.node.className || "");
    return (
      aria === "true" ||
      state === "active" ||
      /selected|active/.test(className)
    );
  });
  return active?.value || visible[0]?.value || "";
}

function remoteContextFromDom() {
  const hostId = selectedAttributeValue(
    "data-app-action-sidebar-thread-host-id",
  );
  const projectPath = selectedAttributeValue(
    "data-app-action-sidebar-project-list-id",
  );
  return {
    hostId: hostId && hostId !== "local" ? hostId : "",
    path: projectPath.startsWith("/") ? projectPath : "",
  };
}

function getActiveThreadElement() {
  const active = document.querySelector(
    '[data-app-action-sidebar-thread-active="true"]',
  );
  if (active instanceof HTMLElement) return active;
  return document.querySelector(
    '[data-app-action-sidebar-thread-id][aria-current="page"]',
  );
}

function sessionContextFromDom() {
  const thread = getActiveThreadElement();
  const projectPath = selectedAttributeValue(
    "data-app-action-sidebar-project-list-id",
  );
  const path = projectPath.startsWith("/") ? projectPath : "";
  if (!(thread instanceof HTMLElement)) {
    const legacy = remoteContextFromDom();
    return {
      ...legacy,
      threadId: "",
      kind: "",
      isRemote: Boolean(legacy.hostId),
    };
  }
  const hostId =
    thread.getAttribute("data-app-action-sidebar-thread-host-id") || "";
  return {
    hostId: hostId && hostId !== "local" ? hostId : "",
    path,
    threadId: thread.getAttribute("data-app-action-sidebar-thread-id") || "",
    kind: thread.getAttribute("data-app-action-sidebar-thread-kind") || "",
    isRemote: Boolean(hostId && hostId !== "local"),
  };
}

function parseWebPortsFromText(text) {
  const ports = new Map();
  const patterns = [
    /\bhttps?:\/\/(?:localhost|127\.0\.0\.1|0\.0\.0\.0|\[::1\]):([0-9]{1,5})(?:[/?#][^\s"'<>]*)?/gi,
    /\b(?:https?:\/\/)?(?:localhost|127\.0\.0\.1|0\.0\.0\.0|\[::1\]):([0-9]{1,5})\b/gi,
  ];
  for (const pattern of patterns) {
    for (const match of text.matchAll(pattern)) {
      const port = Number(match[1]);
      if (!Number.isInteger(port) || port < 1 || port > 65535) continue;
      if (ports.has(port)) continue;
      const raw = match[0];
      const url = /^https?:\/\//i.test(raw)
        ? raw
        : `http://127.0.0.1:${port}`;
      ports.set(port, { port, url });
    }
  }
  return Array.from(ports.values());
}

function portKey(context, remotePort, localPort) {
  const localPortKey =
    Number.isInteger(localPort) && localPort > 0 ? localPort : "custom";
  return [
    context.hostId || "unknown",
    context.path || "",
    remotePort,
    localPortKey,
  ].join(":");
}

function hasActiveTerminal() {
  if (findTerminalPortScanRoots().length > 0) return true;
  return Boolean(document.querySelector(".xterm, [class*='xterm' i]"));
}

function hasRemoteForwardingContext() {
  return sessionContextFromDom().isRemote;
}

function hasPortForwardingContext() {
  const ctx = sessionContextFromDom();
  if (ctx.isRemote) return true;
  if (detectedPorts.size > 0) return true;
  return hasActiveTerminal();
}

function isPortForwardingOperational() {
  return featureSettings.portForwardingEnabled && hasPortForwardingContext();
}

function portsPanelIsVisible() {
  const pinned = document.querySelector(`[${helperPortsPinnedAttribute}]`);
  return pinned instanceof HTMLElement && pinned.isConnected;
}

function schedulePortScan() {
  if (pendingPortScan) return;
  pendingPortScan = window.setTimeout(() => {
    pendingPortScan = 0;
    scanTerminalWebPorts();
  }, 500);
}

function currentPortScanSessionKey() {
  const ctx = sessionContextFromDom();
  return `${ctx.hostId}|${ctx.path}|${ctx.threadId}`;
}

function pruneDetectedPortsForSessionChange() {
  const sessionKey = currentPortScanSessionKey();
  if (!sessionKey || sessionKey === lastPortScanSessionKey) return false;
  lastPortScanSessionKey = sessionKey;
  if (detectedPorts.size === 0) return false;
  detectedPorts.clear();
  return true;
}

function ensurePortScanLoop() {
  if (portScanIntervalId) return;
  portScanIntervalId = window.setInterval(() => {
    if (!findPinnedSummaryCard() && !portsPanelIsVisible()) {
      stopPortScanLoop();
      return;
    }
    scanTerminalWebPorts();
  }, 2000);
}

function stopPortScanLoop() {
  if (!portScanIntervalId) return;
  clearInterval(portScanIntervalId);
  portScanIntervalId = 0;
}

function findTerminalPortScanRoots() {
  const selector = [
    ".xterm",
    "[class*='xterm' i]",
    "[data-testid*='terminal' i]",
    "[data-test*='terminal' i]",
    "[aria-label*='terminal' i]",
    "[data-panel-id*='terminal' i]",
    "[data-codex-terminal]",
  ].join(",");
  const roots = [];
  for (const node of document.querySelectorAll(selector)) {
    if (!(node instanceof HTMLElement)) continue;
    if (!isVisibleElement(node)) continue;
    if (node.closest(`[${helperPageAttribute}], [${helperToastAttribute}]`)) {
      continue;
    }
    if (roots.some((root) => root.contains(node))) continue;
    for (let index = roots.length - 1; index >= 0; index -= 1) {
      if (node.contains(roots[index])) roots.splice(index, 1);
    }
    roots.push(node);
  }
  return roots;
}

function appendTerminalTextFromRoot(root, parts, seen) {
  if (!(root instanceof HTMLElement)) return;
  const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT);
  while (walker.nextNode()) {
    const parent = walker.currentNode.parentElement;
    if (parent?.closest(`[${helperPageAttribute}], [${helperToastAttribute}]`)) {
      continue;
    }
    const text = (walker.currentNode.nodeValue || "").trim();
    if (!text || seen.has(text)) continue;
    seen.add(text);
    parts.push(text);
  }
}

function terminalTextForPortScan() {
  const parts = [];
  const seen = new Set();
  for (const root of findTerminalPortScanRoots()) {
    appendTerminalTextFromRoot(root, parts, seen);
  }
  for (const xterm of document.querySelectorAll(".xterm, [class*='xterm' i]")) {
    if (xterm.closest(`[${helperPageAttribute}], [${helperToastAttribute}]`)) {
      continue;
    }
    appendTerminalTextFromRoot(xterm, parts, seen);
  }
  return parts.join("\n");
}

function localPortForDetectedPort(remotePort) {
  return featureSettings.portSameLocalPort ? remotePort : 0;
}

function shouldAutoForwardDetectedPort(entry, context) {
  return Boolean(
    featureSettings.portAutoForwardWeb && context.hostId && entry.localPort,
  );
}

function scanTerminalWebPorts() {
  if (pruneDetectedPortsForSessionChange()) {
    refreshPortsPanelIfVisible();
  }
  const context = sessionContextFromDom();
  const text = terminalTextForPortScan();
  let changed = false;
  for (const candidate of parseWebPortsFromText(text)) {
    const localPort = localPortForDetectedPort(candidate.port);
    const key = portKey(context, candidate.port, localPort);
    const existing = detectedPorts.get(key);
    if (existing?.status === "forwarding" || existing?.status === "active")
      continue;
    const entry = {
      key,
      hostId: context.hostId,
      remotePath: context.path,
      remotePort: candidate.port,
      localPort,
      url: candidate.url,
      status: "detected",
    };
    detectedPorts.set(key, entry);
    changed = true;
    if (
      featureSettings.portForwardingEnabled &&
      shouldAutoForwardDetectedPort(entry, context)
    ) {
      forwardDetectedPort(entry).catch((error) => {
        entry.status = "failed";
        entry.message = error?.message || String(error);
        logDiagnostic("ports_auto_forward_failed", {
          error: entry.message,
          remotePort: entry.remotePort,
        });
        refreshPortsPanelIfVisible();
      });
    }
  }
  if (changed) refreshPortsPanelIfVisible();
}

async function forwardDetectedPort(entry, source = "auto") {
  entry.status = "forwarding";
  const result = await bridge("/ports/forward", {
    hostId: entry.hostId,
    remotePath: entry.remotePath,
    remotePort: entry.remotePort,
    localPort: entry.localPort,
    source,
  });
  if (result?.status !== "ok") {
    entry.status = "failed";
    entry.message = result?.message || "Port forwarding failed";
    logDiagnostic("ports_auto_forward_failed", {
      result,
      remotePort: entry.remotePort,
    });
    refreshPortsPanelIfVisible();
    return;
  }
  entry.status = "active";
  entry.id = result.id;
  entry.localUrl = result.localUrl;
  showHelperToast(
    `Forwarded remote port ${entry.remotePort} to localhost:${entry.localPort}`,
  );
  refreshPortsPanelIfVisible();
}

function removeLegacyPortsBottomPanelUi() {
  for (const attr of [
    "data-codex-helper-ports-home-card",
    "data-codex-helper-ports-entry",
    "data-codex-helper-ports-panel",
  ]) {
    for (const node of document.querySelectorAll(`[${attr}]`)) {
      node.remove();
    }
  }
  document
    .querySelectorAll("[data-codex-helper-ports-native-hidden]")
    .forEach((node) => {
      if (!(node instanceof HTMLElement)) return;
      node.style.display = "";
      node.removeAttribute("data-codex-helper-ports-native-hidden");
    });
  if (portsSurface === "bottom") portsSurface = "none";
}

function findPinnedSummarySectionHeading(label) {
  return Array.from(
    document.querySelectorAll(
      "div, section, span, button, h2, h3, h4, p, li, label",
    ),
  ).find(
    (node) =>
      node instanceof HTMLElement &&
      isVisibleElement(node) &&
      exactText(node, label),
  );
}

function pinnedSummaryCardLooksLikePanel(node) {
  const style = window.getComputedStyle(node);
  const radius =
    parseFloat(style.borderTopLeftRadius) ||
    parseFloat(style.borderRadius) ||
    0;
  const bg = style.backgroundColor;
  const hasBackground =
    Boolean(bg) && bg !== "transparent" && !bg.endsWith(", 0)");
  return (
    radius >= 6 ||
    style.boxShadow !== "none" ||
    hasBackground ||
    style.borderWidth !== "0px"
  );
}

function findPinnedSummaryCard() {
  if (pinnedSummaryCardRef?.isConnected) {
    const cachedText = textOf(pinnedSummaryCardRef);
    if (
      cachedText.includes("Environment") &&
      cachedText.includes("Sources")
    ) {
      return pinnedSummaryCardRef;
    }
    pinnedSummaryCardRef = null;
  }

  const env = findPinnedSummarySectionHeading("Environment");
  const sources = findPinnedSummarySectionHeading("Sources");
  const anchor = env || sources;
  if (!(anchor instanceof HTMLElement)) return null;

  let best = null;
  let bestArea = Infinity;
  for (
    let node = anchor.parentElement;
    node instanceof HTMLElement && node !== document.body;
    node = node.parentElement
  ) {
    const combined = textOf(node);
    if (!combined.includes("Environment") || !combined.includes("Sources")) {
      continue;
    }
    const rect = node.getBoundingClientRect();
    if (rect.width < 160 || rect.height < 72) continue;
    if (rect.width > window.innerWidth * 0.45) continue;
    if (!pinnedSummaryCardLooksLikePanel(node) && best) continue;
    const area = rect.width * rect.height;
    if (area < bestArea) {
      bestArea = area;
      best = node;
    }
  }
  if (best instanceof HTMLElement) pinnedSummaryCardRef = best;
  return best;
}

function findPinnedSummarySectionsHost(card) {
  return (
    card.querySelector(
      ".flex.h-fit.max-h-full.min-h-0.flex-col.gap-3.overflow-y-auto",
    ) ||
    Array.from(card.children).find(
      (child) =>
        child instanceof HTMLElement &&
        child.querySelector("section") &&
        (child.textContent || "").includes("Sources"),
    ) ||
    card
  );
}

function findSourcesSummarySection(host) {
  return Array.from(host.querySelectorAll("section")).find(
    (section) =>
      section instanceof HTMLElement &&
      !section.hasAttribute(helperPortsPinnedAttribute) &&
      (section.textContent || "").includes("Sources"),
  );
}

function findSummaryRowTemplate(host) {
  const sources = findSourcesSummarySection(host);
  if (!(sources instanceof HTMLElement)) return null;
  return sources.querySelector("[class*='summary-panel-row']");
}

function findSummaryIconRowTemplate(host, label) {
  return Array.from(host.querySelectorAll("[class*='summary-panel-row']")).find(
    (row) => row instanceof HTMLElement && exactText(row, label),
  );
}

function findPortForwardListContainer(section) {
  return (
    section.querySelector("div.flex.flex-col.gap-px.px-4") ||
    section.querySelector("div.relative.z-0.overflow-hidden > div")
  );
}

function setSummarySectionTitle(section, title) {
  const titleNode =
    section.querySelector("header button span") ||
    section.querySelector("header span");
  if (titleNode) titleNode.textContent = title;
}

function setSummaryRowText(row, text) {
  const label =
    row.querySelector("span.flex.min-w-0.flex-1 span") ||
    row.querySelector("span.flex.min-w-0.flex-1") ||
    row.querySelector("span");
  if (label) label.textContent = text;
}

function createSummaryRowFromTemplate(templateRow, text, iconRow) {
  const row = templateRow.cloneNode(true);
  row.removeAttribute("id");
  if (iconRow instanceof HTMLElement) {
    const icon = iconRow.querySelector("svg");
    const target = row.querySelector("svg");
    if (icon && target) target.replaceWith(icon.cloneNode(true));
  }
  setSummaryRowText(row, text);
  return row;
}

function portRowLabel(entry) {
  const remotePort = entry.remotePort || "—";
  const localPort =
    entry.localPort ||
    (entry.localUrl ? String(entry.localUrl).replace(/.*:/, "") : "—");
  return `${remotePort} → ${localPort}`;
}

function emptyPortForwardLabel(rows) {
  if (rows.length === 0 && detectedPorts.size === 0) {
    return "No ports detected yet";
  }
  return "No forwarded ports";
}

function populatePortForwardList(section, activePorts, host) {
  const list = findPortForwardListContainer(section);
  const templateRow = findSummaryRowTemplate(host);
  if (!(list instanceof HTMLElement) || !(templateRow instanceof HTMLElement)) {
    return false;
  }

  const portIconRow =
    findSummaryIconRowTemplate(host, "Remote") || templateRow;
  const rows = mergedPortRows(activePorts);
  list.replaceChildren();

  if (rows.length === 0) {
    list.appendChild(
      createSummaryRowFromTemplate(
        templateRow,
        emptyPortForwardLabel(rows),
        templateRow,
      ),
    );
    return true;
  }

  for (const entry of rows) {
    list.appendChild(
      createSummaryRowFromTemplate(
        templateRow,
        portRowLabel(entry),
        portIconRow,
      ),
    );
  }
  return true;
}

function buildPinnedPortsSnapshot(activePorts) {
  return JSON.stringify({
    rows: mergedPortRows(activePorts).map((row) => ({
      id: row.id || row.key || "",
      remotePort: row.remotePort || 0,
      localPort: row.localPort || 0,
      status: row.status || "detected",
    })),
  });
}

function ensurePortsPinnedSection(card) {
  const host = findPinnedSummarySectionsHost(card);
  const existing = host.querySelector(`[${helperPortsPinnedAttribute}]`);
  if (existing instanceof HTMLElement) {
    if (existing.querySelector("[class*='summary-panel-row']")) return existing;
    existing.remove();
  }

  const sources = findSourcesSummarySection(host);
  if (!(sources instanceof HTMLElement)) return null;

  const section = sources.cloneNode(true);
  section.setAttribute(helperPortsPinnedAttribute, "true");
  setSummarySectionTitle(section, "Port Forward");
  const list = findPortForwardListContainer(section);
  if (list instanceof HTMLElement) list.replaceChildren();
  sources.insertAdjacentElement("afterend", section);
  return section;
}

function renderPortsPinnedSummary(activePorts) {
  const card = findPinnedSummaryCard();
  if (!(card instanceof HTMLElement)) return false;

  const host = findPinnedSummarySectionsHost(card);
  const snapshot = buildPinnedPortsSnapshot(activePorts);
  if (snapshot === pinnedPortsLastSnapshot) return true;

  const section = ensurePortsPinnedSection(card);
  if (!(section instanceof HTMLElement)) return false;

  setSummarySectionTitle(section, "Port Forward");
  if (!populatePortForwardList(section, activePorts, host)) return false;

  pinnedPortsLastSnapshot = snapshot;
  portsSurface = "pinned";
  return true;
}

function maintainPortsPanel() {
  if (maintainPortsPanelTimer) return;
  maintainPortsPanelTimer = window.setTimeout(() => {
    maintainPortsPanelTimer = 0;
    maintainPortsPanelNow();
  }, 150);
}

function maintainPortsPanelNow() {
  removeLegacyPortsBottomPanelUi();
  const card = findPinnedSummaryCard();
  if (!(card instanceof HTMLElement)) {
    if (!pinnedSummaryHideTimer) {
      pinnedSummaryHideTimer = window.setTimeout(() => {
        pinnedSummaryHideTimer = 0;
        if (findPinnedSummaryCard()) return;
        document
          .querySelectorAll(`[${helperPortsPinnedAttribute}]`)
          .forEach((node) => {
            node.remove();
          });
        pinnedPortsLastSnapshot = "";
        pinnedSummaryCardRef = null;
        portsSurface = "none";
        stopPortScanLoop();
      }, 800);
    }
    return;
  }

  if (pinnedSummaryHideTimer) {
    clearTimeout(pinnedSummaryHideTimer);
    pinnedSummaryHideTimer = 0;
  }

  ensurePortScanLoop();
  schedulePortScan();
  scheduleRefreshPortsPanel();
}

function mergedPortRows(activePorts) {
  const rows = new Map();
  for (const port of activePorts) {
    rows.set(
      port.id ||
      `${port.hostId}:${port.remotePath}:${port.remotePort}:${port.localPort}`,
      port,
    );
  }
  for (const entry of detectedPorts.values()) {
    const id = entry.id || entry.key;
    if (!rows.has(id)) rows.set(id, entry);
  }
  return Array.from(rows.values()).sort(
    (a, b) => (a.remotePort || 0) - (b.remotePort || 0),
  );
}

function portsUnavailableMessage() {
  if (!featureSettings.portForwardingEnabled) {
    return "Enable port forwarding in Helper Settings.";
  }
  if (!hasRemoteForwardingContext()) {
    return "Connect to a remote session to forward ports.";
  }
  return "No ports detected yet.";
}

function scheduleRefreshPortsPanel() {
  if (refreshPortsPanelTimer) return;
  refreshPortsPanelTimer = window.setTimeout(() => {
    refreshPortsPanelTimer = 0;
    refreshPortsPanelIfVisible().catch(() => { });
  }, 300);
}

async function refreshPortsPanelIfVisible() {
  if (!findPinnedSummaryCard() && !portsPanelIsVisible()) return;
  const result = await bridge("/ports/list");
  const activePorts =
    result?.status === "ok" && Array.isArray(result.ports) ? result.ports : [];
  if (findPinnedSummaryCard()) {
    renderPortsPinnedSummary(activePorts);
  }
}

async function handlePortCommand(button) {
  const command = button.getAttribute(helperPortCommandAttribute) || "";
  if (
    (command === "forward" || command === "manual") &&
    !isPortForwardingOperational()
  ) {
    throw new Error(portsUnavailableMessage());
  }
  const id = button.getAttribute("data-codex-helper-port-id") || "";
  const localUrl = button.getAttribute("data-codex-helper-port-url") || "";
  if (command === "open" && localUrl) {
    window.open(localUrl, "_blank", "noopener,noreferrer");
    return;
  }
  if (command === "copy" && localUrl) {
    await navigator.clipboard.writeText(localUrl);
    showHelperToast("Copied port URL");
    return;
  }
  if (command === "stop" && id) {
    const result = await bridge("/ports/stop", { id });
    if (result?.status !== "ok")
      throw new Error(result?.message || "Stop failed");
    await refreshPortsPanelIfVisible();
    return;
  }
  if (command === "forward") {
    const entry = detectedPorts.get(id);
    if (!entry) return;
    const localPort =
      entry.localPort ||
      Number(window.prompt("Local port", String(entry.remotePort)));
    if (!Number.isInteger(localPort) || localPort < 1 || localPort > 65535)
      return;
    const previousKey = entry.key;
    entry.localPort = localPort;
    entry.key = portKey(
      { hostId: entry.hostId, path: entry.remotePath },
      entry.remotePort,
      localPort,
    );
    if (previousKey !== entry.key) {
      detectedPorts.delete(previousKey);
      detectedPorts.set(entry.key, entry);
    }
    await forwardDetectedPort(entry, "manual");
  }
}
