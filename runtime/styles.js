// Injected stylesheet for Helper UI
// biome-ignore-all lint/correctness/noUnusedVariables: called from bootstrap.js in the bundled runtime
function installHelperStyles() {
  let style = document.getElementById("codex-helper-runtime-style");
  if (!(style instanceof HTMLStyleElement)) {
    style = document.createElement("style");
    style.id = "codex-helper-runtime-style";
    document.head.appendChild(style);
  }
  style.textContent = `
      [${helperNativeSettingsEntryAttribute}][data-active="true"] {
        background: color-mix(in srgb, currentColor 10%, transparent) !important;
      }
      [data-codex-helper-muted-selected="true"] {
        background: transparent !important;
        box-shadow: none !important;
      }
      [${helperUsageHiddenAttribute}="true"] {
        display: none !important;
      }
      [${helperNativeSettingsContentHostAttribute}][data-codex-helper-active="true"] > :not([${helperNativeSettingsPageAttribute}]) {
        display: none !important;
      }
      [${helperNativeSettingsContentHostAttribute}][data-codex-helper-active="true"] {
        min-height: 0 !important;
      }
      [${helperNativeSettingsGroupAttribute}] {
        display: flex;
        flex-direction: column;
        gap: 2px;
        margin-top: 18px;
        padding-bottom: 10px;
      }
      [${helperNativeSettingsGroupAttribute}] .codex-helper-native-settings-group-label {
        padding: 4px 10px;
        font-size: 12px;
        color: color-mix(in srgb, currentColor 55%, transparent);
      }
      [${helperNativeSettingsGroupAttribute}] .codex-helper-native-settings-sidebar-icon {
        width: 16px;
        height: 16px;
        flex: 0 0 16px;
      }
      [${helperNativeSettingsPageAttribute}] {
        box-sizing: border-box;
        display: block;
        width: 100%;
        min-height: 100%;
        color: inherit;
      }
      [${helperNativeSettingsPageAttribute}="logs"] {
        height: 100%;
        min-height: 0;
      }
      [${helperNativeSettingsPageAttribute}] .codex-helper-native-settings-page-inner {
        max-width: 48rem;
        width: 100%;
        margin: 0 auto;
        min-height: 100%;
        min-width: calc(320px * var(--codex-window-zoom, 1));
      }
      [${helperNativeSettingsPageAttribute}="logs"] .codex-helper-native-settings-page-inner {
        height: 100%;
        min-height: 0;
      }
      [${helperNativeSettingsPageAttribute}] .codex-helper-native-settings-page-content {
        padding-top: var(--padding-panel, 20px);
        gap: 2.5rem;
      }
      [${helperNativeSettingsPageAttribute}="logs"] .codex-helper-native-settings-page-content {
        flex: 1 1 auto;
        min-height: 0;
      }
      [${helperNativeSettingsPageAttribute}] .codex-helper-native-settings-page-description {
        line-height: 1.45;
      }
      [${helperNativeSettingsPageAttribute}] .codex-helper-panel {
        background-color: var(--color-background-panel, var(--color-background-primary-soft-alpha));
      }
      [${helperNativeSettingsPageAttribute}] .codex-helper-panel > :not(:last-child) {
        position: relative;
      }
      [${helperNativeSettingsPageAttribute}] .codex-helper-panel > :not(:last-child)::after {
        content: "";
        pointer-events: none;
        position: absolute;
        inset-inline: 1rem;
        bottom: 0;
        height: 0.5px;
        background: var(--color-border, var(--border, color-mix(in srgb, currentColor 10%, transparent)));
      }
      [${helperNativeSettingsPageAttribute}] .codex-helper-settings-row {
        padding-inline-start: 1rem;
        padding-inline-end: 1rem;
      }
      [${helperNativeSettingsPageAttribute}] .codex-helper-action {
        border-color: transparent;
      }
      [${helperNativeSettingsPageAttribute}] a.codex-helper-external-link {
        text-decoration: none;
      }
      [${helperNativeSettingsPageAttribute}] .codex-helper-switch {
        position: relative;
        cursor: pointer;
        margin-inline-end: 0;
      }
      [${helperNativeSettingsPageAttribute}] .codex-helper-switch input {
        position: absolute;
        width: 1px;
        height: 1px;
        opacity: 0;
        pointer-events: none;
      }
      [${helperNativeSettingsPageAttribute}] .codex-helper-switch-track {
        box-sizing: border-box;
        display: inline-flex;
        width: 32px;
        height: 20px;
        padding: 2px;
        align-items: center;
        border-radius: 999px;
        background-color: color-mix(in srgb, currentColor 10%, transparent);
        transition: background-color 200ms ease-out;
      }
      [${helperNativeSettingsPageAttribute}] .codex-helper-switch-thumb {
        box-sizing: border-box;
        width: 16px;
        height: 16px;
        border: 0;
        border-radius: 999px;
        background-color: #fff;
        box-shadow: 0 0 0 0.5px rgb(0 0 0 / 8%), 0 1px 1px rgb(0 0 0 / 12%);
        transform: translateX(0);
        transition: transform 200ms ease-out;
      }
      [${helperNativeSettingsPageAttribute}] .codex-helper-switch input:focus-visible + .codex-helper-switch-track {
        outline: 2px solid var(--color-token-focus-border, rgb(48, 145, 255));
        outline-offset: 2px;
      }
      [${helperNativeSettingsPageAttribute}] .codex-helper-switch input:checked + .codex-helper-switch-track {
        background-color: var(--color-chart-blue, rgb(48, 145, 255));
      }
      [${helperNativeSettingsPageAttribute}] .codex-helper-switch input:checked + .codex-helper-switch-track > .codex-helper-switch-thumb {
        transform: translateX(12px);
      }
      [${helperNativeSettingsPageAttribute}] .codex-helper-number-input,
      [${helperNativeSettingsPageAttribute}] .codex-helper-text-input {
        height: 1.875rem;
        border: 1px solid var(--color-border, color-mix(in srgb, currentColor 16%, transparent));
        border-radius: 8px;
        background: transparent;
        color: inherit;
        padding: 0 8px;
        font: inherit;
        font-size: 13px;
      }
      [${helperNativeSettingsPageAttribute}] .codex-helper-number-input {
        width: 64px;
      }
      [${helperNativeSettingsPageAttribute}] .codex-helper-text-input {
        width: min(220px, 42vw);
      }
      [${helperNativeSettingsPageAttribute}] .codex-helper-endpoint-models {
        display: flex;
        flex-wrap: wrap;
        gap: 6px;
        margin-top: 8px;
      }
      [${helperNativeSettingsPageAttribute}] .codex-helper-endpoint-models:empty {
        display: none;
      }
      [${helperNativeSettingsPageAttribute}] .codex-helper-endpoint-model-tag {
        box-sizing: border-box;
        margin: 0;
        padding: 3px 10px;
        border: 1px solid color-mix(in srgb, currentColor 14%, transparent);
        border-radius: 999px;
        background: color-mix(in srgb, currentColor 6%, transparent);
        color: inherit;
        font: inherit;
        font-size: 12px;
        line-height: 18px;
        white-space: nowrap;
        cursor: pointer;
      }
      [${helperNativeSettingsPageAttribute}] .codex-helper-endpoint-model-tag:hover,
      [${helperNativeSettingsPageAttribute}] .codex-helper-endpoint-model-tag:focus-visible {
        background: color-mix(in srgb, currentColor 12%, transparent);
        outline: none;
      }
      [${helperNativeSettingsPageAttribute}] .codex-helper-endpoint-generate {
        display: flex;
        flex-wrap: wrap;
        align-items: center;
        gap: 8px;
        padding: 12px;
      }
      [${helperNativeSettingsPageAttribute}] .codex-helper-endpoint-generate .codex-helper-text-input {
        flex: 1 1 160px;
        min-width: 0;
      }
      [${helperNativeSettingsPageAttribute}] .codex-helper-endpoint-secret {
        font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
        font-size: 12px;
        word-break: break-all;
      }
      [${helperNativeSettingsPageAttribute}] .codex-helper-endpoint-key-actions {
        display: flex;
        gap: 4px;
        flex: none;
      }
      [${helperNativeSettingsPageAttribute}] .codex-helper-settings-scroll {
        max-height: min(420px, 52vh);
        overflow: auto;
        min-height: 0;
      }
      [${helperNativeSettingsPageAttribute}="logs"] .codex-helper-settings-scroll {
        max-height: none;
      }
      [${helperNativeSettingsPageAttribute}] .codex-helper-native-settings-log-panel {
        flex: 1 1 auto;
        min-height: 0;
        height: clamp(360px, calc(100vh - 260px), 900px);
      }
      [${helperNativeSettingsPageAttribute}="logs"] [data-codex-helper-log-list] {
        flex: 1 1 auto;
        min-height: 0;
      }
      [${helperNativeSettingsPageAttribute}="logs"] .codex-helper-native-settings-log-toolbar {
        display: flex;
        align-items: center;
        gap: 8px;
        min-width: 0;
        padding: 0 4px;
      }
      [${helperNativeSettingsPageAttribute}="logs"] .codex-helper-native-settings-log-toolbar .codex-helper-text-input[data-codex-helper-log-date],
      [${helperNativeSettingsPageAttribute}="logs"] .codex-helper-native-settings-log-toolbar .codex-helper-text-input[data-codex-helper-log-event-filter] {
        width: auto;
        flex: 0 0 auto;
        min-width: 9.5rem;
      }
      [${helperNativeSettingsPageAttribute}="logs"] .codex-helper-native-settings-log-toolbar .codex-helper-text-input[data-codex-helper-log-search] {
        width: auto;
        flex: 1 1 auto;
        min-width: 0;
      }
      [${helperNativeSettingsPageAttribute}="logs"] .codex-helper-native-settings-log-toolbar .codex-helper-native-settings-icon-button {
        flex: 0 0 auto;
      }
      [${helperNativeSettingsPageAttribute}="logs"] .codex-helper-native-settings-log-regex {
        display: inline-flex;
        align-items: center;
        gap: 6px;
        flex: 0 0 auto;
        font-size: 12px;
        color: color-mix(in srgb, currentColor 70%, transparent);
        user-select: none;
      }
      [${helperNativeSettingsPageAttribute}="logs"] .codex-helper-native-settings-log-footer {
        display: flex;
        align-items: center;
        justify-content: space-between;
        gap: 12px;
      }
      [${helperNativeSettingsPageAttribute}="logs"] .codex-helper-native-settings-log-pager {
        display: inline-flex;
        align-items: center;
        gap: 2px;
      }
      [${helperNativeSettingsPageAttribute}="logs"] .codex-helper-native-settings-log-pager button:disabled {
        opacity: 0.35;
        cursor: default;
      }
      [${helperNativeSettingsPageAttribute}="logs"] [data-codex-helper-log-detail] {
        display: none;
        flex: 1 1 auto;
        min-height: 0;
        flex-direction: column;
      }
      [${helperNativeSettingsPageAttribute}="logs"][data-codex-helper-log-view="detail"] .codex-helper-native-settings-log-toolbar,
      [${helperNativeSettingsPageAttribute}="logs"][data-codex-helper-log-view="detail"] [data-codex-helper-log-list],
      [${helperNativeSettingsPageAttribute}="logs"][data-codex-helper-log-view="detail"] .codex-helper-native-settings-log-footer {
        display: none;
      }
      [${helperNativeSettingsPageAttribute}="logs"][data-codex-helper-log-view="detail"] [data-codex-helper-log-detail] {
        display: flex;
      }
      [${helperNativeSettingsPageAttribute}="logs"] .codex-helper-native-settings-log-detail-toolbar {
        display: flex;
        align-items: center;
        gap: 2px;
        padding: 8px 12px 4px;
        flex: 0 0 auto;
        min-width: 0;
      }
      [${helperNativeSettingsPageAttribute}="logs"] .codex-helper-native-settings-log-back {
        display: inline-flex;
        align-items: center;
        gap: 4px;
        margin: 0;
        padding: 4px 2px;
        border: 0;
        background: transparent;
        color: inherit;
        font: inherit;
        font-size: 13px;
        cursor: pointer;
      }
      [${helperNativeSettingsPageAttribute}="logs"] .codex-helper-native-settings-log-event {
        min-width: 0;
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
        font-size: 13px;
        line-height: 1;
      }
      [${helperNativeSettingsPageAttribute}="logs"] .codex-helper-native-settings-log-back svg {
        width: 16px;
        height: 16px;
      }
      [${helperNativeSettingsPageAttribute}="logs"] [data-codex-helper-log-detail-body] {
        display: flex;
        flex-direction: column;
        flex: 1 1 auto;
        min-height: 0;
        overflow: hidden;
      }
      [${helperNativeSettingsPageAttribute}="logs"] .codex-helper-native-settings-log-detail-content {
        display: flex;
        flex-direction: row;
        align-items: stretch;
        gap: 0;
        padding: 0;
        min-width: 0;
        min-height: 0;
        flex: 1 1 auto;
        height: 100%;
      }
      [${helperNativeSettingsPageAttribute}="logs"] .codex-helper-native-settings-log-detail-meta {
        display: flex;
        flex-direction: column;
        gap: 10px;
        flex: 0 0 28.571%;
        min-width: 148px;
        max-width: 40%;
        overflow: auto;
        padding: 8px 10px 16px 12px;
      }
      [${helperNativeSettingsPageAttribute}="logs"] .codex-helper-native-settings-log-split-handle {
        position: relative;
        flex: 0 0 8px;
        margin: 8px 0;
        cursor: col-resize;
        touch-action: none;
        user-select: none;
      }
      [${helperNativeSettingsPageAttribute}="logs"] .codex-helper-native-settings-log-split-handle::before {
        content: "";
        position: absolute;
        inset: 0 3px;
        border-radius: 99px;
        background: color-mix(in srgb, currentColor 14%, transparent);
      }
      [${helperNativeSettingsPageAttribute}="logs"] .codex-helper-native-settings-log-split-handle:hover::before,
      [${helperNativeSettingsPageAttribute}="logs"] .codex-helper-native-settings-log-split-handle[data-active="true"]::before {
        background: color-mix(in srgb, currentColor 32%, transparent);
      }
      [${helperNativeSettingsPageAttribute}="logs"] .codex-helper-native-settings-log-detail-field {
        display: flex;
        flex-direction: row;
        align-items: baseline;
        justify-content: space-between;
        gap: 8px;
        min-width: 0;
      }
      [${helperNativeSettingsPageAttribute}="logs"] .codex-helper-native-settings-log-detail-field .codex-helper-settings-row-title {
        flex: 0 1 auto;
      }
      [${helperNativeSettingsPageAttribute}="logs"] .codex-helper-native-settings-log-detail-field .codex-helper-settings-row-description {
        min-width: 0;
        text-align: right;
        overflow-wrap: anywhere;
        word-break: break-word;
        white-space: normal;
      }
      [${helperNativeSettingsPageAttribute}="logs"] .codex-helper-native-settings-log-session-reveal {
        display: inline-flex;
        align-items: center;
        gap: 6px;
        margin-top: 4px;
        padding: 4px 0;
        border: 0;
        background: transparent;
        color: inherit;
        font: inherit;
        font-size: 12px;
        cursor: pointer;
      }
      [${helperNativeSettingsPageAttribute}="logs"] .codex-helper-native-settings-log-session-reveal svg {
        width: 14px;
        height: 14px;
      }
      [${helperNativeSettingsPageAttribute}="logs"] .codex-helper-native-settings-log-json-section {
        position: relative;
        display: flex;
        flex-direction: column;
        flex: 1 1 auto;
        min-width: 0;
        min-height: 0;
        padding: 8px 12px 12px;
      }
      [${helperNativeSettingsPageAttribute}="logs"] .codex-helper-native-settings-log-json {
        margin: 0;
        padding: 10px 12px;
        flex: 1 1 auto;
        min-height: 0;
        overflow: auto;
        border-radius: 10px;
        background: color-mix(in srgb, currentColor 6%, transparent);
        font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
        font-size: 12px;
        line-height: 1.45;
        white-space: pre-wrap;
        word-break: break-word;
      }
      [${helperNativeSettingsPageAttribute}="logs"] .codex-helper-native-settings-log-json-copy {
        position: absolute;
        top: 16px;
        right: 24px;
        z-index: 1;
        opacity: 0;
        background: color-mix(in srgb, currentColor 10%, transparent);
      }
      [${helperNativeSettingsPageAttribute}="logs"] .codex-helper-native-settings-log-json-section:hover .codex-helper-native-settings-log-json-copy,
      [${helperNativeSettingsPageAttribute}="logs"] .codex-helper-native-settings-log-json-section:focus-within .codex-helper-native-settings-log-json-copy {
        opacity: 1;
      }
      [${helperNativeSettingsPageAttribute}] .codex-helper-settings-compact-row {
        display: flex;
        align-items: center;
        justify-content: space-between;
        gap: 12px;
        min-width: 0;
        padding: 8px 12px;
        border-top: 0.5px solid color-mix(in srgb, currentColor 10%, transparent);
      }
      [${helperNativeSettingsPageAttribute}] button.codex-helper-settings-compact-row {
        width: 100%;
        border: 0;
        border-top: 0.5px solid color-mix(in srgb, currentColor 10%, transparent);
        border-radius: 0;
        background: transparent;
        color: inherit;
        font: inherit;
        text-align: left;
        cursor: pointer;
      }
      [${helperNativeSettingsPageAttribute}] button.codex-helper-settings-compact-row:hover,
      [${helperNativeSettingsPageAttribute}] button.codex-helper-settings-compact-row:focus-visible {
        background: color-mix(in srgb, currentColor 6%, transparent);
        outline: none;
      }
      [${helperNativeSettingsPageAttribute}="logs"] button.codex-helper-settings-compact-row .codex-helper-settings-compact-text {
        display: flex;
        flex-direction: row;
        align-items: baseline;
        min-width: 0;
      }
      [${helperNativeSettingsPageAttribute}="logs"] button.codex-helper-settings-compact-row .codex-helper-settings-row-title {
        flex: 0 0 auto;
      }
      [${helperNativeSettingsPageAttribute}="logs"] button.codex-helper-settings-compact-row .codex-helper-settings-row-description {
        flex: 1 1 auto;
        min-width: 0;
      }
      [${helperNativeSettingsPageAttribute}="logs"] button.codex-helper-settings-compact-row .codex-helper-settings-row-title,
      [${helperNativeSettingsPageAttribute}="logs"] button.codex-helper-settings-compact-row .codex-helper-settings-row-description {
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
      }
      [${helperNativeSettingsPageAttribute}] .codex-helper-settings-compact-row:first-child {
        border-top: 0;
      }
      [${helperNativeSettingsPageAttribute}] .codex-helper-settings-compact-text {
        min-width: 0;
        flex: 1 1 auto;
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
        font-size: 13px;
        color: inherit;
      }
      [${helperNativeSettingsPageAttribute}] .codex-helper-settings-compact-meta {
        flex-shrink: 0;
        font-size: 12px;
        color: color-mix(in srgb, currentColor 55%, transparent);
      }
      [${helperNativeSettingsPageAttribute}] .codex-helper-settings-scroll-empty {
        padding: 12px;
        font-size: 13px;
        color: color-mix(in srgb, currentColor 55%, transparent);
      }
      [${helperNativeSettingsPageAttribute}] .codex-helper-settings-section-heading {
        display: flex;
        align-items: center;
        justify-content: space-between;
        gap: 8px;
        padding: 0 2px;
      }
      [${helperNativeSettingsPageAttribute}] .codex-helper-settings-section-title {
        padding: 0;
      }
      [${helperNativeSettingsPageAttribute}] .codex-helper-native-settings-list-section {
        display: flex;
        flex-direction: column;
        gap: 10px;
        min-width: 0;
      }
      [${helperNativeSettingsPageAttribute}="logs"] .codex-helper-native-settings-log-section {
        flex: 1 1 auto;
        min-height: 0;
      }
      [${helperNativeSettingsPageAttribute}] .codex-helper-native-settings-list-header {
        display: flex;
        align-items: flex-start;
        justify-content: space-between;
        gap: 12px;
        min-width: 0;
        padding: 0 4px;
      }
      [${helperNativeSettingsPageAttribute}] .codex-helper-native-settings-path-line {
        display: flex;
        align-items: center;
        gap: 6px;
        min-width: 0;
        flex: 1 1 auto;
      }
      [${helperNativeSettingsPageAttribute}] .codex-helper-native-settings-path {
        min-width: 0;
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
        font-family: var(--font-mono, ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace);
        font-size: 13px;
      }
      [${helperNativeSettingsPageAttribute}] .codex-helper-native-settings-icon-button {
        display: inline-flex;
        align-items: center;
        justify-content: center;
        flex: 0 0 28px;
        width: 28px;
        height: 28px;
        border: 0;
        border-radius: 8px;
        padding: 0;
        background: transparent;
        color: color-mix(in srgb, currentColor 62%, transparent);
        cursor: pointer;
      }
      [${helperNativeSettingsPageAttribute}] .codex-helper-native-settings-icon-button:hover,
      [${helperNativeSettingsPageAttribute}] .codex-helper-native-settings-icon-button:focus-visible {
        background: color-mix(in srgb, currentColor 8%, transparent);
        color: inherit;
        outline: none;
      }
      [${helperNativeSettingsPageAttribute}] .codex-helper-native-settings-icon-button svg {
        width: 14px;
        height: 14px;
      }
      [${helperNativeSettingsPageAttribute}] .codex-helper-native-settings-list-footer {
        min-height: 28px;
        padding: 7px 12px 8px;
        border-top: 0.5px solid color-mix(in srgb, currentColor 10%, transparent);
        text-align: left;
      }
      [${helperNativeSettingsPageAttribute}] .codex-helper-native-settings-about-header,
      [${helperNativeSettingsPageAttribute}] .codex-helper-native-settings-about-row {
        display: flex;
        align-items: center;
        justify-content: space-between;
        gap: 12px;
        min-width: 0;
        padding: 12px;
      }
      [${helperNativeSettingsPageAttribute}] .codex-helper-native-settings-about-hero {
        justify-content: flex-start;
        padding: 0;
      }
      [${helperNativeSettingsPageAttribute}] .codex-helper-native-settings-about-row {
        border-top: 0.5px solid color-mix(in srgb, currentColor 10%, transparent);
      }
      [${helperNativeSettingsPageAttribute}] .codex-helper-native-settings-about-row:first-child {
        border-top: 0;
      }
      [${helperNativeSettingsPageAttribute}] .codex-helper-native-settings-about-icon {
        display: inline-flex;
        align-items: center;
        justify-content: center;
        flex: 0 0 32px;
        width: 32px;
        height: 32px;
        border-radius: 9px;
        opacity: 0.75;
        transition: opacity 160ms ease-out;
      }
      [${helperNativeSettingsPageAttribute}] .codex-helper-native-settings-about-icon:hover {
        opacity: 1;
      }
      [${helperNativeSettingsPageAttribute}] .codex-helper-native-settings-about-icon svg {
        width: 32px;
        height: 32px;
        display: block;
      }
      [${helperNativeSettingsPageAttribute}] .codex-helper-native-settings-about-logo {
        border-radius: 9px;
      }
      [${helperNativeSettingsPageAttribute}] .codex-helper-native-settings-about-name {
        font-size: 15px;
        font-weight: 600;
      }
      [${helperNativeSettingsPageAttribute}] .codex-helper-native-settings-about-notes {
        display: block;
        padding: 12px;
        border-top: 0.5px solid color-mix(in srgb, currentColor 10%, transparent);
      }
      [${helperNativeSettingsPageAttribute}] .codex-helper-native-settings-about-notes[hidden] {
        display: none;
      }
      [${helperNativeSettingsPageAttribute}] .codex-helper-native-settings-about-notes-body {
        margin: 8px 0 0;
        max-height: 240px;
        overflow: auto;
        white-space: pre-wrap;
        font: inherit;
        font-size: 12px;
        line-height: 1.45;
      }
      [data-codex-helper-port-row][data-codex-helper-port-row-menu-open="true"]
        [class*="summary-panel-row-accessory"] {
        opacity: 1 !important;
        visibility: visible !important;
        pointer-events: auto !important;
      }
      [data-codex-helper-port-row] .codex-helper-port-row-actions {
        flex-shrink: 0;
        display: inline-flex;
        align-items: center;
        opacity: 0;
        pointer-events: none;
      }
      [data-codex-helper-port-row]:hover .codex-helper-port-row-actions,
      [data-codex-helper-port-row]:focus-within .codex-helper-port-row-actions,
      [data-codex-helper-port-row][data-codex-helper-port-row-menu-open="true"]
        .codex-helper-port-row-actions {
        opacity: 1;
        pointer-events: auto;
      }
      .codex-helper-port-row-leading-icon {
        width: 16px;
        height: 16px;
        flex: 0 0 16px;
      }
      .codex-helper-port-local-url {
        display: inline;
        border: 0;
        padding: 0;
        background: transparent;
        color: inherit;
        font: inherit;
        cursor: pointer;
      }
      .codex-helper-port-local-url:hover,
      .codex-helper-port-local-url:focus-visible {
        text-decoration: underline;
        outline: none;
      }
      .codex-helper-port-row-action {
        display: inline-flex;
        align-items: center;
        justify-content: center;
        width: 32px;
        height: 28px;
        border: 0;
        border-radius: 10px;
        padding: 0;
        background: transparent;
        color: color-mix(in srgb, currentColor 68%, transparent);
        cursor: pointer;
      }
      .codex-helper-port-row-action:hover,
      .codex-helper-port-row-action:focus-visible {
        background: color-mix(in srgb, currentColor 8%, transparent);
        color: inherit;
        outline: none;
      }
      .codex-helper-port-row-action svg {
        width: 13px;
        height: 13px;
      }
      [data-codex-helper-port-menu] {
        position: fixed;
        z-index: 2147483646;
        min-width: 280px;
        padding: 4px;
        border-radius: 8px;
        border: 1px solid color-mix(in srgb, currentColor 12%, transparent);
        background: Canvas;
        color: CanvasText;
        box-shadow: 0 12px 36px color-mix(in srgb, black 18%, transparent);
      }
      [data-codex-helper-port-menu] button {
        display: flex;
        width: 100%;
        align-items: center;
        justify-content: flex-start;
        gap: 8px;
        border: 0;
        border-radius: 6px;
        padding: 7px 8px;
        background: transparent;
        color: inherit;
        font: inherit;
        font-size: 13px;
        text-align: left;
        cursor: pointer;
      }
      [data-codex-helper-port-menu] button.codex-helper-port-menu-toggle,
      [data-codex-helper-port-menu] [role="menuitemcheckbox"] {
        justify-content: space-between;
        gap: 12px;
      }
      [data-codex-helper-port-menu] .codex-helper-port-menu-label {
        flex: 1 1 auto;
        min-width: 0;
        text-align: left;
      }
      [data-codex-helper-port-menu] button:hover,
      [data-codex-helper-port-menu] button:focus-visible {
        background: color-mix(in srgb, currentColor 8%, transparent);
        outline: none;
      }
      [data-codex-helper-port-menu] svg {
        width: 14px;
        height: 14px;
        flex-shrink: 0;
      }
      [data-codex-helper-port-menu] .codex-helper-port-menu-check {
        display: inline-flex;
        width: 16px;
        flex: 0 0 16px;
        align-items: center;
        justify-content: flex-end;
        margin-left: auto;
      }
      [data-codex-helper-port-menu] .codex-helper-port-menu-separator {
        height: 1px;
        margin: 4px 8px;
        background: color-mix(in srgb, currentColor 12%, transparent);
      }
      [data-codex-helper-port-dialog] {
        position: fixed;
        inset: 0;
        z-index: 2147483646;
        display: grid;
        place-items: center;
        padding: 24px;
        background: color-mix(in srgb, black 24%, transparent);
        color: CanvasText;
      }
      [data-codex-helper-port-dialog] .codex-helper-port-dialog-panel {
        width: min(460px, calc(100vw - 48px));
        display: flex;
        flex-direction: column;
        gap: 12px;
        border-radius: 12px;
        border: 1px solid color-mix(in srgb, currentColor 12%, transparent);
        background: Canvas;
        box-shadow: 0 20px 64px color-mix(in srgb, black 24%, transparent);
        padding: 16px;
      }
      [data-codex-helper-port-dialog] .codex-helper-port-dialog-title {
        font-size: 14px;
        font-weight: 600;
      }
      [data-codex-helper-port-dialog] .codex-helper-port-dialog-message {
        font-size: 13px;
        line-height: 1.4;
        color: color-mix(in srgb, currentColor 72%, transparent);
      }
      [data-codex-helper-port-dialog] .codex-helper-port-dialog-port-row {
        display: flex;
        align-items: end;
        gap: 12px;
        min-width: 0;
      }
      [data-codex-helper-port-dialog] .codex-helper-port-dialog-port-row label {
        flex: 1 1 0;
        min-width: 0;
      }
      [data-codex-helper-port-dialog] .codex-helper-port-dialog-arrow {
        display: inline-flex;
        align-items: center;
        justify-content: center;
        flex: 0 0 24px;
        height: 32px;
        color: color-mix(in srgb, currentColor 54%, transparent);
        font-size: 14px;
      }
      [data-codex-helper-port-dialog] label {
        display: flex;
        flex-direction: column;
        gap: 6px;
        font-size: 12px;
        color: color-mix(in srgb, currentColor 64%, transparent);
      }
      [data-codex-helper-port-dialog] input {
        height: 32px;
        width: 100%;
        min-width: 0;
        box-sizing: border-box;
        border: 1px solid color-mix(in srgb, currentColor 14%, transparent);
        border-radius: 8px;
        padding: 0 9px;
        background: color-mix(in srgb, Canvas 96%, currentColor 4%);
        color: CanvasText;
        font: inherit;
        font-size: 13px;
      }
      [data-codex-helper-port-dialog] input:focus-visible {
        outline: 2px solid var(--color-token-focus-border, rgb(48, 145, 255));
        outline-offset: 1px;
      }
      [data-codex-helper-port-dialog] input[readonly] {
        color: color-mix(in srgb, currentColor 60%, transparent);
      }
      [data-codex-helper-port-dialog] .codex-helper-port-dialog-error {
        min-height: 16px;
        font-size: 12px;
        color: rgb(196, 55, 55);
      }
      [data-codex-helper-port-dialog] .codex-helper-port-dialog-actions {
        display: flex;
        justify-content: flex-end;
        gap: 8px;
      }
      [data-codex-helper-port-dialog] .codex-helper-port-dialog-actions button {
        border: 0;
        border-radius: 8px;
        padding: 7px 10px;
        background: color-mix(in srgb, currentColor 8%, transparent);
        color: inherit;
        font: inherit;
        font-size: 13px;
        cursor: pointer;
      }
      [data-codex-helper-port-dialog] .codex-helper-port-dialog-actions button:hover,
      [data-codex-helper-port-dialog] .codex-helper-port-dialog-actions button:focus-visible {
        background: color-mix(in srgb, currentColor 12%, transparent);
        outline: none;
      }
      [${helperNativeSettingsPageAttribute}] .codex-helper-provider-list-header {
        display: flex;
        align-items: center;
        justify-content: space-between;
        gap: 12px;
        min-height: 28px;
      }
      [${helperNativeSettingsPageAttribute}] .codex-helper-provider-add-button {
        display: inline-flex;
        align-items: center;
        justify-content: center;
        width: 28px;
        height: 28px;
        border: 0;
        border-radius: 8px;
        background: transparent;
        color: inherit;
        cursor: pointer;
      }
      [${helperNativeSettingsPageAttribute}] .codex-helper-provider-add-button:hover,
      [${helperNativeSettingsPageAttribute}] .codex-helper-provider-add-button:focus-visible {
        background: color-mix(in srgb, currentColor 8%, transparent);
        outline: none;
      }
      [${helperNativeSettingsPageAttribute}] .codex-helper-provider-add-button svg {
        width: 16px;
        height: 16px;
      }
      [${helperNativeSettingsPageAttribute}] .codex-helper-provider-row {
        display: flex;
        align-items: center;
        gap: 12px;
        min-width: 0;
        padding: 10px 12px;
        border-top: 0.5px solid color-mix(in srgb, currentColor 10%, transparent);
      }
      [${helperNativeSettingsPageAttribute}] .codex-helper-provider-row:first-child {
        border-top: 0;
      }
      [${helperNativeSettingsPageAttribute}] .codex-helper-provider-row,
      [${helperNativeSettingsPageAttribute}] .codex-helper-provider-drag-handle {
        -webkit-user-drag: none;
        user-select: none;
      }
      [${helperNativeSettingsPageAttribute}] .codex-helper-provider-drag-handle {
        display: inline-flex;
        align-items: center;
        justify-content: center;
        width: 16px;
        height: 28px;
        margin: 0;
        padding: 0;
        border: 0;
        border-radius: 6px;
        background: transparent;
        color: color-mix(in srgb, currentColor 42%, transparent);
        flex: 0 0 16px;
        cursor: grab;
        touch-action: none;
      }
      [${helperNativeSettingsPageAttribute}] .codex-helper-provider-row[data-codex-helper-provider-id="official"] .codex-helper-provider-drag-handle {
        cursor: default;
        pointer-events: none;
      }
      [${helperNativeSettingsPageAttribute}] .codex-helper-provider-drag-handle:hover,
      [${helperNativeSettingsPageAttribute}] .codex-helper-provider-drag-handle:focus-visible {
        color: inherit;
        outline: none;
        background: color-mix(in srgb, currentColor 8%, transparent);
      }
      [${helperNativeSettingsPageAttribute}] .codex-helper-provider-drag-handle:active {
        cursor: grabbing;
      }
      [${helperNativeSettingsPageAttribute}] .codex-helper-provider-drag-handle svg {
        width: 14px;
        height: 14px;
        pointer-events: none;
      }
      [${helperNativeSettingsPageAttribute}] .codex-helper-provider-row[data-dragging="true"] {
        opacity: 0.4;
      }
      [${helperNativeSettingsPageAttribute}] .codex-helper-provider-row[data-drop-edge="before"] {
        box-shadow: 0 -2px 0 0 var(--color-token-focus-border, rgb(48, 145, 255));
      }
      [${helperNativeSettingsPageAttribute}] .codex-helper-provider-row[data-drop-edge="after"] {
        box-shadow: 0 2px 0 0 var(--color-token-focus-border, rgb(48, 145, 255));
      }
      .codex-helper-provider-reorder-layer[${helperNativeSettingsPageAttribute}] {
        position: fixed;
        inset: 0;
        pointer-events: none;
        display: block;
        width: auto;
        min-height: 0;
        z-index: 2147483646;
      }
      .codex-helper-provider-reorder-ghost {
        position: fixed;
        z-index: 2147483646;
        box-sizing: border-box;
        display: flex;
        align-items: center;
        overflow: hidden;
        margin: 0;
        pointer-events: none;
        opacity: 0.92;
        border-radius: 10px;
        background: Canvas;
        box-shadow: 0 10px 28px color-mix(in srgb, CanvasText 18%, transparent);
      }
      .codex-helper-provider-reorder-ghost .codex-helper-provider-drag-handle svg {
        width: 14px;
        height: 14px;
      }
      .codex-helper-provider-reorder-ghost .codex-helper-provider-usage-pie,
      .codex-helper-provider-reorder-ghost .codex-helper-provider-usage-pie svg {
        width: 28px;
        height: 28px;
      }
      .codex-helper-provider-reorder-ghost .codex-helper-switch-track {
        box-sizing: border-box;
        width: 32px;
        height: 20px;
        border-radius: 999px;
      }
      .codex-helper-provider-reorder-ghost .codex-helper-switch-thumb {
        box-sizing: border-box;
        width: 16px;
        height: 16px;
        border-radius: 999px;
      }
      .codex-helper-provider-reorder-ghost .codex-helper-provider-chevron svg {
        width: 16px;
        height: 16px;
      }
      html[data-codex-helper-provider-reordering],
      html[data-codex-helper-provider-reordering] * {
        cursor: grabbing !important;
        user-select: none !important;
      }
      [${helperNativeSettingsPageAttribute}] .codex-helper-provider-row-copy {
        min-width: 0;
        flex: 1 1 auto;
      }
      [${helperNativeSettingsPageAttribute}] .codex-helper-provider-row-name {
        font-size: 13px;
        font-weight: 500;
      }
      [${helperNativeSettingsPageAttribute}] .codex-helper-provider-row-meta {
        font-size: 12px;
        color: color-mix(in srgb, currentColor 55%, transparent);
      }
      [${helperNativeSettingsPageAttribute}] .codex-helper-provider-usage-pie {
        box-sizing: border-box;
        display: inline-flex;
        align-items: center;
        justify-content: center;
        width: 28px;
        height: 28px;
        margin: 0;
        padding: 0;
        border: 0;
        border-radius: 999px;
        background: transparent;
        color: inherit;
        flex: 0 0 28px;
        --usage-percent: 0;
      }
      [${helperNativeSettingsPageAttribute}] button.codex-helper-provider-usage-pie {
        cursor: pointer;
      }
      [${helperNativeSettingsPageAttribute}] button.codex-helper-provider-usage-pie:hover,
      [${helperNativeSettingsPageAttribute}] button.codex-helper-provider-usage-pie:focus-visible {
        background: color-mix(in srgb, currentColor 8%, transparent);
        outline: none;
      }
      [${helperNativeSettingsPageAttribute}] .codex-helper-provider-usage-pie svg {
        display: block;
        width: 28px;
        height: 28px;
        transform: rotate(-90deg);
      }
      [${helperNativeSettingsPageAttribute}] .codex-helper-provider-usage-pie circle {
        fill: none;
        stroke-width: 3.5;
      }
      [${helperNativeSettingsPageAttribute}] .codex-helper-provider-usage-track {
        stroke: color-mix(in srgb, currentColor 16%, transparent);
      }
      [${helperNativeSettingsPageAttribute}] .codex-helper-provider-usage-fill {
        stroke: currentColor;
        stroke-linecap: round;
        stroke-dasharray: 62.831853;
        stroke-dashoffset: 62.831853;
        opacity: 0;
      }
      [${helperNativeSettingsPageAttribute}] .codex-helper-provider-usage-pie[data-has-usage] .codex-helper-provider-usage-fill {
        opacity: 1;
        stroke-dashoffset: calc(62.831853 * (1 - var(--usage-percent) / 100));
      }
      [${helperNativeSettingsPageAttribute}] .codex-helper-provider-row > .codex-helper-switch {
        flex: 0 0 auto;
      }
      [${helperNativeSettingsPageAttribute}] .codex-helper-provider-row-openable {
        cursor: pointer;
      }
      [${helperNativeSettingsPageAttribute}] .codex-helper-provider-row-openable:hover,
      [${helperNativeSettingsPageAttribute}] .codex-helper-provider-row-openable:focus-visible {
        background: color-mix(in srgb, currentColor 6%, transparent);
        outline: none;
      }
      [${helperNativeSettingsPageAttribute}] .codex-helper-provider-chevron {
        display: inline-flex;
        align-items: center;
        justify-content: center;
        width: 20px;
        height: 20px;
        flex: 0 0 20px;
        color: color-mix(in srgb, currentColor 42%, transparent);
      }
      [${helperNativeSettingsPageAttribute}] .codex-helper-provider-chevron svg {
        width: 16px;
        height: 16px;
      }
      [data-codex-helper-provider-dialog] {
        height: 100%;
        min-height: 0;
        overflow: hidden;
        padding: 0;
        display: flex;
        flex-direction: column;
      }
      [data-codex-helper-provider-dialog] > .helper-settings-page-inner {
        flex: 1 1 auto;
        min-height: 0;
        overflow: auto;
        padding: 52px 32px 16px;
        gap: 16px;
      }
      [data-codex-helper-provider-dialog] .codex-helper-provider-dialog-panel {
        display: flex;
        flex-direction: column;
        gap: 16px;
        min-width: 0;
        min-height: 0;
        width: 100%;
        flex: 1 1 auto;
        overflow: auto;
      }
      [data-codex-helper-provider-dialog] .codex-helper-provider-dialog-message,
      [data-codex-helper-provider-dialog] .codex-helper-provider-dialog-error {
        font-size: 13px;
        line-height: 1.4;
      }
      [data-codex-helper-provider-dialog] .codex-helper-provider-dialog-error {
        min-height: 16px;
        color: rgb(196, 55, 55);
      }
      [data-codex-helper-provider-dialog] .codex-helper-provider-dialog-body {
        display: flex;
        flex-direction: column;
        gap: 10px;
      }
      [data-codex-helper-provider-dialog] .codex-helper-provider-field-row {
        display: grid;
        grid-template-columns: 132px minmax(0, 1fr);
        align-items: center;
        column-gap: 16px;
        min-height: 32px;
      }
      [data-codex-helper-provider-dialog] .codex-helper-provider-field-row[hidden] {
        display: none !important;
      }
      [data-codex-helper-provider-dialog][data-auth-mode="github_copilot"] .codex-helper-provider-api-only,
      [data-codex-helper-provider-dialog][data-auth-mode="xai_oauth"] .codex-helper-provider-api-only {
        display: none !important;
      }
      [data-codex-helper-provider-dialog] .codex-helper-provider-field-label {
        font-size: 13px;
        line-height: 32px;
        color: color-mix(in srgb, CanvasText 78%, transparent);
        white-space: nowrap;
      }
      [data-codex-helper-provider-dialog] .codex-helper-provider-field-row > input,
      [data-codex-helper-provider-dialog] .codex-helper-provider-field-row > select,
      [data-codex-helper-provider-dialog] .codex-helper-provider-secret-row,
      [data-codex-helper-provider-dialog] .codex-helper-provider-oauth {
        min-width: 0;
        width: 100%;
      }
      [data-codex-helper-provider-dialog] input,
      [data-codex-helper-provider-dialog] select {
        height: 32px;
        width: 100%;
        box-sizing: border-box;
        border: 1px solid color-mix(in srgb, currentColor 14%, transparent);
        border-radius: 8px;
        padding: 0 9px;
        background: color-mix(in srgb, Canvas 96%, currentColor 4%);
        color: CanvasText;
        font: inherit;
        font-size: 13px;
      }
      [data-codex-helper-provider-dialog] .codex-helper-provider-auth-hint {
        margin: 0;
        padding-left: 148px;
        font-size: 12px;
        line-height: 1.4;
        color: color-mix(in srgb, currentColor 58%, transparent);
      }
      [data-codex-helper-provider-dialog] .codex-helper-provider-secret-row,
      [data-codex-helper-provider-dialog] .codex-helper-provider-model-row {
        display: flex;
        gap: 8px;
        align-items: center;
      }
      [data-codex-helper-provider-dialog] .codex-helper-provider-secret-row input,
      [data-codex-helper-provider-dialog] .codex-helper-provider-model-row input {
        flex: 1 1 auto;
        width: auto;
      }
      [data-codex-helper-provider-dialog] .codex-helper-provider-secret-toggle {
        width: 32px;
        height: 32px;
        flex: 0 0 32px;
        display: inline-flex;
        align-items: center;
        justify-content: center;
        padding: 0;
        border: 1px solid color-mix(in srgb, currentColor 14%, transparent);
        border-radius: 8px;
        background: color-mix(in srgb, currentColor 6%, transparent);
        color: color-mix(in srgb, currentColor 72%, transparent);
        cursor: pointer;
      }
      [data-codex-helper-provider-dialog] .codex-helper-provider-secret-toggle:hover,
      [data-codex-helper-provider-dialog] .codex-helper-provider-secret-toggle:focus-visible {
        color: inherit;
        outline: none;
        background: color-mix(in srgb, currentColor 10%, transparent);
      }
      [data-codex-helper-provider-dialog] .codex-helper-provider-secret-toggle.is-revealed {
        color: inherit;
        background: color-mix(in srgb, currentColor 16%, transparent);
        border-color: color-mix(in srgb, currentColor 28%, transparent);
      }
      [data-codex-helper-provider-dialog] .codex-helper-provider-secret-toggle svg {
        width: 18px;
        height: 18px;
      }
      [data-codex-helper-provider-dialog] .codex-helper-provider-secret-toggle:disabled {
        opacity: 0.45;
        cursor: default;
      }
      [data-codex-helper-provider-dialog] .codex-helper-provider-oauth {
        display: flex;
        flex-direction: column;
        gap: 8px;
        align-items: stretch;
      }
      [data-codex-helper-provider-dialog] .codex-helper-provider-oauth-row {
        display: flex;
        align-items: center;
        justify-content: space-between;
        gap: 12px;
      }
      [data-codex-helper-provider-dialog] .codex-helper-provider-oauth-status {
        font-size: 13px;
        line-height: 1.4;
      }
      [data-codex-helper-provider-dialog] .codex-helper-provider-oauth-action {
        flex: 0 0 auto;
        width: auto;
      }
      [data-codex-helper-provider-dialog] .codex-helper-provider-oauth-code {
        font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
        font-size: 20px;
        font-weight: 600;
        letter-spacing: 0.08em;
      }
      [data-codex-helper-provider-dialog] .codex-helper-provider-fetch-error {
        min-height: 16px;
        font-size: 12px;
        line-height: 1.4;
        color: rgb(196, 55, 55);
      }
      [data-codex-helper-provider-dialog] .codex-helper-provider-fetched {
        display: flex;
        flex-wrap: wrap;
        gap: 6px;
      }
      [data-codex-helper-provider-dialog] .codex-helper-provider-fetched[hidden] {
        display: none;
      }
      [data-codex-helper-provider-dialog] .codex-helper-provider-fetched-model {
        border: 1px solid color-mix(in srgb, currentColor 14%, transparent);
        border-radius: 999px;
        padding: 4px 10px;
        background: color-mix(in srgb, currentColor 6%, transparent);
        color: inherit;
        font: inherit;
        font-size: 12px;
        cursor: pointer;
      }
      [data-codex-helper-provider-dialog] .codex-helper-provider-fetched-model[aria-pressed="true"] {
        background: color-mix(in srgb, currentColor 14%, transparent);
        font-weight: 600;
      }
      [data-codex-helper-provider-dialog] .codex-helper-provider-mapping-block {
        display: flex;
        flex-direction: column;
        gap: 8px;
        min-width: 0;
      }
      [data-codex-helper-provider-dialog] .codex-helper-provider-mapping-header {
        display: grid;
        grid-template-columns: 132px minmax(0, 1fr);
        column-gap: 16px;
        align-items: center;
        font-size: 13px;
        font-weight: 400;
      }
      [data-codex-helper-provider-dialog] .codex-helper-provider-mapping-header .codex-helper-provider-field-label {
        font-weight: 400;
        color: inherit;
      }
      [data-codex-helper-provider-dialog] .codex-helper-provider-mapping-actions {
        display: flex;
        align-items: center;
        justify-content: flex-end;
        gap: 8px;
      }
      [data-codex-helper-provider-dialog] .codex-helper-provider-mapping-body {
        margin-left: 148px;
        min-width: 0;
        display: flex;
        flex-direction: column;
        gap: 8px;
      }
      [data-codex-helper-provider-dialog] .codex-helper-provider-catalog {
        display: grid;
        width: 100%;
        min-width: 0;
        grid-template-columns: minmax(0, 1fr) minmax(0, 1fr) 132px minmax(328px, max-content) 32px;
        column-gap: 8px;
        row-gap: 8px;
        align-items: start;
      }
      [data-codex-helper-provider-dialog] .codex-helper-provider-catalog-columns,
      [data-codex-helper-provider-dialog] [data-codex-helper-catalog-list],
      [data-codex-helper-provider-dialog] .codex-helper-provider-catalog-row {
        display: contents;
      }
      [data-codex-helper-provider-dialog] .codex-helper-provider-catalog-columns {
        font-size: 12px;
        text-align: left;
        color: color-mix(in srgb, currentColor 58%, transparent);
      }
      [data-codex-helper-provider-dialog] .codex-helper-provider-catalog-columns > span {
        min-width: 0;
      }
      [data-codex-helper-provider-dialog] .codex-helper-provider-catalog-row > input,
      [data-codex-helper-provider-dialog] .codex-helper-provider-catalog-row > select,
      [data-codex-helper-provider-dialog] .codex-helper-provider-catalog-context,
      [data-codex-helper-provider-dialog] .codex-helper-provider-catalog-context select,
      [data-codex-helper-provider-dialog] .codex-helper-provider-catalog-context input {
        min-width: 0;
        width: 100%;
      }
      [data-codex-helper-provider-dialog] .codex-helper-provider-catalog-context {
        display: grid;
        grid-template-columns: minmax(0, 1fr) auto;
        align-items: center;
        column-gap: 4px;
        min-width: 0;
        width: 100%;
      }
      [data-codex-helper-provider-dialog] .codex-helper-provider-catalog-context select {
        grid-column: 1 / -1;
        grid-row: 1;
      }
      [data-codex-helper-provider-dialog] .codex-helper-provider-catalog-context input {
        grid-column: 1;
        grid-row: 1;
      }
      [data-codex-helper-provider-dialog] .codex-helper-provider-catalog-context-unit {
        grid-column: 2;
        grid-row: 1;
        color: color-mix(in srgb, currentColor 55%, transparent);
        font-size: 12px;
        line-height: 32px;
        padding-right: 2px;
      }
      [data-codex-helper-provider-dialog] .codex-helper-provider-catalog-context select[hidden],
      [data-codex-helper-provider-dialog] .codex-helper-provider-catalog-context input[hidden],
      [data-codex-helper-provider-dialog] .codex-helper-provider-catalog-context-unit[hidden] {
        display: none !important;
      }
      [data-codex-helper-provider-dialog] .codex-helper-provider-reasoning {
        display: flex;
        flex-wrap: nowrap;
        gap: 0;
        align-items: stretch;
        height: 32px;
        min-width: 328px;
        width: max-content;
        max-width: none;
        overflow: visible;
      }
      [data-codex-helper-provider-dialog] .codex-helper-provider-reasoning-chip {
        box-sizing: border-box;
        height: 32px;
        padding: 0 8px;
        margin-left: -1px;
        border: 1px solid color-mix(in srgb, currentColor 16%, transparent);
        border-radius: 0;
        background: transparent;
        color: color-mix(in srgb, currentColor 72%, transparent);
        font: inherit;
        font-size: 12px;
        line-height: 30px;
        white-space: nowrap;
        flex: 0 0 auto;
        cursor: pointer;
      }
      [data-codex-helper-provider-dialog] .codex-helper-provider-reasoning-chip:first-child {
        margin-left: 0;
        border-radius: 8px 0 0 8px;
      }
      [data-codex-helper-provider-dialog] .codex-helper-provider-reasoning-chip:last-child {
        border-radius: 0 8px 8px 0;
      }
      [data-codex-helper-provider-dialog] .codex-helper-provider-reasoning-chip:hover,
      [data-codex-helper-provider-dialog] .codex-helper-provider-reasoning-chip:focus-visible {
        color: inherit;
        outline: none;
        z-index: 1;
        border-color: color-mix(in srgb, currentColor 28%, transparent);
      }
      [data-codex-helper-provider-dialog] .codex-helper-provider-reasoning-chip[aria-pressed="true"] {
        color: inherit;
        font-weight: 600;
        z-index: 1;
        background: color-mix(in srgb, currentColor 14%, transparent);
        border-color: color-mix(in srgb, currentColor 28%, transparent);
      }
      [data-codex-helper-provider-dialog] .codex-helper-provider-dialog-actions {
        display: flex;
        align-items: center;
        gap: 8px;
        flex: 0 0 auto;
        padding: 10px 0;
        border-top: 1px solid color-mix(in srgb, currentColor 12%, transparent);
        background: Canvas;
      }
      [data-codex-helper-provider-dialog] > .codex-helper-provider-dialog-actions {
        padding: 10px 32px;
      }
      [data-codex-helper-provider-dialog] .codex-helper-provider-delete:disabled {
        opacity: 0.45;
        cursor: default;
      }
      [data-codex-helper-provider-dialog] .codex-helper-provider-dialog-spacer {
        flex: 1 1 auto;
      }
      [data-codex-helper-provider-dialog] .codex-helper-provider-dialog-actions button,
      [data-codex-helper-provider-dialog] .codex-helper-provider-mapping-add,
      [data-codex-helper-provider-dialog] .codex-helper-provider-mapping-fetch,
      [data-codex-helper-provider-dialog] .codex-helper-provider-mapping-remove,
      [data-codex-helper-provider-dialog] .codex-helper-provider-oauth button {
        border: 0;
        border-radius: 8px;
        padding: 7px 10px;
        background: color-mix(in srgb, currentColor 8%, transparent);
        color: inherit;
        font: inherit;
        font-size: 13px;
        font-weight: 400;
        cursor: pointer;
      }
      [data-codex-helper-provider-dialog] .codex-helper-provider-mapping-remove {
        width: 32px;
        height: 32px;
        padding: 0;
        display: inline-flex;
        align-items: center;
        justify-content: center;
      }
      [data-codex-helper-provider-dialog] .codex-helper-provider-mapping-remove svg {
        width: 14px;
        height: 14px;
      }
      [${helperToastAttribute}] {
        position: fixed;
        right: 24px;
        bottom: 24px;
        z-index: 2147483647;
        display: flex;
        align-items: center;
        gap: 8px;
        max-width: min(420px, calc(100vw - 48px));
        border-radius: 10px;
        padding: 10px 12px;
        background: color-mix(in srgb, Canvas 96%, currentColor 4%);
        color: CanvasText;
        border: 1px solid color-mix(in srgb, currentColor 12%, transparent);
        box-shadow: 0 12px 34px color-mix(in srgb, black 18%, transparent);
        font-size: 13px;
      }
      [${helperToastAttribute}] .codex-helper-toast-spinner {
        width: 14px;
        height: 14px;
        flex: 0 0 auto;
        border-radius: 999px;
        border: 2px solid color-mix(in srgb, currentColor 22%, transparent);
        border-top-color: currentColor;
        animation: codex-helper-toast-spin 0.8s linear infinite;
      }
      [${helperToastAttribute}] button {
        margin-left: 10px;
        border: 0;
        border-radius: 7px;
        padding: 5px 8px;
        background: color-mix(in srgb, currentColor 10%, transparent);
        color: inherit;
        font: inherit;
        cursor: pointer;
      }
      @keyframes codex-helper-toast-spin {
        to {
          transform: rotate(360deg);
        }
      }
      @media (prefers-reduced-motion: reduce) {
        [${helperToastAttribute}] .codex-helper-toast-spinner {
          animation: none;
        }
      }
    `;
}
