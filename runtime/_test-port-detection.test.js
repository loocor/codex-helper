import { expect, test } from "bun:test";
import { buildRuntimeBundle } from "./bundle.ts";

const source = buildRuntimeBundle();

function extractFunction(name) {
  const marker = `function ${name}(`;
  const start = source.indexOf(marker);
  if (start === -1) throw new Error(`${name} not found`);
  const braceStart = source.indexOf("{", start);
  let depth = 0;
  for (let index = braceStart; index < source.length; index += 1) {
    const char = source[index];
    if (char === "{") depth += 1;
    if (char === "}") depth -= 1;
    if (depth === 0) return source.slice(start, index + 1);
  }
  throw new Error(`${name} closing brace not found`);
}

function loadFunction(name) {
  return new Function(`${extractFunction(name)}; return ${name};`)();
}

function sessionContextForDom({ activeThread, projectPath, hostNodes = [] }) {
  class Element {
    constructor(attrs) {
      this.attrs = attrs;
      this.textContent = "";
      this.className = attrs.class || "";
    }

    getAttribute(name) {
      return this.attrs[name] || null;
    }

    getBoundingClientRect() {
      return { width: 240, height: 32 };
    }
  }

  const document = {
    thread: new Element(activeThread),
    querySelector(selector) {
      if (selector === '[data-app-action-sidebar-thread-active="true"]') {
        return this.thread;
      }
      return null;
    },
    querySelectorAll(selector) {
      const attribute = selector.match(/^\[([^\]]+)\]$/)?.[1] || "";
      if (attribute === "data-app-action-sidebar-thread-host-id") {
        return hostNodes.map((attrs) => new Element(attrs));
      }
      if (attribute === "data-app-action-sidebar-project-list-id") {
        return [
          new Element({
            "data-app-action-sidebar-project-list-id": projectPath,
          }),
        ];
      }
      return [];
    },
  };

  return new Function(
    "document",
    "HTMLElement",
    [
      extractFunction("textOf"),
      extractFunction("selectedAttributeValue"),
      extractFunction("remoteContextFromDom"),
      extractFunction("getActiveThreadElement"),
      extractFunction("normalizeRemoteHostId"),
      extractFunction("threadKindIsLocal"),
      extractFunction("sessionContextFromDom"),
      "return sessionContextFromDom();",
    ].join("\n"),
  )(document, Element);
}

test("parseWebPortsFromText extracts clear web URLs", () => {
  const parseWebPortsFromText = loadFunction("parseWebPortsFromText");

  expect(
    parseWebPortsFromText(`
      Vite ready at http://localhost:5173/
      API listening on http://127.0.0.1:8000/health
      Preview: http://0.0.0.0:3000
      IPv6: http://[::1]:7000/path
      Local server on localhost:8787
    `),
  ).toEqual([
    { port: 5173, url: "http://localhost:5173/" },
    { port: 8000, url: "http://127.0.0.1:8000/health" },
    { port: 3000, url: "http://0.0.0.0:3000" },
    { port: 7000, url: "http://[::1]:7000/path" },
    { port: 8787, url: "http://127.0.0.1:8787" },
  ]);
});

test("parseWebPortsFromText ignores ambiguous and invalid ports", () => {
  const parseWebPortsFromText = loadFunction("parseWebPortsFromText");

  expect(
    parseWebPortsFromText(`
      listening on port 5432
      invalid http://localhost:70000
      not local http://example.com:3000
    `),
  ).toEqual([]);
});

test("active conversation id parser accepts Codex thread routes", () => {
  const parseActiveConversationIdFromPath = new Function(
    [
      extractFunction("normalizeConversationId"),
      extractFunction("parseActiveConversationIdFromPath"),
      "return parseActiveConversationIdFromPath;",
    ].join("\n"),
  )();

  expect(parseActiveConversationIdFromPath("/local/019e-thread")).toBe(
    "019e-thread",
  );
  expect(parseActiveConversationIdFromPath("/remote/019e-thread")).toBe(
    "019e-thread",
  );
  expect(
    parseActiveConversationIdFromPath("/hotkey-window/thread/019e-thread"),
  ).toBe("019e-thread");
  expect(parseActiveConversationIdFromPath("/settings")).toBe("");
});

