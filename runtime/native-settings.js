// Native Codex Settings Helper group and pages
function nativeHelperSettingsPageDefinitions() {
  return [
    {
      id: "general",
      label: "General",
      standardIconName: "sliders-horizontal",
      description:
        "Configure Helper integrations, session actions, and port forwarding.",
    },
    {
      id: "user-scripts",
      label: "User Scripts",
      standardIconName: "file-code-2",
      description:
        "Manage local user-defined scripts loaded by Codex Helper.",
      hidden: true,
    },
    {
      id: "deleted-chats",
      label: "Deleted chats",
      standardIconName: "trash-2",
      description:
        "Search deleted local chat backups and restore them when needed.",
    },
    {
      id: "logs",
      label: "Logs",
      standardIconName: "scroll-text",
      description:
        "Inspect recent Helper runtime and bridge diagnostics.",
    },
    { id: "about", label: "About", standardIconName: "info" },
  ];
}

function nativeHelperSettingsPages() {
  return nativeHelperSettingsPageDefinitions().filter((page) => !page.hidden);
}

function isCodexSettingsSidebarCandidate(sidebar) {
  if (!(sidebar instanceof HTMLElement) || !isVisibleElement(sidebar))
    return false;
  const text = textOf(sidebar);
  return (
    text.includes("Back to app") &&
    text.includes("General") &&
    text.includes("Appearance") &&
    text.includes("Configuration") &&
    text.includes("Personalization")
  );
}

function findCodexSettingsSidebar() {
  const selector = "aside, nav, [role='navigation'], [role='tablist']";
  return (
    Array.from(document.querySelectorAll(selector)).find((node) =>
      isCodexSettingsSidebarCandidate(node),
    ) || null
  );
}

function nativeSettingsClickableSelector() {
  return "button, a, [role='button'], [role='tab'], [role='menuitem']";
}

function findClickableSettingsItem(sidebar, label) {
  if (!(sidebar instanceof HTMLElement)) return null;
  return (
    Array.from(sidebar.querySelectorAll(nativeSettingsClickableSelector())).find(
      (node) => {
        if (!(node instanceof HTMLElement)) return false;
        if (node.closest(`[${helperNativeSettingsGroupAttribute}]`)) return false;
        if (!exactText(node, label)) return false;
        const rect = node.getBoundingClientRect();
        return rect.width > 40 && rect.height > 12;
      },
    ) || null
  );
}

function cloneNativeSettingsEntry(sidebar, label, pageId) {
  const template =
    findClickableSettingsItem(sidebar, "Configuration") ||
    findClickableSettingsItem(sidebar, "General");
  const entry = template?.cloneNode(true);
  if (!(entry instanceof HTMLElement)) return null;
  entry.setAttribute(helperNativeSettingsEntryAttribute, pageId);
  entry.removeAttribute("id");
  entry.removeAttribute("aria-current");
  entry.removeAttribute("aria-selected");
  entry.removeAttribute("data-state");
  entry.removeAttribute("data-active");
  entry.removeAttribute("data-highlighted");
  entry.setAttribute("tabindex", "0");
  setSessionMenuItemLabel(entry, label);
  setNativeSettingsEntryIcon(entry, pageId);
  return entry;
}

