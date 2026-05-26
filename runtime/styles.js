// Injected stylesheet for Helper UI
// biome-ignore-all lint/correctness/noUnusedVariables: called from bootstrap.js and sessions.js in the bundled runtime
function installHelperStyles() {
  let style = document.getElementById("codex-helper-runtime-style");
  if (!(style instanceof HTMLStyleElement)) {
    style = document.createElement("style");
    style.id = "codex-helper-runtime-style";
    document.head.appendChild(style);
  }
  style.textContent = `
      [${helperEntryAttribute}][data-active="true"],
      [${helperNativeSettingsEntryAttribute}][data-active="true"] {
        background: color-mix(in srgb, currentColor 10%, transparent) !important;
      }
      [data-codex-helper-muted-selected="true"] {
        background: transparent !important;
        box-shadow: none !important;
      }
      [${helperContentHostAttribute}][data-codex-helper-active="true"] > :not([${helperPageAttribute}]) {
        display: none !important;
      }
      [${helperContentHostAttribute}][data-codex-helper-active="true"] {
        min-height: 0 !important;
        overflow: auto !important;
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
        max-width: 42rem;
        margin: 0 auto;
        min-height: 100%;
      }
      [${helperNativeSettingsPageAttribute}="logs"] .codex-helper-native-settings-page-inner {
        height: 100%;
        min-height: 0;
      }
      [${helperNativeSettingsPageAttribute}] .codex-helper-native-settings-page-content {
        padding-top: var(--padding-panel, 20px);
      }
      [${helperNativeSettingsPageAttribute}="logs"] .codex-helper-native-settings-page-content {
        flex: 1 1 auto;
        min-height: 0;
      }
      [${helperNativeSettingsPageAttribute}] .codex-helper-native-settings-page-description {
        line-height: 1.45;
      }
      [${helperNativeSettingsPageAttribute}] .codex-helper-panel {
        background-color: var(--color-background-panel, var(--color-token-bg-fog));
      }
      [${helperNativeSettingsPageAttribute}] .codex-helper-action {
        border-color: transparent;
      }
      [${helperNativeSettingsPageAttribute}] a.codex-helper-external-link {
        text-decoration: none;
      }
      [${helperNativeSettingsPageAttribute}] .codex-helper-switch {
        position: relative;
      }
      [${helperNativeSettingsPageAttribute}] .codex-helper-switch input {
        position: absolute;
        width: 1px;
        height: 1px;
        opacity: 0;
        pointer-events: none;
      }
      [${helperNativeSettingsPageAttribute}] .codex-helper-switch input:focus-visible + span {
        outline: 2px solid var(--color-token-focus-border, rgb(48, 145, 255));
        outline-offset: 2px;
      }
      [${helperNativeSettingsPageAttribute}] .codex-helper-switch input:checked + span {
        background-color: var(--color-token-charts-blue, rgb(48, 145, 255));
      }
      [${helperNativeSettingsPageAttribute}] .codex-helper-switch input:checked + span > span {
        transform: translateX(14px);
      }
      [${helperNativeSettingsPageAttribute}] pre[data-codex-helper-log] {
        margin: 0;
        padding: 12px;
        white-space: pre-wrap;
        word-break: break-word;
        font-size: 12px;
        line-height: 1.45;
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
      [${helperNativeSettingsPageAttribute}="logs"] pre[data-codex-helper-log] {
        flex: 1 1 auto;
        min-height: 0;
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
      [${helperPageAttribute}] {
        display: flex;
        flex-direction: column;
        border-top: 0.5px solid var(--color-token-border, rgba(26, 28, 31, 0.12));
        padding-top: var(--padding-panel, 20px);
        color: inherit;
      }
      [${helperPageAttribute}] .codex-helper-panel {
        background-color: var(--color-background-panel, var(--color-token-bg-fog));
      }
      [${helperPageAttribute}] .codex-helper-action {
        border-color: transparent;
      }
      [${helperPageAttribute}] a.codex-helper-external-link {
        text-decoration: none;
      }
      [${helperPageAttribute}] .codex-helper-switch {
        position: relative;
      }
      [${helperPageAttribute}] .codex-helper-switch input {
        position: absolute;
        width: 1px;
        height: 1px;
        opacity: 0;
        pointer-events: none;
      }
      [${helperPageAttribute}] .codex-helper-switch input:focus-visible + span {
        outline: 2px solid var(--color-token-focus-border, rgb(48, 145, 255));
        outline-offset: 2px;
      }
      [${helperPageAttribute}] .codex-helper-switch input:checked + span {
        background-color: var(--color-token-charts-blue, rgb(48, 145, 255));
      }
      [${helperPageAttribute}] .codex-helper-switch input:checked + span > span {
        transform: translateX(14px);
      }
      [${helperPageAttribute}] pre[data-codex-helper-log] {
        margin: 0;
        padding: 12px;
        white-space: pre-wrap;
        word-break: break-word;
        font-size: 12px;
        line-height: 1.45;
      }
      [${helperPageAttribute}] .codex-helper-settings-scroll {
        max-height: 160px;
        overflow: auto;
        min-height: 0;
      }
      [${helperPageAttribute}] .codex-helper-settings-compact-row {
        display: flex;
        align-items: center;
        justify-content: space-between;
        gap: 12px;
        min-width: 0;
        padding: 8px 12px;
        border-top: 0.5px solid color-mix(in srgb, currentColor 10%, transparent);
      }
      [${helperPageAttribute}] .codex-helper-settings-compact-row:first-child {
        border-top: 0;
      }
      [${helperPageAttribute}] .codex-helper-settings-compact-text {
        min-width: 0;
        flex: 1 1 auto;
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
        font-size: 13px;
        color: inherit;
      }
      [${helperPageAttribute}] .codex-helper-settings-compact-meta {
        flex-shrink: 0;
        font-size: 12px;
        color: color-mix(in srgb, currentColor 55%, transparent);
      }
      [${helperPageAttribute}] .codex-helper-settings-scroll-empty {
        padding: 12px;
        font-size: 13px;
        color: color-mix(in srgb, currentColor 55%, transparent);
      }
      [${helperPageAttribute}] .codex-helper-settings-section-title {
        padding: 0;
      }
      [${helperPageAttribute}] .codex-helper-settings-section-heading {
        display: flex;
        align-items: center;
        justify-content: space-between;
        gap: 8px;
        padding: 0 2px;
      }
      [${helperPageAttribute}] .codex-helper-settings-section-link {
        display: inline-flex;
        align-items: center;
        justify-content: center;
        width: 28px;
        height: 28px;
        border: 0;
        border-radius: 6px;
        background: transparent;
        color: color-mix(in srgb, currentColor 70%, transparent);
        cursor: pointer;
      }
      [${helperPageAttribute}] .codex-helper-settings-section-link:hover {
        background: color-mix(in srgb, currentColor 8%, transparent);
        color: inherit;
      }
      [${helperPageAttribute}] .codex-helper-settings-section-link:focus-visible {
        outline: 2px solid var(--color-token-focus-border, rgb(48, 145, 255));
        outline-offset: 2px;
      }
      [${helperPageAttribute}] .codex-helper-settings-section-link svg {
        width: 14px;
        height: 14px;
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
      [${helperToastAttribute}] {
        position: fixed;
        right: 24px;
        bottom: 24px;
        z-index: 2147483647;
        max-width: min(420px, calc(100vw - 48px));
        border-radius: 10px;
        padding: 10px 12px;
        background: color-mix(in srgb, Canvas 96%, currentColor 4%);
        color: CanvasText;
        border: 1px solid color-mix(in srgb, currentColor 12%, transparent);
        box-shadow: 0 12px 34px color-mix(in srgb, black 18%, transparent);
        font-size: 13px;
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
      [data-codex-helper-project-fork] {
        position: fixed;
        inset: 0;
        z-index: 2147483646;
        background: color-mix(in srgb, black 18%, transparent);
      }
      [data-codex-helper-project-fork] .codex-helper-project-fork-panel {
        position: fixed;
        width: min(360px, calc(100vw - 32px));
        max-height: min(420px, calc(100vh - 32px));
        overflow: auto;
        border-radius: 12px;
        border: 1px solid color-mix(in srgb, currentColor 12%, transparent);
        background: Canvas;
        color: CanvasText;
        box-shadow: 0 16px 40px color-mix(in srgb, black 22%, transparent);
      }
      [data-codex-helper-project-fork] .codex-helper-project-fork-header {
        padding: 14px 16px 10px;
        border-bottom: 1px solid color-mix(in srgb, currentColor 10%, transparent);
      }
      [data-codex-helper-project-fork] .codex-helper-project-fork-title {
        font-size: 14px;
        font-weight: 600;
      }
      [data-codex-helper-project-fork] .codex-helper-project-fork-list {
        padding: 8px;
      }
      [data-codex-helper-project-fork] .codex-helper-project-fork-empty {
        padding: 12px;
        font-size: 13px;
        color: color-mix(in srgb, currentColor 55%, transparent);
      }
      [data-codex-helper-project-fork] .codex-helper-project-fork-item {
        display: block;
        width: 100%;
        border: 0;
        border-radius: 8px;
        padding: 10px 12px;
        background: transparent;
        color: inherit;
        text-align: left;
        cursor: pointer;
      }
      [data-codex-helper-project-fork] .codex-helper-project-fork-item:hover,
      [data-codex-helper-project-fork] .codex-helper-project-fork-item:focus-visible {
        background: color-mix(in srgb, currentColor 8%, transparent);
        outline: none;
      }
      [data-codex-helper-project-fork] .codex-helper-project-fork-item-title {
        font-size: 13px;
        font-weight: 500;
      }
      [data-codex-helper-project-fork] .codex-helper-project-fork-item-path {
        margin-top: 2px;
        font-size: 12px;
        color: color-mix(in srgb, currentColor 55%, transparent);
        word-break: break-all;
      }
    `;
}