test("active conversation id can be read from the sidebar thread", () => {
  class Element {
    getAttribute(name) {
      return name === "data-app-action-sidebar-thread-id"
        ? "local:019e-thread"
        : "";
    }
  }
  const activeConversationIdFromDom = new Function(
    "document",
    "HTMLElement",
    [
      extractFunction("getActiveThreadElement"),
      extractFunction("normalizeConversationId"),
      extractFunction("activeConversationIdFromDom"),
      "return activeConversationIdFromDom;",
    ].join("\n"),
  )(
    {
      querySelector(selector) {
        return selector ===
          '[data-app-action-sidebar-thread-active="true"]'
          ? new Element()
          : null;
      },
    },
    Element,
  );

  expect(activeConversationIdFromDom()).toBe("019e-thread");
});

test("structured execution target produces a remote forwarding context", () => {
  const normalizeStructuredExecutionTarget = new Function(
    [
      extractFunction("normalizeRemoteHostId"),
      extractFunction("normalizeConversationId"),
      extractFunction("normalizeStructuredExecutionContext"),
      extractFunction("normalizeStructuredExecutionTarget"),
      "return normalizeStructuredExecutionTarget;",
    ].join("\n"),
  )();

  expect(
    normalizeStructuredExecutionTarget(
      {
        cwd: "/Volumes/External/GitHub/MCPMate/admin",
        hostId: "remote-ssh-codex-managed:MacMini",
        hostConfig: {
          id: "remote-ssh-codex-managed:MacMini",
          kind: "ssh",
          display_name: "MacMini",
        },
        conversationId: "019e-thread",
      },
      "019e-thread",
    ),
  ).toEqual({
    hostId: "remote-ssh-codex-managed:MacMini",
    path: "/Volumes/External/GitHub/MCPMate/admin",
    threadId: "019e-thread",
    kind: "ssh",
    isRemote: true,
    source: "codex-structured-context",
  });
});

test("structured execution target rejects local and incomplete contexts", () => {
  const normalizeStructuredExecutionTarget = new Function(
    [
      extractFunction("normalizeRemoteHostId"),
      extractFunction("normalizeConversationId"),
      extractFunction("normalizeStructuredExecutionContext"),
      extractFunction("normalizeStructuredExecutionTarget"),
      "return normalizeStructuredExecutionTarget;",
    ].join("\n"),
  )();

  expect(
    normalizeStructuredExecutionTarget(
      {
        cwd: "/Volumes/External/GitHub/CodexHelper",
        hostId: "local",
        hostConfig: { id: "local", kind: "local" },
      },
      "019e-thread",
    ),
  ).toBe(null);
  expect(
    normalizeStructuredExecutionTarget(
      {
        cwd: "",
        hostId: "remote-ssh-codex-managed:MacMini",
        hostConfig: {
          id: "remote-ssh-codex-managed:MacMini",
          kind: "ssh",
        },
        conversationId: "019e-thread",
      },
      "019e-thread",
    ),
  ).toBe(null);
});

test("structured execution target rejects another conversation", () => {
  const normalizeStructuredExecutionTarget = new Function(
    [
      extractFunction("normalizeRemoteHostId"),
      extractFunction("normalizeConversationId"),
      extractFunction("normalizeStructuredExecutionContext"),
      extractFunction("normalizeStructuredExecutionTarget"),
      "return normalizeStructuredExecutionTarget;",
    ].join("\n"),
  )();

  expect(
    normalizeStructuredExecutionTarget(
      {
        cwd: "/Volumes/External/GitHub/MCPMate/admin",
        hostId: "remote-ssh-codex-managed:MacMini",
        hostConfig: {
          id: "remote-ssh-codex-managed:MacMini",
          kind: "ssh",
        },
        conversationId: "other-thread",
      },
      "019e-thread",
    ),
  ).toBe(null);
});