function nativeSettingsStandardIconSvg(iconName, className = "") {
  const classes = className ? ` class="${className}"` : "";
  const base = `<svg${classes} data-lucide="${iconName}" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">`;
  switch (iconName) {
    case "sliders-horizontal":
      return `${base}<line x1="21" x2="14" y1="4" y2="4"></line><line x1="10" x2="3" y1="4" y2="4"></line><line x1="21" x2="12" y1="12" y2="12"></line><line x1="8" x2="3" y1="12" y2="12"></line><line x1="21" x2="16" y1="20" y2="20"></line><line x1="12" x2="3" y1="20" y2="20"></line><line x1="14" x2="14" y1="2" y2="6"></line><line x1="8" x2="8" y1="10" y2="14"></line><line x1="16" x2="16" y1="18" y2="22"></line></svg>`;
    case "file-code-2":
      return `${base}<path d="M4 22h14a2 2 0 0 0 2-2V7.5L14.5 2H6a2 2 0 0 0-2 2v4"></path><path d="M14 2v6h6"></path><path d="m5 12-3 3 3 3"></path><path d="m9 18 3-3-3-3"></path></svg>`;
    case "trash-2":
      return `${base}<path d="M3 6h18"></path><path d="M19 6v14c0 1-1 2-2 2H7c-1 0-2-1-2-2V6"></path><path d="M8 6V4c0-1 1-2 2-2h4c1 0 2 1 2 2v2"></path><line x1="10" x2="10" y1="11" y2="17"></line><line x1="14" x2="14" y1="11" y2="17"></line></svg>`;
    case "scroll-text":
      return `${base}<path d="M15 12h-5"></path><path d="M15 8h-5"></path><path d="M19 17V5a2 2 0 0 0-2-2H4"></path><path d="M8 21h12a2 2 0 0 0 2-2v-1a1 1 0 0 0-1-1H11a1 1 0 0 0-1 1v1a2 2 0 1 1-4 0V5a2 2 0 1 0-4 0v2a1 1 0 0 0 1 1h3"></path></svg>`;
    case "info":
      return `${base}<circle cx="12" cy="12" r="10"></circle><path d="M12 16v-4"></path><path d="M12 8h.01"></path></svg>`;
    case "refresh-cw":
      return `${base}<path d="M3 12a9 9 0 0 1 9-9 9.75 9.75 0 0 1 6.74 2.74L21 8"></path><path d="M21 3v5h-5"></path><path d="M21 12a9 9 0 0 1-9 9 9.75 9.75 0 0 1-6.74-2.74L3 16"></path><path d="M8 16H3v5"></path></svg>`;
    case "external-link":
      return `${base}<path d="M15 3h6v6"></path><path d="M10 14 21 3"></path><path d="M18 13v6a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h6"></path></svg>`;
    default:
      return `${base}<circle cx="12" cy="12" r="10"></circle></svg>`;
  }
}

function setNativeSettingsEntryIcon(entry, pageId) {
  if (!(entry instanceof HTMLElement)) return;
  const page = nativeHelperSettingsPageDefinitions().find(
    (item) => item.id === pageId,
  );
  const iconName = page?.standardIconName || "info";
  const wrapper = document.createElement("span");
  wrapper.innerHTML = nativeSettingsStandardIconSvg(
    iconName,
    "codex-helper-native-settings-sidebar-icon",
  );
  const icon = wrapper.firstElementChild;
  if (!(icon instanceof SVGElement)) return;
  const current = entry.querySelector("svg");
  if (current instanceof SVGElement) current.replaceWith(icon);
  else entry.insertBefore(icon, entry.firstChild);
}

function cloneNativeSettingsGroupLabel(sidebar) {
  const hostText = Array.from(sidebar.querySelectorAll("*")).find(
    (node) =>
      node instanceof HTMLElement &&
      exactText(node, "Host") &&
      isVisibleElement(node),
  );
  const label = hostText?.cloneNode(true);
  if (label instanceof HTMLElement) {
    label.textContent = "Helper";
    label.removeAttribute("id");
    label.removeAttribute("aria-current");
    label.removeAttribute("aria-selected");
    label.removeAttribute("data-state");
    return label;
  }
  const fallback = document.createElement("div");
  fallback.textContent = "Helper";
  fallback.className = "codex-helper-native-settings-group-label";
  return fallback;
}

function createNativeSettingsGroup(sidebar) {
  const group = document.createElement("div");
  group.setAttribute(helperNativeSettingsGroupAttribute, "true");
  group.className = "codex-helper-native-settings-group";
  group.appendChild(cloneNativeSettingsGroupLabel(sidebar));
  for (const page of nativeHelperSettingsPages()) {
    const entry = cloneNativeSettingsEntry(sidebar, page.label, page.id);
    if (!(entry instanceof HTMLElement)) {
      logDiagnostic("settings_insertion_failed", {
        reason: "entry_template_not_found",
        page: page.id,
      });
      return null;
    }
    group.appendChild(entry);
  }
  return group;
}

function installNativeHelperSettingsGroup() {
  const sidebar = findCodexSettingsSidebar();
  if (!(sidebar instanceof HTMLElement)) return false;
  if (sidebar.querySelector(`[${helperNativeSettingsGroupAttribute}]`))
    return true;
  const group = createNativeSettingsGroup(sidebar);
  if (!(group instanceof HTMLElement)) return false;
  sidebar.appendChild(group);
  return true;
}

