(() => {
  if (typeof document === "undefined") return;

  const helperZedAttribute = "data-codex-helper-zed-menu-item";
  const helperUiSelector =
    "[data-codex-helper-settings-entry], [data-codex-helper-zed-menu-item], [data-codex-helper-settings-page]";
  const sidebarThreadSelector = "[data-app-action-sidebar-thread-id]";
  let observerInstalled = false;
  let zedRemoteContextCache = { scope: null, at: 0, value: null };
  const zedRemoteContextCacheTtlMs = 1200;
  const zedRemoteMissingHostMessage = "Cannot determine remote SSH host for this file";

  function bridge(path, payload = {}) {
    if (typeof window.__codexHelperBridge !== "function") {
      return Promise.resolve({
        status: "failed",
        message: "Codex Helper bridge is not installed",
      });
    }
    return window.__codexHelperBridge(path, payload);
  }

  function logDiagnostic(event, detail = {}) {
    bridge("/diagnostics/log", {
      event,
      detail,
      href: window.location.href,
    }).catch((error) => {
      console.warn("[Codex Helper] diagnostic log failed", error);
    });
  }

  function textOf(node) {
    return (node?.textContent || "").replace(/\s+/g, " ").trim();
  }

  function exactText(node, value) {
    return textOf(node) === value;
  }

  function replaceTextNodes(node, from, to) {
    const walker = document.createTreeWalker(node, NodeFilter.SHOW_TEXT);
    const textNodes = [];
    while (walker.nextNode()) textNodes.push(walker.currentNode);
    for (const textNode of textNodes) {
      if ((textNode.nodeValue || "").trim() === from) {
        textNode.nodeValue = (textNode.nodeValue || "").replace(from, to);
      }
    }
  }

  function isHelperUiNode(node) {
    return !!node?.closest?.(helperUiSelector);
  }

  function zedRemoteString(value) {
    return typeof value === "string" || typeof value === "number" ? String(value).trim() : "";
  }

  function zedRemoteTruthy(value) {
    if (value === true) return true;
    if (typeof value === "string") return /^(true|1|yes|enabled|ssh)$/i.test(value.trim());
    return false;
  }

  function zedRemoteHasTrustedSshSignal(source, hostConfig) {
    return zedRemoteTruthy(source?.supportsSsh) || zedRemoteTruthy(hostConfig?.supportsSsh);
  }

  function zedRemoteContextFromObject(source) {
    if (!source || typeof source !== "object") return null;
    const hostConfig = source.hostConfig || source.sshHostConfig || source.remoteHostConfig || source.ssh || {};
    const host = zedRemoteString(
      source.remoteHost ||
      source.sshHost ||
      source.host ||
      source.hostname ||
      source.hostName ||
      hostConfig.host ||
      hostConfig.hostname ||
      hostConfig.hostName ||
      hostConfig.sshHost,
    );
    const hostId = zedRemoteString(source.hostId);
    const cwd = zedRemoteString(
      source.cwd ||
      source.workspaceRoot ||
      source.rootPath ||
      source.remoteWorkspaceRoot ||
      hostConfig.remoteWorkspaceRoot ||
      hostConfig.workspaceRoot ||
      hostConfig.rootPath,
    );
    if (
      (!host || !zedRemoteHasTrustedSshSignal(source, hostConfig)) &&
      !(hostId.startsWith("remote-ssh-") && cwd.startsWith("/"))
    ) {
      return null;
    }
    const user = zedRemoteString(
      source.remoteUser ||
      source.sshUser ||
      source.user ||
      source.username ||
      hostConfig.user ||
      hostConfig.username ||
      hostConfig.sshUser,
    );
    const port = zedRemoteString(
      source.remotePort ||
      source.sshPort ||
      source.port ||
      hostConfig.port ||
      hostConfig.sshPort,
    );
    return { hostId, ssh: { user, host, port }, workspaceRoot: cwd };
  }

  function zedRemoteWalkObject(root, visitor, options = {}) {
    const maxDepth = options.maxDepth || 6;
    const maxNodes = options.maxNodes || 180;
    const visited = new WeakSet();
    const stack = [{ value: root, depth: 0 }];
    let scanned = 0;
    while (stack.length && scanned < maxNodes) {
      const { value, depth } = stack.pop();
      if (!value || typeof value !== "object" || visited.has(value) || depth > maxDepth) continue;
      visited.add(value);
      scanned += 1;
      const result = visitor(value);
      if (result) return result;
      if (
        value instanceof Element ||
        value === window ||
        value === document ||
        value === document.body ||
        value === document.documentElement
      ) {
        continue;
      }
      for (const key of Object.keys(value).slice(0, 80)) {
        if (key === "ownerDocument" || key === "parentElement" || key === "parentNode" || key === "children" || key === "childNodes") {
          continue;
        }
        let child;
        try {
          child = value[key];
        } catch {
          continue;
        }
        if (child && typeof child === "object") stack.push({ value: child, depth: depth + 1 });
      }
    }
    return null;
  }

  function zedRemoteReactKeys(element) {
    return Object.keys(element).filter(
      (key) =>
        key.startsWith("__reactFiber") ||
        key.startsWith("__reactInternalInstance") ||
        key.startsWith("__reactProps"),
    );
  }

  function zedRemoteContextFromElement(element) {
    for (const key of zedRemoteReactKeys(element)) {
      const context = zedRemoteWalkObject(element[key], zedRemoteContextFromObject);
      if (context) return context;
    }
    return null;
  }

  function zedRemoteContextForElement(element) {
    for (let node = element; node && node !== document.body; node = node.parentElement) {
      const context = zedRemoteContextFromElement(node);
      if (context) return context;
    }
    return null;
  }

  function zedRemoteHostIdFromText(text) {
    const match = String(text || "").match(/\bremote-ssh-[A-Za-z0-9:_-]+\b/);
    return match ? match[0] : "";
  }

  function zedRemoteWorkspaceRootForPath(path) {
    const source = String(path || "").trim();
    const projects = Array.from(document.querySelectorAll(sidebarThreadSelector))
      .map((row) => ({
        label: (row.textContent || "").replace(/\s+/g, " ").trim(),
        selected:
          row.getAttribute("aria-current") === "page" ||
          row.getAttribute("data-selected") === "true" ||
          row.getAttribute("data-active") === "true" ||
          String(row.className || "").includes("selected"),
      }))
      .filter((row) => row.label);
    const selected = projects.find((row) => row.selected)?.label || "";
    for (const label of [selected, ...projects.map((row) => row.label)]) {
      const name = label.match(/^([A-Za-z0-9._-]+)/)?.[1];
      if (name && source.includes(`/repo/${name}/`)) {
        return source.slice(0, source.indexOf(`/repo/${name}/`) + `/repo/${name}`.length);
      }
    }
    const repoIndex = source.indexOf("/bin/repo/");
    if (repoIndex >= 0) {
      const afterRepo = source.slice(repoIndex + "/bin/repo/".length);
      const project = afterRepo.split("/")[0];
      if (project) return source.slice(0, repoIndex + "/bin/repo/".length + project.length);
    }
    return source;
  }

  function zedRemoteFallbackContextForElement(element) {
    const pathText = (element.textContent || "").trim();
    if (!pathText.startsWith("/")) return null;
    const root = element.closest("main") || document.body;
    const hostId = zedRemoteHostIdFromText(root?.textContent || "") || "remote-ssh-codex-managed:remote";
    return {
      hostId,
      ssh: { user: "", host: "", port: "" },
      workspaceRoot: zedRemoteWorkspaceRootForPath(pathText),
    };
  }

  function zedRemoteContextFromSerializedState(text) {
    const source = String(text || "");
    if (!source.includes("hostConfig") || !source.includes("supportsSsh") || !source.includes("remoteWorkspaceRoot")) {
      return null;
    }
    const trimmed = source.trim();
    if (/^[{[]/.test(trimmed)) {
      try {
        const parsed = JSON.parse(trimmed);
        const context = zedRemoteWalkObject(parsed, zedRemoteContextFromObject, {
          maxDepth: 10,
          maxNodes: 300,
        });
        if (context) return context;
      } catch {
      }
    }
    if (!/['"]supportsSsh['"]\s*:\s*true/.test(source)) return null;
    const fieldValue = (name) => {
      const match = source.match(new RegExp(`["']${name}["']\\s*:\\s*["']([^"']+)["']`));
      return match ? match[1] : "";
    };
    const host =
      fieldValue("host") ||
      fieldValue("hostname") ||
      fieldValue("hostName") ||
      fieldValue("sshHost") ||
      fieldValue("remoteHost");
    if (!host) return null;
    return {
      ssh: {
        user:
          fieldValue("user") ||
          fieldValue("username") ||
          fieldValue("sshUser") ||
          fieldValue("remoteUser"),
        host,
        port: fieldValue("port") || fieldValue("sshPort") || fieldValue("remotePort"),
      },
      workspaceRoot:
        fieldValue("remoteWorkspaceRoot") ||
        fieldValue("workspaceRoot") ||
        fieldValue("rootPath"),
    };
  }

  function zedRemoteScopedElements(scope, selector) {
    const root = scope?.querySelectorAll ? scope : document;
    const nodes = [];
    if (scope instanceof HTMLElement && scope.matches?.(selector)) nodes.push(scope);
    for (const node of root.querySelectorAll?.(selector) || []) nodes.push(node);
    return Array.from(new Set(nodes));
  }

  function zedRemoteContextFromDataset(node) {
    if (!(node instanceof HTMLElement)) return null;
    const data = node.dataset;
    return zedRemoteContextFromObject({
      hostConfig: data.hostConfig ? { host: data.hostConfig, supportsSsh: true } : {},
      supportsSsh: data.supportsSsh || data.supportsSshRemote,
      sshHost: data.sshHost,
      remoteHost: data.remoteHost,
      host: data.host,
      sshUser: data.sshUser,
      remoteUser: data.remoteUser,
      user: data.user,
      sshPort: data.sshPort,
      remotePort: data.remotePort,
      port: data.port,
      remoteWorkspaceRoot: data.remoteWorkspaceRoot,
      workspaceRoot: data.workspaceRoot,
    });
  }

  function zedRemoteContextUncached(scope = document) {
    const explicitSelector =
      "[data-host-config], [data-ssh-host], [data-remote-host], [data-remote-workspace-root], [data-supports-ssh]";
    for (const node of zedRemoteScopedElements(scope, explicitSelector)) {
      if (isHelperUiNode(node)) continue;
      const context = zedRemoteContextFromDataset(node);
      if (context) return context;
    }
    const reactSelector =
      "[data-remote-path], [data-file-path], [data-path], [data-open-in-targets], [data-open-file], [data-codex-open-file], [role='menuitem']";
    const reactNodes = zedRemoteScopedElements(scope, reactSelector);
    if (scope instanceof HTMLElement && !isHelperUiNode(scope)) reactNodes.unshift(scope);
    for (const node of Array.from(new Set(reactNodes)).slice(0, 60)) {
      if (!(node instanceof HTMLElement) || isHelperUiNode(node)) continue;
      const context = zedRemoteContextFromElement(node);
      if (context) return context;
    }
    if (scope !== document) return null;
    const scripts = Array.from(
      document.querySelectorAll(
        "script[type='application/json'], script[data-state], script#__NEXT_DATA__, script:not([src])",
      ),
    );
    for (const script of scripts.slice(0, 20)) {
      const context = zedRemoteContextFromSerializedState(script.textContent || "");
      if (context) return context;
    }
    return null;
  }

  function zedRemoteContext(scope = document) {
    const now = Date.now();
    if (
      zedRemoteContextCache.scope === scope &&
      now - zedRemoteContextCache.at < zedRemoteContextCacheTtlMs
    ) {
      return zedRemoteContextCache.value;
    }
    const value = zedRemoteContextUncached(scope);
    zedRemoteContextCache = { scope, at: now, value };
    return value;
  }

  function zedRemoteAbsolutePath(value, workspaceRoot) {
    const text = String(value || "").trim();
    if (!text) return "";
    if (text.startsWith("/")) return text;
    if (workspaceRoot && !text.includes("://") && !text.startsWith("~")) {
      return `${workspaceRoot.replace(/\/+$/, "")}/${text.replace(/^\.\//, "")}`;
    }
    return "";
  }

  function zedRemoteMetadataRemotePath(source) {
    if (!source || typeof source !== "object") return "";
    return zedRemoteString(
      source.remotePath ||
      source.remote_path ||
      source.path ||
      source.filePath ||
      source.file_path ||
      source.openFile?.remotePath ||
      source.openFile?.path,
    );
  }

  function zedRemotePathFromElementMetadata(element) {
    const dataPath = element.dataset.remotePath || element.dataset.filePath || element.dataset.path || "";
    if (dataPath) return dataPath;
    for (const key of zedRemoteReactKeys(element)) {
      const path = zedRemoteWalkObject(element[key], zedRemoteMetadataRemotePath, {
        maxDepth: 6,
        maxNodes: 120,
      });
      if (path) return path;
    }
    return "";
  }

  function zedRemoteInlinePathFromElement(element, context) {
    if (!context?.hostId && !context?.ssh?.host) return "";
    const text = (element.textContent || "").trim();
    if (!text || text.length > 600 || !text.startsWith("/")) return "";
    const path = zedRemoteAbsolutePath(text, context.workspaceRoot || "");
    if (!path) return "";
    if (
      context.workspaceRoot &&
      !path.startsWith(`${context.workspaceRoot.replace(/\/+$/, "")}/`) &&
      path !== context.workspaceRoot
    ) {
      return "";
    }
    return path;
  }

  function zedRemoteAnchorHasOpenFileMetadata(anchor) {
    if (!(anchor instanceof HTMLAnchorElement)) return false;
    if (
      anchor.dataset.remotePath ||
      anchor.dataset.filePath ||
      anchor.dataset.path ||
      anchor.dataset.openInTargets ||
      anchor.dataset.openFile ||
      anchor.dataset.codexOpenFile
    ) {
      return true;
    }
    const label = `${anchor.getAttribute("aria-label") || ""} ${anchor.getAttribute("data-testid") || ""} ${anchor.getAttribute("rel") || ""}`;
    return /open[-_\s]?file|open-in-targets|remote/i.test(label) && !!zedRemotePathFromElementMetadata(anchor);
  }

  function zedRemoteFileCandidates(context, scope = document) {
    const candidates = [];
    const seen = new Set();
    const addCandidate = (node, candidateContext, rawPath) => {
      if (!candidateContext?.ssh?.host && !candidateContext?.hostId) return;
      const path = zedRemoteAbsolutePath(rawPath, candidateContext.workspaceRoot || "");
      if (!path || seen.has(path)) return;
      seen.add(path);
      candidates.push({
        node,
        request: {
          ssh: candidateContext.ssh,
          hostId: candidateContext.hostId || "",
          path,
        },
      });
    };
    const selectors =
      "[data-remote-path], [data-file-path], [data-path], [data-open-in-targets], [data-open-file], [data-codex-open-file], a[data-remote-path], a[data-file-path], a[data-path]";
    zedRemoteScopedElements(scope, selectors).forEach((node) => {
      if (!(node instanceof HTMLElement) || isHelperUiNode(node)) return;
      if (node instanceof HTMLAnchorElement && !zedRemoteAnchorHasOpenFileMetadata(node)) return;
      addCandidate(
        node,
        zedRemoteContextForElement(node) || context,
        zedRemotePathFromElementMetadata(node),
      );
    });
    if (scope !== document) {
      zedRemoteScopedElements(scope, "span.inline-markdown, code, [class*='inlineMarkdown']").forEach((node) => {
        if (!(node instanceof HTMLElement) || isHelperUiNode(node)) return;
        const candidateContext =
          zedRemoteContextForElement(node) || context || zedRemoteFallbackContextForElement(node);
        if (!candidateContext?.hostId && !candidateContext?.ssh?.host) return;
        const path = zedRemoteInlinePathFromElement(node, candidateContext);
        if (path) addCandidate(node, candidateContext, path);
      });
    }
    return candidates;
  }

  function zedRemoteBestOpenRequest(scope = document, context = zedRemoteContext(scope) || zedRemoteContext(document) || {}) {
    const candidates = zedRemoteFileCandidates(context, scope);
    if (candidates.length) return candidates[0].request;
    const workspaceRoot = zedRemoteAbsolutePath(context.workspaceRoot || "", "");
    if (!workspaceRoot || (!context?.ssh?.host && !context?.hostId)) return null;
    return { ssh: context.ssh, hostId: context.hostId || "", path: workspaceRoot };
  }

  function selectedAttributeValue(attribute) {
    const nodes = Array.from(document.querySelectorAll(`[${attribute}]`));
    const visible = nodes
      .filter((node) => node instanceof HTMLElement)
      .map((node) => ({
        node,
        rect: node.getBoundingClientRect(),
        value: node.getAttribute(attribute) || "",
      }))
      .filter((item) => item.rect.width > 0 && item.rect.height > 0);
    const active = visible.find((item) => {
      const aria = item.node.getAttribute("aria-selected");
      const state = item.node.getAttribute("data-state");
      const className = String(item.node.className || "");
      return aria === "true" || state === "active" || /selected|active/.test(className);
    });
    return (active || visible[0])?.value || "";
  }

  function remoteContextFromSidebar() {
    const hostId = selectedAttributeValue("data-app-action-sidebar-thread-host-id");
    const projectPath = selectedAttributeValue("data-app-action-sidebar-project-list-id");
    return {
      hostId: hostId && hostId !== "local" ? hostId : "",
      path: projectPath.startsWith("/") ? projectPath : "",
    };
  }

  async function resolveZedRemoteHost(hostId) {
    const result = await bridge("/zed-remote/resolve-host", { hostId });
    return result?.status === "ok" && result.ssh ? result.ssh : null;
  }

  async function resolveZedRemoteFallbackRequest() {
    const result = await bridge("/zed-remote/fallback-request", {});
    return result?.status === "ok" && result.request ? result.request : null;
  }

  async function zedOpenRequestFromContext(scope = document) {
    const request = zedRemoteBestOpenRequest(scope);
    if (request) return { status: "ok", request };

    const sidebar = remoteContextFromSidebar();
    if (sidebar.hostId && sidebar.path) {
      const resolved = await bridge("/zed-remote/resolve-host", { hostId: sidebar.hostId });
      if (resolved?.status !== "ok") return resolved;
      return {
        status: "ok",
        request: {
          hostId: sidebar.hostId,
          ssh: resolved.ssh,
          path: sidebar.path,
        },
      };
    }

    const fallback = await resolveZedRemoteFallbackRequest();
    if (!fallback) {
      return {
        status: "failed",
        message: "Cannot determine remote workspace or file for Zed",
      };
    }
    return { status: "ok", request: fallback };
  }

  async function openZedRemote(request) {
    let nextRequest = request;
    if (!nextRequest?.ssh?.host && nextRequest?.hostId) {
      const ssh = await resolveZedRemoteHost(nextRequest.hostId);
      nextRequest = ssh ? { ...nextRequest, ssh } : nextRequest;
    }
    if (!nextRequest?.ssh?.host) {
      throw new Error(zedRemoteMissingHostMessage);
    }
    const result = await bridge("/zed-remote/open", nextRequest);
    if (result?.status !== "ok") {
      throw new Error(result?.message || "Cannot open Zed remote target");
    }
    return result;
  }

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

  async function openCurrentRemoteInZed(menuItem) {
    menuItem.setAttribute("aria-disabled", "true");
    const originalText = menuItem.textContent;
    menuItem.dataset.codexHelperOriginalText = originalText || "Zed";
    if (originalText === "Zed") replaceTextNodes(menuItem, "Zed", "Opening Zed");

    const scope = menuItem.closest('[role="menu"]') || document;
    const requestResult = await zedOpenRequestFromContext(scope);
    if (requestResult?.status !== "ok" || !requestResult.request) {
      throw new Error(requestResult?.message || "Cannot build Zed remote open request");
    }

    const openResult = await openZedRemote(requestResult.request);
    logDiagnostic("zed_remote_opened", {
      url: openResult.url,
      hostId: requestResult.request.hostId,
      path: requestResult.request.path,
    });
    replaceTextNodes(menuItem, "Opening Zed", "Zed");
    menuItem.removeAttribute("aria-disabled");
    closeOpenMenus();
  }

  function installZedMenuItem(menu) {
    if (!(menu instanceof HTMLElement)) return false;
    if (menu.querySelector(`[${helperZedAttribute}]`)) return true;
    if (!textOf(menu).includes("Cursor") || textOf(menu).includes("Zed")) return false;
    const cursorItem = Array.from(menu.querySelectorAll("[role='menuitem'], div, button")).find(
      (node) => node instanceof HTMLElement && exactText(node, "Cursor"),
    );
    if (!(cursorItem instanceof HTMLElement)) return false;

    const item = cursorItem.cloneNode(true);
    if (!(item instanceof HTMLElement)) return false;
    item.setAttribute(helperZedAttribute, "true");
    item.setAttribute("role", "menuitem");
    item.removeAttribute("data-highlighted");
    replaceTextNodes(item, "Cursor", "Zed");
    const image = item.querySelector("img");
    if (image instanceof HTMLImageElement) {
      image.src = "apps/zed.png";
      image.alt = "Zed";
    }
    item.addEventListener(
      "click",
      (event) => {
        event.preventDefault();
        event.stopPropagation();
        openCurrentRemoteInZed(item).catch((error) => {
          item.removeAttribute("aria-disabled");
          logDiagnostic("zed_remote_open_failed", { error: error?.message || String(error) });
          item.textContent = `Zed failed: ${error?.message || String(error)}`;
          console.error("[Codex Helper] Zed remote open failed", error);
        });
      },
      true,
    );
    cursorItem.insertAdjacentElement("afterend", item);
    logDiagnostic("zed_menu_item_injected", {});
    return true;
  }

  function installZedMenuItems() {
    const menus = Array.from(document.querySelectorAll("[role='menu']"));
    for (const menu of menus) installZedMenuItem(menu);
  }

  function installObserver() {
    if (observerInstalled) return;
    observerInstalled = true;
    const observer = new MutationObserver(() => {
      installZedMenuItems();
    });
    observer.observe(document.documentElement, {
      childList: true,
      subtree: true,
      attributes: true,
      attributeFilter: ["role", "aria-selected", "data-state", "class"],
    });
  }

  installZedMenuItems();
  installObserver();
})();