test("structured execution target can be read from a React fiber tree", () => {
  const readStructuredRemoteContextFromFiberRoot = new Function(
    [
      extractFunction("normalizeRemoteHostId"),
      extractFunction("normalizeConversationId"),
      extractFunction("normalizeStructuredExecutionContext"),
      extractFunction("normalizeStructuredExecutionTarget"),
      extractFunction("findStructuredExecutionContextInValue"),
      extractFunction("findStructuredExecutionTargetInValue"),
      extractFunction("readStructuredContextFromFiberRoot"),
      extractFunction("readStructuredRemoteContextFromFiberRoot"),
      "return readStructuredRemoteContextFromFiberRoot;",
    ].join("\n"),
  )();

  const fiberRoot = {
    memoizedProps: { label: "root" },
    child: {
      memoizedProps: {
        value: {
          cwd: "/Volumes/External/GitHub/MCPMate/admin",
          hostId: "remote-ssh-codex-managed:MacMini",
          hostConfig: {
            id: "remote-ssh-codex-managed:MacMini",
            kind: "ssh",
            display_name: "MacMini",
          },
          conversationId: "019e-thread",
        },
      },
    },
  };

  expect(
    readStructuredRemoteContextFromFiberRoot(fiberRoot, "019e-thread"),
  ).toEqual({
    hostId: "remote-ssh-codex-managed:MacMini",
    path: "/Volumes/External/GitHub/MCPMate/admin",
    threadId: "019e-thread",
    kind: "ssh",
    isRemote: true,
    source: "codex-structured-context",
  });
});

test("React fiber root can be discovered from element metadata", () => {
  class Element {}
  const { reactFiberFromElement, rootFiberFromFiber } = new Function(
    "HTMLElement",
    [
      extractFunction("reactFiberKeys"),
      extractFunction("reactFiberFromElement"),
      extractFunction("rootFiberFromFiber"),
      "return { reactFiberFromElement, rootFiberFromFiber };",
    ].join("\n"),
  )(Element);
  const root = { tag: "root" };
  const parent = { stateNode: { _internalRoot: { current: root } } };
  const child = { return: parent };
  const element = new Element();
  element.__reactFiber$codex = child;

  expect(reactFiberFromElement(element)).toBe(child);
  expect(rootFiberFromFiber(child)).toBe(root);
});

test("current remote context prefers Codex structured context before DOM fallback", () => {
  const currentRemoteForwardingContext = extractFunction(
    "currentRemoteForwardingContext",
  );

  expect(source).toContain("function structuredForwardingContextFromCodex()");
  expect(currentRemoteForwardingContext.indexOf(
    "const structuredContext = structuredForwardingContextFromCodex();",
  )).toBeGreaterThan(-1);
  expect(currentRemoteForwardingContext.indexOf("sessionContextFromDom()")).toBeGreaterThan(
    currentRemoteForwardingContext.indexOf(
      "const structuredContext = structuredForwardingContextFromCodex();",
    ),
  );
});

test("remote forwarding availability uses the unified context resolver", () => {
  const hasRemoteForwardingContext = extractFunction(
    "hasRemoteForwardingContext",
  );

  expect(hasRemoteForwardingContext).toContain(
    "const context = currentRemoteForwardingContext();",
  );
  expect(hasRemoteForwardingContext.indexOf("pinnedSummaryShowsRemote()")).toBeGreaterThan(
    hasRemoteForwardingContext.indexOf("currentRemoteForwardingContext()"),
  );
  expect(hasRemoteForwardingContext).toContain(
    'context?.source === "codex-structured-context"',
  );
});

test("structured local context blocks stale remote fallback state", () => {
  const currentRemoteForwardingContext = extractFunction(
    "currentRemoteForwardingContext",
  );

  expect(source).toContain("function normalizeStructuredExecutionContext(");
  expect(currentRemoteForwardingContext).toContain(
    "structuredContext.source === \"codex-structured-context\"",
  );
  expect(currentRemoteForwardingContext).toContain(
    "resolvedRemoteForwardingContext = null",
  );
});

test("terminal port scanner does not read helper-owned panel text directly", () => {
  expect(source).toContain("function terminalTextForPortScan()");
  expect(source).toContain("const text = terminalTextForPortScan();");
  expect(source).not.toContain("const text = textOf(document.body);");
});

