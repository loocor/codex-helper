// Session context menu, actions, project forks, and toast UI
  function enabledSessionActions() {
    const order = ["autoRename", "export", "fork"];
    return order.filter((action) => {
      if (action === "autoRename") return featureSettings.autoRenameMenuEnabled;
      if (action === "export") return featureSettings.markdownExportEnabled;
      if (action === "fork") return featureSettings.sessionMoveEnabled;
      return false;
    });
  }

  function sessionActionEntries(actions, row, remoteProjects = []) {
    const entries = [];
    for (const action of actions) {
      if (action === "fork")
        entries.push(...enabledForkSessionActions(row, remoteProjects));
      else entries.push(action);
    }
    return entries;
  }

  function sessionRowFromTarget(target) {
    return target?.closest?.("[data-app-action-sidebar-thread-id]") || null;
  }

  function sessionRefFromRow(row) {
    const href =
      row.getAttribute("href") ||
      row.querySelector("a")?.getAttribute("href") ||
      "";
    const idMatch =
      href.match(/(?:session|conversation|thread)[=/:-]([A-Za-z0-9_.-]+)/i) ||
      href.match(/([A-Za-z0-9_-]{8,})$/);
    const sessionId =
      row.getAttribute("data-app-action-sidebar-thread-id") ||
      row.getAttribute("data-session-id") ||
      idMatch?.[1] ||
      "";
    const titleNode = row.querySelector(
      "[data-thread-title], .truncate.select-none, .truncate.text-base",
    );
    const rawTitle =
      titleNode?.textContent || row.textContent || "Untitled session";
    const title = rawTitle
      .replace(/\s*(Delete|Export|Move)(\s*(Delete|Export|Move))*$/g, "")
      .trim()
      .slice(0, 160);
    return { session_id: sessionId, title: title || "Untitled session" };
  }

  function trackSessionContextMenu(row) {
    const ref = sessionRefFromRow(row);
    if (!ref.session_id) return;
    const openedAt = Date.now();
    installSessionContextMenuBridge();
    pendingSessionMenuContext = {
      row,
      ref,
      openedAt,
    };
    refreshFeatureSettings().catch((error) => {
      logDiagnostic("session_menu_settings_failed", {
        error: error?.message || String(error),
      });
    });
    setTimeout(() => {
      if (
        pendingSessionMenuContext?.ref?.session_id === ref.session_id &&
        pendingSessionMenuContext?.openedAt === openedAt
      ) {
        clearPendingSessionMenuContext();
      }
    }, SESSION_CONTEXT_MENU_MAX_AGE_MS);
  }

  function clearPendingSessionMenuContext() {
    pendingSessionMenuContext = null;
    if (sessionContextMenuMapRestore) sessionContextMenuMapRestore();
  }

  function codexAppServerHostId(hostId) {
    return sessionRemoteHostId(hostId) || "local";
  }

  function codexThreadId(sessionId) {
    return String(sessionId || "")
      .trim()
      .replace(/^local:/, "")
      .replace(/^remote:/, "");
  }

  function buildHelperSessionMenuModelItems(actions, context, remoteProjects = []) {
    if (actions.length === 0) return [];
    const entries = sessionActionEntries(actions, context.row, remoteProjects);
    if (entries.length === 0) return [];
    const labels = sessionActionMenuLabels();
    const items = [{ type: "separator" }];
    for (const action of entries) {
      const item = {
        id: helperSessionActionId(action),
        nativeLabel: labels[action] || action,
        enabled: true,
        onSelect: () => {
          if (!context?.row?.isConnected || !context?.ref?.session_id) return;
          handleSessionAction(action, context.row, context.ref).catch((error) => {
            showHelperToast(error?.message || String(error));
            logDiagnostic("session_menu_action_failed", {
              action,
              session_id: context.ref.session_id,
              error: error?.message || String(error),
            });
          });
        },
      };
      const icon = helperSessionMenuIcon(action);
      if (icon) item.icon = icon;
      items.push(item);
    }
    return items;
  }

  function hasHelperSessionMenuItem(items) {
    for (const item of items) {
      if (item?.type !== "separator" && helperSessionActionFromId(item?.id)) {
        return true;
      }
    }
    return false;
  }

  function isCodexSessionMenuItemId(id) {
    return (
      id === "toggle-thread-pin" ||
      id === "pin-thread" ||
      id === "unpin-thread" ||
      id === "rename-thread" ||
      id === "archive-thread" ||
      id === "mark-thread-unread" ||
      id === "copy-session-id" ||
      id === "copy-deeplink" ||
      id === "copy-app-link" ||
      id === "copy-conversation-path" ||
      id === "copy-working-directory" ||
      id === "copy-cwd" ||
      id === "copyConversationMarkdown" ||
      id === "openSideChat" ||
      id === "open-in-new-window" ||
      id === "open-thread-new-window" ||
      id === "open-thread-folder" ||
      id === "fork-into-local" ||
      id === "fork-into-same-worktree" ||
      id === "fork-into-worktree"
    );
  }

  function looksLikeCodexSessionMenuItems(items) {
    let sessionActionCount = 0;
    for (const item of items) {
      if (isCodexSessionMenuItemId(item?.id)) sessionActionCount += 1;
    }
    return sessionActionCount >= 2;
  }

  function hasNativeSessionMenuLabels(items) {
    let nativeLabelCount = 0;
    for (const item of items) {
      if (
        isCodexSessionMenuItemId(item?.id) &&
        typeof item?.nativeLabel === "string"
      ) {
        nativeLabelCount += 1;
      }
    }
    return nativeLabelCount >= 2;
  }

  function appendHelperSessionMenuItems(items) {
    const context = pendingSessionMenuContext;
    if (
      !Array.isArray(items) ||
      !context?.row?.isConnected ||
      !context?.ref?.session_id ||
      Date.now() - context.openedAt >= SESSION_CONTEXT_MENU_MAX_AGE_MS
    ) {
      clearPendingSessionMenuContext();
      return;
    }
    const hasHelperItem = hasHelperSessionMenuItem(items);
    if (hasHelperItem) {
      clearPendingSessionMenuContext();
      return;
    }
    if (
      !looksLikeCodexSessionMenuItems(items) ||
      !hasNativeSessionMenuLabels(items)
    ) {
      return;
    }
    const actions = enabledSessionActions();
    const remoteProjects = cachedRemoteProjectMetadataLoaded
      ? cachedRemoteProjectMetadata
      : [];
    const helperItems = buildHelperSessionMenuModelItems(
      actions,
      context,
      remoteProjects,
    );
    if (helperItems.length > 0) items.push(...helperItems);
    clearPendingSessionMenuContext();
  }

  function installSessionContextMenuBridge() {
    if (sessionContextMenuMapRestore) return;
    const originalArrayMap = Array.prototype.map;
    const patchedArrayMap = function patchedArrayMap(callback, thisArg) {
      try {
        appendHelperSessionMenuItems(this);
      } catch (error) {
        clearPendingSessionMenuContext();
        logDiagnostic("session_menu_patch_failed", {
          error: error?.message || String(error),
        });
      }
      return originalArrayMap.call(this, callback, thisArg);
    };
    Object.defineProperty(Array.prototype, "map", {
      value: patchedArrayMap,
      writable: true,
      configurable: true,
    });
    sessionContextMenuMapRestore = () => {
      if (Array.prototype.map === patchedArrayMap) {
        Object.defineProperty(Array.prototype, "map", {
          value: originalArrayMap,
          writable: true,
          configurable: true,
        });
      }
      sessionContextMenuMapRestore = null;
    };
  }

  function displayProjectName(path) {
    if (!path) return "Project";
    const normalized = path.replace(/\\/g, "/");
    const parts = normalized.split("/").filter(Boolean);
    if (parts.length > 0) return parts[parts.length - 1];
    if (/^[0-9a-f-]{36}$/i.test(path)) return "Remote project";
    return path;
  }

  function normalizeWorkspacePath(path) {
    const normalized = String(path || "")
      .trim()
      .replace(/\\/g, "/")
      .replace(/\/+$/, "");
    return normalized || String(path || "").trim();
  }

  function sessionRemoteHostId(value) {
    const hostId = String(value || "").trim();
    return hostId && hostId !== "local" ? hostId : "";
  }

  function isRemoteProjectPath(path) {
    const value = String(path || "");
    return (
      /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i.test(
        value,
      ) || value.startsWith("remote-ssh-")
    );
  }

  async function loadRemoteProjectMetadata() {
    const result = await bridge("/projects/remote-list", {});
    if (result?.status !== "ok") {
      throw new Error(result?.message || "Remote project metadata unavailable");
    }
    return Array.isArray(result.projects) ? result.projects : [];
  }

  async function loadRemoteProjectMetadataOrEmpty() {
    try {
      const projects = await loadRemoteProjectMetadata();
      cachedRemoteProjectMetadata = projects;
      cachedRemoteProjectMetadataLoaded = true;
      return projects;
    } catch (error) {
      logDiagnostic("remote_project_metadata_unavailable", {
        error: error?.message || String(error),
      });
      cachedRemoteProjectMetadata = [];
      cachedRemoteProjectMetadataLoaded = true;
      return [];
    }
  }

  function sessionContextMenuReady() {
    return featureSettingsLoaded && cachedRemoteProjectMetadataLoaded;
  }

  async function prepareSessionContextMenu() {
    await refreshFeatureSettings();
    await loadRemoteProjectMetadataOrEmpty();
  }

  function remoteProjectMetadataById(remoteProjects) {
    const map = new Map();
    for (const project of remoteProjects || []) {
      const id = String(project?.id || "").trim();
      if (!id) continue;
      map.set(id, {
        hostId: sessionRemoteHostId(project?.hostId),
        remotePath: normalizeWorkspacePath(project?.remotePath || ""),
        label: String(project?.label || "").trim(),
      });
    }
    return map;
  }

  function projectsSection() {
    return document.querySelector(
      '[data-app-action-sidebar-section-heading="Projects"]',
    );
  }

  function nativeProjectTargets(remoteProjects = []) {
    const section = projectsSection();
    const targets = [];
    const seen = new Set();
    const remoteById = remoteProjectMetadataById(remoteProjects);
    const addTarget = (path, label, hostId = "") => {
      const rawPath = normalizeWorkspacePath(path);
      const remoteProject = remoteById.get(rawPath);
      const normalized = remoteProject?.remotePath || rawPath;
      const normalizedHostId = sessionRemoteHostId(hostId || remoteProject?.hostId);
      if (!normalized) return;
      const seenKey = `${normalizedHostId || "local"}:${normalized}`;
      if (seen.has(seenKey)) return;
      seen.add(seenKey);
      const remote = Boolean(normalizedHostId) || isRemoteProjectPath(normalized);
      const displayLabel = String(
        label || remoteProject?.label || displayProjectName(normalized),
      );
      targets.push({
        path: normalized,
        label: remote ? `${displayLabel} (Remote)` : displayLabel,
        remote,
        hostId: normalizedHostId,
      });
    };
    for (const row of document.querySelectorAll(
      "[data-app-action-sidebar-project-row]",
    )) {
      if (!(row instanceof HTMLElement)) continue;
      if (section && !section.contains(row)) continue;
      const path = row.getAttribute("data-app-action-sidebar-project-id") || "";
      const label =
        row.getAttribute("data-app-action-sidebar-project-label") ||
        row.getAttribute("aria-label") ||
        "";
      const hostId =
        row.getAttribute("data-app-action-sidebar-project-host-id") ||
        row.getAttribute("data-app-action-sidebar-thread-host-id") ||
        row.getAttribute("data-host-id") ||
        "";
      addTarget(path, label, hostId);
    }
    for (const list of document.querySelectorAll(
      "[data-app-action-sidebar-project-list-id]",
    )) {
      if (!(list instanceof HTMLElement)) continue;
      if (section && !section.contains(list)) continue;
      const path = list.getAttribute("data-app-action-sidebar-project-list-id") || "";
      const hostId =
        list.getAttribute("data-app-action-sidebar-project-host-id") ||
        list.getAttribute("data-app-action-sidebar-thread-host-id") ||
        list.getAttribute("data-host-id") ||
        "";
      addTarget(path, displayProjectName(path), hostId);
    }
    return targets.sort((left, right) =>
      left.label.localeCompare(right.label, undefined, { sensitivity: "base" }),
    );
  }

  function sessionProjectContext(row, remoteProjects = []) {
    const projectList =
      typeof row?.closest === "function"
        ? row.closest("[data-app-action-sidebar-project-list-id]")
        : null;
    const remoteById = remoteProjectMetadataById(remoteProjects);
    const hostId = sessionRemoteHostId(
      row?.getAttribute?.("data-app-action-sidebar-thread-host-id") ||
        row?.getAttribute?.("data-app-action-sidebar-project-host-id") ||
        "",
    );
    const rawPath = normalizeWorkspacePath(
      row?.getAttribute?.("data-app-action-sidebar-thread-cwd") ||
        row?.getAttribute?.("data-app-action-sidebar-project-list-id") ||
        row?.getAttribute?.("data-app-action-sidebar-project-id") ||
        projectList?.getAttribute?.("data-app-action-sidebar-project-list-id") ||
        "",
    );
    const remoteProject = remoteById.get(rawPath);
    const remoteHostId = sessionRemoteHostId(hostId || remoteProject?.hostId);
    const path = remoteProject?.remotePath || rawPath;
    return { hostId: remoteHostId, remote: Boolean(remoteHostId), path };
  }

  function forkActionTargetPredicate(action, context) {
    return (target) => {
      if (action === "forkRemoteProject") return target.remote && !!target.hostId;
      if (action === "forkLocalProject") return !target.remote;
      if (action !== "forkAnotherProject") return false;
      if (context.remote) {
        if (!target.remote) return false;
        if (!context.hostId || target.hostId !== context.hostId) return false;
      } else if (target.remote) {
        return false;
      }
      return !context.path || target.path !== context.path;
    };
  }

  function forkTargetsForAction(action, row, remoteProjects = []) {
    const context = sessionProjectContext(row, remoteProjects);
    return nativeProjectTargets(remoteProjects).filter(
      forkActionTargetPredicate(action, context),
    );
  }

  function enabledForkSessionActions(row, remoteProjects = []) {
    const context = sessionProjectContext(row, remoteProjects);
    const actions = context.remote
      ? ["forkLocalProject", "forkAnotherProject"]
      : ["forkRemoteProject", "forkAnotherProject"];
    return actions.filter(
      (action) => forkTargetsForAction(action, row, remoteProjects).length > 0,
    );
  }

  function forkActionDialogTitle(action, context) {
    if (action === "forkRemoteProject") return "Choose remote project";
    if (action === "forkLocalProject") return "Choose local project";
    return context.remote
      ? "Choose another remote project"
      : "Choose another local project";
  }

  function confirmForkSessionAction(ref, target) {
    const title = ref.title || ref.session_id || "Untitled session";
    const destination =
      target?.label || displayProjectName(target?.path || "") || "selected project";
    return window.confirm(`Fork "${title}" into "${destination}"?`);
  }

  function openProjectForkMenu(ref, row, anchorElement, action, remoteProjects = []) {
    installHelperStyles();
    return new Promise((resolve) => {
      document
        .querySelectorAll("[data-codex-helper-project-fork]")
        .forEach((node) => { node.remove(); });
      const overlay = document.createElement("div");
      overlay.setAttribute("data-codex-helper-project-fork", "true");
      const panel = document.createElement("div");
      panel.className = "codex-helper-project-fork-panel";
      panel.setAttribute("role", "dialog");
      panel.setAttribute("aria-modal", "true");
      const context = sessionProjectContext(row, remoteProjects);
      panel.setAttribute("aria-label", forkActionDialogTitle(action, context));
      const header = document.createElement("div");
      header.className = "codex-helper-project-fork-header";
      const title = document.createElement("div");
      title.className = "codex-helper-project-fork-title";
      title.textContent = forkActionDialogTitle(action, context);
      header.appendChild(title);
      const list = document.createElement("div");
      list.className = "codex-helper-project-fork-list";
      panel.appendChild(header);
      panel.appendChild(list);
      overlay.appendChild(panel);
      const anchorRect =
        anchorElement instanceof HTMLElement
          ? anchorElement.getBoundingClientRect()
          : null;
      const panelWidth = Math.min(360, Math.max(240, window.innerWidth - 32));
      if (anchorRect) {
        panel.style.left = `${Math.max(
          16,
          Math.min(
            window.innerWidth - panelWidth - 16,
            anchorRect.right - panelWidth,
          ),
        )}px`;
        panel.style.top = `${Math.max(
          16,
          Math.min(window.innerHeight - 120, anchorRect.bottom + 6),
        )}px`;
      } else {
        panel.style.left = `${Math.max(16, (window.innerWidth - panelWidth) / 2)}px`;
        panel.style.top = "20%";
      }
      const close = (value = null) => {
        overlay.remove();
        resolve(value);
      };
      overlay.addEventListener(
        "click",
        (event) => {
          if (event.target === overlay) close(null);
        },
        true,
      );
      overlay.addEventListener(
        "keydown",
        (event) => {
          if (event.key === "Escape") {
            event.preventDefault();
            event.stopPropagation();
            close(null);
          }
        },
        true,
      );
      document.body.appendChild(overlay);
      const targets = forkTargetsForAction(action, row, remoteProjects);
      list.textContent = "";
      if (targets.length === 0) {
        const empty = document.createElement("div");
        empty.className = "codex-helper-project-fork-empty";
        empty.textContent = "No matching projects found in the sidebar.";
        list.appendChild(empty);
        panel.focus();
        return;
      }
      for (const target of targets) {
        const item = document.createElement("button");
        item.type = "button";
        item.className = "codex-helper-project-fork-item";
        const itemTitle = document.createElement("div");
        itemTitle.className = "codex-helper-project-fork-item-title";
        itemTitle.textContent = target.label;
        const itemPath = document.createElement("div");
        itemPath.className = "codex-helper-project-fork-item-path";
        itemPath.textContent = target.path;
        item.appendChild(itemTitle);
        item.appendChild(itemPath);
        item.addEventListener(
          "click",
          (event) => {
            event.preventDefault();
            event.stopPropagation();
            if (!confirmForkSessionAction(ref, target)) return;
            close(target);
          },
          true,
        );
        list.appendChild(item);
      }
      list.querySelector("button")?.focus();
    });
  }

  function sessionActionMenuLabels() {
    return {
      autoRename: "Regenerate chat title",
      export: "Export Markdown",
      forkRemoteProject: "Fork into remote project...",
      forkLocalProject: "Fork into local project...",
      forkAnotherProject: "Fork into another project...",
    };
  }

  function helperSessionMenuIcon(action) {
    const svgs = {
      export:
        '<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 16 16"><path fill="black" d="M8 2.25v6.5M5.1 6.35 8 9.25l2.9-2.9"/><path fill="black" d="M3.25 12.75h9.5v1H3.25z"/></svg>',
      autoRename:
        '<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 16 16"><path fill="black" d="M8 1.5 9.15 5l3.35 1.15-3.35 1.2L8 10.5 6.85 7.35 3.5 6.15 6.85 5z"/><path fill="black" d="M3.75 10.25h8.5v1.25h-8.5zm0 2.5h6v1.25h-6z"/></svg>',
      forkRemoteProject:
        '<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 16 16"><path fill="black" d="M3 4.5h10v1.25H3zm1.25 2.5h7.5v1.25H4.25zm1.25 2.5h5v1.25H5.5zm1.25 2.5h2.5v1.25H6.75z"/><path fill="black" d="M11.5 3.25 14.25 6l-2.75 2.75V6.5H9.25V5.5h3.75z"/></svg>',
      forkLocalProject:
        '<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 16 16"><path fill="black" d="M3 4.5h10v1.25H3zm1.25 2.5h7.5v1.25H4.25zm1.25 2.5h5v1.25H5.5zm1.25 2.5h2.5v1.25H6.75z"/><path fill="black" d="M11.5 3.25 14.25 6l-2.75 2.75V6.5H9.25V5.5h3.75z"/></svg>',
      forkAnotherProject:
        '<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 16 16"><path fill="black" d="M3 4.5h10v1.25H3zm1.25 2.5h7.5v1.25H4.25zm1.25 2.5h5v1.25H5.5zm1.25 2.5h2.5v1.25H6.75z"/><path fill="black" d="M11.5 3.25 14.25 6l-2.75 2.75V6.5H9.25V5.5h3.75z"/></svg>',
    };
    const svg = svgs[action];
    if (!svg) return undefined;
    return `data:image/svg+xml;charset=utf-8,${encodeURIComponent(svg)}`;
  }

  function helperSessionActionId(action) {
    return `${helperSessionActionPrefix}${action}`;
  }

  function helperSessionActionFromId(id) {
    if (typeof id !== "string" || !id.startsWith(helperSessionActionPrefix))
      return null;
    return id.slice(helperSessionActionPrefix.length) || null;
  }

  function setSessionMenuItemLabel(item, label) {
    const textNodes = [];
    const walker = document.createTreeWalker(item, NodeFilter.SHOW_TEXT);
    while (walker.nextNode()) {
      if ((walker.currentNode.nodeValue || "").trim())
        textNodes.push(walker.currentNode);
    }
    if (textNodes.length > 0) {
      textNodes[0].nodeValue = label;
      for (let index = 1; index < textNodes.length; index += 1)
        textNodes[index].nodeValue = "";
      return;
    }
    const labelNode = document.createElement("span");
    labelNode.textContent = label;
    item.appendChild(labelNode);
  }

  function forkedSessionPath(result, target) {
    if (target?.hostId) return "";
    const sessionId = String(result?.new_session_id || result?.newSessionId || "")
      .replace(/^local:/, "")
      .trim();
    return sessionId ? `/local/${encodeURIComponent(sessionId)}` : "";
  }

  function navigateAfterFork(result, target) {
    const path = forkedSessionPath(result, target);
    if (!path) return;
    window.setTimeout(() => {
      window.location.assign(path);
    }, 0);
  }

  function reactRootFiber() {
    const root =
      document.querySelector("#root") ||
      document.querySelector("[data-reactroot]") ||
      document.body?.firstElementChild;
    if (!root) return null;
    const key = Object.keys(root).find((name) =>
      name.startsWith("__reactContainer$"),
    );
    return key ? root[key] : null;
  }

  function collectSidebarConversationManagers() {
    const rootFiber = reactRootFiber();
    if (!rootFiber) return [];
    const managers = [];
    const seenObjects = new WeakSet();
    let visitedFibers = 0;
    function scanValue(value, depth) {
      if (!value || (typeof value !== "object" && typeof value !== "function"))
        return;
      if (seenObjects.has(value) || depth > 4) return;
      seenObjects.add(value);
      if (
        typeof value.refreshRecentConversations === "function" &&
        typeof value.hostId === "string" &&
        value.hostId.trim()
      ) {
        managers.push(value);
      }
      for (const key of Object.keys(value).slice(0, 50)) {
        if (key.startsWith("_")) continue;
        let child = null;
        try {
          child = value[key];
        } catch {
          continue;
        }
        if (typeof child === "object" || typeof child === "function") {
          scanValue(child, depth + 1);
        }
      }
    }
    function visit(fiber) {
      if (!fiber || visitedFibers > 2000) return;
      visitedFibers += 1;
      scanValue(fiber.memoizedProps, 0);
      scanValue(fiber.memoizedState, 0);
      scanValue(fiber.stateNode, 0);
      visit(fiber.child);
      visit(fiber.sibling);
    }
    visit(rootFiber);
    return [
      ...new Map(
        managers.map((manager) => [manager.hostId.trim(), manager]),
      ).values(),
    ];
  }

  function findSidebarConversationManager(hostId) {
    const normalizedHostId = codexAppServerHostId(hostId);
    return collectSidebarConversationManagers().find(
      (candidate) => candidate.hostId === normalizedHostId,
    ) || null;
  }

  function sidebarRefreshDelay(delayMs) {
    const delay = Number(delayMs || 0);
    if (delay <= 0) return Promise.resolve();
    return new Promise((resolve) => {
      window.setTimeout(resolve, delay);
    });
  }

  function sidebarConversationById(manager, sessionId) {
    const threadId = codexThreadId(sessionId);
    if (!threadId || typeof manager?.getConversation !== "function") return null;
    return manager.getConversation(threadId) || null;
  }

  function sidebarRecentConversationById(manager, sessionId) {
    const threadId = codexThreadId(sessionId);
    if (!threadId || typeof manager?.getRecentConversations !== "function")
      return null;
    const conversations = manager.getRecentConversations();
    if (!Array.isArray(conversations)) return null;
    return (
      conversations.find((conversation) => codexThreadId(conversation?.id) === threadId) ||
      null
    );
  }

  function normalizeSidebarRefreshExpectation(expectation) {
    if (!expectation || typeof expectation !== "object") return null;
    const conversationId = codexThreadId(expectation.conversationId);
    const title = String(expectation.title || "").trim();
    return {
      conversationId,
      title,
      valid: Boolean(conversationId),
    };
  }

  function sidebarRefreshExpectationMatches(manager, expectation) {
    const normalized = normalizeSidebarRefreshExpectation(expectation);
    if (!normalized) return true;
    if (!normalized.conversationId) return true;
    const conversation =
      sidebarConversationById(manager, normalized.conversationId) ||
      sidebarRecentConversationById(manager, normalized.conversationId);
    if (!conversation) return false;
    if (!normalized.title) return true;
    return String(conversation.title || "").trim() === normalized.title;
  }

  async function refreshSidebarStateForHost(hostId, expectation, options) {
    const normalizedHostId = codexAppServerHostId(hostId);
    const manager = findSidebarConversationManager(normalizedHostId);
    const normalizedExpectation = normalizeSidebarRefreshExpectation(expectation);
    if (expectation && !normalizedExpectation?.valid) {
      logDiagnostic("sidebar_refresh_expectation_missing", {
        host_id: normalizedHostId,
        title: normalizedExpectation?.title || "",
      });
      return {
        attempts: 0,
        hostId: normalizedHostId,
        managerFound: Boolean(manager),
        ok: false,
        verified: false,
      };
    }
    if (!manager) {
      logDiagnostic("sidebar_refresh_manager_missing", {
        host_id: normalizedHostId,
      });
      return {
        attempts: 0,
        hostId: normalizedHostId,
        managerFound: false,
        ok: false,
        verified: false,
      };
    }
    const retryDelays =
      Array.isArray(options?.retryDelays) && options.retryDelays.length > 0
        ? options.retryDelays
        : [0, 300, 900, 1800];
    let lastError = null;
    let lastAttemptFailed = false;
    let refreshErrorCount = 0;
    for (let index = 0; index < retryDelays.length; index += 1) {
      await sidebarRefreshDelay(retryDelays[index]);
      try {
        await manager.refreshRecentConversations({ sortKey: "updated_at" });
        lastAttemptFailed = false;
      } catch (error) {
        lastError = error;
        lastAttemptFailed = true;
        refreshErrorCount += 1;
      }
      if (
        !lastAttemptFailed &&
        sidebarRefreshExpectationMatches(manager, normalizedExpectation)
      ) {
        return {
          attempts: index + 1,
          hostId: normalizedHostId,
          managerFound: true,
          ok: true,
          verified: true,
        };
      }
    }
    if (lastAttemptFailed) {
      logDiagnostic("sidebar_refresh_failed", {
        host_id: normalizedHostId,
        message: lastError?.message || String(lastError),
      });
    } else {
      logDiagnostic("sidebar_refresh_unverified", {
        host_id: normalizedHostId,
        session_id: normalizedExpectation?.conversationId || "",
        title: normalizedExpectation?.title || "",
        refresh_error_count: refreshErrorCount,
        last_error: lastError?.message || "",
      });
    }
    return {
      attempts: retryDelays.length,
      hostId: normalizedHostId,
      managerFound: true,
      ok: false,
      verified: false,
    };
  }

  async function refreshSidebarConversationsForHost(hostId) {
    const result = await refreshSidebarStateForHost(hostId);
    return result.ok;
  }

  function sidebarThreadRowsBySessionId(row, sessionId) {
    const threadId = codexThreadId(sessionId);
    if (!threadId) return [];
    const rows = [];
    const seen = new Set();
    const addRow = (candidate) => {
      if (!(candidate instanceof HTMLElement) || seen.has(candidate)) return;
      const candidateId = codexThreadId(
        candidate.getAttribute("data-app-action-sidebar-thread-id") ||
          candidate.getAttribute("data-session-id") ||
          "",
      );
      if (candidateId !== threadId) return;
      seen.add(candidate);
      rows.push(candidate);
    };
    if (typeof document?.querySelectorAll === "function") {
      for (const candidate of document.querySelectorAll(
        "[data-app-action-sidebar-thread-id], [data-session-id]",
      )) {
        addRow(candidate);
      }
    }
    addRow(row);
    return rows;
  }

  function setSidebarConversationTitleInDom(row, sessionId, title) {
    const threadId = codexThreadId(sessionId);
    const name = String(title || "").trim();
    if (!threadId || !name) return false;
    let updatedCount = 0;
    for (const candidate of sidebarThreadRowsBySessionId(row, threadId)) {
      const titleNode = candidate.querySelector(
        "[data-thread-title], .truncate.select-none, .truncate.text-base",
      );
      if (!(titleNode instanceof HTMLElement)) continue;
      titleNode.textContent = name;
      if (typeof titleNode.setAttribute === "function") {
        titleNode.setAttribute("title", name);
      }
      updatedCount += 1;
    }
    logDiagnostic(
      updatedCount > 0
        ? "sidebar_title_dom_updated"
        : "sidebar_title_dom_missing",
      {
        session_id: threadId,
        title: name,
        updated_count: updatedCount,
      },
    );
    return updatedCount > 0;
  }

  async function setSidebarConversationTitleForHost(hostId, sessionId, title) {
    const normalizedHostId = codexAppServerHostId(hostId);
    const threadId = codexThreadId(sessionId);
    const name = String(title || "").trim();
    if (!threadId || !name) return false;
    const manager = findSidebarConversationManager(normalizedHostId);
    if (!manager) {
      logDiagnostic("sidebar_title_manager_missing", {
        host_id: normalizedHostId,
        session_id: threadId,
      });
      return false;
    }
    try {
      const conversation =
        typeof manager.getConversation === "function"
          ? manager.getConversation(threadId)
          : null;
      const recentConversation =
        conversation || sidebarRecentConversationById(manager, threadId);
      if (!recentConversation) {
        logDiagnostic("sidebar_title_conversation_missing", {
          host_id: normalizedHostId,
          session_id: threadId,
        });
        return false;
      }
      if (
        typeof manager.applyThreadTitleUpdateAndNotify === "function"
      ) {
        manager.applyThreadTitleUpdateAndNotify({
          ...recentConversation,
          title: name,
        });
        logDiagnostic("sidebar_title_update_applied", {
          host_id: normalizedHostId,
          session_id: threadId,
          source: conversation ? "conversation" : "recent",
          title: name,
        });
        return true;
      }
      logDiagnostic("sidebar_title_update_unavailable", {
        host_id: normalizedHostId,
        session_id: threadId,
      });
      return false;
    } catch (error) {
      logDiagnostic("sidebar_title_update_failed", {
        host_id: normalizedHostId,
        session_id: threadId,
        message: error?.message || String(error),
      });
      return false;
    }
  }

  async function refreshSidebarAfterFork(target, result) {
    const sessionId = String(result?.new_session_id || result?.newSessionId || "");
    return await refreshSidebarStateForHost(target?.hostId || "", {
      conversationId: sessionId,
    });
  }

  function autoNamingRangePayload() {
    return {
      autoNamingMinChars: featureSettings.autoNamingMinChars,
      autoNamingMaxChars: featureSettings.autoNamingMaxChars,
    };
  }

  async function handleSessionAction(action, row, ref) {
    if (action === "autoRename") {
      const context = sessionProjectContext(row);
      const payload = {
        ...ref,
        host_id: context.hostId,
        ...autoNamingRangePayload(),
      };
      const finishTaskToast = showHelperTaskToast("Regenerating chat title...");
      const result = await bridge("/auto-rename-chat", payload);
      if (result?.status !== "renamed") {
        logDiagnostic("auto_rename_chat_failed", {
          session_id: ref.session_id,
          message: result?.message || "Auto rename failed",
        });
        throw new Error(result?.message || "Auto rename failed");
      }
      logDiagnostic("auto_rename_chat_succeeded", {
        session_id: ref.session_id,
        name: result.name || "",
        source: result.source || "",
      });
      finishTaskToast(result.message || "Regenerated chat title");
      await refreshSidebarStateForHost(context.hostId, {
        conversationId: ref.session_id,
      });
      await setSidebarConversationTitleForHost(
        context.hostId,
        ref.session_id,
        result.name || "",
      );
      setSidebarConversationTitleInDom(row, ref.session_id, result.name || "");
      return;
    }
    if (action === "export") {
      const context = sessionProjectContext(row);
      const finishTaskToast = showHelperTaskToast("Exporting Markdown...");
      const result = await bridge("/export-markdown", {
        ...ref,
        host_id: context.hostId,
        friendlyFilename: featureSettings.markdownFriendlyFilenameEnabled,
        ...autoNamingRangePayload(),
      });
      if (
        result?.status !== "exported" ||
        typeof result.markdown !== "string" ||
        !result.filename
      ) {
        if (featureSettings.markdownFriendlyFilenameEnabled) {
          logDiagnostic("markdown_friendly_filename_failed", {
            session_id: ref.session_id,
            message: result?.message || "Export failed",
          });
        }
        throw new Error(result?.message || "Export failed");
      }
      if (featureSettings.markdownFriendlyFilenameEnabled) {
        logDiagnostic("markdown_friendly_filename_succeeded", {
          session_id: ref.session_id,
          filename: result.filename,
        });
      }
      downloadMarkdown(result.filename, result.markdown);
      finishTaskToast(result.message || "Exported");
      return;
    }
    if (action.startsWith("fork")) {
      const remoteProjects = await loadRemoteProjectMetadataOrEmpty();
      const target = await openProjectForkMenu(
        ref,
        row,
        row,
        action,
        remoteProjects,
      );
      if (!target) return;
      const context = sessionProjectContext(row, remoteProjects);
      const finishTaskToast = showHelperTaskToast("Forking conversation...");
      const result = await bridge("/fork-thread-project", {
        ...ref,
        source_host_id: context.hostId,
        source_cwd: context.path,
        target_host_id: target.hostId || "",
        target_cwd: target.path,
        target_name: ref.title,
      });
      if (result?.status !== "forked")
        throw new Error(result?.message || "Fork failed");
      finishTaskToast(result.warning || result.message || "Forked");
      await refreshSidebarAfterFork(target, result);
      navigateAfterFork(result, target);
      return;
    }
  }

  function downloadMarkdown(filename, markdown) {
    const blob = new Blob([markdown], { type: "text/markdown;charset=utf-8" });
    const url = URL.createObjectURL(blob);
    const link = document.createElement("a");
    link.href = url;
    link.download = filename;
    document.body.appendChild(link);
    link.click();
    link.remove();
    setTimeout(() => URL.revokeObjectURL(url), 1000);
  }

  function showHelperToast(message, options) {
    const toastOptions = options || {};
    document.querySelectorAll(`[${helperToastAttribute}]`).forEach((node) => {
      node.remove();
    });
    const toast = document.createElement("div");
    toast.setAttribute(helperToastAttribute, "true");
    toast.setAttribute("role", "status");
    toast.setAttribute("aria-live", "polite");
    if (toastOptions.loading) {
      toast.setAttribute("data-codex-helper-toast-state", "loading");
      const spinner = document.createElement("span");
      spinner.className = "codex-helper-toast-spinner";
      spinner.setAttribute("aria-hidden", "true");
      const label = document.createElement("span");
      label.textContent = message;
      toast.replaceChildren(spinner, label);
    } else {
      toast.textContent = message;
    }
    document.body.appendChild(toast);
    if (!toastOptions.persist) setTimeout(() => toast.remove(), 8000);
  }

  function showHelperTaskToast(message) {
    showHelperToast(message, { loading: true, persist: true });
    return (finalMessage) => showHelperToast(finalMessage);
  }
