// Helper Settings pages hosted in the standalone Helper window
function nativeHelperSettingsPageDefinitions() {
  return [
    {
      id: "general",
      label: "General",
      standardIconName: "sliders-horizontal",
      description:
        "Configure Helper integrations, interface options, and port forwarding.",
    },
    {
      id: "providers",
      label: "Providers",
      standardIconName: "plug",
      description:
        "Switch ChatGPT desktop between the official login, API keys, GitHub Copilot, and xAI Grok. Helper writes ~/.codex/config.toml and keeps serving without a Helper restart.",
    },
    {
      id: "endpoint",
      label: "Endpoint",
      standardIconName: "radio",
      description:
        "Local OpenAI-compatible URL for other agents. Uses the active provider. Add named keys if you want inbound checks.",
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
      id: "logs",
      label: "Logs",
      standardIconName: "scroll-text",
      description:
        "Browse Helper diagnostics by date, search log contents, and open a record to inspect its stored detail.",
    },
    { id: "about", label: "About", standardIconName: "info" },
  ];
}

function nativeHelperSettingsPages() {
  return nativeHelperSettingsPageDefinitions().filter((page) => !page.hidden);
}

function nativeSettingsStandardIconSvg(iconName, className = "") {
  const classes = className ? ` class="${className}"` : "";
  const base = `<svg${classes} data-lucide="${iconName}" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">`;
  switch (iconName) {
    case "sliders-horizontal":
      return `${base}<line x1="21" x2="14" y1="4" y2="4"></line><line x1="10" x2="3" y1="4" y2="4"></line><line x1="21" x2="12" y1="12" y2="12"></line><line x1="8" x2="3" y1="12" y2="12"></line><line x1="21" x2="16" y1="20" y2="20"></line><line x1="12" x2="3" y1="20" y2="20"></line><line x1="14" x2="14" y1="2" y2="6"></line><line x1="8" x2="8" y1="10" y2="14"></line><line x1="16" x2="16" y1="18" y2="22"></line></svg>`;
    case "file-code-2":
      return `${base}<path d="M4 22h14a2 2 0 0 0 2-2V7.5L14.5 2H6a2 2 0 0 0-2 2v4"></path><path d="M14 2v6h6"></path><path d="m5 12-3 3 3 3"></path><path d="m9 18 3-3-3-3"></path></svg>`;
    case "scroll-text":
      return `${base}<path d="M15 12h-5"></path><path d="M15 8h-5"></path><path d="M19 17V5a2 2 0 0 0-2-2H4"></path><path d="M8 21h12a2 2 0 0 0 2-2v-1a1 1 0 0 0-1-1H11a1 1 0 0 0-1 1v1a2 2 0 1 1-4 0V5a2 2 0 1 0-4 0v2a1 1 0 0 0 1 1h3"></path></svg>`;
    case "plug":
      return `${base}<path d="M12 22v-5"></path><path d="M9 8V2"></path><path d="M15 8V2"></path><path d="M18 8v5a6 6 0 0 1-6 6 6 6 0 0 1-6-6V8Z"></path></svg>`;
    case "radio":
      return `${base}<path d="M4.9 19.1C1 15.2 1 8.8 4.9 4.9"></path><path d="M7.8 16.2c-2.3-2.3-2.3-6.1 0-8.5"></path><circle cx="12" cy="12" r="2"></circle><path d="M16.2 7.8c2.3 2.3 2.3 6.1 0 8.5"></path><path d="M19.1 4.9C23 8.8 23 15.1 19.1 19"></path></svg>`;
    case "dices":
      return `${base}<rect width="12" height="12" x="2" y="10" rx="2" ry="2"></rect><path d="m17.92 14 3.5-3.5a2.24 2.24 0 0 0 0-3l-5-4.92a2.24 2.24 0 0 0-3 0L10 6"></path><path d="M6 18h.01"></path><path d="M10 14h.01"></path><path d="M15 6h.01"></path><path d="M18 9h.01"></path></svg>`;
    case "copy":
      return `${base}<rect width="14" height="14" x="8" y="8" rx="2" ry="2"></rect><path d="M4 16c-1.1 0-2-.9-2-2V4c0-1.1.9-2 2-2h10c1.1 0 2 .9 2 2"></path></svg>`;
    case "trash-2":
      return `${base}<path d="M3 6h18"></path><path d="M19 6v14c0 1-1 2-2 2H7c-1 0-2-1-2-2V6"></path><path d="M8 6V4c0-1 1-2 2-2h4c1 0 2 1 2 2v2"></path><line x1="10" x2="10" y1="11" y2="17"></line><line x1="14" x2="14" y1="11" y2="17"></line></svg>`;
    case "plus":
      return `${base}<path d="M5 12h14"></path><path d="M12 5v14"></path></svg>`;
    case "x":
      return `${base}<path d="M18 6 6 18"></path><path d="m6 6 12 12"></path></svg>`;
    case "ellipsis":
      return `${base}<circle cx="5" cy="12" r="1"></circle><circle cx="12" cy="12" r="1"></circle><circle cx="19" cy="12" r="1"></circle></svg>`;
    case "chevron-right":
      return `${base}<path d="m9 18 6-6-6-6"></path></svg>`;
    case "chevron-left":
      return `${base}<path d="m15 18-6-6 6-6"></path></svg>`;
    case "eye":
      return `${base}<path d="M2 12s3.5-7 10-7 10 7 10 7-3.5 7-10 7S2 12 2 12Z"></path><circle cx="12" cy="12" r="3" fill="currentColor"></circle></svg>`;
    case "eye-off":
      return `${base}<path d="M2 12s3.5-7 10-7c2 0 3.8.6 5.3 1.4"></path><path d="M20.6 8.5C21.5 9.6 22 10.8 22 12c0 0-3.5 7-10 7-1.8 0-3.4-.5-4.8-1.2"></path><circle cx="12" cy="12" r="3"></circle><path d="M4 4l16 16" stroke-width="2.4"></path></svg>`;
    case "info":
      return `${base}<circle cx="12" cy="12" r="10"></circle><path d="M12 16v-4"></path><path d="M12 8h.01"></path></svg>`;
    case "refresh-cw":
      return `${base}<path d="M3 12a9 9 0 0 1 9-9 9.75 9.75 0 0 1 6.74 2.74L21 8"></path><path d="M21 3v5h-5"></path><path d="M21 12a9 9 0 0 1-9 9 9.75 9.75 0 0 1-6.74-2.74L3 16"></path><path d="M8 16H3v5"></path></svg>`;
    case "external-link":
      return `${base}<path d="M15 3h6v6"></path><path d="M10 14 21 3"></path><path d="M18 13v6a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h6"></path></svg>`;
    case "grip-vertical":
      return `${base}<circle cx="9" cy="5" r="1" fill="currentColor"></circle><circle cx="9" cy="12" r="1" fill="currentColor"></circle><circle cx="9" cy="19" r="1" fill="currentColor"></circle><circle cx="15" cy="5" r="1" fill="currentColor"></circle><circle cx="15" cy="12" r="1" fill="currentColor"></circle><circle cx="15" cy="19" r="1" fill="currentColor"></circle></svg>`;
    default:
      return `${base}<circle cx="12" cy="12" r="10"></circle></svg>`;
  }
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
  const description = nativeSettingsPageDescription(pageId);
  return `
    <header class="helper-settings-page-header">
      <h1 class="helper-settings-page-title">${nativeSettingsPageTitle(pageId)}</h1>
      ${description ? `<p class="helper-settings-page-description">${description}</p>` : ""}
    </header>
  `;
}