function updateNativeSettingsActiveEntry(pageId) {
  for (const entry of document.querySelectorAll(
    `[${helperNativeSettingsEntryAttribute}]`,
  )) {
    if (!(entry instanceof HTMLElement)) continue;
    const active =
      entry.getAttribute(helperNativeSettingsEntryAttribute) === pageId;
    entry.setAttribute("data-active", active ? "true" : "false");
    entry.setAttribute("aria-selected", active ? "true" : "false");
    if (active) entry.setAttribute("aria-current", "page");
    else entry.removeAttribute("aria-current");
  }
}

function findNativeSettingsContentRoot(sidebar) {
  if (!(sidebar instanceof HTMLElement)) return null;
  const sidebarRect = sidebar.getBoundingClientRect();
  for (
    let ancestor = sidebar.parentElement;
    ancestor instanceof HTMLElement;
    ancestor = ancestor === document.body ? null : ancestor.parentElement
  ) {
    const candidates = Array.from(ancestor.children)
      .filter((child) => child instanceof HTMLElement && !child.contains(sidebar))
      .map((child) => child);
    const root = candidates
      .filter((candidate) =>
        isValidNativeSettingsContentRoot(candidate, sidebar, sidebarRect),
      )
      .sort(
        (a, b) =>
          nativeSettingsContentRootScore(b, sidebarRect) -
          nativeSettingsContentRootScore(a, sidebarRect),
      )[0];
    if (root instanceof HTMLElement) {
      return findNativeSettingsScrollContentRoot(root, sidebar, sidebarRect) || root;
    }
  }
  return null;
}

function findNativeSettingsScrollContentRoot(root, sidebar, sidebarRect) {
  const candidates = Array.from(root.querySelectorAll("div, main, section"));
  return (
    candidates
      .filter((candidate) =>
        isValidNativeSettingsScrollContentRoot(candidate, sidebar, sidebarRect),
      )
      .sort(
        (a, b) =>
          nativeSettingsContentRootScore(b, sidebarRect) -
          nativeSettingsContentRootScore(a, sidebarRect),
      )[0] || null
  );
}

function nativeSettingsContentRootScore(root, sidebarRect) {
  const rect = root.getBoundingClientRect();
  const style = getComputedStyle(root);
  const className = String(root.className || "");
  let score = rect.width * rect.height;
  if (rect.left >= sidebarRect.right - 8) score += 1_000_000_000;
  if (className.includes("p-panel")) score += 20_000;
  if (className.includes("scrollbar-stable")) score += 10_000;
  if (style.overflowY === "auto" || style.overflowY === "scroll") score += 5_000;
  return score;
}

function isValidNativeSettingsScrollContentRoot(root, sidebar, sidebarRect) {
  if (!(root instanceof HTMLElement) || !isVisibleElement(root)) return false;
  if (root.contains(sidebar) || sidebar.contains(root)) return false;
  if (root.closest(`[${helperNativeSettingsGroupAttribute}]`)) return false;
  if (root.querySelector(`[${helperNativeSettingsEntryAttribute}]`))
    return false;
  const rect = root.getBoundingClientRect();
  if (rect.left < sidebarRect.right - 8) return false;
  const style = getComputedStyle(root);
  const className = String(root.className || "");
  return (
    className.includes("p-panel") ||
    className.includes("scrollbar-stable") ||
    style.overflowY === "auto" ||
    style.overflowY === "scroll"
  );
}

function isValidNativeSettingsContentRoot(root, sidebar, sidebarRect) {
  if (!(root instanceof HTMLElement) || !isVisibleElement(root)) return false;
  if (root.contains(sidebar) || sidebar.contains(root)) return false;
  if (root.closest(`[${helperNativeSettingsGroupAttribute}]`)) return false;
  if (root.querySelector(`[${helperNativeSettingsEntryAttribute}]`))
    return false;
  const rect = root.getBoundingClientRect();
  return rect.left >= sidebarRect.right - 8;
}

function stashNativeSettingsContent(host) {
  if (!(host instanceof HTMLElement)) return;
  if (helperNativeSettingsContentStash?.host === host) return;
  restoreNativeSettingsContent();
  const marker = document.createComment("codex-helper-native-settings-stash");
  const fragment = document.createDocumentFragment();
  host.insertBefore(marker, host.firstChild);
  for (const node of Array.from(host.childNodes)) {
    if (node === marker) continue;
    if (
      node instanceof HTMLElement &&
      node.hasAttribute(helperNativeSettingsPageAttribute)
    )
      continue;
    fragment.appendChild(node);
  }
  helperNativeSettingsContentStash = { host, marker, fragment };
}