test("terminal port scanner is scoped to terminal-like roots", () => {
  const terminalTextForPortScan = extractFunction("terminalTextForPortScan");

  expect(source).toContain("function findTerminalPortScanRoots()");
  expect(terminalTextForPortScan).not.toContain("createTreeWalker(document.body");
  expect(terminalTextForPortScan).toContain("findTerminalPortScanRoots()");
});

test("detected ports can auto-forward when same-port default is disabled", () => {
  const localPortForDetectedPort = new Function(
    "featureSettings",
    `${extractFunction("localPortForDetectedPort")}; return localPortForDetectedPort;`,
  )({ portSameLocalPort: false });
  const shouldAutoForwardDetectedPort = new Function(
    "featureSettings",
    `${extractFunction("shouldAutoForwardDetectedPort")}; return shouldAutoForwardDetectedPort;`,
  )({ portAutoForwardWeb: true });

  expect(localPortForDetectedPort(5173)).toBe(0);
  expect(shouldAutoForwardDetectedPort({ remotePort: 5173, localPort: 0 }, { hostId: "remote" })).toBe(true);
});

test("remote forwarding context requires a thread id for lifecycle actions", () => {
  const remoteForwardingContextIsReady = loadFunction(
    "remoteForwardingContextIsReady",
  );

  expect(
    remoteForwardingContextIsReady({
      isRemote: true,
      hostId: "remote-ssh-codex-managed:MacMini",
      path: "/Volumes/External/GitHub/MCPMate",
      threadId: "",
    }),
  ).toBe(false);
  expect(
    remoteForwardingContextIsReady({
      isRemote: true,
      hostId: "remote-ssh-codex-managed:MacMini",
      path: "/Volumes/External/GitHub/MCPMate",
      threadId: "019e-thread",
    }),
  ).toBe(true);
});

test("remote discovery candidates need current terminal port evidence", () => {
  const { terminalPortEvidenceSet, portsWithSessionEvidence } = new Function(
    [
      extractFunction("parseWebPortsFromText"),
      extractFunction("terminalPortEvidenceSet"),
      extractFunction("portsWithSessionEvidence"),
      "return { terminalPortEvidenceSet, portsWithSessionEvidence };",
    ].join("\n"),
  )();
  const evidence = terminalPortEvidenceSet(`
    Next.js ready at http://localhost:3000
  `);

  expect(
    portsWithSessionEvidence(
      [
        { remotePort: 3000, command: "next" },
        { remotePort: 5173, command: "vite" },
      ],
      evidence,
    ),
  ).toEqual([{ remotePort: 3000, command: "next" }]);
});

test("port keys keep custom local port choices distinct", () => {
  const portKey = loadFunction("portKey");
  const context = { hostId: "remote", path: "/srv/app", threadId: "thread-a" };

  expect(portKey(context, 5173, 0)).toBe("remote:/srv/app:thread-a:5173:custom");
  expect(portKey(context, 5173, 15173)).toBe("remote:/srv/app:thread-a:5173:15173");
});

test("remote session context can use sidebar host when active thread omits host", () => {
  const sessionContextFromDom = extractFunction("sessionContextFromDom");

  expect(sessionContextFromDom).toContain("const legacy = remoteContextFromDom();");
  expect(sessionContextFromDom).toContain("threadHostId ||");
  expect(sessionContextFromDom).toContain(
    "!rawThreadHostId && kind && !threadKindIsLocal(kind)",
  );
  expect(sessionContextFromDom).toContain("threadKindIsLocal(kind)");
});