function renderNativeHelperSettingsPage(host, pageId) {
  if (!(host instanceof HTMLElement)) return null;
  closeProviderDialog();
  host.replaceChildren();
  const page = document.createElement("section");
  page.setAttribute(helperNativeSettingsPageAttribute, pageId);
  page.className = "codex-helper-native-settings-page helper-settings-page";
  page.innerHTML = `
    <div class="helper-settings-page-inner">
      ${nativeSettingsPageHeader(pageId)}
      <div class="helper-settings-page-body">
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
  return `<div class="${classes}">${rows}</div>`;
}

function nativeSettingsGroupTitle(title) {
  return `<div class="codex-helper-settings-section-title">${title}</div>`;
}

function nativeSettingsGroupSection(title, rows, sectionId = "") {
  const sectionAttr = sectionId
    ? ` ${helperSettingsSectionAttribute}="${sectionId}"`
    : "";
  return `
    <section class="codex-helper-settings-section"${sectionAttr}>
      ${nativeSettingsGroupTitle(title)}
      ${nativeSettingsPanel(rows)}
    </section>`;
}

function nativeSettingsIconSvg(name) {
  if (name === "refresh") {
    return nativeSettingsStandardIconSvg("refresh-cw");
  }
  if (name === "copy" || name === "trash-2" || name === "radio") {
    return nativeSettingsStandardIconSvg(name);
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
        <span class="codex-helper-native-settings-path" ${pathAttr}>Loading</span>
        ${nativeSettingsIconButton(openCommand, "Open path", "open")}
      </div>
      ${refreshCommand ? nativeSettingsIconButton(refreshCommand, "Refresh", "refresh") : ""}
    </div>`;
}