function restoreNativeSettingsContent() {
  if (!helperNativeSettingsContentStash) return;
  const { host, marker, fragment } = helperNativeSettingsContentStash;
  if (
    host instanceof HTMLElement &&
    marker instanceof Comment &&
    marker.parentNode === host
  ) {
    host.insertBefore(fragment, marker.nextSibling);
    marker.remove();
  }
  helperNativeSettingsContentStash = null;
}

function clearNativeHelperSettingsPage() {
  for (const node of document.querySelectorAll(
    `[${helperNativeSettingsPageAttribute}]`,
  )) {
    node.remove();
  }
  restoreNativeSettingsContent();
  for (const host of document.querySelectorAll(
    `[${helperNativeSettingsContentHostAttribute}]`,
  )) {
    host.removeAttribute(helperNativeSettingsContentHostAttribute);
    host.removeAttribute("data-codex-helper-active");
  }
  helperNativeSettingsRoot = null;
  helperNativeSettingsContentHost = null;
  helperNativeSettingsActivePage = "";
  updateNativeSettingsActiveEntry("");
}

function nativeSettingsPageTitle(pageId) {
  const page = nativeHelperSettingsPageDefinitions().find(
    (item) => item.id === pageId,
  );
  return page?.label || "General";
}

function nativeSettingsPageDescription(pageId) {
  const page = nativeHelperSettingsPageDefinitions().find(
    (item) => item.id === pageId,
  );
  return page?.description || "";
}

function nativeSettingsPageHeader(pageId) {
  if (pageId === "about") {
    return nativeSettingsAboutHeader();
  }
  return `
    <div class="flex h-toolbar items-center justify-between gap-2 px-0 py-0">
      <div class="flex min-w-0 flex-1 flex-col gap-1">
        <div class="heading-base text-token-text-primary">${nativeSettingsPageTitle(pageId)}</div>
        ${nativeSettingsPageDescription(pageId)
          ? `<div class="codex-helper-native-settings-page-description text-token-text-secondary text-sm">${nativeSettingsPageDescription(pageId)}</div>`
          : ""}
      </div>
    </div>
  `;
}

function renderNativeHelperSettingsPage(host, pageId) {
  if (!(host instanceof HTMLElement)) return null;
  host.querySelectorAll(`[${helperNativeSettingsPageAttribute}]`).forEach(
    (node) => {
      node.remove();
    },
  );
  stashNativeSettingsContent(host);
  host.setAttribute(helperNativeSettingsContentHostAttribute, "true");
  host.setAttribute("data-codex-helper-active", "true");
  const page = document.createElement("section");
  page.setAttribute(helperNativeSettingsPageAttribute, pageId);
  page.className = "codex-helper-native-settings-page";
  page.innerHTML = `
    <div class="codex-helper-native-settings-page-inner flex w-full flex-col">
      ${nativeSettingsPageHeader(pageId)}
      <div class="codex-helper-native-settings-page-content flex flex-col gap-4">
        ${nativeSettingsPageContent(pageId)}
      </div>
    </div>
  `;
  host.appendChild(page);
  helperNativeSettingsRoot = page;
  helperNativeSettingsContentHost = host;
  helperNativeSettingsActivePage = pageId;
  updateNativeSettingsActiveEntry(pageId);
  return page;
}

function nativeSettingsPanel(rows, extraClass = "") {
  const classes = extraClass
    ? `${helperPanelClass} ${extraClass}`
    : helperPanelClass;
  return `<div class="${classes}" style="background-color: var(--color-background-panel, var(--color-token-bg-fog));">${rows}</div>`;
}

function nativeSettingsIconSvg(name) {
  if (name === "refresh") {
    return nativeSettingsStandardIconSvg("refresh-cw");
  }
  return nativeSettingsStandardIconSvg("external-link");
}

function nativeSettingsIconButton(command, ariaLabel, iconName) {
  return `<button type="button" class="codex-helper-native-settings-icon-button" ${helperCommandAttribute}="${command}" aria-label="${ariaLabel}">${nativeSettingsIconSvg(iconName)}</button>`;
}