test("remote host id wins when Codex marks a remote thread kind as local", () => {
  const context = sessionContextForDom({
    activeThread: {
      "data-app-action-sidebar-thread-active": "true",
      "data-app-action-sidebar-thread-host-id":
        "remote-ssh-codex-managed:MacMini",
      "data-app-action-sidebar-thread-id": "local:thread-a",
      "data-app-action-sidebar-thread-kind": "local",
    },
    projectPath: "/Volumes/External/GitHub/MCPMate",
    hostNodes: [
      {
        "data-app-action-sidebar-thread-host-id":
          "remote-ssh-codex-managed:MacMini",
      },
    ],
  });

  expect(context).toEqual({
    hostId: "remote-ssh-codex-managed:MacMini",
    path: "/Volumes/External/GitHub/MCPMate",
    threadId: "local:thread-a",
    kind: "local",
    isRemote: true,
  });
});

test("remote session context falls back to selected host for non-local thread kinds", () => {
  const context = sessionContextForDom({
    activeThread: {
      "data-app-action-sidebar-thread-active": "true",
      "data-app-action-sidebar-thread-id": "local:thread-a",
      "data-app-action-sidebar-thread-kind": "remote",
    },
    projectPath: "/Volumes/External/GitHub/MCPMate",
    hostNodes: [
      {
        "data-app-action-sidebar-thread-host-id":
          "remote-ssh-codex-managed:MacMini",
      },
    ],
  });

  expect(context).toEqual({
    hostId: "remote-ssh-codex-managed:MacMini",
    path: "/Volumes/External/GitHub/MCPMate",
    threadId: "local:thread-a",
    kind: "remote",
    isRemote: true,
  });
});

test("missing thread kind does not inherit a remote sidebar host", () => {
  const context = sessionContextForDom({
    activeThread: {
      "data-app-action-sidebar-thread-active": "true",
      "data-app-action-sidebar-thread-id": "local:thread-a",
    },
    projectPath: "/Volumes/External/GitHub/MCPMate",
    hostNodes: [
      {
        "data-app-action-sidebar-thread-host-id":
          "remote-ssh-codex-managed:MacMini",
      },
    ],
  });

  expect(context).toEqual({
    hostId: "",
    path: "/Volumes/External/GitHub/MCPMate",
    threadId: "local:thread-a",
    kind: "",
    isRemote: false,
  });
});

test("local thread kind does not inherit a remote sidebar host", () => {
  const context = sessionContextForDom({
    activeThread: {
      "data-app-action-sidebar-thread-active": "true",
      "data-app-action-sidebar-thread-id": "local:thread-a",
      "data-app-action-sidebar-thread-kind": "local",
    },
    projectPath: "/Volumes/External/GitHub/MCPMate",
    hostNodes: [
      {
        "data-app-action-sidebar-thread-host-id":
          "remote-ssh-codex-managed:MacMini",
      },
    ],
  });

  expect(context).toEqual({
    hostId: "",
    path: "/Volumes/External/GitHub/MCPMate",
    threadId: "local:thread-a",
    kind: "local",
    isRemote: false,
  });
});

test("local host id does not inherit a remote sidebar host", () => {
  const context = sessionContextForDom({
    activeThread: {
      "data-app-action-sidebar-thread-active": "true",
      "data-app-action-sidebar-thread-host-id": "local",
      "data-app-action-sidebar-thread-id": "local:thread-a",
      "data-app-action-sidebar-thread-kind": "remote",
    },
    projectPath: "/Volumes/External/GitHub/MCPMate",
    hostNodes: [
      {
        "data-app-action-sidebar-thread-host-id": "local",
      },
      {
        "data-app-action-sidebar-thread-host-id":
          "remote-ssh-codex-managed:MacMini",
      },
    ],
  });

  expect(context).toEqual({
    hostId: "",
    path: "/Volumes/External/GitHub/MCPMate",
    threadId: "local:thread-a",
    kind: "remote",
    isRemote: false,
  });
});

test("non-path project ids do not produce a remote forwarding context", () => {
  const context = sessionContextForDom({
    activeThread: {
      "data-app-action-sidebar-thread-active": "true",
      "data-app-action-sidebar-thread-host-id":
        "remote-ssh-codex-managed:MacMini",
      "data-app-action-sidebar-thread-id": "local:thread-a",
      "data-app-action-sidebar-thread-kind": "remote",
    },
    projectPath: "9dca1045-0dcf-423b-b0a9-d49c99a31166",
    hostNodes: [
      {
        "data-app-action-sidebar-thread-host-id":
          "remote-ssh-codex-managed:MacMini",
      },
    ],
  });

  expect(context).toEqual({
    hostId: "remote-ssh-codex-managed:MacMini",
    path: "",
    threadId: "local:thread-a",
    kind: "remote",
    isRemote: false,
  });
});