function nativeSettingsListFooter(statusAttr) {
  return `<div class="codex-helper-native-settings-list-footer" ${statusAttr}>Loading</div>`;
}

function nativeSettingsListSection(header, panel, extraClass = "") {
  const classes = extraClass
    ? `codex-helper-native-settings-list-section ${extraClass}`
    : "codex-helper-native-settings-list-section";
  return `<div class="${classes}">${header}${panel}</div>`;
}

function nativeSettingsSwitchRow(title, description, descKey, toggleKey, ariaLabel) {
  return `
    <div class="codex-helper-settings-row">
      <div class="codex-helper-settings-row-copy">
        <div class="codex-helper-settings-row-title">${title}</div>
        <div class="codex-helper-settings-row-description" data-codex-helper-setting-desc="${descKey}">${description}</div>
      </div>
      <label class="codex-helper-switch" aria-label="${ariaLabel}">
        <input type="checkbox" ${helperToggleAttribute}="${toggleKey}">
        <span class="codex-helper-switch-track" aria-hidden="true"><span class="codex-helper-switch-thumb"></span></span>
      </label>
    </div>`;
}

function nativeSettingsTextRow(title, description, field, inputType, ariaLabel, extra = "") {
  return `
    <div class="codex-helper-settings-row">
      <div class="codex-helper-settings-row-copy">
        <div class="codex-helper-settings-row-title">${title}</div>
        <div class="codex-helper-settings-row-description">${description}</div>
      </div>
      <input type="${inputType}" class="codex-helper-text-input" data-codex-helper-provider-field="${field}" aria-label="${ariaLabel}" ${extra}>
    </div>`;
}

function nativeSettingsSelectRow(title, description, field, options, ariaLabel) {
  const optionHtml = options
    .map(([value, label]) => `<option value="${value}">${label}</option>`)
    .join("");
  return `
    <div class="codex-helper-settings-row">
      <div class="codex-helper-settings-row-copy">
        <div class="codex-helper-settings-row-title">${title}</div>
        <div class="codex-helper-settings-row-description">${description}</div>
      </div>
      <select class="codex-helper-text-input" data-codex-helper-provider-field="${field}" aria-label="${ariaLabel}">${optionHtml}</select>
    </div>`;
}

function nativeSettingsProvidersPageContent() {
  return `
    <section class="codex-helper-settings-section">
      <div class="codex-helper-provider-list-header">
        <div class="codex-helper-settings-section-title">Providers</div>
        <button type="button" class="codex-helper-provider-add-button" ${helperCommandAttribute}="new-provider" aria-label="Add provider">${nativeSettingsStandardIconSvg("plus")}</button>
      </div>
      ${nativeSettingsPanel(`
        <div class="codex-helper-settings-scroll" data-codex-helper-providers-list></div>
        ${nativeSettingsListFooter("data-codex-helper-providers-status")}
      `)}
    </section>
  `;
}