function nativeSettingsCodexHelperLogoSvg() {
  return `<svg class="codex-helper-native-settings-about-logo" viewBox="0 0 640 640" xmlns="http://www.w3.org/2000/svg" aria-hidden="true"><path d="m291 0c1.3.7 3.6.8 6 1.1 36.4 4.2 69.3 20.7 95.9 46.6 75-20.3 153.7 15.9 187.8 85.4 9.8 20 15.9 41.1 16.4 63.8 1.1 17.4-.9 33.8-5.1 50.9 28 27.9 44.1 63.6 47 102.6 5.8 77.5-44.8 149-119.9 169-4 10.4-7 19.9-11.8 29.6-24.4 50.1-73 84.5-128.5 90-2.3.2-4.1.3-5.1 1h-26c-1-.6-2.9-.8-5.1-1-35-3.8-66.7-19.5-92.4-43.4-2.8-2.6-5.2-3.1-8.8-2-10.4 3.1-20.5 3.7-31.6 4.1-54.4 2-105.4-23.4-137-67.5-7.7-9.4-12.8-19.1-17.5-30.4-14.2-33.5-17.2-70.1-7.7-106.2-25.6-25.9-42.2-59.1-46.5-95.5-.3-2.1-.2-4.3-.8-5.1v-28c1.7-17.3 5.5-34 12.6-50.5 15.7-36.5 43.5-65.9 79.2-83.4 15.6-7.4 19.5-7.7 28.2-10.3 19.5-67.8 74.4-113.2 143.7-120.8z"></path><g fill="#fff"><path d="m176.8 420.3 51.4-86.8c4-6.7 4.4-15.2.5-22.1l-52.2-91.5c-6.4-11.3-20.8-13.8-30.9-7.7-10.5 6.3-14.2 19.7-8 30.7l45.1 79-43.9 74c-6.6 11.1-4.2 24.4 6.3 31.4s24.8 4.7 31.7-6.9z"></path><path d="m463.2 420.3-51.4-86.8c-4-6.7-4.4-15.2-.5-22.1l52.2-91.5c6.4-11.3 20.8-13.8 30.9-7.7s14.2 19.7 8 30.7l-45.1 79 43.9 74c6.6 11.1 4.2 24.4-6.3 31.4s-24.8 4.7-31.7-6.9z"></path><path d="m362.7 342.6c13.2 0 23.2-10.1 23.2-22.7s-10-22.6-23.2-22.6h-85.7c-13.1 0-22.8 10.3-22.9 22.6 0 12.3 9.7 22.6 22.9 22.6h85.7z"></path></g></svg>`;
}

function nativeSettingsPathHeader(pathAttr, openCommand, refreshCommand = "") {
  return `
    <div class="codex-helper-native-settings-list-header">
      <div class="codex-helper-native-settings-path-line">
        <span class="codex-helper-native-settings-path text-token-text-primary" ${pathAttr}>Loading</span>
        ${nativeSettingsIconButton(openCommand, "Open path", "open")}
      </div>
      ${refreshCommand ? nativeSettingsIconButton(refreshCommand, "Refresh", "refresh") : ""}
    </div>`;
}

function nativeSettingsListFooter(statusAttr) {
  return `<div class="codex-helper-native-settings-list-footer text-token-text-secondary text-xs" ${statusAttr}>Loading</div>`;
}

function nativeSettingsListSection(header, panel, extraClass = "") {
  const classes = extraClass
    ? `codex-helper-native-settings-list-section ${extraClass}`
    : "codex-helper-native-settings-list-section";
  return `<div class="${classes}">${header}${panel}</div>`;
}

function nativeSettingsSwitchRow(title, description, descKey, toggleKey, ariaLabel) {
  return `
    <div class="flex items-center justify-between gap-4 p-3">
      <div class="flex min-w-0 flex-col gap-1">
        <div class="min-w-0 text-sm text-token-text-primary">${title}</div>
        <div class="text-token-text-secondary min-w-0 text-sm" data-codex-helper-setting-desc="${descKey}">${description}</div>
      </div>
      <label class="codex-helper-switch inline-flex shrink-0 items-center" aria-label="${ariaLabel}">
        <input type="checkbox" ${helperToggleAttribute}="${toggleKey}">
        <span class="relative inline-flex h-5 w-8 shrink-0 items-center rounded-full bg-token-foreground/20 transition-colors duration-200 ease-out"><span class="h-4 w-4 translate-x-[2px] rounded-full border border-[color:var(--gray-0)] bg-[color:var(--gray-0)] shadow-sm transition-transform duration-200 ease-out"></span></span>
      </label>
    </div>`;
}