test("runtime discovers remote ports through bridge heartbeat", () => {
  expect(source).toContain('bridge("/ports/discover"');
  expect(source).toContain("function syncRemoteSessionPorts()");
  expect(source).toContain("syncRemoteSessionPorts().catch");
});

test("runtime can resolve remote context from pinned summary fallback", () => {
  expect(source).toContain("function pinnedSummaryShowsRemote()");
  expect(source).toContain("function resolveRemoteForwardingContext()");
  expect(source).toContain('bridge("/zed-remote/fallback-request"');
  expect(source).toContain("rememberRemoteForwardingContext({");
});

test("local pinned summary clears cached remote forwarding context", () => {
  const currentRemoteForwardingContext = extractFunction(
    "currentRemoteForwardingContext",
  );

  expect(source).toContain("let resolvedRemoteForwardingContext = null");
  expect(currentRemoteForwardingContext).toContain("pinnedSummaryShowsLocal()");
  expect(currentRemoteForwardingContext).toContain(
    "resolvedRemoteForwardingContext = null",
  );
});

test("remote discovery failures do not fall back to terminal text scanning", () => {
  const ensurePortScanLoop = extractFunction("ensurePortScanLoop");
  const handleRemotePortDiscoveryFailure = extractFunction("handleRemotePortDiscoveryFailure");

  expect(ensurePortScanLoop).toContain("handleRemotePortDiscoveryFailure(error)");
  expect(handleRemotePortDiscoveryFailure).toContain("ports_remote_discovery_failed");
  expect(ensurePortScanLoop).not.toContain("scanTerminalWebPorts()");
  expect(handleRemotePortDiscoveryFailure).not.toContain("scanTerminalWebPorts()");
});

test("runtime removes stale session ports when discovery no longer reports them", () => {
  expect(source).toContain("function pruneStaleDetectedPorts(");
  expect(source).toContain("discoveredRemotePorts.has(entry.remotePort)");
  expect(source).toContain('bridge("/ports/stop"');
});

test("runtime stops stale active tunnels even without detected cache entries", () => {
  const syncRemoteSessionPortsOnce = extractFunction("syncRemoteSessionPortsOnce");

  expect(source).toContain("function stopStaleForwardedTunnels(");
  expect(source).toContain("function stopDuplicateForwardedTunnels(");
  expect(source).toContain("function activeForwardedPortMap(");
  expect(source).toContain("function discoveredRemotePortSet(");
  expect(source).toContain('ports.some((port) => port.source === "auto")');
  expect(source).toContain('port.source !== "auto"');
  expect(source).toContain('bridge("/ports/list"');
  expect(source).toContain('bridge("/ports/stop"');
  expect(syncRemoteSessionPortsOnce).toContain("stopStaleForwardedTunnels(");
  expect(syncRemoteSessionPortsOnce).toContain("stopDuplicateForwardedTunnels(");
  expect(syncRemoteSessionPortsOnce).toContain(
    "reconcileDiscoveredRemotePorts(",
  );
});

test("terminal evidence gates new candidates but not stale tunnel pruning", () => {
  const syncRemoteSessionPortsOnce = extractFunction("syncRemoteSessionPortsOnce");
  const reconcileDiscoveredRemotePorts = extractFunction(
    "reconcileDiscoveredRemotePorts",
  );

  expect(syncRemoteSessionPortsOnce).toContain(
    "const evidencedPorts = portsWithSessionEvidence(",
  );
  expect(syncRemoteSessionPortsOnce).toContain(
    "const discoveredRemotePorts = discoveredRemotePortSet(discoveredPorts);",
  );
  expect(syncRemoteSessionPortsOnce).toContain(
    "stopStaleForwardedTunnels(\n    context,\n    discoveredRemotePorts,",
  );
  expect(syncRemoteSessionPortsOnce).toContain(
    "reconcileDiscoveredRemotePorts(\n    context,\n    evidencedPorts,\n    activePorts,\n    discoveredRemotePorts,",
  );
  expect(reconcileDiscoveredRemotePorts).toContain(
    "discoveredRemotePorts = discoveredRemotePortSet(ports)",
  );
  expect(reconcileDiscoveredRemotePorts).toContain(
    "pruneStaleDetectedPorts(context, discoveredRemotePorts)",
  );
});

