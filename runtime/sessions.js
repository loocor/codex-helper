// Session context menu, actions, move, and toast UI
  function enabledSessionActions() {
    const order = ["export", "move", "delete"];
    return order.filter((action) => {
      if (action === "delete") return featureSettings.sessionDeleteEnabled;
      if (action === "export") return featureSettings.markdownExportEnabled;
      if (action === "move") return featureSettings.sessionMoveEnabled;
      return false;
    });
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
        pendingSessionMenuContext = null;
        if (sessionContextMenuMapRestore) sessionContextMenuMapRestore();
      }
    }, 2500);
  }

  function clearPendingSessionMenuContext() {
    pendingSessionMenuContext = null;
    if (sessionContextMenuMapRestore) sessionContextMenuMapRestore();
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

  function isRemoteProjectPath(path) {
    const value = String(path || "");
    return (
      /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i.test(
        value,
      ) || value.startsWith("remote-ssh-")
    );
  }

  function projectsSection() {
    return document.querySelector(
      '[data-app-action-sidebar-section-heading="Projects"]',
    );
  }

  function nativeProjectTargets() {
    const section = projectsSection();
    const targets = [];
    const seen = new Set();
    const addTarget = (path, label) => {
      const normalized = normalizeWorkspacePath(path);
      if (!normalized || seen.has(normalized)) return;
      seen.add(normalized);
      const remote = isRemoteProjectPath(normalized);
      const displayLabel = String(label || displayProjectName(path));
      targets.push({
        path: normalized,
        label: remote ? `${displayLabel} (Remote)` : displayLabel,
        remote,
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
      addTarget(path, label);
    }
    for (const list of document.querySelectorAll(
      "[data-app-action-sidebar-project-list-id]",
    )) {
      if (!(list instanceof HTMLElement)) continue;
      if (section && !section.contains(list)) continue;
      const path = list.getAttribute("data-app-action-sidebar-project-list-id") || "";
      addTarget(path, displayProjectName(path));
    }
    return targets.sort((left, right) =>
      left.label.localeCompare(right.label, undefined, { sensitivity: "base" }),
    );
  }

  function confirmMoveSession(ref, target) {
    const title = ref.title || ref.session_id || "Untitled session";
    const destination =
      target?.label || displayProjectName(target?.path || "") || "selected project";
    return window.confirm(`Move "${title}" to "${destination}"?`);
  }

  function openProjectMoveMenu(ref, anchorElement) {
    installHelperStyles();
    return new Promise((resolve) => {
      document
        .querySelectorAll("[data-codex-helper-project-move]")
        .forEach((node) => { node.remove(); });
      const overlay = document.createElement("div");
      overlay.setAttribute("data-codex-helper-project-move", "true");
      const panel = document.createElement("div");
      panel.className = "codex-helper-project-move-panel";
      panel.setAttribute("role", "dialog");
      panel.setAttribute("aria-modal", "true");
      panel.setAttribute("aria-label", "Move session");
      const header = document.createElement("div");
      header.className = "codex-helper-project-move-header";
      const title = document.createElement("div");
      title.className = "codex-helper-project-move-title";
      title.textContent = `Move "${ref.title || ref.session_id}"`;
      header.appendChild(title);
      const list = document.createElement("div");
      list.className = "codex-helper-project-move-list";
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
      const targets = nativeProjectTargets();
      list.textContent = "";
      if (targets.length === 0) {
        const empty = document.createElement("div");
        empty.className = "codex-helper-project-move-empty";
        empty.textContent = "No projects found in the sidebar.";
        list.appendChild(empty);
        panel.focus();
        return;
      }
      for (const target of targets) {
        const item = document.createElement("button");
        item.type = "button";
        item.className = "codex-helper-project-move-item";
        const itemTitle = document.createElement("div");
        itemTitle.className = "codex-helper-project-move-item-title";
        itemTitle.textContent = target.label;
        const itemPath = document.createElement("div");
        itemPath.className = "codex-helper-project-move-item-path";
        itemPath.textContent = target.path;
        item.appendChild(itemTitle);
        item.appendChild(itemPath);
        item.addEventListener(
          "click",
          (event) => {
            event.preventDefault();
            event.stopPropagation();
            if (!confirmMoveSession(ref, target)) return;
            close(target.path);
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
      delete: "Delete Session",
      export: "Export Markdown",
      move: "Move Session",
    };
  }

  function helperSessionMenuIcon(action) {
    const svgs = {
      export:
        '<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 16 16"><path fill="black" d="M8 2.25v6.5M5.1 6.35 8 9.25l2.9-2.9"/><path fill="black" d="M3.25 12.75h9.5v1H3.25z"/></svg>',
      move:
        '<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 16 16"><path fill="black" d="M3 4.5h10v1.25H3zm1.25 2.5h7.5v1.25H4.25zm1.25 2.5h5v1.25H5.5zm1.25 2.5h2.5v1.25H6.75z"/><path fill="black" d="M11.5 3.25 14.25 6l-2.75 2.75V6.5H9.25V5.5h3.75z"/></svg>',
      delete:
        '<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 16 16"><path fill="black" d="M6 2.5h4l.5 1H13v1H3V3.5h2.5zm-.5 3h1v6.5H5.5zm3 0h1v6.5H8.5zm3 0h1v6.5h-1z"/></svg>',
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

  function buildHelperSessionMenuModelItems(actions, context) {
    if (actions.length === 0) return [];
    const labels = sessionActionMenuLabels();
    const items = [{ type: "separator" }];
    for (const action of actions) {
      const item = {
        id: helperSessionActionId(action),
        nativeLabel: labels[action] || action,
        enabled: true,
        onSelect: () => {
          handleSessionAction(action, context.row, context.ref).catch((error) => {
            showHelperToast(error?.message || String(error));
            logDiagnostic("session_menu_action_failed", {
              action,
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
      if (
        item?.type !== "separator" &&
        helperSessionActionFromId(item?.id)
      ) {
        return true;
      }
    }
    return false;
  }

  function isCodexSessionMenuItemId(id) {
    return (
      id === "toggle-thread-pin" ||
      id === "pin-thread" ||
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
      Date.now() - context.openedAt >= 2500
    ) {
      clearPendingSessionMenuContext();
      return;
    }
    if (
      hasHelperSessionMenuItem(items) ||
      !looksLikeCodexSessionMenuItems(items) ||
      !hasNativeSessionMenuLabels(items)
    ) {
      if (hasHelperSessionMenuItem(items)) clearPendingSessionMenuContext();
      return;
    }
    const actions = enabledSessionActions();
    if (actions.length > 0)
      items.push(...buildHelperSessionMenuModelItems(actions, context));
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

  function rowHref(row) {
    const href =
      row.getAttribute("href") ||
      row.querySelector("a[href]")?.getAttribute("href") ||
      "";
    if (!href) return "";
    try {
      return new URL(href, window.location.href).href;
    } catch (_) {
      return href;
    }
  }

  function isCurrentSessionRow(row, ref) {
    const activeNode = row.closest(
      '[aria-current="page"], [aria-selected="true"], [data-state="active"]',
    );
    if (activeNode instanceof HTMLElement) return true;
    const href = rowHref(row);
    if (href && href === window.location.href) return true;
    return !!ref.session_id && window.location.href.includes(ref.session_id);
  }

  function visibleSessionRows(root = document) {
    return Array.from(
      root.querySelectorAll("[data-app-action-sidebar-thread-id]"),
    ).filter((candidate) => {
      if (!(candidate instanceof HTMLElement)) return false;
      const rect = candidate.getBoundingClientRect();
      return rect.width > 0 && rect.height > 0;
    });
  }

  function nearestSessionRowContainer(row) {
    let node = row.parentElement;
    while (node && node !== document.body) {
      if (visibleSessionRows(node).length > 1) return node;
      node = node.parentElement;
    }
    return null;
  }

  function findReplacementSessionRow(deletedRow) {
    const inSameContainer = visibleSessionRows(
      nearestSessionRowContainer(deletedRow) || document,
    ).filter((row) => row !== deletedRow);
    const allRows = visibleSessionRows(document).filter(
      (row) => row !== deletedRow,
    );
    const candidates = inSameContainer.length ? inSameContainer : allRows;
    if (candidates.length === 0) return null;
    const allBeforeDelete = visibleSessionRows(document);
    const deletedIndex = allBeforeDelete.indexOf(deletedRow);
    if (deletedIndex >= 0) {
      const after = candidates.find(
        (row) => allBeforeDelete.indexOf(row) > deletedIndex,
      );
      if (after) return after;
      const before = [...candidates]
        .reverse()
        .find((row) => allBeforeDelete.indexOf(row) < deletedIndex);
      if (before) return before;
    }
    return candidates[0] || null;
  }

  function clickElement(element) {
    if (!(element instanceof HTMLElement)) return false;
    element.dispatchEvent(
      new MouseEvent("pointerdown", { bubbles: true, cancelable: true }),
    );
    element.dispatchEvent(
      new MouseEvent("mousedown", { bubbles: true, cancelable: true }),
    );
    element.click();
    return true;
  }

  function navigateToSessionRow(row) {
    if (!(row instanceof HTMLElement)) return false;
    const link = row.matches("a[href]") ? row : row.querySelector("a[href]");
    if (clickElement(link instanceof HTMLElement ? link : row)) return true;
    const href = rowHref(row);
    if (!href) return false;
    window.location.assign(href);
    return true;
  }

  function openNewChat() {
    const control = Array.from(
      document.querySelectorAll("a, button, [role='button']"),
    ).find((node) => {
      if (!(node instanceof HTMLElement)) return false;
      const rect = node.getBoundingClientRect();
      return rect.width > 0 && rect.height > 0 && exactText(node, "New chat");
    });
    return clickElement(control instanceof HTMLElement ? control : null);
  }

  function navigateAfterDeletedCurrentSession(replacementRow) {
    window.setTimeout(() => {
      if (navigateToSessionRow(replacementRow)) return;
      if (openNewChat()) return;
      logDiagnostic("delete_navigation_target_not_found", {});
    }, 0);
  }

  async function handleSessionAction(action, row, ref) {
    if (action === "delete") {
      if (!window.confirm(`Delete "${ref.title || ref.session_id}"?`)) return;
      const deletingCurrentSession = isCurrentSessionRow(row, ref);
      const replacementRow = deletingCurrentSession
        ? findReplacementSessionRow(row)
        : null;
      const result = await bridge("/delete", ref);
      if (
        result?.status === "server_deleted" ||
        result?.status === "local_deleted"
      ) {
        row.remove();
        if (deletingCurrentSession)
          navigateAfterDeletedCurrentSession(replacementRow);
        showHelperToast(result.message || "Deleted", result.undo_token);
        return;
      }
      throw new Error(result?.message || "Delete failed");
    }
    if (action === "export") {
      const result = await bridge("/export-markdown", ref);
      if (
        result?.status !== "exported" ||
        typeof result.markdown !== "string" ||
        !result.filename
      ) {
        throw new Error(result?.message || "Export failed");
      }
      downloadMarkdown(result.filename, result.markdown);
      showHelperToast(result.message || "Exported");
      return;
    }
    if (action === "move") {
      const target = await openProjectMoveMenu(ref, row);
      if (!target) return;
      const result = await bridge("/move-thread-workspace", {
        ...ref,
        target_cwd: target,
      });
      if (result?.status !== "moved")
        throw new Error(result?.message || "Move failed");
      showHelperToast(result.message || "Moved");
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

  function showHelperToast(message, undoToken) {
    document.querySelectorAll(`[${helperToastAttribute}]`).forEach((node) => {
      node.remove();
    });
    const toast = document.createElement("div");
    toast.setAttribute(helperToastAttribute, "true");
    toast.textContent = message;
    if (undoToken) {
      const undo = document.createElement("button");
      undo.type = "button";
      undo.textContent = "Undo";
      undo.addEventListener("click", async () => {
        const result = await bridge("/undo", { undo_token: undoToken });
        toast.textContent = result?.message || "Undo complete";
      });
      toast.appendChild(undo);
    }
    document.body.appendChild(toast);
    setTimeout(() => toast.remove(), 8000);
  }