function nativeSettingsActionRow(title, detail, command, buttonLabel, detailAttr = "") {
  return `
    <div class="flex items-center justify-between gap-4 p-3">
      <div class="flex min-w-0 flex-col gap-1">
        <div class="min-w-0 text-sm text-token-text-primary">${title}</div>
        <div class="text-token-text-secondary min-w-0 text-sm"${detailAttr ? ` ${detailAttr}` : ""}>${detail}</div>
      </div>
      <button type="button" class="${helperActionClass}" ${helperCommandAttribute}="${command}">${buttonLabel}</button>
    </div>`;
}

function nativeSettingsAboutHeader() {
  return `
    <div class="codex-helper-native-settings-about-header codex-helper-native-settings-about-hero">
      <div class="codex-helper-native-settings-about-icon" aria-hidden="true">
        ${nativeSettingsCodexHelperLogoSvg()}
      </div>
      <div class="flex min-w-0 flex-col gap-1">
        <div class="codex-helper-native-settings-about-name text-token-text-primary">Codex Helper</div>
        <div class="text-token-text-secondary text-sm">A local runtime helper for Codex settings, session tools, scripts, logs, and developer workflows.</div>
      </div>
    </div>
  `;
}

function nativeSettingsAboutPageContent() {
  return nativeSettingsPanel(`
    <div class="codex-helper-native-settings-about-row">
      <div class="flex min-w-0 flex-col gap-1">
        <div class="min-w-0 text-sm text-token-text-primary">Last updated</div>
        <div class="text-token-text-secondary min-w-0 text-sm">${helperBuildDate}</div>
      </div>
    </div>
    <div class="codex-helper-native-settings-about-row">
      <div class="flex min-w-0 flex-col gap-1">
        <div class="min-w-0 text-sm text-token-text-primary">Project repository</div>
        <div class="text-token-text-secondary min-w-0 truncate text-sm">${helperRepoUrl}</div>
      </div>
      <a href="${helperRepoUrl}" target="_blank" rel="noopener noreferrer" class="codex-helper-native-settings-icon-button codex-helper-external-link" aria-label="Open project repository">${nativeSettingsIconSvg("open")}</a>
    </div>
  `);
}

function nativeSettingsPageContent(pageId) {
  if (pageId === "user-scripts") {
    return nativeSettingsPanel(`
      ${nativeSettingsPathHeader(
        "data-codex-helper-scripts-path",
        "open-scripts-dir",
        "refresh",
      )}
      <div class="codex-helper-settings-scroll" data-codex-helper-scripts-list></div>
      ${nativeSettingsListFooter("data-codex-helper-scripts-status")}
    `);
  }
  if (pageId === "deleted-chats") {
    return nativeSettingsListSection(
      nativeSettingsPathHeader(
        "data-codex-helper-backups-path",
        "open-backups-dir",
        "refresh",
      ),
      nativeSettingsPanel(`
        <div class="codex-helper-chat-search">
          <input class="codex-helper-chat-search-input" data-codex-helper-deleted-chat-search type="search" placeholder="Search deleted chats" autocomplete="off" spellcheck="false" aria-label="Search deleted chats">
        </div>
        <div class="codex-helper-settings-scroll" data-codex-helper-backups></div>
        ${nativeSettingsListFooter("data-codex-helper-backups-status")}
      `),
    );
  }
  if (pageId === "logs") {
    return nativeSettingsListSection(
      nativeSettingsPathHeader(
        "data-codex-helper-log-path",
        "open-log-file",
        "refresh",
      ),
      nativeSettingsPanel(`
        <pre class="codex-helper-settings-scroll text-token-text-secondary min-w-0 text-xs" data-codex-helper-log>Loading</pre>
        ${nativeSettingsListFooter("data-codex-helper-log-status")}
      `, "codex-helper-native-settings-log-panel"),
      "codex-helper-native-settings-log-section",
    );
  }
  if (pageId === "about") {
    return nativeSettingsAboutPageContent();
  }
  return `
    ${nativeSettingsPanel(`
      ${nativeSettingsActionRow("Backend", "Loading", "refresh", "Refresh", "data-codex-helper-backend")}
      ${nativeSettingsActionRow("Open in Zed", "Loading", "refresh", "Refresh", "data-codex-helper-zed-status")}
      ${nativeSettingsActionRow("DevTools", "Open Chrome DevTools for this Codex window.", "open-devtools", "Open")}
    `)}
    ${nativeSettingsPanel(`
      ${nativeSettingsSwitchRow("Delete sessions", "Show delete controls in the session list context menu.", "sessionDeleteEnabled", "sessionDeleteEnabled", "Delete sessions")}
      ${nativeSettingsSwitchRow("Markdown export", "Export conversations as Markdown from the session menu.", "markdownExportEnabled", "markdownExportEnabled", "Markdown export")}
      ${nativeSettingsSwitchRow("Move sessions", "Move sessions between projects from the sidebar context menu.", "sessionMoveEnabled", "sessionMoveEnabled", "Move sessions")}
    `)}
    ${nativeSettingsPanel(`
      ${nativeSettingsSwitchRow("Enable port forwarding", "Detect and forward ports from agent sessions.", "portForwardingEnabled", "portForwardingEnabled", "Enable port forwarding")}
      ${nativeSettingsSwitchRow("Auto-forward detected web ports", "Open forwarded web URLs when a common dev port is detected.", "portAutoForwardWeb", "portAutoForwardWeb", "Auto-forward detected web ports")}
      ${nativeSettingsSwitchRow("Use the same local port by default", "Bind forwarded ports to the same local port number when possible.", "portSameLocalPort", "portSameLocalPort", "Use the same local port by default")}
    `)}
  `;
}