function nativeSettingsActionRow(title, detail, command, buttonLabel, detailAttr = "") {
  return `
    <div class="codex-helper-settings-row">
      <div class="codex-helper-settings-row-copy">
        <div class="codex-helper-settings-row-title">${title}</div>
        <div class="codex-helper-settings-row-description"${detailAttr ? ` ${detailAttr}` : ""}>${detail}</div>
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
      <div class="helper-settings-about-copy">
        <div class="codex-helper-native-settings-about-name">Codex Helper</div>
        <div class="helper-settings-page-description">A local runtime helper for Codex settings, providers, scripts, logs, and developer workflows.</div>
      </div>
    </div>
  `;
}

function nativeSettingsAboutPageContent() {
  return nativeSettingsPanel(`
    <div class="codex-helper-native-settings-about-row">
      <div class="helper-settings-about-copy">
        <div class="codex-helper-settings-row-title">Last updated</div>
        <div class="codex-helper-settings-row-description">${helperBuildDate}</div>
      </div>
    </div>
    <div class="codex-helper-native-settings-about-row">
      <div class="helper-settings-about-copy">
        <div class="codex-helper-settings-row-title">Project repository</div>
        <div class="codex-helper-settings-row-description helper-settings-truncate">${helperRepoUrl}</div>
      </div>
      <a href="${helperRepoUrl}" target="_blank" rel="noopener noreferrer" class="codex-helper-native-settings-icon-button codex-helper-external-link" aria-label="Open project repository">${nativeSettingsIconSvg("open")}</a>
    </div>
    <div class="codex-helper-native-settings-about-row">
      <div class="helper-settings-about-copy">
        <div class="codex-helper-settings-row-title">Update</div>
        <div class="codex-helper-settings-row-description" data-codex-helper-update-detail>Checking for updates</div>
      </div>
      <button type="button" class="${helperActionClass}" ${helperCommandAttribute}="check-update" data-codex-helper-update-action disabled>Check</button>
    </div>
    <div class="codex-helper-native-settings-about-notes" data-codex-helper-update-notes-row hidden>
      <div class="helper-settings-about-copy">
        <div class="codex-helper-settings-row-title">Latest release</div>
        <pre class="codex-helper-native-settings-about-notes-body" data-codex-helper-update-notes></pre>
      </div>
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
  if (pageId === "logs") {
    return nativeSettingsListSection(
      `
      <div class="codex-helper-native-settings-log-toolbar">
        <select class="codex-helper-text-input" data-codex-helper-log-date aria-label="Log date">
          <option value="">All dates</option>
        </select>
        <select class="codex-helper-text-input" data-codex-helper-log-event-filter aria-label="Log event">
          <option value="">All events</option>
        </select>
        <input type="search" class="codex-helper-text-input" data-codex-helper-log-search placeholder="Search logs" aria-label="Search logs">
        <label class="codex-helper-native-settings-log-regex">
          <input type="checkbox" data-codex-helper-log-regex>
          <span>Regex</span>
        </label>
        ${nativeSettingsIconButton("refresh", "Refresh", "refresh")}
      </div>
      `,
      nativeSettingsPanel(`
        <div class="codex-helper-settings-scroll" data-codex-helper-log-list></div>
        <div class="codex-helper-native-settings-log-detail" data-codex-helper-log-detail>
          <div class="codex-helper-native-settings-log-detail-toolbar">
            <button type="button" class="codex-helper-native-settings-log-back" ${helperCommandAttribute}="logs-back" aria-label="Back to logs">
              ${nativeSettingsStandardIconSvg("chevron-left")}
              <span>Logs</span>
            </button>
            <span class="codex-helper-native-settings-log-event" data-codex-helper-log-event></span>
          </div>
          <div class="codex-helper-settings-scroll" data-codex-helper-log-detail-body></div>
        </div>
        <div class="codex-helper-native-settings-list-footer codex-helper-native-settings-log-footer">
          <span data-codex-helper-log-status>Loading</span>
          <div class="codex-helper-native-settings-log-pager">
            <button type="button" class="codex-helper-native-settings-icon-button" ${helperCommandAttribute}="logs-prev" data-codex-helper-log-prev aria-label="Newer logs" disabled>${nativeSettingsStandardIconSvg("chevron-left")}</button>
            <button type="button" class="codex-helper-native-settings-icon-button" ${helperCommandAttribute}="logs-next" data-codex-helper-log-next aria-label="Older logs" disabled>${nativeSettingsStandardIconSvg("chevron-right")}</button>
          </div>
        </div>
      `, "codex-helper-native-settings-log-panel"),
      "codex-helper-native-settings-log-section",
    );
  }
  if (pageId === "endpoint") {
    return nativeSettingsEndpointPageContent();
  }
  if (pageId === "about") {
    return nativeSettingsAboutPageContent();
  }
  if (pageId === "providers") {
    return nativeSettingsProvidersPageContent();
  }
  return `
    ${nativeSettingsGroupSection("Integrations", `
      ${nativeSettingsActionRow("Backend", "Loading", "refresh", "Refresh", 'data-codex-helper-backend data-status="loading"')}
      ${nativeSettingsActionRow("DevTools", "Open Chrome DevTools for the active Codex window.", "open-devtools", "Open")}
    `)}
    ${nativeSettingsGroupSection("Startup", `
      ${nativeSettingsSwitchRow("Start at login", "Open Codex Helper when you log in to this Mac.", "launchAtLoginEnabled", "launchAtLoginEnabled", "Start at login")}
    `)}
    ${nativeSettingsGroupSection("Interface", `
      ${nativeSettingsSwitchRow("Hide usage-limit overlay", "Hide the You're out of Codex and Work usage card above the composer. This does not reset or bypass account limits.", "hideUsageLimitBannerEnabled", "hideUsageLimitBannerEnabled", "Hide usage-limit overlay")}
    `)}
    ${nativeSettingsGroupSection("Diagnostics", `
      ${nativeSettingsSwitchRow("Log API provider LLM traffic", "Record request metadata and a short user-message preview for API-provider calls that go through the local Helper proxy. Official ChatGPT login traffic is not visible. Request and response bodies are not stored.", "logLlmTrafficEnabled", "logLlmTrafficEnabled", "Log API provider LLM traffic")}
    `)}
    ${nativeSettingsGroupSection("Port forwarding", `
      ${nativeSettingsSwitchRow("Enable port forwarding", "Detect and forward ports from agent sessions.", "portForwardingEnabled", "portForwardingEnabled", "Enable port forwarding")}
      ${nativeSettingsSwitchRow("Auto-forward detected web ports", "Open forwarded web URLs when a common dev port is detected.", "portAutoForwardWeb", "portAutoForwardWeb", "Auto-forward detected web ports")}
      ${nativeSettingsSwitchRow("Use the same local port by default", "Bind forwarded ports to the same local port number when possible.", "portSameLocalPort", "portSameLocalPort", "Use the same local port by default")}
    `, "port-forwarding")}
  `;
}

function helperSettingsContentHost() {
  const host = document.getElementById("helper-settings-content");
  return host instanceof HTMLElement ? host : null;
}

function openNativeHelperSettingsPage(pageId) {
  const resolved = nativeHelperSettingsPages().some((page) => page.id === pageId)
    ? pageId
    : "general";
  const host = helperSettingsContentHost();
  if (!(host instanceof HTMLElement)) {
    throw new Error("Helper Settings content host not found");
  }
  renderNativeHelperSettingsPage(host, resolved);
  refreshHelperPage().catch((error) => {
    setHelperText(
      "[data-codex-helper-backend]",
      error?.message || String(error),
    );
    logDiagnostic("settings_refresh_failed", {
      surface: "helper-window",
      error: error?.message || String(error),
    });
  });
  return true;
}
