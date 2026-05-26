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
  suppressedPortMappings.clear();
  if (detectedPorts.size === 0) return false;
  detectedPorts.clear();
  return true;
}

function ensurePortScanLoop() {
  if (portScanIntervalId) return;
  portScanIntervalId = window.setInterval(() => {
    if (
      !featureSettings.portForwardingEnabled ||
      !hasRemoteForwardingContext() ||
      !helperWindowIsPortOwner()
    ) {
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

function portSuppressionKey(context, remotePort) {
  const port = Number(remotePort);
  if (!Number.isInteger(port) || port < 1 || port > 65535) return "";
  return `${portSessionKey(context)}|${port}`;
}

function suppressPortMapping(entry) {
  if (!entry) return false;
  const context = {
    hostId: entry.hostId || "",
    path: entry.remotePath || "",
    threadId: entry.threadId || "",
  };
  const key = portSuppressionKey(context, entry.remotePort);
  if (!key) return false;
  suppressedPortMappings.add(key);
  return true;
}

function unsuppressPortMapping(entry) {
  if (!entry) return false;
  const context = {
    hostId: entry.hostId || "",
    path: entry.remotePath || "",
    threadId: entry.threadId || "",
  };
  const key = portSuppressionKey(context, entry.remotePort);
  return Boolean(key && suppressedPortMappings.delete(key));
}

function portMappingIsSuppressed(context, remotePort) {
  const key = portSuppressionKey(context, remotePort);
  return Boolean(key && suppressedPortMappings.has(key));
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
  if (
    !featureSettings.portForwardingEnabled ||
    !hasRemoteForwardingContext() ||
    !helperWindowIsPortOwner()
  ) {
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
    if (portMappingIsSuppressed(context, candidate.port)) continue;
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

function reconcileDiscoveredRemotePorts(
  context,
  ports,
  activePorts = [],
  discoveredRemotePorts = discoveredRemotePortSet(ports),
) {
  let changed = markRemotePortDiscoverySucceeded(context);
  const canOwnPorts = helperWindowIsPortOwner();
  const activeForwardedPorts = activeForwardedPortMap(activePorts, context);
  changed =
    (canOwnPorts && pruneStaleDetectedPorts(context, discoveredRemotePorts)) ||
    changed;
  for (const port of ports) {
    const remotePort = Number(port.remotePort);
    if (!Number.isInteger(remotePort) || remotePort < 1 || remotePort > 65535)
      continue;
    const activeForward = activeForwardedPorts.get(remotePort);
    if (portMappingIsSuppressed(context, remotePort)) {
      if (activeForward?.id) {
        bridge("/ports/stop", { id: activeForward.id }).catch((error) => {
          logDiagnostic("ports_suppressed_stop_failed", {
            error: error?.message || String(error),
            remotePort,
          });
        });
      }
      continue;
    }
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
        helperWindowIsPortOwner() &&
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
      helperWindowIsPortOwner() &&
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
  if (!helperWindowIsPortOwner()) return;
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
  const discoveredRemotePorts = discoveredRemotePortSet(discoveredPorts);
  const evidencedPorts = portsWithSessionEvidence(
    discoveredPorts,
    terminalPortEvidenceSet(),
  );
  const activeResult = await bridge("/ports/list");
  if (remoteForwardingContextChanged(initialSessionKey)) return;
  const activePorts =
    activeResult?.status === "ok" && Array.isArray(activeResult.ports)
      ? activeResult.ports
      : [];
  if (!helperWindowIsPortOwner()) return;
  const stopped = await stopStaleForwardedTunnels(
    context,
    discoveredRemotePorts,
    activePorts,
  );
  if (remoteForwardingContextChanged(initialSessionKey)) return;
  const stoppedDuplicates = await stopDuplicateForwardedTunnels(
    context,
    activePorts,
  );
  if (remoteForwardingContextChanged(initialSessionKey)) return;
  reconcileDiscoveredRemotePorts(
    context,
    evidencedPorts,
    activePorts,
    discoveredRemotePorts,
  );
  if (stopped || stoppedDuplicates) scheduleRefreshPortsPanel();
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
  suppressedPortMappings.clear();
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

function findEnvironmentSummarySection(host) {
  return Array.from(host.querySelectorAll("section")).find(
    (section) =>
      section instanceof HTMLElement &&
      !section.hasAttribute(helperPortsPinnedAttribute) &&
      (section.textContent || "").includes("Environment"),
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
  const environment = findEnvironmentSummarySection(host);
  if (environment instanceof HTMLElement) {
    const commitRow = Array.from(
      environment.querySelectorAll("[class*='summary-panel-row']"),
    ).find(
      (row) =>
        row instanceof HTMLElement && textOf(row).includes("Commit"),
    );
    if (commitRow instanceof HTMLElement) return commitRow;

    const actionRow = Array.from(
      environment.querySelectorAll("[class*='summary-panel-row']"),
    ).find(
      (row) =>
        row instanceof HTMLElement &&
        row.querySelector("[class*='summary-panel-row-accessory']"),
    );
    if (actionRow instanceof HTMLElement) return actionRow;

    const row = environment.querySelector("[class*='summary-panel-row']");
    if (row instanceof HTMLElement) return row;
  }

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
      candidate.querySelector("[class*='summary-panel-row-accessory']"),
  );
}

function fallbackSummaryRowTemplate(sources) {
  const list = findPortForwardListContainer(sources);
  const first = list?.firstElementChild;
  return first instanceof HTMLElement ? first : null;
}

function findSummaryIconRowTemplate(host, label) {
  const environment = findEnvironmentSummarySection(host);
  if (environment instanceof HTMLElement) {
    const row = Array.from(
      environment.querySelectorAll("[class*='summary-panel-row']"),
    ).find((candidate) => exactText(candidate, label));
    if (row instanceof HTMLElement) return row;
  }
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

function summaryRowLabelNode(row) {
  return (
    row.querySelector("span.flex.min-w-0.flex-1 span") ||
    row.querySelector("span.flex.min-w-0.flex-1") ||
    row.querySelector("span")
  );
}

function setSummaryRowText(row, text) {
  const label = summaryRowLabelNode(row);
  if (label) label.textContent = text;
  else row.textContent = text;
}

function cloneSummaryPanelIcon(host, selectors) {
  const roots = [
    findPinnedSummaryCard(),
    findEnvironmentSummarySection(host),
    host,
    document,
  ].filter((node) => node instanceof HTMLElement);
  for (const root of roots) {
    for (const selector of selectors) {
      const svg = root.querySelector(selector);
      if (svg instanceof SVGElement) {
        const cloned = svg.cloneNode(true);
        cloned.removeAttribute("id");
        return cloned;
      }
    }
  }
  return null;
}

function findNativeSummaryMenuItemTemplate(labelFragment) {
  return Array.from(document.querySelectorAll('[role="menuitem"]')).find(
    (item) =>
      item instanceof HTMLElement &&
      textOf(item).includes(labelFragment),
  );
}

function replaceMenuItemLabel(item, label) {
  const spans = Array.from(item.querySelectorAll("span"));
  for (let index = spans.length - 1; index >= 0; index -= 1) {
    const span = spans[index];
    if (span.querySelector("svg")) continue;
    if ((span.textContent || "").trim()) {
      span.textContent = label;
      return;
    }
  }
  item.appendChild(document.createTextNode(label));
}

function setMenuItemCheckedState(item, checked) {
  item.setAttribute("aria-checked", checked ? "true" : "false");
  const checkSlot = item.querySelector(".codex-helper-port-menu-check");
  if (checkSlot instanceof HTMLElement) {
    checkSlot.replaceChildren();
    if (checked) checkSlot.appendChild(createPortActionIcon("check"));
    return;
  }
  let checkIcon = item.querySelector(
    'svg[class*="check" i], svg.lucide-check, svg.lucide-check-check',
  );
  if (!(checkIcon instanceof SVGElement)) {
    const svgs = item.querySelectorAll("svg");
    checkIcon = svgs.length ? svgs[svgs.length - 1] : null;
  }
  if (checkIcon instanceof SVGElement) {
    checkIcon.style.visibility = checked ? "visible" : "hidden";
    checkIcon.style.opacity = checked ? "1" : "0";
  }
}

function createPortActionIcon(name) {
  const domPatterns = {
    copy: ['svg[class*="copy" i]', "svg.lucide-copy"],
    open: [
      'svg[class*="external-link" i]',
      "svg.lucide-external-link",
      'svg[class*="globe" i]',
    ],
    settings: ['svg[class*="settings" i]', "svg.lucide-settings"],
    check: ['svg[class*="check" i]', "svg.lucide-check"],
  };
  if (domPatterns[name]) {
    const cloned = cloneSummaryPanelIcon(document, domPatterns[name]);
    if (cloned instanceof SVGElement) return cloned;
  }

  const svg = document.createElementNS("http://www.w3.org/2000/svg", "svg");
  svg.setAttribute("viewBox", "0 0 24 24");
  svg.setAttribute("fill", "none");
  svg.setAttribute("stroke", "currentColor");
  svg.setAttribute("stroke-width", "2");
  svg.setAttribute("stroke-linecap", "round");
  svg.setAttribute("stroke-linejoin", "round");
  const paths = (() => {
    if (name === "trash") {
      return [
        "M3 6h18",
        "M8 6V4h8v2",
        "M19 6l-1 14H6L5 6",
        "M10 11v6",
        "M14 11v6",
      ];
    }
    if (name === "plus") {
      return ["M12 5v14", "M5 12h14"];
    }
    if (name === "ellipsis") {
      return ["M12 12h.01", "M19 12h.01", "M5 12h.01"];
    }
    if (name === "copy") {
      return ["M15 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12", "M15 2v5h5"];
    }
    if (name === "check") {
      return ["M20 6 9 17l-5-5"];
    }
    return [
      "M12 20h9",
      "M16.5 3.5a2.1 2.1 0 0 1 3 3L7 19l-4 1 1-4 12.5-12.5z",
    ];
  })();
  for (const value of paths) {
    const path = document.createElementNS("http://www.w3.org/2000/svg", "path");
    path.setAttribute("d", value);
    svg.appendChild(path);
  }
  svg.setAttribute("aria-hidden", "true");
  return svg;
}

function createPortForwardIcon(host = document) {
  const cloned = cloneSummaryPanelIcon(host, [
    'svg[class*="wifi" i]',
    "svg.lucide-wifi",
    'svg[class*="wi-fi" i]',
  ]);
  if (cloned instanceof SVGElement) {
    cloned.classList.add("codex-helper-port-row-leading-icon");
    return cloned;
  }

  const svg = document.createElementNS("http://www.w3.org/2000/svg", "svg");
  svg.classList.add("codex-helper-port-row-leading-icon");
  svg.setAttribute("viewBox", "0 0 24 24");
  svg.setAttribute("fill", "none");
  svg.setAttribute("stroke", "currentColor");
  svg.setAttribute("stroke-width", "2");
  svg.setAttribute("stroke-linecap", "round");
  svg.setAttribute("stroke-linejoin", "round");
  svg.setAttribute("aria-hidden", "true");
  for (const value of [
    "M12 20h.01",
    "M2 8.82a15 15 0 0 1 20 0",
    "M5 12.86a10 10 0 0 1 14 0",
    "M8.5 16.429a5 5 0 0 1 7 0",
  ]) {
    const path = document.createElementNS("http://www.w3.org/2000/svg", "path");
    path.setAttribute("d", value);
    svg.appendChild(path);
  }
  return svg;
}

function localUrlForPortEntry(entry) {
  const localUrl = String(entry.localUrl || "").trim();
  if (localUrl) return localUrl;
  if (Number.isInteger(entry.localPort) && entry.localPort > 0) {
    return `http://127.0.0.1:${entry.localPort}`;
  }
  return "";
}

function localForwardedUrlIsAllowed(value) {
  try {
    const url = new URL(value);
    const host = url.hostname.toLowerCase().replace(/^\[|\]$/g, "");
    return (
      (url.protocol === "http:" || url.protocol === "https:") &&
      Boolean(url.port) &&
      ["localhost", "127.0.0.1", "::1", "0.0.0.0"].includes(host)
    );
  } catch {
    return false;
  }
}

function setPortCommandDataset(element, entry) {
  element.setAttribute("data-codex-helper-port-id", entry.id || "");
  element.setAttribute("data-codex-helper-port-key", entry.key || "");
  element.setAttribute("data-codex-helper-port-host-id", entry.hostId || "");
  element.setAttribute(
    "data-codex-helper-port-remote-path",
    entry.remotePath || "",
  );
  element.setAttribute("data-codex-helper-port-thread-id", entry.threadId || "");
  element.setAttribute(
    "data-codex-helper-port-remote-port",
    String(entry.remotePort || ""),
  );
  element.setAttribute(
    "data-codex-helper-port-local-port",
    String(entry.localPort || ""),
  );
  element.setAttribute(
    "data-codex-helper-port-url",
    localUrlForPortEntry(entry),
  );
}

function createPortRowActionButton(action, label, iconName, entry) {
  const button = document.createElement("button");
  button.type = "button";
  button.className = "codex-helper-port-row-action";
  button.setAttribute(helperPortCommandAttribute, "show-mapping-menu");
  button.setAttribute("data-codex-helper-port-action", action);
  button.setAttribute("aria-label", label);
  button.title = label;
  setPortCommandDataset(button, entry);
  button.appendChild(createPortActionIcon(iconName));
  return button;
}

function summaryActionRowTemplate(host) {
  return findSummaryRowTemplate(host);
}

function summaryAccessoryTemplate(host) {
  const row = summaryActionRowTemplate(host);
  const accessory = row?.querySelector("[class*='summary-panel-row-accessory']");
  return accessory instanceof HTMLElement ? accessory : null;
}

function summaryActionButtonTemplate(host) {
  const row = summaryActionRowTemplate(host);
  if (!(row instanceof HTMLElement)) return null;
  const accessory =
    row.querySelector("[class*='summary-panel-row-accessory']") ||
    row.querySelector("button[aria-label*='actions' i]") ||
    row.querySelector("button");
  const button =
    accessory instanceof HTMLButtonElement
      ? accessory
      : accessory?.querySelector?.("button");
  return button instanceof HTMLButtonElement ? button : null;
}

function clearPortForwardRowActions(row) {
  const accessory = row.querySelector("[class*='summary-panel-row-accessory']");
  if (accessory instanceof HTMLElement) {
    accessory.querySelector("button")?.remove();
    return;
  }
  row.querySelector(".codex-helper-port-row-actions")?.remove();
}

function createPortRowActionsAccessory(button, host) {
  const template = summaryAccessoryTemplate(host);
  if (template instanceof HTMLElement) {
    const accessory = template.cloneNode(true);
    accessory.querySelector("button")?.remove();
    accessory.appendChild(button);
    return accessory;
  }
  const actions = document.createElement("span");
  actions.className = "codex-helper-port-row-actions";
  actions.appendChild(button);
  return actions;
}

function createNativePortRowActionButton(host, entry) {
  const template = summaryActionButtonTemplate(host);
  if (!(template instanceof HTMLButtonElement)) return null;
  const nativeButton = template.cloneNode(true);
  if (!(nativeButton instanceof HTMLButtonElement)) return null;
  nativeButton.removeAttribute("id");
  nativeButton.removeAttribute("data-state");
  nativeButton.removeAttribute("aria-expanded");
  nativeButton.type = "button";
  nativeButton.setAttribute(helperPortCommandAttribute, "show-mapping-menu");
  nativeButton.setAttribute("data-codex-helper-port-action", "menu");
  nativeButton.setAttribute("aria-label", "Port mapping actions");
  nativeButton.title = "Port mapping actions";
  setPortCommandDataset(nativeButton, entry);
  return nativeButton;
}

function installPortForwardRowActions(row, entry, host = document) {
  row.setAttribute("data-codex-helper-port-row", "true");
  setPortCommandDataset(row, entry);
  const button =
    createNativePortRowActionButton(host, entry) ||
    createPortRowActionButton(
      "menu",
      "Port mapping actions",
      "ellipsis",
      entry,
    );
  const accessory = row.querySelector("[class*='summary-panel-row-accessory']");
  if (accessory instanceof HTMLElement) {
    accessory.replaceChildren(button);
    return;
  }
  row.appendChild(createPortRowActionsAccessory(button, host));
}

function createCurrentSessionPortCommandSource(
  context = currentRemoteForwardingContext(),
) {
  if (!remoteForwardingContextIsReady(context)) return null;
  const source = document.createElement("span");
  source.setAttribute("data-codex-helper-port-host-id", context.hostId || "");
  source.setAttribute("data-codex-helper-port-remote-path", context.path || "");
  source.setAttribute("data-codex-helper-port-thread-id", context.threadId || "");
  source.setAttribute("data-codex-helper-port-remote-port", "");
  source.setAttribute("data-codex-helper-port-local-port", "");
  source.setAttribute("data-codex-helper-port-id", "");
  source.setAttribute("data-codex-helper-port-key", "");
  return source;
}

function installPortForwardEmptyActions(row, context, host = document) {
  row.setAttribute("data-codex-helper-port-empty-row", "true");
  row.setAttribute("data-codex-helper-port-row", "true");
  const source = createCurrentSessionPortCommandSource(context);
  if (!(source instanceof HTMLElement)) return;
  const entry = {
    hostId: source.getAttribute("data-codex-helper-port-host-id") || "",
    remotePath: source.getAttribute("data-codex-helper-port-remote-path") || "",
    threadId: source.getAttribute("data-codex-helper-port-thread-id") || "",
    remotePort: "",
    localPort: "",
  };
  installPortForwardRowActions(row, entry, host);
}

function createSummaryRowFromTemplate(templateRow, text, iconRow) {
  const row = templateRow.cloneNode(true);
  row.removeAttribute("id");
  clearPortForwardRowActions(row);
  if (iconRow instanceof HTMLElement) {
    const icon = iconRow.querySelector("svg");
    const target = row.querySelector("svg");
    if (icon && target) target.replaceWith(icon.cloneNode(true));
  }
  setSummaryRowText(row, text);
  return row;
}

function ensurePortForwardRowIcon(row, host = document) {
  const icon = createPortForwardIcon(host);
  const target = row.querySelector("svg");
  if (target) {
    target.replaceWith(icon);
    return;
  }
  const label = summaryRowLabelNode(row);
  const container = label?.parentElement;
  if (label && container) container.insertBefore(icon, label);
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
  return `${remotePort} ↔ ${localPort} · ${status}`;
}

function setPortRowContent(row, entry) {
  const label = summaryRowLabelNode(row);
  if (!(label instanceof HTMLElement)) {
    setSummaryRowText(row, portRowLabel(entry));
    return;
  }
  const remote = document.createElement("span");
  remote.textContent = String(entry.remotePort || "—");
  const separator = document.createElement("span");
  separator.textContent = " ↔ ";
  const localUrl = localUrlForPortEntry(entry);
  const local = document.createElement(localUrl ? "button" : "span");
  local.textContent = portLocalPortLabel(entry);
  if (local instanceof HTMLButtonElement) {
    local.type = "button";
    local.className = "codex-helper-port-local-url";
    local.setAttribute(helperPortCommandAttribute, "open-local-url-system");
    local.setAttribute("data-codex-helper-port-local-url", "true");
    local.setAttribute("aria-label", `Open localhost:${portLocalPortLabel(entry)}`);
    local.title = `Open ${localUrl}`;
    setPortCommandDataset(local, entry);
  }
  const status = document.createElement("span");
  status.textContent = ` · ${portStatusLabel(entry)}`;
  label.replaceChildren(remote, separator, local, status);
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
    const emptyRow = createSummaryRowFromTemplate(
      templateRow,
      emptyPortForwardLabel(rows, context),
      portIconRow,
    );
    ensurePortForwardRowIcon(emptyRow, host);
    installPortForwardEmptyActions(emptyRow, context, host);
    list.appendChild(emptyRow);
    return true;
  }

  for (const entry of rows) {
    const row = createSummaryRowFromTemplate(
      templateRow,
      portRowLabel(entry),
      portIconRow,
    );
    ensurePortForwardRowIcon(row, host);
    setPortRowContent(row, entry);
    installPortForwardRowActions(row, entry, host);
    list.appendChild(row);
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

function findPortForwardSectionHeaderButtons(section) {
  const header = section.querySelector("header");
  if (!(header instanceof HTMLElement)) {
    return { disclosure: null, settings: null };
  }
  const buttons = Array.from(header.querySelectorAll("button")).filter(
    (node) => node instanceof HTMLButtonElement,
  );
  if (buttons.length === 0) return { disclosure: null, settings: null };
  const disclosure = buttons[0];
  const settings =
    buttons.find(
      (button) =>
        button !== disclosure &&
        (/settings|configure/i.test(button.getAttribute("aria-label") || "") ||
          /settings|configure/i.test(button.title || "")),
    ) || (buttons.length >= 2 ? buttons[buttons.length - 1] : null);
  return { disclosure, settings };
}

function ensurePortForwardSectionSettingsButton(section, host = document) {
  const { settings } = findPortForwardSectionHeaderButtons(section);
  if (settings instanceof HTMLButtonElement) return settings;
  const environment = findEnvironmentSummarySection(host);
  const envSettings = environment
    ? findPortForwardSectionHeaderButtons(environment).settings
    : null;
  const header = section.querySelector("header");
  if (!(envSettings instanceof HTMLButtonElement) || !(header instanceof HTMLElement)) {
    return null;
  }
  const cloned = envSettings.cloneNode(true);
  if (!(cloned instanceof HTMLButtonElement)) return null;
  cloned.removeAttribute("id");
  header.appendChild(cloned);
  return cloned;
}

function installPortForwardSectionSettings(section, host = document) {
  ensurePortForwardSectionSettingsButton(section, host);
  const { settings } = findPortForwardSectionHeaderButtons(section);
  if (!(settings instanceof HTMLButtonElement)) return false;
  settings.setAttribute(helperPortCommandAttribute, "show-settings-menu");
  settings.setAttribute("data-codex-helper-port-settings-button", "true");
  settings.setAttribute("aria-label", "Port forwarding settings");
  settings.title = "Port forwarding settings";
  settings.removeAttribute("data-state");
  settings.removeAttribute("aria-expanded");
  return true;
}

function setPortForwardPinnedIconDirection(section, collapsed) {
  const { disclosure } = findPortForwardSectionHeaderButtons(section);
  const icon = disclosure?.querySelector("svg");
  if (!(icon instanceof SVGElement)) return;
  icon.style.transform = collapsed ? "rotate(-90deg)" : "";
}

function setPortForwardPinnedDisclosure(section, collapsed) {
  const { disclosure } = findPortForwardSectionHeaderButtons(section);
  const button = disclosure;
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
  const { disclosure } = findPortForwardSectionHeaderButtons(section);
  const button = disclosure;
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
    if (
      existing.getAttribute("data-codex-helper-ports-template") ===
      "environment" &&
      existing.querySelector("[class*='summary-panel-row']")
    ) {
      installPortForwardPinnedDisclosure(existing);
      installPortForwardSectionSettings(existing, host);
      return existing;
    }
    existing.remove();
  }

  const environment = findEnvironmentSummarySection(host);
  const sources = findSourcesSummarySection(host);
  const sectionTemplate = environment || sources;
  if (!(sectionTemplate instanceof HTMLElement)) return null;
  if (!(sources instanceof HTMLElement)) return null;

  const section = sectionTemplate.cloneNode(true);
  section.setAttribute(helperPortsPinnedAttribute, "true");
  section.setAttribute("data-codex-helper-ports-template", "environment");
  setSummarySectionTitle(section, "Port Forward");
  const list = findPortForwardListContainer(section);
  if (list instanceof HTMLElement) list.replaceChildren();
  installPortForwardPinnedDisclosure(section);
  installPortForwardSectionSettings(section, host);
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
  ensurePortScanLoop();
  syncRemoteSessionPorts().catch((error) => {
    handleRemotePortDiscoveryFailure(error);
  });

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
      }, 800);
    }
    return;
  }

  if (pinnedSummaryHideTimer) {
    clearTimeout(pinnedSummaryHideTimer);
    pinnedSummaryHideTimer = 0;
  }

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

function closePortForwardRowMenu() {
  if (portForwardMenuAnchorRow?.isConnected) {
    portForwardMenuAnchorRow.removeAttribute(
      "data-codex-helper-port-row-menu-open",
    );
  }
  portForwardMenuAnchorRow = null;
  portForwardSettingsAnchorButton = null;
  if (portForwardMenuRoot?.isConnected) portForwardMenuRoot.remove();
  portForwardMenuRoot = null;
}

function closePortForwardDialog() {
  if (portForwardDialogRoot?.isConnected) portForwardDialogRoot.remove();
  portForwardDialogRoot = null;
}

function copyPortCommandDataset(source, target) {
  for (const name of [
    "data-codex-helper-port-id",
    "data-codex-helper-port-key",
    "data-codex-helper-port-host-id",
    "data-codex-helper-port-remote-path",
    "data-codex-helper-port-thread-id",
    "data-codex-helper-port-remote-port",
    "data-codex-helper-port-local-port",
    "data-codex-helper-port-url",
  ]) {
    target.setAttribute(name, source.getAttribute(name) || "");
  }
}

function createPortMenuItem(command, label, iconName, source) {
  const item = document.createElement("button");
  item.type = "button";
  item.setAttribute(helperPortCommandAttribute, command);
  copyPortCommandDataset(source, item);
  if (iconName) item.appendChild(createPortActionIcon(iconName));
  item.appendChild(document.createTextNode(label));
  return item;
}

function createPortMenuSeparator() {
  const separator = document.createElement("div");
  separator.className = "codex-helper-port-menu-separator";
  separator.setAttribute("role", "separator");
  separator.setAttribute("aria-hidden", "true");
  return separator;
}

function createPortSettingsToggleMenuItem(settingKey, label) {
  const checked = featureSettings[settingKey] === true;
  const template =
    findNativeSummaryMenuItemTemplate("Use the same local port") ||
    findNativeSummaryMenuItemTemplate("Auto-forward detected web ports");
  if (template instanceof HTMLElement) {
    const item = template.cloneNode(true);
    if (!(item instanceof HTMLElement)) {
      return createPortSettingsToggleMenuItemFallback(settingKey, label, checked);
    }
    item.removeAttribute("id");
    item.removeAttribute("data-highlighted");
    item.removeAttribute("data-state");
    if (item instanceof HTMLButtonElement) {
      item.type = "button";
    }
    item.setAttribute(helperPortCommandAttribute, "toggle-setting");
    item.setAttribute("data-codex-helper-port-setting-key", settingKey);
    item.classList.add("codex-helper-port-menu-toggle");
    item.setAttribute("role", "menuitemcheckbox");
    replaceMenuItemLabel(item, label);
    setMenuItemCheckedState(item, checked);
    return item;
  }
  return createPortSettingsToggleMenuItemFallback(settingKey, label, checked);
}

function createPortSettingsToggleMenuItemFallback(settingKey, label, checked) {
  const item = document.createElement("button");
  item.type = "button";
  item.className = "codex-helper-port-menu-toggle";
  item.setAttribute(helperPortCommandAttribute, "toggle-setting");
  item.setAttribute("data-codex-helper-port-setting-key", settingKey);
  item.setAttribute("role", "menuitemcheckbox");
  item.setAttribute("aria-checked", checked ? "true" : "false");
  const labelSpan = document.createElement("span");
  labelSpan.className = "codex-helper-port-menu-label";
  labelSpan.textContent = label;
  const check = document.createElement("span");
  check.className = "codex-helper-port-menu-check";
  check.setAttribute("aria-hidden", "true");
  if (checked) check.appendChild(createPortActionIcon("check"));
  item.appendChild(labelSpan);
  item.appendChild(check);
  return item;
}

function createPortMenuRoot() {
  const menu = document.createElement("div");
  menu.setAttribute("data-codex-helper-port-menu", "true");
  menu.setAttribute("role", "menu");
  document.body.appendChild(menu);
  return menu;
}

function positionPortForwardMenu(menu, anchor) {
  const rect = anchor.getBoundingClientRect();
  const menuRect = menu.getBoundingClientRect();
  const top = Math.min(rect.bottom + 6, window.innerHeight - menuRect.height - 8);
  const left = Math.min(
    rect.right - menuRect.width,
    window.innerWidth - menuRect.width - 8,
  );
  menu.style.top = `${Math.max(8, top)}px`;
  menu.style.left = `${Math.max(8, left)}px`;
  portForwardMenuRoot = menu;
}

function positionPortForwardMenuAtPoint(menu, clientX, clientY) {
  const menuRect = menu.getBoundingClientRect();
  const top = Math.min(clientY, window.innerHeight - menuRect.height - 8);
  const left = Math.min(clientX, window.innerWidth - menuRect.width - 8);
  menu.style.top = `${Math.max(8, top)}px`;
  menu.style.left = `${Math.max(8, left)}px`;
  portForwardMenuRoot = menu;
}

function openPortForwardRowMenu(button) {
  closePortForwardRowMenu();
  const row = button.closest("[data-codex-helper-port-row]");
  if (row instanceof HTMLElement) {
    row.setAttribute("data-codex-helper-port-row-menu-open", "true");
    portForwardMenuAnchorRow = row;
  }
  const menu = createPortMenuRoot();
  menu.appendChild(
    createPortMenuItem(
      "add-mapping",
      "Add port mapping record",
      "plus",
      button,
    ),
  );
  const remotePort = Number(
    button.getAttribute("data-codex-helper-port-remote-port") || "",
  );
  if (Number.isInteger(remotePort) && remotePort > 0) {
    menu.appendChild(
      createPortMenuItem(
        "edit-mapping",
        "Edit mapping record",
        "pencil",
        button,
      ),
    );
    menu.appendChild(
      createPortMenuItem(
        "delete-mapping",
        "Delete mapping record",
        "trash",
        button,
      ),
    );
  }
  positionPortForwardMenu(menu, button);
}

function openPortForwardSettingsMenu(button) {
  closePortForwardRowMenu();
  portForwardSettingsAnchorButton = button;
  const menu = createPortMenuRoot();
  menu.setAttribute("data-codex-helper-port-settings-menu", "true");
  menu.appendChild(
    createPortSettingsToggleMenuItem(
      "portAutoForwardWeb",
      "Auto-forward detected web ports",
    ),
  );
  menu.appendChild(
    createPortSettingsToggleMenuItem(
      "portSameLocalPort",
      "Use the same local port by default",
    ),
  );
  menu.appendChild(createPortMenuSeparator());
  menu.appendChild(
    createPortMenuItem("open-settings", "Helper Settings", "settings", button),
  );
  positionPortForwardMenu(menu, button);
}

function openPortLocalUrlMenu(button, event) {
  closePortForwardRowMenu();
  const menu = createPortMenuRoot();
  menu.appendChild(
    createPortMenuItem(
      "open-local-url-system",
      "Open in default browser",
      "open",
      button,
    ),
  );
  menu.appendChild(
    createPortMenuItem("copy", "Copy local address", "copy", button),
  );
  positionPortForwardMenuAtPoint(menu, event.clientX, event.clientY);
}

function commandContextFromButton(button) {
  const hostId = button.getAttribute("data-codex-helper-port-host-id") || "";
  const remotePath =
    button.getAttribute("data-codex-helper-port-remote-path") || "";
  const threadId = button.getAttribute("data-codex-helper-port-thread-id") || "";
  return { hostId, path: remotePath, threadId };
}

function portEntryFromCommandButton(button, options = {}) {
  const id = button.getAttribute("data-codex-helper-port-id") || "";
  const key = button.getAttribute("data-codex-helper-port-key") || "";
  for (const entry of detectedPorts.values()) {
    if ((id && entry.id === id) || (key && entry.key === key)) return entry;
  }

  const context = commandContextFromButton(button);
  const remotePort = Number(
    button.getAttribute("data-codex-helper-port-remote-port") || "",
  );
  const localPort = Number(
    button.getAttribute("data-codex-helper-port-local-port") || "",
  );
  if (
    !context.hostId ||
    !context.path ||
    !context.threadId ||
    !Number.isInteger(remotePort) ||
    remotePort < 1
  ) {
    return null;
  }
  const entry = {
    key: key || portKey(context, remotePort, localPort),
    id,
    hostId: context.hostId,
    remotePath: context.path,
    threadId: context.threadId,
    remotePort,
    localPort: Number.isInteger(localPort) && localPort > 0 ? localPort : 0,
    status: id ? "active" : "detected",
    lastSeenAt: portTimestamp(),
  };
  if (options.create !== false) detectedPorts.set(entry.key, entry);
  return entry;
}

async function stopPortEntryTunnel(entry) {
  if (!entry?.id) return;
  const result = await bridge("/ports/stop", { id: entry.id });
  if (result?.status !== "ok") throw new Error(result?.message || "Stop failed");
  delete entry.id;
  delete entry.localUrl;
}

function parsePortInput(value) {
  const port = Number(String(value || "").trim());
  return Number.isInteger(port) && port >= 1 && port <= 65535 ? port : 0;
}

function resolvePortMappingDialog(result, value, cleanup) {
  cleanup();
  result(value);
}

async function requestPortMappingInput(options = {}) {
  closePortForwardRowMenu();
  closePortForwardDialog();
  if (portForwardDialogRoot?.isConnected) return null;
  return new Promise((resolve) => {
    const dialog = document.createElement("div");
    dialog.setAttribute("data-codex-helper-port-dialog", "true");
    dialog.setAttribute("role", "dialog");
    dialog.setAttribute("aria-modal", "true");
    dialog.setAttribute("aria-label", options.title || "Port mapping");
    dialog.innerHTML = `
      <div class="codex-helper-port-dialog-panel">
        <div class="codex-helper-port-dialog-title">${options.title || "Port mapping"}</div>
        <div class="codex-helper-port-dialog-port-row">
          <label>
            <span>Remote port</span>
            <input data-codex-helper-port-dialog-remote inputmode="numeric" autocomplete="off">
          </label>
          <span class="codex-helper-port-dialog-arrow" aria-hidden="true">↔</span>
          <label>
            <span>Local port</span>
            <input data-codex-helper-port-dialog-local inputmode="numeric" autocomplete="off">
          </label>
        </div>
        <div class="codex-helper-port-dialog-error" data-codex-helper-port-dialog-error></div>
        <div class="codex-helper-port-dialog-actions">
          <button type="button" data-codex-helper-port-dialog-cancel>Cancel</button>
          <button type="button" data-codex-helper-port-dialog-submit>${options.submitLabel || "Save"}</button>
        </div>
      </div>
    `;
    const remoteInput = dialog.querySelector(
      "[data-codex-helper-port-dialog-remote]",
    );
    const localInput = dialog.querySelector(
      "[data-codex-helper-port-dialog-local]",
    );
    const error = dialog.querySelector("[data-codex-helper-port-dialog-error]");
    if (
      !(remoteInput instanceof HTMLInputElement) ||
      !(localInput instanceof HTMLInputElement) ||
      !(error instanceof HTMLElement)
    ) {
      resolve(null);
      return;
    }
    remoteInput.value = options.remotePort ? String(options.remotePort) : "";
    localInput.value = options.localPort ? String(options.localPort) : "";
    if (options.lockRemotePort) {
      remoteInput.readOnly = true;
      remoteInput.setAttribute("aria-readonly", "true");
    }
    const cleanup = () => {
      dialog.remove();
      if (portForwardDialogRoot === dialog) portForwardDialogRoot = null;
    };
    const submit = () => {
      const remotePort = parsePortInput(remoteInput.value);
      const localPort = parsePortInput(localInput.value);
      if (!remotePort || !localPort) {
        error.textContent = "Enter valid ports from 1 to 65535.";
        return;
      }
      resolvePortMappingDialog(resolve, { remotePort, localPort }, cleanup);
    };
    dialog.addEventListener(
      "click",
      (event) => {
        const target = event.target;
        if (
          target === dialog ||
          target?.closest?.("[data-codex-helper-port-dialog-cancel]")
        ) {
          event.preventDefault();
          resolvePortMappingDialog(resolve, null, cleanup);
          return;
        }
        if (target?.closest?.("[data-codex-helper-port-dialog-submit]")) {
          event.preventDefault();
          submit();
        }
      },
      true,
    );
    dialog.addEventListener(
      "keydown",
      (event) => {
        if (event.key === "Escape") {
          event.preventDefault();
          resolvePortMappingDialog(resolve, null, cleanup);
          return;
        }
        if (event.key === "Enter") {
          event.preventDefault();
          submit();
        }
      },
      true,
    );
    document.body.appendChild(dialog);
    portForwardDialogRoot = dialog;
    (options.lockRemotePort ? localInput : remoteInput).focus();
  });
}

async function confirmPortMappingDelete(entry) {
  closePortForwardRowMenu();
  closePortForwardDialog();
  if (portForwardDialogRoot?.isConnected) return false;
  return new Promise((resolve) => {
    const dialog = document.createElement("div");
    dialog.setAttribute("data-codex-helper-port-dialog", "true");
    dialog.setAttribute("role", "dialog");
    dialog.setAttribute("aria-modal", "true");
    dialog.setAttribute("aria-label", "Delete port mapping");
    dialog.innerHTML = `
      <div class="codex-helper-port-dialog-panel">
        <div class="codex-helper-port-dialog-title">Delete port mapping?</div>
        <div class="codex-helper-port-dialog-message">Remote port ${entry.remotePort} will stop forwarding to localhost:${portLocalPortLabel(entry)}.</div>
        <div class="codex-helper-port-dialog-actions">
          <button type="button" data-codex-helper-port-dialog-cancel>Cancel</button>
          <button type="button" data-codex-helper-port-dialog-delete>Delete</button>
        </div>
      </div>
    `;
    const cleanup = () => {
      dialog.remove();
      if (portForwardDialogRoot === dialog) portForwardDialogRoot = null;
    };
    dialog.addEventListener(
      "click",
      (event) => {
        const target = event.target;
        if (
          target === dialog ||
          target?.closest?.("[data-codex-helper-port-dialog-cancel]")
        ) {
          event.preventDefault();
          resolvePortMappingDialog(resolve, false, cleanup);
          return;
        }
        if (target?.closest?.("[data-codex-helper-port-dialog-delete]")) {
          event.preventDefault();
          resolvePortMappingDialog(resolve, true, cleanup);
        }
      },
      true,
    );
    dialog.addEventListener(
      "keydown",
      (event) => {
        if (event.key !== "Escape") return;
        event.preventDefault();
        resolvePortMappingDialog(resolve, false, cleanup);
      },
      true,
    );
    document.body.appendChild(dialog);
    portForwardDialogRoot = dialog;
    const cancel = dialog.querySelector(
      "[data-codex-helper-port-dialog-cancel]",
    );
    if (cancel instanceof HTMLElement) cancel.focus();
  });
}

async function addPortMapping(button) {
  const context = commandContextFromButton(button);
  if (!context.hostId || !context.path || !context.threadId) return;
  const input = await requestPortMappingInput({
    title: "Add port mapping",
    remotePort: "",
    localPort: "",
    submitLabel: "Add",
  });
  if (!input) return;
  const key = portKey(context, input.remotePort, input.localPort);
  const existing =
    detectedPorts.get(key) ||
    detectedEntryForRemotePort(
      { hostId: context.hostId, path: context.path, threadId: context.threadId },
      input.remotePort,
    );
  const entry =
    existing ||
    {
      key,
      hostId: context.hostId,
      remotePath: context.path,
      threadId: context.threadId,
      remotePort: input.remotePort,
      localPort: input.localPort,
      status: "detected",
      lastSeenAt: portTimestamp(),
    };
  if (
    existing?.id &&
    Number.isInteger(existing.localPort) &&
    existing.localPort > 0 &&
    existing.localPort !== input.localPort
  ) {
    await stopPortEntryTunnel(existing);
  }
  entry.localPort = input.localPort;
  entry.status = "detected";
  entry.message = "";
  setDetectedPortEntryKey(entry, key);
  unsuppressPortMapping(entry);
  await forwardDetectedPort(entry, "manual");
}

async function editPortMapping(button) {
  const entry = portEntryFromCommandButton(button);
  if (!entry) return;
  const currentLocalPort =
    Number.isInteger(entry.localPort) && entry.localPort > 0
      ? entry.localPort
      : entry.remotePort;
  const input = await requestPortMappingInput({
    title: "Edit port mapping",
    remotePort: entry.remotePort,
    localPort: currentLocalPort,
    lockRemotePort: true,
    submitLabel: "Save",
  });
  if (!input) return;
  unsuppressPortMapping(entry);
  await stopPortEntryTunnel(entry);
  const previousKey = entry.key;
  entry.localPort = input.localPort;
  entry.status = "detected";
  entry.message = "";
  const nextKey = portKey(
    { hostId: entry.hostId, path: entry.remotePath, threadId: entry.threadId },
    entry.remotePort,
    input.localPort,
  );
  if (previousKey !== nextKey) setDetectedPortEntryKey(entry, nextKey);
  await forwardDetectedPort(entry, "manual");
}

async function deletePortMapping(button) {
  const entry = portEntryFromCommandButton(button);
  if (!entry) return;
  if (!(await confirmPortMappingDelete(entry))) return;
  const removedId = entry.id || "";
  const removedKey = entry.key || "";
  await stopPortEntryTunnel(entry);
  suppressPortMapping(entry);
  for (const [key, value] of Array.from(detectedPorts.entries())) {
    const sameMapping =
      value.hostId === entry.hostId &&
      value.remotePath === entry.remotePath &&
      value.threadId === entry.threadId &&
      Number(value.remotePort) === Number(entry.remotePort);
    if (
      value === entry ||
      (removedId && value.id === removedId) ||
      (removedKey && key === removedKey) ||
      sameMapping
    ) {
      detectedPorts.delete(key);
    }
  }
  showHelperToast(`Deleted port mapping for ${entry.remotePort}`);
  await refreshPortsPanelIfVisible();
}

function openPortLocalUrlInCodex(button) {
  const localUrl = button.getAttribute("data-codex-helper-port-url") || "";
  if (!localForwardedUrlIsAllowed(localUrl)) {
    throw new Error("Only local forwarded URLs can be opened");
  }
  closePortForwardRowMenu();
  window.open(localUrl, "_blank", "noopener,noreferrer");
}

async function openPortLocalUrlInSystem(button) {
  const localUrl = button.getAttribute("data-codex-helper-port-url") || "";
  if (!localForwardedUrlIsAllowed(localUrl)) {
    throw new Error("Only local forwarded URLs can be opened");
  }
  closePortForwardRowMenu();
  const result = await bridge("/url/open-external", { url: localUrl });
  if (result?.status !== "ok") {
    throw new Error(result?.message || "Open failed");
  }
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

async function togglePortForwardSetting(button) {
  const key = button.getAttribute("data-codex-helper-port-setting-key") || "";
  if (!(key in featureSettings)) {
    throw new Error("Unknown port forwarding setting");
  }
  const settingsButton = portForwardSettingsAnchorButton?.isConnected
    ? portForwardSettingsAnchorButton
    : null;
  const result = await bridge("/settings/set", { [key]: !featureSettings[key] });
  if (result?.status !== "ok") {
    throw new Error(result?.message || "Settings update failed");
  }
  applySettings(result);
  closePortForwardRowMenu();
  if (settingsButton instanceof HTMLButtonElement) {
    openPortForwardSettingsMenu(settingsButton);
  }
}

async function handlePortCommand(button) {
  const command = button.getAttribute(helperPortCommandAttribute) || "";
  if (
    (
      command === "forward" ||
      command === "manual" ||
      command === "show-mapping-menu" ||
      command === "add-mapping" ||
      command === "edit-mapping" ||
      command === "delete-mapping"
    ) &&
    !isPortForwardingOperational()
  ) {
    throw new Error(portsUnavailableMessage());
  }
  const id = button.getAttribute("data-codex-helper-port-id") || "";
  const localUrl = button.getAttribute("data-codex-helper-port-url") || "";
  if (command === "show-settings-menu") {
    openPortForwardSettingsMenu(button);
    return;
  }
  if (command === "toggle-setting") {
    await togglePortForwardSetting(button);
    return;
  }
  if (command === "open-settings") {
    closePortForwardRowMenu();
    await openNativeHelperSettingsFromApp("general");
    return;
  }
  if (command === "show-mapping-menu") {
    openPortForwardRowMenu(button);
    return;
  }
  if (command === "add-mapping") {
    await addPortMapping(button);
    return;
  }
  if (command === "edit-mapping") {
    await editPortMapping(button);
    return;
  }
  if (command === "delete-mapping") {
    await deletePortMapping(button);
    return;
  }
  if (command === "open-local-url-codex") {
    openPortLocalUrlInCodex(button);
    return;
  }
  if (command === "open-local-url-system") {
    await openPortLocalUrlInSystem(button);
    return;
  }
  if (command === "open" && localUrl) {
    closePortForwardRowMenu();
    window.open(localUrl, "_blank", "noopener,noreferrer");
    return;
  }
  if (command === "copy" && localUrl) {
    closePortForwardRowMenu();
    await navigator.clipboard.writeText(localUrl);
    showHelperToast("Copied local address");
    return;
  }
  if (command === "stop" && id) {
    closePortForwardRowMenu();
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
    const input = await requestPortMappingInput({
      title: "Forward port",
      remotePort: entry.remotePort,
      localPort: entry.localPort || entry.remotePort,
      lockRemotePort: true,
      submitLabel: "Forward",
    });
    if (!input) return;
    const previousKey = entry.key;
    entry.localPort = input.localPort;
    entry.key = portKey(
      { hostId: entry.hostId, path: entry.remotePath, threadId: entry.threadId },
      entry.remotePort,
      input.localPort,
    );
    if (previousKey !== entry.key) {
      detectedPorts.delete(previousKey);
      detectedPorts.set(entry.key, entry);
    }
    await forwardDetectedPort(entry, "manual");
  }
}