function openNativeHelperSettingsPage(pageId) {
  const sidebar = findCodexSettingsSidebar();
  if (!(sidebar instanceof HTMLElement)) {
    logDiagnostic("settings_insertion_failed", { reason: "sidebar_not_found" });
    throw new Error("Native Settings sidebar not found");
  }
  const contentRoot = findNativeSettingsContentRoot(sidebar);
  if (!(contentRoot instanceof HTMLElement)) {
    logDiagnostic("settings_content_root_failed", {
      reason: "content_root_not_found",
    });
    throw new Error("Native Settings content root not found");
  }
  renderNativeHelperSettingsPage(contentRoot, pageId || "general");
  refreshHelperPage().catch((error) => {
    setHelperText(
      "[data-codex-helper-backend]",
      error?.message || String(error),
    );
    logDiagnostic("settings_refresh_failed", {
      surface: "native",
      error: error?.message || String(error),
    });
  });
  return true;
}

function activateNativeSettingsElement(element) {
  if (!(element instanceof HTMLElement)) return;
  element.focus?.();
  for (const type of [
    "pointerdown",
    "mousedown",
    "pointerup",
    "mouseup",
    "click",
  ]) {
    element.dispatchEvent(
      new MouseEvent(type, {
        bubbles: true,
        cancelable: true,
        view: window,
      }),
    );
  }
}

function nativeSettingsMenuTriggerScore(node) {
  if (!(node instanceof HTMLElement) || !isVisibleElement(node)) return -1;
  if (node.closest("[data-codex-helper-port-menu]")) return -1;
  if (node.closest(`[${helperNativeSettingsGroupAttribute}]`)) return -1;
  const text = textOf(node).toLowerCase();
  const label = [
    node.getAttribute("aria-label") || "",
    node.getAttribute("title") || "",
  ]
    .join(" ")
    .toLowerCase();
  const rect = node.getBoundingClientRect();
  let score = 0;
  if (exactText(node, "Settings")) score += 100;
  if (text.includes("settings") || label.includes("settings")) score += 60;
  if (label.includes("account") || label.includes("profile")) score += 40;
  if (label.includes("user") || label.includes("menu")) score += 30;
  if (node.hasAttribute("aria-haspopup")) score += 28;
  if (node.hasAttribute("aria-expanded")) score += 20;
  if (rect.left < 420) score += 8;
  if (rect.top > window.innerHeight * 0.5) score += 8;
  return score > 0 ? score : -1;
}

