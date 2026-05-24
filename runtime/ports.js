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

function normalizeRemoteHostId(hostId) {
  return hostId && hostId !== "local" ? hostId : "";
}

function parseActiveConversationIdFromPath(pathname = window.location.pathname) {
  const match = String(pathname || "").match(
    /^\/(?:local|remote|hotkey-window\/thread)\/([^/?#]+)/,
  );
  return normalizeConversationId(match ? decodeURIComponent(match[1]) : "");
}

function normalizeConversationId(value) {
  const id = String(value || "").trim();
  if (!id) return "";
  const prefixed = id.match(/^(?:local|remote):(.+)$/);
  return prefixed ? prefixed[1] : id;
}

function activeConversationIdFromDom() {
  const thread = getActiveThreadElement();
  if (!(thread instanceof HTMLElement)) return "";
  return normalizeConversationId(
    thread.getAttribute("data-app-action-sidebar-thread-id") || "",
  );
}

function currentConversationId() {
  return parseActiveConversationIdFromPath() || activeConversationIdFromDom();
}

function reactFiberKeys(element) {
  return Object.keys(element || {}).filter(
    (key) =>
      key.startsWith("__reactFiber") ||
      key.startsWith("__reactInternalInstance"),
  );
}

function reactFiberFromElement(element) {
  if (!(element instanceof HTMLElement)) return null;
  for (const key of reactFiberKeys(element)) {
    const fiber = element[key];
    if (fiber && typeof fiber === "object") return fiber;
  }
  return null;
}

function rootFiberFromFiber(fiber) {
  let current = fiber;
  let steps = 0;
  while (current?.return && steps < 100) {
    current = current.return;
    steps += 1;
  }
  return (
    current?.stateNode?._internalRoot?.current ||
    current?.alternate?.stateNode?._internalRoot?.current ||
    current ||
    null
  );
}

function codexRootCandidateElements() {
  const candidates = [
    getActiveThreadElement(),
    document.querySelector("#root"),
    document.querySelector("#__next"),
    document.querySelector("[data-reactroot]"),
    findPinnedSummaryCard(),
  ];
  const active = getActiveThreadElement();
  for (
    let node = active;
    node instanceof HTMLElement && node !== document.body;
    node = node.parentElement
  ) {
    candidates.push(node);
  }
  if (document.body instanceof HTMLElement) candidates.push(document.body);
  return Array.from(
    new Set(candidates.filter((node) => node instanceof HTMLElement)),
  );
}

function normalizeStructuredExecutionContext(value, conversationId) {
  if (!value || typeof value !== "object") return null;
  const hostConfig = value.hostConfig;
  const expectedConversationId = normalizeConversationId(conversationId);
  const valueConversationId = normalizeConversationId(
    value.conversationId || value.threadId || "",
  );
  if (
    expectedConversationId &&
    (!valueConversationId || valueConversationId !== expectedConversationId)
  ) {
    return null;
  }
  const rawHostId = value.hostId || hostConfig?.id || "";
  const hostId = normalizeRemoteHostId(rawHostId);
  const cwd =
    typeof value.cwd === "string" && value.cwd.startsWith("/")
      ? value.cwd
      : "";
  const kind = typeof hostConfig?.kind === "string" ? hostConfig.kind : "";
  if (!cwd) return null;
  if (kind === "local" || rawHostId === "local") {
    return {
      hostId: "",
      path: cwd,
      threadId: expectedConversationId || valueConversationId,
      kind: kind || "local",
      isRemote: false,
      source: "codex-structured-context",
    };
  }
  if (!hostId) return null;
  return {
    hostId,
    path: cwd,
    threadId: expectedConversationId || valueConversationId,
    kind,
    isRemote: true,
    source: "codex-structured-context",
  };
}

function normalizeStructuredExecutionTarget(value, conversationId) {
  const context = normalizeStructuredExecutionContext(value, conversationId);
  return context?.isRemote ? context : null;
}

function findStructuredExecutionContextInValue(
  value,
  conversationId,
  seen = new Set(),
  depth = 0,
) {
  if (!value || typeof value !== "object") return null;
  if (seen.has(value)) return null;
  seen.add(value);

  const direct = normalizeStructuredExecutionContext(value, conversationId);
  if (direct) return direct;
  if (depth >= 5) return null;

  const values = Array.isArray(value)
    ? value.slice(0, 25)
    : Object.values(value).slice(0, 50);
  for (const child of values) {
    const context = findStructuredExecutionContextInValue(
      child,
      conversationId,
      seen,
      depth + 1,
    );
    if (context) return context;
  }
  return null;
}

function findStructuredExecutionTargetInValue(
  value,
  conversationId,
  seen = new Set(),
  depth = 0,
) {
  if (!value || typeof value !== "object") return null;
  if (seen.has(value)) return null;
  seen.add(value);

  const direct = normalizeStructuredExecutionTarget(value, conversationId);
  if (direct) return direct;
  if (depth >= 5) return null;

  const values = Array.isArray(value)
    ? value.slice(0, 25)
    : Object.values(value).slice(0, 50);
  for (const child of values) {
    const context = findStructuredExecutionTargetInValue(
      child,
      conversationId,
      seen,
      depth + 1,
    );
    if (context) return context;
  }
  return null;
}

function readStructuredContextFromFiberRoot(rootFiber, conversationId) {
  if (!rootFiber || typeof rootFiber !== "object") return null;
  const stack = [rootFiber];
  const seenFibers = new Set();
  let visited = 0;
  while (stack.length && visited < 2000) {
    const fiber = stack.pop();
    if (!fiber || typeof fiber !== "object" || seenFibers.has(fiber)) continue;
    seenFibers.add(fiber);
    visited += 1;

    for (const key of [
      "memoizedProps",
      "pendingProps",
      "memoizedState",
      "updateQueue",
    ]) {
      const context = findStructuredExecutionContextInValue(
        fiber[key],
        conversationId,
      );
      if (context) return context;
    }

    if (fiber.sibling) stack.push(fiber.sibling);
    if (fiber.child) stack.push(fiber.child);
  }
  return null;
}

function readStructuredRemoteContextFromFiberRoot(rootFiber, conversationId) {
  const context = readStructuredContextFromFiberRoot(rootFiber, conversationId);
  return context?.isRemote ? context : null;
}

function codexRootFiber() {
  const explicitRoot = window.__codexRoot?._internalRoot?.current;
  if (explicitRoot) return explicitRoot;
  for (const element of codexRootCandidateElements()) {
    for (
      let node = element;
      node instanceof HTMLElement && node !== document.body.parentElement;
      node = node.parentElement
    ) {
      const fiber = reactFiberFromElement(node);
      const root = rootFiberFromFiber(fiber);
      if (root) return root;
    }
  }
  return null;
}

function structuredForwardingContextFromCodex() {
  const threadId = currentConversationId();
  if (!threadId) return null;
  return readStructuredContextFromFiberRoot(codexRootFiber(), threadId);
}

function threadKindIsLocal(kind) {
  return /\blocal\b/i.test(kind || "");
}

function sessionContextFromDom() {
  const thread = getActiveThreadElement();
  const projectPath = selectedAttributeValue(
    "data-app-action-sidebar-project-list-id",
  );
  const path = projectPath.startsWith("/") ? projectPath : "";
  const legacy = remoteContextFromDom();
  if (!(thread instanceof HTMLElement)) {
    return {
      ...legacy,
      threadId: "",
      kind: "",
      isRemote: Boolean(legacy.hostId && legacy.path),
    };
  }
  const kind = thread.getAttribute("data-app-action-sidebar-thread-kind") || "";
  const rawThreadHostId =
    thread.getAttribute("data-app-action-sidebar-thread-host-id") || "";
  const threadHostId = normalizeRemoteHostId(rawThreadHostId);
  const hostId =
    threadHostId ||
    (!rawThreadHostId && kind && !threadKindIsLocal(kind)
      ? legacy.hostId
      : "");
  return {
    hostId,
    path: path || legacy.path,
    threadId: thread.getAttribute("data-app-action-sidebar-thread-id") || "",
    kind,
    isRemote: Boolean(hostId && (path || legacy.path)),
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
    context.threadId || "",
    remotePort,
    localPortKey,
  ].join(":");
}

function portSessionKey(context) {
  return [
    context.hostId || "",
    context.path || context.remotePath || "",
    context.threadId || "",
  ].join("|");
}

function portTimestamp() {
  return Date.now();
}

function hasActiveTerminal() {
  if (findTerminalPortScanRoots().length > 0) return true;
  return Boolean(document.querySelector(".xterm, [class*='xterm' i]"));
}

function pinnedSummaryHasRow(label) {
  const card = findPinnedSummaryCard();
  if (!(card instanceof HTMLElement)) return false;
  return Array.from(card.querySelectorAll("[class*='summary-panel-row']")).some(
    (row) =>
      row instanceof HTMLElement && isVisibleElement(row) && exactText(row, label),
  );
}

function pinnedSummaryShowsRemote() {
  return pinnedSummaryHasRow("Remote");
}

function pinnedSummaryShowsLocal() {
  return pinnedSummaryHasRow("Local");
}

function remoteForwardingContextIsReady(context) {
  return Boolean(
    context?.isRemote && context.hostId && context.path && context.threadId,
  );
}

function rememberRemoteForwardingContext(context) {
  if (!remoteForwardingContextIsReady(context)) return context;
  resolvedRemoteForwardingContext = { ...context };
  return context;
}

function currentRemoteForwardingContext() {
  const structuredContext = structuredForwardingContextFromCodex();
  if (
    structuredContext &&
    structuredContext.source === "codex-structured-context"
  ) {
    if (remoteForwardingContextIsReady(structuredContext)) {
      return rememberRemoteForwardingContext(structuredContext);
    }
    resolvedRemoteForwardingContext = null;
    return structuredContext;
  }
  const context = sessionContextFromDom();
  if (remoteForwardingContextIsReady(context)) {
    return rememberRemoteForwardingContext(context);
  }
  if (pinnedSummaryShowsLocal()) {
    resolvedRemoteForwardingContext = null;
    return context;
  }
  if (pinnedSummaryShowsRemote() && resolvedRemoteForwardingContext) {
    return {
      ...resolvedRemoteForwardingContext,
      threadId:
        context.threadId || resolvedRemoteForwardingContext.threadId || "",
      kind: context.kind || resolvedRemoteForwardingContext.kind || "remote",
      isRemote: true,
    };
  }
  return context;
}

function hasRemoteForwardingContext() {
  const context = currentRemoteForwardingContext();
  if (remoteForwardingContextIsReady(context)) return true;
  if (context?.source === "codex-structured-context") return false;
  if (pinnedSummaryShowsLocal()) {
    resolvedRemoteForwardingContext = null;
    return false;
  }
  return pinnedSummaryShowsRemote();
}

function hasPortForwardingContext() {
  return hasRemoteForwardingContext();
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
  return portSessionKey(currentRemoteForwardingContext());
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
    syncRemoteSessionPorts().catch((error) => {
      handleRemotePortDiscoveryFailure(error);
    });
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

function terminalPortEvidenceSet(text = terminalTextForPortScan()) {
  return new Set(parseWebPortsFromText(text).map((candidate) => candidate.port));
}

function portsWithSessionEvidence(ports, evidence) {
  if (!(evidence instanceof Set) || evidence.size === 0) return [];
  return ports.filter((port) => {
    const remotePort = Number(port?.remotePort);
    return Number.isInteger(remotePort) && evidence.has(remotePort);
  });
}

function localPortForDetectedPort(remotePort) {
  return featureSettings.portSameLocalPort ? remotePort : 0;
}

function shouldAutoForwardDetectedPort(entry, context) {
  return Boolean(
    featureSettings.portAutoForwardWeb && context.hostId && entry.remotePort,
  );
}

function scanTerminalWebPorts() {
  if (!featureSettings.portForwardingEnabled || !hasRemoteForwardingContext()) {
    return;
  }
  if (pruneDetectedPortsForSessionChange()) {
    refreshPortsPanelIfVisible();
  }
  const context = currentRemoteForwardingContext();
  if (!remoteForwardingContextIsReady(context)) return;
  const text = terminalTextForPortScan();
  let changed = false;
  for (const candidate of parseWebPortsFromText(text)) {
    const localPort = localPortForDetectedPort(candidate.port);
    const key = portKey(context, candidate.port, localPort);
    const existing = detectedPorts.get(key);
    if (
      existing?.status === "starting" ||
      existing?.status === "forwarding" ||
      existing?.status === "active"
    ) {
      continue;
    }
    const entry = {
      key,
      hostId: context.hostId,
      remotePath: context.path,
      threadId: context.threadId,
      remotePort: candidate.port,
      localPort,
      url: candidate.url,
      status: "detected",
      lastSeenAt: portTimestamp(),
    };
    detectedPorts.set(key, entry);
    changed = true;
  }
  if (changed) refreshPortsPanelIfVisible();
}

async function forwardDetectedPort(entry, source = "auto") {
  entry.status = "starting";
  const result = await bridge("/ports/forward", {
    hostId: entry.hostId,
    remotePath: entry.remotePath,
    threadId: entry.threadId || "",
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
  entry.message = "";
  entry.lastForwardedAt = portTimestamp();
  if (Number.isInteger(result.localPort) && result.localPort > 0) {
    entry.localPort = result.localPort;
  }
  showHelperToast(
    `Forwarded remote port ${entry.remotePort} to localhost:${entry.localPort}`,
  );
  refreshPortsPanelIfVisible();
}

function samePortSession(entry, context) {
  return (
    (entry.hostId || "") === (context.hostId || "") &&
    (entry.remotePath || "") === (context.path || "") &&
    (entry.threadId || "") === (context.threadId || "")
  );
}

function markPortEntrySeen(entry, port) {
  const now = portTimestamp();
  const previousStatus = entry.status;
  const previousMessage = entry.message || "";
  entry.lastSeenAt = now;
  entry.lastDiscoveryOkAt = now;
  entry.failureCount = 0;
  entry.command = port?.command || entry.command || "";
  if (entry.status === "unreachable" || entry.status === "stopped") {
    entry.status = entry.id ? "active" : "detected";
    entry.message = "";
  }
  return previousStatus !== entry.status || previousMessage !== (entry.message || "");
}

function activePortIds(activePorts) {
  return new Set(
    activePorts
      .map((port) => port?.id || "")
      .filter((id) => typeof id === "string" && id.length > 0),
  );
}

function preferredForwardedTunnel(ports) {
  const manual = ports.find((port) => port.source !== "auto");
  if (manual) return manual;
  return (
    ports.find(
      (port) => Number(port.localPort) === Number(port.remotePort),
    ) ||
    ports[0] ||
    null
  );
}

function activeForwardedPortMap(activePorts, context) {
  const grouped = new Map();
  for (const port of activePortsForCurrentSession(activePorts, context)) {
    const remotePort = Number(port.remotePort);
    if (!Number.isInteger(remotePort) || remotePort < 1) continue;
    const ports = grouped.get(remotePort) || [];
    ports.push(port);
    grouped.set(remotePort, ports);
  }
  const activePortsByRemotePort = new Map();
  for (const [remotePort, ports] of grouped.entries()) {
    const preferred = preferredForwardedTunnel(ports);
    if (preferred) activePortsByRemotePort.set(remotePort, preferred);
  }
  return activePortsByRemotePort;
}

function reconcileForwardedTunnelList(activePorts, context) {
  const activeIds = activePortIds(activePorts);
  let changed = false;
  for (const entry of detectedPorts.values()) {
    if (!samePortSession(entry, context)) continue;
    if (!entry.id || activeIds.has(entry.id)) continue;
    delete entry.id;
    delete entry.localUrl;
    if (entry.status !== "unreachable") {
      entry.status = "stopped";
      entry.message = "Forwarding stopped";
    }
    changed = true;
  }
  return changed;
}

function markRemotePortDiscoverySucceeded(context) {
  const now = portTimestamp();
  const previous = portDiscoveryStates.get(portSessionKey(context));
  portDiscoveryStates.set(portSessionKey(context), {
    status: "ok",
    lastDiscoveryOkAt: now,
    lastDiscoveryFailedAt: 0,
    message: "",
  });
  return previous?.status !== "ok" || Boolean(previous?.message);
}

function markCurrentSessionPortsUnreachable(context, message) {
  const now = portTimestamp();
  let changed = false;
  portDiscoveryStates.set(portSessionKey(context), {
    status: "unreachable",
    lastDiscoveryOkAt:
      portDiscoveryStates.get(portSessionKey(context))?.lastDiscoveryOkAt || 0,
    lastDiscoveryFailedAt: now,
    message,
  });
  for (const entry of detectedPorts.values()) {
    if (!samePortSession(entry, context)) continue;
    entry.status = "unreachable";
    entry.message = message;
    entry.lastDiscoveryFailedAt = now;
    entry.failureCount = (entry.failureCount || 0) + 1;
    changed = true;
  }
  return changed;
}

function handleRemotePortDiscoveryFailure(error) {
  const message = error?.message || String(error);
  const context = currentRemoteForwardingContext();
  markCurrentSessionPortsUnreachable(context, message);
  logDiagnostic("ports_remote_discovery_failed", { error: message });
  scheduleRefreshPortsPanel();
}

function pruneStaleDetectedPorts(context, discoveredRemotePorts) {
  let changed = false;
  for (const [key, entry] of Array.from(detectedPorts.entries())) {
    if (!samePortSession(entry, context)) continue;
    if (discoveredRemotePorts.has(entry.remotePort)) continue;
    entry.status = "missing";
    entry.message = "Remote service stopped";
    detectedPorts.delete(key);
    changed = true;
    if (entry.id) {
      bridge("/ports/stop", { id: entry.id }).catch((error) => {
        logDiagnostic("ports_stale_stop_failed", {
          error: error?.message || String(error),
          remotePort: entry.remotePort,
        });
      });
    }
  }
  return changed;
}

function discoveredRemotePortSet(ports) {
  return new Set(
    ports
      .map((port) => Number(port.remotePort))
      .filter((port) => Number.isInteger(port) && port > 0),
  );
}

async function stopStaleForwardedTunnels(
  context,
  discoveredRemotePorts,
  activePorts = [],
) {
  let changed = false;
  for (const port of activePortsForCurrentSession(activePorts, context)) {
    const remotePort = Number(port.remotePort);
    const id = typeof port.id === "string" ? port.id : "";
    if (!id || discoveredRemotePorts.has(remotePort)) continue;
    await bridge("/ports/stop", { id });
    changed = true;
  }
  return changed;
}

async function stopDuplicateForwardedTunnels(context, activePorts) {
  const grouped = new Map();
  for (const port of activePortsForCurrentSession(activePorts, context)) {
    const remotePort = Number(port.remotePort);
    const id = typeof port.id === "string" ? port.id : "";
    if (!id || !Number.isInteger(remotePort) || remotePort < 1) continue;
    const ports = grouped.get(remotePort) || [];
    ports.push(port);
    grouped.set(remotePort, ports);
  }

  let changed = false;
  for (const ports of grouped.values()) {
    if (ports.length <= 1 || !ports.some((port) => port.source === "auto")) {
      continue;
    }
    const keep = preferredForwardedTunnel(ports);
    for (const port of ports) {
      if (port === keep || port.source !== "auto") continue;
      await bridge("/ports/stop", { id: port.id });
      changed = true;
    }
  }
  return changed;
}

async function stopForwardedTunnelsOutsideSession(context, activePorts) {
  let changed = false;
  for (const port of activePorts) {
    const id = typeof port.id === "string" ? port.id : "";
    if (!id) continue;
    if (!samePortSession(port, context)) {
      await bridge("/ports/stop", { id });
      changed = true;
    }
  }
  return changed;
}

function detectedEntryForRemotePort(context, remotePort) {
  for (const entry of detectedPorts.values()) {
    if (!samePortSession(entry, context)) continue;
    if (Number(entry.remotePort) === remotePort) return entry;
  }
  return null;
}

function setDetectedPortEntryKey(entry, key) {
  if (entry.key === key) return false;
  if (entry.key) detectedPorts.delete(entry.key);
  entry.key = key;
  detectedPorts.set(key, entry);
  return true;
}

function updateDetectedPortFromForwardedTunnel(entry, port) {
  let changed = false;
  const nextId = typeof port.id === "string" ? port.id : "";
  const nextLocalUrl = typeof port.localUrl === "string" ? port.localUrl : "";
  const nextLocalPort = Number(port.localPort);
  if (nextId && entry.id !== nextId) {
    entry.id = nextId;
    changed = true;
  }
  if (nextLocalUrl && entry.localUrl !== nextLocalUrl) {
    entry.localUrl = nextLocalUrl;
    changed = true;
  }
  if (
    Number.isInteger(nextLocalPort) &&
    nextLocalPort > 0 &&
    entry.localPort !== nextLocalPort
  ) {
    entry.localPort = nextLocalPort;
    changed = true;
  }
  if (entry.status !== "active") {
    entry.status = "active";
    changed = true;
  }
  if (entry.message) {
    entry.message = "";
    changed = true;
  }
  entry.lastForwardedAt = entry.lastForwardedAt || portTimestamp();
  return changed;
}

function reconcileDiscoveredRemotePorts(context, ports, activePorts = []) {
  let changed = markRemotePortDiscoverySucceeded(context);
  const discoveredRemotePorts = discoveredRemotePortSet(ports);
  const activeForwardedPorts = activeForwardedPortMap(activePorts, context);
  changed = pruneStaleDetectedPorts(context, discoveredRemotePorts) || changed;
  for (const port of ports) {
    const remotePort = Number(port.remotePort);
    if (!Number.isInteger(remotePort) || remotePort < 1 || remotePort > 65535)
      continue;
    const activeForward = activeForwardedPorts.get(remotePort);
    const activeLocalPort = Number(activeForward?.localPort);
    const localPort =
      Number.isInteger(activeLocalPort) && activeLocalPort > 0
        ? activeLocalPort
        : localPortForDetectedPort(remotePort);
    const key = portKey(context, remotePort, localPort);
    const existing =
      detectedPorts.get(key) || detectedEntryForRemotePort(context, remotePort);
    if (existing) {
      changed = setDetectedPortEntryKey(existing, key) || changed;
      existing.localPort = localPort;
      changed = markPortEntrySeen(existing, port) || changed;
      if (activeForward) {
        changed =
          updateDetectedPortFromForwardedTunnel(existing, activeForward) ||
          changed;
      }
      if (
        existing.status === "detected" &&
        !activeForward &&
        featureSettings.portForwardingEnabled &&
        shouldAutoForwardDetectedPort(existing, context)
      ) {
        changed = true;
        forwardDetectedPort(existing).catch((error) => {
          existing.status = "failed";
          existing.message = error?.message || String(error);
          logDiagnostic("ports_auto_forward_failed", {
            error: existing.message,
            remotePort: existing.remotePort,
          });
          refreshPortsPanelIfVisible();
        });
      }
      continue;
    }
    const now = portTimestamp();
    const entry = {
      key,
      hostId: context.hostId,
      remotePath: context.path,
      threadId: context.threadId,
      remotePort,
      localPort,
      status: activeForward ? "active" : "detected",
      command: port.command || "",
      lastSeenAt: now,
      lastDiscoveryOkAt: now,
      failureCount: 0,
    };
    if (activeForward) {
      updateDetectedPortFromForwardedTunnel(entry, activeForward);
    }
    detectedPorts.set(key, entry);
    changed = true;
    if (
      !activeForward &&
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

async function syncRemoteSessionPorts() {
  if (remotePortSyncInFlight) return;
  remotePortSyncInFlight = true;
  try {
    await syncRemoteSessionPortsOnce();
  } finally {
    remotePortSyncInFlight = false;
  }
}

function contextSessionKey(context) {
  return remoteForwardingContextIsReady(context) ? portSessionKey(context) : "";
}

function remoteForwardingContextChanged(initialSessionKey) {
  const currentSessionKey = contextSessionKey(currentRemoteForwardingContext());
  return !currentSessionKey || currentSessionKey !== initialSessionKey;
}

async function syncRemoteSessionPortsOnce() {
  if (!featureSettings.portForwardingEnabled) return;
  const context = await resolveRemoteForwardingContext();
  if (!remoteForwardingContextIsReady(context)) return;
  const initialSessionKey = contextSessionKey(context);
  const result = await bridge("/ports/discover", {
    hostId: context.hostId,
    remotePath: context.path,
    threadId: context.threadId || "",
  });
  if (remoteForwardingContextChanged(initialSessionKey)) return;
  if (result?.status !== "ok") {
    throw new Error(result?.message || "Port discovery failed");
  }
  const discoveredPorts = Array.isArray(result.ports) ? result.ports : [];
  const ports = portsWithSessionEvidence(
    discoveredPorts,
    terminalPortEvidenceSet(),
  );
  const activeResult = await bridge("/ports/list");
  if (remoteForwardingContextChanged(initialSessionKey)) return;
  const activePorts =
    activeResult?.status === "ok" && Array.isArray(activeResult.ports)
      ? activeResult.ports
      : [];
  if (remoteForwardingContextChanged(initialSessionKey)) return;
  const stoppedOutsideSession =
    await stopForwardedTunnelsOutsideSession(context, activePorts);
  if (remoteForwardingContextChanged(initialSessionKey)) return;
  const stopped = await stopStaleForwardedTunnels(
    context,
    discoveredRemotePortSet(ports),
    activePorts,
  );
  if (remoteForwardingContextChanged(initialSessionKey)) return;
  const stoppedDuplicates = await stopDuplicateForwardedTunnels(
    context,
    activePorts,
  );
  if (remoteForwardingContextChanged(initialSessionKey)) return;
  reconcileDiscoveredRemotePorts(context, ports, activePorts);
  if (stopped || stoppedDuplicates || stoppedOutsideSession)
    scheduleRefreshPortsPanel();
}

async function resolveRemoteForwardingContext() {
  const context = currentRemoteForwardingContext();
  if (remoteForwardingContextIsReady(context)) return context;
  if (!pinnedSummaryShowsRemote()) return context;
  const result = await bridge("/zed-remote/fallback-request", {});
  const request = result?.status === "ok" ? result.request : null;
  const hostId = normalizeRemoteHostId(request?.hostId || "");
  const path =
    typeof request?.path === "string" && request.path.startsWith("/")
      ? request.path
      : "";
  if (!hostId || !path) return context;
  return rememberRemoteForwardingContext({
    hostId,
    path,
    threadId: context.threadId || "",
    kind: context.kind || "remote",
    isRemote: true,
  });
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

function removePortsPinnedSummaryUi() {
  document.querySelectorAll(`[${helperPortsPinnedAttribute}]`).forEach((node) => {
    node.remove();
  });
  pinnedPortsLastSnapshot = "";
  portsSurface = "none";
  stopPortScanLoop();
}

async function stopAllManagedPortForwards() {
  if (managedPortStopInFlight) return;
  managedPortStopInFlight = true;
  try {
    const result = await bridge("/ports/list");
    const ports =
      result?.status === "ok" && Array.isArray(result.ports) ? result.ports : [];
    for (const port of ports) {
      const id = typeof port.id === "string" ? port.id : "";
      if (!id) continue;
      const stopResult = await bridge("/ports/stop", { id });
      if (stopResult?.status !== "ok") {
        logDiagnostic("ports_disable_stop_failed", {
          id,
          result: stopResult,
        });
      }
    }
  } finally {
    managedPortStopInFlight = false;
  }
}

function handlePortForwardingDisabled() {
  removePortsPinnedSummaryUi();
  detectedPorts.clear();
  portDiscoveryStates.clear();
  resolvedRemoteForwardingContext = null;
  stopAllManagedPortForwards().catch((error) => {
    logDiagnostic("ports_disable_stop_failed", {
      error: error?.message || String(error),
    });
  });
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
  const row = sources.querySelector("[class*='summary-panel-row']");
  if (row instanceof HTMLElement) return row;
  const fallback = fallbackSummaryRowTemplate(sources);
  if (fallback instanceof HTMLElement) return fallback;
  return Array.from(host.querySelectorAll("[class*='summary-panel-row']")).find(
    (candidate) =>
      candidate instanceof HTMLElement &&
      !candidate.closest(`[${helperPortsPinnedAttribute}]`) &&
      !candidate.querySelector("[class*='summary-panel-row-accessory']"),
  );
}

function fallbackSummaryRowTemplate(sources) {
  const list = findPortForwardListContainer(sources);
  const first = list?.firstElementChild;
  return first instanceof HTMLElement ? first : null;
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

function findPortForwardDisclosureContent(section) {
  const list = findPortForwardListContainer(section);
  if (!(list instanceof HTMLElement)) return null;
  const container = list.parentElement;
  if (
    container instanceof HTMLElement &&
    String(container.className || "").includes("overflow-hidden")
  ) {
    return container;
  }
  return list;
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
  else row.textContent = text;
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

function portStatusLabel(entry) {
  const status = entry.status || "detected";
  if (status === "active") return "active";
  if (status === "starting" || status === "forwarding") return "starting";
  if (status === "failed") return "failed";
  if (status === "unreachable") return "unreachable";
  if (status === "missing" || status === "stopped") return "stopped";
  return "detected";
}

function portLocalPortLabel(entry) {
  if (Number.isInteger(entry.localPort) && entry.localPort > 0) {
    return String(entry.localPort);
  }
  if (entry.localUrl) return String(entry.localUrl).replace(/.*:/, "");
  return "auto";
}

function portRowLabel(entry) {
  const remotePort = entry.remotePort || "—";
  const localPort = portLocalPortLabel(entry);
  const status = portStatusLabel(entry);
  return `${remotePort} → ${localPort} · ${status}`;
}

function emptyPortForwardLabel(rows, context = sessionContextFromDom()) {
  const state = portDiscoveryStates.get(portSessionKey(context));
  if (state?.status === "unreachable") {
    return "Remote unavailable";
  }
  if (rows.length === 0) {
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
  const context = currentRemoteForwardingContext();
  const rows = mergedPortRows(activePorts, context);
  list.replaceChildren();

  if (rows.length === 0) {
    list.appendChild(
      createSummaryRowFromTemplate(
        templateRow,
        emptyPortForwardLabel(rows, context),
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

function buildPinnedPortsSnapshot(activePorts, context = sessionContextFromDom()) {
  const state = portDiscoveryStates.get(portSessionKey(context));
  return JSON.stringify({
    discovery: {
      status: state?.status || "",
      message: state?.message || "",
      lastDiscoveryOkAt: state?.lastDiscoveryOkAt || 0,
      lastDiscoveryFailedAt: state?.lastDiscoveryFailedAt || 0,
    },
    rows: mergedPortRows(activePorts, context).map((row) => ({
      id: row.id || row.key || "",
      remotePort: row.remotePort || 0,
      localPort: row.localPort || 0,
      status: row.status || "detected",
      message: row.message || "",
      failureCount: row.failureCount || 0,
    })),
  });
}

function portForwardPinnedSectionHasRows(section) {
  const list = findPortForwardListContainer(section);
  return Boolean(
    list instanceof HTMLElement &&
    list.querySelector("[class*='summary-panel-row']"),
  );
}

function setPortForwardPinnedIconDirection(section, collapsed) {
  const icon = section.querySelector("header button svg");
  if (!(icon instanceof SVGElement)) return;
  icon.style.transform = collapsed ? "rotate(-90deg)" : "";
}

function setPortForwardPinnedDisclosure(section, collapsed) {
  const button = section.querySelector("header button");
  const content = findPortForwardDisclosureContent(section);
  if (button instanceof HTMLElement) {
    button.setAttribute("aria-expanded", collapsed ? "false" : "true");
  }
  if (content instanceof HTMLElement) {
    content.hidden = collapsed;
    content.style.display = collapsed ? "none" : "";
  }
  section.setAttribute(
    "data-codex-helper-ports-collapsed",
    collapsed ? "true" : "false",
  );
  setPortForwardPinnedIconDirection(section, collapsed);
}

function togglePortForwardPinnedDisclosure(section) {
  const collapsed =
    section.getAttribute("data-codex-helper-ports-collapsed") !== "true";
  setPortForwardPinnedDisclosure(section, collapsed);
}

function installPortForwardPinnedDisclosure(section) {
  const button = section.querySelector("header button");
  if (!(button instanceof HTMLElement)) return false;
  if (
    section.getAttribute("data-codex-helper-ports-disclosure-installed") ===
    "true"
  ) {
    setPortForwardPinnedDisclosure(
      section,
      section.getAttribute("data-codex-helper-ports-collapsed") === "true",
    );
    return true;
  }
  section.setAttribute("data-codex-helper-ports-disclosure-installed", "true");
  button.addEventListener(
    "click",
    (event) => {
      event.preventDefault();
      event.stopPropagation();
      event.stopImmediatePropagation();
      togglePortForwardPinnedDisclosure(section);
    },
    true,
  );
  setPortForwardPinnedDisclosure(
    section,
    section.getAttribute("data-codex-helper-ports-collapsed") === "true",
  );
  return true;
}

function ensurePortsPinnedSection(card) {
  const host = findPinnedSummarySectionsHost(card);
  const existing = host.querySelector(`[${helperPortsPinnedAttribute}]`);
  if (existing instanceof HTMLElement) {
    if (existing.querySelector("[class*='summary-panel-row']")) {
      installPortForwardPinnedDisclosure(existing);
      return existing;
    }
    existing.remove();
  }

  const sources = findSourcesSummarySection(host);
  if (!(sources instanceof HTMLElement)) return null;

  const section = sources.cloneNode(true);
  section.setAttribute(helperPortsPinnedAttribute, "true");
  setSummarySectionTitle(section, "Port Forward");
  const list = findPortForwardListContainer(section);
  if (list instanceof HTMLElement) list.replaceChildren();
  installPortForwardPinnedDisclosure(section);
  sources.insertAdjacentElement("afterend", section);
  return section;
}

function portForwardingUiAvailable() {
  return featureSettings.portForwardingEnabled && hasRemoteForwardingContext();
}

function renderPortsPinnedSummary(activePorts) {
  if (!portForwardingUiAvailable()) {
    removePortsPinnedSummaryUi();
    return false;
  }
  const card = findPinnedSummaryCard();
  if (!(card instanceof HTMLElement)) return false;

  const host = findPinnedSummarySectionsHost(card);
  const context = currentRemoteForwardingContext();
  const snapshot = buildPinnedPortsSnapshot(activePorts, context);

  const section = ensurePortsPinnedSection(card);
  if (!(section instanceof HTMLElement)) return false;
  if (
    snapshot === pinnedPortsLastSnapshot && portForwardPinnedSectionHasRows(section)
  ) {
    return true;
  }

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
  if (!portForwardingUiAvailable()) {
    if (featureSettingsLoaded) stopAllManagedPortForwards().catch((error) => {
      logDiagnostic("ports_stop_all_failed", {
        error: error?.message || String(error),
      });
    });
    removePortsPinnedSummaryUi();
    return;
  }
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
  syncRemoteSessionPorts().catch((error) => {
    handleRemotePortDiscoveryFailure(error);
  });
  scheduleRefreshPortsPanel();
}

function activePortsForCurrentSession(activePorts, context) {
  return activePorts.filter((entry) => samePortSession(entry, context));
}

function mergedPortRows(activePorts, context = sessionContextFromDom()) {
  const rows = new Map();
  for (const port of activePortsForCurrentSession(activePorts, context)) {
    rows.set(
      port.id ||
      `${port.hostId}:${port.remotePath}:${port.threadId || ""}:${port.remotePort}:${port.localPort}`,
      port,
    );
  }
  for (const entry of detectedPorts.values()) {
    if (!samePortSession(entry, context)) continue;
    const id = entry.id || entry.key;
    const current = rows.get(id);
    if (current) {
      rows.set(id, {
        ...current,
        ...entry,
        localPort: entry.localPort || current.localPort,
        localUrl: entry.localUrl || current.localUrl,
        status: entry.status || current.status,
        message: entry.message || current.message || "",
      });
    } else {
      rows.set(id, entry);
    }
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
  if (!portForwardingUiAvailable()) {
    removePortsPinnedSummaryUi();
    return;
  }
  if (!findPinnedSummaryCard() && !portsPanelIsVisible()) return;
  const context = currentRemoteForwardingContext();
  let result = await bridge("/ports/list");
  let activePorts =
    result?.status === "ok" && Array.isArray(result.ports) ? result.ports : [];
  const duplicatesStopped = await stopDuplicateForwardedTunnels(context, activePorts);
  if (duplicatesStopped) {
    result = await bridge("/ports/list");
    activePorts =
      result?.status === "ok" && Array.isArray(result.ports) ? result.ports : [];
  }
  const changed = reconcileForwardedTunnelList(activePorts, context);
  if (findPinnedSummaryCard()) {
    renderPortsPinnedSummary(activePortsForCurrentSession(activePorts, context));
  }
  if (changed || duplicatesStopped) scheduleRefreshPortsPanel();
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
    for (const [key, entry] of Array.from(detectedPorts.entries())) {
      if (entry.id !== id) continue;
      entry.status = "stopped";
      detectedPorts.delete(key);
    }
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
      { hostId: entry.hostId, path: entry.remotePath, threadId: entry.threadId },
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
