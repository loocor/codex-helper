// Bridge, diagnostics, and DOM helpers
  function bridge(path, payload = {}) {
    if (typeof window.__codexHelperBridge !== "function") {
      return Promise.resolve({
        status: "failed",
        message: "Codex Helper bridge is not installed",
      });
    }
    return window.__codexHelperBridge(path, payload);
  }

  function helperCallerSnapshot() {
    if (typeof window.__codexHelperCaller === "function") {
      return window.__codexHelperCaller();
    }
    return {
      href: window.location.href,
      hasFocus: document.hasFocus(),
      visibilityState: document.visibilityState || "",
    };
  }

  function helperRuntimeActivityDetail() {
    return helperCallerSnapshot();
  }

  function helperWindowIsPortOwner() {
    return document.hasFocus();
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

  function isVisibleElement(node) {
    const rect = node.getBoundingClientRect();
    return rect.width > 0 && rect.height > 0;
  }

  function restoreNativeSettingsPanels() {
    for (const node of document.querySelectorAll(
      "[data-codex-helper-native-hidden='true']",
    )) {
      if (!(node instanceof HTMLElement)) continue;
      node.hidden = false;
      node.removeAttribute("aria-hidden");
      node.removeAttribute("data-codex-helper-native-hidden");
    }
    const helperPanel = document.getElementById(helperSettingsPanelId);
    if (helperPanel instanceof HTMLElement) {
      helperPanel.remove();
    }
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