test("remote port lifecycle loop is not gated by pinned summary visibility", () => {
  const ensurePortScanLoop = extractFunction("ensurePortScanLoop");
  const maintainPortsPanelNow = extractFunction("maintainPortsPanelNow");

  expect(ensurePortScanLoop).toContain("!featureSettings.portForwardingEnabled");
  expect(ensurePortScanLoop).toContain("!hasRemoteForwardingContext()");
  expect(ensurePortScanLoop).not.toContain(
    "!findPinnedSummaryCard() && !portsPanelIsVisible()",
  );
  expect(maintainPortsPanelNow).toContain("ensurePortScanLoop();");
  expect(maintainPortsPanelNow.indexOf("ensurePortScanLoop();")).toBeLessThan(
    maintainPortsPanelNow.indexOf("const card = findPinnedSummaryCard();"),
  );
  expect(maintainPortsPanelNow).not.toContain("stopPortScanLoop();");
});

test("runtime keeps managed tunnels outside the active session", () => {
  const syncRemoteSessionPortsOnce = extractFunction("syncRemoteSessionPortsOnce");
  const maintainPortsPanelNow = extractFunction("maintainPortsPanelNow");

  expect(source).toContain("function stopAllManagedPortForwards(");
  expect(syncRemoteSessionPortsOnce).not.toContain(
    "stopForwardedTunnelsOutsideSession(",
  );
  expect(syncRemoteSessionPortsOnce).toContain("activePorts");
  expect(maintainPortsPanelNow).toContain("!featureSettings.portForwardingEnabled");
  expect(maintainPortsPanelNow).toContain("stopAllManagedPortForwards()");
  expect(maintainPortsPanelNow.indexOf("!featureSettings.portForwardingEnabled"))
    .toBeLessThan(
      maintainPortsPanelNow.indexOf("!portForwardingUiAvailable()"),
  );
});

test("remote sync aborts stale async results after session changes", () => {
  const syncRemoteSessionPortsOnce = extractFunction("syncRemoteSessionPortsOnce");

  expect(source).toContain("function contextSessionKey(");
  expect(source).toContain("function remoteForwardingContextChanged(");
  expect(syncRemoteSessionPortsOnce).toContain("const initialSessionKey = contextSessionKey(context);");
  expect(syncRemoteSessionPortsOnce).toContain("remoteForwardingContextChanged(initialSessionKey)");
});

test("remote discovery failures mark current session ports unreachable", () => {
  expect(source).toContain("function handleRemotePortDiscoveryFailure(");
  expect(source).toContain("function markCurrentSessionPortsUnreachable(");
  expect(source).toContain('entry.status = "unreachable"');
  expect(source).toContain("lastDiscoveryFailedAt");
});

test("remote discovery success records session lifecycle state", () => {
  expect(source).toContain("function markRemotePortDiscoverySucceeded(");
  expect(source).toContain("lastDiscoveryOkAt");
  expect(source).toContain("portDiscoveryStates.set");
});

test("runtime reconciles exited tunnel processes with detected port state", () => {
  expect(source).toContain("function reconcileForwardedTunnelList(");
  expect(source).toContain("activePortIds(activePorts)");
  expect(source).toContain("delete entry.id");
  expect(source).toContain("entry.status = \"stopped\"");
});

test("terminal scanner is gated to remote forwarding sessions", () => {
  const scanTerminalWebPorts = extractFunction("scanTerminalWebPorts");

  expect(source).toContain("function hasRemoteForwardingContext()");
  expect(scanTerminalWebPorts).toContain("!hasRemoteForwardingContext()");
});