function isNativeSettingsAccountMenu(menu) {
  if (!(menu instanceof HTMLElement) || !isVisibleElement(menu)) return false;
  const text = textOf(menu);
  return (
    text.includes("Settings") &&
    (text.includes("Sign out") ||
      text.includes("Log out") ||
      text.includes("Usage") ||
      text.includes("Account") ||
      text.includes("Plan"))
  );
}

function nativeSettingsAccountMenuCandidates() {
  const menus = Array.from(
    document.querySelectorAll(
      '[role="menu"], [data-radix-menu-content], [data-radix-popper-content-wrapper]',
    ),
  ).filter((node) => isNativeSettingsAccountMenu(node));
  for (const item of document.querySelectorAll('[role="menuitem"]')) {
    if (!(item instanceof HTMLElement) || !exactText(item, "Settings"))
      continue;
    for (
      let node = item.parentElement, depth = 0;
      node instanceof HTMLElement && depth < 5;
      node = node.parentElement, depth += 1
    ) {
      if (isNativeSettingsAccountMenu(node)) menus.push(node);
    }
  }
  return Array.from(new Set(menus));
}

function nativeSettingsMenuTriggerCandidates() {
  return Array.from(document.querySelectorAll("button"))
    .filter((node) => nativeSettingsMenuTriggerScore(node) >= 0)
    .sort(
      (left, right) =>
        nativeSettingsMenuTriggerScore(right) -
        nativeSettingsMenuTriggerScore(left),
    );
}

function findNativeSettingsMenuItem() {
  for (const menu of nativeSettingsAccountMenuCandidates()) {
    const item = Array.from(menu.querySelectorAll('[role="menuitem"]')).find(
      (node) =>
        node instanceof HTMLElement &&
        isVisibleElement(node) &&
        exactText(node, "Settings"),
    );
    if (item instanceof HTMLElement) return item;
  }
  return null;
}

function closeNativeSettingsCandidateMenus() {
  const eventInit = {
    key: "Escape",
    code: "Escape",
    keyCode: 27,
    which: 27,
    bubbles: true,
    cancelable: true,
  };
  document.dispatchEvent(new KeyboardEvent("keydown", eventInit));
  window.dispatchEvent(new KeyboardEvent("keydown", eventInit));
}

function waitForNativeSettingsCondition(check, timeoutMs) {
  return new Promise((resolve) => {
    const startedAt = Date.now();
    const tick = () => {
      const value = check();
      if (value || Date.now() - startedAt >= timeoutMs) {
        resolve(value || null);
        return;
      }
      window.setTimeout(tick, 50);
    };
    tick();
  });
}

async function ensureCodexNativeSettingsOpen() {
  if (findCodexSettingsSidebar() instanceof HTMLElement) return true;
  const existingMenuItem = findNativeSettingsMenuItem();
  if (existingMenuItem instanceof HTMLElement) {
    activateNativeSettingsElement(existingMenuItem);
  } else {
    let opened = false;
    for (const trigger of nativeSettingsMenuTriggerCandidates()) {
      activateNativeSettingsElement(trigger);
      const menuItem = await waitForNativeSettingsCondition(
        () => findNativeSettingsMenuItem(),
        500,
      );
      if (menuItem instanceof HTMLElement) {
        activateNativeSettingsElement(menuItem);
        opened = true;
        break;
      }
      closeNativeSettingsCandidateMenus();
    }
    if (!opened) {
      throw new Error("Native Settings menu item not found");
    }
  }
  const sidebar = await waitForNativeSettingsCondition(
    () => findCodexSettingsSidebar(),
    1800,
  );
  if (!(sidebar instanceof HTMLElement)) {
    throw new Error("Native Settings sidebar not found");
  }
  return true;
}

async function openNativeHelperSettingsFromApp(pageId = "general") {
  await ensureCodexNativeSettingsOpen();
  if (!installNativeHelperSettingsGroup()) {
    throw new Error("Helper settings group could not be installed");
  }
  openNativeHelperSettingsPage(pageId || "general");
  return true;
}

function isNativeSettingsNavigationClick(target) {
  if (!(target instanceof Element)) return false;
  if (target.closest(`[${helperNativeSettingsGroupAttribute}]`)) return false;
  const clickable = target.closest(nativeSettingsClickableSelector());
  if (!(clickable instanceof HTMLElement)) return false;
  const sidebar = findCodexSettingsSidebar();
  return sidebar instanceof HTMLElement && sidebar.contains(clickable);
}
